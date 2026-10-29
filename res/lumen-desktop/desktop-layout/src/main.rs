use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

const MAX_ITEMS: usize = 5000;

fn identity(value: &str) -> bool {
    let (numbers, suffix) = value.split_once('-').unwrap_or((value, ""));
    let Some((device, inode)) = numbers.split_once(':') else { return false; };
    let decimal = |part: &str| (1..=24).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit());
    decimal(device) && decimal(inode) && (!value.contains('-') || suffix.len() == 16 && suffix.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}

fn stack_identity(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("stack:") else { return false; };
    let Some((kind, name)) = rest.split_once(':') else { return false; };
    matches!(kind, "kind" | "modified" | "tags") && (1..=128).contains(&name.len()) && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b' ' || byte == b',')
}

fn point(value: &Value) -> bool {
    value.as_array().is_some_and(|parts| parts.len() == 2 && parts.iter().all(|item| item.as_f64().is_some_and(|number| number.is_finite() && (0.0..=1.0).contains(&number)) && !item.is_boolean()))
}

fn validate(value: &Value) -> Result<Value, String> {
    let object = value.as_object().ok_or("Invalid desktop arrangement")?;
    if object.keys().any(|key| !matches!(key.as_str(), "grouping" | "positions" | "stacks")) { return Err("Invalid desktop arrangement".into()); }
    let grouping = object.get("grouping").and_then(Value::as_str).filter(|name| matches!(*name, "none" | "kind" | "modified" | "tags")).ok_or("Invalid desktop arrangement")?;
    let positions = object.get("positions").and_then(Value::as_object).filter(|items| items.len() <= MAX_ITEMS).ok_or("Invalid desktop arrangement")?;
    if positions.iter().any(|(key, value)| !identity(key) || !point(value)) { return Err("Invalid desktop item position".into()); }
    let stacks = match object.get("stacks") { Some(value) => value.as_object().ok_or("Invalid desktop stack arrangement")?, None => &Map::new() };
    if stacks.len() > 64 || stacks.iter().any(|(key, value)| !stack_identity(key) || !point(value)) { return Err("Invalid desktop stack arrangement".into()); }
    let mut result = json!({"grouping": grouping, "positions": positions});
    if !stacks.is_empty() { result["stacks"] = Value::Object(stacks.clone()); }
    Ok(result)
}

fn owned(metadata: &fs::Metadata, directory: bool) -> Result<(), String> {
    if metadata.uid() != unsafe { libc::getuid() } || metadata.permissions().mode() & 0o077 != 0 || (if directory { !metadata.is_dir() } else { !metadata.is_file() }) {
        return Err("Desktop arrangement requires owned private storage".into());
    }
    Ok(())
}

fn state_root() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    base.join("lumen-desktop/desktop-layouts")
}

fn handle(request: &Value) -> Result<Value, String> {
    let object = request.as_object().ok_or("Invalid desktop arrangement request")?;
    if object.keys().any(|key| !matches!(key.as_str(), "action" | "directory" | "items" | "layout")) { return Err("Unknown desktop arrangement field".into()); }
    let action = object.get("action").and_then(Value::as_str).filter(|action| matches!(*action, "load" | "save" | "reset")).ok_or("Invalid desktop arrangement request")?;
    let directory = object.get("directory").and_then(Value::as_str).filter(|path| path.starts_with('/') && !path.contains('\0')).ok_or("An absolute desktop directory is required")?;
    let items = object.get("items").and_then(Value::as_array).filter(|items| items.len() <= MAX_ITEMS).ok_or("Invalid desktop item identities")?;
    let mut listed = HashSet::new();
    for item in items {
        let key = item.as_str().filter(|key| identity(key)).ok_or("Invalid desktop item identities")?;
        if !listed.insert(key) { return Err("Invalid desktop item identities".into()); }
    }
    let folder = fs::metadata(directory).map_err(|error| error.to_string())?;
    if !folder.is_dir() { return Err("Desktop location is not a directory".into()); }
    let key = format!("{:x}", Sha256::digest(format!("{}:{}", folder.dev(), folder.ino()).as_bytes()));
    let root = state_root();
    fs::DirBuilder::new().recursive(true).mode(0o700).create(&root).map_err(|error| error.to_string())?;
    let root_info = fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
    owned(&root_info, true)?;
    let lock_path = root.join(format!("{key}.lock"));
    let lock = OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(lock_path).map_err(|error| error.to_string())?;
    owned(&lock.metadata().map_err(|error| error.to_string())?, false)?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 { return Err(std::io::Error::last_os_error().to_string()); }
    let path = root.join(format!("{key}.json"));
    let mut layout = json!({"grouping":"none","positions":{}});
    match action {
        "save" => layout = validate(object.get("layout").ok_or("Invalid desktop arrangement")?)?,
        "load" => {
            let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(&path);
            if let Ok(file) = file {
                let info = file.metadata().map_err(|error| error.to_string())?;
                owned(&info, false)?;
                if info.len() > 2_000_000 { return Err("Desktop arrangement exceeds its size limit".into()); }
                let mut bytes = Vec::new();
                file.take(2_000_001).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
                layout = validate(&serde_json::from_slice::<Value>(&bytes).map_err(|error| error.to_string())?)?;
            } else if file.unwrap_err().kind() != std::io::ErrorKind::NotFound { return Err("Desktop arrangement could not be read safely".into()); }
        }
        _ => {}
    }
    if let Some(positions) = layout.get_mut("positions").and_then(Value::as_object_mut) { positions.retain(|key, _| listed.contains(key.as_str())); }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|error| error.to_string())?.as_nanos();
    let temporary = root.join(format!(".layout-{}-{nonce}", std::process::id()));
    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut file, &layout).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
        File::open(&root).and_then(|directory| directory.sync_all()).map_err(|error| error.to_string())?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    write_result?;
    Ok(json!({"layout":layout}))
}

fn main() {
    let result = (|| -> Result<Value, String> {
        let mut bytes = Vec::new();
        std::io::stdin().lock().take(2_000_001).read_until(b'\n', &mut bytes).map_err(|error| error.to_string())?;
        if bytes.len() > 2_000_000 { return Err("Desktop arrangement request is too large".into()); }
        handle(&serde_json::from_slice(&bytes).map_err(|error| error.to_string())?)
    })();
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => { println!("{}", json!({"error":error})); std::process::exit(1); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_validation() {
        assert!(validate(&json!({"grouping":"kind","positions":{"1:2":[0.2,0.8]},"stacks":{"stack:kind:Documents":[0.5,0.4]}})).is_ok());
        assert!(validate(&json!({"grouping":"kind","positions":{},"stacks":{"stack:kind:../outside":[0,0]}})).is_err());
        assert!(validate(&json!({"grouping":"none","positions":{"1:2":[2,0]}})).is_err());
    }
}
