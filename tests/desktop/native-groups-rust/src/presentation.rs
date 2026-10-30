use super::*;

fn window(ctx: &Context, n: usize) -> Result<Value> {
    ctx.window(n)?
        .ok_or_else(|| fail(format!("Presentation window {n} absent")))
}
fn focus(ctx: &Context, n: usize) -> Result<()> {
    ctx.sway(&format!("[con_id={}] focus", window(ctx, n)?["id"]), true)
}
fn masks(ctx: &Context) -> Result<Vec<bool>> {
    (0..4)
        .map(|n| Ok(truth(&window(ctx, n)?["lumen_state"]["group_hidden"])))
        .collect()
}
fn expected(ctx: &Context, wanted: &[bool]) -> Result<()> {
    wait("exact per-application scene masks", || {
        Ok((masks(ctx)? == wanted).then_some(()))
    })
}
fn mode(ctx: &Context, value: &str) -> Result<()> {
    ctx.ipc("desired", &[value])?;
    wait("live preference applied and acknowledged", || {
        let ui = ctx.ui()?;
        Ok((ctx.groups()?["window_mode"] == value
            && ui["state"]["windowMode"] == value
            && ui["worker"] == false
            && ui["error"] == "")
            .then_some(()))
    })
}
fn placements(ctx: &Context) -> Result<Value> {
    Ok(Value::Array((0..4).map(|n|{
        let w=window(ctx,n)?;
        Ok(json!({"id":w["id"],"pid":w["pid"],"app_id":w["app_id"],"rect":w["rect"],"window_rect":w["window_rect"],"floating":w["floating"],"fullscreen_mode":w["fullscreen_mode"],"minimized":w["lumen_state"]["minimized"],"hidden":w["lumen_state"]["application_hidden"]}))
    }).collect::<Result<Vec<_>>>()?))
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let mut checks = Vec::new();
    ensure(
        ctx.native("get_version")?["native_window_group_presentation"] == true,
        "Provider lacks explicit presentation capability",
    )?;
    ensure(
        ctx.groups()?["window_mode"] == "all_together" && ctx.groups()?["enabled"] == false,
        "Default presentation changed",
    )?;
    ctx.sway(
        &format!(
            "[con_id={}] move container to workspace 1",
            ctx.captured[3]["id"]
        ),
        true,
    )?;
    ctx.sway("workspace 1; focus output HEADLESS-1", true)?;
    ctx.group_action("on")?;
    let group = ctx.group(0)?["id"].clone();
    for n in [1, 3] {
        ctx.group_action(&format!(
            "add {group} {} {}",
            ctx.captured[n]["id"], ctx.captured[n]["pid"]
        ))?;
    }
    ctx.group_action(&format!("select {group}"))?;
    ensure(
        ctx.group(0)?["members"]
            .as_array()
            .is_some_and(|m| m.len() == 4),
        "Owner/transient membership lost",
    )?;
    for (n, (x, y)) in [
        (0, (20, 40)),
        (1, (400, 40)),
        (2, (20, 360)),
        (3, (800, 40)),
    ] {
        ctx.sway(
            &format!("[con_id={}] move position {x} {y}", ctx.captured[n]["id"]),
            true,
        )?;
    }
    focus(ctx, 0)?;
    focus(ctx, 1)?;
    expected(ctx, &[false, false, false, false])?;
    let geometry = placements(ctx)?;
    save(
        &ctx.opt.output.join("all-before.json"),
        &ctx.native("get_tree")?,
    )?;
    mode(ctx, "one_at_a_time")?;
    expected(ctx, &[true, false, true, false])?;
    ensure(
        placements(ctx)? == geometry,
        "Mode changed native placement or independent hide reasons",
    )?;
    ctx.picture("one-second")?;
    checks.push("Default all mode preserves all members; live one mode selects one root per native application, not per PID, and owner dialogs follow that root without geometry mutation");
    focus(ctx, 0)?;
    expected(ctx, &[false, true, false, false])?;
    mode(ctx, "all_together")?;
    focus(ctx, 1)?;
    mode(ctx, "one_at_a_time")?;
    expected(ctx, &[true, false, true, false])?;
    ctx.group_action("off")?;
    focus(ctx, 0)?;
    ctx.group_action("on")?;
    let fresh = ctx.group(0)?["id"].clone();
    for n in [1, 3] {
        ctx.group_action(&format!(
            "add {fresh} {} {}",
            ctx.captured[n]["id"], ctx.captured[n]["pid"]
        ))?;
    }
    focus(ctx, 0)?;
    expected(ctx, &[false, true, false, false])?;
    checks.push("Actual native focus cycles same-application roots; focus observed in all and disabled modes remains available when presentation is enabled");
    let before = ctx.groups()?;
    ctx.sway(
        &format!(
            "lumen_window groups {} mode all_together",
            integer(&before["revision"])? - 1
        ),
        false,
    )?;
    ensure(
        ctx.groups()? == before,
        "Stale mode revision mutated groups",
    )?;
    ctx.sway(
        &format!("lumen_window groups {} mode invalid", before["revision"]),
        false,
    )?;
    ensure(ctx.groups()? == before, "Invalid mode mutated groups")?;
    let locker = ctx.opt.locker.clone();
    let li = ctx.launch(&locker, &[], "presentation-lock.log", true)?;
    wait("actual lock", || {
        Ok(truth(&ctx.groups()?["locked"]).then_some(()))
    })?;
    let locked = ctx.groups()?;
    ctx.sway(
        &format!(
            "lumen_window groups {} mode all_together",
            locked["revision"]
        ),
        false,
    )?;
    ensure(ctx.groups()? == locked, "Locked command mutated groups")?;
    ctx.ipc("desired", &["all_together"])?;
    wait("locked observer receipt", || {
        Ok((ctx.ui()?["state"]["locked"] == true).then_some(()))
    })?;
    ensure(
        ctx.groups()?["window_mode"] == "one_at_a_time",
        "Live setting bypassed lock",
    )?;
    ctx.write(li, "u\n")?;
    wait("lock client reaped", || {
        Ok(ctx.procs[li].0.try_wait()?.map(|_| ()))
    })?;
    mode(ctx, "all_together")?;
    expected(ctx, &[false, false, false, false])?;
    checks.push("Stale revision and malformed mode reject without state changes; actual session lock blocks both native and live preference dispatch, and latest desired mode applies after unlock");
    ctx.ipc("open", &[])?;
    wait("actual Settings mode control", || {
        Ok((ctx.ui()?["settings"]["mode"]["enabled"] == true
            && ctx.ui()?["settings"]["mode"]["focus"] == true)
            .then_some(()))
    })?;
    ctx.key(&["-k", "space", "-k", "Down", "-k", "Return"])?;
    wait("physical Settings selection", || {
        let ui = ctx.ui()?;
        Ok(
            (ui["edits"] == json!([{"key":"groupWindowMode","value":"one_at_a_time"}])
                && ctx.groups()?["window_mode"] == "one_at_a_time")
                .then_some(()),
        )
    })?;
    ctx.ipc("close", &[])?;
    ctx.ipc("open", &[])?;
    wait("retained Settings selection", || {
        Ok((ctx.ui()?["settings"]["mode"]["index"] == 1).then_some(()))
    })?;
    ctx.ipc("close", &[])?;
    checks.push("Actual accessible Settings combo receives physical keyboard selection, dispatches only canonical preference enum, and retains the selected value on reopening");
    save(
        &ctx.opt.output.join("final-native.json"),
        &ctx.native("get_tree")?,
    )?;
    ctx.result(&checks)
}
