use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use url::Url;

const SUPPORTED: &[&str] = &["RDP", "VNC", "SSH", "SFTP", "SPICE", "WWW"];

fn roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    vec![
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share")).join("remmina"),
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("remmina"),
    ]
}

fn profile(path: &Path) -> Result<Option<Value>, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut remmina = false;
    let mut keys: HashMap<String, String> = HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() { continue; }
        if line.starts_with('[') && line.ends_with(']') { remmina = &line[1..line.len()-1] == "remmina"; continue; }
        if !remmina { continue; }
        let Some((key, value)) = line.split_once('=') else { return Err("Invalid Remmina profile".into()); };
        let key = key.trim().to_ascii_lowercase();
        if keys.insert(key, value.trim().to_owned()).is_some() { return Err("Duplicate Remmina field".into()); }
    }
    if !remmina && keys.is_empty() { return Ok(None); }
    let protocol = keys.get("protocol").map(|value| value.to_ascii_uppercase()).unwrap_or_default();
    if !SUPPORTED.contains(&protocol.as_str()) { return Ok(None); }
    let identifier = format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()));
    let server = keys.get("server").map(String::as_str).unwrap_or("");
    let host = if server.len() <= 253 && !server.contains("://") && !server.contains('@') && server.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'[' | b']' | b'-')) { server } else { "" };
    let name = keys.get("name").map(|value| value.trim()).filter(|value| !value.is_empty()).map(str::to_owned).unwrap_or_else(|| format!("Saved {protocol} connection"));
    let printer = keys.get("shareprinter").is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true"));
    Ok(Some(json!({"id": &identifier[..24], "name": name.chars().take(160).collect::<String>(), "protocol":protocol, "host":host, "printer":printer})))
}

fn profiles() -> (Vec<Value>, HashMap<String, PathBuf>) {
    let mut rows = Vec::new();
    let mut files = HashMap::new();
    for directory in roots() {
        let Ok(directory) = directory.canonicalize() else { continue; };
        if !directory.is_dir() { continue; }
        let Ok(entries) = fs::read_dir(&directory) else { continue; };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|extension| extension == "remmina")).collect();
        paths.sort();
        for path in paths.into_iter().take(500) {
            let Ok(resolved) = path.canonicalize() else { continue; };
            if !resolved.starts_with(&directory) || !resolved.is_file() || fs::metadata(&resolved).map_or(true, |metadata| metadata.len() > 1024*1024) { continue; }
            let Ok(Some(row)) = profile(&resolved) else { continue; };
            let id = row["id"].as_str().unwrap_or("").to_owned();
            if files.insert(id.clone(), resolved).is_none() { rows.push(row); }
        }
    }
    rows.sort_by_key(|row| row["name"].as_str().unwrap_or("").to_lowercase());
    (rows, files)
}

fn quick_uri(address: &str, protocol: &str) -> Result<String, String> {
    if !matches!(protocol, "RDP" | "VNC") { return Err("Choose RDP or VNC".into()); }
    if address.is_empty() || address.len() > 253 || address.chars().any(char::is_whitespace) { return Err("Enter a hostname or IP address, with an optional port".into()); }
    let candidate = format!("{}://{address}", protocol.to_ascii_lowercase());
    let parsed = Url::parse(&candidate).map_err(|_| "Enter a valid hostname or port".to_owned())?;
    if parsed.host_str().is_none() || !parsed.username().is_empty() || parsed.password().is_some() || !parsed.path().is_empty() || parsed.query().is_some() || parsed.fragment().is_some() { return Err("Enter only a host and optional port; credentials are requested by the connection app".into()); }
    let host = parsed.host_str().ok_or("Enter a valid hostname")?;
    let host = if host.starts_with('[') { host.to_owned() } else {
        if !host.bytes().next().is_some_and(|byte| byte.is_ascii_alphanumeric()) || !host.bytes().last().is_some_and(|byte| byte.is_ascii_alphanumeric()) || !host.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')) { return Err("Enter a valid hostname".into()); }
        host.to_owned()
    };
    Ok(format!("{}://{host}{}", protocol.to_ascii_lowercase(), parsed.port().map(|port| format!(":{port}")).unwrap_or_default()))
}

fn handle(request: &Value) -> Result<Value, String> {
    let action = request.get("action").and_then(Value::as_str).unwrap_or("list");
    let (rows, files) = profiles();
    if action == "list" { return Ok(json!({"connections":rows})); }
    let mut command = Command::new("remmina");
    command.args(["--disable-news", "--disable-stats"]);
    match action {
        "connect" | "edit" => {
            let id = request.get("id").and_then(Value::as_str).unwrap_or("");
            let path = files.get(id).ok_or("This saved connection is no longer available")?;
            command.arg(if action == "connect" { "--connect" } else { "--edit" }).arg(path);
        }
        "quick" => {
            let address = request.get("address").and_then(Value::as_str).unwrap_or("").trim();
            let protocol = request.get("protocol").and_then(Value::as_str).unwrap_or("RDP");
            command.arg("--connect").arg(quick_uri(address, protocol)?);
        }
        "new" => { command.arg("--new"); }
        "advanced" => {}
        _ => return Err("Unknown connection action".into()),
    }
    unsafe { command.pre_exec(|| { if libc::setsid() == -1 { return Err(std::io::Error::last_os_error()); } Ok(()) }); }
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())?;
    Ok(json!({"started":true}))
}

fn main() {
    let result = (|| -> Result<Value, String> {
        let mut input = Vec::new();
        std::io::stdin().lock().take(1024*1024).read_until(b'\n', &mut input).map_err(|error| error.to_string())?;
        let request: Value = serde_json::from_slice(&input).map_err(|error| error.to_string())?;
        handle(&request)
    })();
    match result { Ok(value) => println!("{value}"), Err(error) => { println!("{}", json!({"error":error})); std::process::exit(1); } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_uris_are_exact() {
        assert_eq!(quick_uri("example.test:3389", "RDP").unwrap(), "rdp://example.test:3389");
        assert_eq!(quick_uri("[::1]:5900", "VNC").unwrap(), "vnc://[::1]:5900");
        for bad in ["user:password@example.test", "host/path", "host/", "host?key=value", "host;bad", "-option", "host:99999", "host\nother"] { assert!(quick_uri(bad, "RDP").is_err(), "{bad}"); }
    }
}
