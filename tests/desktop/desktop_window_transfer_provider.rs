//! Disposable stdio confirmations for the production captured transfer controller.
use std::{
    env, fs,
    io::{self, BufRead, Read, Write},
    thread,
    time::Duration,
};
fn main() {
    let mut line = Vec::new();
    io::stdin()
        .lock()
        .take(16385)
        .read_until(b'\n', &mut line)
        .unwrap();
    assert!(line.len() <= 16384 && line.last() == Some(&b'\n'));
    let request = String::from_utf8(line).unwrap();
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(env::var("LUMEN_TRANSFER_LOG").unwrap())
        .unwrap();
    log.write_all(request.as_bytes()).unwrap();
    log.flush().unwrap();
    if request.contains("\"foreign\":\"stall\"") {
        thread::sleep(Duration::from_secs(60));
    } else if request.contains("\"foreign\":\"malformed\"") {
        println!("invalid response");
    } else if request.contains("\"foreign\":\"stale\"") {
        println!("{{\"error\":\"The captured source changed\"}}");
    } else {
        println!("{{\"ok\":true}}");
    }
}
