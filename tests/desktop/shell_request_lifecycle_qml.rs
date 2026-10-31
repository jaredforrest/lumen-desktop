//! Exercise stale request and selection boundaries in the actual QML components.
mod private_qml_root;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration};

unsafe extern "C" { fn kill(pid: i32, signal: i32) -> i32; }

#[test]
fn shell_request_lifecycle() {
    let root = private_qml_root::create().unwrap();
    let result = (|| {
        let source_path = std::path::Path::new(file!()).canonicalize()?;
        let shell = source_path.parent().unwrap().parent().unwrap().parent().unwrap().join("res/lumen-desktop/shell");
        let bin = root.join("bin");
        fs::create_dir(&bin)?;
        let search = bin.join("search-peer");
        let compiled = Command::new("rustc")
            .args(["--edition=2024","-Dwarnings", source_path.with_file_name("shell_request_search_stub.rs").to_str().unwrap(), "-o", search.to_str().unwrap()])
            .status()?;
        assert!(compiled.success(), "search peer compilation failed");
        fs::write(
            root.join("shell.qml"),
            include_str!("shell_request_lifecycle_fixture.qml")
                .replace("LUMEN_ASSETS", &format!("file:{}", shell.display()))
                .replace("LUMEN_TRUE", &env::var("LUMEN_TEST_TRUE").expect("Nix test true path"))
                .replace("LUMEN_SEARCH_PEER", &search.display().to_string()),
        )?;
        let runtime = root.join("runtime");
        fs::create_dir(&runtime)?;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
        let home = root.join("home");
        fs::create_dir(&home)?;
        let mut child = Command::new("dbus-run-session")
            .args(["--", "quickshell", "-p", root.to_str().unwrap()])
            .process_group(0)
            .env("HOME", home)
            .env("XDG_RUNTIME_DIR", runtime)
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("PATH", format!("{}:{}", bin.display(), env::var("PATH").unwrap_or_default()))
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
            .env_remove("AT_SPI_BUS_ADDRESS")
            .env_remove("WAYLAND_DISPLAY")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        for _ in 0..1200 {
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
                && combined.contains("PASS shell request lifecycle")
                && !combined.contains("FAIL ")
                && !combined.contains("Error:"),
            "quickshell status: {}\n{combined}",
            output.status
        );
        Ok::<(), std::io::Error>(())
    })();
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
