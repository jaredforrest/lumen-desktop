//! Interoperable file URI clipboard with local-only validation.
use serde_json::{Value, json};
use std::{collections::HashSet, env, io::{BufRead, BufReader}, os::unix::process::CommandExt,
    path::PathBuf, process::{Command, Stdio}, sync::mpsc, thread, time::Duration};

type ClipboardResult<T> = Result<T, String>;

fn hex(byte: u8) -> Option<u8> {
    match byte { b'0'..=b'9' => Some(byte - b'0'), b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10), _ => None }
}

fn decode(value: &str) -> ClipboardResult<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let upper = *bytes.get(index + 1).ok_or("Invalid file URI")?;
            let lower = *bytes.get(index + 2).ok_or("Invalid file URI")?;
            output.push(hex(upper).zip(hex(lower)).map(|(a,b)| a * 16 + b).ok_or("Invalid file URI")?);
            index += 3;
        } else { output.push(bytes[index]); index += 1; }
    }
    let path = String::from_utf8(output).map_err(|_| "Invalid file URI")?;
    if !path.starts_with('/') || path.contains('\0') { return Err("Invalid file URI".into()); }
    Ok(path)
}

fn uri_path(line: &str) -> ClipboardResult<String> {
    let scheme = line.get(..7).ok_or("Only local files can be pasted")?;
    if !scheme.eq_ignore_ascii_case("file://") || line.contains(['?', '#']) {
        return Err("Only local files can be pasted".into());
    }
    let rest = &line[7..];
    let path = if rest.starts_with('/') { rest } else if let Some(path) = rest.strip_prefix("localhost") {
        if !path.starts_with('/') { return Err("Only local files can be pasted".into()); }
        path
    } else { return Err("Only local files can be pasted".into()); };
    decode(path)
}

fn paths_from_uris(value: &str) -> ClipboardResult<Vec<String>> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0;
    for line in value.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') { continue; }
        total += 1;
        if total > 5000 { return Err("The clipboard does not contain a supported file selection".into()); }
        let path = uri_path(line)?;
        if seen.insert(path.clone()) { paths.push(path); }
    }
    if paths.is_empty() {
        return Err("The clipboard does not contain a supported file selection".into());
    }
    Ok(paths)
}

fn quote_path(path: &str) -> String {
    let mut output = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) { output.push(byte as char); }
        else { output.push_str(&format!("%{byte:02X}")); }
    }
    output
}

fn paste(args: &[&str]) -> ClipboardResult<String> {
    let output = Command::new("timeout").arg("3s").arg("wl-paste").args(args)
        .output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Err("File clipboard could not be read".into()); }
    String::from_utf8(output.stdout).map_err(|_| "Invalid file clipboard encoding".into())
}

fn read() -> ClipboardResult<Value> {
    let types = paste(&["--list-types"])?;
    let types: HashSet<&str> = types.lines().collect();
    let mut operation = "copy";
    let body = if types.contains("x-special/gnome-copied-files") {
        let value = paste(&["--no-newline", "--type", "x-special/gnome-copied-files"])?;
        let (header, body) = value.split_once('\n').unwrap_or((&value, ""));
        if !["cut", "copy"].contains(&header) { return Err("Invalid file clipboard operation".into()); }
        if header == "cut" { operation = "move"; }
        body.to_owned()
    } else if types.contains("text/uri-list") {
        let value = paste(&["--no-newline", "--type", "text/uri-list"])?;
        if types.contains("application/x-kde-cutselection") &&
            paste(&["--no-newline", "--type", "application/x-kde-cutselection"])?.trim() == "1" {
            operation = "move";
        }
        value
    } else { return Err("Copy files from a file manager before pasting".into()); };
    Ok(json!({"paths":paths_from_uris(&body)?,"operation":operation}))
}

fn write(paths: &[Value], operation: &str) -> ClipboardResult<Value> {
    if !["copy", "move"].contains(&operation) { return Err("Unknown clipboard operation".into()); }
    if paths.is_empty() || paths.len() > 5000 { return Err("Select files to copy".into()); }
    let mut uris = Vec::new();
    for item in paths {
        let value = item.as_str().ok_or("Invalid file path")?;
        let path = PathBuf::from(value);
        let path = if path.is_absolute() { path } else { env::current_dir().map_err(|error| error.to_string())?.join(path) };
        uris.push(quote_path(path.to_str().ok_or("Invalid file path")?));
    }
    let scoped = env::var("LUMEN_LAUNCH_SCOPES").ok().as_deref() == Some("1");
    let mut command = if scoped { Command::new("lumen-launch-app") } else { Command::new("lumen-file-clipboard") };
    if scoped { command.args(["--", "lumen-file-clipboard"]); }
    command.arg(operation).args(&uris).stdout(Stdio::piped()).stderr(Stdio::null());
    unsafe { command.pre_exec(|| { if libc::setsid() < 0 { return Err(std::io::Error::last_os_error()); } Ok(()) }); }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().ok_or("No clipboard handshake")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send((result, line));
    });
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok((Ok(_), line)) if line.trim() == "ready" => Ok(json!({"ok":true})),
        _ => { let _ = child.kill(); let _ = child.wait(); Err("The file clipboard could not be claimed".into()) }
    }
}

fn handle(request: &Value) -> ClipboardResult<Value> {
    match request["action"].as_str() {
        Some("read") => read(),
        Some("parse-uris") => Ok(json!({"paths":paths_from_uris(request["uris"].as_str().ok_or("Missing file URIs")?)?})),
        Some("write") => write(request["paths"].as_array().ok_or("Missing file paths")?,
            request["operation"].as_str().ok_or("Missing clipboard operation")?),
        _ => Err("Unknown clipboard action".into()),
    }
}

fn main() {
    unsafe { libc::umask(0o077); }
    let mut line = String::new();
    let result = std::io::stdin().lock().read_line(&mut line)
        .map_err(|error| error.to_string())
        .and_then(|_| serde_json::from_str::<Value>(&line).map_err(|error| error.to_string()))
        .and_then(|request| handle(&request));
    match result { Ok(value) => println!("{value}"), Err(error) => println!("{}", json!({"error":error})) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_local_file_uris_are_accepted() {
        assert_eq!(paths_from_uris("file:///tmp/a%20b\r\nfile:///tmp/%23c").unwrap(), ["/tmp/a b", "/tmp/#c"]);
        for bad in ["https://example.org/x", "file://remote/tmp/a", "file:///tmp/x?query", "file:///tmp/%00x", "file:///tmp/%FF"] {
            assert!(paths_from_uris(bad).is_err());
        }
        assert_eq!(paths_from_uris("#comment\nfile:///a\nfile:///a").unwrap(), ["/a"]);
    }
    #[test]
    fn uri_round_trip_preserves_literal_characters() {
        let path = "/tmp/a # ü $(not-command)";
        assert_eq!(uri_path(&quote_path(path)).unwrap(), path);
    }
}
