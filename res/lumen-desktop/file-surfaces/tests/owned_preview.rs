use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn owned_preview_keeps_an_immutable_source_until_explicit_retirement() {
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
    std::fs::write(&source, "original private text").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-owned-preview-rust-wip"))
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env("LUMEN_FILE_TAGS_BIN", tags)
        .env("LUMEN_FILE_PREVIEW_BIN", preview)
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
    assert_eq!(generation.len(), 48);
    writeln!(
        input,
        "{}",
        json!({"id":1,"action":"preview","generation":generation,"path":source})
    )
    .unwrap();
    let result = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(result["text"], "original private text", "{result}");
    let snapshot = std::path::PathBuf::from(result["resourcePath"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(&snapshot).unwrap(),
        "original private text"
    );
    assert_eq!(
        std::fs::metadata(&snapshot).unwrap().permissions().mode() & 0o777,
        0o400
    );
    let capability = result["actionCapability"].as_str().unwrap();
    let replacement = runtime.path().join("replacement.txt");
    std::fs::write(&replacement, "replacement text").unwrap();
    std::fs::rename(&replacement, &source).unwrap();
    writeln!(input, "{}", json!({"id":2,"action":"resource-action","generation":generation,"capability":capability,"operation":"applications"})).unwrap();
    let stale = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(stale["reason"], "source_changed", "{stale}");
    assert_eq!(
        std::fs::read_to_string(&snapshot).unwrap(),
        "original private text"
    );
    writeln!(
        input,
        "{}",
        json!({"id":3,"action":"retire-preview","generation":generation,"requests":[1]})
    )
    .unwrap();
    let retired = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(retired["retired"], true, "{retired}");
    assert!(!snapshot.exists());
    writeln!(
        input,
        "{}",
        json!({"id":4,"action":"preview","generation":generation,"path":source})
    )
    .unwrap();
    let again = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    let remaining = std::path::PathBuf::from(again["resourcePath"].as_str().unwrap());
    assert!(remaining.exists());
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if child.try_wait().unwrap().is_none() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("SIGTERM did not release the provider");
    }
    assert!(!remaining.exists());
    drop(input);
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
}
