//! Captured desktop transfer. No executable strings cross the native boundary.
use crate::Result;
use lumen_window_policy::guard;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const MAX_WIRE: usize = 2048;
fn fields(value: &Value, names: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or("Expected captured transfer object")?;
    if object.len() != names.len() || names.iter().any(|name| !object.contains_key(*name)) {
        return Err("Unknown or missing captured transfer field".into());
    }
    Ok(())
}
fn id(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
        .ok_or("Invalid captured native identity".into())
}
fn text(value: &Value, max: usize) -> Result<&str> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(|c| c.is_control()))
        .ok_or("Invalid captured transfer text".into())
}
fn hex(value: &str, len: usize) -> Result<Vec<u8>> {
    if value.len() != len * 2
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid captured transfer digest".into());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|p| {
            u8::from_str_radix(std::str::from_utf8(p).map_err(|e| e.to_string())?, 16)
                .map_err(|e| e.to_string())
        })
        .collect()
}
fn walk<'a>(
    node: &'a Value,
    workspace: Option<&'a Value>,
    output: Option<&'a Value>,
    found: &mut Vec<(&'a Value, Option<&'a Value>, Option<&'a Value>)>,
) {
    let workspace = if node["type"] == "workspace" {
        Some(node)
    } else {
        workspace
    };
    let output = if node["type"] == "output" {
        Some(node)
    } else {
        output
    };
    found.push((node, workspace, output));
    for key in ["nodes", "floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            walk(child, workspace, output, found);
        }
    }
}
fn fullscreen_in_path(node: &Value, wanted: &Value, inherited: bool) -> Option<bool> {
    let fullscreen = inherited
        || (matches!(node["type"].as_str(), Some("con" | "floating_con"))
            && node["fullscreen_mode"] != 0);
    if node["id"] == *wanted {
        return Some(fullscreen);
    }
    for key in ["nodes", "floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            if let Some(found) = fullscreen_in_path(child, wanted, fullscreen) {
                return Some(found);
            }
        }
    }
    None
}
fn contains_fullscreen(node: &Value) -> bool {
    (matches!(node["type"].as_str(), Some("con" | "floating_con")) && node["fullscreen_mode"] != 0)
        || ["nodes", "floating_nodes"].iter().any(|key| {
            node[*key]
                .as_array()
                .into_iter()
                .flatten()
                .any(contains_fullscreen)
        })
}
pub fn snapshot(tree: &Value) -> Result<Value> {
    // The shared traversal validates bytes/nodes/depth before recursive walking.
    let digest = Sha256::digest(topology(tree)?);
    if tree["space_transfer_guard"] != 3
        || tree["window_groups"]["locked"] != false
        || tree["display_spaces"]["available"] != true
    {
        return Err("Captured desktop transfer requires an unlocked supported session".into());
    }
    let session = text(&tree["display_spaces"]["session"], 32)?;
    hex(session, 16)?;
    let generation = text(&tree["display_spaces"]["generation"], 20)?;
    if generation.parse::<u64>().is_err() {
        return Err("Invalid native desktop generation".into());
    }
    Ok(json!({"digest":format!("{digest:x}"),"session":session,"generation":generation}))
}
fn topology(tree: &Value) -> Result<Vec<u8>> {
    // Validate the complete raw input before cloning. Only native floating
    // stacking order is irrelevant to an exact captured transfer; focus order,
    // tiling order and all other facts retain the shared codec's semantics.
    guard::canonical_topology(tree)?;
    fn normalize(
        node: &mut Value,
        depth: usize,
        ids: &mut std::collections::HashSet<u64>,
    ) -> Result<()> {
        if depth > 64 || !node.is_object() || ids.len() >= 8192 || !ids.insert(id(&node["id"])?) {
            return Err("Invalid or duplicate native transfer topology identity".into());
        }
        node.as_object_mut().unwrap().remove("lumen_transfer_frame");
        if node["pid"].as_i64().is_some_and(|pid| pid > 0) {
            let mut frame = Vec::new();
            for (index, key) in ["x", "y", "width", "height"].into_iter().enumerate() {
                let value = node["rect"][key]
                    .as_i64()
                    .filter(|value| i32::try_from(*value).is_ok() && (index < 2 || *value >= 0))
                    .ok_or("Invalid native transfer frame")?;
                frame.push(json!(value));
            }
            node["lumen_transfer_frame"] = json!(frame);
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(value) = node.get_mut(key) {
                let children = value
                    .as_array_mut()
                    .ok_or("Invalid native transfer child list")?;
                if children.len() > 8192 {
                    return Err("Native transfer child limit".into());
                }
                for child in children.iter_mut() {
                    normalize(child, depth + 1, ids)?;
                }
                if key == "floating_nodes" {
                    children.sort_by_key(|child| child["id"].as_u64().unwrap());
                }
            }
        }
        Ok(())
    }
    let mut normalized = tree.clone();
    normalize(&mut normalized, 0, &mut std::collections::HashSet::new())?;
    guard::canonical_topology(&normalized)
}
pub fn source(tree: &Value, wanted: &Value) -> Result<Value> {
    source_family(tree, wanted).map(|(capture, _)| capture)
}
fn managed_role(node: &Value) -> bool {
    node["shell"] == "xdg_shell"
        || (node["shell"] == "xwayland"
            && node["window"].as_u64().is_some_and(|id| id > 0 && id <= u32::MAX.into()))
}
pub fn source_family(tree: &Value, wanted: &Value) -> Result<(Value, Vec<Value>)> {
    topology(tree)?;
    let mut all = Vec::new();
    walk(tree, None, None, &mut all);
    let (node, workspace, output) = all
        .iter()
        .copied()
        .find(|(node, _, _)| node["id"] == *wanted)
        .ok_or("Captured window retired")?;
    let workspace = workspace.ok_or("Window has no ordinary desktop")?;
    let output = output.ok_or("Window has no output")?;
    let relation = &node["lumen_relationships"];
    let state = &node["lumen_state"];
    let group_hidden = state["group_hidden"] != false;
    if !managed_role(node)
        || lumen_window_policy::projection::parse(node)?.is_some_and(|p| p.enabled)
        || node["sticky"] != false
        || node["fullscreen_mode"] != 0
        || fullscreen_in_path(tree, wanted, false) != Some(false)
        || workspace["name"] == "__i3_scratch"
        || relation["parent_supported"] != true
        || !relation["parent"].is_null()
        || relation["version"] != 1
        || !(relation["modal"].is_null() || relation["modal"].is_boolean())
        || relation["modal"] == true
        || group_hidden
        || !node["lumen_pair"]["id"].is_null()
        || node["lumen_pair"]["survivor"] == true
        || state["pair_owned"] != false
        || state["hidden"] != false
        || state["minimised"] != false
        || state["filled"] != false
        || state["desktop_hidden"] != false
        || !state["restore_workspace"].is_null()
    {
        return Err("This window retains its current desktop while owned, group-hidden, hidden, filled, paired or full screen".into());
    }
    let rectangle = ["x", "y", "width", "height"].map(|key| node["rect"][key].clone());
    let captured = json!({"id":node["id"],"pid":node["pid"],"foreign":node["foreign_toplevel_identifier"],"workspace":workspace["id"],"output":output["id"],"rect":rectangle});
    validate_source(&captured)?;
    let mut family = vec![(node, 0usize)];
    let mut next = 0;
    while next < family.len() {
        let (parent, depth) = family[next];
        for (child, child_workspace, child_output) in &all {
            let validated_parent = child["lumen_relationships"]["parent"] == parent["id"];
            // An unsupported X11 edge cannot authorize membership. Its raw XID
            // observation can only refuse an otherwise misleading transfer offer.
            let unsupported_x11_edge = child["shell"] == "xwayland"
                && parent["shell"] == "xwayland"
                && parent["window"].as_u64().is_some_and(|id| id > 0 && id <= u32::MAX.into())
                && child["lumen_relationships"]["parent_supported"] != true
                && child["window_properties"]["transient_for"] == parent["window"];
            if unsupported_x11_edge {
                return Err("An unsupported owned window retains this desktop".into());
            }
            if !validated_parent {
                continue;
            }
            if family.len() >= 16
                || depth >= 15
                || family.iter().any(|(member, _)| member["id"] == child["id"])
            {
                return Err("Window family exceeds the captured transfer bound".into());
            }
            let relation = &child["lumen_relationships"];
            let state = &child["lumen_state"];
            if !matches!(child["floating"].as_str(), Some("user_on" | "auto_on"))
                || child_workspace.is_none_or(|owner| owner["id"] != workspace["id"])
                || child_output.is_none_or(|owner| owner["id"] != output["id"])
                || !managed_role(child)
                || lumen_window_policy::projection::parse(child)?.is_some_and(|p| p.enabled)
                || child["sticky"] != false
                || child["fullscreen_mode"] != 0
                || fullscreen_in_path(tree, &child["id"], false) != Some(false)
                || relation["version"] != 1
                || relation["parent_supported"] != true
                || !(relation["modal"].is_null() || relation["modal"].is_boolean())
                || relation["modal"] == true
                || state["group_hidden"] != false
                || state["hidden"] != false
                || state["minimised"] != false
                || state["filled"] != false
                || state["desktop_hidden"] != false
                || !state["restore_workspace"].is_null()
                || !child["lumen_pair"]["id"].is_null()
                || child["lumen_pair"]["survivor"] == true
                || state["pair_owned"] != false
            {
                return Err("This window family retains its desktop while a member is modal, tiled, hidden, saved or paired".into());
            }
            let child_capture = json!({"id":child["id"],"pid":child["pid"],"foreign":child["foreign_toplevel_identifier"],"workspace":workspace["id"],"output":output["id"],"rect":[child["rect"]["x"],child["rect"]["y"],child["rect"]["width"],child["rect"]["height"]]});
            validate_source(&child_capture)?;
            family.push((child, depth + 1));
        }
        next += 1;
    }
    Ok((
        captured,
        family
            .into_iter()
            .map(|(member, _)| member.clone())
            .collect(),
    ))
}
pub fn destination(tree: &Value, desktop: &Value) -> Result<Value> {
    guard::canonical_topology(tree)?;
    let mut all = Vec::new();
    walk(tree, None, None, &mut all);
    let output = all
        .iter()
        .find(|(node, _, _)| node["type"] == "output" && node["name"] == desktop["output"])
        .map(|(node, _, _)| *node)
        .ok_or("Destination display retired")?;
    let live = all
        .iter()
        .find(|(node, _, _)| node["type"] == "workspace" && node["name"] == desktop["name"]);
    if live.is_some_and(|(node, _, owner)| {
        node["id"] != desktop["id"] || owner.is_none_or(|o| o["id"] != output["id"])
    }) {
        return Err("Destination desktop binding changed".into());
    }
    if live.is_none() && !desktop["id"].is_null() {
        return Err("Destination desktop retired".into());
    }
    if live.is_some_and(|(node, _, _)| contains_fullscreen(node)) {
        return Err("A full-screen container retains this destination desktop".into());
    }
    let captured = json!({"logicalId":desktop["logicalId"],"workspace":desktop["id"],"output":output["id"],"name":desktop["name"]});
    validate_destination(&captured)?;
    Ok(captured)
}
fn validate_source(source: &Value) -> Result<()> {
    fields(
        source,
        &["id", "pid", "foreign", "workspace", "output", "rect"],
    )?;
    id(&source["id"])?;
    id(&source["workspace"])?;
    id(&source["output"])?;
    source["pid"]
        .as_u64()
        .filter(|pid| *pid >= 2 && *pid <= i32::MAX as u64)
        .ok_or("Invalid captured process")?;
    text(&source["foreign"], 256)?;
    let rect = source["rect"]
        .as_array()
        .filter(|r| r.len() == 4)
        .ok_or("Invalid captured placement")?;
    if rect
        .iter()
        .any(|n| n.as_i64().is_none_or(|n| i32::try_from(n).is_err()))
        || rect[2].as_i64().unwrap_or(0) <= 0
        || rect[3].as_i64().unwrap_or(0) <= 0
    {
        return Err("Invalid captured placement".into());
    }
    Ok(())
}
fn validate_destination(destination: &Value) -> Result<()> {
    fields(destination, &["logicalId", "workspace", "output", "name"])?;
    text(&destination["logicalId"], 128)?;
    text(&destination["name"], 512)?;
    id(&destination["output"])?;
    if !destination["workspace"].is_null() {
        id(&destination["workspace"])?;
    }
    Ok(())
}
pub fn packet(snapshot: &Value, source: &Value, destination: &Value) -> Result<Vec<u8>> {
    fields(snapshot, &["digest", "session", "generation"])?;
    validate_source(source)?;
    validate_destination(destination)?;
    let mut wire = b"LST3".to_vec();
    wire.extend(hex(text(&snapshot["digest"], 64)?, 32)?);
    wire.extend(hex(text(&snapshot["session"], 32)?, 16)?);
    let generation = text(&snapshot["generation"], 20)?
        .parse::<u64>()
        .map_err(|_| "Invalid native desktop generation")?;
    wire.extend(generation.to_be_bytes());
    wire.extend(id(&source["id"])?.to_be_bytes());
    wire.extend((source["pid"].as_u64().unwrap() as u32).to_be_bytes());
    let foreign = text(&source["foreign"], 256)?;
    wire.extend((foreign.len() as u32).to_be_bytes());
    wire.extend(foreign.as_bytes());
    for key in ["workspace", "output"] {
        wire.extend(id(&source[key])?.to_be_bytes());
    }
    for n in source["rect"].as_array().unwrap() {
        wire.extend((n.as_i64().unwrap() as i32).to_be_bytes());
    }
    wire.extend(destination["workspace"].as_u64().unwrap_or(0).to_be_bytes());
    wire.extend(id(&destination["output"])?.to_be_bytes());
    let name = text(&destination["name"], 512)?;
    wire.extend((name.len() as u32).to_be_bytes());
    wire.extend(name.as_bytes());
    if wire.len() > MAX_WIRE {
        return Err("Captured transfer exceeds native wire budget".into());
    }
    Ok(wire)
}
pub fn command(snapshot: &Value, source: &Value, destination: &Value) -> Result<String> {
    Ok(format!(
        "lumen_space_transfer {}",
        packet(snapshot, source, destination)?
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_native_golden_and_malformed_identity_refusal() {
        let tree:Value=serde_json::from_str(r#"{"id":1,"type":"root","nodes":[{"id":4,"type":"workspace","focus":[9,8],"nodes":[{"id":10},{"id":11}],"floating_nodes":[{"id":9,"focused":false},{"id":8,"focused":true}]}]}"#).unwrap();
        let original = tree.clone();
        assert_eq!(
            format!("{:x}", Sha256::digest(topology(&tree).unwrap())),
            "861cab3b9443f4463db768e0e01dbe76425534531e1910500191634187f80850"
        );
        assert_eq!(
            tree, original,
            "Normalization must not mutate the caller's captured native tree"
        );
        let mut changed = tree.clone();
        changed["nodes"][0]["nodes"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert_ne!(
            topology(&tree).unwrap(),
            topology(&changed).unwrap(),
            "Tiling order remains bound"
        );
        for replacement in [
            json!({}),
            json!({"id":8}),
            json!({"id":10}),
            json!({"id":1.0}),
            json!({"id":9_007_199_254_740_992_u64}),
        ] {
            let mut bad = tree.clone();
            bad["nodes"][0]["floating_nodes"][0] = replacement;
            assert!(topology(&bad).is_err());
        }
        changed = tree.clone();
        changed["nodes"][0]["floating_nodes"][0]["focused"] = json!(true);
        assert_ne!(topology(&tree).unwrap(), topology(&changed).unwrap());
    }
    #[test]
    fn harmless_floating_raise_is_distinct_from_focus_or_structural_change() {
        let mut a = tree();
        let workspace = &mut a["nodes"][0]["nodes"][0];
        let first = workspace["nodes"][0].clone();
        let mut second = first.clone();
        second["id"] = json!(9);
        second["foreign_toplevel_identifier"] = json!("surface-b");
        workspace["floating_nodes"] = json!([second, first]);
        workspace["nodes"] = json!([]);
        workspace["focus"] = json!([9, 8]);
        let mut b = a.clone();
        b["nodes"][0]["nodes"][0]["floating_nodes"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert_ne!(
            guard::canonical_topology(&a).unwrap(),
            guard::canonical_topology(&b).unwrap(),
            "Existing focus guard must keep strict floating order"
        );
        assert_eq!(
            snapshot(&a).unwrap(),
            snapshot(&b).unwrap(),
            "Raising the already-focused Overview must not retire an otherwise identical transfer"
        );
        b["nodes"][0]["nodes"][0]["focus"] = json!([8, 9]);
        assert_ne!(snapshot(&a).unwrap(), snapshot(&b).unwrap());
    }
    fn tree() -> Value {
        json!({"type":"root","id":1,"space_transfer_guard":3,"display_spaces":{"available":true,"session":"ab".repeat(16),"generation":"3"},"window_groups":{"locked":false,"enabled":false,"groups":[]},"nodes":[{"type":"output","id":2,"name":"HEADLESS-1","nodes":[{"type":"workspace","id":4,"name":"one","nodes":[{"type":"con","id":8,"pid":19,"shell":"xdg_shell","app_id":"org.example.Editor","foreign_toplevel_identifier":"surface-a","sticky":false,"fullscreen_mode":0,"rect":{"x":3,"y":4,"width":320,"height":240},"lumen_relationships":{"version":1,"parent_supported":true,"parent":null,"modal":null},"lumen_pair":{"id":null,"survivor":false},"lumen_state":{"group_member":false,"group_hidden":false,"pair_owned":false,"hidden":false,"minimised":false,"filled":false,"desktop_hidden":false,"restore_workspace":null}}]}]}]})
    }
    #[test]
    fn native_frame_projection_golden_accepts_zero_and_overwrites_reserved_field() {
        let raw = json!({"id":1,"type":"con","pid":19,"rect":{"x":3,"y":4,"width":0,"height":0},"lumen_transfer_frame":"untrusted"});
        let digest = format!("{:x}", Sha256::digest(topology(&raw).unwrap()));
        assert_eq!(
            digest,
            "d827ac360164dfb9c15fb63c81809e6448f38e7428b2d120f33e622667f25e1e"
        );
        let mut clean = raw.clone();
        clean
            .as_object_mut()
            .unwrap()
            .remove("lumen_transfer_frame");
        assert_eq!(topology(&raw).unwrap(), topology(&clean).unwrap());
        clean["rect"]["x"] = json!(4);
        assert_ne!(topology(&raw).unwrap(), topology(&clean).unwrap());
        clean["rect"]["width"] = json!(-1);
        assert!(topology(&clean).is_err());
    }
    fn ordinary_family_tree() -> Value {
        let mut family = tree();
        let root = &mut family["nodes"][0]["nodes"][0]["nodes"][0];
        root["floating"] = json!("user_on");
        let mut child = root.clone();
        child["id"] = json!(9);
        child["foreign_toplevel_identifier"] = json!("surface-child");
        child["lumen_relationships"]["parent"] = json!(8);
        child["rect"]["x"] = json!(21);
        family["nodes"][0]["nodes"][0]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(child);
        family
    }
    #[test]
    fn managed_x11_transfer_uses_observed_owner_and_bounded_window_identity() {
        let mut tree = ordinary_family_tree();
        let members = tree["nodes"][0]["nodes"][0]["nodes"]
            .as_array_mut().unwrap();
        for (index, member) in members.iter_mut().enumerate() {
            member["shell"] = json!("xwayland");
            member["window"] = json!(100 + index);
            member["lumen_relationships"]["modal"] = json!(false);
        }
        assert_eq!(source_family(&tree, &json!(8)).unwrap().1.len(), 2);
        for invalid in [Value::Null, json!(0), json!(4294967296_u64)] {
            let mut rejected = tree.clone();
            rejected["nodes"][0]["nodes"][0]["nodes"][0]["window"] = invalid;
            assert!(source(&rejected, &json!(8)).is_err());
        }
        tree["nodes"][0]["nodes"][0]["nodes"][1]["lumen_relationships"]["parent_supported"] = json!(false);
        assert!(source(&tree, &json!(8)).is_err());
    }
    #[test]
    fn native_family_refusal_is_observed_even_when_unsupported_parent_is_redacted() {
        let mut tree = ordinary_family_tree();
        tree["nodes"][0]["nodes"][0]["nodes"][0]["shell"] = json!("xwayland");
        tree["nodes"][0]["nodes"][0]["nodes"][0]["window"] = json!(100);
        tree["nodes"][0]["nodes"][0]["nodes"][1]["shell"] = json!("xwayland");
        tree["nodes"][0]["nodes"][0]["nodes"][1]["window"] = json!(101);
        tree["nodes"][0]["nodes"][0]["nodes"][1]["window_properties"]["transient_for"] = json!(100);
        tree["nodes"][0]["nodes"][0]["nodes"][1]["lumen_relationships"]["parent"] = Value::Null;
        tree["nodes"][0]["nodes"][0]["nodes"][1]["lumen_relationships"]["parent_supported"] = json!(false);
        assert!(source(&tree, &json!(8)).is_err());
        tree["nodes"][0]["nodes"][0]["nodes"][1]["window_properties"]["transient_for"] = json!(999);
        assert!(source(&tree, &json!(8)).is_ok());
        tree["nodes"][0]["nodes"][0]["nodes"][1]["window_properties"]["transient_for"] = json!(100);
        tree["nodes"][0]["nodes"][0]["nodes"][1]["shell"] = json!("xdg_shell");
        assert!(source(&tree, &json!(8)).is_ok());
    }
    #[test]
    fn enabled_projection_retains_every_family_member_until_explicitly_cleared() {
        let metadata = json!({"version":1,"enabled":true,"supported":false,"projected":false,"reason":"relationship-unsupported","revision":"7","origin_workspace":{"id":"4","name":"one"},"effective_workspace":{"id":"4","name":"one"},"origin_output":{"id":"2","name":"HEADLESS-1"},"effective_output":{"id":"2","name":"HEADLESS-1"}});
        for member in [0, 1] {
            let mut tree = ordinary_family_tree();
            tree["nodes"][0]["nodes"][0]["nodes"][member]["lumen_all_desktops"] = metadata.clone();
            assert!(source(&tree, &json!(8)).is_err());
            tree["nodes"][0]["nodes"][0]["nodes"][member]["lumen_all_desktops"]["enabled"] = json!(false);
            assert!(source(&tree, &json!(8)).is_ok());
            tree["nodes"][0]["nodes"][0]["nodes"][member]["lumen_all_desktops"]["version"] = json!(2);
            assert!(source(&tree, &json!(8)).is_err());
        }
    }
    fn nonview_fullscreen_tree() -> Value {
        let mut tree = tree();
        let leaf = tree["nodes"][0]["nodes"][0]["nodes"][0].clone();
        tree["nodes"][0]["nodes"][0]["nodes"][0] =
            json!({"id":9,"type":"con","fullscreen_mode":1,"nodes":[leaf]});
        assert_eq!(
            tree["nodes"][0]["nodes"][0]["nodes"][0]["nodes"][0]["fullscreen_mode"],
            0
        );
        tree
    }
    #[test]
    fn workspace_fullscreen_label_is_not_a_container_fullscreen_ancestor() {
        let mut tree = tree();
        tree["nodes"][0]["nodes"][0]["fullscreen_mode"] = json!(1);
        assert!(source(&tree, &json!(8)).is_ok());
        assert!(
            destination(
                &tree,
                &json!({"logicalId":"desktop-one","id":4,"name":"one","output":"HEADLESS-1"})
            )
            .is_ok()
        );
    }
    #[test]
    fn nonview_fullscreen_ancestor_does_not_offer_source() {
        assert!(source(&nonview_fullscreen_tree(), &json!(8)).is_err());
    }
    #[test]
    fn nonview_fullscreen_target_does_not_offer_destination() {
        assert!(
            destination(
                &nonview_fullscreen_tree(),
                &json!({"logicalId":"desktop-one","id":4,"name":"one","output":"HEADLESS-1"})
            )
            .is_err()
        );
    }
    #[test]
    fn tiled_root_accepts_only_floating_ordinary_descendants() {
        let mut family = ordinary_family_tree();
        family["nodes"][0]["nodes"][0]["nodes"][0]["floating"] = json!("user_off");
        assert!(source(&family, &json!(8)).is_ok());
        family["nodes"][0]["nodes"][0]["nodes"][1]["floating"] = json!("user_off");
        assert!(source(&family, &json!(8)).is_err());
    }
    #[test]
    fn ordinary_nonmodal_floating_family_is_captured_with_child_geometry() {
        let mut family = ordinary_family_tree();
        assert!(source(&family, &json!(8)).is_ok());
        assert!(source(&family, &json!(9)).is_err());
        let before = snapshot(&family).unwrap();
        family["nodes"][0]["nodes"][0]["nodes"][1]["rect"]["x"] = json!(22);
        assert_ne!(before, snapshot(&family).unwrap());
        family["nodes"][0]["nodes"][0]["nodes"][1]["lumen_relationships"]["modal"] = json!(true);
        assert!(source(&family, &json!(8)).is_err());
        family["nodes"][0]["nodes"][0]["nodes"][1]["lumen_relationships"]["modal"] =
            json!("unknown");
        assert!(source(&family, &json!(8)).is_err());
        family["nodes"][0]["nodes"][0]["nodes"][1]["lumen_relationships"]["modal"] = json!(false);
        let child = family["nodes"][0]["nodes"][0]["nodes"][1].clone();
        for index in 0..14 {
            let mut extra = child.clone();
            extra["id"] = json!(10 + index);
            extra["foreign_toplevel_identifier"] = json!(format!("child-{index}"));
            family["nodes"][0]["nodes"][0]["nodes"]
                .as_array_mut()
                .unwrap()
                .push(extra);
        }
        assert!(source(&family, &json!(8)).is_ok());
        let mut extra = child;
        extra["id"] = json!(24);
        family["nodes"][0]["nodes"][0]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(extra);
        assert!(source(&family, &json!(8)).is_err());
    }
    #[test]
    fn visible_grouped_root_is_not_a_blanket_transfer_exclusion() {
        let mut grouped = tree();
        let state = &mut grouped["nodes"][0]["nodes"][0]["nodes"][0]["lumen_state"];
        state["group_member"] = json!(true);
        state["group_hidden"] = json!(false);
        assert!(source(&grouped, &json!(8)).is_ok());
        grouped["nodes"][0]["nodes"][0]["nodes"][0]["lumen_state"]["group_hidden"] = json!(true);
        assert!(source(&grouped, &json!(8)).is_err());
    }
    #[test]
    fn actual_native_null_dialog_is_independent_but_owned_or_masked_source_refuses() {
        let base = tree();
        assert!(snapshot(&base).is_ok());
        assert!(source(&base, &json!(8)).is_ok());
        let mut owned = base.clone();
        owned["nodes"][0]["nodes"][0]["nodes"][0]["app_id"] = json!("org.quickshell");
        assert!(source(&owned, &json!(8)).is_ok());
        for (section, key, value) in [
            ("lumen_relationships", "modal", json!(true)),
            ("lumen_relationships", "parent_supported", json!(false)),
            ("lumen_relationships", "parent", json!(7)),
            ("lumen_state", "group_hidden", json!(true)),
            ("lumen_state", "pair_owned", json!(true)),
            ("lumen_state", "hidden", json!(true)),
            ("lumen_state", "minimised", json!(true)),
            ("lumen_state", "filled", json!(true)),
            ("lumen_state", "desktop_hidden", json!(true)),
        ] {
            let mut changed = base.clone();
            changed["nodes"][0]["nodes"][0]["nodes"][0][section][key] = value;
            assert!(source(&changed, &json!(8)).is_err(), "{section}.{key}");
        }
        let mut changed = base.clone();
        changed["nodes"][0]["nodes"][0]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":9,"lumen_relationships":{"parent":8}}));
        assert!(source(&changed, &json!(8)).is_err());
    }
    #[test]
    fn captured_destination_requires_exact_native_slot_or_actual_absence() {
        let tree = tree();
        let live = json!({"logicalId":"logical","name":"one","id":4,"output":"HEADLESS-1"});
        assert!(destination(&tree, &live).is_ok());
        let mut absent = live.clone();
        absent["id"] = Value::Null;
        assert!(destination(&tree, &absent).is_err());
        absent["name"] = json!("reserved");
        assert!(destination(&tree, &absent).is_ok());
        let mut wrong = live.clone();
        wrong["id"] = json!(17);
        assert!(destination(&tree, &wrong).is_err());
        wrong["output"] = json!("disconnected");
        assert!(destination(&tree, &wrong).is_err());
    }
    fn values() -> (Value, Value, Value) {
        (
            json!({"digest":"00".repeat(32),"session":"ab".repeat(16),"generation":"3"}),
            json!({"id":8,"pid":19,"foreign":"surface-a","workspace":4,"output":2,"rect":[3,4,320,240]}),
            json!({"logicalId":"logical-a","workspace":9,"output":2,"name":"two"}),
        )
    }
    #[test]
    fn fixed_wire_is_bounded_and_contains_no_commands() {
        let (n, s, d) = values();
        let wire = packet(&n, &s, &d).unwrap();
        assert_eq!(&wire[..4], b"LST3");
        assert_eq!(
            wire.len(),
            4 + 32 + 16 + 8 + 8 + 4 + 4 + 9 + 8 + 8 + 16 + 8 + 8 + 4 + 3
        );
        assert_eq!(&wire[52..60], &3_u64.to_be_bytes());
        assert!(
            command(&n, &s, &d)
                .unwrap()
                .starts_with("lumen_space_transfer 4c535433")
        );
    }
    #[test]
    fn malformed_unknown_and_oversized_captures_refuse() {
        let (n, s, d) = values();
        for (key, value) in [
            ("id", json!(9_007_199_254_740_992_u64)),
            ("workspace", json!(9_007_199_254_740_992_u64)),
            ("pid", json!(0)),
            ("pid", json!(2147483648_u64)),
            ("foreign", json!("x".repeat(257))),
            ("rect", json!([0, 0, 0, 4])),
            ("extra", json!(true)),
        ] {
            let mut bad = s.clone();
            bad[key] = value;
            assert!(packet(&n, &bad, &d).is_err(), "{bad}");
        }
        for value in [json!("bad\nname"), json!("x".repeat(513)), json!(null)] {
            let mut bad = d.clone();
            bad["name"] = value;
            assert!(packet(&n, &s, &bad).is_err());
        }
        let mut bad = n.clone();
        bad["digest"] = json!("XX".repeat(32));
        assert!(packet(&bad, &s, &d).is_err());
    }
    #[test]
    fn absent_destination_has_explicit_zero_identity() {
        let (n, s, mut d) = values();
        d["workspace"] = Value::Null;
        assert!(packet(&n, &s, &d).is_ok());
        d["workspace"] = json!(0);
        assert!(packet(&n, &s, &d).is_err());
    }
}
