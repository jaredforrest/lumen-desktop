//! Private opt-in history, generation fencing, and store integrity.
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    policy: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("lumen-search-history-{}-{unique}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let policy = root.join("settings.json");
        fs::write(&policy, json!({"search":{"queryHistory":true}}).to_string()).unwrap();
        Self { root, policy }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lumen-search-history"));
        command.env("XDG_STATE_HOME", &self.root)
            .env("LUMEN_NIX_SETTINGS_ACTIVE", &self.policy);
        command
    }

    fn cli(&self, request: Value, expected: i32) -> Value {
        let mut child = self.command().stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(expected), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn call(&self, mut request: Value) -> Value {
        if request["action"] == "record" && request.get("generation").is_none() {
            request["generation"] = self.cli(json!({"action":"list"}), 0)["generation"].clone();
        }
        self.cli(request, 0)
    }

    fn history(&self) -> PathBuf { self.root.join("lumen-desktop/search/queries.json") }
    fn enabled(&self, enabled: bool) {
        fs::write(&self.policy, json!({"search":{"queryHistory":enabled}}).to_string()).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); }
}

#[test]
fn opt_in_disable_and_retired_generations() {
    let fixture = Fixture::new();
    fixture.call(json!({"action":"record","query":"private fixture"}));
    assert!(fixture.history().exists());
    fixture.enabled(false);
    assert_eq!(fixture.call(json!({"action":"list"}))["queries"], json!([]));
    fixture.call(json!({"action":"record","query":"must never be stored"}));
    assert!(!fixture.history().exists());

    let fixture = Fixture::new();
    let generation = fixture.call(json!({"action":"list"}))["generation"].clone();
    fixture.call(json!({"action":"clear"}));
    assert_eq!(fixture.call(json!({"action":"record","query":"retired","generation":generation}))["retired"], true);
    let generation = fixture.call(json!({"action":"list"}))["generation"].clone();
    fixture.enabled(false);
    fixture.call(json!({"action":"list"}));
    fixture.enabled(true);
    assert_eq!(fixture.call(json!({"action":"record","query":"revoked","generation":generation}))["retired"], true);
    assert_eq!(fixture.call(json!({"action":"list"}))["queries"], json!([]));
    assert_eq!(fixture.cli(json!({"action":"record","query":"no authority"}), 0)["retired"], true);
}

#[test]
fn queued_worker_cannot_restore_cleared_generation() {
    let fixture = Fixture::new();
    let epoch = fixture.call(json!({"action":"list"}))["generation"].clone();
    let mut child = fixture.command().stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGSTOP) }, 0);
    writeln!(child.stdin.as_mut().unwrap(), "{}", json!({"action":"record","query":"queued fixture","generation":epoch})).unwrap();
    let cleared = fixture.call(json!({"action":"clear"}));
    assert_ne!(cleared["generation"], epoch);
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGCONT) }, 0);
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(answer["retired"], true);
    assert_eq!(fixture.call(json!({"action":"list"}))["queries"], json!([]));
    assert_eq!(fixture.call(json!({"action":"record","query":"new intent","generation":cleared["generation"]}))["queries"], json!(["new intent"]));
}

#[test]
fn bounded_private_mru_invalid_input_and_store_integrity() {
    let fixture = Fixture::new();
    for index in 0..25 { fixture.call(json!({"action":"record","query":format!("query {index}")})); }
    let result = fixture.call(json!({"action":"record","query":"query 17"}));
    let queries = result["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 20);
    assert_eq!(queries[0], "query 17");
    assert_eq!(queries.iter().filter(|query| *query == "query 17").count(), 1);
    assert!(!queries.contains(&json!("query 0")));
    let path = fixture.history();
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(path.parent().unwrap().metadata().unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(fixture.call(json!({"action":"clear"}))["queries"], json!([]));
    assert!(!path.exists());

    fixture.call(json!({"action":"record","query":"keep"}));
    for query in [json!(false), json!("x".repeat(513)), json!("line\nbreak"), json!("nul\0value")] {
        let generation = fixture.call(json!({"action":"list"}))["generation"].clone();
        assert!(fixture.cli(json!({"action":"record","query":query,"generation":generation}), 1).get("error").is_some());
    }
    assert_eq!(fixture.call(json!({"action":"list"}))["queries"], json!(["keep"]));

    let external = fixture.root.join("external");
    fs::write(&external, "[\"external\"]").unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&external, &path).unwrap();
    assert!(fixture.cli(json!({"action":"list"}), 1).get("error").is_some());
    assert_eq!(fs::read_to_string(&external).unwrap(), "[\"external\"]");
    fs::remove_file(&path).unwrap();
    fs::write(&path, "[\"insecure\"]").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(fixture.cli(json!({"action":"list"}), 1).get("error").is_some());
}

#[test]
fn malformed_store_clear_recovers() {
    let fixture = Fixture::new();
    fixture.call(json!({"action":"record","query":"keep"}));
    fs::write(fixture.history(), "{}").unwrap();
    assert!(fixture.cli(json!({"action":"list"}), 1).get("error").is_some());
    fixture.call(json!({"action":"clear"}));
    assert_eq!(fixture.call(json!({"action":"list"}))["queries"], json!([]));
}
