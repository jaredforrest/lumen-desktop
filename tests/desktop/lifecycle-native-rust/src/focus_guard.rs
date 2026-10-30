//! Actual capture-to-dispatch races against the typed compositor command.
use crate::harness::{Fixture, Options, Result, require, save, wait, wait_true};
use lumen_window_policy::{guard, native_focus_plan};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};
fn rpc(f: &Fixture, kind: u32, payload: &str) -> Result<Value> {
    let mut socket = UnixStream::connect(f.vars.get("SWAYSOCK").ok_or("Private socket missing")?)?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut packet = b"i3-ipc".to_vec();
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(&kind.to_le_bytes());
    packet.extend_from_slice(payload.as_bytes());
    socket.write_all(&packet)?;
    let mut header = [0; 14];
    socket.read_exact(&mut header)?;
    require(
        &header[..6] == b"i3-ipc" && u32::from_le_bytes(header[10..14].try_into()?) == kind,
        "Private IPC response mismatch",
    )?;
    let len = u32::from_le_bytes(header[6..10].try_into()?) as usize;
    require(len <= 4 * 1024 * 1024, "Fixture reply exceeds bound")?;
    let mut bytes = vec![0; len];
    socket.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn flat(tree: &Value) -> Vec<Value> {
    let mut all = Vec::new();
    Fixture::nodes(tree, &mut all);
    all
}
fn node(f: &Fixture, title: &str) -> Result<Value> {
    wait("owned client native surface", || {
        Ok(flat(&f.tree()?).into_iter().find(|n| {
            n["name"] == title
                && n["shell"] == "xdg_shell"
                && n["foreign_toplevel_identifier"].is_string()
        }))
    })
}
fn focused(tree: &Value) -> Result<Value> {
    flat(tree)
        .into_iter()
        .find(|n| n["focused"] == true && n["shell"] == "xdg_shell")
        .ok_or_else(|| "No native focus".into())
}
fn yes(f: &Fixture, command: &str) -> Result<()> {
    let reply = rpc(f, 0, command)?;
    require(
        reply
            .as_array()
            .is_some_and(|r| !r.is_empty() && r.iter().all(|r| r["success"] == true)),
        format!("Setup command rejected: {command}: {reply}"),
    )
}
fn focus(f: &Fixture, id: &Value) -> Result<()> {
    yes(f, &format!("[con_id={id}] focus"))?;
    wait_true("actual captured native focus", || {
        Ok(focused(&f.tree()?)?["id"] == *id)
    })
}
fn captured(f: &Fixture, id: &Value, unhide: &[i64], space: bool) -> Result<Vec<u8>> {
    let tree = f.tree()?;
    let source = focused(&tree)?;
    let source = json!({"kind":"native","conId":source["id"],"foreignId":source["foreign_toplevel_identifier"]});
    let plan = native_focus_plan(&tree, id.as_i64().ok_or("Container ID")?, unhide, space)?;
    Ok(guard::packet(&tree, &source, &plan.target, &plan.actions)?)
}
fn dispatch(f: &Fixture, packet: &[u8], success: bool, label: &str) -> Result<()> {
    let before = f.tree()?;
    let command = guard::command(packet)?;
    let reply = rpc(f, 0, &command)?;
    let after = f.tree()?;
    save(
        &f.options.output.join(format!("guard-{label}.json")),
        &json!({"packetHex":command.strip_prefix("lumen_focus_guard "),"before":before,"reply":reply,"after":after}),
    )?;
    require(
        reply
            .as_array()
            .is_some_and(|r| r.len() == 1 && r[0]["success"] == success),
        format!("Guard result differs: {label}: {reply}"),
    )?;
    if !success {
        require(
            reply[0]["error"]
                .as_str()
                .is_some_and(|e| e.starts_with("LFG_REJECTED:")),
            "Rejection was not preflight-only",
        )?;
        require(
            focused(&before).ok().map(|n| n["id"].clone())
                == focused(&after).ok().map(|n| n["id"].clone()),
            "Rejected guard changed source focus",
        )?;
    }
    Ok(())
}
fn client_command(f: &mut Fixture, index: usize, text: &str) -> Result<()> {
    let log = f.options.output.join("relationships.log");
    let before = fs::read_to_string(&log)?.len();
    f.processes[index]
        .0
        .stdin
        .as_mut()
        .ok_or("Client stdin missing")?
        .write_all(format!("{text}\n").as_bytes())?;
    wait_true("client protocol acknowledgement", || {
        Ok(fs::read_to_string(&log)?
            .get(before..)
            .is_some_and(|s| s.contains(&format!("ack {text}\n"))))
    })
}
fn witness_len(packet: &[u8], offset: usize) -> usize {
    16 + u32::from_be_bytes(packet[offset + 12..offset + 16].try_into().unwrap()) as usize
}
fn action_offsets(packet: &[u8]) -> Vec<usize> {
    let mut offset = 5;
    if packet[4] == 1 {
        offset += witness_len(packet, offset);
    }
    offset += witness_len(packet, offset);
    offset += 32;
    let count = u32::from_be_bytes(packet[offset..offset + 4].try_into().unwrap()) as usize;
    offset += 4;
    let mut result = Vec::new();
    for _ in 0..count {
        result.push(offset);
        offset += 1 + witness_len(packet, offset + 1);
    }
    result
}
fn unsupported(mut f: Fixture) -> Result<()> {
    require(
        rpc(&f, 7, "")?["native_focus_guard"] != 2,
        "Missing-capability fixture unexpectedly supports guard",
    )?;
    let observer = f.launch_input(Path::new("lumen-application-lifecycle"), "observer.log")?;
    let latest = || -> Result<Value> {
        let text = fs::read_to_string(f.options.output.join("observer.log"))?;
        text.lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line).ok())
            .ok_or_else(|| "No observer output".into())
    };
    wait_true("observer explicitly reports unsupported guard", || {
        let v = latest()?;
        Ok(v["available"] == true
            && v["nativeGuardAvailable"] == false
            && v["nativeWindows"] == json!([]))
    })?;
    let before = focused(&f.tree()?)?["id"].clone();
    f.processes[observer]
        .0
        .stdin
        .as_mut()
        .ok_or("Observer stdin")?
        .write_all(b"{\"action\":\"focus_native\",\"requestId\":\"unsupported-1\"}\n")?;
    let result = wait("correlated explicit unsupported result", || {
        let text = fs::read_to_string(f.options.output.join("observer.log"))?;
        Ok(text
            .lines()
            .rev()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|value| {
                value["result"]["nativeFocus"]["requestId"] == "unsupported-1"
                    && value["result"]["nativeFocus"]["unsupported"] == true
            }))
    })?;
    require(
        result["result"]["nativeFocus"]["requestId"] == "unsupported-1"
            && result["result"]["nativeFocus"]["requested"] == false,
        "Unsupported result lost owner",
    )?;
    require(
        focused(&f.tree()?)?["id"] == before,
        "Unsupported guard changed focus",
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":["missing capability hides exact native captures","explicit correlated unsupported result without dispatch","native focus preserved"],"state":result}),
    )
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    f.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log")?;
    let path = f
        .options
        .relationships_client
        .clone()
        .ok_or("--relationships-client required")?;
    let client = f.launch_input(&path, "relationships.log")?;
    let a = node(&f, "Relationship 0")?;
    let b = node(&f, "Relationship 1")?;
    let modal = node(&f, "Relationship 2")?;
    let other = node(&f, "Relationship 3")?;
    let pid = f.processes[client].0.id();
    require(
        [&a, &b, &modal, &other].iter().all(|n| n["pid"] == pid),
        "Native client process witness mismatch",
    )?;
    focus(&f, &other["id"])?;
    wait_true("actual native keyboard recipient ready", || {
        Ok(fs::read_to_string(f.options.output.join("relationships.log"))?.contains("enter 3\n"))
    })?;
    if f.options.scenario == "focus-guard-unsupported" {
        return unsupported(f);
    }
    require(
        rpc(&f, 7, "")?["native_focus_guard"] == 2,
        "Guard v2 not advertised",
    )?;
    yes(&f, "workspace 19")?;
    wait_true("affirmative empty-workspace client focus", || {
        Ok(focused(&f.tree()?).is_err())
    })?;
    let empty_tree = f.tree()?;
    let empty_plan = native_focus_plan(&empty_tree, a["id"].as_i64().unwrap(), &[], true)?;
    let empty_packet = guard::packet(
        &empty_tree,
        &json!({"kind":"none"}),
        &empty_plan.target,
        &empty_plan.actions,
    )?;
    dispatch(&f, &empty_packet, true, "positive-no-focused-client")?;
    require(
        focused(&f.tree()?)?["id"] == a["id"],
        "Affirmative empty source did not focus captured target",
    )?;
    focus(&f, &other["id"])?;
    let layer_path = f.options.output.join("guard-layer.qml");
    fs::write(
        &layer_path,
        r##"import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
ShellRoot {
 PanelWindow {id:panel;implicitWidth:120;implicitHeight:80;visible:true;color:"#222222";WlrLayershell.namespace:"lumen-guard-source-test";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
  Item{id:keys;focus:true;Component.onCompleted:forceActiveFocus()}
 }
 IpcHandler{target:"guardLayer";function status():string{return JSON.stringify({active:keys.activeFocus && keys.Window.active,visible:panel.visible});}}
}
"##,
    )?;
    let layer = f.launch(
        Path::new("quickshell"),
        &["-p", layer_path.to_str().ok_or("Layer path")?],
        "guard-layer.log",
    )?;
    wait_true("owned exclusive layer actually owns keyboard focus", || {
        let state: Value = serde_json::from_str(&f.call(
            "quickshell",
            &[
                "-p",
                layer_path.to_str().unwrap(),
                "ipc",
                "call",
                "guardLayer",
                "status",
            ],
        )?)?;
        Ok(state["active"] == true && focused(&f.tree()?).is_err())
    })?;
    let layer_tree = f.tree()?;
    let layer_plan = native_focus_plan(&layer_tree, a["id"].as_i64().unwrap(), &[], true)?;
    let false_none = guard::packet(
        &layer_tree,
        &json!({"kind":"none"}),
        &layer_plan.target,
        &layer_plan.actions,
    )?;
    dispatch(&f, &false_none, false, "focused-layer-is-not-none")?;
    let state: Value = serde_json::from_str(&f.call(
        "quickshell",
        &[
            "-p",
            layer_path.to_str().unwrap(),
            "ipc",
            "call",
            "guardLayer",
            "status",
        ],
    )?)?;
    require(
        state["active"] == true,
        "Rejected guard stole layer keyboard focus",
    )?;
    f.stop(layer)?;
    focus(&f, &other["id"])?;
    let initial = captured(&f, &a["id"], &[], true)?;
    dispatch(&f, &initial, true, "positive-focus")?;
    wait_true("typed focus reaches exact native target", || {
        Ok(focused(&f.tree()?)?["id"] == a["id"])
    })?;
    // Retain the old sampled-command race as an actual baseline on this compositor.
    focus(&f, &other["id"])?;
    let baseline_tree = f.tree()?;
    let old = native_focus_plan(&baseline_tree, a["id"].as_i64().unwrap(), &[], true)?
        .command
        .ok_or("Baseline command")?;
    focus(&f, &b["id"])?;
    yes(&f, &old)?;
    require(
        focused(&f.tree()?)?["id"] == a["id"],
        "Unconditional baseline did not reproduce stale-focus dispatch",
    )?;
    save(
        &f.options.output.join("unguarded-race-baseline.json"),
        &json!({"captured":baseline_tree,"interveningConId":b["id"],"command":old,"after":f.tree()?}),
    )?;
    focus(&f, &other["id"])?;
    let stale = captured(&f, &a["id"], &[], true)?;
    focus(&f, &b["id"])?;
    dispatch(&f, &stale, false, "source-race")?;
    focus(&f, &other["id"])?;
    let stale = captured(&f, &a["id"], &[], true)?;
    client_command(&mut f, client, "modal")?;
    wait_true("actual new protocol modal", || {
        Ok(node(&f, "Relationship 2")?["lumen_relationships"]["modal"] == true)
    })?;
    dispatch(&f, &stale, false, "new-modal-race")?;
    let positive = captured(&f, &a["id"], &[], true)?;
    dispatch(&f, &positive, true, "positive-modal")?;
    require(
        focused(&f.tree()?)?["id"] == modal["id"],
        "Shared modal redirection changed",
    )?;
    client_command(&mut f, client, "unmodal")?;
    focus(&f, &other["id"])?;
    let stale = captured(&f, &b["id"], &[], true)?;
    client_command(&mut f, client, "close-second")?;
    wait_true("captured target actually closed", || {
        Ok(!flat(&f.tree()?).iter().any(|n| n["id"] == b["id"]))
    })?;
    dispatch(&f, &stale, false, "closed-target-race")?;
    client_command(&mut f, client, "reopen-second")?;
    let replacement = node(&f, "Relationship 1")?;
    require(
        replacement["pid"] == b["pid"]
            && replacement["foreign_toplevel_identifier"] != b["foreign_toplevel_identifier"],
        "Replacement lacks distinct same-process surface witness",
    )?;
    focus(&f, &other["id"])?;
    dispatch(&f, &stale, false, "same-process-replacement-race")?;
    let current = captured(&f, &a["id"], &[], true)?;
    for (label, packet) in [
        ("truncated", current[..current.len() - 1].to_vec()),
        ("trailing", {
            let mut p = current.clone();
            p.push(0);
            p
        }),
        ("unknown-version", {
            let mut p = current.clone();
            p[3] = b'9';
            p
        }),
        ("unknown-source", {
            let mut p = current.clone();
            p[4] = 2;
            p
        }),
        ("nul-source-identifier", {
            let mut p = current.clone();
            p[21] = 0;
            p
        }),
        ("unknown-operation", {
            let mut p = current.clone();
            let offset = *action_offsets(&p).last().ok_or("Focus action missing")?;
            p[offset] = 99;
            p
        }),
    ] {
        dispatch(&f, &packet, false, label)?;
    }
    for (label, token) in [
        ("invalid-hex", "0z0z0z0z".to_owned()),
        ("odd-hex", "012345678".to_owned()),
        ("oversize-token", "0".repeat(guard::MAX_BYTES * 2 + 2)),
    ] {
        let before = focused(&f.tree()?)?["id"].clone();
        let reply = rpc(&f, 0, &format!("lumen_focus_guard {token}"))?;
        save(
            &f.options.output.join(format!("guard-{label}.json")),
            &json!({"tokenBytes":token.len(),"reply":reply,"tree":f.tree()?}),
        )?;
        require(
            reply[0]["success"] == false
                && reply[0]["error"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("LFG_REJECTED:")),
            "Malformed token was not rejected before dispatch",
        )?;
        require(
            focused(&f.tree()?)?["id"] == before,
            "Malformed token changed focus",
        )?;
    }
    let moved = captured(&f, &a["id"], &[], true)?;
    yes(
        &f,
        &format!("[con_id={}] move container to workspace 2", a["id"]),
    )?;
    dispatch(&f, &moved, false, "workspace-topology-race")?;
    yes(
        &f,
        &format!("[con_id={}] move container to workspace 1", a["id"]),
    )?;
    focus(&f, &other["id"])?;
    for (id, label) in [(&a["id"], "a"), (&replacement["id"], "b")] {
        yes(&f, &format!("[con_id={id}] lumen_window hide"))?;
        save(
            &f.options.output.join(format!("hidden-{label}.json")),
            &f.tree()?,
        )?;
    }
    focus(&f, &other["id"])?;
    let group = [
        a["id"].as_i64().unwrap(),
        replacement["id"].as_i64().unwrap(),
    ];
    let valid = captured(&f, &a["id"], &group, true)?;
    let offsets = action_offsets(&valid);
    require(offsets.len() >= 3, "Missing group unhide actions")?;
    let mut duplicate = valid.clone();
    let first = valid[offsets[0] + 1..offsets[1]].to_vec();
    let second_end = offsets[1] + 1 + witness_len(&valid, offsets[1] + 1);
    duplicate.splice(offsets[1] + 1..second_end, first);
    dispatch(&f, &duplicate, false, "duplicate-unhide")?;
    require(
        node(&f, "Relationship 0")?["lumen_state"]["hidden"] == true
            && node(&f, "Relationship 1")?["lumen_state"]["hidden"] == true,
        "Invalid final action partially unhid group",
    )?;
    dispatch(&f, &valid, true, "positive-group-unhide")?;
    require(
        node(&f, "Relationship 0")?["lumen_state"]["hidden"] == false
            && node(&f, "Relationship 1")?["lumen_state"]["hidden"] == false,
        "Group unhide not preserved",
    )?;
    yes(&f, &format!("[con_id={}] lumen_window minimize", a["id"]))?;
    focus(&f, &other["id"])?;
    let minimized = captured(&f, &a["id"], &[], true)?;
    dispatch(&f, &minimized, true, "positive-minimized")?;
    require(
        focused(&f.tree()?)?["id"] == a["id"],
        "Minimized target not restored",
    )?;
    yes(
        &f,
        &format!("[con_id={}] move container to workspace 2", a["id"]),
    )?;
    focus(&f, &other["id"])?;
    let restricted = captured(&f, &a["id"], &[], false)?;
    dispatch(&f, &restricted, true, "positive-space-restriction")?;
    require(
        focused(&f.tree()?)?["id"] == other["id"],
        "Restricted activation switched spaces",
    )?;
    let permitted = captured(&f, &a["id"], &[], true)?;
    dispatch(&f, &permitted, true, "positive-space-switch")?;
    require(
        focused(&f.tree()?)?["id"] == a["id"],
        "Permitted activation did not switch spaces",
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":["version2 digest guard advertised","affirmative empty workspace source positive","owned exclusive layer rejects false-none source","just-over-limit wire token rejected before mutation","workspace topology change rejects stale packet","actual old unguarded source race retained","new native focus rejects stale packet","new protocol modal rejects stale packet","captured close and same-process replacement reject","truncated/trailing/unknownversion/source/operation reject before mutation","duplicate final unhide rejected without partial group mutation","typed exact focus positive","shared protocol modal redirect positive","captured group unhide positive","minimized restoration positive","space restriction and permitted switching positive"],"scope":"Real compositor IPC and native same-process parent/dialog surfaces; no host activation or kernel process-lifetime atomicity claim"}),
    )
}
