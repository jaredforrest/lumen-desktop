//! Private process checks for persisted desktop item and stack arrangements.
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture { root: PathBuf, directory: PathBuf }
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("lumen-layout-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let directory = root.join("Desktop");
        fs::create_dir(&directory).unwrap();
        Self { root, directory }
    }
    fn request(&self, action: &str, items: &[&str]) -> Value {
        json!({"action":action,"directory":self.directory,"items":items})
    }
    fn invoke(&self, request: &Value, valid: bool) -> Value {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-desktop-layout"))
            .env("XDG_STATE_HOME", self.root.join("state"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
        let result = child.wait_with_output().unwrap();
        assert_eq!(result.status.success(), valid, "{}", String::from_utf8_lossy(&result.stderr));
        let response: Value = serde_json::from_slice(&result.stdout).unwrap();
        if !valid { assert!(response.get("error").is_some(), "{response}"); }
        response
    }
    fn save(&self, items: &[&str], layout: Value) -> Value {
        let mut request = self.request("save", items);
        request["layout"] = layout;
        self.invoke(&request, true)["layout"].clone()
    }
    fn storage(&self) -> PathBuf { self.root.join("state/lumen-desktop/desktop-layouts") }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }

#[test]
fn identity_persistence_rename_removal_and_reset() {
    let mut fixture = Fixture::new();
    let layout = json!({"grouping":"kind","positions":{"1:2":[0.2,0.8],"1:3":[0.4,0.6]}});
    assert_eq!(fixture.save(&["1:2","1:3"], layout.clone()), layout);
    let renamed = fixture.root.join("Renamed");
    fs::rename(&fixture.directory, &renamed).unwrap();
    fixture.directory = renamed;
    assert_eq!(fixture.invoke(&fixture.request("load", &["1:2","1:3"]), true)["layout"], layout);
    assert_eq!(fixture.invoke(&fixture.request("load", &["1:2"]), true)["layout"]["positions"], json!({"1:2":[0.2,0.8]}));
    assert_eq!(fixture.invoke(&fixture.request("load", &["1:2","1:3"]), true)["layout"]["positions"], json!({"1:2":[0.2,0.8]}));

    let other = fixture.root.join("Other"); fs::create_dir(&other).unwrap();
    let mut request = fixture.request("load", &["1:2"]);
    request["directory"] = json!(other);
    assert_eq!(fixture.invoke(&request, true)["layout"]["positions"], json!({}));
    assert_eq!(fixture.invoke(&fixture.request("reset", &["1:2"]), true)["layout"], json!({"grouping":"none","positions":{}}));
}

#[test]
fn rejects_invalid_positions_unknown_fields_and_unsafe_storage() {
    let fixture = Fixture::new();
    for point in [json!([-0.1,0]), json!([0,1.1]), json!([true,0]), json!(["NaN",0])] {
        let mut request = fixture.request("save", &["1:2"]);
        request["layout"] = json!({"grouping":"none","positions":{"1:2":point}});
        fixture.invoke(&request, false);
    }
    let mut request = fixture.request("load", &["../../outside"]);
    fixture.invoke(&request, false);
    request = fixture.request("load", &["1:2"]);
    request["unexpected"] = json!(true);
    fixture.invoke(&request, false);
    fixture.invoke(&fixture.request("load", &["1:2"]), true);
    fs::set_permissions(fixture.storage(), fs::Permissions::from_mode(0o755)).unwrap();
    fixture.invoke(&fixture.request("load", &["1:2"]), false);
}

#[test]
fn symlink_store_never_writes_target() {
    let fixture = Fixture::new();
    fixture.invoke(&fixture.request("load", &["1:2"]), true);
    let path = fs::read_dir(fixture.storage()).unwrap().map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json")).unwrap();
    fs::remove_file(&path).unwrap();
    let target = fixture.root.join("target"); fs::write(&target, "keep").unwrap();
    symlink(&target, &path).unwrap();
    fixture.invoke(&fixture.request("load", &["1:2"]), false);
    assert_eq!(fs::read_to_string(target).unwrap(), "keep");
}

#[test]
fn stacks_survive_group_switch_and_rename() {
    let mut fixture = Fixture::new();
    let layout = json!({"grouping":"kind","positions":{"1:2":[0.2,0.8]},"stacks":{"stack:kind:Documents":[0.5,0.4]}});
    assert_eq!(fixture.save(&["1:2"], layout.clone()), layout);
    let mut restored = fixture.invoke(&fixture.request("load", &["1:2"]), true)["layout"].clone();
    restored["grouping"] = json!("none");
    fixture.save(&["1:2"], restored);
    let moved = fixture.root.join("Moved"); fs::rename(&fixture.directory, &moved).unwrap();
    fixture.directory = moved;
    assert_eq!(fixture.invoke(&fixture.request("load", &["1:2"]), true)["layout"]["stacks"], layout["stacks"]);
}

#[test]
fn stacks_validate_scope_and_old_layouts() {
    let fixture = Fixture::new();
    for key in ["stack:kind:../outside", "stack:unknown:Documents", &format!("stack:kind:{}", "a".repeat(200)), "1:2"] {
        let mut request = fixture.request("save", &[]);
        request["layout"] = json!({"grouping":"kind","positions":{},"stacks":{key:[0,0]}});
        fixture.invoke(&request, false);
    }
    let mut request = fixture.request("save", &[]);
    request["layout"] = json!({"grouping":"kind","positions":{},"stacks":{"stack:kind:Documents":[2,0]}});
    fixture.invoke(&request, false);
    assert_eq!(fixture.save(&[], json!({"grouping":"none","positions":{}}))["stacks"], Value::Null);
}
