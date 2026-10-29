use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

const ON: &str = "Turn On for 10 Minutes";
const REFRESH: &str = "Refresh 10 Minutes";
const OFF: &str = "Turn Off";

fn choices(status: &Value) -> (Vec<&'static str>, String) {
    if status.get("ready").and_then(Value::as_bool).unwrap_or(false) {
        let remaining = status.get("remaining").and_then(Value::as_i64).unwrap_or(0).max(0);
        (vec![OFF, REFRESH], format!("Visible for about {} more minute(s)", remaining / 60 + 1))
    } else {
        (vec![ON], "AirDrop receiving is off".into())
    }
}

fn message(kind: &str, body: &str) {
    let mut child = match Command::new("zenity").arg(format!("--{kind}")).arg("--title=AirDrop Receiving")
        .arg(format!("--text={body}")).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(child) => child,
        Err(_) => return,
    };
    for _ in 0..300 {
        if child.try_wait().ok().flatten().is_some() { return; }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn run() -> Result<i32, String> {
    let status_output = Command::new("timeout").args(["15", "iphone-airdrop", "status"])
        .output().map_err(|error| error.to_string())?;
    if !status_output.status.success() { return Err(String::from_utf8_lossy(&status_output.stderr).trim().to_string()); }
    let status: Value = serde_json::from_slice(&status_output.stdout).map_err(|error| error.to_string())?;
    let (options, description) = choices(&status);
    let mut picker = Command::new("rofi").args(["-no-config", "-dmenu", "-no-custom", "-p", "AirDrop Receiving", "-mesg", &description])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())?;
    if let Some(mut input) = picker.stdin.take() {
        let _ = writeln!(input, "{}", options.join("\n"));
    }
    let selection = picker.wait_with_output().map_err(|error| error.to_string())?;
    let selected = String::from_utf8_lossy(&selection.stdout).trim().to_owned();
    if !selection.status.success() || !options.contains(&selected.as_str()) { return Ok(0); }
    let action = if selected == OFF { "off" } else { "on" };
    let mut progress = Command::new("zenity").args(["--progress", "--pulsate", "--no-cancel", "--title=AirDrop Receiving", "--text=Changing AirDrop receiving…", "--percentage=5"])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())?;
    let result = Command::new("timeout").args(["45", "iphone-airdrop", action]).output();
    let _ = progress.kill();
    let _ = progress.wait();
    let result = result.map_err(|error| error.to_string())?;
    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr).trim().to_owned();
        message("error", if detail.is_empty() { "AirDrop receiving could not change state." } else { &detail });
        return Ok(result.status.code().unwrap_or(1));
    }
    message("info", if action == "off" { "AirDrop receiving is off." } else { "AirDrop receiving is on for up to 10 minutes. Normal Wi‑Fi resumes when it ends." });
    Ok(0)
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => { message("error", &format!("AirDrop receiving could not start: {error}")); 1 }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn off_offers_only_on() { let (options, text) = choices(&serde_json::json!({"ready":false})); assert_eq!(options, vec![ON]); assert!(text.contains("off")); }
    #[test]
    fn visible_offers_off_and_refresh() { let (options, text) = choices(&serde_json::json!({"ready":true,"remaining":421})); assert_eq!(options, vec![OFF, REFRESH]); assert!(text.contains("8 more minute")); }
}
