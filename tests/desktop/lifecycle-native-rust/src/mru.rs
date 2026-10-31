//! Three real application identities retain activation order through observer/shell loss.
use crate::harness::{Fixture, Options, Result, read, require, save, wait_true};
use serde_json::{Value, json};
use std::{fs, path::Path};
const IDS: [&str; 3] = ["org.example.MruA", "org.example.MruB", "org.example.MruC"];
fn state(f: &Fixture) -> Result<Value> {
    f.state("mruAudit", "last-mru.json")
}
fn physical(f:&Fixture,key:&str)->Result<()>{
    wait_true("switcher owns keyboard before physical decision",||{
        let current=state(f)?;
        save(&f.options.output.join("mru-keyboard-readiness.json"),&current)?;
        Ok(current["switcher"]["keyboardReady"]==true)
    })?;
    f.call("wtype",&["-k",key])?;Ok(())
}
fn ordered(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| IDS.contains(id))
        .map(str::to_owned)
        .collect()
}
fn order(f: &Fixture, expected: &[&str]) -> Result<()> {
    wait_true("real application activation order", || {
        let value=state(f)?;
        Ok(ordered(&value,"history")==expected && ordered(&value,"observerHistory")==expected)
    })
}
fn picker(f: &Fixture, action: &str) -> Result<Value> {
    Ok(serde_json::from_str(&f.ipc(
        "desktop",
        "appSwitcher",
        &[action],
    )?)?)
}
fn focus(f: &Fixture, id: &str) -> Result<()> {
    f.call("swaymsg", &[&format!("[app_id=\"{id}\"]"), "focus"])?;
    wait_true("captured native application focus", || {
        save(&f.options.output.join("focus-native-tree.json"), &f.tree()?)?;
        Ok(state(f)?["active"] == id)
    })
}
fn selected(f: &Fixture, id: &str) -> Result<Value> {
    let mut value = picker(f, "begin")?;
    let mut attempts=vec![value.clone()];
    let count = value["items"]
        .as_array()
        .ok_or("Switcher items missing")?
        .len();
    for _ in 0..count {
        if value["key"] == id {
            return Ok(value);
        }
        value = picker(f, "next")?;
        attempts.push(value.clone());
    }
    save(&f.options.output.join(format!("selection-missing-{id}.json")),&json!({"requested":id,"attempts":attempts,"state":state(f)?,"tree":f.tree()?}))?;
    Err("Live switcher missing captured application".into())
}
fn client(f: &Fixture, index: usize) -> Result<Value> {
    read(
        &f.options
            .output
            .join(format!("client-{index}/client-state.json")),
    )
}
fn instances(value: &Value, id: &str) -> Result<Value> {
    value["apps"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["appId"] == id))
        .map(|row| row["instanceIds"].clone())
        .ok_or_else(|| "Missing native application instances".into())
}
fn zero(f: &Fixture) -> Result<bool> {
    Ok(state(f)?["apps"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["appId"] == IDS[1] && row["zeroWindows"] == true)
    }))
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    f.launch(Path::new("swaymsg"), &["-m","-t","subscribe","[\"window\"]"], "native-window-events.log")?;
    let qml = f.qml.clone();
    let desktop = f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().ok_or("QML path")?],
        "desktop.log",
    )?;
    wait_true("MRU observer ready", || Ok(state(&f)?["available"] == true))?;
    // Keep a real virtual keyboard mapped before the switcher requests exclusive focus.
    f.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log")?;
    let tool = f.sibling("lifecycle-client")?;
    let mut children = Vec::new();
    for (index, id) in IDS.iter().enumerate() {
        let root = f.options.output.join(format!("client-{index}"));
        fs::create_dir(&root)?;
        children.push(f.launch(
            &tool,
            &[root.to_str().ok_or("Client root")?, id],
            &format!("client-{index}.log"),
        )?);
        wait_true("independent native client windows", || {
            Ok(client(&f, index)?["windows"] == 2)
        })?;
    }
    // GTK window objects may exist before their surfaces have mapped. Require
    // both compositor observation and shell toplevel enumeration before focus.
    wait_true("all native application surfaces mapped", || {
        let value=state(&f)?;
        Ok(IDS.iter().all(|id| value["windows"].as_array().is_some_and(|windows|windows.iter().filter(|window|window.as_str()==Some(id)).count()==2)
            && value["apps"].as_array().is_some_and(|apps|apps.iter().any(|app|app["appId"]==*id && app["windowCount"]==2))))
    })?;
    for id in IDS {
        focus(&f, id)?;
    }
    let expected = [IDS[2], IDS[1], IDS[0]];
    order(&f, &expected)?;
    save(&f.options.output.join("mru-after-focus-sequence.json"),&state(&f)?)?;
    save(&f.options.output.join("native-before-zero-windows.json"),&f.tree()?)?;
    fs::write(
        f.options.output.join("client-1/command"),
        "hide all windows",
    )?;
    wait_true("retained middle application has zero windows", || zero(&f))?;
    save(&f.options.output.join("native-after-zero-windows.json"),&f.tree()?)?;
    order(&f, &expected)?;
    let initial = state(&f)?;
    require(
        initial["sessionEpoch"]
            .as_str()
            .is_some_and(|epoch| !epoch.is_empty()),
        "Observer compositor epoch missing",
    )?;
    let mut native = Vec::new();
    Fixture::nodes(&f.tree()?, &mut native);
    let captured: Vec<Value> = native
        .into_iter()
        .filter(|node| node["app_id"].as_str().is_some_and(|id| IDS.contains(&id)))
        .map(|node| json!({"conId":node["id"],"pid":node["pid"],"appId":node["app_id"]}))
        .collect();
    require(
        captured.iter().any(|node| node["appId"] == IDS[0])
            && captured.iter().any(|node| node["appId"] == IDS[2]),
        "Native application container witnesses missing",
    )?;
    for index in [0, 2] {
        require(
            captured.iter().any(|node| {
                node["appId"] == IDS[index]
                    && node["pid"] == f.processes[children[index]].0.id()
                    && node["conId"].as_u64().is_some()
            }),
            "Native MRU container PID differs from owned client",
        )?;
    }
    save(
        &f.options.output.join("mru-native-witnesses.json"),
        &json!(captured),
    )?;
    let retained = instances(&initial, IDS[1])?;
    save(&f.options.output.join("mru-before.json"), &initial)?;
    let preview = selected(&f, IDS[1])?;
    require(
        ordered(&json!({"items":preview["items"]}), "items") == expected,
        "Switcher relative order differs before restart",
    )?;
    order(&f, &expected)?;
    physical(&f,"Escape")?;
    wait_true("physical Escape closes preview", || {
        Ok(state(&f)?["switcher"]["visible"] == false)
    })?;
    order(&f, &expected)?;
    f.ipc("mruAudit", "restartObserver", &[])?;
    wait_true("observer restart outage", || {
        Ok(state(&f)?["available"] == false)
    })?;
    wait_true("observer validates retained instance", || {
        Ok(state(&f)?["available"] == true
            && zero(&f)?
            && instances(&state(&f)?, IDS[1])? == retained)
    })?;
    order(&f, &expected)?;
    save(
        &f.options.output.join("mru-observer-recovered.json"),
        &state(&f)?,
    )?;
    f.stop(desktop)?;
    for child in &children {
        require(
            f.processes[*child].alive()?,
            "Shell restart terminated external client",
        )?;
    }
    f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().ok_or("QML path")?],
        "desktop-restarted.log",
    )?;
    wait_true("new shell validates same zero-window identity", || {
        Ok(state(&f)?["available"] == true
            && zero(&f)?
            && instances(&state(&f)?, IDS[1])? == retained)
    })?;
    order(&f, &expected)?;
    let recovered = state(&f)?;
    require(
        recovered["sessionEpoch"] == initial["sessionEpoch"],
        "Shell restart changed compositor epoch",
    )?;
    save(
        &f.options.output.join("mru-shell-recovered.json"),
        &recovered,
    )?;
    require(
        ordered(&recovered, "observerHistory") == expected,
        "Observer journal order differs from recovered shell",
    )?;
    let preview = selected(&f, IDS[1])?;
    let commits = preview["commits"].as_u64().ok_or("Switcher commit count")?;
    order(&f, &expected)?;
    physical(&f,"Return")?;
    wait_true("physical commit reopens retained owner", || {
        Ok(client(&f, 1)?["windows"] == 2 && state(&f)?["active"] == IDS[1])
    })?;
    require(
        instances(&state(&f)?, IDS[1])? == retained && f.processes[children[1]].alive()?,
        "Switcher replaced retained process",
    )?;
    wait_true("physical commit recorded exactly once", || {
        Ok(state(&f)?["switcher"]["commits"] == commits + 1)
    })?;
    order(&f, &[IDS[1], IDS[2], IDS[0]])?;
    focus(&f, IDS[0])?;
    order(&f, &[IDS[0], IDS[1], IDS[2]])?;
    // Capture a zero-window owner, remove that exact process, then release a real
    // decision key. A dead captured owner must never launch a replacement instance.
    focus(&f, IDS[0])?;
    fs::write(
        f.options.output.join("client-2/command"),
        "hide all windows",
    )?;
    wait_true("stale target windows hidden", || {
        Ok(client(&f, 2)?["windows"] == 0)
    })?;
    let captured = selected(&f, IDS[2])?;
    let commits = captured["commits"].clone();
    f.stop(children[2])?;
    wait_true("captured process removed from observer", || {
        Ok(state(&f)?["apps"]
            .as_array()
            .is_some_and(|rows| !rows.iter().any(|row| row["appId"] == IDS[2])))
    })?;
    physical(&f,"Return")?;
    wait_true("stale physical commit fails closed", || {
        let v = state(&f)?;
        Ok(v["switcher"]["visible"] == false
            && v["switcher"]["phase"] == "cancelled"
            && v["switcher"]["commits"] == commits)
    })?;
    require(
        !f.processes[children[2]].alive()? && state(&f)?["active"] == IDS[0],
        "Stale owner changed active application",
    )?;
    save(&f.options.output.join("mru-final.json"), &state(&f)?)?;
    // Reap first: errors emitted during shutdown are part of the acceptance result.
    for child in f.processes.iter_mut().rev() {
        child.stop()?;
    }
    for log in ["desktop.log", "desktop-restarted.log"] {
        let text = fs::read_to_string(f.options.output.join(log))?;
        for marker in [
            "TypeError:",
            "ReferenceError:",
            "Failed to load configuration",
        ] {
            require(!text.contains(marker), format!("Shell emitted {marker}"))?;
        }
    }
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":["three independent native app IDs and retained zero-window owner","preview and physical Escape preserve relative MRU","observer restart validates same identities and relative application order","whole shell restart restores observer journal order","physical Return activates and reopens exact retained process once","new native focus takes precedence","dead captured zero-window owner fails closed without replacement"],"scope":"Actual native application MRU restart and captured physical decisions; no injected activation order or host state"}),
    )?;
    Ok(())
}
