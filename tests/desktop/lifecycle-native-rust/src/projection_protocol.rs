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
pub(super) fn family_bounds(f: &mut Fixture) -> Result<()> {
    let (client, id, log) = launch(f, false, "bounded-family")?;
    let pid = f.processes[client].0.id();
    request(f, &operation(&view(f, id)?, true)?, None)?;
    let origin = view(f, id)?["lumen_all_desktops"]["origin_workspace"].clone();
    let mut previous_id = id;
    for index in 1..16 {
        send(f, client, &log, "extend", &format!("known-chain-{index}"))?;
        previous_id = wait("real mapped chain parent identity", || {
            Ok(nodes(f)?
                .into_iter()
                .find(|n| {
                    n["pid"] == pid
                        && n["name"] == format!("Projection protocol {index}")
                        && n["lumen_relationships"]["parent_supported"] == true
                        && n["lumen_relationships"]["parent"] == previous_id
                })
                .and_then(|n| n["id"].as_u64()))
        })?;
    }
    let members = wait("sixteen real nonmodal family views", || {
        let rows: Vec<_> = nodes(f)?.into_iter().filter(|n| n["pid"] == pid).collect();
        Ok((rows.len() == 16).then_some(rows))
    })?;
    for member in &members {
        let member_id = member["id"].as_u64().ok_or("Bounded family id")?;
        request(f, &operation(&view(f, member_id)?, true)?, None)?;
    }
    request(f, "workspace 3", None)?;
    let before = f.tree()?;
    for member in &members {
        let current = view(f, member["id"].as_u64().ok_or("Member id")?)?;
        require(
            current["lumen_all_desktops"]["supported"] == true
                && current["lumen_all_desktops"]["projected"] == true,
            "Exactly16 known direct family members must remain eligible",
        )?;
    }
    let revision = view(f, id)?["lumen_all_desktops"]["revision"].clone();
    for member in members.iter().take(3) {
        request(f, &format!("[con_id={}] focus", member["id"]), None)?;
    }
    let raised = f.tree()?;
    save(
        &f.options.output.join("family-raise-order.json"),
        &json!({"before":before,"after":raised,"root":view(f,id)?,"revision":revision}),
    )?;
    require(
        view(f, id)?["lumen_all_desktops"]["revision"] == revision,
        "Ordinary raises must not revise unchanged family membership",
    )?;
    send(f, client, &log, "extend17", "known17-overflow")?;
    let overflow = wait("seventeenth real family member", || {
        Ok(nodes(f)?
            .into_iter()
            .find(|n| n["pid"] == pid && n["name"] == "Projection protocol 16"))
    })?;
    let after = f.tree()?;
    save(
        &f.options.output.join("family-overflow.json"),
        &json!({"before":raised,"after":after,"overflow":overflow,"root":view(f,id)?,"current":current_workspace(f)?,"focus":focused(f)?}),
    )?;
    require(
        view(f, id)?["lumen_all_desktops"]["reason"] == "owned-family-bounds"
            && overflow["lumen_all_desktops"]["reason"] == "owned-family-bounds"
            && overflow["lumen_all_desktops"]["enabled"] == false
            && overflow["lumen_all_desktops"]["origin_workspace"] == origin
            && current_workspace(f)?["name"] == origin["name"]
            && view(f, focused(f)?)?["visible"] == true,
        "Over-bound family must refuse projection while retaining usable same origin",
    )?;
    let overflow_id = overflow["id"].as_u64().ok_or("Overflow id")?;
    let refusal_before = f.tree()?;
    request(
        f,
        &operation(&view(f, overflow_id)?, true)?,
        Some("Projection unavailable"),
    )?;
    save(
        &f.options.output.join("family-overflow-refusal.json"),
        &json!({"before":refusal_before,"after":f.tree()?}),
    )?;
    require(
        refusal_before == f.tree()?,
        "Over-bound direct intent changed native family",
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":4}),
    )
}
pub(super) fn family(f: &mut Fixture, primary: u64) -> Result<()> {
    let (client, id, log) = launch(f, false, "family-protocol")?;
    request(f, &operation(&view(f, id)?, true)?, None)?;
    request(f, "workspace 3", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    let before = f.tree()?;
    let owner_before = view(f, id)?;
    require(
        owner_before["lumen_all_desktops"]["projected"] == true,
        "Family fixture requires foreground off-origin projection",
    )?;
    send(f, client, &log, "modal", "foreground-family-modal")?;
    let child = wait("real owned modal mapped", || {
        Ok(nodes(f)?
            .into_iter()
            .find(|n| n["name"] == "Projection protocol 1"))
    })?;
    wait_true("modal owner suspension published", || {
        Ok(view(f, id)?["lumen_all_desktops"]["supported"] == false)
    })?;
    let owner = view(f, id)?;
    let current = current_workspace(f)?;
    save(
        &f.options.output.join("foreground-modal-family.json"),
        &json!({"before":before,"after":f.tree()?,"ownerBefore":owner_before,
            "owner":owner,"child":child,"currentWorkspace":current,"focus":focused(f)?,"client":rows(&log)?}),
    )?;
    require(
        owner["lumen_all_desktops"]["origin_workspace"]
            == child["lumen_all_desktops"]["origin_workspace"],
        "Foreground modal split from retained owner origin",
    )?;
    require(
        current["name"] == owner["lumen_all_desktops"]["origin_workspace"]["name"],
        "Foreground modal owner is not usable on current origin Space",
    )?;
    let child_id = child["id"].as_u64().ok_or("Modal child identity")?;
    let modal_before = f.tree()?;
    request(
        f,
        &operation(&view(f, child_id)?, true)?,
        Some("Projection unavailable"),
    )?;
    save(
        &f.options.output.join("modal-direct-refusal.json"),
        &json!({"before":modal_before,"after":f.tree()?}),
    )?;
    require(
        modal_before == f.tree()?,
        "Modal direct flag refusal changed family",
    )?;
    send(f, client, &log, "unmodal", "family-modal-close")?;
    wait_true("modal child retired", || {
        Ok(!nodes(f)?.iter().any(|n| n["id"] == child_id))
    })?;
    request(f, "workspace 3", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    send(f, client, &log, "owned", "foreground-nonmodal-child")?;
    let owned = wait("real nonmodal pending child", || {
        Ok(nodes(f)?.into_iter().find(|n| {
            n["name"] == "Projection protocol 1"
                && n["lumen_all_desktops"]["reason"] == "owned-family-pending"
        }))
    })?;
    let owned_id = owned["id"].as_u64().ok_or("Owned child id")?;
    let owner_pending = view(f, id)?;
    save(
        &f.options.output.join("nonmodal-direct-pending.json"),
        &json!({"owner":owner_pending,"child":owned,"tree":f.tree()?}),
    )?;
    require(
        owner_pending["lumen_all_desktops"]["reason"] == "owned-family-pending"
            && owned["lumen_all_desktops"]["enabled"] == false
            && owner_pending["lumen_all_desktops"]["origin_workspace"]
                == owned["lumen_all_desktops"]["origin_workspace"],
        "Nonmodal family must retain origin without inheriting child intent",
    )?;
    request(f, &operation(&view(f, owned_id)?, true)?, None)?;
    request(f, "workspace 3", None)?;
    for member in [id, owned_id] {
        let node = view(f, member)?;
        save(
            &f.options
                .output
                .join(format!("family-enabled-{member}.json")),
            &node,
        )?;
        require(
            node["lumen_all_desktops"]["projected"] == true && node["visible"] == true,
            "Explicitly enabled nonmodal member did not project with its family",
        )?;
    }
    request(f, &format!("[con_id={owned_id}] focus"), None)?;
    require(
        current_workspace(f)?["name"] == "3" && focused(f)? == owned_id,
        "Nonmodal projected child activation jumped origin",
    )?;
    let before_clear = view(f, id)?;
    request(f, &operation(&view(f, owned_id)?, false)?, None)?;
    let after_clear = view(f, id)?;
    save(
        &f.options.output.join("family-direct-clear.json"),
        &json!({"beforeOwner":before_clear,"afterOwner":after_clear,"child":view(f, owned_id)?,"tree":f.tree()?}),
    )?;
    require(
        after_clear["lumen_all_desktops"]["enabled"] == true
            && after_clear["lumen_all_desktops"]["reason"] == "owned-family-pending"
            && after_clear["lumen_all_desktops"]["revision"]
                != before_clear["lumen_all_desktops"]["revision"],
        "Child clear must retain owner direct intent and invalidate family witness",
    )?;
    let focused_after_clear = view(f, focused(f)?)?;
    save(
        &f.options.output.join("family-clear-focus-continuity.json"),
        &json!({"focused":focused_after_clear,"current":current_workspace(f)?,"tree":f.tree()?}),
    )?;
    require(
        focused_after_clear["visible"] == true,
        "Explicit family clear left focused view invisible on inactive origin",
    )?;
    send(f, client, &log, "unmodal", "nonmodal-close")?;
    wait_true("nonmodal child retired", || {
        Ok(!nodes(f)?.iter().any(|n| n["id"] == owned_id))
    })?;
    request(f, "workspace 3", None)?;
    request(f, &format!("[con_id={primary}] focus"), None)?;
    let background_focus = focused(f)?;
    send(f, client, &log, "modal", "background-modal-map")?;
    let background = wait("background modal child origin", || {
        Ok(nodes(f)?
            .into_iter()
            .find(|n| n["name"] == "Projection protocol 1"))
    })?;
    save(
        &f.options.output.join("background-modal-family.json"),
        &json!({"owner":view(f,id)?,"child":background,"current":current_workspace(f)?,"focus":focused(f)?,"expectedFocus":background_focus,"tree":f.tree()?}),
    )?;
    require(
        view(f, id)?["lumen_all_desktops"]["origin_workspace"]
            == background["lumen_all_desktops"]["origin_workspace"]
            && current_workspace(f)?["name"] == "3"
            && focused(f)? == background_focus,
        "Background family map split origin or stole active Space/focus",
    )?;
    let background_id = background["id"].as_u64().ok_or("Background modal id")?;
    let qml = f.qml.clone();
    f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().ok_or("QML path")?],
        "family-desktop.log",
    )?;
    wait_true("actual shell observes owned family QObjects", || {
        let state = audit(f)?;
        save(
            &f.options.output.join("family-shell-readiness.json"),
            &state,
        )?;
        Ok(state["available"] == true
            && state["windows"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|r| r["foreignId"] == background["foreign_toplevel_identifier"])
            }))
    })?;
    let choice: Value = serde_json::from_str(&f.ipc(
        "projectionAudit",
        "switchTo",
        &["org.example.ProjectionProtocol"],
    )?)?;
    save(
        &f.options.output.join("modal-global-switcher-choice.json"),
        &choice,
    )?;
    require(
        choice["missing"] != true,
        "Global switcher omitted suspended owned family",
    )?;
    wait_true(
        "global switcher activates usable retained modal family",
        || {
            let state = audit(f)?;
            save(
                &f.options.output.join("modal-global-switcher-result.json"),
                &json!({"tree":f.tree()?,"audit":state,"focus":focused(f)?,"current":current_workspace(f)?}),
            )?;
            Ok(current_workspace(f)?["name"]
                == owner["lumen_all_desktops"]["origin_workspace"]["name"]
                && focused(f)? == background_id
                && view(f, background_id)?["visible"] == true)
        },
    )?;
    send(f, client, &log, "unmodal", "global-modal-close")?;
    wait_true("global modal child retired", || {
        Ok(!nodes(f)?.iter().any(|n| n["id"] == background_id))
    })?;
    request(f, "workspace 3", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    send(f, client, &log, "owned-utility", "explicit-other-app-child")?;
    let utility = wait("real other-application owned child", || {
        Ok(nodes(f)?
            .into_iter()
            .find(|n| n["app_id"] == "org.example.ProjectionUtility"))
    })?;
    let utility_id = utility["id"].as_u64().ok_or("Utility identity")?;
    f.call(
        "lumen-spaces",
        &["assign", "wayland:org.example.ProjectionProtocol", "all"],
    )?;
    let unassigned = view(f, utility_id)?;
    save(
        &f.options.output.join("other-app-no-inheritance.json"),
        &json!({"owner":view(f,id)?,"utility":unassigned,"tree":f.tree()?}),
    )?;
    require(
        unassigned["lumen_all_desktops"]["enabled"] == false
            && unassigned["lumen_all_desktops"]["reason"] == "owned-family-pending",
        "Parent application assignment must not inherit authority over utility child",
    )?;
    f.call(
        "lumen-spaces",
        &["assign", "wayland:org.example.ProjectionUtility", "all"],
    )?;
    wait_true(
        "explicit utility application assignment converges direct flag",
        || {
            Ok(
                view(f, utility_id)?["lumen_all_desktops"]["enabled"] == true
                    && view(f, id)?["lumen_all_desktops"]["supported"] == true,
            )
        },
    )?;
    request(f, "workspace 3", None)?;
    request(f, &format!("[con_id={utility_id}] focus"), None)?;
    let before_unparent = f.tree()?;
    send(f, client, &log, "unparent", "direct-utility-unparent")?;
    let after_unparent = f.tree()?;
    save(
        &f.options.output.join("explicit-utility-unparent.json"),
        &json!({"before":before_unparent,"after":after_unparent,"owner":view(f,id)?,"utility":view(f,utility_id)?}),
    )?;
    require(
        view(f, utility_id)?["lumen_all_desktops"]["enabled"] == true
            && view(f, utility_id)?["lumen_all_desktops"]["supported"] == true,
        "Unparent discarded explicit utility direct intent",
    )?;
    f.call(
        "lumen-spaces",
        &["assign", "wayland:org.example.ProjectionProtocol", "none"],
    )?;
    request(f, &operation(&view(f, id)?, false)?, None)?;
    save(
        &f.options.output.join("independent-parent-clear.json"),
        &json!({"owner":view(f,id)?,"utility":view(f,utility_id)?,"current":current_workspace(f)?,"focus":focused(f)?,"tree":f.tree()?}),
    )?;
    require(
        view(f, utility_id)?["lumen_all_desktops"]["enabled"] == true
            && view(f, utility_id)?["visible"] == true
            && focused(f)? == utility_id
            && current_workspace(f)?["name"] == "3",
        "Clearing former owner changed independent utility intent or stole focus",
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":12}),
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
