//! Actual native URI Copy, physical Files Ctrl+Z, then the existing Put Back service route.
use crate::{
    files_drag::{file_uri, offer, state},
    harness::{Fixture, Result, require, save, wait_true},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn job(value: &Value, operation: &str) -> Option<Value> {
    value["jobs"]
        .as_array()?
        .iter()
        .find(|item| item["operation"] == operation)
        .cloned()
}
fn focused(value: &Value, title: &str) -> bool {
    if value["name"] == title && value["focused"] == true {
        return true;
    }
    ["nodes", "floating_nodes"].iter().any(|key| {
        value[*key]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|child| focused(child, title)))
    })
}
pub fn run(f: &mut Fixture, qml: &Path, peer: &Path, folder: &Path, incoming: &Path) -> Result<()> {
    // The namespace's /tmp and retained /artifacts may be distinct mounts.
    // Mirror ordinary same-volume home/Trash placement for this supported Undo
    // journey; cross-volume refusal is independently covered by endpoint tests.
    let destination = f.options.output.join("CopyUndoFiles");
    fs::create_dir(&destination)?;
    for name in ["drag space # café.txt", "second.txt"] {
        fs::copy(folder.join(name), destination.join(name))?;
    }
    let folder = destination.as_path();
    f.ipc(
        qml,
        "filesDragTest",
        "navigate",
        &[folder.to_str().ok_or("Copy Undo destination")?],
    )?;
    wait_true("same-volume owned Files destination listing", || {
        let current = state(f, qml)?;
        Ok(current["path"] == folder.display().to_string()
            && current["busy"] == false
            && current["rows"]
                .as_array()
                .is_some_and(|rows| rows.len() == 2))
    })?;
    let original = incoming.join("copy undo # café.txt");
    let output = folder.join(original.file_name().ok_or("Incoming basename")?);
    let payload = b"Native Copy Undo retains the original source.\n";
    fs::write(&original, payload)?;
    let receipt = offer(
        f,
        qml,
        peer,
        "copy-undo",
        &file_uri(&original),
        "text/uri-list",
        1,
        None,
        false,
        None,
    )?;
    wait_true("native Copy completed with exact named Undo", || {
        let current = state(f, qml)?;
        Ok(job(&current, "copy").is_some_and(|item| {
            item["state"] == "complete"
                && item["completed"] == 1
                && item["selected"] == output.display().to_string()
        }) && current["working"] == false
            && current["canUndo"] == true
            && current["undoKind"] == "copy"
            && current["undoLabel"] == "Undo Copy of “copy undo # café.txt”")
    })?;
    require(
        fs::read(&original)? == payload && fs::read(&output)? == payload,
        "Native Copy source/output bytes differ",
    )?;
    let before = state(f, qml)?;
    require(
        before["jobs"]
            .as_array()
            .is_some_and(|jobs| jobs.len() == 1),
        "Copy started unexpected extra jobs",
    )?;
    save(&f.options.output.join("copy-complete.json"), &before)?;
    f.screenshot("copy-complete.png", false)?;

    // This only establishes focus. Undo itself must arrive through the real
    // compositor virtual keyboard and FilesWindow Keys.onPressed handler.
    f.ipc(qml, "filesDragTest", "focusContents", &[])?;
    wait_true("actual Files native and content focus", || {
        let current = state(f, qml)?;
        let title = current["title"].as_str().ok_or("Files title")?;
        let tree: Value = serde_json::from_str(&f.sway(&["-t", "get_tree", "-r"])?)?;
        Ok(current["contentFocus"] == true && focused(&tree, title))
    })?;
    save(
        &f.options.output.join("before-keyboard.json"),
        &json!({"files":state(f,qml)?,"tree":serde_json::from_str::<Value>(&f.sway(&["-t","get_tree","-r"])?)?,"physicalCommand":["wtype","-M","ctrl","-k","z","-m","ctrl"]}),
    )?;
    f.call("wtype", &["-M", "ctrl", "-k", "z", "-m", "ctrl"])?;
    wait_true("physical Ctrl+Z completed one Copy Undo", || {
        let current = state(f, qml)?;
        Ok(current["working"] == false
            && job(&current, "undo")
                .is_some_and(|item| item["state"] == "complete" && item["completed"] == 1))
    })?;
    let after = state(f, qml)?;
    require(
        after["jobs"].as_array().is_some_and(|jobs| jobs.len() == 2),
        "Ctrl+Z started anything other than one Undo",
    )?;
    require(
        after["error"] == ""
            && after["canUndo"] == false
            && !output.exists()
            && fs::read(&original)? == payload,
        "Undo changed source, retained destination or advertised consumed record",
    )?;
    let undo = job(&after, "undo").ok_or("Completed Undo receipt")?;
    let recovery = PathBuf::from(
        undo["recoveryPath"]
            .as_str()
            .ok_or("Verified Trash recovery path")?,
    );
    let trash =
        PathBuf::from(f.env.get("XDG_DATA_HOME").ok_or("Private data directory")?).join("Trash");
    require(
        recovery.parent() == Some(trash.join("files").as_path()) && fs::read(&recovery)? == payload,
        "Undo recovery is not the standard Trash payload",
    )?;
    let token = recovery
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Exact Trash token")?;
    let info = trash.join("info").join(format!("{token}.trashinfo"));
    let record = fs::read_to_string(&info)?;
    require(
        record.starts_with("[Trash Info]\nPath=")
            && record.contains(&format!(
                "Path={}\n",
                file_uri(&output).trim_start_matches("file://")
            )),
        "Trash record did not retain the exact escaped copied destination",
    )?;
    require(
        undo["undoMessage"]
            .as_str()
            .is_some_and(|text| text.contains("Put Back")),
        "Undo omitted the existing recovery route",
    )?;
    save(
        &f.options.output.join("undo-complete.json"),
        &json!({"files":after,"trashPayload":recovery,"trashInfo":info,"record":record}),
    )?;
    f.screenshot("undo-complete.png", false)?;

    f.ipc(qml, "filesDragTest", "navigate", &["trash:"])?;
    wait_true("real Files listing contains exact Undo Trash token", || {
        let current = state(f, qml)?;
        Ok(current["path"] == "trash:"
            && current["busy"] == false
            && current["rows"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|row| row["path"] == recovery.display().to_string())
            }))
    })?;
    f.ipc(
        qml,
        "filesDragTest",
        "selectPath",
        &[recovery.to_str().ok_or("Trash path")?],
    )?;
    let selected = state(f, qml)?;
    require(
        selected["selected"] == json!([recovery]),
        "Put Back selection did not capture exact Trash token",
    )?;
    save(&f.options.output.join("put-back-selected.json"), &selected)?;
    // Same production service call as the existing Put Back menu item;
    // this deliberately makes no claim about physically clicking that menu.
    f.ipc(qml, "filesDragTest", "putBackSelected", &[])?;
    wait_true("existing Put Back service restored copied bytes", || {
        let current = state(f, qml)?;
        Ok(current["working"] == false
            && output.exists()
            && job(&current, "restore")
                .is_some_and(|item| item["state"] == "complete" && item["completed"] == 1))
    })?;
    let restored = state(f, qml)?;
    require(
        restored["jobs"]
            .as_array()
            .is_some_and(|jobs| jobs.len() == 3),
        "Put Back started extra mutations",
    )?;
    require(
        fs::read(&original)? == payload
            && fs::read(&output)? == payload
            && !recovery.exists()
            && !info.exists(),
        "Put Back did not restore exact bytes or left active Trash entry",
    )?;
    require(
        fs::read_to_string(folder.join("drag space # café.txt"))? == "original outbound bytes"
            && fs::read_to_string(folder.join("second.txt"))? == "second outbound bytes",
        "Undo/Put Back changed unrelated Files rows",
    )?;
    save(
        &f.options.output.join("acceptance.json"),
        &json!({"passed":true,"cases":["native Copy accepted","target-specific Undo label","source/output bytes","physical Ctrl+Z with native Files focus","single Undo job","standard Trash payload and record","exact selected Trash token","existing Put Back service restores bytes","unrelated outputs preserved"],"nativeCopy":receipt,"files":restored,"backend":f.options.sway,"effects":f.options.effects,"assets":f.assets,"scope":"Real Qt native URI offer into actual FilesState/FilesWindow, physical compositor Ctrl+Z through production key handler, existing Put Back service route. No physical context-menu click or power-loss claim."}),
    )?;
    f.screenshot("put-back-complete.png", false)?;
    println!(
        "PASS native Copy, physical Files Ctrl+Z, standard Trash and selected-token Put Back service outcome"
    );
    Ok(())
}
