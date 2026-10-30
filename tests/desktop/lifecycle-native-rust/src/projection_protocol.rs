//! Real xdg-shell fullscreen ingress; all receipts describe owned disposable views.
use super::*;
use std::io::Write;
use std::path::{Path, PathBuf};
fn rows(path: &Path) -> Result<Vec<Value>> {
    let bytes = fs::read(path)?;
    require(bytes.len() < 65536, "Protocol log exceeds bound")?;
    std::str::from_utf8(&bytes)?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}
fn launch(f: &mut Fixture, fixed: bool, label: &str) -> Result<(usize, u64, PathBuf)> {
    let program = std::env::split_paths(std::ffi::OsStr::new(f.vars.get("PATH").ok_or("PATH")?))
        .map(|p| p.join("lumen-projection-protocol-client"))
        .find(|p| p.is_file())
        .ok_or("Pinned protocol client missing")?
        .canonicalize()?;
    require(
        program.starts_with("/nix/store"),
        "Immutable protocol client required",
    )?;
    let log = f.options.output.join(format!("{label}.log"));
    let process = f.launch_input_args(
        &program,
        if fixed { &["fixed"] } else { &[] },
        log.file_name()
            .and_then(|p| p.to_str())
            .ok_or("Protocol log name")?,
    )?;
    let pid = f.processes[process].0.id();
    let id = wait("actual owned protocol native view", || {
        let ready = rows(&log)?.iter().any(|r| r["ready"] == true);
        let node = nodes(f)?
            .into_iter()
            .find(|n| n["pid"] == pid && n["name"] == "Projection protocol 0");
        Ok(ready
            .then_some(node)
            .flatten()
            .and_then(|n| n["id"].as_u64()))
    })?;
    Ok((process, id, log))
}
fn send(f: &mut Fixture, process: usize, log: &Path, command: &str, label: &str) -> Result<()> {
    let prior = rows(log)?.iter().filter(|r| r["ack"] == command).count();
    let before = f.tree()?;
    let input = f.processes[process]
        .0
        .stdin
        .as_mut()
        .ok_or("Owned protocol stdin")?;
    writeln!(input, "{command}")?;
    input.flush()?;
    wait_true("real client roundtrip acknowledgement", || {
        let transcript = rows(log)?;
        save(
            &f.options.output.join(format!("protocol-{label}.json")),
            &json!({"command":command,"before":before,"after":f.tree()?,"transcript":transcript}),
        )?;
        Ok(transcript.iter().filter(|r| r["ack"] == command).count() > prior)
    })
}
fn unchanged(f: &mut Fixture, p: usize, log: &Path, command: &str, label: &str) -> Result<()> {
    let before = f.tree()?;
    send(f, p, log, command, label)?;
    let after = f.tree()?;
    save(
        &f.options.output.join(format!("unchanged-{label}.json")),
        &json!({"before":before,"after":after}),
    )?;
    require(
        before == after,
        format!("Fullscreen refusal changed native tree: {label}"),
    )
}
pub(super) fn run(f: &mut Fixture, primary: u64) -> Result<()> {
    let (fixed, fixed_id, fixed_log) = launch(f, true, "fixed-protocol")?;
    request(f, &operation(&view(f, fixed_id)?, true)?, None)?;
    request(f, "workspace 3", None)?;
    request(f, &format!("[con_id={fixed_id}] focus"), None)?;
    let before = f.tree()?;
    request(
        f,
        &format!("[con_id={fixed_id}] lumen_window fullscreen"),
        Some("Window action is unavailable"),
    )?;
    let after = f.tree()?;
    save(
        &f.options.output.join("fixed-fullscreen-refusal.json"),
        &json!({"before":before,"after":after,"client":rows(&fixed_log)?}),
    )?;
    require(
        before == after,
        "Unavailable fixed-window action changed Space/focus/native state",
    )?;
    f.stop(fixed)?;
    wait_true("fixed client removed", || {
        Ok(!nodes(f)?.iter().any(|n| n["id"] == fixed_id))
    })?;
    request(f, "workspace 2", None)?;
    request(f, "create_output", None)?;
    wait_true("second output for real output-directed request", || {
        let output: Value =
            serde_json::from_str(&f.call("swaymsg", &["-t", "get_outputs", "-r"])?)?;
        Ok(output
            .as_array()
            .is_some_and(|r| r.iter().any(|o| o["name"] == "HEADLESS-2")))
    })?;
    request(f, "output HEADLESS-2 position 1440 0", None)?;
    let (client, id, log) = launch(f, false, "ordinary-protocol")?;
    request(f, &operation(&view(f, id)?, true)?, None)?;
    let origin = view(f, id)?["lumen_all_desktops"]["origin_workspace"].clone();
    request(f, "workspace 3", None)?;
    request(f, "move workspace to output HEADLESS-1", None)?;
    request(f, &format!("[con_id={primary}] focus"), None)?;
    require(
        view(f, id)?["lumen_all_desktops"]["projected"] == true
            && current_workspace(f)?["name"] == "3",
        "Background fullscreen fixture requires an off-origin projected target",
    )?;
    for (command, label) in [
        ("fullscreen none", "background-no-output"),
        ("fullscreen HEADLESS-1", "background-owner-output"),
        ("fullscreen HEADLESS-2", "background-other-output"),
    ] {
        unchanged(f, client, &log, command, label)?;
    }
    send(f, client, &log, "modal", "real-modal-child")?;
    wait_true("native modal-owner suspension", || {
        Ok(view(f, id)?["lumen_all_desktops"]["supported"] == false)
    })?;
    request(f, "workspace 3", None)?;
    request(f, &format!("[con_id={primary}] focus"), None)?;
    unchanged(
        f,
        client,
        &log,
        "fullscreen HEADLESS-1",
        "suspended-owner-output",
    )?;
    require(
        view(f, id)?["lumen_all_desktops"]["origin_workspace"] == origin,
        "Suspended output request moved structural origin",
    )?;
    send(f, client, &log, "unmodal", "remove-modal-child")?;
    wait_true("native modal owner eligible again", || {
        Ok(view(f, id)?["lumen_all_desktops"]["supported"] == true)
    })?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    send(
        f,
        client,
        &log,
        "fullscreen none",
        "foreground-native-fullscreen",
    )?;
    wait_true("native foreground fullscreen on retained origin", || {
        Ok(view(f, id)?["fullscreen_mode"] == 1 && current_workspace(f)?["name"] == origin["name"])
    })?;
    send(
        f,
        client,
        &log,
        "unfullscreen",
        "foreground-native-unfullscreen",
    )?;
    wait_true("native fullscreen exited", || {
        Ok(view(f, id)?["fullscreen_mode"] == 0)
    })?;
    request(f, "workspace 3", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    request(f, &format!("[con_id={id}] fullscreen enable global"), None)?;
    require(
        view(f, id)?["fullscreen_mode"] == 2 && current_workspace(f)?["name"] == origin["name"],
        "Standard global fullscreen lost usable retained origin",
    )?;
    request(f, &format!("[con_id={id}] fullscreen disable"), None)?;
    require(
        view(f, id)?["lumen_all_desktops"]["origin_workspace"] == origin,
        "Fullscreen exit rewrote structural origin",
    )?;
    let old = view(f, id)?;
    request(
        f,
        &format!("[con_id={id}] move container to output HEADLESS-2"),
        None,
    )?;
    wait_true("explicit output move updates native origin", || {
        Ok(view(f, id)?["lumen_all_desktops"]["origin_output"]["name"] == "HEADLESS-2")
    })?;
    let before_stale = f.tree()?;
    request(
        f,
        &operation(&old, false)?,
        Some("Projection witness unavailable"),
    )?;
    require(
        before_stale == f.tree()?,
        "Old output witness changed native tree",
    )?;
    request(f, "output HEADLESS-2 disable", None)?;
    wait_true(
        "output removal preserves valid bounded fallback metadata",
        || {
            let node = view(f, id)?;
            Ok(node["lumen_all_desktops"]["enabled"] == true
                && node["lumen_all_desktops"]["origin_output"]["name"] == "HEADLESS-1"
                && lumen_window_policy::projection::parse(&node).is_ok())
        },
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":8,"origin":origin,"final":f.tree()?,"scope":"Real fixed-size role refusal; acknowledged background native fullscreen no/same/other-output requests; suspended modal-owner same-output refusal; foreground native and standard global fullscreen usable origin; explicit move/output removal and stale-origin refusal"}),
    )?;
    Ok(())
}
