//! Declarative owned shortcut edits; dispatch remains compositor-owned.
use lumen_shortcut_settings::{process, runtime};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{BufRead, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, OpenOptionsExt},
    },
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
fn read(path: &PathBuf) -> Result<(Vec<u8>, Value)> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 512 * 1024 {
        return Err("Shortcut declarations exceed their size limit".into());
    }
    let value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok((bytes, value))
}
fn directory() -> Result<PathBuf> {
    let base =
        PathBuf::from(env::var("XDG_RUNTIME_DIR").map_err(|_| "Session runtime is unavailable")?)
            .join("lumen-shortcut-settings");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&base)
        .map_err(|e| e.to_string())?;
    Ok(base)
}
fn configuration() -> Result<PathBuf> {
    env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| env::var("HOME").map(|p| PathBuf::from(p).join(".config")))
        .map(|p| p.join("lumen-desktop"))
        .map_err(|_| "Configuration directory is unavailable".into())
}
struct Snapshot {
    catalog: Value,
    registry: Value,
    generation: String,
    runtime: Option<Value>,
    runtime_error: String,
}
fn snapshot() -> Result<Snapshot> {
    let config = configuration()?;
    let (catalog_bytes, catalog) = read(&config.join("shortcut-editor.json"))?;
    let (registry_bytes, registry) = read(&config.join("shortcuts.json"))?;
    if catalog["version"] != 1
        || !catalog["editable"].is_boolean()
        || !catalog["bindings"].is_array()
        || registry["version"] != 1
        || registry["valid"] != true
        || !registry["bindings"].is_array()
    {
        return Err("Configured shortcuts are invalid; editing is unavailable".into());
    }
    let expected = PathBuf::from(
        catalog["registrySource"]
            .as_str()
            .ok_or("Shortcut generation identity is missing")?,
    );
    if !expected.starts_with("/nix/store") || read(&expected)?.0 != registry_bytes {
        return Err(
            "Shortcut configuration is between generations; wait for publication to finish".into(),
        );
    }
    let mut digest = Sha256::new();
    digest.update((catalog_bytes.len() as u64).to_le_bytes());
    digest.update(catalog_bytes);
    digest.update(registry_bytes);
    let (runtime, runtime_error) = match runtime::confirmed(&expected) {
        Ok(value) => (Some(value), String::new()),
        Err(error) => (None, error),
    };
    digest.update(serde_json::to_vec(&runtime).map_err(|e| e.to_string())?);
    Ok(Snapshot {
        catalog,
        registry,
        generation: format!("{:x}", digest.finalize()),
        runtime,
        runtime_error,
    })
}
fn status(s: &Snapshot) -> Value {
    json!({"available":true,"configuredEditable":s.catalog["editable"],"editable":s.catalog["editable"]==true && s.runtime.is_some(),"runtimeConfirmed":s.runtime.is_some(),"runtimeDetail":s.runtime_error,"generation":s.generation,"bindings":s.catalog["bindings"],"boundary":"Owned default-mode symbol bindings only. Raw compositor configuration, runtime changes, keycodes and undeclared application shortcuts are not covered."})
}
fn settings(args: &[&str]) -> Result<Value> {
    let budget = if args.first() == Some(&"read") {
        20
    } else {
        4000
    };
    let result = process::run(
        "lumen-nix-settings",
        args,
        Duration::from_secs(budget),
        &directory()?.join("provider-diagnostic.log"),
    )?;
    let parsed = serde_json::from_slice::<Value>(&result.stdout)
        .map_err(|_| "Settings provider returned an invalid response; outcome is unconfirmed")?;
    if !result.success || parsed.get("error").is_some() {
        return Err(parsed["error"]
            .as_str()
            .unwrap_or("Settings update failed")
            .chars()
            .take(2000)
            .collect());
    }
    Ok(parsed)
}
fn validate(s: &Snapshot, row: &Value, chord: Option<&str>) -> Result<()> {
    let mut bindings = Vec::new();
    for binding in s.registry["bindings"].as_array().unwrap() {
        if row["enabled"] == true
            && binding["id"]
                == format!(
                    "sway:{}",
                    row["chord"].as_str().ok_or("Invalid configured chord")?
                )
        {
            continue;
        }
        let mut binding = binding
            .as_object()
            .ok_or("Invalid configured binding")?
            .clone();
        binding.remove("physical");
        bindings.push(Value::Object(binding));
    }
    if let Some(chord) = chord {
        bindings.push(json!({"id":row["id"],"action":row["action"],"chord":chord,"scope":"global","owner":"desktop"}));
    }
    let path = directory()?.join(format!("candidate-{}.json", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(
        json!({"version":1,"bindings":bindings})
            .to_string()
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let result = process::run(
        "lumen-shortcuts",
        &[path.to_str().ok_or("Invalid validation path")?],
        Duration::from_secs(5),
        &directory()?.join("validator-diagnostic.log"),
    );
    let _ = fs::remove_file(path);
    let result = result?;
    if !result.success {
        let value: Value = serde_json::from_slice(&result.stdout).unwrap_or(Value::Null);
        return Err(format!(
            "Shortcut rejected; previous map retained. {}",
            value["diagnostics"]
                .to_string()
                .chars()
                .take(2000)
                .collect::<String>()
        ));
    }
    Ok(())
}
fn confirms(id: &str, chord: Option<&str>) -> Option<Snapshot> {
    let s = snapshot().ok()?;
    let confirmed = s.catalog["editable"] == true
        && s.runtime.is_some()
        && s.catalog["bindings"].as_array().is_some_and(|rows| {
            rows.iter().any(|r| {
                r["id"] == id
                    && r["editable"] == true
                    && match chord {
                        None => {
                            r["enabled"] == false
                                && r["chord"].is_null()
                                && !s.registry["bindings"].as_array().unwrap().iter().any(|b| {
                                    b["id"]
                                        == format!(
                                            "sway:{}",
                                            r["defaultChord"].as_str().unwrap_or("")
                                        )
                                        && b["action"] == r["action"]
                                        && b["enabled"] == true
                                })
                        }
                        Some(chord) => {
                            r["enabled"] == true
                                && r["chord"] == chord
                                && s.registry["bindings"].as_array().is_some_and(|bindings| {
                                    bindings.iter().any(|b| {
                                        b["id"] == format!("sway:{chord}")
                                            && b["action"] == r["action"]
                                            && b["enabled"] == true
                                    })
                                })
                        }
                    }
            })
        });
    confirmed.then_some(s)
}
fn edit(request: &Value, s: &Snapshot) -> Result<Value> {
    if s.catalog["editable"] != true {
        return Err("Shortcut editing requires an explicitly wired consumer configuration".into());
    }
    if s.runtime.is_none() {
        return Err(format!(
            "Shortcut activation is unconfirmed: {}",
            s.runtime_error
        ));
    }
    if request["generation"] != s.generation {
        return Err("Shortcut configuration changed; review it before retrying".into());
    }
    let id = request["id"]
        .as_str()
        .ok_or("A stable binding identity is required")?;
    let row = s.catalog["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id && r["editable"] == true)
        .ok_or("This shortcut is protected or consumer-owned")?;
    let action = request["action"]
        .as_str()
        .ok_or("Edit action is required")?;
    let chord = match action {
        "disable" => None,
        "restore" => Some(
            row["defaultChord"]
                .as_str()
                .ok_or("Default chord is unavailable")?,
        ),
        "set" => Some(
            request["chord"]
                .as_str()
                .ok_or("A symbol chord is required")?,
        ),
        _ => return Err("Unsupported shortcut action".into()),
    };
    if chord.is_some_and(|chord| {
        chord.starts_with("--")
            || chord.is_empty()
            || chord.len() > 512
            || chord.chars().any(char::is_control)
    }) {
        return Err("Use an explicit default-mode press symbol chord".into());
    }
    validate(s, row, chord)?;
    let saved = settings(&["read"])?;
    let previous = saved["shortcuts"].clone();
    let mut next = previous.clone();
    let rebindings = next["rebindings"]
        .as_object_mut()
        .ok_or("Authoritative shortcut settings are unavailable")?;
    if action == "restore" {
        rebindings.remove(id);
    } else {
        rebindings.insert(
            id.to_owned(),
            chord.map(|v| json!(v)).unwrap_or(Value::Null),
        );
    }
    if snapshot()?.generation != s.generation {
        return Err("Shortcut configuration changed before saving; retry".into());
    }
    settings(&[
        "compare-set",
        "shortcuts",
        &previous.to_string(),
        &next.to_string(),
    ])?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(witness) = confirms(id, chord) {
            let mut reply = status(&witness);
            reply["message"] = json!("Shortcut applied and confirmed in the running compositor");
            return Ok(reply);
        }
        thread::sleep(Duration::from_millis(50));
    }
    if settings(&["read"])?["shortcuts"] != next || snapshot()?.generation != s.generation {
        return Err("Shortcut settings or the effective map changed concurrently; rollback stopped to preserve the newer configuration. Inspect it before retrying.".into());
    }
    let rollback = settings(&[
        "compare-set",
        "shortcuts",
        &next.to_string(),
        &previous.to_string(),
    ]);
    if rollback.is_err() {
        return Err("The effective map did not confirm this edit, and settings rollback failed. Inspect configuration before retrying.".into());
    }
    if settings(&["read"])?["shortcuts"] != previous || snapshot()?.generation != s.generation {
        return Err("Rollback could not confirm the previous settings and effective map. Inspect configuration before retrying.".into());
    }
    Err("The consumer did not publish the requested effective shortcut. Previous settings restored; the effective map remains unconfirmed. No successful apply is claimed.".into())
}
fn run() -> Result<Value> {
    let mut input = String::new();
    std::io::BufReader::new(std::io::stdin().take(65537))
        .read_line(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() > 65536 {
        return Err("Shortcut request is too large".into());
    }
    let request: Value = serde_json::from_str(&input).map_err(|e| e.to_string())?;
    let object = request
        .as_object()
        .ok_or("Shortcut request must be an object")?;
    if object
        .keys()
        .any(|key| !["action", "id", "chord", "generation"].contains(&key.as_str()))
    {
        return Err("Unknown shortcut request field".into());
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(directory()?.join("transaction.lock"))
        .map_err(|e| e.to_string())?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err("Could not lock shortcut settings".into());
    }
    let snapshot = snapshot()?;
    if request["action"] == "status" {
        Ok(status(&snapshot))
    } else {
        edit(&request, &snapshot)
    }
}
fn main() {
    process::install_interrupts();
    // Children inherit only the pinned package PATH supplied by the session wrapper.
    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            let mut value = snapshot()
                .map(|s| status(&s))
                .unwrap_or(json!({"available":false,"editable":false,"bindings":[]}));
            value["error"] = json!(error);
            println!("{value}");
            std::process::exit(1);
        }
    }
}
