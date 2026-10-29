use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

fn hex_digit(value: u8) -> Option<u8> {
    match value { b'0'..=b'9' => Some(value-b'0'), b'a'..=b'f' => Some(value-b'a'+10), b'A'..=b'F' => Some(value-b'A'+10), _ => None }
}
fn decode(uri: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let Some(part) = uri.strip_prefix("file://") else {
        let path = PathBuf::from(uri);
        if !path.is_absolute() { return Err("Offer path is not absolute".into()); }
        return Ok(path);
    };
    let part = {
        let value = part;
        if !value.starts_with('/') { return Err("Remote URI is not a local file".into()); }
        value
    };
    let mut bytes = Vec::new();
    let mut input = part.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let hi = hex_digit(input.next().ok_or("Truncated percent escape")?).ok_or("Invalid percent escape")?;
            let lo = hex_digit(input.next().ok_or("Truncated percent escape")?).ok_or("Invalid percent escape")?;
            bytes.push(hi*16+lo);
        } else { bytes.push(byte); }
    }
    let text = String::from_utf8(bytes)?;
    let path = PathBuf::from(text);
    if !path.is_absolute() { return Err("Offer path is not absolute".into()); }
    Ok(path)
}
fn uri(path: &str) -> String {
    let mut result = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte,b'/'|b'-'|b'.'|b'_'|b'~') { result.push(byte as char); }
        else { result.push_str(&format!("%{byte:02X}")); }
    }
    result
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("Missing output log")?);
    let mut rows = Vec::new();
    for argument in args {
        let path = decode(&argument)?;
        let text = path.to_str().ok_or("Offer path is not UTF-8")?;
        rows.push(json!({"argument":argument,"uri":uri(text),"path":text,
            "sha256":format!("{:x}",Sha256::digest(fs::read(&path)?))}));
    }
    let mut file = OpenOptions::new().create(true).append(true).open(output)?;
    writeln!(file,"{}",json!(rows))?;
    Ok(())
}
