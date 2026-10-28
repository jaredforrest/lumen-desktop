//! Private JSON-lines provider for the Network Locations QML flow.
use std::io::{self, BufRead, Write};

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    let Some(start) = line.find(&format!("\"{key}\":")) else { return ""; };
    let value = &line[start + key.len() + 3..];
    if let Some(quoted) = value.strip_prefix('"') {
        quoted.split('"').next().unwrap_or_default()
    } else {
        value.split(|character: char| !character.is_ascii_digit()).next().unwrap_or_default()
    }
}

fn emit(line: &str) -> io::Result<()> {
    println!("{line}");
    io::stdout().flush()
}

fn main() -> io::Result<()> {
    let mut connection = String::new();
    let id = "1".repeat(32);
    let snapshot = |event: &str, connection: &str| format!(
        "{{\"event\":\"{event}\",\"connections\":{connection},\"protocols\":[\"sftp\"],\"available\":true,\"reason\":\"\"}}"
    );
    emit(&snapshot("ready", "[]"))?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let request = field(&line, "id");
        let action = field(&line, "action");
        match action {
            "status" => emit(&format!("{{\"id\":{request},\"action\":\"status\",\"connections\":{connection},\"protocols\":[\"sftp\"],\"available\":true,\"reason\":\"\"}}", connection = if connection.is_empty() { "[]" } else { &connection }))?,
            "connect" => {
                let uri = field(&line, "uri");
                let state = if uri.ends_with("ask") { "authenticating" } else { "connected" };
                connection = format!("[{{\"id\":\"{id}\",\"name\":\"Fixture\",\"uri\":\"{uri}\",\"state\":\"{state}\",\"path\":\"\",\"canDisconnect\":true,\"error\":\"\"}}]");
                emit(&snapshot("state", &connection))?;
                if state == "connected" {
                    emit(&format!("{{\"id\":{request},\"action\":\"connect\",\"connected\":true,\"connectionId\":\"{id}\"}}"))?;
                    emit(&snapshot("state", &connection))?;
                }
            }
            "browse" => {
                let uri = field(&line, "uri");
                emit(&format!("{{\"id\":{request},\"action\":\"browse\",\"connectionId\":\"{id}\",\"rows\":[{{\"name\":\"Folder\",\"uri\":\"sftp://fixture/root/Folder\",\"directory\":true,\"path\":\"\"}},{{\"name\":\"document.txt\",\"uri\":\"sftp://fixture/root/document.txt\",\"directory\":false,\"path\":\"\"}}],\"uri\":\"{uri}\",\"parent\":\"\",\"path\":\"\",\"limited\":false}}"))?;
            }
            "cancel" => {
                let target = field(&line, "target");
                if !connection.is_empty() { connection = connection.replace("\"authenticating\"", "\"disconnected\""); }
                emit(&format!("{{\"id\":{target},\"action\":\"connect\",\"cancelled\":true,\"error\":\"Cancelled\"}}"))?;
                emit(&snapshot("state", if connection.is_empty() { "[]" } else { &connection }))?;
                emit(&format!("{{\"id\":{request},\"action\":\"cancel\"}}"))?;
            }
            _ => {}
        }
    }
    Ok(())
}
