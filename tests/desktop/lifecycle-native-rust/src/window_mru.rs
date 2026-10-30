//! Exact backing-window selection through restart, duplicate captions and retirement.
use crate::harness::{Fixture, Options, Result, read, require, save, wait, wait_true};
use serde_json::{Value, json};
use std::{fs, path::Path};
const APP: &str = "org.example.MruA";
const OTHER: &str = "org.example.MruC";
fn state(f: &Fixture) -> Result<Value> {
    f.state("windowMruAudit", "last-window-mru.json")
}
fn nodes(f: &Fixture, id: &str) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    Fixture::nodes(&f.tree()?, &mut all);
    Ok(all
        .into_iter()
        .filter(|n| n["app_id"] == id && n["shell"] == "xdg_shell" && n["window"].is_null())
        .collect())
}
fn focused(f: &Fixture) -> Result<Value> {
    let mut all = Vec::new();
    Fixture::nodes(&f.tree()?, &mut all);
    all.into_iter()
        .find(|n| n["focused"] == true && n["shell"] == "xdg_shell")
        .ok_or_else(|| "No focused native client".into())
}
fn focus(f: &Fixture, id: &Value) -> Result<()> {
    let number = id.as_u64().ok_or("Native container ID")?;
    f.call("swaymsg", &[&format!("[con_id={number}]"), "focus"])?;
    wait_true("exact native container focus", || {
        Ok(focused(f)?["id"] == *id)
    })
}
fn capture(f: &Fixture, con: &Value, pid: u32) -> Result<Value> {
    wait("exact conID/foreignID/process observer binding", || {
        Ok(state(f)?["nativeWindows"]
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|row| {
                    row["conId"] == *con
                        && row["appId"] == APP
                        && row["foreignId"].as_str().is_some_and(|id| !id.is_empty())
                        && row["instanceId"].as_str().is_some_and(|id| !id.is_empty())
                })
            })
            .cloned()
            .filter(|_| {
                nodes(f, APP).is_ok_and(|rows| {
                    rows.iter()
                        .any(|row| row["id"] == *con && row["pid"] == pid)
                })
            }))
    })
}
fn picker(f: &Fixture, action: &str) -> Result<Value> {
    Ok(serde_json::from_str(&f.ipc(
        "desktop",
        "appSwitcher",
        &[action],
    )?)?)
}
fn backing(f: &Fixture, con: &Value) -> Result<()> {
    wait_true(
        "observer records exact application backing-window MRU",
        || {
            Ok(state(f)?["nativeWindows"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["appId"] == APP))
                .is_some_and(|row| row["conId"] == *con))
        },
    )
}
fn select(f: &Fixture) -> Result<Value> {
    let mut value = picker(f, "begin")?;
    let count = value["items"].as_array().ok_or("Switcher items")?.len();
    for _ in 0..count {
        if value["key"] == APP {
            return Ok(value);
        }
        value = picker(f, "next")?;
    }
    Err("Captured application absent from switcher".into())
}
fn physical(f: &Fixture, key: &str) -> Result<()> {
    f.call("wtype", &["-k", key])?;
    Ok(())
}
fn retire(f: &Fixture, command: &str) -> Result<()> {
    let root = f.options.output.join("duplicate-client");
    let before = read(&root.join("client-state.json"))?["generation"]
        .as_u64()
        .ok_or("Client generation")?;
    fs::write(root.join("command"), command)?;
    wait_true("exact owned GTK window replacement", || {
        Ok(read(&root.join("client-state.json"))?["generation"] == before + 1)
    })
}
fn closed(f: &Fixture, con: &Value) -> Result<()> {
    wait_true("captured native container retired", || {
        Ok(!nodes(f, APP)?.iter().any(|row| row["id"] == *con))
    })
}
fn same_capture(before: &Value, after: &Value) -> Result<()> {
    for key in [
        "conId",
        "foreignId",
        "appId",
        "instanceId",
        "epoch",
        "groupInstances",
    ] {
        require(
            !before[key].is_null() && before[key] == after[key],
            format!("Native window capture changed {key}"),
        )?;
    }
    Ok(())
}
fn baseline(mut f: Fixture, desktop: usize, second: Value, other: Value) -> Result<()> {
    save(
        &f.options.output.join("baseline-expectation.json"),
        &json!({"expectedFailure":true,"scope":"Old title-only switcher dispatch baseline; independent exact conID assertion, not final native-window API acceptance","expectedSecondConId":second,"otherConId":other,"before":f.tree()?}),
    )?;
    f.stop(desktop)?;
    let qml = f.qml.clone();
    f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().ok_or("QML path")?],
        "desktop-restarted.log",
    )?;
    wait_true("baseline restarted shell maps duplicate surfaces", || {
        Ok(state(&f)?["windows"]
            .as_array()
            .is_some_and(|rows| rows.iter().filter(|row| row["appId"] == APP).count() == 2))
    })?;
    let decision = select(&f)?;
    save(
        &f.options.output.join("baseline-captured-decision.json"),
        &decision,
    )?;
    physical(&f, "Return")?;
    wait_true(
        "BASELINE exact duplicate-title second native window after shell restart",
        || {
            save(
                &f.options.output.join("baseline-native-after-commit.json"),
                &f.tree()?,
            )?;
            save(
                &f.options.output.join("baseline-switcher-after-commit.json"),
                &state(&f)?,
            )?;
            Ok(focused(&f)?["id"] == second)
        },
    )?;
    save(
        &f.options.output.join("baseline-unexpected-pass.json"),
        &json!({"passed":true,"scope":"Old-title-path diagnostic unexpectedly selected exact target; this is not full window MRU acceptance"}),
    )?;
    Ok(())
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    let qml = f.qml.clone();
    f.launch(
        Path::new("swaymsg"),
        &["-m", "-t", "subscribe", "[\"window\"]"],
        "native-window-events.log",
    )?;
    let desktop = f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().ok_or("QML path")?],
        "desktop.log",
    )?;
    wait_true("window MRU observer ready", || {
        Ok(state(&f)?["available"] == true)
    })?;
    f.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log")?;
    let client = f.sibling("lifecycle-client")?;
    let root = f.options.output.join("duplicate-client");
    fs::create_dir(&root)?;
    let owner = f.launch(
        &client,
        &[
            root.to_str().ok_or("Client path")?,
            APP,
            "--duplicate-titles",
        ],
        "duplicate-client.log",
    )?;
    let mut initial = wait("two mapped duplicate-title owned surfaces", || {
        let rows = nodes(&f, APP)?;
        Ok((rows.len() == 2
            && rows.iter().all(|r| {
                r["name"] == "Duplicate document" && r["pid"] == f.processes[owner].0.id()
            }))
        .then_some(rows))
    })?;
    initial.sort_by_key(|node| node["id"].as_u64());
    let first = initial[0]["id"].clone();
    let second = initial[1]["id"].clone();
    let other_root = f.options.output.join("other-client");
    fs::create_dir(&other_root)?;
    let other = f.launch(
        &client,
        &[other_root.to_str().ok_or("Other path")?, OTHER],
        "other-client.log",
    )?;
    let other_id = wait("other owned application mapped", || {
        Ok(nodes(&f, OTHER)?
            .into_iter()
            .find(|r| r["pid"] == f.processes[other].0.id())
            .map(|r| r["id"].clone()))
    })?;
    // Seed a deliberately non-creation order. Titles are identical and cannot
    // establish which native surface the later switcher decision owns.
    focus(&f, &first)?;
    focus(&f, &second)?;
    if f.options.scenario != "window-mru-baseline" {
        backing(&f, &second)?;
    }
    focus(&f, &other_id)?;
    if f.options.scenario == "window-mru-baseline" {
        return baseline(f, desktop, second, other_id);
    }
    let witness = capture(&f, &second, f.processes[owner].0.id())?;
    let first_witness = capture(&f, &first, f.processes[owner].0.id())?;
    require(
        witness["foreignId"] != first_witness["foreignId"],
        "Duplicate native windows share foreign ID",
    )?;
    save(
        &f.options.output.join("window-mru-before.json"),
        &json!({"first":first_witness,"second":witness,"native":initial,"state":state(&f)?}),
    )?;
    select(&f)?;
    physical(&f, "Escape")?;
    wait_true("physical preview Escape releases capture", || {
        Ok(state(&f)?["switcher"]["visible"] == false)
    })?;
    require(
        focused(&f)?["id"] == other_id,
        "Preview Escape changed native focus",
    )?;
    f.ipc("windowMruAudit", "restartObserver", &[])?;
    wait_true("observer outage", || Ok(state(&f)?["available"] == false))?;
    wait_true("observer recovers exact backing window", || {
        Ok(state(&f)?["available"] == true)
    })?;
    same_capture(&witness, &capture(&f, &second, f.processes[owner].0.id())?)?;
    f.stop(desktop)?;
    require(
        f.processes[owner].alive()? && f.processes[other].alive()?,
        "Shell restart exited native clients",
    )?;
    f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().ok_or("QML path")?],
        "desktop-restarted.log",
    )?;
    wait_true("new shell native observer ready", || {
        Ok(state(&f)?["available"] == true)
    })?;
    same_capture(&witness, &capture(&f, &second, f.processes[owner].0.id())?)?;
    save(
        &f.options.output.join("window-mru-recovered.json"),
        &state(&f)?,
    )?;
    select(&f)?;
    physical(&f, "Return")?;
    // This is the actual old title-only path's baseline: either duplicate title
    // must never substitute for the exact second native MRU surface.
    wait_true(
        "physical switcher chooses exact second native MRU after restart",
        || {
            save(
                &f.options.output.join("native-focus-after-commit.json"),
                &f.tree()?,
            )?;
            Ok(focused(&f)?["id"] == second)
        },
    )?;
    focus(&f, &other_id)?;
    wait_true("initial physical native reply completed", || {
        Ok(state(&f)?["switcher"]["phase"] == "committed")
    })?;
    wait_true("observed native source before overlapping decision", || {
        Ok(state(&f)?["nativeFocused"]["conId"] == other_id)
    })?;
    select(&f)?;
    let serial = state(&f)?["nativeSerial"]
        .as_u64()
        .ok_or("Native request serial missing")?;
    let overlap: Value =
        serde_json::from_str(&f.ipc("windowMruAudit", "overlappingCommitBegin", &[])?)?;
    save(
        &f.options
            .output
            .join("window-mru-overlapping-invocations.json"),
        &overlap,
    )?;
    let before = &overlap["before"];
    let after = &overlap["after"];
    let commits = before["commits"].as_u64().ok_or("Overlap commit count")?;
    require(
        before["visible"] == true
            && before["key"] == APP
            && before["nativeCapture"]["conId"] == second,
        "Overlap probe did not capture exact target application/window",
    )?;
    require(
        overlap["committed"]["phase"] == "pending"
            && after["phase"] == "pending"
            && after["visible"] == false
            && after["key"] == before["key"]
            && after["nativeCapture"] == before["nativeCapture"]
            && after["commits"] == commits
            && overlap["sourceBefore"] == overlap["sourceAfter"]
            && overlap["sourceBefore"]["conId"] == other_id,
        "New begin replaced a pending invocation target or source",
    )?;
    wait_true(
        "single exact native dispatch survives overlapping begin",
        || {
            let value = state(&f)?;
            Ok(focused(&f)?["id"] == second
                && value["nativeSerial"] == serial + 1
                && value["switcher"]["phase"] == "committed"
                && value["switcher"]["commits"] == commits + 1)
        },
    )?;
    picker(&f, "commit")?;
    let completed = state(&f)?;
    require(
        completed["nativeSerial"] == serial + 1 && completed["switcher"]["commits"] == commits + 1,
        "A closed invocation dispatched a duplicate native commit",
    )?;
    let fresh = picker(&f, "begin")?;
    require(
        fresh["visible"] == true && fresh["phase"] == "preview",
        "Completed native decision prevented a fresh preview",
    )?;
    physical(&f, "Escape")?;
    wait_true(
        "fresh preview after completed native dispatch closes",
        || Ok(state(&f)?["switcher"]["visible"] == false),
    )?;
    require(
        focused(&f)?["id"] == second,
        "Fresh preview Escape changed the committed exact target",
    )?;
    focus(&f, &other_id)?;
    let decision = select(&f)?;
    let commits = decision["commits"].clone();
    retire(&f, "close second")?;
    closed(&f, &second)?;
    physical(&f, "Return")?;
    wait_true("closed captured window rejects physical commit", || {
        let status = state(&f)?;
        Ok(status["switcher"]["visible"] == false
            && status["switcher"]["commits"] == commits
            && status["switcher"]["phase"] == "cancelled")
    })?;
    require(
        focused(&f)?["id"] == other_id,
        "Closed captured target fell back to its sibling",
    )?;
    // Create a fresh same-process duplicate, focus it to make a genuine new MRU
    // capture, then replace it after preview. Neither sibling nor replacement
    // is authorized by that old captured decision.
    retire(&f, "replace second")?;
    let replacement = wait("new same-process native surface", || {
        Ok(nodes(&f, APP)?
            .into_iter()
            .find(|r| r["id"] != first && r["id"] != second)
            .map(|r| r["id"].clone()))
    })?;
    focus(&f, &replacement)?;
    backing(&f, &replacement)?;
    focus(&f, &other_id)?;
    capture(&f, &replacement, f.processes[owner].0.id())?;
    let decision = select(&f)?;
    let commits = decision["commits"].clone();
    retire(&f, "close second")?;
    closed(&f, &replacement)?;
    retire(&f, "replace second")?;
    let newer = wait("replacement surface cannot inherit capture", || {
        Ok(nodes(&f, APP)?
            .into_iter()
            .find(|r| r["id"] != first && r["id"] != second && r["id"] != replacement)
            .map(|r| r["id"].clone()))
    })?;
    // The overlay still owns exclusive keyboard focus. Ask the compositor to
    // restore the exact underlying owner without treating its focused flag as
    // visible native focus until the physical decision releases that overlay.
    let before_restore = f.tree()?;
    require(
        nodes(&f, OTHER)?
            .iter()
            .any(|row| row["id"] == other_id && row["pid"] == f.processes[other].0.id()),
        "Underlying source container no longer belongs to the captured client",
    )?;
    let restore_reply: Value = serde_json::from_str(&f.call(
        "swaymsg",
        &[
            "-r",
            &format!(
                "[con_id={}]",
                other_id.as_u64().ok_or("Underlying source conID")?
            ),
            "focus",
        ],
    )?)?;
    require(
        restore_reply
            .as_array()
            .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| row["success"] == true)),
        "Compositor rejected exact underlying source focus",
    )?;
    save(
        &f.options
            .output
            .join("window-mru-exclusive-source-restore.json"),
        &json!({"sourceConId":other_id,"sourcePid":f.processes[other].0.id(),"before":before_restore,"reply":restore_reply,"after":f.tree()?,"switcher":state(&f)?["switcher"]}),
    )?;
    physical(&f, "Return")?;
    wait_true(
        "same-process replacement rejects stale physical commit",
        || {
            let status = state(&f)?;
            Ok(status["switcher"]["visible"] == false
                && status["switcher"]["commits"] == commits
                && status["switcher"]["phase"] == "cancelled"
                && focused(&f)?["id"] == other_id)
        },
    )?;
    require(
        focused(&f)?["id"] == other_id && f.processes[owner].alive()?,
        "Replacement inherited retired capture",
    )?;
    save(
        &f.options.output.join("window-mru-final.json"),
        &json!({"state":state(&f)?,"firstConId":first,"capturedSecondConId":second,"retiredReplacement":replacement,"newUncaptured":newer,"tree":f.tree()?}),
    )?;
    for child in f.processes.iter_mut().rev() {
        child.stop()?;
    }
    for name in ["desktop.log", "desktop-restarted.log"] {
        let log = fs::read_to_string(f.options.output.join(name))?;
        for marker in [
            "TypeError:",
            "ReferenceError:",
            "Failed to load configuration",
        ] {
            require(
                !log.contains(marker),
                format!("Native window MRU shell error {marker}"),
            )?;
        }
    }
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":["duplicate titles have distinct exact native conID/foreignID/ownedPID witnesses","preview and physical Escape preserve backing-window focus","observer restart preserves captured backing-window identity","whole shell restart restores exact second-window MRU","physical Return targets exact duplicate-title native MRU","closed captured target refuses sibling fallback","same-process replacement cannot inherit stale capture","overlapping begin preserves pending owner and source, dispatches once and permits fresh preview afterward"],"scope":"Native duplicate-title backing-window MRU and captured physical switcher decisions; no modal/space/reference parity claim"}),
    )?;
    Ok(())
}
