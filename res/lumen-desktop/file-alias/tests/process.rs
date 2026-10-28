use serde_json::{Value, json};
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{self, Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn invoke(root: &Path, target: &str, code: i32) -> (Output, Value) {
    let result = Command::new(env!("CARGO_BIN_EXE_lumen-file-alias"))
        .args(["--gio", root.join("gio").to_str().unwrap(), target])
        .env("FAKE_GIO_LOG", root.join("gio-argv"))
        .env("FAKE_GIO_EXIT", code.to_string())
        .output()
        .unwrap();
    let payload = serde_json::from_slice(&result.stdout).unwrap();
    (result, payload)
}

fn main() {
    if Path::new(&env::args().next().unwrap())
        .file_name()
        .is_some_and(|name| name == "gio")
    {
        let args: Vec<_> = env::args().skip(1).collect();
        fs::write(env::var("FAKE_GIO_LOG").unwrap(), serde_json::to_string(&args).unwrap()).unwrap();
        process::exit(env::var("FAKE_GIO_EXIT").unwrap().parse().unwrap());
    }
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = env::temp_dir().join(format!("lumen-file-alias-{}-{unique}", process::id()));
    fs::create_dir(&root).unwrap();
    symlink(env::current_exe().unwrap(), root.join("gio")).unwrap();
    let path = root.join("literal # ü $(not-executed).txt");
    fs::write(&path, "untouched").unwrap();
    let path_text = path.to_str().unwrap();
    let log = root.join("gio-argv");

    let (opened, payload) = invoke(&root, path_text, 0);
    assert!(opened.status.success());
    assert_eq!(payload, json!({"opened":true}));
    let args: Vec<String> = serde_json::from_slice(&fs::read(&log).unwrap()).unwrap();
    assert_eq!(args, ["open".to_owned(), format!("file://{}/literal%20%23%20%C3%BC%20%24%28not-executed%29.txt", root.display())]);
    assert_eq!(fs::read_to_string(&path).unwrap(), "untouched");

    fs::remove_file(&log).unwrap();
    let gone = root.join("gone");
    for missing in ["relative.txt", "file:///something", root.to_str().unwrap(), gone.to_str().unwrap()] {
        let (result, payload) = invoke(&root, missing, 0);
        assert!(!result.status.success(), "unexpected success: {missing}");
        assert!(payload.get("error").is_some());
    }
    assert!(!log.exists());

    let (failed, payload) = invoke(&root, path_text, 1);
    assert!(!failed.status.success());
    assert!(payload.get("error").is_some());
    assert_eq!(fs::read_to_string(&path).unwrap(), "untouched");
    fs::remove_dir_all(root).unwrap();
}
