//! Typed shortcut inventory, collision diagnostics and reachable UI hints.
use serde_json::{Map, Value, json};
use std::{collections::{BTreeMap, HashSet}, env, fs, path::PathBuf, process::ExitCode};

#[derive(Clone)]
struct Physical { key: String, modifiers: Vec<&'static str>, trigger: String, device: String, locked: bool, inhibited: bool }
impl Physical {
    fn json(&self) -> Value { json!({"key":self.key,"modifiers":self.modifiers,"trigger":self.trigger,
        "device":self.device,"locked":self.locked,"inhibited":self.inhibited}) }
}
struct Binding { raw: Value, id: String, action: String, chord: String, scope: String, app: String,
    mode: String, owner: String, enabled: bool, physical: Physical }

fn words(source: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut active = false;
    let mut quote = None;
    let mut chars = source.chars();
    while let Some(ch) = chars.next() {
        match quote {
            Some('\'') if ch == '\'' => quote = None,
            Some('"') if ch == '"' => quote = None,
            Some('\'') => current.push(ch),
            Some('"') if ch == '\\' => current.push(chars.next().ok_or("Chord quoting is invalid")?),
            Some('"') => current.push(ch),
            None if ch == '\'' || ch == '"' => { quote = Some(ch); active = true; },
            None if ch == '\\' => { current.push(chars.next().ok_or("Chord quoting is invalid")?); active = true; },
            None if ch.is_whitespace() => {
                if active { result.push(std::mem::take(&mut current)); active = false; }
            }
            None => { current.push(ch); active = true; }
            Some(_) => unreachable!("only single and double quotes are accepted"),
        }
    }
    if quote.is_some() { return Err("Chord quoting is invalid".into()); }
    if active { result.push(current); }
    Ok(result)
}

fn bounded(value: Option<&Value>, name: &str, max: usize) -> Result<String, String> {
    let s = value.and_then(Value::as_str).ok_or_else(|| format!("Binding requires bounded {name}"))?;
    if s.is_empty() || s.chars().count() > max || s.chars().any(|c| (c as u32) < 32) {
        return Err(format!("Binding requires bounded {name}"));
    }
    Ok(s.into())
}

fn chord(value: &str) -> Result<Physical, String> {
    if value.is_empty() || value.chars().count() > 512 || value.chars().any(|c| (c as u32) < 32) {
        return Err("Chord must be a non-empty bounded string".into());
    }
    let mut tokens = words(value)?.into_iter();
    let mut flags = HashSet::new();
    let mut device = String::from("*");
    let mut key_token = None;
    while let Some(token) = tokens.next() {
        if key_token.is_none() && token.starts_with("--") {
            if let Some(value) = token.strip_prefix("--input-device=") {
                if value.is_empty() { return Err("Input device must be explicit".into()); }
                device = value.into();
            } else if ["--release", "--locked", "--inhibited", "--no-repeat", "--whole-window", "--border", "--exclude-titlebar"].contains(&token.as_str()) {
                flags.insert(token);
            } else { return Err(format!("Unsupported binding option: {token}")); }
        } else if key_token.replace(token).is_some() || tokens.next().is_some() {
            return Err("A chord requires exactly one final key".into());
        }
    }
    let token = key_token.ok_or("A chord requires exactly one final key")?;
    let mut parts: Vec<_> = token.split('+').collect();
    let key = parts.pop().unwrap_or("");
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.,/;[]\\`-=".contains(&b)) {
        return Err("Use an explicit keysym name for the final key".into());
    }
    let mut modifiers = Vec::new();
    for part in parts {
        let modifier = match part.to_ascii_lowercase().as_str() {
            "shift" => "Shift", "lock" | "capslock" => "Lock", "control" | "ctrl" => "Control",
            "alt" | "mod1" => "Mod1", "mod2" => "Mod2", "mod3" => "Mod3",
            "mod4" | "super" => "Mod4", "mod5" => "Mod5",
            _ => return Err(format!("Unknown physical modifier: {part}")),
        };
        if modifiers.contains(&modifier) { return Err("A modifier occurs more than once".into()); }
        modifiers.push(modifier);
    }
    let order = ["Shift", "Lock", "Control", "Mod1", "Mod2", "Mod3", "Mod4", "Mod5"];
    modifiers.sort_by_key(|m| order.iter().position(|candidate| candidate == m).unwrap());
    let key = match key.to_ascii_lowercase().as_str() {
        "esc" => "escape".into(), "enter" => "return".into(), _ => key.to_ascii_lowercase(),
    };
    Ok(Physical { key, modifiers, trigger: if flags.contains("--release") { "release" } else { "press" }.into(),
        device, locked: flags.contains("--locked"), inhibited: flags.contains("--inhibited") })
}

fn binding(value: Value) -> Result<Binding, String> {
    let object = value.as_object().ok_or("Binding has unknown fields or is not an object")?;
    let fields = ["id", "action", "chord", "scope", "appId", "mode", "device", "trigger", "owner", "fallback", "enabled"];
    if object.keys().any(|key| !fields.contains(&key.as_str())) { return Err("Binding has unknown fields or is not an object".into()); }
    let id = bounded(object.get("id"), "id", 512)?;
    let action = bounded(object.get("action"), "action", 512)?;
    let chord_text = bounded(object.get("chord"), "chord", 512)?;
    let enabled = object.get("enabled").map(Value::as_bool).unwrap_or(Some(true)).ok_or("Enabled must be boolean")?;
    let scope = object.get("scope").map(Value::as_str).unwrap_or(Some("global")).ok_or("Unknown shortcut scope")?;
    if !["global", "application", "current-application"].contains(&scope) { return Err("Unknown shortcut scope".into()); }
    let app = object.get("appId").map(Value::as_str).unwrap_or(Some("")).ok_or("Application scope requires an exact appId")?;
    if scope == "application" && (app.is_empty() || app.contains('*')) { return Err("Application scope requires an exact appId".into()); }
    if scope != "application" && !app.is_empty() { return Err("appId is only valid for application scope".into()); }
    let mode = object.get("mode").map(Value::as_str).unwrap_or(Some("default")).ok_or("Mode must be a bounded string")?;
    if mode.is_empty() || mode.chars().count() > 128 { return Err("Mode must be a bounded string".into()); }
    let mut physical = chord(&chord_text)?;
    if let Some(trigger) = object.get("trigger") {
        let trigger = trigger.as_str().ok_or("Unknown trigger")?;
        if !["press", "release"].contains(&trigger) { return Err("Unknown trigger".into()); }
        if physical.trigger == "release" && trigger != "release" { return Err("Trigger conflicts with release flag".into()); }
        physical.trigger = trigger.into();
    }
    if let Some(device) = object.get("device") {
        let device = device.as_str().ok_or("Device must be a bounded identifier")?;
        if device.is_empty() || device.chars().count() > 256 { return Err("Device must be a bounded identifier".into()); }
        if physical.device != "*" && physical.device != device { return Err("Device conflicts with chord option".into()); }
        physical.device = device.into();
    }
    for field in ["owner", "fallback"] {
        if object.get(field).is_some_and(|v| !v.is_string()) { return Err(format!("{field} must be a string")); }
    }
    let owner = object.get("owner").and_then(Value::as_str).unwrap_or("").into();
    Ok(Binding { raw: value.clone(), id, action, chord: chord_text, scope: scope.into(), app: app.into(),
        mode: mode.into(), owner, enabled, physical })
}

fn overlaps(a: &Binding, b: &Binding) -> bool {
    a.mode == b.mode && a.physical.key == b.physical.key && a.physical.modifiers == b.physical.modifiers &&
    a.physical.trigger == b.physical.trigger && (a.physical.device == "*" || b.physical.device == "*" || a.physical.device == b.physical.device) &&
    !(a.scope == "application" && b.scope == "application" && a.app != b.app)
}

fn validate(document: Value) -> Result<Value, String> {
    let object = document.as_object().ok_or("Expected shortcut registry version 1 and a bindings list")?;
    if object.keys().any(|key| !["version", "bindings"].contains(&key.as_str())) || object.get("version").and_then(Value::as_i64) != Some(1) || !object.get("version").is_some_and(Value::is_number) {
        return Err("Expected shortcut registry version 1 and a bindings list".into());
    }
    let rows = object.get("bindings").and_then(Value::as_array).ok_or("Expected shortcut registry version 1 and a bindings list")?;
    if rows.len() > 4096 { return Err("Too many shortcut declarations".into()); }
    let mut values = Vec::new();
    let mut identities = HashSet::new();
    for row in rows {
        let item = binding(row.clone())?;
        if !identities.insert(item.id.clone()) { return Err(format!("Duplicate binding ID: {}", item.id)); }
        values.push(item);
    }
    let mut diagnostics = Vec::new();
    for (index, left) in values.iter().enumerate() {
        if !left.enabled { continue; }
        for right in values.iter().skip(index + 1) {
            if right.enabled && overlaps(left, right) {
                let chord = left.physical.modifiers.iter().copied().chain(std::iter::once(left.physical.key.as_str())).collect::<Vec<_>>().join("+");
                diagnostics.push(json!({"code":"shortcut_collision","bindings":[left.id,right.id],
                    "actions":[left.action,right.action],"chord":chord,"trigger":left.physical.trigger,
                    "message":"Overlapping active shortcut scopes; disable or explicitly rescope one declaration."}));
            }
        }
    }
    let mut hints: BTreeMap<(String, String, String), Vec<Value>> = BTreeMap::new();
    if diagnostics.is_empty() {
        for item in &values {
            if !item.enabled || item.owner == "reserved" || item.action == "consumer.binding" || item.mode != "default" || item.physical.device != "*" || item.physical.trigger != "press" { continue; }
            hints.entry((item.scope.clone(), item.app.clone(), item.action.clone())).or_default().push(json!({
                "id":item.id,"chord":item.chord,"physical":item.physical.json()}));
        }
    }
    let hints: Vec<_> = hints.into_iter().map(|((scope, app, action), mut bindings)| {
        bindings.sort_by_key(|row| (row["physical"]["modifiers"].as_array().map(Vec::len).unwrap_or(0), row["chord"].as_str().unwrap_or("").to_owned()));
        json!({"scope":scope,"appId":app,"action":action,"bindings":bindings})
    }).collect();
    let bindings: Vec<_> = values.into_iter().map(|item| {
        let mut object = item.raw.as_object().cloned().unwrap_or_else(Map::new);
        object.insert("scope".into(), json!(item.scope));
        object.insert("appId".into(), json!(item.app));
        object.insert("mode".into(), json!(item.mode));
        object.insert("enabled".into(), json!(item.enabled));
        object.insert("physical".into(), item.physical.json());
        Value::Object(object)
    }).collect();
    Ok(json!({"version":1,"valid":diagnostics.is_empty(),"bindings":bindings,"diagnostics":diagnostics,"hints":hints}))
}

fn run() -> Result<ExitCode, String> {
    let mut args = env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("Registry path required")?);
    let mut output = None;
    if let Some(flag) = args.next() {
        if flag != "--output" { return Err("Unknown option".into()); }
        output = Some(PathBuf::from(args.next().ok_or("Output path required")?));
    }
    if args.next().is_some() { return Err("Unknown option".into()); }
    let source = fs::read(source).map_err(|error| error.to_string())?;
    let value = serde_json::from_slice(&source).map_err(|error| error.to_string())?;
    let result = validate(value)?;
    let encoded = serde_json::to_string_pretty(&result).map_err(|error| error.to_string())? + "\n";
    if let Some(path) = output { fs::write(path, encoded).map_err(|error| error.to_string())?; }
    else { print!("{encoded}"); }
    if result["valid"] == false {
        for row in result["diagnostics"].as_array().unwrap() {
            eprintln!("Shortcut collision: {} ({})", row["bindings"].as_array().unwrap().iter().map(|v| v.as_str().unwrap_or("")).collect::<Vec<_>>().join(", "), row["chord"].as_str().unwrap_or(""));
        }
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            println!("{}", json!({"valid":false,"diagnostics":[{"code":"invalid_registry","message":error}]}));
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{chord, validate};
    use serde_json::json;
    #[test]
    fn aliases_collide_and_scope_is_explicit() {
        let result = validate(json!({"version":1,"bindings":[
            {"id":"first","action":"one","chord":"Mod4+Control+Shift+f"},
            {"id":"other","action":"two","chord":"Shift+Ctrl+Super+F"}]})).unwrap();
        assert_eq!(result["valid"], false);
        assert_eq!(result["diagnostics"][0]["bindings"], json!(["first","other"]));
        assert!(chord("Ctrl+Control+x").is_err());
    }
}
