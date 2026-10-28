//! Native fixture resource ownership and activation-free private D-Bus checks.
use std::process::Child;

pub fn close_reaped_pipes(children: &mut [Child]) -> Result<(), &'static str> {
    if children
        .iter_mut()
        .any(|child| child.try_wait().ok().flatten().is_none())
    {
        return Err("Fixture child must be reaped before its pipes are closed");
    }
    for child in children {
        child.stdin.take();
        child.stdout.take();
        child.stderr.take();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn owned_child_pipes_remain_open_until_reaped_and_close_twice() {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        assert!(close_reaped_pipes(std::slice::from_mut(&mut child)).is_err());
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"observed\n")
            .unwrap();
        child.stdin.take();
        child.wait().unwrap();
        let mut output = String::new();
        std::io::Read::read_to_string(child.stdout.as_mut().unwrap(), &mut output).unwrap();
        assert_eq!(output, "observed\n");
        close_reaped_pipes(std::slice::from_mut(&mut child)).unwrap();
        assert!(child.stdout.is_none() && child.stderr.is_none());
        close_reaped_pipes(std::slice::from_mut(&mut child)).unwrap();
    }

    #[test]
    fn private_bus_has_no_host_activation_services() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("lumen-private-bus-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let result = test_bus(&root);
        std::fs::remove_dir_all(&root).unwrap();
        result.unwrap();
    }

    fn test_bus(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let config = root.join("bus.conf");
        std::fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
                root.display()
            ),
        )?;
        let mut bus = Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--print-address=1")
            .arg(format!("--config-file={}", config.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut address = String::new();
        BufReader::new(bus.stdout.as_mut().unwrap()).read_line(&mut address)?;
        assert!(address.starts_with("unix:"));
        let reply = Command::new("dbus-send")
            .args([
                "--session",
                "--print-reply",
                "--dest=org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus.ListActivatableNames",
            ])
            .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}/no-system-bus", root.display()),
            )
            .output()?;
        bus.kill()?;
        bus.wait()?;
        assert!(
            reply.status.success(),
            "{}",
            String::from_utf8_lossy(&reply.stderr)
        );
        let text = String::from_utf8(reply.stdout)?;
        assert_eq!(text.matches("string \"").count(), 1, "{text}");
        assert!(text.contains("string \"org.freedesktop.DBus\""));
        assert!(!std::fs::read_to_string(config)?.contains("servicedir"));
        Ok(())
    }
}
