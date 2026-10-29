//! Exact-device compositor input preferences with durable Nix settings.
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::OpenOptions;
use std::io::{BufRead, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

type Result<T> = std::result::Result<T, String>;
const MAGIC: &[u8; 6] = b"i3-ipc";

#[derive(Clone, Copy)]
struct Field { key: &'static str, label: &'static str, kind: &'static str, min: f64, max: f64,
    step: f64, types: &'static [&'static str], source: &'static str }
const FIELDS: &[Field] = &[
    Field { key:"repeat_rate", label:"Key repeat rate", kind:"number", min:0.0, max:60.0, step:1.0, types:&["keyboard"], source:"repeat_rate" },
    Field { key:"repeat_delay", label:"Key repeat delay (ms)", kind:"number", min:100.0, max:2000.0, step:50.0, types:&["keyboard"], source:"repeat_delay" },
    Field { key:"pointer_accel", label:"Tracking speed", kind:"number", min:-1.0, max:1.0, step:0.1, types:&["pointer","touchpad"], source:"accel_speed" },
    Field { key:"scroll_factor", label:"Scroll speed", kind:"number", min:0.1, max:5.0, step:0.1, types:&["pointer","touchpad"], source:"scroll_factor" },
    Field { key:"natural_scroll", label:"Natural scrolling", kind:"bool", min:0.0, max:0.0, step:0.0, types:&["pointer","touchpad"], source:"natural_scroll" },
    Field { key:"left_handed", label:"Swap primary and secondary buttons", kind:"bool", min:0.0, max:0.0, step:0.0, types:&["pointer","touchpad"], source:"left_handed" },
    Field { key:"tap", label:"Tap to click", kind:"bool", min:0.0, max:0.0, step:0.0, types:&["touchpad"], source:"tap" },
    Field { key:"dwt", label:"Disable trackpad while typing", kind:"bool", min:0.0, max:0.0, step:0.0, types:&["touchpad"], source:"dwt" },
];
fn field(key: &str) -> Option<&'static Field> { FIELDS.iter().find(|field| field.key == key) }

fn valid(field: &Field, value: &Value) -> bool {
    if field.kind == "bool" { return value.is_boolean(); }
    if value.is_boolean() { return false; }
    let Some(number) = value.as_f64() else { return false; };
    number.is_finite() && number >= field.min && number <= field.max
        && (!matches!(field.key, "repeat_rate" | "repeat_delay") || value.as_i64().is_some())
}

fn connect() -> Result<UnixStream> {
    let path = env::var("SWAYSOCK").map_err(|_| "The compositor input provider is unavailable.")?;
    if path.is_empty() { return Err("The compositor input provider is unavailable.".into()); }
    let stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    Ok(stream)
}

fn send(stream: &mut UnixStream, kind: u32, payload: &str) -> Result<()> {
    let mut header = [0u8;14];
    header[..6].copy_from_slice(MAGIC);
    header[6..10].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[10..14].copy_from_slice(&kind.to_le_bytes());
    stream.write_all(&header).map_err(|e| e.to_string())?;
    stream.write_all(payload.as_bytes()).map_err(|e| e.to_string())
}

fn response(stream: &mut UnixStream) -> Result<(u32, Value)> {
    let mut header = [0u8;14];
    stream.read_exact(&mut header).map_err(|e| e.to_string())?;
    let length = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    let kind = u32::from_le_bytes(header[10..14].try_into().unwrap());
    if &header[..6] != MAGIC || length > 4*1024*1024 { return Err("Invalid compositor response".into()); }
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).map_err(|e| e.to_string())?;
    Ok((kind, serde_json::from_slice(&payload).map_err(|e| e.to_string())?))
}

fn ipc(kind: u32, payload: &str) -> Result<Value> {
    let mut stream = connect()?;
    send(&mut stream, kind, payload)?;
    let (actual, value) = response(&mut stream)?;
    if actual != kind { return Err("Unexpected compositor response".into()); }
    Ok(value)
}

