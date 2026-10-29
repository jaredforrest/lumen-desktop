use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn cancel_content_kills_the_owned_helper_and_keeps_the_visible_snapshot() {
    if std::env::var_os("LUMEN_CONTENT_FIXTURE_CHILD").is_some() {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let value: Value = serde_json::from_str(&input).unwrap();
        let marker = std::env::var("LUMEN_CONTENT_MARKER").unwrap();
        std::fs::write(
            &marker,
            json!({"pid":std::process::id(),"path":value["path"]}).to_string(),
        )
        .unwrap();
        std::thread::sleep(Duration::from_secs(120));
        return;
    }
    let (Ok(tags), Ok(preview)) = (
        std::env::var("LUMEN_TEST_FILE_TAGS"),
        std::env::var("LUMEN_TEST_FILE_PREVIEW"),
    ) else {
        return;
    };
    let runtime = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(runtime.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = runtime.path().join("source.txt");
    let marker = runtime.path().join("marker.json");
    std::fs::write(&source, "captured bytes").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-owned-preview-rust-wip"))
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env("LUMEN_FILE_TAGS_BIN", tags)
        .env("LUMEN_FILE_PREVIEW_BIN", preview)
        .env("LUMEN_CONTEXT_TOOLS", std::env::current_exe().unwrap())
        .env("LUMEN_CONTENT_FIXTURE_CHILD", "1")
        .env("LUMEN_CONTENT_MARKER", &marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let output = child.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            if let Ok(line) = line {
                let _ = sender.send(serde_json::from_str::<Value>(&line).unwrap());
            }
        }
    });
    let ready = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    let generation = ready["generation"].as_str().unwrap();
    writeln!(
        input,
        "{}",
        json!({"id":1,"action":"preview","generation":generation,"path":source})
    )
    .unwrap();
    let preview = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    let resource = std::path::PathBuf::from(preview["resourcePath"].as_str().unwrap());
    let capability = preview["actionCapability"].as_str().unwrap();
    writeln!(input, "{}", json!({"id":2,"action":"resource-action","generation":generation,"capability":capability,"operation":"ocr"})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(marker.exists());
    let helper: Value = serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
    assert_eq!(helper["path"], json!(resource));
    writeln!(input, "{}", json!({"id":3,"action":"cancel-action","generation":generation,"target":2,"capability":"wrong"})).unwrap();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(3)).unwrap()["accepted"],
        false
    );
    writeln!(input, "{}", json!({"id":4,"action":"cancel-action","generation":generation,"target":2,"capability":capability})).unwrap();
    let a = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    let b = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    let canceled = if a["action"] == "resource-action" {
        a
    } else {
        b
    };
    assert_eq!(canceled["reason"], "canceled", "{canceled}");
    assert_eq!(
        std::fs::read_to_string(&resource).unwrap(),
        "captured bytes"
    );
    let pid = helper["pid"].as_i64().unwrap() as i32;
    assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
    writeln!(
        input,
        "{}",
        json!({"id":5,"action":"retire-preview","generation":generation,"requests":[1]})
    )
    .unwrap();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(3)).unwrap()["retired"],
        true
    );
    assert!(!resource.exists());
    drop(input);
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
}
