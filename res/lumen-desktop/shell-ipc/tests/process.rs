use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{self, Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn stub() -> ! {
    let invoked_as = env::args().next().unwrap();
    let mode = Path::new(&invoked_as).file_name().unwrap().to_str().unwrap();
    match mode {
        "systemctl" => {
            println!("{}", env::var("IPC_PID_VALUE").unwrap_or_default());
            process::exit(0);
        }
        "quickshell" => {
            let args: Vec<_> = env::args().skip(1).collect();
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(env::var("IPC_CALLS").unwrap())
                .unwrap();
            writeln!(file, "{}", serde_json::to_string(&args).unwrap()).unwrap();
            process::exit(env::var("IPC_EXIT").unwrap().parse().unwrap());
        }
        mode => panic!("unexpected stub mode: {mode}"),
    }
}

fn calls(path: &Path) -> Vec<Vec<String>> {
    if !path.exists() {
        return Vec::new();
    }
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn route(root: &Path, calls: &Path, pid: &str, exit_code: i32) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lumen-shell-ipc"))
        .args([
            "--systemctl",
            root.join("systemctl").to_str().unwrap(),
            "--quickshell",
            root.join("quickshell").to_str().unwrap(),
            "--",
            "call",
            "desktop",
            "appSwitcher",
            "next",
        ])
        .env("IPC_PID_VALUE", pid)
        .env("IPC_CALLS", calls)
        .env("IPC_EXIT", exit_code.to_string())
        .output()
        .unwrap()
}

fn main() {
    let invoked_as = env::args().next().unwrap();
    if matches!(Path::new(&invoked_as).file_name().unwrap().to_str(), Some("systemctl" | "quickshell")) {
        stub();
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::temp_dir().join(format!("lumen-ipc-{}-{unique}", process::id()));
    fs::create_dir(&root).unwrap();
    let self_binary = env::current_exe().unwrap();
    symlink(&self_binary, root.join("systemctl")).unwrap();
    symlink(&self_binary, root.join("quickshell")).unwrap();
    let calls_path = root.join("calls.jsonl");

    // A live, same-user service PID receives exactly one literal IPC call.
    let pid = process::id().to_string();
    let success = route(&root, &calls_path, &pid, 0);
    assert_eq!(success.status.code(), Some(0), "{}", String::from_utf8_lossy(&success.stderr));
    assert_eq!(calls(&calls_path), [vec![
        "ipc".to_owned(), "--pid".to_owned(), pid.clone(),
        "call".to_owned(), "desktop".to_owned(), "appSwitcher".to_owned(), "next".to_owned(),
    ]]);

    fs::remove_file(&calls_path).unwrap();
    for invalid in ["0", "", "123\n456", "-1", "１２３", "999999999"] {
        let rejected = route(&root, &calls_path, invalid, 0);
        assert_eq!(rejected.status.code(), Some(1), "PID {invalid:?}");
    }
    assert!(calls(&calls_path).is_empty());

    // A failed dispatch is never retried against a replacement process.
    let failed = route(&root, &calls_path, &pid, 1);
    assert_eq!(failed.status.code(), Some(1));
    assert_eq!(calls(&calls_path).len(), 1);
    fs::remove_dir_all(root).unwrap();
}
