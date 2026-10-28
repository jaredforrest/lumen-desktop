use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_OBJECTS: usize = 16_384;
const MAX_ROWS: usize = 64;
const DEADLINE: f64 = 2.0;
const COVERAGE: &str = "PipeWire-visible graph only. Direct device access, hidden nodes and other capture paths are not assessed.";
const UNAVAILABLE: &str = "Capture observations are unavailable; no idle or permission conclusion can be drawn.";

fn unavailable(reason: &str) -> Value { json!({"available":false,"rows":[],"coverage":COVERAGE,"error":reason}) }

fn identifier(value: Option<&Value>) -> Result<u32, String> {
    let value = value.ok_or("Invalid graph identity")?;
    if let Some(number) = value.as_u64() { return u32::try_from(number).map_err(|_| "Invalid graph identity".into()); }
    if let Some(text) = value.as_str() {
        if text.len() <= 10 && !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
            return text.parse().map_err(|_| "Invalid graph identity".into());
        }
    }
    Err("Invalid graph identity".into())
}

fn label(value: Option<&Value>, fallback: &str) -> String {
    let Some(text) = value.and_then(Value::as_str) else { return fallback.into(); };
    if text.trim().is_empty() { return fallback.into(); }
    let filtered: String = text.chars().filter(|ch| !ch.is_control()).take(160).collect();
    if filtered.is_empty() { fallback.into() } else { filtered }
}

fn describe(objects: &Value) -> Result<Value, String> {
    let objects = objects.as_array().filter(|objects| objects.len() <= MAX_OBJECTS).ok_or("Unsupported graph size")?;
    let mut indexed = HashMap::new();
    for object in objects {
        let object = object.as_object().ok_or("Invalid graph object")?;
        let id = identifier(object.get("id"))?;
        if indexed.insert(id, Value::Object(object.clone())).is_some() { return Err("Duplicate graph identity".into()); }
    }
    let mut rows: BTreeMap<(String, u32), Value> = BTreeMap::new();
    for object in indexed.values() {
        if object["type"] != "PipeWire:Interface:Link" { continue; }
        let info = object.get("info").and_then(Value::as_object).ok_or("Missing link evidence")?;
        if info.get("state").and_then(Value::as_str) != Some("active") { continue; }
        let source = identifier(info.get("output-node-id"))?;
        let target = identifier(info.get("input-node-id"))?;
        let node = indexed.get(&target).ok_or("Incomplete active graph")?;
        let origin = indexed.get(&source).ok_or("Incomplete active graph")?;
        if node["type"] != "PipeWire:Interface:Node" || origin["type"] != "PipeWire:Interface:Node" { return Err("Invalid link endpoint".into()); }
        let empty = json!({});
        let ni = node.get("info").unwrap_or(&empty);
        if !ni.is_object() { return Err("Invalid node information".into()); }
        let props = ni.get("props").unwrap_or(&empty);
        if !props.is_object() { return Err("Invalid node metadata".into()); }
        let kind = match props["media.class"].as_str() { Some("Stream/Input/Audio") => "audio-input", Some("Stream/Input/Video") => "video-input", _ => continue };
        let client = if props.get("client.id").is_some() { indexed.get(&identifier(props.get("client.id"))?) } else { None };
        let ci = client.filter(|client| client["type"] == "PipeWire:Interface:Client").and_then(|client| client.get("info"));
        if ci.is_some_and(|info| !info.is_object()) { return Err("Invalid client information".into()); }
        let cp = ci.and_then(|info| info.get("props"));
        if cp.is_some_and(|props| !props.is_object()) { return Err("Invalid client metadata".into()); }
        let key = (kind.to_owned(), target);
        if !rows.contains_key(&key) {
            if rows.len() >= MAX_ROWS { return Err("Capture observation count exceeds supported limit".into()); }
            let reported = cp.and_then(|props| props.get("application.name")).filter(|name| name.as_str().is_some_and(|name| !name.is_empty()))
                .or_else(|| props.get("application.name"));
            rows.insert(key.clone(), json!({"nodeId":target.to_string(),"kind":kind,"reportedApplication":label(reported,"Unidentified client"),"sources":[],"attribution":"Reported PipeWire client metadata; not authenticated application identity"}));
        }
        let sources = rows.get_mut(&key).and_then(|row| row.get_mut("sources")).and_then(Value::as_array_mut).ok_or("Invalid observation")?;
        if !sources.iter().any(|item| item.as_str() == Some(&source.to_string())) { sources.push(json!(source.to_string())); }
    }
    let mut result: Vec<Value> = rows.into_values().collect();
    for row in &mut result {
        if let Some(sources) = row.get_mut("sources").and_then(Value::as_array_mut) {
            sources.sort_by_key(|item| item.as_str().and_then(|item| item.parse::<u32>().ok()).unwrap_or(0));
        }
    }
    Ok(json!({"available":true,"rows":result,"coverage":COVERAGE,"error":""}))
}

