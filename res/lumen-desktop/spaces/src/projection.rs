//! Exact-instance ownership of native presentation. No structural moves or sticky claims.
use crate::{Policy, Result, process_instance};
use lumen_window_policy::projection::{Projection, parse};
use serde_json::{Value, json};
use std::collections::HashMap;
const LIMIT: usize = 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    Applied,
    Stale,
    Locked,
}
impl From<()> for CommandOutcome {
    fn from(_: ()) -> Self {
        Self::Applied
    }
}
pub fn checked_reply(code: Option<i32>, stdout: &[u8]) -> Result<CommandOutcome> {
    if stdout.len() > 65536 {
        return Err("Native projection reply exceeds limit".into());
    }
    let reply: Value =
        serde_json::from_slice(stdout).map_err(|_| "Invalid native projection reply")?;
    let row = reply
        .as_array()
        .filter(|rows| rows.len() == 1)
        .and_then(|rows| rows.first())
        .ok_or("Expected one native projection receipt")?;
    if code == Some(0) && row["success"] == true {
        return Ok(CommandOutcome::Applied);
    }
    if code == Some(2)
        && row["success"] == false
        && row["parse_error"] == true
        && row["error"] == "Projection witness unavailable"
    {
        return Ok(CommandOutcome::Stale);
    }
    if code == Some(2)
        && row["success"] == false
        && row["parse_error"] == true
        && row["error"] == "Projection unavailable while session locked"
    {
        return Ok(CommandOutcome::Locked);
    }
    Err("Native projection command failed".into())
}

