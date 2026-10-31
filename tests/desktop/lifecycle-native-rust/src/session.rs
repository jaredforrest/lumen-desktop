use crate::harness::{Fixture, Options, Result, pause, read, require, save, wait, wait_true};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

fn state(f: &Fixture) -> Result<Value> {
    f.state("sessionAudit", "last-state.json")
}
fn app_state(f: &Fixture) -> Result<Value> {
    read(&f.options.output.join("client-state.json"))
}
fn phase(f: &Fixture, value: &str) -> Result<bool> {
    Ok(state(f)?["session"]["phase"] == value)
}
fn application(f: &Fixture) -> Result<bool> {
    Ok(state(f)?["applications"].as_array().is_some_and(|apps| {
        apps.iter()
            .any(|app| app["appId"] == "org.example.SessionDocument")
    }))
}
fn prompt(f: &Fixture) -> Result<Option<Value>> {
    let mut nodes = Vec::new();
    Fixture::nodes(&f.tree()?, &mut nodes);
    Ok(nodes
        .into_iter()
        .find(|node| node["name"] == "Save session document?"))
}
fn focus_prompt(f: &Fixture) -> Result<Value> {
    let dialog = wait("native save prompt", || prompt(f))?;
    let id = dialog["id"].as_i64().ok_or("Prompt native ID missing")?;
    f.call("swaymsg", &[&format!("[con_id={id}]"), "focus"])?;
    pause(150);
    Ok(dialog)
}
fn choose(f: &Fixture, letter: &str) -> Result<()> {
    focus_prompt(f)?;
    f.call("wtype", &["-s", "200", "-M", "alt", letter, "-m", "alt"])?;
    Ok(())
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    let qml = f.qml.clone();
    f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().unwrap()],
        "shell.log",
    )?;
    wait_true("session lifecycle available", || {
        Ok(state(&f)?["available"] == true)
    })?;
    let client = f.sibling("session-client")?;
    let output = f.options.output.display().to_string();
    let first = f.launch(&client, &[&output], "client.log")?;
    wait_true("client status", || {
        Ok(f.options.output.join("client-state.json").exists())
    })?;
    wait_true("session document tracked", || application(&f))?;
    f.ipc("sessionAudit", "begin", &[])?;
    wait_true("preflight waiting", || {
        let phase = state(&f)?["session"]["phase"].clone();
        Ok(phase == "waiting" || phase == "blocked")
    })?;
    require(phase(&f, "waiting")?, "Preflight did not wait for document")?;
    wait_true("app-owned prompt", || Ok(app_state(&f)?["prompt"] == true))?;
    let dialog = wait("native prompt relationship", || prompt(&f))?;
    require(
        !dialog["lumen_relationships"]["parent"].is_null(),
        "Native prompt has no parent relationship",
    )?;
    f.call(
        "grim",
        &[f.options.output.join("app-prompt.png").to_str().unwrap()],
    )?;
    focus_prompt(&f)?;
    f.call("wtype", &["-s", "200", "-k", "Escape"])?;
    wait_true("app cancel choice", || {
        Ok(app_state(&f)?["choice"] == "cancel")
    })?;
    pause(1200);
    let marker = f.options.output.join("host-actions");
    require(
        phase(&f, "waiting")? && !marker.exists(),
        "Cancellation invoked final action",
    )?;
    f.ipc("sessionAudit", "cancel", &[])?;
    wait_true("session idle after cancel", || phase(&f, "idle"))?;
    require(
        f.processes[first].alive()? && !marker.exists(),
        "Cancellation exited client or invoked host",
    )?;
    f.ipc("sessionAudit", "begin", &[])?;
    wait_true("second prompt", || Ok(app_state(&f)?["prompt"] == true))?;
    let before_loss = state(&f)?;
    let retired_token = before_loss["session"]["token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .ok_or("Missing pending session token")?
        .to_owned();
    f.ipc("sessionAudit", "retireObserver", &[])?;
    wait_true(
        "owned observer retirement cancels pending authorization",
        || {
            let observed = state(&f)?;
            save(
                &f.options.output.join("observer-loss.json"),
                &json!({"before":before_loss,"after":observed}),
            )?;
            Ok(observed["available"] == false
                && observed["session"]["phase"] == "idle"
                && observed["error"]
                    == "Application observation was lost. The session remains open.")
        },
    )?;
    require(
        f.processes[first].alive()? && app_state(&f)?["prompt"] == true && !marker.exists(),
        "Observer retirement exited dirty application or invoked final action",
    )?;
    wait_true(
        "owned observer reconnects without resuming session action",
        || Ok(state(&f)?["available"] == true && application(&f)?),
    )?;
    require(
        phase(&f, "idle")? && state(&f)?["authorizationCount"] == 0 && !marker.exists(),
        "Observer reconnect reused retired session authorization",
    )?;
    f.ipc("sessionAudit", "authorize", &[&retired_token])?;
    wait_true("retired token is refused by the new observer", || {
        Ok(phase(&f, "blocked")?)
    })?;
    require(
        state(&f)?["authorizationCount"] == 0 && !marker.exists(),
        "Retired observer token authorized a host action",
    )?;
    save(
        &f.options
            .output
            .join("observer-reconnected-stale-token.json"),
        &state(&f)?,
    )?;
    focus_prompt(&f)?;
    f.call("wtype", &["-k", "Escape"])?;
    wait_true(
        "cancel original application prompt after observer loss",
        || Ok(app_state(&f)?["choice"] == "cancel" && app_state(&f)?["prompt"] == false),
    )?;
    f.ipc("sessionAudit", "cancel", &[])?;
    wait_true("idle after explicit observation-loss cancellation", || {
        phase(&f, "idle")
    })?;
    f.ipc("sessionAudit", "begin", &[])?;
    wait_true("fresh application prompt after observer loss", || {
        Ok(app_state(&f)?["prompt"] == true)
    })?;
    choose(&f, "s")?;
    wait_true("saved client exit", || Ok(!f.processes[first].alive()?))?;
    wait_true("session ready after save", || phase(&f, "ready"))?;
    require(
        fs::read_to_string(f.options.output.join("saved.txt"))? == "Private unsaved fixture text"
            && !marker.exists(),
        "Save or final-action boundary changed",
    )?;
    f.ipc("sessionAudit", "cancel", &[])?;
    wait_true("idle after saved cancellation", || phase(&f, "idle"))?;
    pause(1200);
    require(!marker.exists(), "Cancellation after save invoked host")?;
    fs::remove_file(f.options.output.join("client-state.json"))?;
    let second = f.launch(&client, &[&output], "client-discard.log")?;
    wait_true("fresh dirty client", || {
        Ok(f.options.output.join("client-state.json").exists())
    })?;
    wait_true("fresh session document tracked", || application(&f))?;
    f.ipc("sessionAudit", "begin", &[])?;
    wait_true("discard prompt", || Ok(app_state(&f)?["prompt"] == true))?;
    choose(&f, "d")?;
    wait_true("discard client exit", || Ok(!f.processes[second].alive()?))?;
    wait_true("ready after discard", || phase(&f, "ready"))?;
    require(!marker.exists(), "Implicit final action after discard")?;
    f.ipc("sessionAudit", "confirm", &[])?;
    wait_true("explicit host stub", || {
        Ok(fs::read_to_string(&marker)
            .map(|text| text.lines().collect::<Vec<_>>() == vec!["reboot"])
            .unwrap_or(false))
    })?;
    require(
        fs::read_to_string(&marker)?.lines().collect::<Vec<_>>() == vec!["reboot"],
        "Host stub called more than once",
    )?;
    pause(300);
    require(
        fs::read_to_string(&marker)?.lines().collect::<Vec<_>>() == vec!["reboot"],
        "Host stub repeated asynchronously",
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":[
            "native app-owned modal save/cancel/discard",
            "cancel preserves live dirty application",
            "owned observer loss cancels authorization; reconnect refuses retired token and requires fresh user preflight",
            "save persists document",
            "no implicit final action after ready or cancellation",
            "explicit fresh confirmation invokes injected final-action stub once"
        ]}),
    )?;
    Ok(())
}
