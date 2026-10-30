use lumen_application_lifecycle::{Lifecycle, ObservationStore, windows::History};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Child, Command},
};
const EPOCH: &str = "0123456789abcdef01234567";
const BOOT: &str = "01234567-89ab-cdef-0123-456789abcdef";
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
fn nodes(owners: &Owners, focused: i64) -> Vec<Value> {
    [(11,0,"app"),(12,0,"app"),(13,1,"other")].into_iter().map(|(id,pid,app)|json!({"id":id,"type":"con","pid":owners.0[pid].id(),"app_id":app,"name":"identical","shell":"xdg_shell","window":null,"foreign_toplevel_identifier":format!("surface-{id}"),"focused":id==focused,"lumen_state":{"version":1,"hidden":false,"minimised":false,"restore_workspace":"one"},"lumen_relationships":{"version":1,"parent_supported":true,"parent":null,"modal":null}})).collect()
}
fn tree(nodes: &[Value]) -> Value {
    json!({"type":"root","nodes":[{"type":"workspace","name":"one","nodes":nodes}]})
}
fn capture(l: &Lifecycle, id: i64) -> Value {
    l.window_history
        .captures(l)
        .into_iter()
        .find(|c| c["conId"] == id)
        .unwrap()
}
struct Dir(std::path::PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("lumen-window-order-{}", std::process::id()));
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn exact_duplicate_title_window_order_reopens_and_reused_id_cannot_inherit() {
    let owners = owners();
    let directory = Dir::new();
    let mut lifecycle = Lifecycle::default();
    for id in [11, 12, 13] {
        let n = nodes(&owners, id);
        lifecycle.refresh(EPOCH, &n);
        lifecycle.observe_focus(&n);
    }
    let chosen = capture(&lifecycle, 12);
    let initial = lifecycle.current_source_focus.clone();
    assert_eq!(
        lifecycle
            .window_history
            .captures(&lifecycle)
            .iter()
            .map(|c| c["conId"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [13, 12, 11]
    );
    let mut store = ObservationStore::new(&directory.0, EPOCH, BOOT).unwrap();
    store
        .save_complete(
            &lifecycle.records(),
            &lifecycle.activation_history,
            &lifecycle.window_history,
        )
        .unwrap();
    drop(store);
    let mut store = ObservationStore::new(&directory.0, EPOCH, BOOT).unwrap();
    let saved = store.load_state();
    let mut reopened = Lifecycle::default();
    reopened.recover(EPOCH, &saved.processes);
    reopened.window_history = saved.windows;
    reopened.activation_history = saved.history;
    let current = nodes(&owners, 13);
    reopened.refresh(EPOCH, &current);
    assert_eq!(
        reopened.window_history.value(),
        lifecycle.window_history.value()
    );
    assert!(reopened.native_coverage_complete);
    let mut mixed = current.clone();
    mixed.push(
        json!({"id":14,"pid":owners.0[0].id(),"window":999,"app_id":null,"shell":"xwayland"}),
    );
    assert!(!reopened.native_coverage_complete(&mixed));
    assert!(reopened.native_focus_plan(&chosen, &tree(&mixed), true).is_err());
    let mut unregistered = current.clone();
    unregistered.push(json!({"id":15,"pid":owners.0[0].id(),"app_id":null,"shell":"xdg_shell","window":null,"foreign_toplevel_identifier":"unknown"}));
    assert!(!reopened.native_coverage_complete(&unregistered));
    assert_eq!(capture(&reopened, 12), chosen);
    assert!(reopened.source_focus_matches(&initial));
    let plan = reopened
        .native_focus_plan(&chosen, &tree(&current), true)
        .unwrap();
    assert_eq!(plan.command.as_deref(), Some("[con_id=12] focus"));
    let mut replacement = current.clone();
    replacement[1]["foreign_toplevel_identifier"] = json!("new-surface-same-pid-and-id");
    reopened.refresh(EPOCH, &replacement);
    assert!(
        reopened
            .native_focus_plan(&chosen, &tree(&replacement), true)
            .is_err()
    );
    let without = replacement
        .iter()
        .filter(|n| n["id"] != 12)
        .cloned()
        .collect::<Vec<_>>();
    reopened.refresh(EPOCH, &without);
    assert!(
        reopened
            .native_focus_plan(&chosen, &tree(&without), true)
            .is_err()
    );
    let changed_focus = nodes(&owners, 11);
    reopened.refresh(EPOCH, &changed_focus);
    assert!(!reopened.source_focus_matches(&initial));
    reopened.refresh("fedcba9876543210fedcba98", &changed_focus);
    assert!(
        reopened
            .native_focus_plan(&chosen, &tree(&changed_focus), true)
            .is_err()
    );
}
#[test]
fn native_capture_never_unhides_a_new_process_with_the_same_advertisement() {
    let owners = owners();
    let mut lifecycle = Lifecycle::default();
    let original = nodes(&owners, 13);
    lifecycle.refresh(EPOCH, &original);
    let chosen = capture(&lifecycle, 12);
    let mut expanded = original.clone();
    let mut extra = original[0].clone();
    extra["id"] = json!(20);
    extra["foreign_toplevel_identifier"] = json!("new-owner");
    extra["pid"] = json!(owners.0[1].id());
    extra["focused"] = json!(false);
    extra["lumen_state"]["hidden"] = json!(true);
    expanded.push(extra);
    expanded[1]["lumen_state"]["hidden"] = json!(true);
    lifecycle.refresh(EPOCH, &expanded);
    let plan = lifecycle
        .native_focus_plan(&chosen, &tree(&expanded), true)
        .unwrap();
    let command = plan.command.unwrap();
    assert!(command.contains("[con_id=12] lumen_window unhide"));
    assert!(!command.contains("con_id=20"));
    let mut invalid = chosen.clone();
    invalid["groupInstances"] = json!([]);
    assert!(
        lifecycle
            .native_focus_plan(&invalid, &tree(&expanded), true)
            .is_err()
    );
    let mut unsupported = expanded.clone();
    unsupported[1]
        .as_object_mut()
        .unwrap()
        .remove("foreign_toplevel_identifier");
    lifecycle.refresh(EPOCH, &unsupported);
    assert!(
        !lifecycle
            .window_history
            .captures(&lifecycle)
            .iter()
            .any(|c| c["conId"] == 12)
    );
    assert!(
        lifecycle
            .native_focus_plan(&chosen, &tree(&unsupported), true)
            .is_err()
    );
}
#[test]
fn bounded_native_history_rejects_corruption_and_retired_owner() {
    let mut owners = owners();
    let mut lifecycle = Lifecycle::default();
    let current = nodes(&owners, 13);
    lifecycle.refresh(EPOCH, &current);
    let records = lifecycle.records();
    let valid = lifecycle.window_history.value();
    for field in ["instanceId", "foreignId", "conId", "extra"] {
        let mut invalid = valid.clone();
        invalid[0][field] = match field {
            "conId" => json!(-1),
            "instanceId" => json!("unassociated"),
            _ => json!(""),
        };
        assert!(History::from_saved(&invalid, &records).is_err());
    }
    let mut many = Vec::new();
    for id in 1..=1025 {
        let mut row = valid[0].clone();
        row["conId"] = json!(id);
        row["foreignId"] = json!(format!("surface-{id}"));
        many.push(row);
    }
    assert!(History::from_saved(&json!(many.clone()), &records).is_err());
    let mut bounded = History::default();
    bounded.reconcile(&many);
    assert_eq!(bounded.value().as_array().unwrap().len(), 1024);
    let chosen = capture(&lifecycle, 12);
    owners.0[0].kill().unwrap();
    owners.0[0].wait().unwrap();
    lifecycle.prune();
    assert!(
        lifecycle
            .native_focus_plan(&chosen, &tree(&current), true)
            .is_err()
    );
    assert!(
        !lifecycle
            .window_history
            .captures(&lifecycle)
            .iter()
            .any(|c| c["appId"] == "app")
    );
}

#[test]
fn unsupported_source_is_distinct_from_no_client_and_owned_native_panel_is_exact() {
    let owners = owners();
    let mut lifecycle = Lifecycle::default();
    let supported = nodes(&owners, 13);
    lifecycle.refresh(EPOCH, &supported);
    let native = lifecycle.current_source_focus.clone();
    assert_eq!(native["kind"], "native");
    assert!(lifecycle.source_focus_matches(&native));
    let mut unsupported_a = supported.clone();
    unsupported_a[2]["shell"] = json!("xwayland");
    unsupported_a[2]["window"] = json!(50);
    lifecycle.refresh(EPOCH, &unsupported_a);
    let unknown = lifecycle.current_source_focus.clone();
    assert_eq!(unknown["kind"], "unsupported");
    assert!(!lifecycle.source_focus_matches(&unknown));
    assert!(!lifecycle.source_focus_matches(&Value::Null));
    let mut unsupported_b = unsupported_a.clone();
    unsupported_b[2]["id"] = json!(99);
    lifecycle.refresh(EPOCH, &unsupported_b);
    assert!(!lifecycle.source_focus_matches(&unknown));
    assert!(!lifecycle.source_focus_matches(&Value::Null));
    let mut unregistered = supported.clone();
    unregistered[2]["app_id"] = Value::Null;
    let flattened = lumen_application_lifecycle::window_nodes(&tree(&unregistered)).unwrap();
    assert_eq!(flattened.len(), 3);
    lifecycle.refresh(EPOCH, &flattened);
    assert_eq!(lifecycle.current_source_focus["kind"], "unsupported");
    assert!(!lifecycle.source_focus_matches(&Value::Null));
    let mut none = supported.clone();
    for node in &mut none {
        node["focused"] = json!(false);
    }
    lifecycle.refresh(EPOCH, &none);
    let empty = lifecycle.current_source_focus.clone();
    assert_eq!(empty["kind"], "none");
    assert!(lifecycle.source_focus_matches(&empty));
    assert!(!lifecycle.source_focus_matches(&Value::Null));
    let mut owned = supported.clone();
    owned[2]["app_id"] = json!("org.quickshell");
    lifecycle.refresh(EPOCH, &owned);
    let panel = lifecycle.current_source_focus.clone();
    assert_eq!(panel["kind"], "native");
    assert_eq!(panel["appId"], "org.quickshell");
    assert!(lifecycle.source_focus_matches(&panel));
    assert!(lifecycle.native_record(&owned[2]).is_none());
    owned[2]["foreign_toplevel_identifier"] = json!("different-owned-panel");
    lifecycle.refresh(EPOCH, &owned);
    assert!(!lifecycle.source_focus_matches(&panel));
}
