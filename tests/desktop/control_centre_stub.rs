//! Private command peers for the Control Centre offscreen test.
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let executable = env::current_exe()?;
    let name = executable.file_name().and_then(|value| value.to_str()).unwrap_or_default();
    let args: Vec<String> = env::args().collect();
    if name == "lumen-nearby-sharing" {
        if args.get(1).map(String::as_str) != Some("status") { std::process::exit(2); }
        println!("{{\"available\":true,\"active\":true,\"canSend\":true,\"detail\":\"On · 3 min\"}}");
    } else if name == "lumen-system-controls" {
        println!("{{}}");
    } else if name == "lumen-appearance" {
        let mut log = OpenOptions::new().create(true).append(true).open(env::var_os("FIXTURE_LOG").expect("write log"))?;
        writeln!(log, "{} {}", args.get(2).map(String::as_str).unwrap_or_default(), args.get(3).map(String::as_str).unwrap_or_default())?;
        println!("{{}}");
    } else {
        std::process::exit(2);
    }
    Ok(())
}
