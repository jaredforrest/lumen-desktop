use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::MetadataExt;
use std::process::{Command, Stdio};

#[test]
fn protocol_keeps_token_in_one_process_and_refuses_replay() {
    let root = std::env::temp_dir().join(format!(
        "lumen-rename-process-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    for name in ["one", "two"] {
        std::fs::write(root.join(name), name).unwrap();
    }
    let items = ["one", "two"].iter().map(|name| {
        let path = root.join(name);
        let info = path.symlink_metadata().unwrap();
        json!({"path":path,"identity":{"device":info.dev().to_string(),"inode":info.ino().to_string()}})
    }).collect::<Vec<_>>();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-batch-rename"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut send = |value: Value| -> Value {
        writeln!(child.stdin.as_mut().unwrap(), "{value}").unwrap();
        child.stdin.as_mut().unwrap().flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let preview = send(json!({"id":1,"action":"rename-preview","items":items,
        "rule":{"mode":"prefix","first":"new-","second":""},"policy":"stop"}));
    assert_eq!(preview["executable"], true);
    assert_eq!(preview["id"], 1);
    assert!(root.join("one").exists());
    let token = preview["token"].clone();
    let confirmed = send(json!({"id":2,"action":"rename-confirm","token":token,"confirmed":true}));
    assert_eq!(confirmed["ok"], true);
    assert_eq!(confirmed["rows"][0]["status"], "renamed");
    assert!(root.join("new-one").exists());
    let replay = send(json!({"id":3,"action":"rename-confirm","token":token,"confirmed":true}));
    assert!(replay["error"].as_str().unwrap().contains("already used"));
    drop(send);
    child.stdin.take();
    assert!(child.wait().unwrap().success());
    std::fs::remove_dir_all(root).unwrap();
}
