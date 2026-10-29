use crate::{Context, Result, ensure, fail, integer, truth, wait};
use serde_json::Value;

const FIRST: &str = "HEADLESS-1";
const SECOND: &str = "HEADLESS-2";
fn surface(ctx: &Context, output: &str) -> Result<Value> {
    ctx.surface(output)?
        .ok_or_else(|| fail(format!("Missing selector surface {output}")))
}
fn point(ctx: &Context, output: &str, name: &str) -> Result<(f64, f64)> {
    let value: Value = serde_json::from_str(&ctx.ipc("geometry", &[output, name])?)?;
    let n = |key: &str| {
        value[key]
            .as_f64()
            .ok_or_else(|| fail(format!("Missing {key} in {value}")))
    };
    Ok((n("x")? + n("width")? * 0.5, n("y")? + n("height")? * 0.5))
}
fn shortcut(ctx: &Context) -> Result<()> {
    ctx.key(&[
        "-M", "logo", "-M", "ctrl", "-k", "g", "-m", "ctrl", "-m", "logo",
    ])
}
fn group_selected(ctx: &Context, id: &Value) -> Result<bool> {
    Ok(ctx.groups()?["groups"]
        .as_array()
        .ok_or("Groups missing")?
        .iter()
        .find(|group| group["id"] == *id)
        .is_some_and(|group| truth(&group["selected"])))
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let out = ctx.opt.output.clone();
    wait("selector state and two outputs", || {
        let v = ctx.ui()?;
        Ok((truth(&v["state"]["available"])
            && v["surfaces"].as_array().is_some_and(|a| a.len() == 2))
        .then_some(()))
    })?;
    ensure(
        !truth(&ctx.ui()?["state"]["enabled"])
            && ctx.ui()?["surfaces"]
                .as_array()
                .ok_or("Surfaces")?
                .iter()
                .all(|s| !truth(&s["visible"])),
        "Selector was visible while mode disabled",
    )?;
    ctx.group_action("on")?;
    wait("selectors concealed on both outputs", || {
        Ok(ctx.ui()?["surfaces"]
            .as_array()
            .filter(|a| {
                a.iter()
                    .all(|s| truth(&s["visible"]) && !truth(&s["revealed"]))
            })
            .map(|_| ()))
    })?;
    let initial = ctx.groups()?["revision"].clone();
    let initial_h2 = ctx.group(3)?["id"].clone();
    ctx.pointer_start()?;
    ctx.move_pointer(2., 300.)?;
    wait("first selector edge reveal", || {
        Ok(truth(&surface(ctx, FIRST)?["revealed"]).then_some(()))
    })?;
    ensure(
        !truth(&surface(ctx, FIRST)?["keyboard"]) && ctx.groups()?["revision"] == initial,
        "Pointer reveal wrote membership",
    )?;
    let first_surface = surface(ctx, FIRST)?;
    let workspace = first_surface["capture"]["workspace"].clone();
    let expected: Vec<Value> = ctx.groups()?["groups"]
        .as_array()
        .ok_or("Groups")?
        .iter()
        .filter(|g| g["workspace"] == workspace)
        .map(|g| g["id"].clone())
        .collect();
    let rows: Vec<Value> = first_surface["rows"]
        .as_array()
        .ok_or("Rows")?
        .iter()
        .map(|r| r["id"].clone())
        .collect();
    ensure(
        expected.len() == rows.len()
            && expected.iter().all(|id| rows.contains(id))
            && !expected.contains(&initial_h2),
        "Visible desktop selector scope incorrect",
    )?;
    ctx.picture("recent-groups.png")?;
    ctx.move_pointer(700., 700.)?;
    wait("selector leave", || {
        Ok((!truth(&surface(ctx, FIRST)?["revealed"])).then_some(()))
    })?;
    ensure(
        ctx.groups()?["revision"] == initial,
        "Pointer leave wrote membership",
    )?;
    let mut checks = vec![
        "Mode off has no selector; enabled mode starts concealed; pointer edge reveal/leave changes no native membership and is output/visible-desktop scoped",
    ];
    ctx.sway(&format!("[con_id={}] focus", ctx.captured[0]["id"]), true)?;
    wait("focused first output", || {
        Ok((ctx.ui()?["state"]["focusedOutput"] == FIRST).then_some(()))
    })?;
    shortcut(ctx)?;
    wait("keyboard selector", || {
        Ok(truth(&surface(ctx, FIRST)?["keyboard"]).then_some(()))
    })?;
    ctx.key(&["-k", "Down"])?;
    ctx.key(&["-k", "Escape"])?;
    wait("Escape selector cancel", || {
        Ok((!truth(&surface(ctx, FIRST)?["revealed"])).then_some(()))
    })?;
    ensure(
        ctx.groups()?["revision"] == initial,
        "Escape changed membership",
    )?;
    shortcut(ctx)?;
    wait("second keyboard selector", || {
        Ok(truth(&surface(ctx, FIRST)?["keyboard"]).then_some(()))
    })?;
    ctx.move_pointer(700., 700.)?;
    ctx.press()?;
    ctx.release()?;
    wait("outside selector click", || {
        Ok((!truth(&surface(ctx, FIRST)?["revealed"])).then_some(()))
    })?;
    ensure(
        ctx.groups()?["revision"] == initial,
        "Outside click changed membership",
    )?;
    shortcut(ctx)?;
    wait("third keyboard selector", || {
        Ok(truth(&surface(ctx, FIRST)?["keyboard"]).then_some(()))
    })?;
    ctx.key(&["-k", "Down"])?;
    let v = surface(ctx, FIRST)?;
    let target = v["rows"][integer(&v["selected"])? as usize]["id"].clone();
    ctx.key(&["-k", "Return"])?;
    wait("keyboard group select", || {
        Ok((surface(ctx, FIRST)?["commits"] == 1).then_some(()))
    })?;
    ensure(
        group_selected(ctx, &target)? && group_selected(ctx, &initial_h2)?,
        "Keyboard group selection crossed outputs",
    )?;
    checks.push("Real configured keyboard shortcut previews, Escape cancels and Return selects an exact group without touching another output");
    ctx.move_pointer(2., 300.)?;
    wait("pointer selector", || {
        Ok(truth(&surface(ctx, FIRST)?["revealed"]).then_some(()))
    })?;
    let target = surface(ctx, FIRST)?["rows"]
        .as_array()
        .ok_or("Rows")?
        .iter()
        .find(|r| !truth(&r["selected"]))
        .ok_or("No unselected group")?["id"]
        .clone();
    let before = ctx.groups()?["revision"].clone();
    let (x, y) = point(ctx, FIRST, &format!("window-group-row-{target}"))?;
    ctx.move_pointer(x, y)?;
    ctx.press()?;
    ctx.key(&["-k", "Escape"])?;
    ctx.release()?;
    ensure(
        ctx.groups()?["revision"] == before,
        "Held row Escape changed membership",
    )?;
    ctx.move_pointer(700., 700.)?;
    ctx.move_pointer(2., 300.)?;
    wait("selector re-reveal", || {
        Ok(truth(&surface(ctx, FIRST)?["revealed"]).then_some(()))
    })?;
    let (x, y) = point(ctx, FIRST, &format!("window-group-row-{target}"))?;
    ctx.move_pointer(x, y)?;
    ctx.press()?;
    ensure(
        ctx.groups()?["revision"] == before,
        "Press committed before release",
    )?;
    ctx.release()?;
    wait("pointer group select", || {
        Ok((surface(ctx, FIRST)?["commits"] == 2).then_some(()))
    })?;
    ensure(
        group_selected(ctx, &target)?,
        "Pointer selected wrong group",
    )?;
    checks.push("Real held row press/Escape cancels; pointer release commits captured group only");
    ctx.move_pointer(700., 700.)?;
    ctx.move_pointer(2., 300.)?;
    wait("scope selector reveal", || {
        Ok(truth(&surface(ctx, FIRST)?["revealed"]).then_some(()))
    })?;
    let old = surface(ctx, FIRST)?["capture"].clone();
    let gid = surface(ctx, FIRST)?["rows"][0]["id"].clone();
    ctx.sway("workspace 2", true)?;
    wait("workspace change closes selector", || {
        Ok((!truth(&surface(ctx, FIRST)?["visible"])).then_some(()))
    })?;
    ctx.sway(
        &format!(
            "lumen_window groups {} select-scoped {gid} {} \"{FIRST}\"",
            ctx.groups()?["revision"],
            old["workspace"]
        ),
        false,
    )?;
    let workspaces = ctx.native("get_workspaces")?;
    ensure(
        workspaces
            .as_array()
            .ok_or("Workspaces")?
            .iter()
            .find(|w| truth(&w["focused"]))
            .is_some_and(|w| w["name"] == "2"),
        "Rejected scope changed workspace",
    )?;
    ctx.sway("workspace 1", true)?;
    wait("first selector restored", || {
        Ok(truth(&surface(ctx, FIRST)?["visible"]).then_some(()))
    })?;
    ctx.ipc("begin", &[SECOND])?;
    wait("second output keyboard selector", || {
        Ok(truth(&surface(ctx, SECOND)?["keyboard"]).then_some(()))
    })?;
    ensure(
        surface(ctx, SECOND)?["rows"]
            .as_array()
            .ok_or("Second rows")?
            .iter()
            .all(|r| r["id"] == initial_h2),
        "Second output leaked first output groups",
    )?;
    ctx.sway("output HEADLESS-2 disable", true)?;
    wait("removed output selector", || {
        Ok(ctx.ui()?["surfaces"]
            .as_array()
            .filter(|a| a.iter().all(|s| s["output"] != SECOND))
            .map(|_| ()))
    })?;
    checks.push("Visible-workspace change cancels and native scope guard rejects stale cross-desktop activation; output disconnect removes its keyboard surface");
    ctx.ipc("begin", &[FIRST])?;
    wait("first keyboard selector before lock", || {
        Ok(truth(&surface(ctx, FIRST)?["keyboard"]).then_some(()))
    })?;
    let locker = ctx.opt.locker.clone();
    let index = ctx.launch(&locker, &[], "locker.log", true)?;
    wait("native lock hides selector", || {
        let v = ctx.ui()?;
        Ok(
            (truth(&v["state"]["locked"]) && !truth(&surface(ctx, FIRST)?["visible"]))
                .then_some(()),
        )
    })?;
    ensure(
        !truth(&surface(ctx, FIRST)?["revealed"]),
        "Locked selector revealed",
    )?;
    ctx.write(index, "u\n")?;
    wait("locker exit", || {
        Ok(ctx.procs[index]
            .0
            .try_wait()?
            .filter(|s| s.success())
            .map(|_| ()))
    })?;
    wait("unlock restores concealed selector", || {
        let s = surface(ctx, FIRST)?;
        Ok((truth(&s["visible"]) && !truth(&s["revealed"])).then_some(()))
    })?;
    ctx.ipc("begin", &[FIRST])?;
    wait("selector before observer loss", || {
        Ok(truth(&surface(ctx, FIRST)?["keyboard"]).then_some(()))
    })?;
    ctx.ipc("stopObserver", &[])?;
    wait("observer loss hides selector", || {
        let v = ctx.ui()?;
        Ok(
            (!truth(&v["state"]["available"]) && !truth(&surface(ctx, FIRST)?["visible"]))
                .then_some(()),
        )
    })?;
    ensure(
        !truth(&surface(ctx, FIRST)?["revealed"]),
        "Observer loss left selector revealed",
    )?;
    checks.push("Actual native lock and observer disconnect close panels and release keyboard; unlock remains concealed");
    let log = std::fs::read_to_string(out.join("selector.log"))?;
    ensure(
        !log.contains(" WARN") && !log.contains(" ERROR"),
        "Selector emitted warnings",
    )?;
    ctx.result(&checks)?;
    println!("PASS recent-group selector native UI");
    Ok(())
}
