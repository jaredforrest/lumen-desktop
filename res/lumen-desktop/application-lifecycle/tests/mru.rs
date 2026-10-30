use lumen_application_lifecycle::{Lifecycle, ObservationStore, mru::History};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command};

struct Clients(Vec<Child>);
impl Drop for Clients {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn clients() -> Clients {
    Clients(
        (0..3)
            .map(|_| Command::new("sleep").arg("30").spawn().unwrap())
            .collect(),
    )
}
fn nodes(clients: &Clients, focused: usize) -> Vec<Value> {
    clients.0.iter().enumerate().map(|(i,p)|json!({"id":10+i,"pid":p.id(),"shell":"xdg_shell","window":null,"app_id":format!("app{i}"),"focused":i==focused})).collect()
}
struct Directory(std::path::PathBuf);
impl Directory {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("lumen-mru-{name}-{}", std::process::id()));
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const EPOCH: &str = "0123456789abcdef01234567";
const BOOT: &str = "01234567-89ab-cdef-0123-456789abcdef";

#[test]
fn queued_native_focus_receipts_preserve_brief_focus_with_later_tree_snapshots() {
    let mut clients=clients();
    let latest=nodes(&clients,2);
    let mut lifecycle=Lifecycle::default();
    lifecycle.refresh(EPOCH,&latest);
    for focused in [0,1,2]{
        let receipt=json!({"change":"focus","container":nodes(&clients,focused)[focused]});
        lifecycle.refresh(EPOCH,&latest);
        lifecycle.observe_focus_event(&receipt);
    }
    assert_eq!(lifecycle.activation_history.order(),["app2","app1","app0"]);
    // The middle application's surface can retire while its ordered focus
    // receipt is queued. Its exact still-live process retains that receipt.
    lifecycle.activation_history=History::default();
    let remaining=latest.iter().filter(|node|node["app_id"]!="app1").cloned().collect::<Vec<_>>();
    lifecycle.refresh(EPOCH,&remaining);
    for focused in [0,1,2]{
        lifecycle.observe_focus_event(&json!({"change":"focus","container":nodes(&clients,focused)[focused]}));
    }
    assert_eq!(lifecycle.activation_history.order(),["app2","app1","app0"]);
    // A queued receipt for a retired container, unrelated change, or false
    // focus flag cannot promote a different or replacement native surface.
    let mut stale=json!({"change":"focus","container":nodes(&clients,0)[0]});
    stale["container"]["id"]=json!(999);
    lifecycle.observe_focus_event(&stale);
    let other=json!({"change":"title","container":nodes(&clients,0)[0]});
    lifecycle.observe_focus_event(&other);
    let not_focused=json!({"change":"focus","container":latest[0]});
    lifecycle.observe_focus_event(&not_focused);
    assert_eq!(lifecycle.activation_history.order(),["app2","app1","app0"]);
    let mut reused=latest.clone();
    reused[0]["pid"]=json!(clients.0[2].id());
    lifecycle.refresh(EPOCH,&reused);
    lifecycle.observe_focus_event(&json!({"change":"focus","container":nodes(&clients,0)[0]}));
    let mut replacement_receipt=reused[0].clone();replacement_receipt["focused"]=json!(true);
    lifecycle.observe_focus_event(&json!({"change":"focus","container":replacement_receipt}));
    assert_eq!(lifecycle.activation_history.order(),["app2","app1","app0"]);
    let retired_receipt=json!({"change":"focus","container":nodes(&clients,1)[1]});
    clients.0[1].kill().unwrap();clients.0[1].wait().unwrap();
    lifecycle.prune();
    lifecycle.observe_focus_event(&retired_receipt);
    assert_eq!(lifecycle.activation_history.order(),["app2","app0"]);
    lifecycle.refresh("fedcba9876543210fedcba98",&remaining);
    lifecycle.observe_focus_event(&json!({"change":"focus","container":nodes(&clients,1)[1]}));
    assert!(lifecycle.activation_history.order().is_empty());
}

#[test]
fn actual_live_process_order_survives_recovery_and_zero_windows_but_not_replacement_or_epoch() {
    let clients = clients();
    let mut lifecycle = Lifecycle::default();
    for focused in [0, 1, 0, 2] {
        let n = nodes(&clients, focused);
        lifecycle.refresh(EPOCH, &n);
        lifecycle.observe_focus(&n);
    }
    assert_eq!(
        lifecycle.activation_history.order(),
        ["app2", "app0", "app1"]
    );
    lifecycle.refresh(EPOCH, &[]);
    assert!(lifecycle.rows().iter().all(|r| r["zeroWindows"] == true));
    let records = lifecycle.records();
    let history = lifecycle.activation_history.clone();
    drop(lifecycle);
    let root = Directory::new("live");
    let mut store = ObservationStore::new(&root.0, EPOCH, BOOT).unwrap();
    store.save_state(&records, &history).unwrap();
    drop(store);
    let mut store = ObservationStore::new(&root.0, EPOCH, BOOT).unwrap();
    let recovered = store.load_state();
    let mut lifecycle = Lifecycle::default();
    lifecycle.recover(EPOCH, &recovered.processes);
    lifecycle.activation_history = recovered.history;
    lifecycle.prune();
    assert_eq!(lifecycle.activation_history, history);
    let n = nodes(&clients, 1);
    lifecycle.refresh(EPOCH, &n);
    lifecycle.observe_focus(&n);
    assert_eq!(
        lifecycle.activation_history.order(),
        ["app1", "app2", "app0"]
    );
    let mut wrong = n;
    wrong[1]["pid"] = json!(1);
    lifecycle.observe_focus(&wrong);
    assert_eq!(
        lifecycle.activation_history.order(),
        ["app1", "app2", "app0"]
    );
    lifecycle.refresh("a different owned compositor epoch", &[]);
    assert!(lifecycle.activation_history.order().is_empty());
}

#[test]
fn schema_version_scope_corruption_and_witness_association_fail_closed() {
    let clients = clients();
    let mut lifecycle = Lifecycle::default();
    let n = nodes(&clients, 0);
    lifecycle.refresh(EPOCH, &n);
    lifecycle.observe_focus(&n);
    let root = Directory::new("scope");
    let mut store = ObservationStore::new(&root.0, EPOCH, BOOT).unwrap();
    store
        .save_state(&lifecycle.records(), &lifecycle.activation_history)
        .unwrap();
    let path = store.directory.join(format!("{EPOCH}.json"));
    let valid: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for field in ["bootId", "epoch", "activationHistory", "version"] {
        let mut bad = valid.clone();
        bad[field] = match field {
            "activationHistory" => {
                json!([{"appId":"unobserved","instanceIds":valid["activationHistory"][0]["instanceIds"]}])
            }
            "version" => json!(3),
            _ => json!("changed"),
        };
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        let saved = store.load_state();
        assert!(saved.processes.is_empty());
        assert!(saved.history.order().is_empty());
    }
    let mut legacy = valid.clone();
    legacy["version"] = json!(1);
    legacy.as_object_mut().unwrap().remove("activationHistory");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let saved = store.load_state();
    assert_eq!(saved.processes.len(), 3);
    assert!(saved.history.order().is_empty());
    let mut dead = valid;
    dead["processes"][0]["identity"]["start"] = json!(0);
    // A self-consistent stale record can parse, but cannot confer live order.
    let first = &dead["processes"][0];
    dead["activationHistory"] = json!([{"appId":first["apps"][0],"instanceIds":[lumen_application_lifecycle::mru::instance(&first["identity"])]}]);
    fs::write(&path, serde_json::to_vec(&dead).unwrap()).unwrap();
    let saved = store.load_state();
    let mut next = Lifecycle::default();
    next.recover(EPOCH, &saved.processes);
    next.activation_history = saved.history;
    next.prune();
    assert!(next.activation_history.order().is_empty());
}

#[test]
fn bounded_rank_budget_preserves_process_recovery_instead_of_overflowing_journal() {
    let apps = (0..8)
        .map(|i| format!("org.example.{i}.{}", "x".repeat(205)))
        .collect::<Vec<_>>();
    let records = (2..514)
        .map(|pid| json!({"identity":{"pid":pid,"start":10,"device":2,"inode":3},"apps":apps}))
        .collect::<Vec<_>>();
    let ids = records
        .iter()
        .map(|r| lumen_application_lifecycle::mru::instance(&r["identity"]))
        .collect::<Vec<_>>();
    let rows = apps
        .iter()
        .map(|app| json!({"appId":app,"instanceIds":ids}))
        .collect::<Vec<_>>();
    let mut history = History::default();
    for app in &apps {
        history.promote(app, &rows);
    }
    assert_eq!(history.order().len(), 4);
    let root = Directory::new("budget");
    let mut store = ObservationStore::new(&root.0, EPOCH, BOOT).unwrap();
    store.save_state(&records, &history).unwrap();
    let saved = store.load_state();
    assert_eq!(saved.processes.len(), 512);
    assert!(!saved.history.order().is_empty());
    assert!(saved.history.order().len() < 4);
    assert!(
        fs::metadata(store.directory.join(format!("{EPOCH}.json")))
            .unwrap()
            .len()
            <= 1024 * 1024
    );
}
