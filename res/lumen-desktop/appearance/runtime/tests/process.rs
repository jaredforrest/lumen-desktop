//! Persisted edits and explicit history retirement through the installed command boundary.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

fn settings() -> ! {
    let args: Vec<String> = env::args().skip(1).collect();
    let path = env::var("FAKE_SETTINGS").unwrap();
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    if args.first().is_some_and(|arg| arg == "patch") {
        let section = &args[1];
        let patch: Value = serde_json::from_str(&args[2]).unwrap();
        for (key, field) in patch.as_object().unwrap() { value[section][key] = field.clone(); }
        fs::write(path, &value.to_string()).unwrap();
    }
    println!("{value}");
    process::exit(0)
}
fn history() -> ! {
    let mut request = String::new(); std::io::stdin().read_to_string(&mut request).unwrap();
    fs::write(env::var("FAKE_HISTORY_LOG").unwrap(), request).unwrap();
    process::exit(0)
}

struct Fixture { root: PathBuf, state: PathBuf, settings: PathBuf, history: PathBuf, path: String }
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = env::temp_dir().join(format!("lumen-appearance-{}-{nonce}", process::id()));
        fs::create_dir(&root).unwrap();
        let settings = root.join("settings.json");
        fs::write(&settings, json!({
            "appearance":{"textScale":1,"mode":"auto","accent":"blue","iconAppearance":"auto",
                "switchToAppSpace":true,"dockMagnification":true,"reduceMotion":false,
                "reduceTransparency":false,"scrollbars":"auto","increaseContrast":false,
                "sidebarIconSize":"medium","dockPins":null,"dockRecentApps":true,
                "dockIndicators":true,"dockFolders":[],"dockPosition":"bottom","dockAutoHide":false,
                "dockSize":48,"dockFiles":[]},
            "search":{"queryHistory":true,"disabledProviders":[]},
            "notifications":{"preview":"always","policies":{}},
            "controlCentre":{"order":["wifi","bluetooth","sharing","media","displays","appearance","focus","brightness","sound"],"hidden":[]},
            "menuBar":{"order":["battery","sound","focus","search","controls"],"visibility":"desktop"},
            "windows":{"tileMargins":true,"tileGap":8,"titlebarDoubleClick":"fill"},
            "desktop":{"showFiles":false,"fileSort":"name","fileDescending":false}
        }).to_string()).unwrap();
        let executable = env::current_exe().unwrap();
        for name in ["lumen-nix-settings", "lumen-search-history"] { symlink(&executable, root.join(name)).unwrap(); }
        let path = format!("{}:{}", root.display(), env::var("PATH").unwrap());
        let state = root.join("state");
        let history = root.join("history.json");
        Self { root, state, settings, history, path }
    }
    fn appearance(&self, args: &[&str]) -> (bool, Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_lumen-appearance"))
            .args(["--no-desktop", "--state-dir", self.state.to_str().unwrap(), "--at", "2026-06-21T12:00:00+01:00"])
            .args(args)
            .env("PATH", &self.path).env("FAKE_SETTINGS", &self.settings)
            .env("FAKE_HISTORY_LOG", &self.history).env("XDG_STATE_HOME", self.root.join("xdg-state"))
            .output().unwrap();
        let reply = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)));
        (output.status.success(), reply)
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }

fn main() {
    let program = env::args().next().unwrap();
    if let Some(name) = Path::new(&program).file_name().and_then(|name| name.to_str()) {
        match name { "lumen-nix-settings" => settings(), "lumen-search-history" => history(), _ => {} }
    }
    let fixture = Fixture::new();
    let (success, reply) = fixture.appearance(&["--set", "accent", "\"green\""]);
    assert!(success, "{reply}");
    assert_eq!(reply["preferences"]["accent"], "green");
    assert_eq!(reply["appearance"]["mode"], "light");
    let state: Value = serde_json::from_slice(&fs::read(fixture.state.join("appearance.json")).unwrap()).unwrap();
    assert_eq!(state["palette"], reply["appearance"]["palette"]);
    assert!(!fs::read_to_string(fixture.state.join("launcher.rasi")).unwrap().contains("@accent@"));
    let settings: Value = serde_json::from_slice(&fs::read(&fixture.settings).unwrap()).unwrap();
    assert_eq!(settings["appearance"]["accent"], "green");
    let (success, reply) = fixture.appearance(&["--set-search", "queryHistory", " false "]);
    assert!(success, "{reply}");
    let request: Value = serde_json::from_slice(&fs::read(&fixture.history).unwrap()).unwrap();
    assert_eq!(request, json!({"action":"clear"}));

    let fixture = Fixture::new();
    let before = fs::read(&fixture.settings).unwrap();
    let (success, reply) = fixture.appearance(&["--set", "accent", "\"invalid\""]);
    assert!(!success);
    assert!(reply.get("error").is_some());
    assert_eq!(fs::read(&fixture.settings).unwrap(), before);
    assert!(!fixture.state.join("appearance.json").exists());
}
