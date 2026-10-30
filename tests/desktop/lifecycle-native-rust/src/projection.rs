//! Native projection keeps the original scene/container and structural slot.
use crate::harness::{Fixture, Options, Result, require, save, wait, wait_true};
use serde_json::{Value, json};
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
#[path = "projection_lock.rs"]
mod lock;
#[path = "projection_protocol.rs"]
mod protocol;
const APP: &str = "org.example.MruA";
static ACTION: AtomicUsize = AtomicUsize::new(0);
fn nodes(f: &Fixture) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    Fixture::nodes(&f.tree()?, &mut all);
    Ok(all)
}
fn view(f: &Fixture, id: u64) -> Result<Value> {
    nodes(f)?
        .into_iter()
        .find(|n| n["id"] == id)
        .ok_or("Owned view missing".into())
}
fn request(f: &Fixture, operation: &str, expected_error: Option<&str>) -> Result<Value> {
    let before = f.tree()?;
    let result = Command::new("swaymsg")
        .args(["-r", operation])
        .env_clear()
        .envs(&f.vars)
        .output()?;
    save(
        &f.options.output.join(format!(
            "action-{:03}.json",
            ACTION.fetch_add(1, Ordering::Relaxed)
        )),
        &json!({"operation":operation,"before":before,"stdout":String::from_utf8_lossy(&result.stdout),"stderr":String::from_utf8_lossy(&result.stderr),"exit":result.status.code(),"after":f.tree()?}),
    )?;
    let value: Value = serde_json::from_slice(&result.stdout)?;
    let rows = value
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or("One native receipt required")?;
    if let Some(error) = expected_error {
        require(
            result.status.code() == Some(2)
                && rows[0]["success"] == false
                && rows[0]["parse_error"] == true
                && rows[0]["error"] == error,
            format!("Wrong native refusal: {:?} {value}", result.status),
        )?;
    } else {
        require(
            result.status.success() && rows[0]["success"] == true,
            format!("Native projection failed: {:?} {value}", result.status),
        )?;
    }
    Ok(value)
}
fn operation(node: &Value, enabled: bool) -> Result<String> {
    let p = &node["lumen_all_desktops"];
    Ok(format!(
        "lumen_projection {} {} {} {} {} {} {}",
        node["id"].as_u64().ok_or("ID")?,
        node["foreign_toplevel_identifier"]
            .as_str()
            .ok_or("Foreign")?,
        node["pid"].as_u64().ok_or("PID")?,
        p["origin_workspace"]["id"].as_str().ok_or("Origin")?,
        p["origin_output"]["id"].as_str().ok_or("Output")?,
        p["revision"].as_str().ok_or("Revision")?,
        if enabled { "on" } else { "off" }
    ))
}
fn structural(node: &Value) -> Value {
    json!({"id":node["id"],"type":node["type"],"layout":node["layout"],"rect":node["rect"],"percent":node["percent"],"floating":node["floating"],
        "nodes":node["nodes"].as_array().map(|a|a.iter().map(structural).collect::<Vec<_>>()),"floating_nodes":node["floating_nodes"].as_array().map(|a|a.iter().map(structural).collect::<Vec<_>>())})
}
pub fn run(options: Options) -> Result<()> {
    let full = options.scenario == "projection";
    let interaction = options.scenario == "projection-interaction";
    let mut f = Fixture::new(options)?;
    let invalid = request(
        &f,
        "lumen_projection malformed",
        Some("Invalid projection witness"),
    )?;
    let folder = f.options.output.join("owned-client");
    fs::create_dir(&folder)?;
    let client = f.sibling("lifecycle-client")?;
    f.launch(
        &client,
        &[
            folder.to_str().ok_or("Client path")?,
            APP,
            "--projection-input",
        ],
        "client.log",
    )?;
    let owned = wait("native projection views", || {
        let views = nodes(&f)?
            .into_iter()
            .filter(|n| n["app_id"] == APP)
            .collect::<Vec<_>>();
        Ok((views.len() == 2).then_some(views))
    })?;
    let id = owned[0]["id"].as_u64().ok_or("Owned ID")?;
    let mut previous = None;
    let origin = wait(
        "owned GTK allocations and stable initial structural layout",
        || {
            let all = nodes(&f)?;
            let origin = all
                .iter()
                .find(|n| n["type"] == "workspace" && n["name"] == "1")
                .ok_or("Origin workspace")?;
            let ready = all.iter().filter(|n| n["app_id"] == APP).all(|node| {
                let Some(index) = node["name"]
                    .as_str()
                    .and_then(|name| name.strip_prefix("Lifecycle document "))
                    .and_then(|s| s.parse::<usize>().ok())
                    .and_then(|i| i.checked_sub(1))
                else {
                    return false;
                };
                fs::read(folder.join(format!("allocation-{index}.json")))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .is_some_and(|allocation| {
                        allocation["width"] == node["window_rect"]["width"]
                            && allocation["height"] == node["window_rect"]["height"]
                            && allocation["width"].as_i64().is_some_and(|v| v > 0)
                    })
            });
            let facts = structural(origin);
            let settled = previous.as_ref() == Some(&facts);
            previous = Some(facts);
            save(
                &f.options.output.join("initial-layout-readiness.json"),
                &json!({"allocationReady":ready,"settled":settled,"origin":origin}),
            )?;
            Ok((ready && settled).then_some(origin.clone()))
        },
    )?;
    let before = structural(&origin);
    let target = view(&f, id)?;
    require(
        target["lumen_all_desktops"]["supported"] == true,
        format!("Projection capability {target}"),
    )?;
    let enable = request(&f, &operation(&target, true)?, None)?;
    wait_true("projection enabled", || {
        Ok(view(&f, id)?["lumen_all_desktops"]["enabled"] == true)
    })?;
    f.call("swaymsg", &["workspace", "2"])?;
    let projected = wait("native projected view in current Space", || {
        let node = view(&f, id)?;
        Ok(
            (node["lumen_all_desktops"]["effective_workspace"]["name"] == "2"
                && node["lumen_all_desktops"]["projected"] == true
                && node["visible"] == true)
                .then_some(node),
        )
    })?;
    f.call("swaymsg", &[&format!("[con_id={id}]"), "focus"])?;
    let workspaces: Value =
        serde_json::from_str(&f.call("swaymsg", &["-t", "get_workspaces", "-r"])?)?;
    require(
        workspaces
            .as_array()
            .ok_or("Workspaces")?
            .iter()
            .any(|w| w["name"] == "2" && w["focused"] == true),
        "Projected native focus jumped to origin",
    )?;
    let after_origin = nodes(&f)?
        .into_iter()
        .find(|n| n["id"] == origin["id"])
        .ok_or("Retained origin")?;
    save(
        &f.options.output.join("structural-comparison.json"),
        &json!({"before":before,"after":structural(&after_origin),"originBefore":origin,"originAfter":after_origin}),
    )?;
    require(
        structural(&after_origin) == before,
        "Projection changed structural tile geometry/slot",
    )?;
    let stale = request(
        &f,
        &operation(&target, false)?,
        Some("Projection witness unavailable"),
    )?;
    if f.options.scenario == "projection-lock" {
        return lock::run(&mut f, id, &folder);
    }
    if f.options.scenario == "projection-protocol" {
        return protocol::run(&mut f, id);
    }
    if f.options.scenario == "projection-focus" {
        return focus_journey(&mut f, id, &owned);
    }
    if full {
        journey(&mut f, id, &owned, &before, &origin)?;
    }
    if interaction {
        interaction_journey(&mut f, id, &owned, &folder)?;
    }
    let clear = request(&f, &operation(&view(&f, id)?, false)?, None)?;
    wait_true("projection clear hides inactive origin", || {
        Ok(view(&f, id)?["visible"] == false)
    })?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":5,"invalidHandler":invalid,"enable":enable,"projected":projected,"workspaces":workspaces,"structuralBefore":before,"structuralAfter":structural(&after_origin),"stale":stale,"clear":clear,"scope":"Native structural tiled projection probe; broader physical input/group/restore/output acceptance pending"}),
    )?;
    Ok(())
}