fn settings(args: &[&str]) -> Result<Value> {
    let output = Command::new("lumen-nix-settings").args(args).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        let message = serde_json::from_slice::<Value>(&output.stdout).ok()
            .and_then(|value| value["error"].as_str().map(str::to_owned))
            .unwrap_or_else(|| String::from_utf8_lossy(&output.stderr).trim().to_owned());
        return Err(if message.is_empty() { "Nix settings update failed".into() } else { message });
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
fn saved() -> Result<Value> { Ok(settings(&["read"])?["inputs"].clone()) }
fn save(value: &Value) -> Result<()> { settings(&["set", "inputs", &value.to_string()]).map(|_| ()) }

fn identity(device: &Value) -> Value {
    json!({"identifier":device["identifier"].as_str().unwrap_or(""),
        "name":device["name"].as_str().unwrap_or(""),"type":device["type"].as_str().unwrap_or("")})
}
fn key_for(value: &Value) -> String { format!("{:x}", Sha256::digest(value.to_string().as_bytes())) }
fn safe_identifier(value: &str) -> bool {
    let mut pieces = value.splitn(3, ':');
    let (Some(a), Some(b), Some(c)) = (pieces.next(), pieces.next(), pieces.next()) else { return false; };
    !a.is_empty() && a.bytes().all(|byte| byte.is_ascii_digit()) && !b.is_empty()
        && b.bytes().all(|byte| byte.is_ascii_digit()) && (1..=512).contains(&c.len())
        && c.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_.:+()-".contains(&byte))
}
fn object(value: &Value) -> Map<String, Value> { value.as_object().cloned().unwrap_or_default() }
fn values(record: &Value, section: &str) -> Map<String, Value> {
    object(&record[section]).into_iter().filter(|(_, value)| !value.is_null()).collect()
}
fn same_value(actual: &Value, expected: &Value) -> bool {
    if expected.is_boolean() { return actual == expected; }
    actual.as_f64().zip(expected.as_f64()).is_some_and(|(a,b)| (a-b).abs() < 0.0001)
}
fn current_value(device: &Value, field: &Field) -> Value {
    if !field.types.contains(&device["type"].as_str().unwrap_or("")) { return Value::Null; }
    let raw = if matches!(field.key, "repeat_rate" | "repeat_delay" | "scroll_factor") {
        &device[field.key]
    } else { &device["libinput"][field.source] };
    if field.kind == "bool" {
        return match raw.as_str() { Some("enabled") => json!(true), Some("disabled") => json!(false), _ => Value::Null };
    }
    if valid(field, raw) { raw.clone() } else { Value::Null }
}

fn rows(devices: &[Value], saved: &Value) -> Vec<Value> {
    let mut rows = Vec::new();
    let mut connected = Vec::new();
    for device in devices {
        let ident = identity(device);
        let key = key_for(&ident);
        connected.push(key.clone());
        let matches = devices.iter().filter(|item| item["identifier"] == ident["identifier"]).count();
        let editable = safe_identifier(ident["identifier"].as_str().unwrap_or("")) && matches == 1;
        let overrides = values(&saved[&key], "values");
        let mut controls = Vec::new();
        for field in FIELDS {
            if !field.types.contains(&ident["type"].as_str().unwrap_or("")) { continue; }
            let value = current_value(device, field);
            let mut control = json!({"key":field.key,"label":field.label,"kind":field.kind,
                "value":value,"supported":editable && !value.is_null(),
                "saved":overrides.contains_key(field.key),
                "inSync":overrides.get(field.key).is_none_or(|expected| same_value(&value, expected))});
            if field.kind == "number" {
                control["minimum"] = json!(field.min);
                control["maximum"] = json!(field.max);
                control["step"] = json!(field.step);
            }
            controls.push(control);
        }
        let detail = if matches > 1 { "Two connected devices share this identifier; separate control is unsafe." }
            else if !editable { "This identifier cannot be safely addressed by the supported adapter." }
            else { "Changes apply only to this compositor identifier, not every device of this type." };
        rows.push(json!({"key":key,"identity":ident,"connected":true,"editable":editable,
            "controls":controls,"saved":!overrides.is_empty(),"detail":detail}));
    }
    for (key, record) in object(saved) {
        if !connected.contains(&key) {
            rows.push(json!({"key":key,"identity":record["identity"],"connected":false,"editable":false,
                "controls":[],"saved":true,
                "detail":"Disconnected. Saved preferences apply only when this exact identity returns."}));
        }
    }
    rows.sort_by_key(|row| (row["connected"] != true,
        row["identity"]["type"].as_str().unwrap_or("").to_owned(),
        row["identity"]["name"].as_str().unwrap_or("").to_owned(),
        row["key"].as_str().unwrap_or("").to_owned()));
    rows
}
fn devices() -> Result<Vec<Value>> { ipc(100, "")?.as_array().cloned().ok_or("Invalid input inventory".into()) }
fn snapshot() -> Result<Value> { Ok(json!({"available":true,"devices":rows(&devices()?, &saved()?)})) }

fn exact_device(ident: &Value, devices: &[Value]) -> Result<Value> {
    let matching: Vec<_> = devices.iter().filter(|device| device["identifier"] == ident["identifier"]).collect();
    if matching.len() != 1 || identity(matching[0]) != *ident {
        return Err("The device disconnected, changed identity or is ambiguous. Refresh before applying.".into());
    }
    Ok(matching[0].clone())
}
fn apply_value(ident: &Value, field: &Field, value: &Value) -> Result<()> {
    let identifier = ident["identifier"].as_str().ok_or("Unsupported device option or value")?;
    if !safe_identifier(identifier) || !valid(field, value) { return Err("Unsupported device option or value".into()); }
    let encoded = if value == true { "enabled".into() } else if value == false { "disabled".into() }
        else { value.to_string() };
    let command = format!("input {} {} {}", json!(identifier), field.key, encoded);
    let result = ipc(0, &command)?;
    if result.as_array().is_none_or(|items| items.is_empty() || !items.iter().all(|item| item["success"] == true)) {
        return Err("The compositor rejected this device setting".into());
    }
    Ok(())
}
fn confirmed(ident: &Value, field: &Field, value: &Value) -> Result<()> {
    let device = exact_device(ident, &devices()?)?;
    if !same_value(&current_value(&device, field), value) {
        return Err("The compositor did not confirm the requested setting".into());
    }
    Ok(())
}
fn runtime() -> Result<PathBuf> { Ok(PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is missing")?)) }
fn handle(request: &Value) -> Result<Value> {
    if !request.is_object() { return Err("Expected an input settings request".into()); }
    let action = request["action"].as_str().unwrap_or("status");
    if action == "status" { return snapshot(); }
    let folder = runtime()?.join("lumen-input-settings");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new().create(true).append(true).mode(0o600).open(folder.join("edit.lock"))
        .map_err(|e| e.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let saved = saved()?;
    let connected = devices()?;
    let rows = rows(&connected, &saved);
    let row = rows.iter().find(|row| row["key"] == request["device"])
        .ok_or("Select a reported device or saved profile")?;
    if action == "forget" {
        if request["confirmed"] != true { return Err("Confirm forgetting this device profile".into()); }
        let mut updated = object(&saved);
        updated.remove(row["key"].as_str().unwrap_or(""));
        save(&Value::Object(updated))?;
        let mut result = snapshot()?;
        result["message"] = json!("Saved profile forgotten. Current device values were not changed.");
        return Ok(result);
    }
    if !matches!(action, "set" | "restore") || row["editable"] != true {
        return Err("This device cannot be edited independently".into());
    }
    let field = field(request["key"].as_str().unwrap_or(""))
        .ok_or("Unsupported input setting")?;
    let ident = &row["identity"];
    let device = exact_device(ident, &connected)?;
    let previous = current_value(&device, field);
    if previous.is_null() { return Err("The device does not report this capability".into()); }
    let key = row["key"].as_str().ok_or("Missing device key")?;
    let record = if saved[key].is_object() { saved[key].clone() }
        else { json!({"identity":ident,"values":{},"baseline":{}}) };
    let mut overrides = values(&record, "values");
    let mut baseline = values(&record, "baseline");
    let value = if action == "restore" {
        if !overrides.contains_key(field.key) || !baseline.contains_key(field.key) {
            return Err("There is no previous value saved for this option".into());
        }
        overrides.remove(field.key);
        baseline.remove(field.key).unwrap()
    } else {
        let value = request["value"].clone();
        if !valid(field, &value) { return Err("Unsupported input option value".into()); }
        baseline.entry(field.key).or_insert(previous.clone());
        overrides.insert(field.key.into(), value.clone());
        value
    };
    exact_device(ident, &devices()?)?;
    apply_value(ident, field, &value)?;
    let mut committed = false;
    let transaction = (|| -> Result<()> {
        confirmed(ident, field, &value)?;
        let mut updated = object(&saved);
        if overrides.is_empty() { updated.remove(key); }
        else { updated.insert(key.into(), json!({"identity":ident,"values":overrides,"baseline":baseline})); }
        save(&Value::Object(updated))?;
        committed = true;
        exact_device(ident, &devices()?)?;
        apply_value(ident, field, &value)?;
        confirmed(ident, field, &value)
    })();
    if let Err(original) = transaction {
        let mut failed = Vec::new();
        if committed && save(&saved).is_err() { failed.push("saved profile"); }
        if exact_device(ident, &devices()?).and_then(|_| apply_value(ident, field, &previous))
            .and_then(|_| confirmed(ident, field, &previous)).is_err() { failed.push("previous device value"); }
        if !failed.is_empty() {
            return Err(format!("Input update failed and could not restore {}. Refresh to inspect current and saved values.", failed.join(", ")));
        }
        return Err(original);
    }
    let mut result = snapshot()?;
    result["message"] = json!(if action == "set" { "Device preference saved." }
        else { "Previous device value restored." });
    Ok(result)
}

fn restore_saved() -> Result<()> {
    let current = devices()?;
    for record in object(&saved()?).values() {
        let ident = &record["identity"];
        if exact_device(ident, &current).is_err() { continue; }
        for (key, value) in values(record, "values") {
            let Some(field) = field(&key) else { continue; };
            let Ok(device) = devices().and_then(|items| exact_device(ident, &items)) else { break; };
            let actual = current_value(&device, field);
            if !actual.is_null() && actual != value { let _ = apply_value(ident, field, &value); }
        }
    }
    Ok(())
}
fn watch() -> Result<()> {
    let mut stream = connect()?;
    send(&mut stream, 2, "[\"input\"]")?;
    let (_, accepted) = response(&mut stream)?;
    if accepted["success"] != true { return Err("Input event subscription failed".into()); }
    restore_saved()?;
    stream.set_read_timeout(None).map_err(|e| e.to_string())?;
    loop {
        let (kind, event) = response(&mut stream)?;
        if kind == 0x80000015 && event["change"] == "added" {
            thread::sleep(Duration::from_millis(100));
            restore_saved()?;
        }
    }
}
fn main() {
    if env::args().nth(1).as_deref() == Some("watch") {
        if let Err(error) = watch() { eprintln!("{error}"); std::process::exit(1); }
        return;
    }
    let mut line = String::new();
    let result = std::io::stdin().lock().read_line(&mut line).map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_str::<Value>(line.lines().next().unwrap_or("")).map_err(|e| e.to_string()))
        .and_then(|request| handle(&request));
    match result { Ok(value) => println!("{value}"),
        Err(error) => println!("{}", json!({"available":false,"devices":[],"error":error})) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_only_exact_device_controls() {
        assert!(safe_identifier("1:2:Mouse_1"));
        for bad in ["*", "type:pointer", "1:2:bad;exec", "1:2:bad\"", "1:2:bad\n"] {
            assert!(!safe_identifier(bad));
        }
        assert!(valid(field("tap").unwrap(), &json!(true)));
        assert!(!valid(field("tap").unwrap(), &json!(1)));
        assert!(!valid(field("repeat_rate").unwrap(), &json!(1.2)));
        assert!(!valid(field("repeat_rate").unwrap(), &json!(true)));
    }
}
