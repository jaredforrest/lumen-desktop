use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn packaged_probe_renders_without_ambient_qt_search_paths() {
    let root = Workspace(std::env::temp_dir().join(format!("typography-env-{}", std::process::id())));
    fs::create_dir(&root.0).unwrap();
    fs::set_permissions(&root.0, fs::Permissions::from_mode(0o700)).unwrap();
    let input = root.0.join("input.json");
    let picture = root.0.join("frame.png");
    fs::write(&input, json!({
        "scale":1,"family":"Interlude","background":"#202124",
        "foreground":"#ffffff","muted":"#cccccc",
        "cases":[{"name":"digits","text":"1234567890","size":22,"tabular":true}]
    }).to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_typography-probe"))
        .args([&input, &picture])
        .env_clear()
        .env("XDG_RUNTIME_DIR", &root.0)
        .env("QML_IMPORT_PATH", "/nonexistent/ambient-qml")
        .env("QML2_IMPORT_PATH", "/nonexistent/ambient-qml2")
        .env("QT_PLUGIN_PATH", "/nonexistent/ambient-plugins")
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["cases"][0]["text"], "1234567890");
    assert!(receipt["cases"][0]["width"].as_f64().unwrap() > 0.0);
    assert_eq!(receipt["cases"][0]["digitWidths"].as_array().unwrap().len(), 10);
    let png = fs::read(&picture).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 1400);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 136);
}
