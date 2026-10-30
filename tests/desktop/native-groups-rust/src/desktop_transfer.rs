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
fn location(ctx: &Context, id: &Value) -> Result<Value> {
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
fn stable(ctx: &Context) -> Result<Value> {
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
fn command(ctx: &Context, wire: &[u8], accepted: bool, label: &str) -> Result<()> {
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
fn capture(ctx: &Context, id: &Value, name: &str) -> Result<(Value, Value, Value, Vec<u8>)> {
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
fn apply_role_refusal(
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
        ctx.native("get_version")?["space_transfer_guard"] == 2,
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
    checks.push("Physical production Overview handle→desktop tile native drop commits once, preserves exact document/frame, and retains Overview focus without activating destination");
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
    let (_, source, dest, wire) = capture(ctx, &id, "1")?;
    let client_pid = ctx.captured[1]["pid"].as_u64().ok_or("Owned process PID")?;
    let client = ctx
        .procs
        .iter()
        .position(|p| u64::from(p.0.id()) == client_pid)
        .ok_or("Owned native client absent")?;
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
    let (_, source, dest, wire) = capture(ctx, &id, "1")?;
    ctx.group_action("on")?;
    apply_role_refusal(ctx, &source, &dest, "grouped-fresh-digest")?;
    ctx.group_action("off")?;
    ctx.sway(&format!("[con_id={id}] lumen_window minimize"), true)?;
    command(ctx, &wire, false, "minimized-source")?;
    ctx.sway(&format!("[con_id={id}] scratchpad show"), true)?;
    wait("minimized source restored", || {
        Ok((native_window(ctx, 1)?["lumen_state"]["minimised"] == false
            && location(ctx, &id)?["name"] == "2")
            .then_some(()))
    })?;
    checks.push("Actual source resize, newly attached modal, enabled grouping and minimization refuse; fresh-digest role forgeries cannot bypass native family/group preflight");
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
    wait("Overview owns native focus before Files card gesture", || {
        Ok((owner(ctx)?["focused"] == true && ctx.ui()?["overviewActive"] == true)
            .then_some(()))
    })?;
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
    ensure(
        ctx.native("get_version")?["space_transfer_guard"] != 2,
        "Legacy provider falsely advertised captured transfer",
    )?;
    ensure(
        transfer::snapshot(&ctx.native("get_tree")?).is_err(),
        "Legacy tree produced supported capture",
    )?;
    ctx.ipc("open", &[&ctx.captured[1]["id"].to_string()])?;
    wait("actual legacy Overview response", || {
        Ok((ctx.ui()?["visible"] == true && ctx.ui()?["busy"] == false).then_some(()))
    })?;
    ensure(
        ctx.ui()?["snapshot"].is_null()
            && ctx.ui()?["windows"]
                .as_array()
                .ok_or("Legacy rows")?
                .iter()
                .all(|w| w["transfer"].is_null()),
        "Legacy UI offered unsupported native window transfer",
    )?;
    let before = stable(ctx)?;
    ctx.sway("lumen_space_transfer 00", false)?;
    ensure(
        stable(ctx)? == before,
        "Unsupported command mutated native state",
    )?;
    ctx.result(&["Guard-disabled or display-policy-disabled provider advertises no transfer capability and produces no capture","Actual production Overview disables unsupported transfer and unknown native command changes no state"])
}
