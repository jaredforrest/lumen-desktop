use serde_json::{Value, json};
use std::io::{BufRead, Read};
use std::process::Command;

const MAX_ID: u64 = (1_u64 << 53) - 1;

fn run(args: &[&str]) -> Result<Value, String> {
    let output = Command::new("timeout").arg("5").arg("swaymsg").arg("-r").args(args).output().map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| "Unable to read native paired placement")?;
    if !output.status.success() || value.as_array().is_some_and(|rows| rows.iter().any(|row| row.get("success").and_then(Value::as_bool) != Some(true))) {
        let reason = value.as_array().and_then(|rows| rows.iter().find_map(|row| row.get("error").and_then(Value::as_str))).unwrap_or("The native placement request was rejected");
        return Err(reason.to_owned());
    }
    Ok(value)
}

fn identity(value: Option<&Value>) -> Result<u64, String> {
    value.and_then(Value::as_u64).filter(|id| *id > 0 && *id <= MAX_ID).ok_or("Choose an exact window identity".into())
}

fn walk(node: &Value, output: &str, result: &mut Vec<Value>) {
    let output = if node.get("type").and_then(Value::as_str) == Some("output") { node.get("name").and_then(Value::as_str).unwrap_or(output) } else { output };
    let properties = node.get("window_properties");
    let app = node.get("app_id").and_then(Value::as_str).filter(|name| !name.is_empty())
        .or_else(|| properties.and_then(|value| value.get("class")).and_then(Value::as_str).filter(|name| !name.is_empty()))
        .or_else(|| properties.and_then(|value| value.get("instance")).and_then(Value::as_str).filter(|name| !name.is_empty()));
    if let (Some(pid), Some(app), Some(id)) = (node.get("pid").and_then(Value::as_u64).filter(|pid| *pid > 0), app, node.get("id").and_then(Value::as_u64)) {
        if matches!(node.get("type").and_then(Value::as_str), Some("con" | "floating_con")) {
            let title = node.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()).unwrap_or(app);
            result.push(json!({"id":id,"pid":pid,"title":title,"app":app,"pair":node.get("lumen_pair").cloned().unwrap_or_else(|| json!({})),"output":output}));
        }
    }
    for kind in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(kind).and_then(Value::as_array) { for child in children { walk(child, output, result); } }
    }
}

fn windows() -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    walk(&run(&["-t", "get_tree"])?, "", &mut result);
    Ok(result)
}

fn active_pair(value: &Value) -> bool {
    match value { Value::Null => false, Value::Bool(state) => *state, Value::Number(number) => number.as_i64() != Some(0), Value::String(text) => !text.is_empty(), _ => true }
}

fn handle(data: &Value) -> Result<Value, String> {
    let object = data.as_object().ok_or("Invalid paired placement request")?;
    if run(&["-t", "get_version"])?.get("native_paired_layouts").and_then(Value::as_bool) != Some(true) { return Err("Paired placement requires the updated compositor at next login".into()); }
    let current = windows()?;
    let source_id = identity(object.get("id"))?;
    let source = current.iter().find(|window| window["id"].as_u64() == Some(source_id)).ok_or("The captured window is no longer available")?;
    let source_pid = source["pid"].as_u64().ok_or("The captured window is no longer available")?;
    let pid = object.get("pid").and_then(Value::as_u64);
    if object.get("pid").is_some_and(|value| !value.is_null() && pid != Some(0) && pid != Some(source_pid)) { return Err("The captured window is no longer available".into()); }
    let action = object.get("action").and_then(Value::as_str).unwrap_or("status");
    if action == "status" {
        let candidates = current.iter().filter(|window| {
            let pair = &window["pair"];
            window["id"].as_u64() != Some(source_id) && pair["eligible"].as_bool() == Some(true)
                && !active_pair(&pair["id"]) && !active_pair(&pair["survivor"])
                && !(window["app"] == "org.quickshell" && matches!(window["title"].as_str(), Some("Paired Windows" | "Window Overview" | "Quick Look")))
        }).cloned().collect::<Vec<_>>();
        return Ok(json!({"source":source,"candidates":candidates}));
    }
    if pid != Some(source_pid) { return Err("The captured window identity changed".into()); }
    let command = match action {
        "create" | "replace" => {
            let candidate_id = identity(object.get("partner"))?;
            let candidate = current.iter().find(|window| window["id"].as_u64() == Some(candidate_id) && window["pid"].as_u64() == object.get("partnerPid").and_then(Value::as_u64));
            if candidate.is_none() || candidate_id == source_id { return Err("The selected partner is no longer available".into()); }
            if action == "replace" { format!("lumen_pair replace {source_id} {candidate_id}") }
            else {
                match object.get("side").and_then(Value::as_str) {
                    Some("left") => format!("lumen_pair create {source_id} {candidate_id}"),
                    Some("right") => format!("lumen_pair create {candidate_id} {source_id}"),
                    _ => return Err("Choose the left or right side".into()),
                }
            }
        }
        "swap" | "leave" => format!("lumen_pair {action} {source_id}"),
        "ratio" => {
            let ratio = object.get("ratio").and_then(Value::as_f64).filter(|value| value.is_finite() && *value > 0.0 && *value < 1.0).ok_or("Choose a valid division")?;
            format!("lumen_pair ratio {source_id} {ratio}")
        }
        _ => return Err("Unknown paired placement action".into()),
    };
    run(&[&command])?;
    Ok(json!({"ok":true}))
}

fn main() {
    let result = (|| -> Result<Value, String> {
        let mut bytes = Vec::new();
        std::io::stdin().lock().take(1024*1024).read_until(b'\n', &mut bytes).map_err(|error| error.to_string())?;
        handle(&serde_json::from_slice(&bytes).map_err(|error| error.to_string())?)
    })();
    match result { Ok(value) => println!("{value}"), Err(error) => println!("{}", json!({"error":error})) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_ids() { for value in [json!(true),json!(0),json!(-1),json!("10; exit"),json!(1_u64<<53)] { assert!(identity(Some(&value)).is_err()); } }
}