pub fn uses_native(node: &Value) -> Result<bool> {
    let Some(p) = parse(node)? else {
        return Ok(false);
    };
    if node["shell"] == "xdg_shell" {
        return Ok(true);
    }
    if node["shell"] == "xwayland" && node["window"].as_u64().is_some_and(|id| id > 0) {
        // Only the exact older provider role discriminator selects legacy sticky.
        // Temporary eligibility changes never switch a modern view's backend.
        return Ok(!(!p.enabled
            && !p.supported
            && node["lumen_all_desktops"]["reason"] == "native-view-required"));
    }
    if !p.enabled && !p.supported {
        return Ok(false);
    }
    Err("Unexpected native projection role".into())
}
/// Legacy group ownership cannot use sticky. A verified native projection
/// capability keeps the group intact, including temporarily suspended members.
pub fn grouped_assignment_blocked(node: &Value) -> bool {
    node["_grouped"] == true && !uses_native(node).unwrap_or(false)
}
fn identity(node: &Value, epoch: &str) -> Option<Value> {
    let foreign = node["foreign_toplevel_identifier"].as_str()?;
    if foreign.len() != 32
        || !foreign
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut identity = process_instance(node, epoch)?;
    identity["foreign"] = json!(foreign);
    Some(identity)
}
fn request(node: &Value, metadata: &Projection, enabled: bool) -> Result<Option<String>> {
    let Some(origin) = &metadata.origin_workspace else {
        return Ok(None);
    };
    let Some(output) = &metadata.origin_output else {
        return Ok(None);
    };
    let id = node["id"]
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or("Invalid projection window")?;
    let pid = node["pid"]
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or("Invalid projection process")?;
    let foreign = node["foreign_toplevel_identifier"]
        .as_str()
        .filter(|v| {
            v.len() == 32
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("Invalid projection foreign identity")?;
    Ok(Some(format!(
        "lumen_projection {id} {foreign} {pid} {} {} {} {}",
        origin.id,
        output.id,
        metadata.revision,
        if enabled { "on" } else { "off" }
    )))
}
pub fn reconcile<R: Into<CommandOutcome>>(
    policy: &mut Policy,
    observed: &[Value],
    epoch: &str,
    command: &mut impl FnMut(&str) -> Result<R>,
) -> Result<()> {
    let existing = policy.state["managed_projection"].as_object();
    if existing.is_some_and(|map| map.len() > LIMIT) || observed.len() > LIMIT {
        return Err("Native projection inventory exceeds limit".into());
    }
    let before = existing.cloned().unwrap_or_default();
    let mut managed = before.clone();
    let live: HashMap<_, _> = observed
        .iter()
        .filter_map(|node| node["id"].as_u64().map(|id| (id.to_string(), node)))
        .collect();
    for (key, record) in &before {
        let Some(node) = live.get(key) else {
            managed.remove(key);
            continue;
        };
        if identity(node, epoch).as_ref() != Some(record) {
            managed.remove(key);
            continue;
        }
        let Some(metadata) = parse(node)? else {
            continue;
        };
        let app = Policy::application(node).unwrap_or_default();
        if policy.state["assignments"][&app]["kind"] != "all" {
            if metadata.enabled {
                // A hidden native scratchpad view has no structural origin. Retain
                // the exact ownership record until restoration permits checked clear.
                let Some(operation) = request(node, &metadata, false)? else {
                    continue;
                };
                if command(&operation)?.into() != CommandOutcome::Applied {
                    continue;
                }
            }
            managed.remove(key);
        } else if !metadata.enabled && !metadata.supported {
            // Keep ownership through native suspension; never move/ungroup/restore.
        }
    }
    for node in observed {
        if !uses_native(node)? {
            continue;
        }
        let app = Policy::application(node).unwrap_or_default();
        if policy.state["assignments"][&app]["kind"] != "all" {
            continue;
        }
        let Some(metadata) = parse(node)? else {
            continue;
        };
        if metadata.enabled || !metadata.supported {
            continue;
        }
        let Some(record) = identity(node, epoch) else {
            continue;
        };
        let Some(operation) = request(node, &metadata, true)? else {
            continue;
        };
        let key = node["id"]
            .as_u64()
            .ok_or("Invalid projection window")?
            .to_string();
        managed.insert(key, record);
        policy.state["managed_projection"] = json!(managed);
        policy.save()?; // durable claim precedes actuation, actual metadata confirms success
        let _outcome: CommandOutcome = command(&operation)?.into();
    }
    if managed != before {
        policy.state["managed_projection"] = json!(managed);
        policy.save()?;
    }
    Ok(())
}
pub fn membership(node: &Value) -> Result<Option<Value>> {
    if !uses_native(node)? {
        return Ok(None);
    }
    let Some(p) = parse(node)? else {
        return Ok(None);
    };
    Ok(Some(if !p.supported {
        json!({"status":"pending","reason":"Native presentation is suspended for this window’s current state"})
    } else if p.enabled {
        json!({"status":"active","reason":"Every desktop on this window’s current display; independent visibility masks still apply"})
    } else {
        json!({"status":"pending","reason":"Waiting for checked native presentation"})
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reconcile_membership;
    fn fixture() -> (std::path::PathBuf, Policy, Value) {
        let root =
            std::env::temp_dir().join(format!("lumen-projection-test-{}", uuid::Uuid::new_v4()));
        let policy = Policy::open(root.join("spaces.json")).unwrap();
        let node = json!({"id":42,"pid":std::process::id(),"app_id":"editor","shell":"xdg_shell","sticky":false,
            "foreign_toplevel_identifier":"0123456789abcdef0123456789abcdef",
            "lumen_relationships":{"parent_supported":true,"parent":null,"modal":null},
            "lumen_all_desktops":{"version":1,"enabled":false,"supported":true,"reason":null,"revision":"9007199254740993","projected":false,
                "origin_workspace":{"id":"2","name":"Origin"},"origin_output":{"id":"1","name":"HEADLESS-1"},
                "effective_workspace":{"id":"2","name":"Origin"},"effective_output":{"id":"1","name":"HEADLESS-1"}}});
        (root, policy, node)
    }
    fn tree(node: &Value) -> Value {
        json!({"nodes":[{"type":"workspace","name":"Origin","nodes":[node]}]})
    }
    #[test]
    fn unsupported_xwayland_metadata_preserves_legacy_sticky_status_and_ownership() {
        let (root, mut policy, mut node) = fixture();
        node["app_id"] = Value::Null;
        node["window"] = json!(2097153);
        node["window_properties"] = json!({"class":"editor"});
        node["shell"] = json!("xwayland");
        node["lumen_all_desktops"]["supported"] = json!(false);
        node["lumen_all_desktops"]["reason"] = json!("native-view-required");
        policy.assign("x11:editor", "all", None).unwrap();
        let make_tree = |node: &Value| json!({"nodes":[{"type":"workspace","name":"Origin","floating_nodes":[node]}]});
        let mut calls = Vec::new();
        reconcile_membership(&mut policy, &make_tree(&node), "session", |cmd| {
            calls.push(cmd.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, ["[con_id=42] sticky enable"]);
        assert!(policy.state["managed_projection"].is_null());
        node["sticky"] = json!(true);
        let observed = crate::windows(&make_tree(&node));
        assert_eq!(
            crate::membership(&policy, &observed[0], "session")["status"],
            "active"
        );
        calls.clear();
        policy.assign("x11:editor", "none", None).unwrap();
        reconcile_membership(&mut policy, &make_tree(&node), "session", |cmd| {
            calls.push(cmd.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, ["[con_id=42] sticky disable"]);
        policy.assign("x11:editor", "all", None).unwrap();
        calls.clear();
        reconcile_membership(&mut policy, &make_tree(&node), "session", |cmd| {
            calls.push(cmd.to_owned());
            Ok(())
        })
        .unwrap();
        policy.assign("x11:editor", "none", None).unwrap();
        reconcile_membership(&mut policy, &make_tree(&node), "session", |cmd| {
            calls.push(cmd.to_owned());
            Ok(())
        })
        .unwrap();
        assert!(calls.is_empty());
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn locked_observation_defers_all_writes_then_reconciles_fresh_witness() {
        let (root, mut policy, mut node) = fixture();
        policy.assign("wayland:editor", "all", None).unwrap();
        let before = policy.state.clone();
        crate::reconcile_membership_observed(
            &mut policy,
            &tree(&node),
            "session",
            &json!({"lumen_session_lock":{"available":true,"locked":true}}),
            |_| -> Result<()> { panic!("locked observation must issue no commands") },
        )
        .unwrap();
        assert_eq!(policy.state, before);
        node["lumen_all_desktops"]["revision"] = json!("9007199254740994");
        let mut commands = Vec::new();
        crate::reconcile_membership_observed(
            &mut policy,
            &tree(&node),
            "session",
            &json!({"lumen_session_lock":{"available":true,"locked":false}}),
            |op| {
                commands.push(op.to_owned());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(commands.len(), 1);
        assert!(commands[0].ends_with(" 9007199254740994 on"));
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn modern_x11_suspension_stays_native_and_owned_legacy_claim_migrates_fresh() {
        let (root, mut policy, mut node) = fixture();
        node["app_id"] = Value::Null;
        node["window"] = json!(2097153);
        node["window_properties"] = json!({"class":"editor"});
        node["shell"] = json!("xwayland");
        node["lumen_all_desktops"]["supported"] = json!(false);
        node["lumen_all_desktops"]["reason"] = json!("fullscreen-suspended");
        policy.assign("x11:editor", "all", None).unwrap();
        assert!(uses_native(&node).unwrap());
        reconcile_membership(&mut policy, &tree(&node), "session", |_| -> Result<()> {
            panic!("suspension must not enable sticky")
        })
        .unwrap();
        node["lumen_all_desktops"]["reason"] = json!("native-view-required");
        let legacy_tree = |node: &Value| json!({"nodes":[{"type":"workspace","name":"Origin","floating_nodes":[node]}]});
        let mut requests = Vec::new();
        reconcile_membership(&mut policy, &legacy_tree(&node), "session", |op| {
            requests.push(op.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(requests, ["[con_id=42] sticky enable"]);
        node["sticky"] = json!(true);
        node["lumen_all_desktops"]["reason"] = json!("native-sticky-owned-elsewhere");
        requests.clear();
        reconcile_membership(&mut policy, &legacy_tree(&node), "session", |op| {
            requests.push(op.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(requests, ["[con_id=42] sticky disable"]);
        assert!(policy.state["managed_sticky"]["42"].is_null());
        assert!(policy.state["managed_projection"]["42"].is_null());
        node["sticky"] = json!(false);
        node["lumen_all_desktops"]["supported"] = json!(true);
        node["lumen_all_desktops"]["reason"] = Value::Null;
        requests.clear();
        reconcile_membership(&mut policy, &legacy_tree(&node), "session", |op| {
            requests.push(op.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("lumen_projection 42 "));
        assert!(requests[0].ends_with(" on"));
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_enable_and_clear_keep_exact_claim_until_fresh_observation() {
        let (root, mut policy, mut node) = fixture();
        policy.assign("wayland:editor", "all", None).unwrap();
        let mut requests = Vec::new();
        reconcile_membership(&mut policy, &tree(&node), "session", |op| {
            requests.push(op.to_owned());
            Ok(CommandOutcome::Stale)
        })
        .unwrap();
        assert_eq!(requests.len(), 1);
        let claim = policy.state["managed_projection"]["42"].clone();
        assert!(claim.is_object());
        assert_eq!(membership(&node).unwrap().unwrap()["status"], "pending");
        node["lumen_all_desktops"]["enabled"] = json!(true);
        policy.assign("wayland:editor", "none", None).unwrap();
        requests.clear();
        reconcile_membership(&mut policy, &tree(&node), "session", |op| {
            requests.push(op.to_owned());
            Ok(CommandOutcome::Stale)
        })
        .unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].ends_with(" off"));
        assert_eq!(policy.state["managed_projection"]["42"], claim);
        reconcile_membership(&mut policy, &tree(&node), "session", |_| {
            Ok(CommandOutcome::Locked)
        })
        .unwrap();
        assert_eq!(policy.state["managed_projection"]["42"], claim);
        node["lumen_all_desktops"]["enabled"] = json!(false);
        reconcile_membership(
            &mut policy,
            &tree(&node),
            "session",
            |_| -> Result<CommandOutcome> { panic!("confirmed clear must not replay") },
        )
        .unwrap();
        assert!(policy.state["managed_projection"]["42"].is_null());
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn only_exact_single_native_witness_refusal_is_stale() {
        assert_eq!(
            checked_reply(Some(0), br#"[{"success":true}]"#).unwrap(),
            CommandOutcome::Applied
        );
        let stale =
            br#"[{"success":false,"parse_error":true,"error":"Projection witness unavailable"}]"#;
        assert_eq!(
            checked_reply(Some(2), stale).unwrap(),
            CommandOutcome::Stale
        );
        for (code, reply) in [
            (Some(0), stale.as_slice()),
            (Some(2), br#"[]"#.as_slice()),
            (
                Some(2),
                br#"[{"success":false,"error":"Projection witness unavailable"}]"#.as_slice(),
            ),
            (
                Some(2),
                br#"[{"success":false,"parse_error":true,"error":"Unknown/invalid command"}]"#
                    .as_slice(),
            ),
            (Some(124), br#""#.as_slice()),
        ] {
            assert!(checked_reply(code, reply).is_err());
        }
    }
    #[test]
    fn grouped_admission_requires_valid_native_role_capability() {
        let (root, policy, mut native) = fixture();
        native["_grouped"] = json!(true);
        assert!(!grouped_assignment_blocked(&native));
        native["lumen_all_desktops"]["supported"] = json!(false);
        native["lumen_all_desktops"]["reason"] = json!("temporarily suspended");
        assert!(!grouped_assignment_blocked(&native));
        native["lumen_all_desktops"]["version"] = json!(99);
        assert!(grouped_assignment_blocked(&native));
        native.as_object_mut().unwrap().remove("lumen_all_desktops");
        assert!(grouped_assignment_blocked(&native));
        native["_grouped"] = json!(false);
        assert!(!grouped_assignment_blocked(&native));
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn tiled_grouped_native_projection_claim_is_exact_and_reversible() {
        let (root, mut policy, mut node) = fixture();
        policy.assign("wayland:editor", "all", None).unwrap();
        let mut calls = Vec::new();
        reconcile_membership(&mut policy, &tree(&node), "session", |cmd| {
            calls.push(cmd.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            calls,
            [format!(
                "lumen_projection 42 0123456789abcdef0123456789abcdef {} 2 1 9007199254740993 on",
                std::process::id()
            )]
        );
        assert_eq!(
            policy.state["managed_projection"]["42"]["foreign"],
            node["foreign_toplevel_identifier"]
        );
        node["lumen_all_desktops"]["enabled"] = json!(true);
        calls.clear();
        assert_eq!(membership(&node).unwrap().unwrap()["status"], "active");
        reconcile_membership(&mut policy, &tree(&node), "session", |cmd| {
            calls.push(cmd.to_owned());
            Ok(())
        })
        .unwrap();
        assert!(calls.is_empty());
        policy.assign("wayland:editor", "none", None).unwrap();
        reconcile_membership(&mut policy, &tree(&node), "session", |cmd| {
            calls.push(cmd.to_owned());
            Ok(())
        })
        .unwrap();
        assert!(calls[0].ends_with(" off"));
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unowned_sticky_and_replaced_process_or_foreign_identity_are_untouched() {
        let (root, mut policy, mut node) = fixture();
        policy.assign("wayland:editor", "all", None).unwrap();
        node["sticky"] = json!(true);
        node["lumen_all_desktops"]["supported"] = json!(false);
        node["lumen_all_desktops"]["reason"] = json!("native-sticky-owned-elsewhere");
        reconcile_membership(&mut policy, &tree(&node), "session", |_| -> Result<()> {
            panic!("unowned sticky changed")
        })
        .unwrap();
        node["sticky"] = json!(false);
        node["lumen_all_desktops"]["supported"] = json!(true);
        node["lumen_all_desktops"]["reason"] = Value::Null;
        reconcile_membership(&mut policy, &tree(&node), "session", |_| Ok(())).unwrap();
        node["lumen_all_desktops"]["enabled"] = json!(true);
        node["foreign_toplevel_identifier"] = json!("ffffffffffffffffffffffffffffffff");
        policy.assign("wayland:editor", "none", None).unwrap();
        reconcile_membership(&mut policy, &tree(&node), "session", |_| -> Result<()> {
            panic!("replacement changed")
        })
        .unwrap();
        assert!(
            policy.state["managed_projection"]
                .as_object()
                .unwrap()
                .is_empty()
        );
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn owned_legacy_sticky_migration_precedes_projection_and_suspended_clear_waits() {
        let (root, mut policy, mut node) = fixture();
        policy.assign("wayland:editor", "all", None).unwrap();
        policy.state["managed_sticky"]["42"] = process_instance(&node, "session").unwrap();
        node["sticky"] = json!(true);
        node["lumen_all_desktops"]["supported"] = json!(false);
        node["lumen_all_desktops"]["reason"] = json!("native-sticky-owned-elsewhere");
        let mut calls = Vec::new();
        reconcile_membership(&mut policy, &tree(&node), "session", |cmd| {
            calls.push(cmd.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, ["[con_id=42] sticky disable"]);
        node["sticky"] = json!(false);
        node["lumen_all_desktops"]["supported"] = json!(true);
        node["lumen_all_desktops"]["reason"] = Value::Null;
        reconcile_membership(&mut policy, &tree(&node), "session", |_| Ok(())).unwrap();
        node["lumen_all_desktops"]["enabled"] = json!(true);
        node["lumen_all_desktops"]["supported"] = json!(false);
        node["lumen_all_desktops"]["reason"] = json!("origin-unavailable");
        for key in [
            "origin_workspace",
            "origin_output",
            "effective_workspace",
            "effective_output",
        ] {
            node["lumen_all_desktops"][key] = Value::Null;
        }
        policy.assign("wayland:editor", "none", None).unwrap();
        reconcile_membership(&mut policy, &tree(&node), "session", |_| -> Result<()> {
            panic!("hidden origin fabricated")
        })
        .unwrap();
        assert!(policy.state["managed_projection"]["42"].is_object());
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
}
