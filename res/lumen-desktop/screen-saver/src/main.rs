//! Decorative screen saver preferences and a fail-closed compositor lock snapshot.
use serde_json::{Value, json};
use std::env;
use std::fs::OpenOptions;
use std::io::{BufRead, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

type Result<T> = std::result::Result<T, String>;
const MAGIC: &[u8; 6] = b"i3-ipc";

fn settings(args: &[&str]) -> Result<Value> {
    let output = Command::new("lumen-nix-settings").args(args).output().map_err(|e| e.to_string())?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()); }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}

fn connect(path: &str) -> Result<(UnixStream, (u64, u64, i32))> {
    let before = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !before.file_type().is_socket() { return Err("Not an IPC socket".into()); }
    let stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
    let mut peer = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED,
        (&raw mut peer).cast(), &raw mut size) } != 0 || size as usize != std::mem::size_of::<libc::ucred>() {
        return Err("Cannot identify IPC owner".into());
    }
    let after = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if before.dev() != after.dev() || before.ino() != after.ino() || peer.uid != unsafe { libc::geteuid() } {
        return Err("Changed or foreign IPC socket".into());
    }
    Ok((stream, (before.dev(), before.ino(), peer.pid)))
}

fn frame(stream: &mut UnixStream, kind: u32, body: &Value) -> Result<Value> {
    let payload = body.to_string();
    let mut header = [0u8; 14];
    header[..6].copy_from_slice(MAGIC);
    header[6..10].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[10..14].copy_from_slice(&kind.to_le_bytes());
    stream.write_all(&header).map_err(|e| e.to_string())?;
    stream.write_all(payload.as_bytes()).map_err(|e| e.to_string())?;
    stream.read_exact(&mut header).map_err(|e| e.to_string())?;
    let length = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    if &header[..6] != MAGIC || length > 65536 || u32::from_le_bytes(header[10..14].try_into().unwrap()) != kind {
        return Err("Invalid IPC frame".into());
    }
    let mut response = vec![0u8; length];
    stream.read_exact(&mut response).map_err(|e| e.to_string())?;
    serde_json::from_slice(&response).map_err(|e| e.to_string())
}

fn lock_snapshot() -> Value {
    let unknown = || json!({"known":false,"locked":true});
    let result = (|| -> Result<Value> {
        let path = env::var("SWAYSOCK").map_err(|e| e.to_string())?;
        let (mut monitor, token) = connect(&path)?;
        if frame(&mut monitor, 2, &json!(["tick", "shutdown"]))?["success"] != true {
            return Err("Subscription rejected".into());
        }
        let (mut query, current) = connect(&path)?;
        if token != current { return Err("Changed compositor".into()); }
        let reply = frame(&mut query, 7, &json!(""))?;
        let mut byte = [0u8; 1];
        let peek = unsafe { libc::recv(monitor.as_raw_fd(), byte.as_mut_ptr().cast(), 1, libc::MSG_PEEK | libc::MSG_DONTWAIT) };
        if peek == 0 { return Err("Closed event channel".into()); }
        let value = &reply["lumen_session_lock"];
        if value["version"] != 1 || value["available"] != true || !value["locked"].is_boolean()
            || value["revision"].as_i64().is_none_or(|number| number < 0) {
            return Err("Unknown lock authority".into());
        }
        Ok(json!({"known":true,"locked":value["locked"]}))
    })();
    result.unwrap_or_else(|_| unknown())
}

fn status() -> Result<Value> {
    let mut preferences = json!({"enabled":false,"idleSeconds":300,"motion":true});
    if let Some(saved) = settings(&["read"])?["screenSaver"].as_object() {
        for (key, value) in saved { preferences[key] = value.clone(); }
    }
    Ok(json!({"preferences":preferences,"lock":lock_snapshot(),"error":""}))
}

fn runtime() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is missing")?))
}

fn handle(request: &Value) -> Result<Value> {
    if !request.is_object() { return Err("A screen saver request object is required.".into()); }
    match request["action"].as_str() {
        Some("set") => {
            let key = request["key"].as_str().ok_or("Unsupported screen saver preference or value.")?;
            let value = &request["value"];
            let valid = match key {
                "enabled" | "motion" => value.is_boolean(),
                "idleSeconds" => value.as_u64().is_some_and(|number| (15..=7200).contains(&number)),
                _ => false,
            };
            if !valid { return Err("Unsupported screen saver preference or value.".into()); }
            let file = OpenOptions::new().create(true).append(true).mode(0o600)
                .open(runtime()?.join("screen-saver.lock")).map_err(|e| e.to_string())?;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let mut current = json!({"enabled":false,"idleSeconds":300,"motion":true});
            if let Some(saved) = settings(&["read"])?["screenSaver"].as_object() {
                for (name, value) in saved { current[name] = value.clone(); }
            }
            current[key] = value.clone();
            settings(&["set", "screenSaver", &current.to_string()])?;
            drop(file);
            status()
        }
        Some("status") => status(),
        _ => Err("Unsupported screen saver action.".into()),
    }
}

fn main() {
    let mut line = String::new();
    let result = std::io::stdin().lock().read_line(&mut line).map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_str::<Value>(line.lines().next().unwrap_or("")).map_err(|e| e.to_string()))
        .and_then(|request| handle(&request));
    match result { Ok(value) => println!("{value}"), Err(error) => println!("{}", json!({"error":error})) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lock_fails_closed_without_socket() {
        unsafe { env::remove_var("SWAYSOCK"); }
        assert_eq!(lock_snapshot(), json!({"known":false,"locked":true}));
    }
    #[test]
    fn rejects_policy_changes() {
        assert!(handle(&json!({"action":"set","key":"locked","value":false})).is_err());
        assert!(handle(&json!({"action":"set","key":"idleSeconds","value":true})).is_err());
    }
}
