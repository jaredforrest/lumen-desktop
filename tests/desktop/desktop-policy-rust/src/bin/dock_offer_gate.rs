use serde_json::json;
use std::env;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gate = PathBuf::from(env::var("LUMEN_DOCK_GATE_PATH")?);
    let record = PathBuf::from(env::var("LUMEN_DOCK_GATE_RECORD")?);
    let helper = PathBuf::from(env::var("LUMEN_DOCK_GATE_HELPER")?);
    let args: Vec<String> = env::args().skip(1).collect();
    let temp = record.with_extension("tmp");
    fs::write(&temp, json!({"pid":std::process::id(),"argv":args.clone()}).to_string())?;
    fs::rename(temp, record)?;
    let deadline = Instant::now() + Duration::from_secs(12);
    while !gate.exists() {
        if Instant::now() >= deadline {
            println!("{}",json!({"error":"Private helper gate expired"}));
            std::process::exit(1);
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(Command::new(helper).args(args).exec().into())
}
