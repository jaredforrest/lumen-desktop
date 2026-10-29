//! Disposable capture fixture seam: pause one child, then exec the real observer.
use serde_json::json;
use std::env;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

fn start_time() -> Result<String, Box<dyn std::error::Error>> {
    let stat = fs::read_to_string("/proc/self/stat")?;
    let fields = stat.rsplit_once(") ").ok_or("Malformed process stat")?.1;
    Ok(fields
        .split_whitespace()
        .nth(19)
        .ok_or("Missing process start time")?
        .to_owned())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let artifacts = Path::new("/artifacts");
    let real = fs::read_to_string(artifacts.join("capture-real-path"))?;
    let real = Path::new(real.trim());
    let canonical = fs::canonicalize(real)?;
    if !canonical.starts_with("/nix/store/") || !canonical.is_file() {
        return Err("Capture observer must be an immutable Nix binary".into());
    }
    if artifacts.join("hold-capture").exists() {
        let facts = json!({
            "pid": std::process::id(),
            "parent": unsafe { libc::getppid() },
            "start": start_time()?,
        });
        fs::write(artifacts.join("held-child.json"), facts.to_string())?;
        let result = unsafe { libc::kill(libc::getpid(), libc::SIGSTOP) };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Err(Command::new(real)
        .args(env::args_os().skip(1))
        .exec()
        .into())
}
