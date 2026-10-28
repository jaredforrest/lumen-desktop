use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_display(request: Value, root: &Path) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-display-settings"))
        .env("XDG_RUNTIME_DIR", root)
        .env("PATH", format!("{}:{}", root.display(), env::var("PATH").unwrap()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn main() {
    if Path::new(&env::args().next().unwrap())
        .file_name()
        .is_some_and(|name| name == "swaymsg")
    {
        if env::args().any(|arg| arg == "get_outputs") {
            println!("{}", json!([{
                "name":"DP-1","active":true,"make":"Fixture","model":"Monitor","serial":"1",
                "current_mode":{"width":1440,"height":1000,"refresh":60000},
                "modes":[{"width":1440,"height":1000,"refresh":60000}],
                "scale":1,"rect":{"x":0,"y":0}
            }]));
        }
        return;
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::temp_dir().join(format!("lumen-display-settings-{}-{unique}", process::id()));
    fs::create_dir(&root).unwrap();
    symlink(env::current_exe().unwrap(), root.join("swaymsg")).unwrap();
    let status = run_display(json!({"action":"status"}), &root);
    assert_eq!(status["outputs"][0]["identity"], json!(["DP-1", "Fixture", "Monitor", "1"]));
    assert_eq!(status["outputs"][0]["label"], "Fixture Monitor");
    fs::write(root.join("lumen-display-change.json"), r#"{"token":"expired","deadline":0}"#).unwrap();
    let expired = run_display(json!({"action":"confirm","token":"expired"}), &root);
    assert_eq!(expired["error"], "The confirmation period has expired");
    fs::remove_dir_all(root).unwrap();
}
