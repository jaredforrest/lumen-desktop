//! A rejected drop must never partly launch a desktop application.
use gio::prelude::*;
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Output};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn receiver() -> ! {
    let mut args = env::args().skip(1);
    let destination = args.next().unwrap();
    fs::write(destination, json!(args.collect::<Vec<_>>()).to_string()).unwrap();
    process::exit(0)
}

struct Fixture { root: PathBuf, text: PathBuf, image: PathBuf, log: PathBuf }
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = env::temp_dir().join(format!("lumen-dock-drop-{}-{nonce}", process::id()));
        let applications = root.join("data/applications");
        fs::create_dir_all(&applications).unwrap();
        let text = root.join("literal ü # ? $(untouched).txt");
        fs::write(&text, "first").unwrap();
        let image = root.join("second.png");
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec(); png.extend([0_u8;32]);
        fs::write(&image, png).unwrap();
        let log = root.join("launch.json");
        let executable = root.join("receiver");
        symlink(env::current_exe().unwrap(), &executable).unwrap();
        fs::write(applications.join("lumen-drop-fixture.desktop"), format!(
            "[Desktop Entry]\nType=Application\nName=Drop Fixture\nExec={} {} %U\nMimeType=text/plain;\nTerminal=false\n",
            executable.display(), log.display())).unwrap();
        Self { root, text, image, log }
    }
    fn uri(path: &Path) -> String { gio::File::for_path(path).uri().to_string() }
    fn drop_files(&self, desktop: &str, uris: Value) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lumen-dock-drop"))
            .args([desktop, &uris.to_string()])
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_DATA_DIRS", self.root.join("data"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .output().unwrap()
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }

fn main() {
    if Path::new(&env::args().next().unwrap()).file_name().is_some_and(|name| name == "receiver") { receiver(); }
    let fixture = Fixture::new();
    let text_uri = Fixture::uri(&fixture.text);
    let result = fixture.drop_files("lumen-drop-fixture", json!([text_uri]));
    assert!(result.status.success(), "{} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    assert_eq!(serde_json::from_slice::<Value>(&result.stdout).unwrap(), json!({"ok":true,"count":1}));
    for _ in 0..50 { if fixture.log.exists() { break; } thread::sleep(Duration::from_millis(20)); }
    assert_eq!(serde_json::from_slice::<Value>(&fs::read(&fixture.log).unwrap()).unwrap(), json!([fixture.text]));
    assert_eq!(fs::read_to_string(&fixture.text).unwrap(), "first");
    fs::remove_file(&fixture.log).unwrap();

    let result = fixture.drop_files("lumen-drop-fixture", json!([text_uri, Fixture::uri(&fixture.image)]));
    assert!(!result.status.success());
    assert!(!fixture.log.exists());
    assert_eq!(fs::read_to_string(&fixture.text).unwrap(), "first");
    assert!(fixture.image.exists());

    for invalid in ["https://example.com/a".to_owned(), "file://server/path".into(),
        "file:///tmp/%ZZ".into(), "file:///tmp/%00".into(), format!("{text_uri}#bad"),
        Fixture::uri(&fixture.root.join("missing")), Fixture::uri(&fixture.root)] {
        let result = fixture.drop_files("lumen-drop-fixture", json!([text_uri,invalid]));
        assert!(!result.status.success(), "unexpected success for {invalid}");
    }
    for (desktop, uris) in [("missing", json!([text_uri])), ("../../app", json!([text_uri])),
        ("lumen-drop-fixture", json!([])), ("lumen-drop-fixture", json!(vec![text_uri;101]))] {
        assert!(!fixture.drop_files(desktop, uris).status.success());
    }
    assert!(!fixture.log.exists());
}
