//! Patch the pinned Electron archive, refusing layouts we do not recognize.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};

const BOOTSTRAP: &[u8] = b"if(process.env.CODEX_MENU_ADAPTER_ROOT)require(process.env.CODEX_MENU_ADAPTER_ROOT+'/menu-adapter.cjs');\n";

fn word(raw: &[u8], start: usize) -> Result<usize, String> {
    let bytes: [u8; 4] = raw.get(start..start + 4).ok_or("Truncated ASAR header")?
        .try_into().map_err(|_| "Invalid ASAR header")?;
    Ok(u32::from_le_bytes(bytes) as usize)
}

fn entries(tree: &Value, prefix: &str, output: &mut Vec<String>) -> Result<(), String> {
    let object = tree.as_object().ok_or("Invalid ASAR file tree")?;
    for (name, node) in object {
        let path = format!("{prefix}{name}");
        if let Some(children) = node.get("files") {
            entries(children, &format!("{path}/"), output)?;
        } else if node.get("offset").is_some() {
            output.push(path);
        }
    }
    Ok(())
}

fn count(data: &[u8], needle: &[u8]) -> usize {
    let mut total = 0;
    let mut pos = 0;
    while let Some(found) = data[pos..].windows(needle.len()).position(|part| part == needle) {
        total += 1;
        pos += found + needle.len();
    }
    total
}

fn replace(data: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(found) = data[pos..].windows(needle.len()).position(|part| part == needle) {
        out.extend_from_slice(&data[pos..pos + found]);
        out.extend_from_slice(replacement);
        pos += found + needle.len();
    }
    out.extend_from_slice(&data[pos..]);
    out
}

fn digest(data: &[u8]) -> String { format!("{:x}", Sha256::digest(data)) }

