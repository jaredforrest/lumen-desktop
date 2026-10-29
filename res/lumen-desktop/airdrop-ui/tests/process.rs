//! Process fixtures for the receive control and streamed peer picker.
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ADDRESS: &str = "c6:6b:14:cd:8a:63";

fn fixture(name: &str) -> ! {
    let args: Vec<String> = env::args().skip(1).collect();
    match name {
        "iphone-airdrop" => match args.first().map(String::as_str) {
            Some("status") => print!("{}", env::var("AIR_STATUS").unwrap()),
            Some("peers") => {
                println!("{ADDRESS}  -55 dBm  [fe80::1%awdl0]:8770  iPhone");
                std::io::stdout().flush().unwrap();
                std::thread::sleep(Duration::from_secs(20));
            }
            _ => fs::write(env::var("AIR_ACTION").unwrap(), args.join("\n")).unwrap(),
        },
        "rofi" => {
            fs::write(env::var("ROFI_ARGS").unwrap(), args.join("\n")).unwrap();
            let mut input = String::new(); std::io::stdin().read_to_string(&mut input).unwrap();
            fs::write(env::var("ROFI_INPUT").unwrap(), input).unwrap();
            print!("{}", env::var("ROFI_CHOICE").unwrap());
        }
        "zenity" => {
            if args.iter().any(|arg| arg == "--progress") {
                std::thread::sleep(Duration::from_secs(30));
            }
        }
        _ => unreachable!(),
    }
    process::exit(0)
}

struct Fixture { root: PathBuf, path: String }
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = env::temp_dir().join(format!("lumen-airdrop-ui-{}-{nonce}", process::id()));
        fs::create_dir(&root).unwrap();
        let exe = env::current_exe().unwrap();
        for command in ["iphone-airdrop", "rofi", "zenity"] { symlink(&exe, root.join(command)).unwrap(); }
        let path = format!("{}:{}", root.display(), env::var("PATH").unwrap());
        Self { root, path }
    }
    fn command(&self, binary: &str, choice: &str) -> Command {
        let binary = match binary {
            "receive" => env!("CARGO_BIN_EXE_lumen-airdrop-receive-ui"),
            "send" => env!("CARGO_BIN_EXE_lumen-airdrop-send-ui"),
            _ => unreachable!(),
        };
        let mut command = Command::new(binary);
        command.env("PATH", &self.path)
            .env("XDG_RUNTIME_DIR", &self.root)
            .env("AIR_ACTION", self.root.join("action"))
            .env("ROFI_INPUT", self.root.join("input"))
            .env("ROFI_ARGS", self.root.join("args"))
            .env("ROFI_CHOICE", choice);
        command
    }
    fn read(&self, name: &str) -> String { fs::read_to_string(self.root.join(name)).unwrap() }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }

fn receive(status: &str, choice: &str, expected_action: &str, message: &str) {
    let fixture = Fixture::new();
    let result = fixture.command("receive", choice).env("AIR_STATUS", status).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(fixture.read("action"), expected_action);
    assert!(fixture.read("input").contains(choice));
    assert!(fixture.read("args").contains(message));
}

fn send(choice: &str, path_count: usize, expected_success: bool) {
    let fixture = Fixture::new();
    let files: Vec<PathBuf> = (0..path_count).map(|index| fixture.root.join(format!("file {index}.txt"))).collect();
    for file in &files { fs::write(file, "sample").unwrap(); }
    let started = Instant::now();
    let result = fixture.command("send", choice).args(&files).output().unwrap();
    assert_eq!(result.status.success(), expected_success, "{}", String::from_utf8_lossy(&result.stderr));
    assert!(started.elapsed() < Duration::from_secs(6));
    assert!(fixture.read("input").contains(ADDRESS));
    if expected_success {
        let action: Vec<_> = fixture.read("action").lines().map(str::to_owned).collect();
        if path_count == 1 {
            assert_eq!(action, ["send", files[0].to_str().unwrap(), ADDRESS]);
        } else {
            assert_eq!(action[0], "send-files");
            assert_eq!(action[1], ADDRESS);
            assert_eq!(action[2..], files.iter().map(|path| path.to_str().unwrap()).collect::<Vec<_>>());
        }
    } else {
        assert!(!fixture.root.join("action").exists());
    }
}

fn main() {
    let name = env::args().next().unwrap();
    if let Some(name) = Path::new(&name).file_name().and_then(|name| name.to_str()) {
        if ["iphone-airdrop", "rofi", "zenity"].contains(&name) { fixture(name); }
    }
    receive("{\"ready\":false}", "Turn On for 10 Minutes", "on", "AirDrop receiving is off");
    receive("{\"ready\":true,\"remaining\":421}", "Turn Off", "off", "8 more minute");
    send(&format!("iPhone  ·  {ADDRESS}"), 1, true);
    send("unlisted device", 1, false);
    send(&format!("iPhone  ·  {ADDRESS}"), 2, true);
}
