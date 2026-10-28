//! Delayed capture response peer for generation-retirement testing.
use std::env;
use std::fs;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

static TERMINATED: AtomicBool = AtomicBool::new(false);
unsafe extern "C" { fn signal(number: i32, handler: extern "C" fn(i32)) -> usize; }
extern "C" fn terminate(_: i32) { TERMINATED.store(true, Ordering::SeqCst); }

fn main() -> io::Result<()> {
    let counter = env::var_os("LUMEN_CAPTURE_COUNTER").expect("counter path");
    let before = fs::read_to_string(&counter)?;
    let serial = if before.contains("\"serial\":0") { 1 } else { 2 };
    if serial == 1 { unsafe { signal(15, terminate); } }
    let reported = if serial == 1 { "retired" } else { "fresh" };
    println!("{{\"available\":true,\"rows\":[{{\"kind\":\"audio-input\",\"reportedApplication\":\"{reported}\",\"attribution\":\"Synthetic private lifecycle\"}}],\"coverage\":\"Synthetic only\"}}");
    io::stdout().flush()?;
    fs::write(counter, format!("{{\"serial\":{serial}}}"))?;
    if serial == 1 {
        for _ in 0..600 {
            if TERMINATED.load(Ordering::SeqCst) { thread::sleep(Duration::from_millis(100)); break; }
            thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}
