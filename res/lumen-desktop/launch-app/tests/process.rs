use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn mock_program(args: &[String]) {
    let mut record = File::create(env::var("LUMEN_LAUNCH_TEST_RECORD").unwrap()).unwrap();
    writeln!(record, "{}", env::current_dir().unwrap().display()).unwrap();
    writeln!(
        record,
        "{}",
        env::var("LUMEN_LAUNCH_TEST_SENTINEL").unwrap()
    )
    .unwrap();
    for arg in args {
        writeln!(record, "{arg}").unwrap();
    }
    let code = env::var("LUMEN_LAUNCH_TEST_EXIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    std::process::exit(code);
}

fn rows(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn test_process() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::temp_dir().join(format!("lumen-launch-app-{}-{unique}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let mock = env::current_exe().unwrap();
    symlink(&mock, root.join("systemd-run")).unwrap();
    let helper = env!("CARGO_BIN_EXE_lumen-launch-app");
    let literal = [
        "literal command",
        "space value",
        "$HOME",
        "${PATH}",
        "$(touch should-not-exist)",
        ";",
        "--flag",
    ];

    let scope_record = root.join("scope-record");
    let scoped = Command::new(helper)
        .arg("--")
        .args(literal)
        .current_dir(&root)
        .env("PATH", &root)
        .env("LUMEN_LAUNCH_SCOPES", "1")
        .env("LUMEN_LAUNCH_TEST_MODE", "stub")
        .env("LUMEN_LAUNCH_TEST_SENTINEL", "preserved value")
        .env("LUMEN_LAUNCH_TEST_RECORD", &scope_record)
        .output()
        .unwrap();
    assert!(
        scoped.status.success(),
        "{}",
        String::from_utf8_lossy(&scoped.stderr)
    );
    let scoped = rows(&scope_record);
    assert_eq!(scoped[0], root.display().to_string());
    assert_eq!(scoped[1], "preserved value");
    assert_eq!(
        &scoped[2..7],
        [
            "--user",
            "--scope",
            "--collect",
            "--quiet",
            "--expand-environment=no"
        ]
    );
    let unit = scoped[7].strip_prefix("--unit=app-lumen-desktop-").unwrap();
    assert_eq!(unit.len(), 32);
    assert!(unit.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(scoped[8], "--");
    assert_eq!(&scoped[9..], literal);
    assert!(!root.join("should-not-exist").exists());

    let direct_record = root.join("direct-record");
    let direct = Command::new(helper)
        .arg(&mock)
        .args(literal)
        .current_dir(&root)
        .env("PATH", &root)
        .env("LUMEN_LAUNCH_SCOPES", "0")
        .env("LUMEN_LAUNCH_TEST_MODE", "stub")
        .env("LUMEN_LAUNCH_TEST_SENTINEL", "preserved value")
        .env("LUMEN_LAUNCH_TEST_RECORD", &direct_record)
        .output()
        .unwrap();
    assert!(direct.status.success());
    let direct = rows(&direct_record);
    assert_eq!(direct[0], root.display().to_string());
    assert_eq!(direct[1], "preserved value");
    assert_eq!(&direct[2..], literal);

    let missing = Command::new(helper)
        .env("PATH", &root)
        .env("LUMEN_LAUNCH_SCOPES", "0")
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));

    let failed_record = root.join("failed-record");
    let failed = Command::new(helper)
        .arg(&mock)
        .current_dir(&root)
        .env("PATH", &root)
        .env("LUMEN_LAUNCH_SCOPES", "1")
        .env("LUMEN_LAUNCH_TEST_MODE", "stub")
        .env("LUMEN_LAUNCH_TEST_SENTINEL", "preserved value")
        .env("LUMEN_LAUNCH_TEST_RECORD", &failed_record)
        .env("LUMEN_LAUNCH_TEST_EXIT", "19")
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(19));
    assert!(failed.stderr.is_empty());
    let failed = rows(&failed_record);
    assert_eq!(&failed[2..7], &scoped[2..7]);
    assert_eq!(failed[8], "--");
    assert_eq!(failed[9], mock.display().to_string());

    fs::remove_dir_all(root).unwrap();
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if env::var("LUMEN_LAUNCH_TEST_MODE").as_deref() == Ok("stub") {
        mock_program(&args);
    } else {
        test_process();
    }
}
