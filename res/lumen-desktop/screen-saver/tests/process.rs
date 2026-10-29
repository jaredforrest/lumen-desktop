use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn settings_stub() {
    let path = env::var("SCREEN_SAVER_TEST_SETTINGS").unwrap();
    let mut args = env::args().skip(1);
    let action = args.next().unwrap();
    let mut saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    match action.as_str() {
        "read" => println!("{saved}"),
        "set" => {
            assert_eq!(args.next().as_deref(), Some("screenSaver"));
            saved["screenSaver"] = serde_json::from_str(&args.next().unwrap()).unwrap();
            fs::write(path, saved.to_string()).unwrap();
            println!("{saved}");
        }
        _ => panic!("unexpected settings action: {action}"),
    }
}

fn invoke(root: &Path, request: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-screen-saver"))
        .env("PATH", format!("{}:{}", root.display(), env::var("PATH").unwrap()))
        .env("SCREEN_SAVER_TEST_SETTINGS", root.join("settings.json"))
        .env("XDG_RUNTIME_DIR", root)
        .env("SWAYSOCK", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn main() {
    if Path::new(&env::args().next().unwrap())
        .file_name()
        .is_some_and(|name| name == "lumen-nix-settings")
    {
        settings_stub();
        return;
    }
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = env::temp_dir().join(format!("lumen-screen-saver-{}-{unique}", process::id()));
    fs::create_dir(&root).unwrap();
    symlink(env::current_exe().unwrap(), root.join("lumen-nix-settings")).unwrap();
    fs::write(root.join("settings.json"), r#"{"inputs":{}}"#).unwrap();

    for (key, value) in [("enabled", json!(true)), ("idleSeconds", json!(15)), ("motion", json!(false))] {
        assert_eq!(invoke(&root, json!({"action":"set","key":key,"value":value}))["error"], "");
    }
    let saved: Value = serde_json::from_slice(&fs::read(root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["screenSaver"], json!({"enabled":true,"idleSeconds":15,"motion":false}));
    for (key, value) in [
        ("enabled", json!(1)), ("motion", json!("false")), ("idleSeconds", json!(true)),
        ("idleSeconds", json!(14)), ("idleSeconds", json!(7201)), ("locked", json!(false)),
        ("ignoreInhibitors", json!(true)), ("lockDelay", json!(0)),
    ] {
        assert!(invoke(&root, json!({"action":"set","key":key,"value":value})).get("error").is_some());
    }
    let saved: Value = serde_json::from_slice(&fs::read(root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["inputs"], json!({}));
    let state = invoke(&root, json!({"action":"status"}));
    assert_eq!(state["lock"], json!({"known":false,"locked":true}));

    fs::write(root.join("settings.json"), r#"{"inputs":{}}"#).unwrap();
    let fresh = invoke(&root, json!({"action":"status"}));
    assert_eq!(fresh["lock"], json!({"known":false,"locked":true}));
    assert_eq!(fresh["preferences"]["enabled"], false);
    fs::remove_dir_all(root).unwrap();
}