fn patch(raw: &[u8], native_frames: bool) -> Result<Vec<u8>, String> {
    let header_len = word(raw, 12)?;
    let body_start = 8usize.checked_add(word(raw, 4)?).ok_or("Invalid ASAR size")?;
    let header_end = 16usize.checked_add(header_len).ok_or("Invalid ASAR header size")?;
    let mut header: Value = serde_json::from_slice(raw.get(16..header_end).ok_or("Truncated ASAR header")?)
        .map_err(|error| error.to_string())?;
    let mut paths = Vec::new();
    entries(header.get("files").ok_or("Missing ASAR file tree")?, "", &mut paths)?;
    let mut body = Vec::new();
    let mut bootstrap_count = 0;
    let mut frame_count = 0;
    for path in paths {
        let mut node = &mut header["files"];
        for part in path.split('/') {
            node = if node.get(part).is_some() { &mut node[part] } else { &mut node["files"][part] };
        }
        let offset: usize = node["offset"].as_str().ok_or("Invalid ASAR offset")?
            .parse().map_err(|_| "Invalid ASAR offset")?;
        let size = node["size"].as_u64().ok_or("Invalid ASAR file size")? as usize;
        let start = body_start.checked_add(offset).ok_or("Invalid ASAR offset")?;
        let end = start.checked_add(size).ok_or("Invalid ASAR file size")?;
        let original = raw.get(start..end).ok_or("Truncated ASAR body")?;
        let mut data = original.to_vec();
        if path == ".vite/build/early-bootstrap.js" {
            bootstrap_count += 1;
            data.splice(0..0, BOOTSTRAP.iter().copied());
        }
        if native_frames && path.starts_with(".vite/build/main-") && path.ends_with(".js") {
            let overlay = b"titleBarStyle:`hidden`,titleBarOverlay:j9(r)";
            let zoom = b"n.setTitleBarOverlay(j9(t))";
            let sync = b"if(process.platform!==`win32`&&process.platform!==`linux`||t!==`primary`";
            if count(&data, overlay) > 0 {
                if count(&data, overlay) != 2 || count(&data, zoom) != 1 || count(&data, sync) != 1 {
                    return Err("Unsupported Codex window-overlay implementation".into());
                }
                frame_count += 1;
                data = replace(&data, overlay, b"titleBarStyle:`default`,titleBarOverlay:!1");
                data = replace(&data, zoom, b"(process.platform!==`linux`&&n.setTitleBarOverlay(j9(t)))");
                data = replace(&data, sync, b"if(process.platform!==`win32`||t!==`primary`");
            }
        }
        if data != original && node.get("integrity").is_some() {
            let block = node["integrity"]["blockSize"].as_u64().unwrap_or(4_194_304) as usize;
            if block == 0 { return Err("Invalid ASAR integrity block size".into()); }
            node["integrity"] = json!({
                "algorithm": "SHA256", "hash": digest(&data), "blockSize": block,
                "blocks": data.chunks(block).map(digest).collect::<Vec<_>>()
            });
        }
        node["offset"] = Value::String(body.len().to_string());
        node["size"] = json!(data.len());
        body.extend_from_slice(&data);
    }
    if bootstrap_count != 1 { return Err("Expected exactly one supported bootstrap".into()); }
    if native_frames && frame_count != 1 { return Err("Expected exactly one supported native window-frame implementation".into()); }
    let serialized = serde_json::to_vec(&header).map_err(|error| error.to_string())?;
    let mut payload = Vec::new();
    payload.extend_from_slice(&(serialized.len() as u32).to_le_bytes());
    payload.extend_from_slice(&serialized);
    while payload.len() % 4 != 0 { payload.push(0); }
    let mut output = Vec::new();
    output.extend_from_slice(&4u32.to_le_bytes());
    output.extend_from_slice(&((payload.len() + 4) as u32).to_le_bytes());
    output.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    output.extend_from_slice(&payload);
    output.extend_from_slice(&body);
    Ok(output)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args().skip(1);
    let path = arguments.next().ok_or("Usage: lumen-asar-patch ARCHIVE [--native-frames]")?;
    let native_frames = match arguments.next().as_deref() {
        None => false, Some("--native-frames") => true, _ => return Err("Unsupported option".into()),
    };
    if arguments.next().is_some() { return Err("Too many options".into()); }
    let raw = fs::read(&path)?;
    let output = patch(&raw, native_frames)?;
    fs::write(Path::new(&path), output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_counts_are_exact() {
        assert_eq!(count(b"ababa", b"aba"), 1);
        assert_eq!(replace(b"foo foo", b"foo", b"bar"), b"bar bar");
    }
    #[test]
    fn refuses_unrecognized_archive() {
        assert!(patch(b"bad", false).is_err());
    }
    #[test]
    fn rewrites_bootstrap_and_integrity() {
        let source = b"original bootstrap";
        let header = json!({"files": {".vite": {"files": {"build": {"files": {
            "early-bootstrap.js": {"offset": "0", "size": source.len(), "integrity": {"blockSize": 4}}
        }}}}}});
        let serialized = serde_json::to_vec(&header).unwrap();
        let mut payload = Vec::new();
        payload.extend_from_slice(&(serialized.len() as u32).to_le_bytes());
        payload.extend_from_slice(&serialized);
        while payload.len() % 4 != 0 { payload.push(0); }
        let mut archive = Vec::new();
        archive.extend_from_slice(&4u32.to_le_bytes());
        archive.extend_from_slice(&((payload.len() + 4) as u32).to_le_bytes());
        archive.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        archive.extend_from_slice(&payload);
        archive.extend_from_slice(source);
        let result = patch(&archive, false).unwrap();
        let output_header: Value = serde_json::from_slice(&result[16..16 + word(&result, 12).unwrap()]).unwrap();
        let node = &output_header["files"][".vite"]["files"]["build"]["files"]["early-bootstrap.js"];
        let data = &result[8 + word(&result, 4).unwrap()..];
        assert!(data.starts_with(BOOTSTRAP));
        assert!(data.ends_with(source));
        assert_eq!(node["size"], data.len());
        assert_eq!(node["integrity"]["hash"], digest(data));
    }
}