fn current_workspace(f: &Fixture) -> Result<Value> {
    let value: Value = serde_json::from_str(&f.call("swaymsg", &["-t", "get_workspaces", "-r"])?)?;
    value
        .as_array()
        .ok_or("Workspaces")?
        .iter()
        .find(|w| w["focused"] == true)
        .cloned()
        .ok_or("Focused workspace missing".into())
}
fn focused(f: &Fixture) -> Result<u64> {
    nodes(f)?
        .iter()
        .find(|n| n["focused"] == true && n["shell"] == "xdg_shell")
        .and_then(|n| n["id"].as_u64())
        .ok_or("Focused view missing".into())
}
fn group(f: &Fixture, action: &str) -> Result<()> {
    let revision = f.tree()?["window_groups"]["revision"]
        .as_u64()
        .ok_or("Groups revision")?;
    request(f, &format!("lumen_window groups {revision} {action}"), None)?;
    Ok(())
}
fn guard_rejected(f: &Fixture, packet: &[u8], label: &str) -> Result<()> {
    let before = f.tree()?;
    let command = lumen_window_policy::guard::command(packet)?;
    let output = Command::new("swaymsg")
        .args(["-r", &command])
        .env_clear()
        .envs(&f.vars)
        .output()?;
    let after = f.tree()?;
    let reply: Value = serde_json::from_slice(&output.stdout)?;
    save(
        &f.options.output.join(format!("{label}.json")),
        &json!({"before":before,"after":after,"reply":reply,"exit":output.status.code(),"stderr":String::from_utf8_lossy(&output.stderr)}),
    )?;
    require(
        output.status.code() == Some(2)
            && reply.as_array().is_some_and(|rows| rows.len() == 1)
            && reply[0]["success"] == false
            && reply[0]["error"] == "LFG_REJECTED: validation failed before any action",
        "Wrong native guard rejection",
    )?;
    require(
        before == after,
        "Rejected projection guard changed native state",
    )
}
fn journey(
    f: &mut Fixture,
    id: u64,
    owned: &[Value],
    initial: &Value,
    origin: &Value,
) -> Result<()> {
    let other = owned
        .iter()
        .find(|node| node["id"] != id)
        .and_then(|node| node["id"].as_u64())
        .ok_or("Second owned view")?;
    request(f, &operation(&view(f, other)?, true)?, None)?;
    group(f, "on")?;
    let groups = f.tree()?["window_groups"]["groups"]
        .as_array()
        .ok_or("Group rows")?
        .clone();
    let group_id = groups
        .iter()
        .find(|g| {
            g["members"]
                .as_array()
                .is_some_and(|members| members.iter().any(|m| m["id"] == id))
        })
        .and_then(|g| g["id"].as_u64())
        .ok_or("Owned target group")?;
    group(
        f,
        &format!(
            "add {group_id} {other} {}",
            view(f, other)?["pid"].as_u64().ok_or("Owned PID")?
        ),
    )?;
    require(
        f.tree()?["window_groups"]["groups"]
            .as_array()
            .is_some_and(|groups| {
                groups.iter().any(|g| {
                    g["id"] == group_id
                        && g["members"].as_array().is_some_and(|members| {
                            members.len() == 2
                                && members.iter().any(|m| m["id"] == id)
                                && members.iter().any(|m| m["id"] == other)
                        })
                })
            }),
        "Explicit group merge did not retain exact members",
    )?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    group(f, "mode one_at_a_time")?;
    wait_true(
        "group one-at-a-time remains an independent projection mask",
        || Ok(view(f, id)?["visible"] == true && view(f, other)?["visible"] == false),
    )?;
    let first_mask = f.tree()?;
    request(f, &format!("[con_id={other}] focus"), None)?;
    wait_true(
        "native focus selects projected group backing on current Space",
        || {
            Ok(view(f, id)?["visible"] == false
                && view(f, other)?["visible"] == true
                && current_workspace(f)?["name"] == "2")
        },
    )?;
    save(
        &f.options.output.join("group-mask.json"),
        &json!({"before":first_mask,"after":f.tree()?}),
    )?;
    group(f, "mode all_together")?;
    wait_true("together group presents both projected members", || {
        Ok(view(f, id)?["visible"] == true && view(f, other)?["visible"] == true)
    })?;
    group(f, "off")?;
    let stable = view(f, id)?;
    for (field, replacement) in [
        (2, "ffffffffffffffffffffffffffffffff"),
        (3, "1"),
        (4, "18446744073709551615"),
        (5, "18446744073709551615"),
    ] {
        let operation = operation(&stable, false)?;
        let mut words = operation
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        words[field] = replacement.into();
        let before = f.tree()?;
        request(f, &words.join(" "), Some("Projection witness unavailable"))?;
        let after = f.tree()?;
        save(
            &f.options.output.join(format!("wrong-witness-{field}.json")),
            &json!({"before":before,"after":after}),
        )?;
        require(
            before == after,
            "Invalid typed projection witness mutated native state",
        )?;
    }
    let tree = f.tree()?;
    let source = view(f, other)?;
    let target = view(f, id)?;
    let packet = lumen_window_policy::guard::packet(
        &tree,
        &json!({"kind":"native","conId":other,"foreignId":source["foreign_toplevel_identifier"]}),
        &target,
        &[json!({"op":"focus","conId":id})],
    )?;
    let revision = target["lumen_all_desktops"]["revision"]
        .as_str()
        .ok_or("Revision")?
        .parse::<u64>()?;
    request(f, &operation(&target, false)?, None)?;
    request(f, &operation(&view(f, id)?, true)?, None)?;
    require(
        view(f, id)?["lumen_all_desktops"]["revision"]
            .as_str()
            .ok_or("Revision")?
            .parse::<u64>()?
            > revision,
        "Projection on/off ABA did not advance revision",
    )?;
    guard_rejected(f, &packet, "projection-aba-guard")?;
    let tree = f.tree()?;
    let source = view(f, other)?;
    let fresh = lumen_window_policy::guard::packet(
        &tree,
        &json!({"kind":"native","conId":other,"foreignId":source["foreign_toplevel_identifier"]}),
        &view(f, id)?,
        &[json!({"op":"focus","conId":id})],
    )?;
    request(f, &lumen_window_policy::guard::command(&fresh)?, None)?;
    require(
        focused(f)? == id && current_workspace(f)?["name"] == "2",
        "Fresh exact projection guard did not focus on effective Space",
    )?;
    let before_fullscreen = f.tree()?;
    request(f, &format!("[con_id={id}] lumen_window fullscreen"), None)?;
    wait_true(
        "projected fullscreen deliberately activates retained origin and stays usable",
        || {
            Ok(view(f, id)?["fullscreen_mode"] == 1
                && view(f, id)?["visible"] == true
                && focused(f)? == id
                && current_workspace(f)?["name"] == "1")
        },
    )?;
    let fullscreen = f.tree()?;
    request(f, &format!("[con_id={id}] lumen_window fullscreen"), None)?;
    wait_true("fullscreen exit retains native projected identity", || {
        Ok(view(f, id)?["fullscreen_mode"] == 0
            && view(f, id)?["lumen_all_desktops"]["enabled"] == true
            && view(f, id)?["visible"] == true)
    })?;
    let restored = nodes(f)?
        .into_iter()
        .find(|node| node["id"] == origin["id"])
        .ok_or("Origin after fullscreen")?;
    save(
        &f.options.output.join("fullscreen-transition.json"),
        &json!({"before":before_fullscreen,"fullscreen":fullscreen,"after":f.tree()?,"initialStructure":initial,"restoredStructure":structural(&restored)}),
    )?;
    require(
        structural(&restored) == *initial,
        "Fullscreen transition corrupted retained origin slot/ratios",
    )?;
    request(f, "workspace 2", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    wait_true("projection resumes after fullscreen exit", || {
        Ok(current_workspace(f)?["name"] == "2" && view(f, id)?["visible"] == true)
    })?;
    save(
        &f.options.output.join("journey-result.json"),
        &json!({"passed":true,"checks":8,"scope":"Group masks, exact typed witnesses, observed on/off ABA guard, usable foreground fullscreen origin transition and structural restoration"}),
    )?;
    Ok(())
}

fn audit(f: &Fixture) -> Result<Value> {
    Ok(serde_json::from_str(&f.ipc(
        "projectionAudit",
        "status",
        &[],
    )?)?)
}
fn clicks(folder: &std::path::Path) -> Result<usize> {
    let file = folder.join("clicks.jsonl");
    if !file.exists() {
        return Ok(0);
    }
    let bytes = fs::read(file)?;
    require(bytes.len() < 65536, "Pointer journal bound")?;
    let text = std::str::from_utf8(&bytes)?;
    let rows = text
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    require(
        rows.iter().all(|row| row["index"] == 0),
        "Unexpected owned GTK pointer recipient",
    )?;
    // Synthetic double/triple-click notifications are not additional presses.
    // The owned EventBox stops propagation; retain every raw callback.
    let mut presses = 0;
    for row in rows {
        require(
            row["time"].as_u64().is_some() && row["button"] == 1 && row["type"].is_string(),
            "Invalid owned pointer event facts",
        )?;
        if row["type"] == "ButtonPress" {
            presses += 1;
        }
    }
    Ok(presses)
}
fn click(
    f: &mut Fixture,
    pointer: usize,
    position: &mut (i64, i64),
    point: (i64, i64),
    folder: &std::path::Path,
    expected: usize,
    id: u64,
    label: &str,
) -> Result<()> {
    use std::io::Write;
    let pipe = f.processes[pointer]
        .0
        .stdin
        .as_mut()
        .ok_or("Pointer pipe")?;
    let command = format!(
        "m {} {}\np\nr\n",
        point.0 - position.0,
        point.1 - position.1
    );
    pipe.write_all(command.as_bytes())?;
    pipe.flush()?;
    *position = point;
    wait_true(label, || {
        let tree = f.tree()?;
        let count = clicks(folder)?;
        save(
            &f.options.output.join(format!("pointer-{label}.json")),
            &json!({"command":command,"point":[point.0,point.1],"count":count,"expected":expected,"tree":tree,
                "seats":serde_json::from_str::<Value>(&f.call("swaymsg", &["-t","get_seats","-r"])?)?,
                "pointerExited":f.processes[pointer].0.try_wait()?.map(|status|status.to_string())}),
        )?;
        Ok(count == expected && focused(f)? == id && current_workspace(f)?["name"] == "2")
    })
}
fn switch_restore(f: &Fixture, id: u64, label: &str) -> Result<Value> {
    let before = audit(f)?;
    let choice: Value = serde_json::from_str(&f.ipc("projectionAudit", "switchTo", &[APP])?)?;
    save(
        &f.options.output.join(format!("{label}-begin.json")),
        &json!({"before":before,"choice":choice,"tree":f.tree()?}),
    )?;
    require(
        choice["before"]["nativeCapture"]["conId"] == id,
        "Global switcher did not freeze exact native projected window",
    )?;
    wait(label, || {
        let state = audit(f)?;
        let tree = f.tree()?;
        save(
            &f.options.output.join(format!("{label}-latest.json")),
            &json!({"audit":state,"tree":tree}),
        )?;
        Ok((state["switcher"]["phase"] == "committed"
            && focused(f).ok() == Some(id)
            && view(f, id)?["visible"] == true)
            .then_some(tree))
    })
}
fn interaction_journey(
    f: &mut Fixture,
    id: u64,
    owned: &[Value],
    folder: &std::path::Path,
) -> Result<()> {
    use std::path::Path;
    require(
        view(f, id)?["name"] == "Lifecycle document 1",
        "Fixture first native surface must match instrumented client index",
    )?;
    let other = owned
        .iter()
        .find(|node| node["id"] != id)
        .and_then(|n| n["id"].as_u64())
        .ok_or("Second owned client")?;
    request(f, &format!("[con_id={other}] kill"), None)?;
    wait_true("closed sibling no longer a native view", || {
        Ok(!nodes(f)?.iter().any(|n| n["id"] == other))
    })?;
    let after_close = f.tree()?;
    save(
        &f.options.output.join("inactive-sibling-close.json"),
        &json!({"after":after_close,"workspace":current_workspace(f)?,"expectedFocused":id}),
    )?;
    require(
        current_workspace(f)?["name"] == "2" && focused(f)? == id,
        "Closing an inactive structural sibling changed projected focus/Space",
    )?;
    let other_folder = f.options.output.join("other-client");
    fs::create_dir(&other_folder)?;
    let program = f.sibling("lifecycle-client")?;
    let process = f.launch(
        &program,
        &[
            other_folder.to_str().ok_or("Other client folder")?,
            "org.example.MruC",
            "--projection-input",
        ],
        "other-client.log",
    )?;
    let pid = f.processes[process].0.id();
    let views = wait("owned other application surfaces", || {
        let views = nodes(f)?
            .into_iter()
            .filter(|n| n["pid"] == pid && n["app_id"] == "org.example.MruC")
            .collect::<Vec<_>>();
        Ok((views.len() == 2).then_some(views))
    })?;
    let ordinary = views
        .iter()
        .find(|n| n["name"] == "Lifecycle document 1")
        .and_then(|n| n["id"].as_u64())
        .ok_or("Other document identity")?;
    let second = views
        .iter()
        .find(|n| n["id"] != ordinary)
        .and_then(|n| n["id"].as_u64())
        .ok_or("Other sibling")?;
    request(f, &format!("[con_id={second}] kill"), None)?;
    for action in [
        "floating enable",
        "resize set 400 300",
        "move position 100 150",
    ] {
        request(f, &format!("[con_id={ordinary}] {action}"), None)?;
    }
    wait_true(
        "owned ordinary floating client allocation agrees with native target",
        || {
            let node = view(f, ordinary)?;
            let allocation: Value =
                serde_json::from_slice(&fs::read(other_folder.join("allocation-0.json"))?)?;
            Ok(allocation["width"] == node["window_rect"]["width"]
                && allocation["height"] == node["window_rect"]["height"]
                && node["rect"]["x"] == 100
                && node["rect"]["y"]
                    .as_i64()
                    .zip(node["deco_rect"]["height"].as_i64())
                    .is_some_and(|(y, title)| y - title == 150))
        },
    )?;
    let pointer = std::env::split_paths(std::ffi::OsStr::new(f.vars.get("PATH").ok_or("PATH")?))
        .map(|p| p.join("wlrctl"))
        .find(|p| p.is_file())
        .ok_or("Pinned pointer driver missing")?
        .canonicalize()?;
    require(
        pointer.starts_with("/nix/store"),
        "Immutable pointer driver required",
    )?;
    let driver = f.launch_input_args(&pointer, &["pointer", "move", "0", "0"], "pointer.log")?;
    {
        use std::io::Write;
        let pipe = f.processes[driver]
            .0
            .stdin
            .as_mut()
            .ok_or("Pointer handshake pipe")?;
        writeln!(pipe, "m 0 0")?;
        pipe.flush()?;
    }
    wait_true(
        "actual virtual pointer attached before establishing absolute origin",
        || {
            let inputs: Value =
                serde_json::from_str(&f.call("swaymsg", &["-t", "get_seats", "-r"])?)?;
            save(
                &f.options.output.join("pointer-input-readiness.json"),
                &inputs,
            )?;
            Ok(inputs.as_array().is_some_and(|rows| {
                rows.iter().any(|seat| {
                    seat["devices"].as_array().is_some_and(|devices| {
                        devices
                            .iter()
                            .any(|row| row["identifier"] == "0:0:wlr_virtual_pointer_v1")
                    })
                })
            }))
        },
    )?;
    request(f, "seat seat0 cursor set 0 0", None)?;
    let mut position = (0, 0);
    click(
        f,
        driver,
        &mut position,
        (180, 220),
        &other_folder,
        1,
        ordinary,
        "ordinary-overlap",
    )?;
    click(
        f,
        driver,
        &mut position,
        (40, 80),
        folder,
        1,
        id,
        "projected-exposed",
    )?;
    click(
        f,
        driver,
        &mut position,
        (180, 220),
        folder,
        2,
        id,
        "projected-raised-overlap",
    )?;
    require(
        clicks(&other_folder)? == 1,
        "Projection raise leaked overlap click to ordinary window",
    )?;
    request(f, &format!("[con_id={ordinary}] focus"), None)?;
    click(
        f,
        driver,
        &mut position,
        (180, 220),
        &other_folder,
        2,
        ordinary,
        "ordinary-raised-overlap",
    )?;
    f.call(
        "grim",
        &[f.options
            .output
            .join("physical-stacking.png")
            .to_str()
            .ok_or("Screenshot path")?],
    )?;
    // Run the actual production QObject/broker/switcher path, not a synthetic
    // activation helper. Only disposable desktop entries and providers exist.
    let qml = f.qml.clone();
    f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().ok_or("QML path")?],
        "desktop.log",
    )?;
    wait_true(
        "actual shell exact QObject and broker projection identities",
        || {
            let state = audit(f)?;
            save(&f.options.output.join("shell-readiness.json"), &state)?;
            Ok(state["available"] == true
                && state["observation"]["captures"]
                    .as_array()
                    .is_some_and(|rows| {
                        rows.iter().any(|r| r["conId"] == id)
                            && rows.iter().any(|r| r["conId"] == ordinary)
                    })
                && state["windows"].as_array().is_some_and(|rows| {
                    rows.iter().any(|r| {
                        r["foreignId"]
                            == view(f, id)
                                .ok()
                                .map(|n| n["foreign_toplevel_identifier"].clone())
                                .unwrap_or(Value::Null)
                    })
                }))
        },
    )?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    let before_desktop = f.tree()?;
    let origin_before_desktop = nodes(f)?
        .into_iter()
        .find(|n| n["type"] == "workspace" && n["name"] == "1")
        .ok_or("Desktop origin")?;
    request(f, "lumen_window show-desktop-on", None)?;
    let desktop_hidden = f.tree()?;
    save(
        &f.options.output.join("projected-show-desktop-hidden.json"),
        &json!({"before":before_desktop,"after":desktop_hidden,"workspace":current_workspace(f)?}),
    )?;
    require(
        view(f, id)?["lumen_state"]["desktop_hidden"] == true
            && view(f, id)?["visible"] == false
            && view(f, ordinary)?["visible"] == false
            && current_workspace(f)?["name"] == "2",
        "Show Desktop left projection visible or changed Space",
    )?;
    request(f, "lumen_window show-desktop-off", None)?;
    wait_true(
        "Show Desktop restores projected origin placement and focus",
        || {
            let origin_after = nodes(f)?
                .into_iter()
                .find(|n| n["type"] == "workspace" && n["name"] == "1")
                .ok_or("Restored desktop origin")?;
            save(
                &f.options
                    .output
                    .join("projected-show-desktop-restored.json"),
                &json!({"before":origin_before_desktop,"after":origin_after,"tree":f.tree()?,"workspace":current_workspace(f)?}),
            )?;
            Ok(
                structural(&origin_before_desktop) == structural(&origin_after)
                    && view(f, id)?["visible"] == true
                    && focused(f)? == id
                    && current_workspace(f)?["name"] == "2",
            )
        },
    )?;
    let before_minimize = f.tree()?;
    request(f, &format!("[con_id={id}] lumen_window minimize"), None)?;
    let minimized = f.tree()?;
    save(
        &f.options.output.join("projected-minimize.json"),
        &json!({"before":before_minimize,"after":minimized,"workspace":current_workspace(f)?}),
    )?;
    require(
        current_workspace(f)?["name"] == "2",
        "Minimizing a projected view jumped to its structural origin",
    )?;
    request(f, &format!("[con_id={ordinary}] focus"), None)?;
    switch_restore(f, id, "global-projected-minimize-restore")?;
    require(
        current_workspace(f)?["name"] == "2" && view(f, id)?["lumen_state"]["minimised"] == false,
        "Global projected restore lost current Space or minimized state",
    )?;
    request(f, &format!("[con_id={id}] lumen_window fullscreen"), None)?;
    wait_true(
        "projected foreground fullscreen ready before saved-state minimize",
        || Ok(view(f, id)?["fullscreen_mode"] == 1 && current_workspace(f)?["name"] == "1"),
    )?;
    request(f, &format!("[con_id={id}] lumen_window minimize"), None)?;
    request(f, &format!("[con_id={ordinary}] focus"), None)?;
    switch_restore(f, id, "global-saved-fullscreen-minimize-restore")?;
    require(
        view(f, id)?["fullscreen_mode"] == 1 && current_workspace(f)?["name"] == "1",
        "Global restore silently dropped saved fullscreen",
    )?;
    request(f, &format!("[con_id={id}] lumen_window hide"), None)?;
    request(f, &format!("[con_id={ordinary}] focus"), None)?;
    let before_unhide = f.tree()?;
    request(f, &format!("[con_id={id}] lumen_window unhide"), None)?;
    let after_unhide = f.tree()?;
    save(
        &f.options
            .output
            .join("background-saved-fullscreen-unhide.json"),
        &json!({"before":before_unhide,"after":after_unhide,"workspace":current_workspace(f)?}),
    )?;
    require(
        view(f, id)?["fullscreen_mode"] == 1
            && focused(f)? == ordinary
            && current_workspace(f)?["name"] == "2",
        "Background restore lost fullscreen or stole focus/Space",
    )?;
    switch_restore(f, id, "global-restored-fullscreen-activation")?;
    request(f, &format!("[con_id={id}] lumen_window hide"), None)?;
    request(f, &format!("[con_id={ordinary}] focus"), None)?;
    switch_restore(f, id, "global-saved-fullscreen-hidden-restore")?;
    require(
        view(f, id)?["fullscreen_mode"] == 1 && current_workspace(f)?["name"] == "1",
        "Global hidden restore silently dropped saved fullscreen",
    )?;
    request(f, &format!("[con_id={id}] lumen_window fullscreen"), None)?;
    request(f, "workspace 2", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    save(
        &f.options.output.join("interaction-result.json"),
        &json!({"passed":true,"checks":10,"pointer":pointer,"scope":"Real GTK pointer recipients and shared floating stacking; actual production switcher exact native captures; projected minimize current Space, saved fullscreen minimize/hide/global restore and focus-false restoration"}),
    )?;
    Ok(())
}

