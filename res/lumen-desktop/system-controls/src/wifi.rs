//! NetworkManager Wi-Fi control. Passwords travel over stdin, never argv or logs.
use super::{Result, Value, json};
use std::{collections::HashSet, io::{Read, BufRead, BufReader, Write}, process::{Command, Stdio}};

fn nm(args: &[&str]) -> Result<String> {
    let out = Command::new("timeout").args(["8", "nmcli", "--colors", "no", "--terse", "--escape", "yes"])
        .args(args).env("LC_ALL", "C").output().map_err(|_| "NetworkManager is unavailable")?;
    if !out.status.success() { return Err("NetworkManager could not read Wi-Fi status".into()); }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
}
// nmcli escapes both ':' and '\\', including inside arbitrary SSIDs.
fn fields(line: &str) -> Vec<String> {
    let mut result = vec![String::new()];
    let mut escaped = false;
    for c in line.chars() {
        if escaped { result.last_mut().unwrap().push(c); escaped = false; }
        else if c == '\\' { escaped = true; }
        else if c == ':' { result.push(String::new()); }
        else { result.last_mut().unwrap().push(c); }
    }
    if escaped { result.last_mut().unwrap().push('\\'); }
    result
}
fn interface() -> Result<(String, String)> {
    let text = nm(&["--fields", "DEVICE,TYPE,STATE", "device", "status"])?;
    let mut rows: Vec<_> = text.lines().map(fields).filter(|f| f.len() == 3 && f[1] == "wifi").collect();
    rows.sort_by_key(|f| if f[2] == "connected" { 0 } else { 1 });
    let row = rows.first().ok_or("No Wi-Fi adapter was found")?;
    Ok((row[0].clone(), row[2].clone()))
}
fn valid_bssid(id: &str) -> bool {
    id.len() == 17 && id.split(':').count() == 6 && id.split(':').all(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn networks(text: &str) -> Vec<Value> {
    let mut rows: Vec<_> = text.lines().map(fields).filter(|f| f.len() == 5 && !f[1].is_empty() && valid_bssid(&f[2])).collect();
    rows.sort_by_key(|f| (f[0] != "*", std::cmp::Reverse(f[3].parse::<u32>().unwrap_or(0))));
    let mut seen = HashSet::new();
    rows.into_iter().filter_map(|f| {
        if !seen.insert((f[1].clone(), f[4].clone())) { return None; }
        Some(json!({"id":f[2],"name":f[1],"connected":f[0]=="*","signal":f[3].parse::<u32>().unwrap_or(0).min(100),
            "security":f[4],"secured":!f[4].is_empty() && f[4]!="--","enterprise":f[4].contains("802.1X") || f[4].contains("EAP")}))
    }).collect()
}
pub fn status() -> Result<Value> {
    let (interface, state) = interface()?;
    let enabled = nm(&["radio", "wifi"])? == "enabled";
    let text = nm(&["--fields", "IN-USE,SSID,BSSID,SIGNAL,SECURITY", "device", "wifi", "list", "ifname", &interface, "--rescan", "no"])?;
    let networks = networks(&text);
    let name = networks.iter().find(|n| n["connected"] == true).map(|n| n["name"].clone()).unwrap_or(json!(""));
    let phase = if !enabled { "disabled" } else { match state.as_str() {
        "connected" => "connected", "disconnected" | "failed" => "disconnected", "unavailable" => "unavailable",
        s if s.starts_with("connecting") => "associating", s if s.starts_with("disconnecting") => "disconnecting", _ => "unknown",
    }};
    Ok(json!({"available":true,"controlAvailable":true,"backend":"network-manager","enabled":enabled,"interface":interface,
        "state":if phase=="connected"{"COMPLETED"}else{"DISCONNECTED"},"phase":phase,"name":name,"networks":networks,"connectivity":"unknown"}))
}
pub fn available() -> bool { nm(&["--fields", "RUNNING", "general"]).is_ok_and(|s| s == "running") }
fn action(args: &[&str], password: Option<&str>) -> Result<Value> {
    let mut child = Command::new("timeout").args(["40", "nmcli", "--colors", "no", "--wait", "30"])
        .args(args).env("LC_ALL", "C").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|_| "Unable to start Wi-Fi control")?;
    if let Some(password) = password {
        if let Some(mut input) = child.stdin.take() { let _ = writeln!(input, "{password}"); }
    } else { drop(child.stdin.take()); }
    let status = child.wait().map_err(|_| "Wi-Fi request did not finish")?;
    if !status.success() { return Err("Could not connect or complete the request. Check the password, signal and network permissions, then try again.".into()); }
    Ok(json!({"ok":true}))
}
pub fn control(action_name: &str, id: Option<&str>) -> Result<Value> {
    let (interface, _) = interface()?;
    match action_name {
        "wifi-on" | "wifi-off" => action(&["radio", "wifi", if action_name=="wifi-on"{"on"}else{"off"}], None),
        "wifi-scan" => action(&["device", "wifi", "rescan", "ifname", &interface], None),
        "wifi-disconnect" => action(&["device", "disconnect", &interface], None),
        "wifi-reconnect" => action(&["device", "connect", &interface], None),
        "wifi-connect" => {
            let id = id.filter(|id| valid_bssid(id)).ok_or("Select an available network")?;
            let current = status()?;
            let network = current["networks"].as_array().and_then(|rows| rows.iter().find(|n| n["id"]==id)).ok_or("This network is no longer in range. Refresh and try again.")?;
            if network["enterprise"]==true { return Err("This network needs enterprise sign-in. Open Advanced Wi-Fi Settings to configure it.".into()); }
            // A bounded, single JSON line from the UI; it is never included in errors.
            let mut input = String::new();
            BufReader::new(std::io::stdin().take(4097)).read_line(&mut input).map_err(|_| "Unable to read password")?;
            if input.len()>4096 { return Err("Password is too long".into()); }
            let request:Value = serde_json::from_str(&input).unwrap_or(json!({}));
            let password = request["password"].as_str().unwrap_or("");
            if password.contains(['\n','\r','\0']) { return Err("Invalid Wi-Fi password".into()); }
            if password.is_empty() {
                action(&["device", "wifi", "connect", id, "ifname", &interface], None)
            } else {
                action(&["--ask", "device", "wifi", "connect", id, "ifname", &interface], Some(password))
            }
        },
        _ => Err("Unknown Wi-Fi control".into()),
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn escaped_names_and_access_points() {
        let data = "*:Cafe\\: upstairs:AA\\:BB\\:CC\\:DD\\:EE\\:FF:72:WPA2\n:Cafe\\: upstairs:11\\:22\\:33\\:44\\:55\\:66:90:WPA2\n:Open\\\\Space:01\\:02\\:03\\:04\\:05\\:06:33:--\n";
        let rows = networks(data);
        assert_eq!(rows.len(),2); assert_eq!(rows[0]["name"],"Cafe: upstairs");
        assert_eq!(rows[0]["connected"],true); assert_eq!(rows[1]["name"],"Open\\Space"); assert_eq!(rows[1]["secured"],false);
        assert!(networks(":Hidden:bad:100:WPA2").is_empty());
    }
}
