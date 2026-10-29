//! Verify status strip partition and clock formatting across real time zones.
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

unsafe extern "C" { fn kill(pid: i32, signal: i32) -> i32; }

#[test]
fn status_clock() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = env::temp_dir().join(format!("lumen-status-clock-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let result = (|| {
        let source_path = std::path::Path::new(file!()).canonicalize()?;
        let shell = source_path.parent().unwrap().parent().unwrap().parent().unwrap().join("res/lumen-desktop/shell");
        let runtime = root.join("runtime");
        fs::create_dir(&runtime)?;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
        let home = root.join("home");
        fs::create_dir(&home)?;
        for (zone, expected) in [
            ("Europe/London", r#"["00:30:00","02:30:00","01:30:00","01:30:00"]"#),
            ("America/New_York", r#"["20:30:00","21:30:00","20:30:00","21:30:00"]"#),
            ("UTC", r#"["00:30:00","01:30:00","00:30:00","01:30:00"]"#),
        ] {
            fs::write(
                root.join("shell.qml"),
                include_str!("status_clock_fixture.qml")
                    .replace("LUMEN_ASSETS", &format!("file:{}", shell.display()))
                    .replace("EXPECTED_CLOCK_VALUES", expected),
            )?;
            let mut child = Command::new("quickshell")
                .args(["-p", root.to_str().unwrap()])
                .process_group(0)
                .env("TZ", zone)
                .env("HOME", &home)
                .env("XDG_RUNTIME_DIR", &runtime)
                .env("QT_QPA_PLATFORM", "offscreen")
                .env("QT_QPA_PLATFORMTHEME", "")
                .env("QT_QUICK_BACKEND", "software")
                .env("XDG_DATA_HOME", root.join("data"))
                .env("XDG_CONFIG_HOME", root.join("config"))
                .env("XDG_CACHE_HOME", root.join("cache"))
                .env("QT_ACCESSIBILITY", "0")
                .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0")
                .env("NO_AT_BRIDGE", "1")
                .env("GTK_A11Y", "none")
                .env_remove("AT_SPI_BUS_ADDRESS")
                .env_remove("WAYLAND_DISPLAY")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            for _ in 0..1000 {
                if child.try_wait()?.is_some() { break; }
                thread::sleep(Duration::from_millis(10));
            }
            unsafe { kill(-(child.id() as i32), 9); }
            let output = child.wait_with_output()?;
            let combined = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            assert!(output.status.success() && combined.contains("PASS status clock") && !combined.contains("FAIL "), "{zone}: {combined}");
        }
        Ok::<(), std::io::Error>(())
    })();
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
