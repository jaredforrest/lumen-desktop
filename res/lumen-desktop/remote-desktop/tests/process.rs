use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{self, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn client() {
    let args: Vec<_> = env::args().skip(1).collect();
    fs::write(env::var("REMMINA_LOG").unwrap(), args.join("\n") + "\n").unwrap();
}

fn request(root: &Path, value: Value) -> (Output, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-remote-desktop"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("PATH", format!("{}:{}", root.display(), env::var("PATH").unwrap()))
        .env("REMMINA_LOG", root.join("launch.log"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{value}").unwrap();
    let output = child.wait_with_output().unwrap();
    let response = serde_json::from_slice(&output.stdout).unwrap();
    (output, response)
}

fn wait_for_log(root: &Path, expected: &str) -> Vec<String> {
    let path = root.join("launch.log");
    for _ in 0..100 {
        if let Ok(text) = fs::read_to_string(&path) {
            if text.ends_with(expected) {
                return text.lines().map(str::to_owned).collect();
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("Remmina launch not recorded: {}", path.display());
}

fn fixture(root: &Path, self_binary: &Path) {
    fs::create_dir(root).unwrap();
    symlink(self_binary, root.join("remmina")).unwrap();
}

fn main() {
    if Path::new(&env::args().next().unwrap())
        .file_name()
        .is_some_and(|name| name == "remmina")
    {
        client();
        return;
    }
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = env::temp_dir().join(format!("lumen-remmina-test-{}-{unique}", process::id()));
    fs::create_dir(&root).unwrap();
    let self_binary = env::current_exe().unwrap();

    let first = root.join("listing");
    fixture(&first, &self_binary);
    let profiles = first.join("data/remmina");
    fs::create_dir_all(&profiles).unwrap();
    let profile = profiles.join("saved.remmina");
    let contents = "[remmina]\nname=Office\nprotocol=RDP\nserver=rdp://user:sentinel-secret@host\npassword=sentinel-secret\nusername=private-user\nshareprinter=1\n";
    fs::write(&profile, contents).unwrap();
    let (listed_status, listed) = request(&first, json!({"action":"list"}));
    assert!(listed_status.status.success());
    assert!(!first.join("launch.log").exists());
    assert!(!listed.to_string().contains("sentinel-secret") && !listed.to_string().contains("private-user"));
    assert_eq!(listed["connections"][0]["host"], "");
    assert_eq!(listed["connections"][0]["printer"], true);
    let id = listed["connections"][0]["id"].clone();
    let (started_status, started) = request(&first, json!({"action":"connect","id":id}));
    assert!(started_status.status.success());
    assert_eq!(started["started"], true);
    let lines = wait_for_log(&first, &format!("{}\n", profile.display()));
    assert_eq!(&lines[lines.len() - 2..], ["--connect", profile.to_str().unwrap()]);
    assert_eq!(fs::read_to_string(&profile).unwrap(), contents);

    let second = root.join("escape");
    fixture(&second, &self_binary);
    fs::write(second.join("outside.remmina"), "[remmina]\nname=Outside\nprotocol=RDP\n").unwrap();
    let profiles = second.join("data/remmina");
    fs::create_dir_all(&profiles).unwrap();
    symlink(second.join("outside.remmina"), profiles.join("escape.remmina")).unwrap();
    let (_, listed) = request(&second, json!({"action":"list"}));
    assert_eq!(listed["connections"], json!([]));
    let (rejected_status, rejected) = request(&second, json!({"action":"connect","id":"invalid"}));
    assert!(!rejected_status.status.success());
    assert!(rejected["error"].as_str().unwrap().contains("no longer available"));

    let third = root.join("quick");
    fixture(&third, &self_binary);
    for (address, protocol, uri) in [
        ("example.test:3389", "RDP", "rdp://example.test:3389"),
        ("[::1]:5900", "VNC", "vnc://[::1]:5900"),
    ] {
        let (status, _) = request(&third, json!({"action":"quick","address":address,"protocol":protocol}));
        assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
        let lines = wait_for_log(&third, &format!("{uri}\n"));
        assert_eq!(lines.last().unwrap(), uri);
    }
    for address in ["user:password@example.test", "host/path", "host?key=value", "host;bad", "-option", "host:99999", "host\nother"] {
        let (status, _) = request(&third, json!({"action":"quick","address":address,"protocol":"RDP"}));
        assert!(!status.status.success(), "accepted invalid address: {address}");
    }
    fs::remove_dir_all(root).unwrap();
}