fn focus_journey(f: &mut Fixture, id: u64, owned: &[Value]) -> Result<()> {
    let origin_sibling = owned
        .iter()
        .find(|n| n["id"] != id)
        .and_then(|n| n["id"].as_u64())
        .ok_or("Origin sibling")?;
    let folder = f.options.output.join("focus-client");
    fs::create_dir(&folder)?;
    let client = f.sibling("lifecycle-client")?;
    let process = f.launch(
        &client,
        &[
            folder.to_str().ok_or("Focus client path")?,
            "org.example.MruB",
            "--projection-input",
        ],
        "focus-client.log",
    )?;
    let pid = f.processes[process].0.id();
    let other = wait("two ordinary focus witnesses", || {
        let rows = nodes(f)?
            .into_iter()
            .filter(|n| n["pid"] == pid)
            .collect::<Vec<_>>();
        Ok((rows.len() == 2).then_some(rows))
    })?;
    let current = other[0]["id"].as_u64().ok_or("Current witness")?;
    let remote = other[1]["id"].as_u64().ok_or("Remote witness")?;
    request(f, "create_output", None)?;
    wait_true("real second headless output", || {
        let outputs: Value =
            serde_json::from_str(&f.call("swaymsg", &["-t", "get_outputs", "-r"])?)?;
        Ok(outputs.as_array().is_some_and(|rows| {
            rows.iter()
                .any(|n| n["name"] == "HEADLESS-2" && n["active"] == true)
        }))
    })?;
    for command in [
        "output HEADLESS-2 resolution 1280x800",
        "output HEADLESS-2 position 1440 0",
        "workspace 3",
        "move workspace to output HEADLESS-2",
    ] {
        request(f, command, None)?;
    }
    request(
        f,
        &format!("[con_id={remote}] move container to workspace 3"),
        None,
    )?;
    request(f, &format!("[con_id={remote}] focus"), None)?;
    require(
        current_workspace(f)?["name"] == "3",
        "Remote witness not focused on second output",
    )?;
    let before_hide = f.tree()?;
    request(f, &format!("[con_id={id}] lumen_window hide"), None)?;
    let hidden = f.tree()?;
    save(
        &f.options.output.join("unfocused-other-output-hide.json"),
        &json!({"before":before_hide,"after":hidden,"workspace":current_workspace(f)?}),
    )?;
    require(
        focused(f)? == remote
            && current_workspace(f)?["name"] == "3"
            && view(f, id)?["lumen_state"]["hidden"] == true,
        "Hiding an unfocused projection stole other-output focus",
    )?;
    request(f, &format!("[con_id={id}] lumen_window unhide"), None)?;
    require(
        focused(f)? == remote && current_workspace(f)?["name"] == "3",
        "Background projection restore stole other-output focus",
    )?;
    let before_desktop = f.tree()?;
    request(f, "lumen_window show-desktop-on", None)?;
    save(
        &f.options
            .output
            .join("other-output-show-desktop-hidden.json"),
        &json!({"before":before_desktop,"after":f.tree()?,"workspace":current_workspace(f)?}),
    )?;
    require(
        current_workspace(f)?["name"] == "3"
            && view(f, id)?["visible"] == false
            && view(f, remote)?["visible"] == false,
        "Show Desktop projection stole other-output desktop or stayed visible",
    )?;
    request(f, "lumen_window show-desktop-off", None)?;
    wait_true("other-output Show Desktop focus restore", || {
        Ok(focused(f)? == remote
            && current_workspace(f)?["name"] == "3"
            && view(f, id)?["visible"] == true)
    })?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    require(
        current_workspace(f)?["name"] == "2",
        "Projected focus did not select effective output desktop",
    )?;
    let before_close = f.tree()?;
    request(f, &format!("[con_id={id}] kill"), None)?;
    wait_true("focused projection removed", || {
        Ok(!nodes(f)?.iter().any(|n| n["id"] == id))
    })?;
    let after_close = f.tree()?;
    save(
        &f.options.output.join("focused-projection-close.json"),
        &json!({"before":before_close,"after":after_close,"workspace":current_workspace(f)?,"expectedCurrent":current,"originSibling":origin_sibling,"remote":remote}),
    )?;
    require(
        focused(f)? == current && current_workspace(f)?["name"] == "2",
        "Closing focused projection jumped to origin sibling instead of current ordinary witness",
    )?;
    require(
        view(f, origin_sibling)?["lumen_all_desktops"]["origin_workspace"]["name"] == "1"
            && view(f, remote)?["lumen_all_desktops"]["origin_workspace"]["name"] == "3",
        "Focused close relocated unrelated windows",
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":5,"scope":"Actual second-output unfocused hide/background restore and Show Desktop focus; focused projection destruction retains effective Space and ordinary current-Space focus without changing origin sibling"}),
    )?;
    Ok(())
}
