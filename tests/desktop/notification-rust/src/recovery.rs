use super::*;

pub fn producer(label: &str) -> Result<()> {
    use std::collections::HashMap;
    use zbus::zvariant::Value as BusValue;
    let connection = Connection::session()?;
    let proxy = Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )?;
    let mut signals = proxy.receive_signal("ActionInvoked")?;
    let hints: HashMap<&str, BusValue<'_>> = HashMap::new();
    let id: u32 = proxy.call(
        "Notify",
        &(
            "Recovery producer",
            0u32,
            "",
            label,
            "Synthetic recovery body",
            vec!["default", "Open"],
            hints,
            0i32,
        ),
    )?;
    println!(
        "{}",
        json!({"ready":id,"sender":connection.unique_name().map(|n|n.to_string()).unwrap_or_default()})
    );
    std::io::stdout().flush()?;
    for signal in &mut signals {
        let (notification, action): (u32, String) = signal.body().deserialize()?;
        if notification == id {
            println!("{}", json!({"action":action,"id":id}));
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}
fn ui(fixture: &Fixture) -> Result<Value> {
    let value = fixture.ui("recoveryAudit")?;
    save(&fixture.options.output.join("last-ui.json"), &value)?;
    Ok(value)
}
fn ipc(fixture: &Fixture, method: &str, args: &[&str]) -> Result<String> {
    fixture.ipc("recoveryAudit", method, args)
}
fn signals(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(BufReader::new(File::open(path)?)
        .lines()
        .filter_map(|line| line.ok().and_then(|text| serde_json::from_str(&text).ok()))
        .collect())
}
fn actions(path: &Path) -> Result<Vec<Value>> {
    Ok(signals(path)?
        .into_iter()
        .filter(|value| value.get("action").is_some())
        .collect())
}
fn producer_process(fixture: &mut Fixture, label: &str, index: usize) -> Result<(usize, PathBuf)> {
    let log = fixture.options.output.join(format!("producer-{index}.log"));
    let current = path(&env::current_exe()?);
    let process = fixture.start(
        &format!("producer-{index}.log"),
        &current,
        &["producer", label],
        false,
    )?;
    wait("producer ready", || {
        Ok(signals(&log)?
            .iter()
            .any(|value| value.get("ready").is_some())
            .then_some(()))
    })?;
    Ok((process, log))
}
fn child_watcher(fixture: &Fixture, ui_index: usize) -> Result<Option<i32>> {
    let parent = fixture.children.0[ui_index].id() as i32;
    let helper = fixture
        .vars
        .get("LUMEN_NOTIFICATION_HELPER")
        .ok_or("Missing helper")?
        .as_bytes();
    let mut found = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(pid) = name.to_string_lossy().parse::<i32>().ok() else {
            continue;
        };
        let Ok(bytes) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let arguments: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
        if !arguments.contains(&helper) || !arguments.contains(&b"watch".as_slice()) {
            continue;
        }
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let ppid = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|value| value.trim().parse::<i32>().ok());
        if ppid == Some(parent) {
            found.push(pid);
        }
    }
    require(
        found.len() <= 1,
        format!("Multiple owned notification watchers: {found:?}"),
    )?;
    Ok(found.into_iter().next())
}
fn watcher_gone(pid: i32) -> bool {
    let path = PathBuf::from(format!("/proc/{pid}/status"));
    match fs::read_to_string(path) {
        Ok(text) => text.contains("\nState:\tZ"),
        Err(_) => true,
    }
}
fn crash(fixture: &mut Fixture, index: usize) -> Result<()> {
    fixture.children.signal(index, libc::SIGKILL);
    fixture.children.0[index].wait()?;
    Ok(())
}
fn probe(fixture: &Fixture, layout: &Value) -> Result<()> {
    let before = ui(fixture)?["probes"]
        .as_i64()
        .ok_or("Missing probe count")?;
    ipc(fixture, "focusProbe", &[])?;
    call(
        "swaymsg",
        &["[title=\"Recovery notification\"]", "focus"],
        &fixture.vars,
    )?;
    call("wtype", &["-k", "space"], &fixture.vars)?;
    wait("keyboard probe", || {
        Ok((ui(fixture)?["probes"] == before + 1).then_some(()))
    })?;
    require(
        ui(fixture)?["layout"] == *layout,
        "Widget layout changed during recovery",
    )?;
    Ok(())
}
fn activate(fixture: &Fixture) -> Result<()> {
    ipc(fixture, "activate", &[])?;
    Ok(())
}
fn start_ui(fixture: &mut Fixture, name: &str) -> Result<usize> {
    let qml = path(&fixture.qml);
    fixture.start(name, "quickshell", &["-p", &qml], false)
}
fn available(fixture: &Fixture) -> Result<bool> {
    let state = ui(fixture)?;
    Ok(state["available"] == true && !state["rows"].as_array().is_some_and(Vec::is_empty))
}
fn unavailable(fixture: &Fixture) -> Result<bool> {
    let state = ui(fixture)?;
    Ok(state["available"] == false && state["rows"].as_array().is_some_and(Vec::is_empty))
}
pub fn run(fixture: &mut Fixture) -> Result<()> {
    let state_qml = fixture.private.join("assets/shell/NotificationState.qml");
    fs::set_permissions(&state_qml, fs::Permissions::from_mode(0o600))?;
    let text = fs::read_to_string(&state_qml)?;
    require(
        text.contains("stdout:SplitParser {onRead:line=>root.applyState(line)}"),
        "Notification watcher source changed",
    )?;
    fs::write(&state_qml,text.replace("stdout:SplitParser {onRead:line=>root.applyState(line)}","stdout:SplitParser {onRead:line=>root.applyState(line)}\n        stderr:SplitParser {onRead:line=>console.log(\"WATCHER_STDERR:\"+line)}"))?;
    fixture.start("keyboard.log", "wtype", &["-s", "60000"], false)?;
    let (first, first_log) = producer_process(fixture, "Before producer death", 1)?;
    let ui_index = start_ui(fixture, "ui.log")?;
    wait("first actionable row", || {
        let state = ui(fixture)?;
        Ok((state["available"] == true
            && state["rows"][0]["actions"]
                .as_array()
                .is_some_and(|a| !a.is_empty()))
        .then_some(()))
    })?;
    ipc(fixture, "configure", &[])?;
    let saved_layout = ui(fixture)?["layout"].clone();
    let layout_file = PathBuf::from(
        fixture
            .vars
            .get("XDG_STATE_HOME")
            .ok_or("Missing state root")?,
    )
    .join("lumen-desktop/session/notification-centre.json");
    wait("saved widget layout", || {
        Ok(
            (layout_file.exists() && read(&layout_file).ok().as_ref() == Some(&saved_layout))
                .then_some(()),
        )
    })?;
    crash(fixture, first)?;
    wait("revoked producer action", || {
        Ok(ui(fixture)?["rows"][0]["actions"]
            .as_array()
            .is_some_and(Vec::is_empty)
            .then_some(()))
    })?;
    activate(fixture)?;
    pause(200);
    require(
        actions(&first_log)?.is_empty(),
        "Dead producer received action",
    )?;
    let mut checks =
        vec!["Actual producer SIGKILL revokes actions without removing retained history"];
    let (_second, second_log) = producer_process(fixture, "Watcher restart action", 2)?;
    wait("second row", || {
        Ok((ui(fixture)?["rows"][0]["summary"] == "Watcher restart action").then_some(()))
    })?;
    let old_lease = ui(fixture)?["lease"].clone();
    let old_watcher = wait("owned watcher", || child_watcher(fixture, ui_index))?;
    unsafe {
        libc::kill(old_watcher, libc::SIGSTOP);
    }
    activate(fixture)?;
    pause(200);
    require(
        actions(&second_log)?.is_empty(),
        "Stopped watcher dispatched action",
    )?;
    wait("stopped watcher unavailable", || {
        Ok(unavailable(fixture)?.then_some(()))
    })?;
    probe(fixture, &saved_layout)?;
    unsafe {
        libc::kill(old_watcher, libc::SIGCONT);
    }
    wait("new watcher", || {
        Ok(child_watcher(fixture, ui_index)?
            .filter(|pid| *pid != old_watcher)
            .and_then(|_| available(fixture).ok()?.then_some(())))
    })?;
    require(
        actions(&second_log)?.is_empty(),
        "Resumed watcher replayed action",
    )?;
    ipc(
        fixture,
        "activateWithLease",
        &[&old_lease.to_string().trim_matches('"').to_owned()],
    )?;
    pause(200);
    require(
        actions(&second_log)?.is_empty(),
        "Expired lease dispatched action",
    )?;
    checks.push("Delayed owned action retains its captured expired lease even after a fresh transport lease arrives");
    checks.push("Stopped transport becomes unavailable while other keyboard controls and layout stay usable; resume never replays its pending action");
    let old_watcher = wait("replacement watcher", || child_watcher(fixture, ui_index))?;
    unsafe {
        libc::kill(old_watcher, libc::SIGSTOP);
    }
    activate(fixture)?;
    pause(200);
    require(
        actions(&second_log)?.is_empty(),
        "Stopped watcher dispatched queued action",
    )?;
    unsafe {
        libc::kill(old_watcher, libc::SIGKILL);
    }
    wait("killed watcher unavailable", || {
        Ok(unavailable(fixture)?.then_some(()))
    })?;
    wait("watcher reconnect", || {
        Ok(child_watcher(fixture, ui_index)?
            .filter(|pid| *pid != old_watcher)
            .and_then(|_| {
                let state = ui(fixture).ok()?;
                (state["available"] == true
                    && state["rows"][0]["summary"] == "Watcher restart action")
                    .then_some(())
            }))
    })?;
    activate(fixture)?;
    wait("one fresh action", || {
        Ok((actions(&second_log)?.len() == 1).then_some(()))
    })?;
    pause(250);
    require(
        actions(&second_log)?.len() == 1,
        "Action dispatched more than once",
    )?;
    checks.push("Stopped transport with queued action is killed; reconnect drops stale command and fresh action dispatches once");
    let (_third, third_log) = producer_process(fixture, "Service restart action", 3)?;
    wait("third row", || {
        Ok((ui(fixture)?["rows"][0]["summary"] == "Service restart action").then_some(()))
    })?;
    fixture.children.signal(fixture.server, libc::SIGSTOP);
    activate(fixture)?;
    wait("stopped service unavailable", || {
        Ok(unavailable(fixture)?.then_some(()))
    })?;
    probe(fixture, &saved_layout)?;
    fixture.children.signal(fixture.server, libc::SIGCONT);
    wait("service resumed", || {
        Ok(
            (available(fixture)? && ui(fixture)?["rows"][0]["summary"] == "Service restart action")
                .then_some(()),
        )
    })?;
    require(
        actions(&third_log)?.is_empty(),
        "Resumed service replayed action",
    )?;
    checks.push("Stopped server times out without freezing owned controls; queued expired action is rejected after resume and transport reconnects");
    let old_instance = fixture.snapshot()?["instance"].clone();
    crash(fixture, fixture.server)?;
    wait("service death unavailable", || {
        Ok(unavailable(fixture)?.then_some(()))
    })?;
    probe(fixture, &saved_layout)?;
    let helper = fixture
        .vars
        .get("LUMEN_NOTIFICATION_HELPER")
        .ok_or("Missing helper")?
        .clone();
    fixture.server = fixture.start("notifications-restart.log", &helper, &["serve"], false)?;
    wait("new service identity", || {
        let state = fixture.snapshot()?;
        Ok((state["instance"] != old_instance
            && state["lock"] == json!({"known":true,"locked":false}))
        .then_some(()))
    })?;
    wait("new service UI", || {
        Ok(
            (available(fixture)? && ui(fixture)?["instance"] == fixture.snapshot()?["instance"])
                .then_some(()),
        )
    })?;
    let state = ui(fixture)?;
    require(
        state["rows"]
            .as_array()
            .ok_or("Missing rows")?
            .iter()
            .all(|row| {
                row["actions"].as_array().is_some_and(Vec::is_empty) && row["active"] == false
            }),
        "Revived producer authority",
    )?;
    require(
        state["rows"]
            .as_array()
            .ok_or("Missing rows")?
            .iter()
            .any(|row| row["summary"] == "Service restart action"),
        "Retained history lost",
    )?;
    activate(fixture)?;
    pause(200);
    require(
        actions(&third_log)?.is_empty(),
        "Retired producer dispatched action",
    )?;
    checks.push("Service SIGKILL recovers retained history with fresh service identity and no revived producer action authority");
    let (_fourth, fourth_log) = producer_process(fixture, "Shell restart action", 4)?;
    wait("fourth row", || {
        Ok((ui(fixture)?["rows"][0]["summary"] == "Shell restart action").then_some(()))
    })?;
    let retained = fixture.private.join("history/history.json");
    let history_before = fs::read(&retained)?;
    let old_watcher = wait("shell watcher", || child_watcher(fixture, ui_index))?;
    crash(fixture, ui_index)?;
    wait("shell watcher gone", || {
        Ok(watcher_gone(old_watcher).then_some(()))
    })?;
    start_ui(fixture, "ui-restart.log")?;
    wait("restored shell", || {
        Ok(
            (available(fixture)? && ui(fixture)?["rows"][0]["summary"] == "Shell restart action")
                .then_some(()),
        )
    })?;
    require(
        fs::read(&retained)? == history_before && ui(fixture)?["layout"] == saved_layout,
        "Shell restart changed persisted state",
    )?;
    probe(fixture, &saved_layout)?;
    activate(fixture)?;
    wait("fresh shell action", || {
        Ok((actions(&fourth_log)?.len() == 1).then_some(()))
    })?;
    pause(250);
    require(
        actions(&fourth_log)?.len() == 1,
        "Shell action dispatched more than once",
    )?;
    checks.push("Owned shell SIGKILL preserves widget layout, service history and producer identity; fresh keyboard control and action each dispatch once");
    for name in ["ui.log", "ui-restart.log"] {
        let log = fs::read_to_string(fixture.options.output.join(name))?;
        require(
            ![
                "TypeError:",
                "ReferenceError:",
                "Binding loop",
                "Failed to load configuration",
                "Cannot assign",
                "PyGIDeprecationWarning",
                "ResourceWarning",
            ]
            .iter()
            .any(|marker| log.contains(marker)),
            format!("UI log failure in {name}: {log}"),
        )?;
    }
    save(
        &fixture.options.output.join("result.json"),
        &json!({"passed":true,"checks":checks,"scope":"Private bus/compositor, synthetic producers and owned controls only; native widget drag/mask acceptance is separate widget-catalogue-rust fixture"}),
    )?;
    Ok(())
}
