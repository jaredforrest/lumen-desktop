use serde_json::{Value,json};
use std::env;
use std::fs;
use std::io::{BufRead,BufReader,Write};
use std::os::unix::fs::symlink;
use std::process::{Child,Command,Stdio};
use std::sync::mpsc;
use std::time::{Duration,Instant};

fn read_until(rx:&mpsc::Receiver<Value>,check:impl Fn(&Value)->bool)->Value {
    let deadline=Instant::now()+Duration::from_secs(6);
    while Instant::now()<deadline {
        if let Ok(value)=rx.recv_timeout(Duration::from_millis(100)) {if check(&value){return value;}}
    }
    panic!("Expected menu message was not received");
}
fn lines(child:&mut Child)->mpsc::Receiver<Value> {
    let stdout=child.stdout.take().unwrap();let (tx,rx)=mpsc::channel();
    std::thread::spawn(move ||for line in BufReader::new(stdout).lines().flatten() {
        if let Ok(value)=serde_json::from_str(&line){if tx.send(value).is_err(){break;}}
    });rx
}
#[test] fn private_bus() {
    if env::var_os("MENU_BRIDGE_CHILD").is_some(){child();return;}
    let mut runner=Command::new("timeout");runner.args(["20","dbus-run-session"]);
    if let Ok(config)=env::var("LUMEN_TEST_DBUS_CONFIG"){runner.arg(format!("--config-file={config}"));}
    let output=runner.arg("--").arg(env::current_exe().unwrap()).args(["--exact","private_bus","--nocapture"])
        .env("MENU_BRIDGE_CHILD","1").output().unwrap();
    assert!(output.status.success(),"{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
}
fn child() {
    let root=tempfile::tempdir().unwrap();let bin=root.path().join("bin");fs::create_dir(&bin).unwrap();
    let sway=bin.join("swaymsg");
    let source=concat!(env!("CARGO_MANIFEST_DIR"),"/tests/support/mock_sway.rs");
    assert!(Command::new("rustc").args(["--edition=2024",source,"-o"]).arg(&sway).status().unwrap().success());
    let exporter_path=env::var("LUMEN_TEST_EXPORT_BIN").unwrap_or_else(|_|"lumen-menu-export".into());
    let helper=bin.join("export-helper");symlink(exporter_path,&helper).unwrap();
    let tree=json!({"type":"root","nodes":[{"type":"output","name":"DP-1","nodes":[{"type":"workspace","nodes":[{"type":"con","id":77,"window":23,"pid":std::process::id(),"focused":true,"name":"Editor","window_properties":{"class":"codex"}}]}]}]});
    let path=format!("{}:{}",bin.display(),env::var("PATH").unwrap_or_default());
    let mut bridge=Command::new(env!("CARGO_BIN_EXE_lumen-menu-bridge"))
        .env("GLOBAL_MENU_ENABLED","1").env("MOCK_SWAY_TREE",tree.to_string()).env("PATH",path)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let bridge_rx=lines(&mut bridge);
    let _=read_until(&bridge_rx,|state|state["token"].as_str().is_some());
    let mut exporter=Command::new(helper).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let exporter_rx=lines(&mut exporter);
    writeln!(exporter.stdin.as_mut().unwrap(),"{}",json!([{"xid":23,"menu":{"id":0,"props":{},"children":[{"id":7,"props":{"label":"Save","enabled":true,"visible":true},"children":[]}]}}])).unwrap();
    read_until(&exporter_rx,|value|value["registered"]==true);
    let state=read_until(&bridge_rx,|value|value["supported"]==true);
    assert_eq!(state["title"],"Editor");assert_eq!(state["menu"]["children"][0]["label"],"Save");
    assert_eq!(state["revision"],1);assert_eq!(state["outputName"],"DP-1");
    assert_eq!(state["contextAppName"],"codex");assert_eq!(state["contextWindowId"],77);
    assert!(state["mainWindowId"].is_null());
    let token=state["token"].as_str().unwrap();
    writeln!(bridge.stdin.as_mut().unwrap(),"{}",json!({"action":"activate","token":"stale","id":7})).unwrap();
    assert!(exporter_rx.recv_timeout(Duration::from_millis(200)).is_err());
    writeln!(bridge.stdin.as_mut().unwrap(),"{}",json!({"action":"activate_when_focused","token":token,"id":7,"revision":0})).unwrap();
    assert!(exporter_rx.recv_timeout(Duration::from_millis(200)).is_err());
    writeln!(bridge.stdin.as_mut().unwrap(),"{}",json!({"action":"activate","token":token,"id":7})).unwrap();
    let click=read_until(&exporter_rx,|value|value["id"]==7);
    assert_eq!(click["xid"],23);
    bridge.kill().unwrap();exporter.kill().unwrap();let _=bridge.wait();let _=exporter.wait();
}
