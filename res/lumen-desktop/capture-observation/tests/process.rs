//! The graph describes observations without claiming capture authorization.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn node(id: Value, kind: &str) -> Value {
    json!({"id":id,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":kind}}})
}
fn link(id: u32, source: u32, target: u32, state: &str) -> Value {
    json!({"id":id,"type":"PipeWire:Interface:Link","info":{"output-node-id":source,"input-node-id":target,"state":state}})
}
fn graph() -> Vec<Value> {
    let mut sink = node(json!(2), "Stream/Input/Audio");
    sink["info"]["props"]["client.id"] = json!("3");
    vec![node(json!(1), "Audio/Source"), sink,
        json!({"id":3,"type":"PipeWire:Interface:Client","info":{"props":{"application.name":"Synthetic recorder"}}}),
        link(4, 1, 2, "active")]
}
fn describe(graph: &Value, valid: bool) -> Option<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-capture-observation"))
        .arg("describe").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    write!(child.stdin.take().unwrap(), "{graph}").unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.success(), valid, "{}", String::from_utf8_lossy(&result.stderr));
    if valid { Some(serde_json::from_slice(&result.stdout).unwrap()) } else { None }
}
fn pw_dump() -> ! {
    match env::var("CAPTURE_MODE").unwrap().as_str() {
        "graph" | "oversize" => {
            let contents = fs::read(env::var("CAPTURE_GRAPH").unwrap()).unwrap();
            std::io::stdout().write_all(&contents).unwrap();
        }
        "failure" => process::exit(2),
        "timeout" => std::thread::sleep(Duration::from_secs(10)),
        _ => unreachable!(),
    }
    process::exit(0)
}
fn snapshot(root: &Path, mode: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_lumen-capture-observation"))
        .env("LUMEN_PW_DUMP", root.join("pw-dump"))
        .env("CAPTURE_GRAPH", root.join("graph.json"))
        .env("CAPTURE_MODE", mode)
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn main() {
    if Path::new(&env::args().next().unwrap()).file_name().is_some_and(|name| name == "pw-dump") { pw_dump(); }
    let active = describe(&json!(graph()), true).unwrap();
    assert_eq!(active["available"], true);
    assert_eq!(active["rows"][0]["kind"], "audio-input");
    assert_eq!(active["rows"][0]["reportedApplication"], "Synthetic recorder");
    assert!(active["rows"][0]["attribution"].as_str().unwrap().contains("not authenticated"));

    let mut paused = graph(); paused[3]["info"]["state"] = json!("paused");
    assert_eq!(describe(&json!(paused), true).unwrap()["rows"], json!([]));
    assert_eq!(describe(&json!(graph()[..3]), true).unwrap()["rows"], json!([]));
    let video = describe(&json!([node(json!(1), "Video/Source"), node(json!(2), "Stream/Input/Video"), link(3,1,2,"active")]), true).unwrap();
    assert_eq!(video["rows"][0]["kind"], "video-input");
    assert!(!video["rows"].to_string().contains("camera"));
    let mut duplicated_link = graph(); duplicated_link.push(link(5,1,2,"active"));
    assert_eq!(describe(&json!(duplicated_link), true).unwrap()["rows"][0]["sources"], json!(["1"]));

    for invalid in [json!(graph()[1..]), json!([graph(), vec![node(json!(1), "Audio/Source")]].concat()),
        json!([node(json!(true), "Audio/Source")]), json!([node(json!(1_u64 << 40), "Audio/Source")])] {
        describe(&invalid, false);
    }
    let empty = describe(&json!([]), true).unwrap();
    assert_eq!(empty["available"], true);
    assert_eq!(empty["rows"], json!([]));
    assert!(empty["coverage"].as_str().unwrap().contains("not assessed"));

    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root: PathBuf = env::temp_dir().join(format!("lumen-capture-{}-{nonce}", process::id()));
    fs::create_dir(&root).unwrap();
    symlink(env::current_exe().unwrap(), root.join("pw-dump")).unwrap();
    fs::write(root.join("graph.json"), json!(graph()).to_string()).unwrap();
    assert_eq!(snapshot(&root, "graph")["rows"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot(&root, "failure")["available"], false);
    fs::write(root.join("graph.json"), "x".repeat(8*1024*1024+1)).unwrap();
    assert_eq!(snapshot(&root, "oversize")["available"], false);
    let started = Instant::now();
    assert_eq!(snapshot(&root, "timeout")["available"], false);
    assert!(started.elapsed() < Duration::from_secs(4));
    fs::remove_dir_all(root).unwrap();
}
