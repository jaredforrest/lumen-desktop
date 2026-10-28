//! Verify actual selected rows and calendar fills across resolved appearance palettes.
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

unsafe extern "C" { fn kill(pid: i32, signal: i32) -> i32; }

#[test]
fn selected_surface_contrast() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::temp_dir().join(format!("lumen-selected-contrast-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let result = (|| {
        let source_path = std::path::Path::new(file!()).canonicalize()?;
        let shell = source_path.parent().unwrap().parent().unwrap().parent().unwrap().join("res/lumen-desktop/shell");
        let palette = include_str!("../../res/lumen-desktop/appearance/palette.json");
        let font = palette.lines().find(|line| line.trim_start().starts_with("\"font\"")).unwrap()
            .split_once(':').unwrap().1.trim().trim_end_matches(',');
        let mut cases = Vec::new();
        let mut initial = String::new();
        for mode in ["light", "dark"] {
            for accent in ["blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite"] {
                for contrast in [false, true] {
                    let result = Command::new("lumen-appearance")
                        .args(["--resolve-palette", mode, accent, if contrast { "true" } else { "false" }])
                        .output()?;
                    assert!(result.status.success(), "appearance resolver: {}", String::from_utf8_lossy(&result.stderr));
                    let colors = String::from_utf8_lossy(&result.stdout);
                    let colors = colors.trim();
                    assert!(colors.starts_with('{') && colors.ends_with('}'), "invalid palette: {colors}");
                    if initial.is_empty() { initial = colors.to_owned(); }
                    cases.push(format!(r#"{{"mode":"{mode}","accent":"{accent}","colors":{colors}}}"#));
                }
            }
        }
        assert_eq!(cases.len(), 32);
        let appearance = root.join("appearance.json");
        fs::write(&appearance, format!(r#"{{"font":{font},"palette":{initial},"preferences":{{}}}}"#))?;
        fs::write(
            root.join("shell.qml"),
            include_str!("selected_surface_contrast_fixture.qml")
                .replace("LUMEN_ASSETS", &format!("file:{}", shell.display()))
                .replace("LUMEN_CASES", &format!("[{}]", cases.join(","))),
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
            .env("DBUS_SYSTEM_BUS_ADDRESS", format!("unix:path={}/no-system", root.display()))
            .env("PULSE_SERVER", format!("unix:{}/no-audio", root.display()))
            .env("PIPEWIRE_REMOTE", "no-pipewire")
            .env("LUMEN_APPEARANCE_STATE", appearance)
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
                && combined.contains("SELECTED_CONTRAST_PASS 32")
                && !["SELECTED_CONTRAST_FAIL", "Binding loop", "ReferenceError:", "TypeError:", "Failed to load configuration"]
                    .iter().any(|marker| combined.contains(marker)),
            "quickshell status: {}\n{combined}",
            output.status
        );
        Ok::<(), std::io::Error>(())
    })();
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
