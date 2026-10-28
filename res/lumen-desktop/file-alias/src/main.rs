//! Open a declared local file alias without changing its target.
use std::{env, fs, path::Path, process::{Command, ExitCode}};

fn file_uri(path: &Path) -> Result<String, &'static str> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if bytes.is_empty() || bytes.len() > 4096 || bytes[0] != b'/' || bytes.contains(&0) {
        return Err("A file alias requires an absolute local path");
    }
    if !path.is_file() || fs::File::open(path).is_err() {
        return Err("This file alias target is unavailable or unreadable");
    }
    let mut uri = String::from("file://");
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(uri)
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let gio = if args.next().as_deref() == Some(std::ffi::OsStr::new("--gio")) {
        args.next().ok_or("Missing gio command")?
    } else {
        args = env::args_os().skip(1);
        "gio".into()
    };
    let path = args.next().ok_or("A file alias requires an absolute local path")?;
    if args.next().is_some() { return Err("Only one file alias is supported".into()); }
    let uri = file_uri(Path::new(&path)).map_err(str::to_owned)?;
    let result = Command::new(gio).arg("open").arg(uri).status()
        .map_err(|_| "No application could open this file")?;
    if !result.success() { return Err("No application could open this file".into()); }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => { println!("{{\"opened\":true}}"); ExitCode::SUCCESS }
        Err(error) => {
            println!("{}", serde_json::json!({"error":error}));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::file_uri;
    use std::path::Path;
    #[test]
    fn rejects_unusable_paths() {
        assert!(file_uri(Path::new("relative")).is_err());
        assert!(file_uri(Path::new("/definitely/missing/file-alias")).is_err());
    }
}
