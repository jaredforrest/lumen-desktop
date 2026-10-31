//! Actual production QML plus disposable stdio ownership and failure cases.
mod private_qml_root;
use std::{
    env, fs,
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::CommandExt,
    },
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
#[test]
fn captured_group_presentation_transport() {
    let root = private_qml_root::create().unwrap();
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let source = Path::new(file!()).canonicalize()?;
        let shell = source
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("res/lumen-desktop/shell");
        fs::write(
            root.join("shell.qml"),
            include_str!("group_presentation_fixture.qml")
                .replace("LUMEN_ASSETS", &format!("file:{}", shell.display())),
        )?;
        for name in ["runtime", "home", "bin"] {
            fs::create_dir(root.join(name))?;
            fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o700))?;
        }
        symlink(
            env::var("LUMEN_GROUP_PRESENTATION_PROVIDER")?,
            root.join("bin/lumen-group-control"),
        )?;
        let log = root.join("requests.jsonl");
        let mut child = Command::new("quickshell")
            .args(["-p", root.to_str().unwrap()])
            .process_group(0)
            .env("HOME", root.join("home"))
            .env("XDG_RUNTIME_DIR", root.join("runtime"))
            .env(
                "PATH",
                format!("{}:{}", root.join("bin").display(), env::var("PATH")?),
            )
            .env("LUMEN_GROUP_PRESENTATION_LOG", &log)
            .env("FONTCONFIG_FILE", env::var("LUMEN_TEST_FONTCONFIG")?)
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("QT_QPA_PLATFORMTHEME", "")
            .env("QT_QUICK_BACKEND", "software")
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("QT_ACCESSIBILITY", "0")
            .env("NO_AT_BRIDGE", "1")
            .env("GTK_A11Y", "none")
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}/inert-session", root.display()),
            )
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}/inert-system", root.display()),
            )
            .env(
                "PULSE_SERVER",
                format!("unix:{}/inert-audio", root.display()),
            )
            .env("PIPEWIRE_REMOTE", "inert")
            .env_remove("AT_SPI_BUS_ADDRESS")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                unsafe {
                    kill(-(child.id() as i32), 9);
                };
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output()?;
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.status.success()
                && combined.contains("GROUP_PRESENTATION_QML_PASS")
                && ![
                    "GROUP_PRESENTATION_QML_FAIL",
                    "ReferenceError:",
                    "TypeError:",
                    "Binding loop",
                    "Failed to load configuration",
                    "Cannot assign",
                    "Unable to assign"
                ]
                .iter()
                .any(|marker| combined.contains(marker)),
            "{}\n{combined}",
            output.status
        );
        let requests = fs::read_to_string(log)?;
        let lines = requests.lines().collect::<Vec<_>>();
        assert_eq!(
            lines.len(),
            6,
            "Unexpected retry/unsupported dispatch: {requests}"
        );
        for (line, epoch) in
            lines
                .iter()
                .zip(["owned", "stale", "stale", "malformed", "stall", "fresh"])
        {
            assert!(
                line.contains(&format!("\"epoch\":\"{epoch}\""))
                    && line.contains("\"windowMode\":\"one_at_a_time\""),
                "Captured request changed: {line}"
            );
        }
        Ok(())
    })();
    fs::remove_dir_all(&root).unwrap();
    result.unwrap();
}
