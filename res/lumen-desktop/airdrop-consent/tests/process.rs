//! Exercise the consent binary with a Rust dialog fixture and literal labels.
use serde_json::json;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn rofi() -> ! {
    let args: Vec<_> = env::args().skip(1).collect();
    fs::write(env::var("ROFI_ARGS").unwrap(), args.join("\n")).unwrap();
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    fs::write(env::var("ROFI_INPUT").unwrap(), input).unwrap();
    print!("{}", env::var("ROFI_OUTPUT").unwrap_or_default());
    process::exit(env::var("ROFI_CODE").unwrap_or_else(|_| "0".into()).parse().unwrap());
}

fn invoke(root: &Path, request: &str, output: &str, code: i32) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-airdrop-consent"))
        .env("LUMEN_AIRDROP_ROFI", root.join("rofi"))
        .env("ROFI_ARGS", root.join("args"))
        .env("ROFI_INPUT", root.join("input"))
        .env("ROFI_OUTPUT", output)
        .env("ROFI_CODE", code.to_string())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(request.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn main() {
    if Path::new(&env::args().next().unwrap()).file_name().is_some_and(|name| name == "rofi") { rofi(); }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root: PathBuf = env::temp_dir().join(format!("lumen-consent-{}-{nonce}", process::id()));
    fs::create_dir(&root).unwrap();
    symlink(env::current_exe().unwrap(), root.join("rofi")).unwrap();
    let request = json!({"sender": format!("<b>iPhone</b>{}", "x".repeat(200)), "files":[{"name":"<bad>.heic"}]}).to_string();
    for (choice, code, accepted) in [("Accept\n", 0, true), ("Decline\n", 0, false), ("Accept\n", 1, false), ("", 0, false)] {
        let result = invoke(&root, &request, choice, code);
        assert_eq!(result.status.success(), accepted, "{}", String::from_utf8_lossy(&result.stderr));
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), if accepted { "accept" } else { "" });
    }
    let args = fs::read_to_string(root.join("args")).unwrap();
    assert!(args.contains("&lt;b&gt;iPhone&lt;/b&gt;"));
    assert!(args.contains("&lt;bad&gt;.heic"));
    assert!(args.len() < 250);
    assert_eq!(fs::read_to_string(root.join("input")).unwrap(), "Decline\nAccept\n");
    let link = json!({"sender":"iPhone","items":["https://example.com/article"]}).to_string();
    let rejected = invoke(&root, &link, "", 1);
    assert!(!rejected.status.success());
    assert!(fs::read_to_string(root.join("args")).unwrap().contains("https://example.com/article"));
    fs::remove_file(root.join("rofi")).unwrap();
    assert!(!invoke(&root, "{}", "", 0).status.success());
    fs::remove_dir_all(root).unwrap();
}
