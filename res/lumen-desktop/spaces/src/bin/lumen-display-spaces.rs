//! Rust 2024 display policy observer and Settings request adapter.
use lumen_spaces::display_policy::{configured, identity_record, preferences, project, remembered_primary};
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, String>;

fn query(kind: &str) -> Result<Value> {
    let output = Command::new("timeout").args(["2s", "swaymsg", "-r", "-t", kind])
        .output().map_err(|error| error.to_string())?;
    if !output.status.success() || output.stdout.len() > 16 * 1024 * 1024 {
        return Err("Display policy observation failed".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

fn endpoint() -> Result<(String, u64, u64, i64, i64)> {
    let path = env::var("SWAYSOCK").map_err(|_| "Compositor socket is unavailable")?;
    let info = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !info.file_type().is_socket() || info.uid() != unsafe { libc::getuid() } {
        return Err("Compositor endpoint is not an owned socket".into());
    }
    Ok((path, info.dev(), info.ino(), info.ctime(), info.ctime_nsec()))
}

fn observe() -> Result<()> {
    let empty = json!({"available":false,"applied":false,"separateDisplays":true,
        "primary":"","primaryDisplay":null,"outputs":[],"error":"Display policy observer is unavailable"});
    println!("{empty}");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let expected = endpoint()?;
    let mut previous = Value::Null;
    loop {
        let started = Instant::now();
        let value = (|| -> Result<Value> {
            if endpoint()? != expected { return Err("Desktop session changed".into()); }
            let tree = query("get_tree")?;
            let outputs = query("get_outputs")?;
            let outputs = outputs.as_array().ok_or("Invalid connected displays")?;
            let desired = configured()?;
            let remembered = if desired["primaryDisplay"].is_null() { remembered_primary()? } else { None };
            let projected = project(&tree["display_spaces"], outputs, &desired, remembered.as_deref())?;
            if started.elapsed() > Duration::from_secs(2) || endpoint()? != expected {
                return Err("Display observation expired".into());
            }
            Ok(projected)
        })()?;
        if value != previous {
            println!("{value}");
            io::stdout().flush().map_err(|error| error.to_string())?;
            previous = value;
        }
        thread::sleep(Duration::from_millis(600));
    }
}

fn edit(request: &Value) -> Result<Value> {
    let canonical = canonical_request(request, &configured()?)?;
    let binary = env::var("LUMEN_NIX_SETTINGS_BIN").unwrap_or_else(|_| "lumen-nix-settings".into());
    let output = Command::new(binary).args(["set", "spaces", &canonical.to_string()])
        .output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(json!({"ok":true}))
}

fn canonical_request(request: &Value, current: &Value) -> Result<Value> {
    let fields = request.as_object().ok_or("Invalid display-space preference action")?;
    if fields.len() != 2 || !fields.contains_key("key") || !fields.contains_key("value") {
        return Err("Invalid display-space preference action".into());
    }
    let key = request["key"].as_str().ok_or("Invalid display-space preference action")?;
    if key != "separateDisplays" && key != "primaryDisplay" {
        return Err("Invalid display-space preference action".into());
    }
    let mut next = json!({"separateDisplays":current["separateDisplays"],
        "primaryDisplay":current["primaryDisplay"].as_str().map(identity_record).transpose()?.unwrap_or(Value::Null)});
    next[key] = request["value"].clone();
    let parsed = preferences(&next)?;
    let canonical = json!({"separateDisplays":parsed["separateDisplays"],
        "primaryDisplay":parsed["primaryDisplay"].as_str().map(identity_record).transpose()?.unwrap_or(Value::Null)});
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_edits_preserve_the_other_preference_and_reject_invalid_values() {
        let current=json!({"separateDisplays":true,"primaryDisplay":null});
        assert_eq!(canonical_request(&json!({"key":"separateDisplays","value":false}),&current).unwrap(),
            json!({"separateDisplays":false,"primaryDisplay":null}));
        let chosen=json!({"separateDisplays":false,"primaryDisplay":"connector:A"});
        assert_eq!(canonical_request(&json!({"key":"primaryDisplay","value":{"kind":"connector","name":"B"}}),&chosen).unwrap(),
            json!({"separateDisplays":false,"primaryDisplay":{"kind":"connector","name":"B","make":null,"model":null,"serial":null}}));
        for bad in [json!({"key":"separateDisplays","value":1}),json!({"key":"unknown","value":true}),
            json!({"key":"primaryDisplay","value":{"kind":"connector","name":"A\n"}}),
            json!({"key":"separateDisplays","value":true,"unexpected":1})] {
            assert!(canonical_request(&bad,&current).is_err(),"{bad}");
        }
    }
}

fn main() {
    if env::args().nth(1).as_deref() == Some("set") {
        let mut line = String::new();
        let response = (|| -> Result<Value> {
            io::stdin().lock().read_line(&mut line).map_err(|error| error.to_string())?;
            if line.len() > 65536 { return Err("Display preference request is too large".into()); }
            let value: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
            edit(&value)
        })().unwrap_or_else(|error| json!({"error":error}));
        println!("{response}");
    } else if let Err(error) = observe() {
        eprintln!("{error}");
    }
}
