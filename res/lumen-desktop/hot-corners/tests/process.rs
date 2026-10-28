//! Private native event and immutable configuration fixture.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn fake(name: &str) -> ! {
    let args: Vec<_> = env::args().skip(1).collect();
    if name == "swaymsg" {
        if args.iter().any(|arg| arg == "get_version") {
            println!("{}", json!({"native_hot_corners":true}));
        } else if args.iter().any(|arg| arg == "subscribe") {
            print!("{}", fs::read_to_string(env::var("CORNERS_EVENTS").unwrap()).unwrap());
        }
    } else {
        let path = env::var("CORNERS_ACTIONS").unwrap();
        let mut file = fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
        writeln!(file, "{}", args.join(" ")).unwrap();
    }
    process::exit(0)
}

struct Fixture { root: PathBuf, path: String }
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = env::temp_dir().join(format!("lumen-hot-corners-{}-{nonce}", process::id()));
        fs::create_dir(&root).unwrap();
        for name in ["swaymsg", "lumen-desktop-action"] { symlink(env::current_exe().unwrap(), root.join(name)).unwrap(); }
        let path = format!("{}:{}", root.display(), env::var("PATH").unwrap());
        Self { root, path }
    }
    fn call(&self, action: &str, value: Option<Value>, valid: bool) -> Value {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-hot-corners"))
            .args(["--config", self.root.join("corners.json").to_str().unwrap(), action])
            .env("PATH", &self.path).env("CORNERS_EVENTS", self.root.join("events"))
            .env("CORNERS_ACTIONS", self.root.join("actions"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        if let Some(value) = value { writeln!(child.stdin.take().unwrap(), "{value}").unwrap(); }
        else { drop(child.stdin.take()); }
        let result = child.wait_with_output().unwrap();
        assert_eq!(result.status.success(), valid, "{}", String::from_utf8_lossy(&result.stderr));
        serde_json::from_slice(&result.stdout).unwrap_or(Value::Null)
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }

fn main() {
    let program = env::args().next().unwrap();
    if let Some(name) = Path::new(&program).file_name().and_then(|name| name.to_str()) {
        if ["swaymsg", "lumen-desktop-action"].contains(&name) { fake(name); }
    }
    let fixture = Fixture::new();
    fixture.call("configure", Some(json!({"top-left":{"action":"overview","modifiers":["super"]}})), true);
    let config = fixture.root.join("corners.json");
    assert_eq!(fs::metadata(&config).unwrap().permissions().mode() & 0o777, 0o600);
    let status = fixture.call("status", None, true);
    assert_eq!(status["nativeAvailable"], true);
    assert_eq!(status["corners"]["top-left"]["action"], "overview");
    let original = fs::read(&config).unwrap();
    fixture.call("configure", Some(json!({"top-left":{"action":"exec rm"}})), false);
    assert_eq!(fs::read(&config).unwrap(), original);
    let event = json!({"kind":"lumen-corner","version":1,"corner":"top-left","output":"DP-1",
        "modifiers":["super"],"locked":false,"fullscreen":false,"adjacent":false});
    let mut removed = event.clone(); removed["corner"] = Value::Null;
    let mut locked = event.clone(); locked["locked"] = json!(true);
    let stream = [event.clone(), event.clone(), removed.clone(), locked, removed, event]
        .iter().map(|item| json!({"payload":item.to_string()}).to_string()).collect::<Vec<_>>().join("\n") + "\n";
    fs::write(fixture.root.join("events"), stream).unwrap();
    fixture.call("watch", None, true);
    assert_eq!(fs::read_to_string(fixture.root.join("actions")).unwrap(), "overview\noverview\n");
}
