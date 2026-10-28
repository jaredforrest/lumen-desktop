use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn wip_files_command_preserves_json_line_identity_and_private_preferences() {
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-files-rust-wip"))
        .env("XDG_STATE_HOME", root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let requests = [
        json!({"id":1,"action":"save-preferences","preferences":{"view":"grid","hidden":true}}),
        json!({"id":2,"action":"preferences"}),
    ];
    let mut rows: Vec<Value> = Vec::new();
    {
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        for request in requests {
            writeln!(input, "{request}").unwrap();
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            rows.push(serde_json::from_str(&line).unwrap());
        }
    }
    assert!(child.wait().unwrap().success());
    assert_eq!(rows.len(), 2);
    let saved = rows.iter().find(|row| row["id"] == 1).unwrap();
    let loaded = rows.iter().find(|row| row["id"] == 2).unwrap();
    assert_eq!(saved["action"], "save-preferences");
    assert_eq!(loaded["action"], "preferences");
    assert_eq!(loaded["preferences"]["view"], "grid");
    assert_eq!(loaded["preferences"]["hidden"], true);
    let info = std::fs::metadata(root.path().join("lumen-desktop/session/files.json")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(info.permissions().mode() & 0o077, 0);
}

#[test]
fn oversized_line_is_drained_before_the_next_request() {
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-files-rust-wip"))
        .env("XDG_STATE_HOME", root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        writeln!(input, "{}", "x".repeat(70_000)).unwrap();
        writeln!(input, "{}", json!({"id":42,"action":"preferences"})).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["error"], "Invalid request");
    assert_eq!(rows[1]["id"], 42);
}

#[test]
fn local_folder_actions_use_exact_paths_and_do_not_overwrite() {
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    let existing = root.path().join("Existing");
    std::fs::create_dir(&existing).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-files-rust-wip"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut responses = Vec::new();
    {
        use std::io::{BufRead, BufReader};
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        for request in [
            json!({"id":1,"action":"mkdir","path":folder,"name":"New"}),
            json!({"id":2,"action":"rename","path":folder.join("New"),"name":"Ready"}),
            json!({"id":3,"action":"make-link","path":folder.join("Ready")}),
            json!({"id":4,"action":"rename","path":folder.join("Ready"),"name":"Ready link"}),
        ] {
            writeln!(input, "{request}").unwrap();
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            responses.push(serde_json::from_str::<Value>(&line).unwrap());
        }
    }
    assert!(child.wait().unwrap().success());
    let rows = responses;
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0]["ok"], true);
    assert_eq!(rows[1]["selected"], json!(folder.join("Ready")));
    assert_eq!(rows[2]["selected"], json!(folder.join("Ready link")));
    assert!(rows[3]["error"].as_str().is_some());
    assert!(folder.join("Ready").is_dir());
    assert!(folder.join("Ready link").is_symlink());
    assert!(existing.is_dir());
}

#[test]
fn listing_watches_its_directory_and_reports_changes() {
    use std::io::{BufRead, BufReader};
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-files-rust-wip"))
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
    writeln!(
        input,
        "{}",
        json!({"id":1,"action":"list","path":root.path()})
    )
    .unwrap();
    let first = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(first["id"], 1);
    assert!(first["error"].is_null(), "{first}");
    std::fs::write(root.path().join("new item"), "content").unwrap();
    let changed = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(changed["event"], "changed", "{changed}");
    assert_eq!(changed["path"], json!(root.path()));
    drop(input);
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
}

#[test]
fn wip_search_scopes_results_and_rejects_stale_selections() {
    use std::io::{BufRead, BufReader};
    let root = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let folder = root.path().join("scope");
    std::fs::create_dir(&folder).unwrap();
    let file = folder.join("report.txt");
    std::fs::write(&file, "private fixture").unwrap();
    let cache = root.path().join("cache");
    let file_tags = std::env::var("LUMEN_TEST_FILE_TAGS").ok();
    let file_preview = std::env::var("LUMEN_TEST_FILE_PREVIEW").ok();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-search-rust-wip"))
        .env("LUMEN_SEARCH_ROOTS", json!([folder]).to_string())
        .env("XDG_RUNTIME_DIR", root.path())
        .env("LUMEN_FILE_TAGS_BIN", file_tags.as_deref().unwrap_or("cat"))
        .env(
            "LUMEN_FILE_PREVIEW_BIN",
            file_preview.as_deref().unwrap_or("lumen-file-preview"),
        )
        .env("XDG_CACHE_HOME", cache)
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
    let mut indexed = false;
    let mut preview_generation = String::new();
    for _ in 0..20 {
        let message = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        if message["event"] == "ready" {
            preview_generation = message["previewGeneration"].as_str().unwrap().to_owned();
        }
        if message["event"] == "index" && message["count"] == 1 && message["scanning"] == false {
            indexed = true;
            break;
        }
    }
    assert!(indexed);
    writeln!(
        input,
        "{}",
        json!({"id":1,"action":"search","query":"report"})
    )
    .unwrap();
    let response = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(response["id"], 1, "{response}");
    let token = response["results"][0]["token"].as_str().unwrap();
    assert!(token.len() > 64);
    writeln!(
        input,
        "{}",
        json!({"id":2,"action":"browse","path":file,"token":token,"parent":true})
    )
    .unwrap();
    let browse = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(browse["browse"], json!(folder));
    writeln!(
        input,
        "{}",
        json!({"id":3,"action":"browse","path":file,"token":"stale","parent":true})
    )
    .unwrap();
    let stale = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(stale["error"].as_str().is_some());
    if file_tags.is_some() && file_preview.is_some() {
        assert_eq!(preview_generation.len(), 48);
        writeln!(input, "{}", json!({"id":4,"action":"preview","path":file,"token":token,"previewGeneration":preview_generation})).unwrap();
        let rendered = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(rendered["id"], 4, "{rendered}");
        assert_eq!(rendered["text"], "private fixture", "{rendered}");
        let snapshot = std::path::PathBuf::from(rendered["resourcePath"].as_str().unwrap());
        assert!(snapshot.exists());
        writeln!(input,"{}",json!({"id":5,"action":"retire-preview","requests":[4],"previewGeneration":preview_generation})).unwrap();
        let retired = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(retired["retired"], true, "{retired}");
        assert!(!snapshot.exists());
    }
    drop(input);
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
}
