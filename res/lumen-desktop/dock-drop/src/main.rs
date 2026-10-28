//! Validate every local file and declared MIME type before launching a Dock drop.
use gio::prelude::*;
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf, process::ExitCode};

fn local_file(uri: &str) -> Result<PathBuf, String> {
    if uri.len() > 32_768 || uri.as_bytes().contains(&0) { return Err("Invalid file URL".into()); }
    let rest = uri.strip_prefix("file://").ok_or("Only local file URLs are supported")?;
    let path = if let Some(path) = rest.strip_prefix("localhost/") {
        format!("/{path}")
    } else if rest.starts_with('/') { rest.to_owned() }
    else { return Err("Only local file URLs are supported".into()); };
    if path.contains(['?', '#']) { return Err("Only local file URLs are supported".into()); }
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() { return Err("Invalid file URL encoding".into()); }
            let hex = |b: u8| (b as char).to_digit(16).map(|v| v as u8);
            let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) else {
                return Err("Invalid file URL encoding".into());
            };
            decoded.push((hi << 4) | lo);
            i += 3;
        } else { decoded.push(bytes[i]); i += 1; }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| "Invalid file URL encoding")?;
    if decoded.as_bytes().contains(&0) || !decoded.starts_with('/') {
        return Err("Invalid local file path".into());
    }
    let path = PathBuf::from(decoded);
    if !path.is_file() || fs::File::open(&path).is_err() {
        return Err("A dropped file is unavailable or unreadable".into());
    }
    Ok(path)
}

fn desktop_id(value: &str) -> Result<String, String> {
    if value.is_empty() || value.len() > 256 || !value.bytes().all(|b|
        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-')) {
        return Err("Invalid application identity".into());
    }
    Ok(if value.ends_with(".desktop") { value.into() } else { format!("{value}.desktop") })
}

fn open_drop(id: &str, uris: Value) -> Result<Value, String> {
    let id = desktop_id(id)?;
    let entries = uris.as_array().ok_or("Drop between one and 100 local files")?;
    if entries.is_empty() || entries.len() > 100 { return Err("Drop between one and 100 local files".into()); }
    let files: Vec<_> = entries.iter().map(|entry| {
        local_file(entry.as_str().ok_or("Invalid file URL")?)
    }).collect::<Result<_, _>>()?;
    let app = gio::DesktopAppInfo::new(&id).ok_or("This application does not accept file drops")?;
    if !app.supports_files() && !app.supports_uris() {
        return Err("This application does not accept file drops".into());
    }
    let types = app.supported_types();
    for file in &files {
        let info = gio::File::for_path(file).query_info("standard::content-type", gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE).map_err(|_| "A dropped file is unavailable or unreadable")?;
        let content = info.content_type().ok_or("This application does not declare support for every dropped file type")?;
        if !types.iter().any(|kind| gio::content_type_is_a(&content, kind)) {
            return Err("This application does not declare support for every dropped file type".into());
        }
    }
    let urls: Vec<_> = files.iter().map(|path| gio::File::for_path(path).uri()).collect();
    let refs: Vec<_> = urls.iter().map(|url| url.as_str()).collect();
    app.launch_uris(&refs, Some(&gio::AppLaunchContext::new()))
        .map_err(|_| "The application could not open this drop")?;
    Ok(json!({"ok":true,"count":files.len()}))
}

fn main() -> ExitCode {
    let mut args = env::args();
    let _ = args.next();
    let result = (|| {
        let id = args.next().ok_or("Invalid application identity")?;
        let input = args.next().ok_or("Drop between one and 100 local files")?;
        if args.next().is_some() { return Err("Drop between one and 100 local files".into()); }
        let uris = serde_json::from_str(&input).map_err(|_| "Invalid file URL")?;
        open_drop(&id, uris)
    })();
    match result {
        Ok(value) => { println!("{value}"); ExitCode::SUCCESS },
        Err(error) => { println!("{}", json!({"error":error})); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::{desktop_id, local_file};
    #[test]
    fn rejects_nonlocal_and_malformed() {
        assert!(desktop_id("fixture").unwrap() == "fixture.desktop");
        assert!(desktop_id("../../app").is_err());
        for uri in ["https://example.com/a", "file://server/path", "file:///tmp/%ZZ", "file:///tmp/%00", "file:///tmp/file#bad"] {
            assert!(local_file(uri).is_err());
        }
    }
}
