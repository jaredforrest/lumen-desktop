mod ipc;
use lumen_shortcut_layout::Observer;
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;
const MAX: usize = 2 * 1024 * 1024;
fn regular(path: &Path) -> Result<Vec<u8>, String> {
    // Declarative config symlinks are intentional; descriptor metadata is authoritative.
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "Shortcut registry unavailable")?;
    let metadata = file
        .metadata()
        .map_err(|_| "Shortcut registry metadata unavailable")?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 {
        return Err("Shortcut registry is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(512 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Shortcut registry read failed")?;
    if bytes.len() > 512 * 1024 {
        return Err("Shortcut registry read exceeds limits".into());
    }
    Ok(bytes)
}
fn query(path: &Path) -> Result<(Value, String), String> {
    let before =
        fs::symlink_metadata(path).map_err(|_| "Native keyboard connection unavailable")?;
    if !before.file_type().is_socket() || before.uid() != unsafe { libc::geteuid() } {
        return Err("Native keyboard socket ownership unavailable".into());
    }
    let deadline = ipc::deadline();
    let mut socket = ipc::connect(path, deadline)?;
    let mut peer = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut peer as *mut libc::ucred).cast(),
            &mut len,
        )
    } != 0
        || peer.uid != before.uid()
        || peer.pid <= 0
    {
        return Err("Native keyboard peer unavailable".into());
    }
    let stat = fs::read_to_string(format!("/proc/{}/stat", peer.pid))
        .map_err(|_| "Native keyboard process unavailable")?;
    let start = stat
        .rsplit_once(')')
        .and_then(|(_, tail)| tail.split_whitespace().nth(19))
        .ok_or("Native keyboard process identity unavailable")?;
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map_err(|_| "Native keyboard session unavailable")?;
    let bytes = ipc::seats(&mut socket, deadline)?;
    let after = fs::symlink_metadata(path).map_err(|_| "Native keyboard socket retired")?;
    if before.dev() != after.dev() || before.ino() != after.ino() || before.uid() != after.uid() {
        return Err("Native keyboard socket changed".into());
    }
    let end = fs::read_to_string(format!("/proc/{}/stat", peer.pid))
        .map_err(|_| "Native keyboard process retired")?;
    if end
        .rsplit_once(')')
        .and_then(|(_, tail)| tail.split_whitespace().nth(19))
        != Some(start)
    {
        return Err("Native keyboard process changed".into());
    }
    Ok((
        serde_json::from_slice(&bytes).map_err(|_| "Invalid native keyboard reply")?,
        format!(
            "{}:{}:{}:{}:{start}:{}",
            before.dev(),
            before.ino(),
            peer.pid,
            peer.uid,
            boot.trim()
        ),
    ))
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let watch = args == ["watch"];
    let preflight = args.len() == 3 && args[0] == "preflight" && args[2].len() <= 512;
    if !watch && !preflight && !args.is_empty() {
        eprintln!("Expected no arguments, watch, or preflight FILE ID");
        std::process::exit(2)
    }
    let mut failed: bool;
    let mut observer = Observer::default();
    let mut previous = Vec::new();
    loop {
        let result = (|| {
            let socket = PathBuf::from(
                std::env::var_os("SWAYSOCK").ok_or("Native keyboard socket unavailable")?,
            );
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .ok_or("Shortcut registry path unavailable")?;
            let registry = serde_json::from_slice::<Value>(&regular(
                &config.join("lumen-desktop/shortcuts.json"),
            )?)
            .map_err(|_| "Invalid shortcut registry")?;
            let (seats, session) = query(&socket)?;
            if preflight {
                let candidate = serde_json::from_slice::<Value>(&regular(Path::new(&args[1]))?)
                    .map_err(|_| "Invalid candidate registry")?;
                lumen_shortcut_layout::preflight(&seats, &candidate, &args[2], &session)
            } else {
                observer.observe(&seats, &registry, &session)
            }
        })();
        failed = result.is_err();
        let value=result.unwrap_or_else(|error:String|json!({"version":1,"available":false,"generation":"","error":error,"seats":[],"diagnostics":[]}));
        let bytes = serde_json::to_vec(&value).unwrap();
        let bytes = if bytes.len() > MAX {
            failed = true;
            serde_json::to_vec(&json!({"version":1,"available":false,"generation":"","error":"Keyboard result exceeds limits","seats":[],"diagnostics":[]})).unwrap()
        } else {
            bytes
        };
        if !watch || bytes != previous {
            let mut stdout = std::io::stdout().lock();
            if stdout
                .write_all(&bytes)
                .and_then(|_| stdout.write_all(b"\n"))
                .and_then(|_| stdout.flush())
                .is_err()
            {
                break;
            }
            previous = bytes;
        }
        if !watch {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if preflight && failed {
        std::process::exit(2)
    }
}
