//! Actual process witnesses and bounded preview inventory acceptance.
use lumen_application_lifecycle::{
    Lifecycle,
    inventory::{MAX_VIEWS, valid_witness},
};
use serde_json::{Value, json};
use std::process::{Child, Command};
const EPOCH: &str = "0123456789abcdef01234567";
struct Owners(Vec<Child>);
impl Drop for Owners {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn owners() -> Owners {
    Owners(
        (0..2)
            .map(|_| Command::new("sleep").arg("30").spawn().unwrap())
            .collect(),
    )
}
fn node(id: i64, pid: u32, app: &str) -> Value {
    json!({"id":id,"pid":pid,"app_id":app,"shell":"xdg_shell","window":null,"foreign_toplevel_identifier":format!("surface-{id}"),"focused":id==1,"lumen_relationships":{"version":1,"parent_supported":true,"parent":null,"modal":null}})
}
fn native(owners: &Owners) -> Vec<Value> {
    vec![
        node(1, owners.0[0].id(), "app.a"),
        node(2, owners.0[1].id(), "app.b"),
    ]
}
fn snapshot(lifecycle: &Lifecycle) -> Value {
    lifecycle
        .native_observation()
        .expect("complete immutable native observation")
}
fn legacy(id: i64, pid: u32) -> Value {
    json!({"id":id,"pid":pid,"app_id":null,"shell":"xwayland","window":1000+id,"foreign_toplevel_identifier":format!("legacy-{id}"),"focused":false,"window_properties":{"class":"Legacy","instance":"legacy","window_role":"dialog","transient_for":null},"lumen_relationships":{"version":1,"parent_supported":false,"parent":null,"modal":null}})
}
#[test]
fn one_snapshot_includes_unsupported_and_owned_managed_views() {
    let owners = owners();
    let mut nodes = native(&owners);
    nodes.push(legacy(3, owners.0[0].id()));
    nodes.push(node(4, std::process::id(), "org.quickshell"));
    let mut lifecycle = Lifecycle::default();
    lifecycle.bind_shell_owner(std::process::id());
    lifecycle.refresh(EPOCH, &nodes);
    let observed = snapshot(&lifecycle);
    assert!(valid_witness(&observed["inventoryWitness"]));
    assert_eq!(observed["inventoryWitness"]["count"], 4);
    assert_eq!(observed["captures"].as_array().unwrap().len(), 2);
    assert_eq!(observed["source"]["conId"], 1);
    assert!(!lifecycle.native_coverage_complete(&nodes));
    for capture in observed["captures"].as_array().unwrap() {
        assert_eq!(capture.as_object().unwrap().len(), 6);
        assert_eq!(capture["epoch"], observed["epoch"]);
    }
    assert!(
        lifecycle
            .require_inventory(&observed["inventoryWitness"])
            .is_ok()
    );
}
#[test]
fn presentation_geometry_focus_and_reordering_are_not_membership() {
    let owners = owners();
    let mut nodes = native(&owners);
    let mut lifecycle = Lifecycle::default();
    lifecycle.refresh(EPOCH, &nodes);
    let old = snapshot(&lifecycle);
    nodes[0]["name"] = json!("a different mutable caption");
    nodes[0]["title"] = json!("another caption");
    nodes[0]["rect"] = json!({"width":900});
    nodes[0]["focused"] = json!(false);
    nodes[1]["focused"] = json!(true);
    nodes.reverse();
    lifecycle.refresh(EPOCH, &nodes);
    lifecycle.observe_focus(&nodes);
    let current = snapshot(&lifecycle);
    assert_eq!(old["inventoryWitness"], current["inventoryWitness"]);
    assert_ne!(old["source"], current["source"]);
    assert!(
        lifecycle
            .require_inventory(&old["inventoryWitness"])
            .is_ok()
    );
    assert!(!lifecycle.source_focus_matches(&old["source"]));
}
#[test]
fn observed_membership_aba_never_recovers_the_old_revision() {
    let owners = owners();
    let original = native(&owners);
    let mut lifecycle = Lifecycle::default();
    lifecycle.refresh(EPOCH, &original);
    let old = snapshot(&lifecycle);
    let mut changed = original.clone();
    changed.push(legacy(3, owners.0[1].id()));
    lifecycle.refresh(EPOCH, &changed);
    assert!(
        lifecycle
            .require_inventory(&old["inventoryWitness"])
            .is_err()
    );
    lifecycle.refresh(EPOCH, &original);
    let current = snapshot(&lifecycle);
    assert_eq!(
        old["inventoryWitness"]["digest"],
        current["inventoryWitness"]["digest"]
    );
    assert_ne!(
        old["inventoryWitness"]["revision"],
        current["inventoryWitness"]["revision"]
    );
    assert!(
        lifecycle
            .require_inventory(&old["inventoryWitness"])
            .is_err()
    );
}
#[test]
fn relevant_event_invalidates_even_when_the_next_sample_is_identical() {
    let owners = owners();
    let nodes = native(&owners);
    let mut lifecycle = Lifecycle::default();
    lifecycle.refresh(EPOCH, &nodes);
    let old = snapshot(&lifecycle);
    for change in ["focus", "title", "move"] {
        lifecycle.inventory.observe_event(&json!({"change":change}));
    }
    assert_eq!(
        old["inventoryWitness"],
        snapshot(&lifecycle)["inventoryWitness"]
    );
    for change in ["new", "close", "app_id", "lumen_relationships"] {
        lifecycle.inventory.observe_event(&json!({"change":change}));
        lifecycle.refresh(EPOCH, &nodes);
        assert!(
            lifecycle
                .require_inventory(&old["inventoryWitness"])
                .is_err()
        );
    }
}
#[test]
fn raw_grouping_and_modal_policy_changes_refuse_the_preview() {
    let owners = owners();
    let mut original = native(&owners);
    original.push(legacy(3, owners.0[1].id()));
    let mutations = [
        ("app_id", json!("new.app")),
        ("pid", json!(owners.0[0].id())),
        ("foreign_toplevel_identifier", json!("replacement-id")),
        ("shell", json!("unknown-shell")),
        ("window", json!(1234)),
        (
            "window_properties",
            json!({"class":"Changed","instance":"new","transient_for":1}),
        ),
        (
            "lumen_relationships",
            json!({"version":1,"parent_supported":true,"parent":1,"modal":true}),
        ),
    ];
    for (field, value) in mutations {
        let mut lifecycle = Lifecycle::default();
        lifecycle.refresh(EPOCH, &original);
        let old = snapshot(&lifecycle);
        let mut changed = original.clone();
        changed[2][field] = value;
        lifecycle.refresh(EPOCH, &changed);
        assert!(
            lifecycle
                .require_inventory(&old["inventoryWitness"])
                .is_err(),
            "{field}"
        );
    }
}
#[test]
fn process_advertisement_changes_and_real_exit_are_visible() {
    let mut owners = owners();
    let nodes = native(&owners);
    let mut lifecycle = Lifecycle::default();
    lifecycle.refresh(EPOCH, &nodes);
    let old = snapshot(&lifecycle);
    lifecycle
        .processes
        .get_mut(&owners.0[1].id())
        .unwrap()
        .apps
        .insert("new.advertised.app".into());
    assert!(
        lifecycle
            .require_inventory(&old["inventoryWitness"])
            .is_err()
    );
    let before_exit = snapshot(&lifecycle);
    owners.0[1].kill().unwrap();
    owners.0[1].wait().unwrap();
    lifecycle.prune();
    let after = snapshot(&lifecycle);
    assert_ne!(before_exit["inventoryWitness"], after["inventoryWitness"]);
    assert_eq!(after["captures"].as_array().unwrap().len(), 1);
}
#[test]
fn unsupported_source_is_explicit_in_the_same_snapshot() {
    let owners = owners();
    let mut nodes = native(&owners);
    nodes[0]["focused"] = json!(false);
    let mut other = legacy(3, owners.0[0].id());
    other["focused"] = json!(true);
    nodes.push(other);
    let mut lifecycle = Lifecycle::default();
    lifecycle.refresh(EPOCH, &nodes);
    let observed = snapshot(&lifecycle);
    assert_eq!(observed["source"]["kind"], "unsupported");
    assert_eq!(observed["captures"].as_array().unwrap().len(), 2);
    assert!(!lifecycle.source_focus_matches(&observed["source"]));
}
#[test]
fn worker_restart_and_session_change_cannot_reuse_a_witness() {
    let owners = owners();
    let nodes = native(&owners);
    let mut first = Lifecycle::default();
    first.refresh(EPOCH, &nodes);
    let old = snapshot(&first);
    let mut second = Lifecycle::default();
    second.refresh(EPOCH, &nodes);
    assert!(second.require_inventory(&old["inventoryWitness"]).is_err());
    first.refresh("abcdef0123456789abcdef01", &nodes);
    assert!(first.require_inventory(&old["inventoryWitness"]).is_err());
    first.close();
    assert!(first.native_observation().is_none());
}
#[test]
fn malformed_duplicate_and_oversized_views_disable_observation() {
    let owners = owners();
    let original = native(&owners);
    let mut malformed = original.clone();
    malformed[0]["lumen_relationships"]["modal"] = json!("yes");
    let mut duplicate = original.clone();
    duplicate[1]["foreign_toplevel_identifier"] =
        duplicate[0]["foreign_toplevel_identifier"].clone();
    let mut invalid_text = original.clone();
    invalid_text[1]["app_id"] = json!("x".repeat(257));
    let oversized = (0..=MAX_VIEWS)
        .map(|id| node(id as i64 + 1, owners.0[0].id(), "app.a"))
        .collect::<Vec<_>>();
    for nodes in [malformed, duplicate, invalid_text, oversized] {
        let mut lifecycle = Lifecycle::default();
        lifecycle.refresh(EPOCH, &nodes);
        assert!(lifecycle.native_observation().is_none());
    }
}
#[test]
fn strict_witness_refuses_partial_unknown_or_changed_requests() {
    let owners = owners();
    let nodes = native(&owners);
    let mut lifecycle = Lifecycle::default();
    lifecycle.refresh(EPOCH, &nodes);
    let old = snapshot(&lifecycle)["inventoryWitness"].clone();
    for field in [
        "version", "observer", "epoch", "digest", "count", "revision",
    ] {
        let mut altered = old.clone();
        altered.as_object_mut().unwrap().remove(field);
        assert!(!valid_witness(&altered));
        assert!(lifecycle.require_inventory(&altered).is_err());
    }
    let mut extra = old.clone();
    extra["unknown"] = json!(true);
    assert!(!valid_witness(&extra));
    let mut changed = old;
    changed["digest"] = json!("0".repeat(64));
    assert!(valid_witness(&changed));
    assert!(lifecycle.require_inventory(&changed).is_err());
}
#[test]
fn bounded_view_count_does_not_bypass_aggregate_byte_budget() {
    let owners = owners();
    let text = "x".repeat(256);
    let nodes = (0..MAX_VIEWS)
        .map(|id| {
            let mut value = node(id as i64 + 1, owners.0[0].id(), &text);
            value["foreign_toplevel_identifier"] = json!(format!("{id:04}{}", "z".repeat(252)));
            value["window_properties"] = json!({"class":text,"instance":text,"window_role":text});
            value
        })
        .collect::<Vec<_>>();
    assert_eq!(nodes.len(), MAX_VIEWS);
    let mut lifecycle = Lifecycle::default();
    lifecycle.refresh(EPOCH, &nodes);
    assert!(lifecycle.native_observation().is_none());
}
