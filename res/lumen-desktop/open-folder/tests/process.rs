use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PREFIX: [&str; 5] = ["-p", "/test/shell", "ipc", "call", "desktop"];

fn stub() {
    let log = env::var("FOLDER_HANDOFF_LOG").expect("log path");
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut record = args.join("\0");
    record.push('\n');
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log)
        .expect("open log")
        .write_all(record.as_bytes())
        .expect("write log");
    std::process::exit(
        env::var("FOLDER_HANDOFF_EXIT")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0),
    );
}

struct Fixture {
    root: PathBuf,
    folder: PathBuf,
    other: PathBuf,
    log: PathBuf,
    executable: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("folder-handoff-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let folder = root.join("literal spaces % # ? ü $(not-a-command)");
        let other = root.join("second");
        fs::create_dir(&folder).unwrap();
        fs::create_dir(&other).unwrap();
        Self {
            log: root.join("calls.log"),
            executable: env::current_exe().unwrap(),
            root,
            folder,
            other,
        }
    }

    fn command(&self, folders: &[&str], code: i32) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lumen-open-folder"))
            .args([
                "--quickshell",
                self.executable.to_str().unwrap(),
                "--shell",
                "/test/shell",
                "--",
            ])
            .args(folders)
            .current_dir(&self.root)
            .env("FOLDER_HANDOFF_STUB", "1")
            .env("FOLDER_HANDOFF_LOG", &self.log)
            .env("FOLDER_HANDOFF_EXIT", code.to_string())
            .output()
            .unwrap()
    }

    fn calls(&self) -> Vec<Vec<String>> {
        fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .map(|line| line.split('\0').map(str::to_owned).collect())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            uri.push(char::from(*byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

fn expected(action: &str, path: &Path) -> Vec<String> {
    PREFIX
        .iter()
        .copied()
        .chain([action, path.to_str().unwrap()])
        .map(str::to_owned)
        .collect()
}

fn check() {
    let fixture = Fixture::new();
    let folder = fixture.folder.to_str().unwrap();
    let other = fixture.other.to_str().unwrap();

    let output = fixture.command(&[&file_uri(&fixture.folder), other], 0);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.calls(),
        [
            expected("files", &fixture.folder),
            expected("filesNew", &fixture.other)
        ]
    );
    fs::remove_file(&fixture.log).unwrap();

    assert!(fixture.command(&[folder], 0).status.success());
    assert!(fixture.command(&[], 0).status.success());
    let calls = fixture.calls();
    assert_eq!(calls[0], expected("files", &fixture.folder));
    assert_eq!(calls[1], expected("files", Path::new("")));
    fs::remove_file(&fixture.log).unwrap();

    let localhost = file_uri(&fixture.other).replacen("file:///", "file://localhost/", 1);
    assert!(fixture.command(&[&localhost], 0).status.success());
    assert!(fixture.command(&["second"], 0).status.success());
    assert_eq!(
        fixture.calls(),
        [
            expected("files", &fixture.other),
            expected("files", &fixture.other)
        ]
    );
    fs::remove_file(&fixture.log).unwrap();

    let regular = fixture.root.join("document.txt");
    fs::write(&regular, "fixture").unwrap();
    let bad = [
        "smb://server/share".to_owned(),
        "https://example.com".to_owned(),
        "file://server/share".to_owned(),
        "file:///tmp/%ZZ".to_owned(),
        "file:relative".to_owned(),
        "file:///tmp/%00".to_owned(),
        format!("{}#fragment", file_uri(&fixture.other)),
        regular.to_str().unwrap().to_owned(),
        fixture.root.join("missing").to_str().unwrap().to_owned(),
    ];
    for value in bad {
        let output = fixture.command(&[folder, &value], 0);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{value}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("Files could not open folder"));
        assert!(!fixture.log.exists(), "partial dispatch for {value}");
    }

    assert_eq!(
        fixture.command(&[folder, other], 17).status.code(),
        Some(17)
    );
    assert_eq!(fixture.calls(), [expected("files", &fixture.folder)]);
    fs::remove_file(&fixture.log).unwrap();

    let desktop = fixture.root.join("test-folder.desktop");
    fs::write(&desktop, format!("[Desktop Entry]\nType=Application\nName=Folder handoff fixture\nExec={} --quickshell {} --shell /test/shell -- %U\nMimeType=inode/directory;\nTerminal=false\n", env!("CARGO_BIN_EXE_lumen-open-folder"), fixture.executable.display())).unwrap();
    let launched = Command::new("gio")
        .args([
            "launch",
            desktop.to_str().unwrap(),
            &file_uri(&fixture.folder),
            &file_uri(&fixture.other),
        ])
        .env("FOLDER_HANDOFF_STUB", "1")
        .env("FOLDER_HANDOFF_LOG", &fixture.log)
        .output()
        .unwrap();
    assert!(
        launched.status.success(),
        "gio: {}",
        String::from_utf8_lossy(&launched.stderr)
    );
    for _ in 0..250 {
        if fixture.log.exists() && fixture.calls().len() == 2 {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        fixture.calls(),
        [
            expected("files", &fixture.folder),
            expected("filesNew", &fixture.other)
        ]
    );
}

fn main() {
    if env::var_os("FOLDER_HANDOFF_STUB").is_some() {
        stub();
    }
    check();
    println!("folder handoff process tests passed");
}
