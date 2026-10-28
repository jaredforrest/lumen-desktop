use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn client(name: &str) {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut log = OpenOptions::new().append(true).create(true).open(env::var("VPN_FIXTURE_LOG").unwrap()).unwrap();
    let mut recorded = vec![name.to_owned()];
    recorded.extend(args.iter().cloned());
    writeln!(log, "{}", json!(recorded)).unwrap();
    match (name, args.as_slice()) {
        ("mullvad", [status, flag]) if status == "status" && flag == "--json" =>
            println!("{}", json!({"state":"connected","details":{"location":{"city":"London","country":"United Kingdom","ipv4":"private"}}})),
        ("mullvad", [account, get]) if account == "account" && get == "get" =>
            println!("Mullvad account  :    1234567890123456\nExpires at :        2026-11-18 19:35:16 +00:00"),
        ("mullvad", [relay, get]) if relay == "relay" && get == "get" =>
            println!("Location: country gb"),
        ("mullvad", [account, login]) if account == "account" && login == "login" => {
            let mut value = String::new();
            std::io::stdin().read_to_string(&mut value).unwrap();
            eprintln!("Failed for {}", value.trim());
            process::exit(1);
        }
        ("mullvad", [connect]) if connect == "connect" => println!("connected"),
        ("tailscale", [status, flag]) if status == "status" && flag == "--json" =>
            println!("{}", json!({"BackendState":"Running","Self":{"Online":true},"CurrentTailnet":{"Name":"private.test"},"Peer":{"one":{"Online":true,"ExitNodeOption":true,"ID":"node-1","DNSName":"london.private.test."},"two":{"Online":false}},"ExitNodeStatus":{"ID":"node-1"},"Health":[]})),
        ("tailscale", [set, node]) if set == "set" && node == "--exit-node=node-1" => println!("set"),
        _ => panic!("unexpected {name} argv: {args:?}"),
    }
}

fn call(root: &Path, request: Value) -> Value {
    let mut process = Command::new(env!("CARGO_BIN_EXE_lumen-vpn-control"))
        .env("PATH", format!("{}:{}", root.display(), env::var("PATH").unwrap()))
        .env("VPN_FIXTURE_LOG", root.join("calls.jsonl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(process.stdin.take().unwrap(), "{request}").unwrap();
    let output = process.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn records(path: &Path) -> Vec<Vec<String>> {
    fs::read_to_string(path).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

fn main() {
    let invoked = env::args().next().unwrap();
    let name = Path::new(&invoked).file_name().unwrap().to_str().unwrap();
    if name == "mullvad" || name == "tailscale" {
        client(name);
        return;
    }
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = env::temp_dir().join(format!("lumen-vpn-test-{}-{unique}", process::id()));
    fs::create_dir(&root).unwrap();
    let self_binary = env::current_exe().unwrap();
    symlink(&self_binary, root.join("mullvad")).unwrap();
    symlink(&self_binary, root.join("tailscale")).unwrap();
    let log = root.join("calls.jsonl");

    let state = call(&root, json!({"action":"status"}));
    assert_eq!(state["mullvad"]["state"], "connected");
    assert_eq!(state["mullvad"]["location"], "London, United Kingdom");
    assert_eq!(state["mullvad"]["account"], true);
    assert_eq!(state["mullvad"]["expires"], "2026-11-18");
    assert!(!state.to_string().contains("1234567890123456"));
    assert!(!state["mullvad"]["location"].as_str().unwrap().contains("private"));
    assert_eq!(state["tailscale"]["online"], true);
    assert_eq!(state["tailscale"]["peers"], 1);
    assert_eq!(state["tailscale"]["exitNodes"], json!([{"id":"node-1","name":"london.private.test"}]));

    let before = records(&log);
    for request in [
        json!({"action":"mullvad-location","country":"gb;rm"}),
        json!({"action":"mullvad-login","account":"123"}),
        json!({"action":"arbitrary-command"}),
    ] {
        assert!(call(&root, request).get("error").is_some());
    }
    assert_eq!(records(&log), before);
    assert!(call(&root, json!({"action":"tailscale-exit","id":"unlisted"})).get("error").is_some());
    assert!(!records(&log).iter().any(|row| row.iter().any(|value| value == "set")));
    assert_eq!(call(&root, json!({"action":"mullvad-connect"}))["mullvad"]["state"], "connected");
    assert_eq!(call(&root, json!({"action":"tailscale-exit","id":"node-1"}))["tailscale"]["online"], true);
    assert!(records(&log).contains(&vec!["tailscale".into(), "set".into(), "--exit-node=node-1".into()]));
    let secret = "1234567890123456";
    let answer = call(&root, json!({"action":"mullvad-login","account":secret}));
    assert!(answer.get("error").is_some() && !answer.to_string().contains(secret));
    fs::remove_dir_all(root).unwrap();
}
