//! Actual production Overview offers and the captured native transfer protocol.
use super::*;
use lumen_spaces::transfer;

fn native_window(ctx: &Context, n: usize) -> Result<Value> {
    ctx.window(n)?
        .ok_or_else(|| fail(format!("Window {n} absent")))
}
pub(super) fn owner(ctx: &Context) -> Result<Value> {
    let tree = ctx.native("get_tree")?;
    let mut all = Vec::new();
    nodes(&tree, &mut all);
    all.into_iter()
        .find(|n| n["pid"] == ctx.shell_pid && n["name"] == "Window Overview")
        .cloned()
        .ok_or_else(|| fail("Native owned Overview absent"))
}
pub(super) fn location(ctx: &Context, id: &Value) -> Result<Value> {
    fn walk(node: &Value, id: &Value, workspace: &Value, output: &Value) -> Option<Value> {
        let workspace = if node["type"] == "workspace" {
            node
        } else {
            workspace
        };
        let output = if node["type"] == "output" {
            node
        } else {
            output
        };
        if node["id"] == *id {
            return Some(
                json!({"workspace":workspace["id"],"name":workspace["name"],"output":output["id"],"outputName":output["name"]}),
            );
        }
        for key in ["nodes", "floating_nodes"] {
            for child in node[key].as_array().into_iter().flatten() {
                if let Some(found) = walk(child, id, workspace, output) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(&ctx.native("get_tree")?, id, &Value::Null, &Value::Null)
        .ok_or_else(|| fail("Native source location absent"))
}
pub(super) fn frame(ctx: &Context, id: &Value) -> Result<Value> {
    let tree = ctx.native("get_tree")?;
    let mut all = Vec::new();
    nodes(&tree, &mut all);
    let n = all
        .into_iter()
        .find(|n| n["id"] == *id)
        .ok_or("Captured native node absent")?;
    Ok(
        json!({"id":n["id"],"pid":n["pid"],"foreign":n["foreign_toplevel_identifier"],"rect":n["rect"],"window_rect":n["window_rect"],"state":n["lumen_state"],"floating":n["floating"],"fullscreen":n["fullscreen_mode"],"location":location(ctx,id)?}),
    )
}
pub(super) fn stable(ctx: &Context) -> Result<Value> {
    let mut previous = Value::Null;
    wait("settled private native state", || {
        let now = ctx.native("get_tree")?;
        if previous == now {
            return Ok(Some(now));
        }
        previous = now;
        Ok(None)
    })
}
pub(super) fn command(ctx: &Context, wire: &[u8], accepted: bool, label: &str) -> Result<()> {
    let before = stable(ctx)?;
    let hex = wire.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let output = Command::new(&ctx.swaymsg)
        .args(["-r", &format!("lumen_space_transfer {hex}")])
        .env_clear()
        .envs(&ctx.vars)
        .output()?;
    let reply: Value = serde_json::from_slice(&output.stdout)?;
    let after = stable(ctx)?;
    save(
        &ctx.opt.output.join(format!("transfer-{label}.json")),
        &json!({"before":before,"reply":reply,"after":after,"exit":output.status.code(),"stderr":String::from_utf8_lossy(&output.stderr),"wireHex":hex}),
    )?;
    ensure(
        reply
            .as_array()
            .is_some_and(|r| r.len() == 1 && r[0]["success"] == accepted),
        format!("Transfer result {label}: {reply}"),
    )?;
    if accepted {
        ensure(output.status.success(), "Native positive transport failed")?;
    } else {
        ensure(
            !output.status.success()
                && reply[0]["error"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("LST_REJECTED:")),
            "Rejection did not prove preflight refusal",
        )?;
        ensure(
            before == after,
            format!("Rejected native transfer mutated state: {label}"),
        )?;
    }
    Ok(())
}
pub(super) fn capture(
    ctx: &Context,
    id: &Value,
    name: &str,
) -> Result<(Value, Value, Value, Vec<u8>)> {
    let tree = stable(ctx)?;
    let snapshot = transfer::snapshot(&tree)?;
    let source = transfer::source(&tree, id)?;
    let rows = ctx.native("get_workspaces")?;
    let live = rows
        .as_array()
        .ok_or("Native workspaces absent")?
        .iter()
        .find(|w| w["name"] == name);
    let desktop = json!({"logicalId":"fixture-logical-slot","name":name,"id":live.map(|w|w["id"].clone()),"output":live.map(|w|w["output"].clone()).unwrap_or(json!("HEADLESS-1"))});
    let destination = transfer::destination(&tree, &desktop)?;
    let wire = transfer::packet(&snapshot, &source, &destination)?;
    Ok((snapshot, source, destination, wire))
}
fn ready(ctx: &Context) -> Result<Value> {
    ctx.ipc("refresh", &[])?;
    wait("production captured Overview ready", || {
        let u = ctx.ui()?;
        Ok((u["visible"] == true
            && u["busy"] == false
            && u["snapshot"].is_object()
            && u["windows"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|w| w["id"] == ctx.captured[1]["id"] && w["transfer"].is_object())
            }))
        .then_some(u))
    })
}
pub(super) fn point(ctx: &Context, name: &str) -> Result<(f64, f64)> {
    let row: Value = serde_json::from_str(&ctx.ipc("geometry", &[name])?)?;
    ensure(
        row["enabled"] == true,
        "Native transfer control unavailable",
    )?;
    let o = owner(ctx)?;
    Ok((
        o["rect"]["x"].as_f64().ok_or("Owner x")?
            + row["x"].as_f64().ok_or("Control x")?
            + row["width"].as_f64().ok_or("Control width")? * 0.5,
        o["rect"]["y"].as_f64().ok_or("Owner y")?
            + row["y"].as_f64().ok_or("Control y")?
            + row["height"].as_f64().ok_or("Control height")? * 0.5,
    ))
}
fn begin(ctx: &mut Context) -> Result<()> {
    ctx.ipc("reveal", &[])?;
    let count = ctx.ui()?["drag"]["commits"]
        .as_u64()
        .ok_or("Commit count")?;
    save(
        &ctx.opt
            .output
            .join(format!("gesture-{count}-press-tree.json")),
        &ctx.native("get_tree")?,
    )?;
    save(
        &ctx.opt
            .output
            .join(format!("gesture-{count}-press-ui.json")),
        &ctx.ui()?,
    )?;
    let name = format!("desktop-window-drag-{}", ctx.captured[1]["id"]);
    let p = point(ctx, &name)?;
    ctx.move_pointer(p.0, p.1)?;
    ctx.press()?;
    ctx.move_pointer(p.0 + 14., p.1 + 2.)?;
    wait("actual owned native desktop offer", || {
        Ok((ctx.ui()?["drag"]["active"] == true).then_some(()))
    })
}
fn drop_on(ctx: &mut Context, desktop: &Value) -> Result<()> {
    let count = ctx.ui()?["drag"]["commits"]
        .as_u64()
        .ok_or("Commit count")?;
    save(
        &ctx.opt
            .output
            .join(format!("gesture-{count}-drop-tree.json")),
        &ctx.native("get_tree")?,
    )?;
    save(
        &ctx.opt.output.join(format!("gesture-{count}-drop-ui.json")),
        &ctx.ui()?,
    )?;
    let p = point(
        ctx,
        &format!(
            "desktop-{}",
            desktop["logicalId"].as_str().ok_or("Logical desktop ID")?
        ),
    )?;
    ctx.move_pointer(p.0, p.1)?;
    ctx.release()
}
pub(super) fn apply_role_refusal(
    ctx: &Context,
    source: &Value,
    destination: &Value,
    label: &str,
) -> Result<()> {
    let tree = stable(ctx)?;
    ensure(
        transfer::source(&tree, &source["id"]).is_err(),
        "Typed backend offered unsupported source",
    )?;
    let wire = transfer::packet(&transfer::snapshot(&tree)?, source, destination)?;
    command(ctx, &wire, false, label)
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let mut checks = Vec::new();
    let id = ctx.captured[1]["id"].clone();
    if ctx.opt.effects {
        ensure(
            fs::read_to_string(ctx.opt.output.join("sway.log"))?.contains("llvmpipe"),
            "Software effects renderer not witnessed",
        )?;
    }
    ensure(
        ctx.native("get_version")?["space_transfer_guard"] == 3,
        "Native captured transfer capability absent",
    )?;
    command(ctx, &[0], false, "capability-dispatch")?;
    let event_tool = ctx.swaymsg.clone();
    ctx.launch(
        &event_tool,
        &[
            "-m",
            "-r",
            "-t",
            "subscribe",
            "[\"window\",\"workspace\",\"output\"]",
        ],
        "transfer-events.jsonl",
        false,
    )?;
    ctx.sway(&format!("[con_id={}] move container to workspace 2; [con_id={}] move workspace to output HEADLESS-1",ctx.captured[3]["id"],ctx.captured[3]["id"]),true)?;
    ctx.sway("focus output HEADLESS-1; workspace 1", true)?;
    let registered = ctx.named("lumen-spaces", &["status"])?;
    fs::write(ctx.opt.output.join("initial-policy.json"), registered)?;
    ctx.ipc("open", &[&id.to_string()])?;
    let ui = ready(ctx)?;
    ensure(
        ctx.ipc("display", &["HEADLESS-1"])? == "true",
        "Actual desktop display selector unavailable",
    )?;
    let destination = ui["desktops"]
        .as_array()
        .ok_or("Desktop catalog absent")?
        .iter()
        .find(|d| d["name"] == "2")
        .cloned()
        .ok_or("Native destination catalog absent")?;
    ctx.pointer_start()?;
    let initial = frame(ctx, &id)?;
    begin(ctx)?;
    ensure(
        frame(ctx, &id)? == initial && ctx.ui()?["drag"]["commits"] == 0,
        "Preview moved native window",
    )?;
    ctx.key(&["-k", "Escape"])?;
    ctx.release()?;
    wait("actual offer cancelled", || {
        Ok((ctx.ui()?["drag"]["active"] == false).then_some(()))
    })?;
    ensure(
        frame(ctx, &id)? == initial && ctx.ui()?["drag"]["commits"] == 0,
        "Escape moved native source",
    )?;
    checks.push("Actual native drag preview/Escape changes no source frame, placement, identity or commit count");
    ready(ctx)?;
    super::desktop_card::escape_cancel(ctx, &id)?;
    super::desktop_card::disabled_press_cancel(ctx, &id)?;
    checks.push("Actual card Escape and a physically pressed card disabled by observed owner refresh retire without transfer or deferred click replay");
    super::desktop_card::outside_cancel(ctx, &id)?;
    checks.push("Physical ordinary card drag released outside Overview cancels without moving, activating or committing the source");
    let focused_before = owner(ctx)?;
    super::desktop_card::begin(ctx, &id)?;
    save(
        &ctx.opt.output.join("before-physical-drop-tree.json"),
        &ctx.native("get_tree")?,
    )?;
    save(
        &ctx.opt.output.join("before-physical-drop-ui.json"),
        &ctx.ui()?,
    )?;
    drop_on(ctx, &destination)?;
    wait("actual captured Qt drop committed", || {
        let observed = ctx.ui()?;
        if observed["drag"]["error"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        {
            save(
                &ctx.opt.output.join("failed-physical-drop-tree.json"),
                &ctx.native("get_tree")?,
            )?;
            return Err(fail(format!(
                "Captured native transfer refused: {}",
                observed["drag"]["error"]
            )));
        }
        Ok((observed["drag"]["commits"] == 1 && location(ctx, &id)?["name"] == "2").then_some(()))
    })?;
    ensure(
        owner(ctx)?["id"] == focused_before["id"] && owner(ctx)?["focused"] == true,
        "Drop activated target desktop instead of retaining Overview focus",
    )?;
    let moved = frame(ctx, &id)?;
    ensure(
        moved["rect"] == initial["rect"]
            && moved["foreign"] == initial["foreign"]
            && moved["pid"] == initial["pid"],
        "Same-output drop changed frame or live source identity",
    )?;
    ctx.picture("desktop-transfer-existing.png")?;
    checks.push("Physical production Overview card→desktop tile native drop commits once, preserves exact document/frame, and retains Overview focus without activating destination");
    let (_, source, _, wire) = capture(ctx, &id, "1")?;
    for (label, bad) in [
        ("truncated", wire[..wire.len() - 1].to_vec()),
        ("trailing", {
            let mut b = wire.clone();
            b.push(0);
            b
        }),
        ("unknown-version", {
            let mut b = wire.clone();
            b[3] = b'9';
            b
        }),
        ("obsolete-transfer-version", {
            let mut b = wire.clone();
            b[3] = b'1';
            b
        }),
        ("invalid-utf8", {
            let mut b = wire.clone();
            let last = b.len() - 1;
            b[last] = 0xff;
            b
        }),
        ("nul-name", {
            let mut b = wire.clone();
            let last = b.len() - 1;
            b[last] = 0;
            b
        }),
        ("oversized", vec![1; 2049]),
        ("unsafe-js-id", {
            let mut b = wire.clone();
            b[60..68].copy_from_slice(&9_007_199_254_740_992_u64.to_be_bytes());
            b
        }),
    ] {
        command(ctx, &bad, false, label)?;
    }
    checks.push("Actual native parser rejects truncated/trailing/unknown version, invalid UTF-8/NUL, just-over-limit and unsafe integer packets with full native state unchanged");
    let (_, _, _, absent) = capture(ctx, &id, "lumen:fixture-absent-slot")?;
    let mut malformed_absent = absent.clone();
    malformed_absent.push(0);
    command(ctx, &malformed_absent, false, "absent-slot-invalid-wire")?;
    ensure(
        !ctx.native("get_workspaces")?
            .as_array()
            .ok_or("Workspaces")?
            .iter()
            .any(|w| w["name"] == "lumen:fixture-absent-slot"),
        "Refusal left a phantom desktop",
    )?;
    let generation = ctx.native("get_tree")?["space_transfer_generation"].clone();
    ctx.sway("workspace lumen:fixture-absent-slot", true)?;
    ctx.sway(&format!("[con_id={}] focus", owner(ctx)?["id"]), true)?;
    wait("native absent slot returned to absence", || {
        Ok((!ctx
            .native("get_workspaces")?
            .as_array()
            .ok_or("Workspaces")?
            .iter()
            .any(|w| w["name"] == "lumen:fixture-absent-slot"))
        .then_some(()))
    })?;
    ensure(
        ctx.native("get_tree")?["space_transfer_generation"] != generation,
        "Native create/destroy serial did not advance",
    )?;
    command(ctx, &absent, false, "absent-slot-create-destroy-aba")?;
    checks.push("Refused empty-slot packet leaves no phantom desktop; real create/destroy ABA advances the captured native allocation serial and rejects the retired absence witness");
    let old_width = source["rect"][2].as_i64().ok_or("Captured width")?;
    let old_height = source["rect"][3].as_i64().ok_or("Captured height")?;
    ctx.sway(
        &format!(
            "[con_id={id}] resize set width {} px height {} px",
            old_width + 80,
            old_height + 40
        ),
        true,
    )?;
    wait("actual source geometry changed", || {
        Ok((native_window(ctx, 1)?["rect"]["width"] != source["rect"][2]).then_some(()))
    })?;
    command(ctx, &wire, false, "source-resized")?;
    ctx.sway(
        &format!("[con_id={id}] resize set width {old_width} px height {old_height} px"),
        true,
    )?;
    let client_pid = ctx.captured[1]["pid"].as_u64().ok_or("Owned process PID")?;
    let client = ctx
        .procs
        .iter()
        .position(|p| u64::from(p.0.id()) == client_pid)
        .ok_or("Owned native client absent")?;
    let child_id = ctx.captured[2]["id"].clone();
    // A floating position command selects the output's current workspace.
    // Position first, then establish the explicitly captured family origin.
    ctx.sway(&format!("[con_id={child_id}] move position 464 px 316 px; [con_id={child_id}] move container to workspace 2"), true)?;
    ctx.write(client, "reparent\n")?;
    wait("ordinary family shares captured origin", || {
        save(
            &ctx.opt.output.join("family-origin-precondition.json"),
            &json!({"child":native_window(ctx,2)?,"location":location(ctx,&child_id)?}),
        )?;
        Ok(
            (native_window(ctx, 2)?["lumen_relationships"]["parent"] == id
                && location(ctx, &child_id)?["name"] == "2")
                .then_some(()),
        )
    })?;
    let (_, _, _, family_old) = capture(ctx, &id, "1")?;
    let child_width = native_window(ctx, 2)?["rect"]["width"]
        .as_i64()
        .ok_or("Child width")?;
    ctx.sway(
        &format!(
            "[con_id={child_id}] resize set width {} px",
            child_width + 7
        ),
        true,
    )?;
    wait("descendant frame changes before dispatch", || {
        Ok((native_window(ctx, 2)?["rect"]["width"] != child_width).then_some(()))
    })?;
    command(ctx, &family_old, false, "descendant-frame-stale")?;
    ctx.sway(
        &format!("[con_id={child_id}] resize set width {child_width} px"),
        true,
    )?;
    for tiled in [false, true] {
        if tiled {
            ctx.sway(&format!("[con_id={id}] floating disable"), true)?;
            wait("ordinary root becomes tiled", || {
                let node = native_window(ctx, 1)?;
                save(&ctx.opt.output.join("tiled-root-precondition.json"), &node)?;
                Ok(
                    matches!(node["floating"].as_str(), Some("auto_off" | "user_off"))
                        .then_some(()),
                )
            })?;
            let root = native_window(ctx, 1)?;
            let x = root["rect"]["x"].as_i64().ok_or("Tiled root x")? + 24;
            let y = root["rect"]["y"].as_i64().ok_or("Tiled root y")? + 36;
            ctx.sway(&format!("[con_id={child_id}] move position {x} px {y} px; [con_id={child_id}] move container to workspace 2"), true)?;
        }
        let phase = if tiled { "tiled" } else { "floating" };
        for target in ["1", "3", "2"] {
            let root_before = frame(ctx, &id)?;
            let child_before = frame(ctx, &child_id)?;
            let (_, _, _, family_wire) = capture(ctx, &id, target)?;
            command(
                ctx,
                &family_wire,
                true,
                &format!("ordinary-{phase}-family-to-{target}"),
            )?;
            wait("entire ordinary family reaches one destination", || {
                Ok((location(ctx, &id)?["name"] == target
                    && location(ctx, &child_id)?["name"] == target)
                    .then_some(()))
            })?;
            let root_after = frame(ctx, &id)?;
            let child_after = frame(ctx, &child_id)?;
            ensure(
                (if tiled {
                    matches!(
                        root_after["floating"].as_str(),
                        Some("auto_off" | "user_off")
                    )
                } else {
                    matches!(root_after["floating"].as_str(), Some("auto_on" | "user_on"))
                }) && root_before["floating"] == root_after["floating"]
                    && child_before["floating"] == child_after["floating"],
                "Family transfer converted native tiling/floating roles",
            )?;
            if tiled && target == "3" {
                ensure(
                    root_before["rect"]["width"] != root_after["rect"]["width"],
                    "Different-width output did not exercise native tiled root layout",
                )?;
            }
            for key in ["width", "height"] {
                ensure(
                    (tiled || root_before["rect"][key] == root_after["rect"][key])
                        && child_before["rect"][key] == child_after["rect"][key],
                    "Family transfer resized a member",
                )?;
            }
            for key in ["x", "y"] {
                let offset = |root: &Value, child: &Value| -> Result<i64> {
                    Ok(child["rect"][key].as_i64().ok_or("Child coordinate")?
                        - root["rect"][key].as_i64().ok_or("Root coordinate")?)
                };
                ensure(
                    offset(&root_before, &child_before)? == offset(&root_after, &child_after)?,
                    "Family transfer changed relative offsets",
                )?;
            }
            ensure(
                root_before["foreign"] == root_after["foreign"]
                    && child_before["foreign"] == child_after["foreign"]
                    && root_before["pid"] == root_after["pid"]
                    && child_before["pid"] == child_after["pid"]
                    && native_window(ctx, 2)?["lumen_relationships"]["parent"] == id
                    && owner(ctx)?["focused"] == true,
                "Family transfer changed ownership, identity or Overview focus",
            )?;
            save(
                &ctx.opt
                    .output
                    .join(format!("ordinary-{phase}-family-{target}.json")),
                &json!({"rootBefore":root_before,"rootAfter":root_after,"childBefore":child_before,"childAfter":child_after}),
            )?;
        }
    }
    checks.push("Ordinary floating and tiled roots carry nonmodal floating descendants within and across outputs, preserving exact live identities, ownership, child sizes/offsets and Overview focus; floating root sizes remain unchanged and stale descendant geometry refuses before mutation");
    ctx.write(client, "unparent\n")?;
    wait(
        "ordinary family detached before modal boundary case",
        || {
            Ok(native_window(ctx, 2)?["lumen_relationships"]["parent"]
                .is_null()
                .then_some(()))
        },
    )?;
    ctx.sway(&format!("[con_id={id}] floating enable; [con_id={id}] resize set width {old_width} px height {old_height} px; [con_id={id}] move position {} px {} px; [con_id={id}] move container to workspace 2", source["rect"][0], source["rect"][1]), true)?;
    wait(
        "independent floating root restored after tiled family proof",
        || {
            Ok((native_window(ctx, 1)?["floating"] == "user_on"
                && location(ctx, &id)?["name"] == "2")
                .then_some(()))
        },
    )?;
    let (_, source, dest, wire) = capture(ctx, &id, "1")?;
    ctx.write(client, "reparent\n")?;
    wait("actual child reparent committed", || {
        Ok((native_window(ctx, 2)?["lumen_relationships"]["parent"] == id).then_some(()))
    })?;
    ctx.write(client, "modal\n")?;
    wait("actual new modal belongs to captured root", || {
        Ok(
            (native_window(ctx, 2)?["lumen_relationships"]["parent"] == id
                && native_window(ctx, 2)?["lumen_relationships"]["modal"] == true)
                .then_some(()),
        )
    })?;
    command(ctx, &wire, false, "new-modal-stale-packet")?;
    apply_role_refusal(ctx, &source, &dest, "new-modal-fresh-digest")?;
    ctx.write(client, "unmodal\n")?;
    wait("actual modal flag cleared", || {
        Ok((native_window(ctx, 2)?["lumen_relationships"]["modal"] == false).then_some(()))
    })?;
    ctx.write(client, "unparent\n")?;
    wait("modal detached", || {
        Ok(native_window(ctx, 2)?["lumen_relationships"]["parent"]
            .is_null()
            .then_some(()))
    })?;
    super::desktop_fullscreen::run(ctx, &id)?;
    checks.push("Actual non-view fullscreen source ancestors and destination splits refuse fresh current-frame packets with the entire native tree unchanged; ordinary transfer remains available after leaving fullscreen");
    let (_, source, dest, wire) = capture(ctx, &id, "1")?;
    ctx.group_action("on")?;
    command(ctx, &wire, false, "group-membership-stale-packet")?;
    let source_group = ctx.group(1)?["id"].clone();
    ctx.group_action(&format!(
        "add {source_group} {} {}",
        ctx.captured[3]["id"], ctx.captured[3]["pid"]
    ))?;
    ctx.group_action(&format!("select {source_group}"))?;
    ctx.sway(&format!("[con_id={}] focus", owner(ctx)?["id"]), true)?;
    wait(
        "selected grouped source visible before captured move",
        || Ok((native_window(ctx, 1)?["lumen_state"]["group_hidden"] == false).then_some(())),
    )?;
    let peer_before = frame(ctx, &ctx.captured[3]["id"])?;
    let source_before = frame(ctx, &id)?;
    let (_, _, _, grouped_wire) = capture(ctx, &id, "1")?;
    command(ctx, &grouped_wire, true, "visible-grouped-root-positive")?;
    wait("moved root receives destination group", || {
        Ok(
            (location(ctx, &id)?["name"] == "1" && ctx.group(1)?["id"] != source_group)
                .then_some(()),
        )
    })?;
    let source_after = frame(ctx, &id)?;
    let peer_after = frame(ctx, &ctx.captured[3]["id"])?;
    ensure(
        peer_before == peer_after
            && ctx.group(3)?["id"] == source_group
            && source_before["rect"] == source_after["rect"]
            && source_before["foreign"] == source_after["foreign"]
            && source_before["pid"] == source_after["pid"]
            && owner(ctx)?["focused"] == true,
        "Grouped transfer changed partner/frame/identity or Overview focus",
    )?;
    save(
        &ctx.opt.output.join("grouped-root-transfer.json"),
        &json!({"sourceBefore":source_before,"sourceAfter":source_after,"peerBefore":peer_before,"peerAfter":peer_after,"oldGroup":source_group,"newGroup":ctx.group(1)?}),
    )?;
    ensure(
        native_window(ctx, 1)?["lumen_state"]["group_hidden"] == true,
        "Destination's existing selected group was replaced by transferred group",
    )?;
    let mut hidden_source = source.clone();
    hidden_source["workspace"] = location(ctx, &id)?["workspace"].clone();
    let hidden_tree = ctx.native("get_tree")?;
    ensure(
        transfer::source(&hidden_tree, &id).is_err(),
        "Hidden grouped source offered transfer",
    )?;
    let mut return_destination = dest.clone();
    return_destination["name"] = json!("2");
    return_destination["workspace"] = source["workspace"].clone();
    apply_role_refusal(
        ctx,
        &hidden_source,
        &return_destination,
        "group-hidden-fresh-digest",
    )?;
    ctx.group_action("off")?;
    ctx.sway(
        &format!("[con_id={id}] move container to workspace 2"),
        true,
    )?;
    ctx.sway(&format!("[con_id={id}] lumen_window minimize"), true)?;
    command(ctx, &wire, false, "minimized-source")?;
    ctx.sway(&format!("[con_id={id}] scratchpad show"), true)?;
    wait("minimized source restored", || {
        Ok((native_window(ctx, 1)?["lumen_state"]["minimised"] == false
            && location(ctx, &id)?["name"] == "2")
            .then_some(()))
    })?;
    checks.push("Actual source resize, newly attached modal, stale group membership, group-hidden source and minimization refuse; fresh-digest role forgeries cannot bypass native family/group preflight");
    checks.push("Visible grouped root moves independently through native reconciliation, preserving its old partner's exact state/group, destination selection, source frame/identity and Overview focus");
    let a = ctx.captured[0]["id"].clone();
    ctx.sway(&format!("[con_id={a}] lumen_window minimize"), true)?;
    wait("unrelated native window hidden in scratchpad", || {
        Ok((location(ctx, &a)?["name"] == "__i3_scratch"
            && native_window(ctx, 0)?["lumen_state"]["minimised"] == true)
            .then_some(()))
    })?;
    ctx.sway(&format!("[con_id={}] focus", owner(ctx)?["id"]), true)?;
    let hidden_before = frame(ctx, &a)?;
    let (_, _, _, empty_with_hidden) = capture(ctx, &id, "lumen:fixture-empty-with-hidden-window")?;
    command(
        ctx,
        &empty_with_hidden,
        true,
        "absent-slot-unrelated-hidden-window",
    )?;
    ensure(
        frame(ctx, &a)? == hidden_before,
        "Empty-target transfer changed unrelated hidden window",
    )?;
    let (_, _, _, back) = capture(ctx, &id, "2")?;
    command(ctx, &back, true, "hidden-window-source-return")?;
    ctx.sway(&format!("[con_id={a}] scratchpad show"), true)?;
    ctx.sway(&format!("[con_id={}] focus", owner(ctx)?["id"]), true)?;
    checks.push("An actual unrelated minimized native window does not block an absent destination; its complete frame, placement and hidden state remain unchanged by the independent transfer");
    let (_, _, _, wire) = capture(ctx, &id, "1")?;
    let locker = ctx.opt.locker.clone();
    let li = ctx.launch(&locker, &[], "transfer-lock.log", true)?;
    wait("actual lock", || {
        Ok(truth(&ctx.groups()?["locked"]).then_some(()))
    })?;
    command(ctx, &wire, false, "locked")?;
    ctx.write(li, "u\n")?;
    wait("lock reaped", || {
        Ok(ctx.procs[li].0.try_wait()?.map(|_| ()))
    })?;
    wait("actual unlock", || {
        Ok((ctx.groups()?["locked"] == false).then_some(()))
    })?;
    checks.push("Native lock refuses captured transfer without frame or state mutation; owned lock process is reaped");
    // Exercise actual native ordinary movement before the new absent-slot UI case.
    ctx.sway(&format!("[con_id={}] focus", owner(ctx)?["id"]), true)?;
    let (_, _, _, wire) = capture(ctx, &id, "1")?;
    command(ctx, &wire, true, "ordinary-positive")?;
    ensure(
        location(ctx, &id)?["name"] == "1" && owner(ctx)?["focused"] == true,
        "Typed ordinary move changed focus or missed destination",
    )?;
    ready(ctx)?;
    let prior = ctx.ui()?["desktops"]
        .as_array()
        .ok_or("Catalog")?
        .iter()
        .map(|d| d["logicalId"].clone())
        .collect::<Vec<_>>();
    ctx.ipc("create", &[])?;
    let empty = wait("actual logical empty desktop created", || {
        let u = ctx.ui()?;
        Ok(u["desktops"]
            .as_array()
            .and_then(|d| {
                d.iter().find(|d| {
                    d["persistent"] == true && d["id"].is_null() && !prior.contains(&d["logicalId"])
                })
            })
            .cloned())
    })?;
    ready(ctx)?;
    begin(ctx)?;
    drop_on(ctx, &empty)?;
    wait("physical empty-slot drop committed", || {
        if ctx.ui()?["drag"]["error"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        {
            save(
                &ctx.opt.output.join("failed-empty-drop-tree.json"),
                &ctx.native("get_tree")?,
            )?;
        }
        Ok(
            (ctx.ui()?["drag"]["commits"] == 2 && location(ctx, &id)?["name"] == empty["name"])
                .then_some(()),
        )
    })?;
    ensure(
        owner(ctx)?["focused"] == true,
        "Empty destination creation activated its desktop",
    )?;
    checks.push("Actual logical persistent empty desktop is created and receives a physical production drop in one callback, with Overview focus retained");
    let back = ctx.ui()?["desktops"]
        .as_array()
        .ok_or("Catalog")?
        .iter()
        .find(|d| d["name"] == "1")
        .cloned()
        .ok_or("Original desktop retired")?;
    ctx.ipc(
        "legacyMove",
        &[back["logicalId"]
            .as_str()
            .ok_or("Original logical identity")?],
    )?;
    wait("existing explicit move compatibility", || {
        Ok((location(ctx, &id)?["name"] == "1" && ctx.ui()?["busy"] == false).then_some(()))
    })?;
    ensure(
        frame(ctx, &id)?["foreign"] == initial["foreign"],
        "Explicit move replaced live source",
    )?;
    checks.push("Existing explicit Overview Move command continues to move the exact live source to its selected logical desktop");
    ctx.ipc("showFiles", &[])?;
    let files = wait("actual production Files native surface", || {
        let tree = ctx.native("get_tree")?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok(all
            .into_iter()
            .find(|n| {
                n["pid"] == ctx.shell_pid
                    && n["name"]
                        .as_str()
                        .is_some_and(|title| title.starts_with("Files — "))
            })
            .cloned())
    })?;
    ensure(
        files["app_id"] == "org.quickshell",
        "Owned Files native classification precondition",
    )?;
    let original_source = ctx.captured[1].clone();
    ctx.captured[1] = files.clone();
    ctx.ipc("open", &[&files["id"].to_string()])?;
    // The newly mapped Files surface owns focus; bring the exact observed
    // Overview surface forward before capturing a user gesture on its card.
    ctx.sway(&format!("[con_id={}] focus", owner(ctx)?["id"]), true)?;
    wait(
        "Overview owns native focus before Files card gesture",
        || {
            Ok(
                (owner(ctx)?["focused"] == true && ctx.ui()?["overviewActive"] == true)
                    .then_some(()),
            )
        },
    )?;
    ready(ctx)?;
    let files_before = frame(ctx, &files["id"])?;
    let commits = ctx.ui()?["drag"]["commits"]
        .as_u64()
        .ok_or("Commit counter")?;
    super::desktop_card::begin(ctx, &files["id"])?;
    drop_on(ctx, &destination)?;
    wait("actual Files physical transfer", || {
        Ok((location(ctx, &files["id"])?["name"] == "2"
            && ctx.ui()?["drag"]["commits"] == commits + 1)
            .then_some(()))
    })?;
    let files_after = frame(ctx, &files["id"])?;
    ensure(
        files_after["foreign"] == files_before["foreign"]
            && files_after["pid"] == files_before["pid"]
            && files_after["rect"] == files_before["rect"]
            && owner(ctx)?["focused"] == true,
        "Files transfer changed native identity/frame or Overview focus",
    )?;
    checks.push("Actual ordinary production Files window shares the exact native transfer path without an application-name exemption, preserving identity/frame and Overview focus");
    ctx.captured[1] = original_source;
    ctx.ipc("open", &[&id.to_string()])?;
    ready(ctx)?;
    let (_, _, _, old) = capture(ctx, &id, "2")?;
    let before_replacement_commits = ctx.ui()?["drag"]["commits"].clone();
    super::desktop_card::begin(ctx, &id)?;
    ctx.write(client, "close-second\n")?;
    wait("captured native source closed", || {
        Ok(ctx.window(1)?.is_none().then_some(()))
    })?;
    ctx.write(client, "reopen-second\n")?;
    ctx.captured[1] = wait("same-process same-application replacement", || {
        ctx.window(1)
    })?;
    ensure(
        ctx.captured[1]["id"] != id && ctx.captured[1]["pid"] == ctx.captured[0]["pid"],
        "Replacement identity precondition failed",
    )?;
    stable(ctx)?;
    let replacement_before_release = frame(ctx, &ctx.captured[1]["id"])?;
    drop_on(ctx, &destination)?;
    wait(
        "retired card source cannot transfer its replacement",
        || {
            let ui = ctx.ui()?;
            Ok((ui["drag"]["active"] == false && ui["drag"]["busy"] == false).then_some(()))
        },
    )?;
    let replacement_ui = ctx.ui()?;
    ensure(
        replacement_ui["drag"]["commits"] == before_replacement_commits
            && replacement_ui["visible"] == true
            && frame(ctx, &ctx.captured[1]["id"])? == replacement_before_release,
        "Retired card source moved or activated its replacement",
    )?;
    save(
        &ctx.opt.output.join("card-retired-source.json"),
        &json!({"beforeRelease":replacement_before_release,"afterRelease":frame(ctx,&ctx.captured[1]["id"])?,"ui":replacement_ui}),
    )?;
    command(ctx, &old, false, "closed-replaced-source")?;
    checks.push("Closed source and same-process/same-application replacement cannot inherit a captured transfer");
    let replacement = ctx.captured[1]["id"].clone();
    ctx.ipc("open", &[&replacement.to_string()])?;
    ctx.sway(&format!("[con_id={}] focus", owner(ctx)?["id"]), true)?;
    ready(ctx)?;
    super::desktop_card::click_activate(ctx, &replacement)?;
    checks.push("Ordinary card click retains existing exact-window activation and does not turn into a desktop transfer");
    let (_, _, _, destination_gone) = capture(ctx, &replacement, "3")?;
    ctx.sway("output HEADLESS-2 disable", true)?;
    command(ctx, &destination_gone, false, "destination-output-gone")?;
    checks.push("Destination output loss retires the captured target without moving the live replacement or mutating the post-disconnect native state");
    ctx.result(&checks)
}

pub fn unsupported(ctx: &mut Context) -> Result<()> {
    wait("actual legacy owned IPC handler ready", || {
        let ui = ctx.ui()?;
        Ok((ui["visible"].is_boolean() && ui["busy"].is_boolean()).then_some(()))
    })?;
    let version = ctx.native("get_version")?;
    let tree = ctx.native("get_tree")?;
    save(
        &ctx.opt.output.join("unsupported-provider-facts.json"),
        &json!({"version":version,"tree":tree}),
    )?;
    ensure(
        version.get("space_transfer_guard").is_none() && tree.get("space_transfer_guard").is_none(),
        "Legacy provider falsely advertised captured transfer",
    )?;
    ensure(
        transfer::snapshot(&tree).is_err(),
        "Legacy tree produced supported capture",
    )?;
    ctx.ipc("open", &[&ctx.captured[1]["id"].to_string()])?;
    wait("actual legacy Overview response", || {
        Ok((ctx.ui()?["visible"] == true && ctx.ui()?["busy"] == false).then_some(()))
    })?;
    let ui = ctx.ui()?;
    let rows = ui["windows"].as_array().ok_or("Legacy rows")?;
    ensure(
        ui["snapshot"].is_null()
            && rows
                .iter()
                .any(|window| window["id"] == ctx.captured[1]["id"])
            && rows.iter().all(|window| window["transfer"].is_null()),
        "Legacy UI offered unsupported native window transfer",
    )?;
    let before = stable(ctx)?;
    let output = Command::new(&ctx.swaymsg)
        .args(["-r", "lumen_space_transfer 00"])
        .env_clear()
        .envs(&ctx.vars)
        .output()?;
    let reply: Value = serde_json::from_slice(&output.stdout)?;
    let after = stable(ctx)?;
    save(
        &ctx.opt.output.join("unsupported-command-receipt.json"),
        &json!({"before":before,"after":after,"reply":reply,"exit":output.status.code(),"stderr":String::from_utf8_lossy(&output.stderr)}),
    )?;
    ensure(
        output.status.code() == Some(2)
            && reply.as_array().is_some_and(|rows| {
                rows.len() == 1
                    && rows[0]["success"] == false
                    && rows[0]["error"].as_str().is_some_and(|error| {
                        error.contains("Unknown/invalid command")
                            && !error.contains("LST_REJECTED:")
                    })
            }),
        "Unsupported native handler was present or returned an unexpected error",
    )?;
    ensure(after == before, "Unsupported command mutated native state")?;
    ctx.result(&["Guard-disabled or display-policy-disabled provider advertises no transfer capability and produces no capture","Actual production Overview disables unsupported transfer and unknown native command changes no state"])
}
