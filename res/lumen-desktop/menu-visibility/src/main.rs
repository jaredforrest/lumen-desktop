//! Read-only, output-scoped fullscreen observation from Sway IPC.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env,
    io::{Read, Write},
    os::unix::net::UnixStream,
    process::Command,
    time::Duration,
};

type Result<T> = std::result::Result<T, String>;

fn snapshot(tree: &Value, outputs: &Value) -> Result<BTreeMap<String, bool>> {
    let outputs = outputs.as_array().ok_or("Invalid compositor snapshot")?;
    if !tree.is_object() {
        return Err("Invalid compositor snapshot".into());
    }
    let active = outputs
        .iter()
        .filter(|row| row["active"] == true)
        .filter_map(|row| {
            Some((
                row["name"].as_str()?.to_owned(),
                row["current_workspace"].as_str().unwrap_or("").to_owned(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut result = active
        .keys()
        .map(|name| (name.clone(), false))
        .collect::<BTreeMap<_, _>>();
    fn walk(
        node: &Value,
        output: Option<&str>,
        workspace: Option<&str>,
        active: &BTreeMap<String, String>,
        result: &mut BTreeMap<String, bool>,
        global: &mut bool,
    ) -> Result<()> {
        if !node.is_object() {
            return Err("Invalid compositor node".into());
        }
        let output = if node["type"] == "output" {
            node["name"].as_str()
        } else {
            output
        };
        let workspace = if node["type"] == "workspace" {
            node["name"].as_str()
        } else {
            workspace
        };
        let mode = if node.get("fullscreen_mode").is_none() {
            0
        } else {
            node["fullscreen_mode"]
                .as_i64()
                .ok_or("Invalid fullscreen state")?
        };
        if ![0, 1, 2].contains(&mode) {
            return Err("Invalid fullscreen state".into());
        }
        if node["type"] == "con" || node["type"] == "floating_con" {
            if mode == 2 {
                *global = true;
            }
            if mode == 1 {
                if let Some((name, current)) = output.and_then(|name| active.get_key_value(name)) {
                    if workspace == Some(current.as_str()) {
                        result.insert(name.clone(), true);
                    }
                }
            }
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = node[key].as_array() {
                for child in children {
                    walk(child, output, workspace, active, result, global)?;
                }
            }
        }
        Ok(())
    }
    let mut global = false;
    walk(tree, None, None, &active, &mut result, &mut global)?;
    if global {
        result.values_mut().for_each(|value| *value = true);
    }
    Ok(result)
}
fn query(kind: &str) -> Result<Value> {
    let output = Command::new("timeout")
        .args(["4s", "swaymsg", "-r", "-t", kind])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Compositor query failed".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
fn packet(stream: &mut UnixStream) -> Result<(u32, Value)> {
    let mut header = [0u8; 14];
    stream.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..6] != b"i3-ipc" {
        return Err("Invalid compositor packet".into());
    }
    let length = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    let kind = u32::from_le_bytes(header[10..14].try_into().unwrap());
    if length > 16 * 1024 * 1024 {
        return Err("Invalid compositor packet".into());
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok((
        kind,
        serde_json::from_slice(&body).map_err(|e| e.to_string())?,
    ))
}
fn subscribe(stream: &mut UnixStream) -> Result<()> {
    let payload = b"[\"window\",\"workspace\",\"output\",\"shutdown\"]";
    stream.write_all(b"i3-ipc").map_err(|e| e.to_string())?;
    stream
        .write_all(&(payload.len() as u32).to_le_bytes())
        .map_err(|e| e.to_string())?;
    stream
        .write_all(&2u32.to_le_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(payload).map_err(|e| e.to_string())?;
    let (kind, ack) = packet(stream)?;
    if kind != 2 || ack["success"] != true {
        return Err("Subscription unavailable".into());
    }
    Ok(())
}
fn emit(
    token: &str,
    revision: &mut u64,
    available: bool,
    outputs: BTreeMap<String, bool>,
    error: &str,
) {
    *revision += 1;
    println!(
        "{}",
        json!({"transport":token,"revision":revision,"available":available,"outputs":outputs,"error":error})
    );
    let _ = std::io::stdout().flush();
}
fn watch(token: &str, revision: &mut u64) -> Result<()> {
    let socket = env::var("SWAYSOCK").map_err(|e| e.to_string())?;
    let mut stream = UnixStream::connect(socket).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .map_err(|e| e.to_string())?;
    subscribe(&mut stream)?;
    stream.set_read_timeout(None).map_err(|e| e.to_string())?;
    emit(
        token,
        revision,
        true,
        snapshot(&query("get_tree")?, &query("get_outputs")?)?,
        "",
    );
    loop {
        let (kind, _) = packet(&mut stream)?;
        if kind == 0x8000_0006 {
            break;
        }
        emit(
            token,
            revision,
            true,
            snapshot(&query("get_tree")?, &query("get_outputs")?)?,
            "",
        );
    }
    Ok(())
}
fn main() {
    let token = env::args().nth(1).unwrap_or_else(|| "0".into());
    let mut revision = 0;
    if let Err(error) = watch(&token, &mut revision) {
        eprintln!("{error}");
    }
    emit(
        &token,
        &mut revision,
        false,
        BTreeMap::new(),
        "Fullscreen menu observation is unavailable",
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(mode: Value) -> Value {
        json!({"nodes":[{"type":"output","name":"one","nodes":[{"type":"workspace","name":"active","fullscreen_mode":1,"nodes":[{"type":"con","fullscreen_mode":mode}]},{"type":"workspace","name":"hidden","nodes":[{"type":"con","fullscreen_mode":1}]}]},{"type":"output","name":"two","nodes":[{"type":"workspace","name":"other","fullscreen_mode":1,"nodes":[]}]}]})
    }
    fn outputs() -> Value {
        json!([{"name":"one","current_workspace":"active","active":true},{"name":"two","current_workspace":"other","active":true}])
    }
    #[test]
    fn only_active_workspace_is_fullscreen() {
        assert_eq!(
            snapshot(&fixture(json!(1)), &outputs()).unwrap(),
            BTreeMap::from([("one".into(), true), ("two".into(), false)])
        );
        assert_eq!(
            snapshot(&fixture(json!(0)), &outputs()).unwrap()["one"],
            false
        );
        assert_eq!(
            snapshot(&fixture(json!(2)), &outputs()).unwrap()["two"],
            true
        );
    }
    #[test]
    fn removed_output_and_invalid_mode() {
        assert_eq!(
            snapshot(
                &fixture(json!(1)),
                &json!([{"name":"two","current_workspace":"other","active":true}])
            )
            .unwrap(),
            BTreeMap::from([("two".into(), false)])
        );
        assert!(snapshot(&fixture(json!(true)), &outputs()).is_err());
    }
}
