//! Exercise the visible and accessible connectivity states in real QML.
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

unsafe extern "C" { fn kill(pid: i32, signal: i32) -> i32; }

#[test]
fn force_quit() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::temp_dir().join(format!("lumen-force-quit-qml-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let result = (|| {
        let source_path = std::path::Path::new(file!()).canonicalize()?;
        let shell = source_path.parent().unwrap().parent().unwrap().parent().unwrap().join("res/lumen-desktop/shell");
        let fixture = root.join("fixture");
        fs::create_dir(&fixture)?;
        for name in ["PanelSurface.qml", "PanelLabel.qml", "ControlRow.qml"] {
            fs::copy(shell.join(name), fixture.join(name))?;
        }
        let dialog = fs::read_to_string(shell.join("ForceQuitDialog.qml"))?;
        let dialog = dialog
            .replacen("PanelWindow {", "Rectangle {", 1)
            .replacen("    anchors {top:true;bottom:true;left:true;right:true}", "    property var screen:null\n    width:800;height:600", 1);
        let dialog = dialog.lines()
            .filter(|line| !["exclusionMode:", "WlrLayershell."].iter().any(|prefix| line.trim_start().starts_with(prefix)))
            .collect::<Vec<_>>().join("\n");
        fs::write(fixture.join("ForceQuitDialog.qml"), dialog)?;
        fs::write(fixture.join("qmldir"), "singleton Theme 1.0 Theme.qml\nForceQuitDialog 1.0 ForceQuitDialog.qml\nPanelSurface 1.0 PanelSurface.qml\nPanelLabel 1.0 PanelLabel.qml\nControlRow 1.0 ControlRow.qml\n")?;
        fs::write(fixture.join("Theme.qml"), "pragma Singleton\nimport QtQuick\nQtObject {property string font:\"Sans\";property color errorText:\"#b42318\";property string outline:\"#999999\";property string popoverMaterial:\"#eeeeee\";property bool dark:false;property var colors:({background:\"#eeeeee\",hover:\"#dddddd\",field:\"#ffffff\",accent:\"#0088ff\",text:\"#000000\",muted:\"#666666\"})}")?;
        let helper = root.join("lumen-application-control");
        let compiled = Command::new("rustc")
            .args(["--edition=2024", source_path.with_file_name("force_quit_stub.rs").to_str().unwrap(), "-o", helper.to_str().unwrap()])
            .status()?;
        assert!(compiled.success(), "Force Quit peer compilation failed");
        fs::write(
            fixture.join("shell.qml"),
            include_str!("force_quit_fixture.qml"),
        )?;
        let runtime = root.join("runtime");
        fs::create_dir(&runtime)?;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
        let home = root.join("home");
        fs::create_dir(&home)?;
        let mut child = Command::new("quickshell")
            .args(["-p", fixture.to_str().unwrap()])
            .process_group(0)
            .env("PATH", format!("{}:{}", root.display(), env::var("PATH").unwrap_or_default()))
            .env("LUMEN_FORCE_QUIT_CALLS", root.join("calls.log"))
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
                && combined.contains("PASS Force Quit selection identity and explicit confirmation")
                && !combined.contains("FAIL "),
            "quickshell status: {}\n{combined}",
            output.status
        );
        let calls = fs::read_to_string(root.join("calls.log"))?;
        let cancels: Vec<_> = calls.lines().filter_map(|line| line.strip_prefix("cancel\t")).collect();
        let confirms: Vec<_> = calls.lines().filter_map(|line| line.strip_prefix("confirm\t")).collect();
        assert_eq!(cancels, ["00000000000000000000000000000002"], "{calls}");
        assert_eq!(confirms, ["00000000000000000000000000000004"], "{calls}");
        Ok::<(), std::io::Error>(())
    })();
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
