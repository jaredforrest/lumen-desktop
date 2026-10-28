//! Explicit desktop consent for an incoming nearby transfer.
use serde_json::Value;
use std::env;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn label(value: &Value) -> String {
    value.as_str().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ")
        .chars().filter(|ch| !ch.is_control()).take(80).collect()
}
fn summary(request: &Value) -> String {
    let sender = match label(&request["sender"]) { value if !value.is_empty() => value, _ => "Nearby device".into() };
    let files = request["files"].as_array();
    let mut names: Vec<String> = files.into_iter().flat_map(|rows| rows.iter().take(3))
        .filter(|row| row.is_object()).map(|row| label(&row["name"]))
        .filter(|name| !name.is_empty()).collect();
    if let Some(rows) = files { if rows.len() > 3 { names.push(format!("and {} more", rows.len()-3)); } }
    if names.is_empty() {
        let items = request["items"].as_array();
        names = items.into_iter().flat_map(|rows| rows.iter().take(3)).map(label)
            .filter(|name| !name.is_empty()).collect();
        if let Some(rows) = items { if rows.len() > 3 { names.push(format!("and {} more", rows.len()-3)); } }
    }
    if names.is_empty() { names.push("a shared item".into()); }
    format!("{sender} wants to send {}", names.join(", "))
}
fn escape(value: &str) -> String {
    let mut result = String::new();
    for ch in value.chars() {
        result.push_str(match ch { '&' => "&amp;", '<' => "&lt;", '>' => "&gt;", '"' => "&quot;",
            '\'' => "&#x27;", _ => { result.push(ch); continue; } });
    }
    result
}
fn decide(request: &Value, rofi: &str) -> bool {
    if !request.is_object() { return false; }
    let Ok(mut child) = Command::new("timeout").args(["40s", rofi, "-no-config", "-dmenu", "-no-custom", "-format", "s",
        "-p", "AirDrop", "-mesg", &escape(&summary(request))])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return false; };
    if child.stdin.take().is_none_or(|mut input| input.write_all(b"Decline\nAccept\n").is_err()) {
        let _ = child.kill(); let _ = child.wait(); return false;
    }
    let Ok(result) = child.wait_with_output() else { return false; };
    result.status.success() && String::from_utf8_lossy(&result.stdout).trim() == "Accept"
}
fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() { std::process::exit(1); }
    let Ok(request) = serde_json::from_str::<Value>(&input) else { std::process::exit(1); };
    let rofi = env::var("LUMEN_AIRDROP_ROFI").unwrap_or_else(|_| "rofi".into());
    if decide(&request, &rofi) { println!("accept"); } else { std::process::exit(1); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn summary_escapes_untrusted_labels() {
        assert_eq!(escape(&summary(&json!({"sender":"<Guest>","files":[{"name":"a & b"}]}))),
            "&lt;Guest&gt; wants to send a &amp; b");
        assert_eq!(summary(&json!({})), "Nearby device wants to send a shared item");
    }
}
