use crate::{Context, Result, ensure, fail, integer, nodes, truth, wait};
use serde_json::Value;
use std::fs;

const FIRST: &str = "HEADLESS-1";
fn ui(ctx: &Context) -> Result<Value> {
    ctx.ui()
}
fn ready(ctx: &Context) -> Result<()> {
    ctx.ipc("refresh", &[])?;
    wait("overview revision ready", || {
        let v = ui(ctx)?;
        Ok((!truth(&v["busy"]) && v["revision"] == v["state"]["revision"]).then_some(()))
    })?;
    Ok(())
}
fn point(ctx: &Context, name: &str, output: &str) -> Result<(f64, f64)> {
    let value: Value = serde_json::from_str(&ctx.ipc("geometry", &[name, output])?)?;
    ensure(
        !value.is_null() && value["enabled"] != false,
        format!("Unavailable geometry {name}: {value}"),
    )?;
    let n = |key: &str| {
        value[key]
            .as_f64()
            .ok_or_else(|| fail(format!("Missing {key}: {value}")))
    };
    let mut x = n("x")? + n("width")? * 0.5;
    let mut y = n("y")? + n("height")? * 0.5;
    if output.is_empty() {
        let tree = ctx.native("get_tree")?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        let owner = all
            .into_iter()
            .find(|node| node["name"] == "Window Overview" && node["pid"] == ctx.shell_pid)
            .ok_or("Native Window Overview absent")?;
        x += owner["rect"]["x"].as_f64().ok_or("Overview x")?;
        y += owner["rect"]["y"].as_f64().ok_or("Overview y")?;
    }
    Ok((x, y))
}
fn start(ctx: &mut Context, n: usize) -> Result<(f64, f64)> {
    ready(ctx)?;
    let id = integer(&ctx.captured[n]["id"])?;
    let p = point(ctx, &format!("group-drag-{id}"), "")?;
    ctx.move_pointer(p.0, p.1)?;
    ctx.press()?;
    ctx.move_pointer(p.0 + 14., p.1 + 2.)?;
    wait("native membership drag", || {
        Ok(truth(&ui(ctx)?["drag"]["active"]).then_some(()))
    })?;
    Ok(p)
}
fn group_id(ctx: &Context, n: usize) -> Result<Value> {
    Ok(ctx.group(n)?["id"].clone())
}
fn drag_commits(ctx: &Context, n: i64) -> Result<bool> {
    Ok(ui(ctx)?["drag"]["commits"] == n)
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let out = ctx.opt.output.clone();
    wait("group observer available", || {
        Ok(truth(&ui(ctx)?["state"]["available"]).then_some(()))
    })?;
    ctx.group_action("on")?;
    wait("groups enabled", || {
        Ok(truth(&ui(ctx)?["state"]["enabled"]).then_some(()))
    })?;
    let id = integer(&ctx.captured[1]["id"])?;
    ctx.ipc("open", &[&id.to_string()])?;
    wait("overview current revision", || {
        let v = ui(ctx)?;
        Ok(
            (truth(&v["visible"]) && !truth(&v["busy"]) && v["revision"] == v["state"]["revision"])
                .then_some(()),
        )
    })?;
    ensure(
        ui(ctx)?["error"].as_str().unwrap_or("").is_empty(),
        "Overview error",
    )?;
    ctx.pointer_start()?;
    let mut checks = Vec::new();
    let original = group_id(ctx, 1)?;
    let target = group_id(ctx, 0)?;
    let revision = ctx.groups()?["revision"].clone();
    start(ctx, 1)?;
    ensure(
        ctx.ipc("forgedOfferAccepted", &[])? == "false",
        "Forged drag offer accepted",
    )?;
    let (x, y) = point(ctx, &format!("group-drop-{target}"), "")?;
    ctx.move_pointer(x, y)?;
    ensure(
        ctx.groups()?["revision"] == revision,
        "Drag preview wrote membership",
    )?;
    ctx.key(&["-k", "Escape"])?;
    ctx.release()?;
    ensure(
        !truth(&ui(ctx)?["drag"]["active"]) && group_id(ctx, 1)? == original,
        "Escape did not preserve source group",
    )?;
    checks.push(
        "Held native membership drag previews without writes; Escape retains exact source group",
    );
    start(ctx, 1)?;
    let (x, y) = point(ctx, &format!("group-drop-{target}"), "")?;
    ctx.move_pointer(x, y)?;
    ensure(
        ui(ctx)?["drag"]["target"]["groupId"] == target && ctx.groups()?["revision"] == revision,
        "Target preview incorrect",
    )?;
    ctx.picture("membership-preview.png")?;
    ctx.release()?;
    wait("overview group add commit", || {
        Ok(drag_commits(ctx, 1)?.then_some(()))
    })?;
    ensure(
        group_id(ctx, 1)? == target,
        "Overview dropped into wrong group",
    )?;
    checks.push(
        "Valid Overview target release atomically adds captured native window to exact group",
    );
    ready(ctx)?;
    ensure(
        ctx.ipc(
            "focusControl",
            &[&format!("group-drag-{}", ctx.captured[1]["id"])],
        )? == "true",
        "Keyboard group handle absent",
    )?;
    ctx.key(&["-k", "space"])?;
    wait("keyboard membership drag", || {
        let d = ui(ctx)?["drag"].clone();
        Ok((truth(&d["active"]) && truth(&d["keyboard"])).then_some(()))
    })?;
    ensure(
        ui(ctx)?["drag"]["destinations"][0]["kind"] == "new",
        "Keyboard New Group option absent",
    )?;
    ctx.key(&["-k", "Return"])?;
    wait("keyboard detach commit", || {
        Ok(drag_commits(ctx, 2)?.then_some(()))
    })?;
    ensure(
        group_id(ctx, 1)? != target,
        "Keyboard detach kept old group",
    )?;
    checks.push(
        "Real keyboard equivalent detaches to a new group through the same captured native intent",
    );
    start(ctx, 1)?;
    ctx.move_pointer(2., 300.)?;
    wait("selector reveal during drag", || {
        Ok(ui(ctx)?["surfaces"]
            .as_array()
            .filter(|a| {
                a.iter()
                    .any(|s| s["output"] == FIRST && truth(&s["revealed"]))
            })
            .map(|_| ()))
    })?;
    let (x, y) = point(ctx, &format!("window-group-row-{target}"), FIRST)?;
    ctx.move_pointer(x, y)?;
    ensure(
        ui(ctx)?["drag"]["target"]["groupId"] == target,
        "Cross-surface target lost",
    )?;
    ctx.release()?;
    wait("selector group add commit", || {
        Ok(drag_commits(ctx, 3)?.then_some(()))
    })?;
    ensure(
        group_id(ctx, 1)? == target,
        "Selector dropped into wrong group",
    )?;
    start(ctx, 1)?;
    ctx.move_pointer(2., 300.)?;
    wait("selector new-group reveal", || {
        Ok(ui(ctx)?["surfaces"]
            .as_array()
            .filter(|a| {
                a.iter()
                    .any(|s| s["output"] == FIRST && truth(&s["revealed"]))
            })
            .map(|_| ()))
    })?;
    let (x, y) = point(ctx, "selector-group-drop-new", FIRST)?;
    ctx.move_pointer(x, y)?;
    ensure(
        ui(ctx)?["drag"]["target"]["kind"] == "new",
        "Cross-surface New Group target lost",
    )?;
    ctx.release()?;
    wait("selector group detach commit", || {
        Ok(drag_commits(ctx, 4)?.then_some(()))
    })?;
    ensure(
        group_id(ctx, 1)? != target,
        "Selector New Group kept old group",
    )?;
    checks.push("Real cross-surface held drag adds through recent selector and detaches through its New Group target");
    let original = group_id(ctx, 1)?;
    let revision = ctx.groups()?["revision"].clone();
    start(ctx, 1)?;
    ctx.move_pointer(900., 700.)?;
    ctx.release()?;
    ensure(
        ctx.groups()?["revision"] == revision && group_id(ctx, 1)? == original,
        "Outside release changed membership",
    )?;
    start(ctx, 1)?;
    let group0 = group_id(ctx, 0)?;
    let id0 = ctx.captured[0]["id"].clone();
    let pid0 = ctx.captured[0]["pid"].clone();
    ctx.group_action(&format!("remove {group0} {id0} {pid0}"))?;
    wait("independent revision cancels drag", || {
        Ok((!truth(&ui(ctx)?["drag"]["active"])).then_some(()))
    })?;
    ctx.release()?;
    ensure(
        group_id(ctx, 1)? == original && drag_commits(ctx, 4)?,
        "Independent revision retargeted source",
    )?;
    checks.push("Outside release and an independent native revision change cancel without retargeting source");
    start(ctx, 1)?;
    let source = ui(ctx)?["drag"]["source"].clone();
    ctx.sway("workspace 2", true)?;
    wait("workspace change cancels drag", || {
        Ok((!truth(&ui(ctx)?["drag"]["active"])).then_some(()))
    })?;
    ctx.release()?;
    ensure(
        group_id(ctx, 1)? == original,
        "Workspace change retargeted source",
    )?;
    ctx.sway(
        &format!(
            "lumen_window groups {} remove-scoped {original} {} \"{FIRST}\" {} {}",
            ctx.groups()?["revision"],
            source["workspace"],
            ctx.captured[1]["id"],
            ctx.captured[1]["pid"]
        ),
        false,
    )?;
    ctx.sway("workspace 1", true)?;
    ready(ctx)?;
    start(ctx, 1)?;
    let locker = ctx.opt.locker.clone();
    let index = ctx.launch(&locker, &[], "locker.log", true)?;
    wait("native lock cancels drag", || {
        let v = ui(ctx)?;
        Ok((truth(&v["state"]["locked"]) && !truth(&v["drag"]["active"])).then_some(()))
    })?;
    ctx.release()?;
    ctx.write(index, "u\n")?;
    wait("locker exit", || {
        Ok(ctx.procs[index]
            .0
            .try_wait()?
            .filter(|s| s.success())
            .map(|_| ()))
    })?;
    wait("native unlock", || {
        Ok((!truth(&ui(ctx)?["state"]["locked"])).then_some(()))
    })?;
    ensure(group_id(ctx, 1)? == original, "Lock changed membership")?;
    checks.push("Visible-workspace change and actual session lock cancel; native atomic guard rejects hidden-desktop membership");
    start(ctx, 1)?;
    ctx.sway("output HEADLESS-1 disable", true)?;
    wait("output disconnect cancels drag", || {
        Ok((!truth(&ui(ctx)?["drag"]["active"])).then_some(()))
    })?;
    ctx.release()?;
    ensure(
        group_id(ctx, 1)? == original,
        "Output disconnect changed membership",
    )?;
    for n in 0..4 {
        let now = ctx.window(n)?.ok_or("Native relationship window missing")?;
        ensure(
            now["id"] == ctx.captured[n]["id"] && now["pid"] == ctx.captured[n]["pid"],
            format!("Window {n} identity changed"),
        )?;
    }
    checks.push("Output disconnect cancels and all original native window/PID identities survive every operation");
    let log = fs::read_to_string(out.join("membership.log"))?;
    ensure(
        !log.contains(" WARN") && !log.contains(" ERROR"),
        "Membership shell emitted warnings",
    )?;
    ctx.result(&checks)?;
    println!("PASS captured group membership native drag");
    Ok(())
}
