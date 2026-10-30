//! Versioned, bounded native guard wire format; no executable payload strings.
use crate::{Result, windows};
use serde_json::Value;
use sha2::{Digest, Sha256};
pub const MAX_BYTES: usize = 32 * 1024;
pub const MAX_TOPOLOGY_BYTES: usize = 1024 * 1024;
pub const MAX_DEPTH: usize = 64;
pub const MAX_ACTIONS: usize = 2048;
const OMIT: &[&str] = &[
    "rect",
    "deco_rect",
    "window_rect",
    "geometry",
    "representation",
    "title",
];
fn bounded(value: &Value) -> Result<()> {
    let mut pending = vec![(value, 0_usize)];
    let mut used = 0_usize;
    while let Some((value, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err("Guard topology exceeds depth limit".into());
        }
        let bytes = match value {
            Value::Null => 1,
            Value::Bool(_) => 2,
            Value::Number(_) => 9,
            Value::String(text) => 5_usize
                .checked_add(text.len())
                .ok_or("Guard size overflow")?,
            Value::Array(items) => {
                if items.len() > 8192 {
                    return Err("Guard array limit".into());
                }
                5 + items.len()
            }
            Value::Object(fields) => {
                if fields.len() > 8192 {
                    return Err("Guard object limit".into());
                }
                fields.keys().try_fold(5_usize, |size, key| {
                    size.checked_add(6 + key.len()).ok_or("Guard size overflow")
                })?
            }
        };
        used = used
            .checked_add(bytes)
            .filter(|used| *used <= MAX_TOPOLOGY_BYTES)
            .ok_or("Guard raw topology exceeds byte limit")?;
        match value {
            Value::Array(items) => pending.extend(items.iter().map(|v| (v, depth + 1))),
            Value::Object(fields) => pending.extend(fields.values().map(|v| (v, depth + 1))),
            _ => {}
        }
    }
    Ok(())
}
fn projection(value: &Value, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err("Guard topology exceeds depth limit".into());
    }
    Ok(match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| projection(item, depth + 1))
                .collect::<Result<_>>()?,
        ),
        Value::Object(fields) => {
            let named = matches!(
                value["type"].as_str(),
                Some("workspace" | "output" | "root")
            );
            Value::Object(
                fields
                    .iter()
                    .filter(|(key, _)| {
                        (!OMIT.contains(&key.as_str())
                            || (named
                                && ["rect", "deco_rect", "window_rect", "geometry"]
                                    .contains(&key.as_str())))
                            && (key.as_str() != "name" || named)
                    })
                    .map(|(key, value)| Ok((key.clone(), projection(value, depth + 1)?)))
                    .collect::<Result<_>>()?,
            )
        }
        value => value.clone(),
    })
}
fn size(out: &[u8]) -> Result<()> {
    if out.len() > MAX_TOPOLOGY_BYTES {
        Err("Guard exceeds byte limit".into())
    } else {
        Ok(())
    }
}
fn string(value: &str, out: &mut Vec<u8>) -> Result<()> {
    if value.contains('\0') {
        return Err("NUL is not supported in guard strings".into());
    }
    out.extend_from_slice(
        &u32::try_from(value.len())
            .map_err(|_| "Guard string exceeds limit")?
            .to_be_bytes(),
    );
    out.extend_from_slice(value.as_bytes());
    size(out)
}
fn canonical(value: &Value, out: &mut Vec<u8>, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err("Guard canonical depth limit".into());
    }
    match value {
        Value::Null => out.push(0),
        Value::Bool(value) => {
            out.push(1);
            out.push(u8::from(*value));
        }
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                out.push(2);
                out.extend_from_slice(&value.to_be_bytes());
            } else {
                if value.is_u64() {
                    return Err("Guard integer exceeds signed range".into());
                }
                let value = value
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or("Invalid guard number")?;
                out.push(3);
                out.extend_from_slice(&value.to_bits().to_be_bytes());
            }
        }
        Value::String(value) => {
            out.push(4);
            string(value, out)?;
        }
        Value::Array(items) => {
            out.push(5);
            out.extend_from_slice(
                &u32::try_from(items.len())
                    .map_err(|_| "Guard array exceeds limit")?
                    .to_be_bytes(),
            );
            for item in items {
                canonical(item, out, depth + 1)?;
            }
        }
        Value::Object(fields) => {
            out.push(6);
            out.extend_from_slice(
                &u32::try_from(fields.len())
                    .map_err(|_| "Guard object exceeds limit")?
                    .to_be_bytes(),
            );
            let mut keys = fields.keys().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                out.push(4);
                string(key, out)?;
                canonical(&fields[key], out, depth + 1)?;
            }
        }
    }
    size(out)
}
pub fn canonical_topology(tree: &Value) -> Result<Vec<u8>> {
    bounded(tree)?;
    let all = windows(tree);
    if all.len() > 8192
        || all
            .iter()
            .filter(|w| {
                !w.node["app_id"].is_null()
                    || !w.node["window"].is_null()
                    || w.node["shell"] == "xdg_shell"
            })
            .count()
            > 2048
    {
        return Err("Guard topology exceeds node limit".into());
    }
    let mut out = Vec::new();
    canonical(&projection(tree, 0)?, &mut out, 0)?;
    Ok(out)
}
fn witness(node: &Value, out: &mut Vec<u8>) -> Result<()> {
    let id = node["id"]
        .as_u64()
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .ok_or("Invalid guard container")?;
    let pid = node["pid"]
        .as_u64()
        .filter(|pid| *pid >= 2 && *pid <= i32::MAX as u64)
        .ok_or("Invalid guard process")?;
    let foreign = node["foreign_toplevel_identifier"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
        .ok_or("Invalid guard surface")?;
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&(pid as u32).to_be_bytes());
    string(foreign, out)
}
pub fn packet(tree: &Value, source: &Value, target: &Value, actions: &[Value]) -> Result<Vec<u8>> {
    if actions.len() > MAX_ACTIONS {
        return Err("Too many guard actions".into());
    }
    let topology = canonical_topology(tree)?;
    let all = windows(tree);
    let mut out = b"LFG2".to_vec();
    match source["kind"].as_str() {
        Some("none") => out.push(0),
        Some("native") => {
            out.push(1);
            let node = all
                .iter()
                .find(|w| {
                    w.node["id"] == source["conId"]
                        && w.node["foreign_toplevel_identifier"] == source["foreignId"]
                })
                .ok_or("Guard source disappeared")?;
            witness(node.node, &mut out)?;
        }
        _ => return Err("Guard source is unsupported".into()),
    }
    witness(target, &mut out)?;
    out.extend_from_slice(&Sha256::digest(&topology));
    out.extend_from_slice(&(actions.len() as u32).to_be_bytes());
    let mut unhidden = std::collections::HashSet::new();
    let mut phase = 0;
    for action in actions {
        let id = action["conId"]
            .as_i64()
            .ok_or("Invalid guard action target")?;
        let op = match action["op"].as_str() {
            Some("unhide") if phase == 0 && unhidden.insert(id) => 1,
            Some("show") if phase == 0 && target["id"] == id => {
                phase = 1;
                2
            }
            Some("focus") if phase < 2 && target["id"] == id => {
                phase = 2;
                3
            }
            _ => return Err("Invalid, duplicate or out-of-order guard action".into()),
        };
        out.push(op);
        let node = all
            .iter()
            .find(|w| w.node["id"] == id)
            .ok_or("Guard action disappeared")?;
        witness(node.node, &mut out)?;
    }
    if phase == 1 {
        return Err("Show requires a following focus action".into());
    }
    if out.len() > MAX_BYTES {
        return Err("Guard wire exceeds byte limit".into());
    }
    Ok(out)
}
pub fn command(packet: &[u8]) -> Result<String> {
    if packet.len() > MAX_BYTES {
        return Err("Guard wire exceeds byte limit".into());
    }
    Ok(format!(
        "lumen_focus_guard {}",
        packet
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn native_double_spelling_keeps_exact_canonical_bits() {
        // Exact C/json-c native output fraction, independently hashed by the
        // unchanged upstream C canonical encoder and SHA256 implementation.
        let tree: Value = serde_json::from_str(r#"{"percent":0.45454545454545453}"#).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(canonical_topology(&tree).unwrap())),
            "60ffe22922859c42204319180bea585e68227bb31dee3c816a16e8302f0f329a"
        );
    }
    #[test]
    fn canonical_golden_and_projection_bounds() {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(canonical_topology(&json!({})).unwrap())
            ),
            "b45482224b439a3d548c65378929b7dcc16a42288530b7b20d5c8103cc879d10"
        );
        let mut bytes = Vec::new();
        canonical(&json!({"z":true,"a":[null,-2,"x",1.5]}), &mut bytes, 0).unwrap();
        assert_eq!(
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "060000000204000000016105000000040002fffffffffffffffe040000000178033ff800000000000004000000017a0101"
        );
        let a = json!({"id":1,"type":"con","name":"one","representation":"one","rect":{"x":1},"lumen_state":{"hidden":false}});
        let mut b = a.clone();
        b["name"] = json!("two");
        b["rect"]["x"] = json!(9);
        b["representation"] = json!("two");
        assert_eq!(
            canonical_topology(&a).unwrap(),
            canonical_topology(&b).unwrap()
        );
        b["lumen_state"]["hidden"] = json!(true);
        assert_ne!(
            canonical_topology(&a).unwrap(),
            canonical_topology(&b).unwrap()
        );
        let mut deep = Value::Null;
        for _ in 0..66 {
            deep = json!([deep]);
        }
        assert!(canonical_topology(&deep).is_err());
        assert!(
            canonical_topology(&json!({"app_id":"x","payload":"x".repeat(MAX_TOPOLOGY_BYTES)}))
                .is_err()
        );
        assert!(canonical_topology(&json!({"id":u64::MAX})).is_err());
    }
    #[test]
    fn presentation_mode_and_revision_invalidate_captured_topology() {
        let before = json!({"window_groups":{"enabled":true,"revision":12,"window_mode":"all_together","groups":[]}});
        let mut mode = before.clone();
        mode["window_groups"]["window_mode"] = json!("one_at_a_time");
        assert_ne!(
            canonical_topology(&before).unwrap(),
            canonical_topology(&mode).unwrap()
        );
        let mut revision = before.clone();
        revision["window_groups"]["revision"] = json!(13);
        assert_ne!(
            canonical_topology(&before).unwrap(),
            canonical_topology(&revision).unwrap()
        );
    }
    #[test]
    fn writer_refuses_unknown_duplicate_and_out_of_order_actions() {
        let target = json!({"id":1,"pid":10,"foreign_toplevel_identifier":"surface","app_id":"x"});
        let tree = json!({"nodes":[target]});
        let source = json!({"kind":"none"});
        for actions in [
            json!([{"op":"shell","conId":1}]),
            json!([{"op":"unhide","conId":1},{"op":"unhide","conId":1}]),
            json!([{"op":"focus","conId":1},{"op":"show","conId":1}]),
            json!([{"op":"show","conId":1}]),
        ] {
            assert!(packet(&tree, &source, &target, actions.as_array().unwrap()).is_err());
        }
        assert!(packet(&tree, &json!({"kind":"unsupported"}), &target, &[]).is_err());
        assert!(
            packet(
                &tree,
                &source,
                &target,
                &vec![json!({"op":"unhide","conId":1}); MAX_ACTIONS + 1]
            )
            .is_err()
        );
        assert!(command(&vec![0; MAX_BYTES + 1]).is_err());
    }
}
