//! Disposable owned stdio faults for the actual group-presentation QML transport.
use std::{
    env, fs,
    io::{self, BufRead, Read, Write},
    thread,
    time::Duration,
};
fn main() {
    let mode = env::args().nth(1).unwrap_or_default();
    if mode == "watch" {
        println!("{{\"available\":false,\"enabled\":false,\"locked\":true,\"outputs\":[]}}");
        io::stdout().flush().unwrap();
        thread::sleep(Duration::from_secs(60));
        return;
    }
    assert_eq!(mode, "mode");
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(4097)
        .read_until(b'\n', &mut bytes)
        .unwrap();
    assert!(bytes.len() <= 4096 && bytes.last() == Some(&b'\n'));
    let request = String::from_utf8(bytes).unwrap();
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(env::var("LUMEN_GROUP_PRESENTATION_LOG").unwrap())
        .unwrap();
    log.write_all(request.as_bytes()).unwrap();
    log.flush().unwrap();
    if request.contains("\"epoch\":\"stall\"") {
        thread::sleep(Duration::from_secs(60));
        return;
    }
    if request.contains("\"epoch\":\"malformed\"") {
        println!("malformed reply");
    } else if request.contains("\"epoch\":\"stale\"") {
        println!("{{\"error\":\"The group list changed\"}}");
    } else {
        println!("{{\"ok\":true}}");
    }
}