fn boottime() -> Result<f64, String> {
    let mut stamp = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut stamp) } != 0 { return Err(std::io::Error::last_os_error().to_string()); }
    Ok(stamp.tv_sec as f64 + stamp.tv_nsec as f64 / 1_000_000_000.0)
}

fn read_graph() -> Result<Value, String> {
    let end = boottime()? + DEADLINE;
    let binary = std::env::var_os("LUMEN_PW_DUMP").unwrap_or_else(|| "pw-dump".into());
    let mut child = Command::new(binary).arg("--no-colors").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())?;
    let result = (|| -> Result<Value, String> {
        let mut output = child.stdout.take().ok_or("PipeWire graph is unavailable")?;
        let descriptor = output.as_raw_fd();
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0 { return Err(std::io::Error::last_os_error().to_string()); }
        let mut bytes = Vec::new();
        loop {
            let remaining = end - boottime()?;
            if remaining <= 0.0 { return Err("Capture observation timed out".into()); }
            let mut poll = libc::pollfd { fd: descriptor, events: libc::POLLIN, revents: 0 };
            let timeout = (remaining * 1000.0).ceil().clamp(1.0, i32::MAX as f64) as i32;
            let ready = unsafe { libc::poll(&mut poll, 1, timeout) };
            if ready < 0 { if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted { continue; } return Err(std::io::Error::last_os_error().to_string()); }
            if boottime()? >= end { return Err("Capture observation timed out".into()); }
            if ready == 0 { return Err("Capture observation timed out".into()); }
            let mut chunk = [0_u8; 65_536];
            let capacity = chunk.len().min(MAX_BYTES - bytes.len() + 1);
            match output.read(&mut chunk[..capacity]) {
                Ok(0) => break,
                Ok(count) => { bytes.extend_from_slice(&chunk[..count]); if bytes.len() > MAX_BYTES { return Err("Capture graph exceeds supported byte limit".into()); } }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock || error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        while child.try_wait().map_err(|error| error.to_string())?.is_none() {
            if boottime()? >= end { return Err("Capture observation timed out".into()); }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !child.wait().map_err(|error| error.to_string())?.success() { return Err("PipeWire graph is unavailable".into()); }
        if boottime()? >= end { return Err("Capture observation timed out".into()); }
        let value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if boottime()? >= end { return Err("Capture observation timed out".into()); }
        Ok(value)
    })();
    if child.try_wait().ok().flatten().is_none() { let _ = child.kill(); }
    let _ = child.wait();
    result
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "snapshot".into());
    let result = match mode.as_str() {
        "snapshot" => Ok(read_graph().and_then(|graph| describe(&graph)).unwrap_or_else(|_| unavailable(UNAVAILABLE))),
        "graph" => read_graph(),
        "describe" => (|| -> Result<Value, String> {
            let mut input = Vec::new();
            std::io::stdin().take((MAX_BYTES + 1) as u64).read_to_end(&mut input).map_err(|error| error.to_string())?;
            if input.len() > MAX_BYTES { Err("Capture graph exceeds supported byte limit".into()) }
            else { serde_json::from_slice::<Value>(&input).map_err(|error| error.to_string()).and_then(|value| describe(&value)) }
        })(),
        _ => Err("Unknown capture observation command".into()),
    };
    match result { Ok(value) => println!("{value}"), Err(_) if mode == "snapshot" => println!("{}", unavailable(UNAVAILABLE)), Err(error) => { let _ = writeln!(std::io::stderr(), "{error}"); std::process::exit(1); } }
}
