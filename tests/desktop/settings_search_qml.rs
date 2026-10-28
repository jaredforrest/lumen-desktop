//! Exercise typed Settings destinations and the authored QML target names.
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

unsafe extern "C" { fn kill(pid: i32, signal: i32) -> i32; }

#[test]
fn settings_search() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::temp_dir().join(format!("lumen-settings-search-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let result = (|| {
        let source_path = std::path::Path::new(file!()).canonicalize()?;
        let shell = source_path.parent().unwrap().parent().unwrap().parent().unwrap().join("res/lumen-desktop/shell");
        let source = include_str!("../../res/lumen-desktop/shell/SettingsSearch.js");
        let qml = fs::read_dir(&shell)?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "qml"))
            .map(|entry| fs::read_to_string(entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .join("\n");
        for line in source.lines().filter(|line| line.contains("\"target\":")) {
            let target = line.split('"').nth(3).unwrap();
            if let Some(mode) = target.strip_prefix("settings-appearance-") {
                assert!(["light", "dark", "auto"].contains(&mode));
                assert!(qml.contains("objectName:\"settings-appearance-\"+modelData"));
            } else {
                assert!(qml.contains(&format!("objectName:\"{target}\"")), "Missing QML target: {target}");
            }
        }
        fs::write(
            root.join("SettingsSearch.js"),
            source,
        )?;
        fs::write(
            root.join("shell.qml"),
            include_str!("settings_search_fixture.qml"),
        )?;
        let runtime = root.join("runtime");
        fs::create_dir(&runtime)?;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
        let home = root.join("home");
        fs::create_dir(&home)?;
        let mut child = Command::new("quickshell")
            .args(["-p", root.to_str().unwrap()])
            .process_group(0)
            .env("HOME", home)
            .env("XDG_RUNTIME_DIR", runtime)
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
            if child.try_wait()?.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        unsafe { kill(-(child.id() as i32), 9); }
        let output = child.wait_with_output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");
        assert!(
            output.status.success()
                && combined.contains("PASS typed settings destinations")
                && !combined.contains("FAIL "),
            "quickshell status: {}\n{combined}",
            output.status
        );
        Ok::<(), std::io::Error>(())
    })();
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
