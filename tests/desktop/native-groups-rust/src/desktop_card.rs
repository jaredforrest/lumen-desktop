//! Actual pointer gestures on ordinary Overview card content, not its handles.
use super::desktop_transfer::{frame, owner, point};
use super::*;
static NEXT_ATTEMPT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(super) fn begin(ctx: &mut Context, id: &Value) -> Result<()> {
    ctx.ipc("reveal", &[])?;
    let name = format!("desktop-window-card-drag-{id}");
    let p = point(ctx, &name)?;
    let before = ctx.ui()?;
    ensure(
        before["visible"] == true && before["drag"]["active"] == false,
        "Card gesture requires idle visible Overview",
    )?;
    let serial = before["drag"]["commits"]
        .as_u64()
        .ok_or("Card commit count")?;
    let attempt = NEXT_ATTEMPT
        .fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |v| v.checked_add(1),
        )
        .map_err(|_| fail("Card attempt count exhausted"))?;
    save(
        &ctx.opt
            .output
            .join(format!("card-{id}-{serial}-{attempt}-begin-before.json")),
        &json!({"ui":before,"source":frame(ctx,id)?,"point":[p.0,p.1]}),
    )?;
    ctx.move_pointer(p.0, p.1)?;
    ctx.press()?;
    ctx.move_pointer(p.0 + 20., p.1 + 2.)?;
    wait("actual native offer from ordinary card content", || {
        let ui = ctx.ui()?;
        Ok(
            (ui["visible"] == true && ui["drag"]["active"] == true && ui["drag"]["busy"] == false)
                .then_some(()),
        )
    })?;
    save(
        &ctx.opt
            .output
            .join(format!("card-{id}-{serial}-{attempt}-begin-active.json")),
        &ctx.ui()?,
    )?;
    Ok(())
}
pub(super) fn outside_cancel(ctx: &mut Context, id: &Value) -> Result<()> {
    let before = frame(ctx, id)?;
    let ui = ctx.ui()?;
    let commits = ui["drag"]["commits"].clone();
    begin(ctx, id)?;
    let o = owner(ctx)?;
    let left = o["rect"]["x"].as_f64().ok_or("Owner x")?;
    let top = o["rect"]["y"].as_f64().ok_or("Owner y")?;
    let width = o["rect"]["width"].as_f64().ok_or("Owner width")?;
    let height = o["rect"]["height"].as_f64().ok_or("Owner height")?;
    let x = if left >= 30. {
        left - 20.
    } else {
        left + width + 20.
    };
    let y = top + height * 0.5;
    ensure(
        x < left || x > left + width,
        "Cancel release must be outside Overview",
    )?;
    ctx.move_pointer(x, y)?;
    ctx.release()?;
    wait("outside card drop retires without transfer", || {
        let ui = ctx.ui()?;
        Ok((ui["drag"]["active"] == false && ui["drag"]["busy"] == false).then_some(()))
    })?;
    let after = frame(ctx, id)?;
    let final_ui = ctx.ui()?;
    save(
        &ctx.opt.output.join("card-outside-cancel.json"),
        &json!({"before":before,"after":after,"uiBefore":ui,"uiAfter":final_ui,"release":[x,y]}),
    )?;
    ensure(
        before == after
            && final_ui["drag"]["commits"] == commits
            && final_ui["visible"] == true
            && final_ui["overviewActive"] == true,
        "Outside card release moved/activated a window or committed a transfer",
    )
}
pub(super) fn click_activate(ctx: &mut Context, id: &Value) -> Result<()> {
    ctx.ipc("reveal", &[])?;
    let before = ctx.ui()?;
    ensure(
        before["drag"]["active"] == false && before["visible"] == true,
        "Ordinary card click requires idle visible Overview",
    )?;
    let p = point(ctx, &format!("desktop-window-card-drag-{id}"))?;
    let original = frame(ctx, id)?;
    ctx.move_pointer(p.0, p.1)?;
    ctx.press()?;
    ctx.release()?;
    wait(
        "ordinary card click activates its exact native window",
        || {
            let ui = ctx.ui()?;
            let tree = ctx.native("get_tree")?;
            let mut all = Vec::new();
            nodes(&tree, &mut all);
            Ok((ui["visible"] == false
                && all
                    .iter()
                    .any(|node| node["id"] == *id && node["focused"] == true))
            .then_some(()))
        },
    )?;
    let after = ctx.ui()?;
    save(
        &ctx.opt.output.join("card-click-activation.json"),
        &json!({"uiBefore":before,"uiAfter":after,"sourceBefore":original,"sourceAfter":frame(ctx,id)?,"nativeAfter":ctx.native("get_tree")?}),
    )?;
    ensure(
        after["drag"]["commits"] == before["drag"]["commits"],
        "Ordinary card click committed a desktop transfer",
    )
}
pub(super) fn escape_cancel(ctx: &mut Context, id: &Value) -> Result<()> {
    let before = frame(ctx, id)?;
    let ui = ctx.ui()?;
    let commits = ui["drag"]["commits"].clone();
    begin(ctx, id)?;
    ctx.key(&["-k", "Escape"])?;
    wait("Escape cancels the native card offer", || {
        let ui = ctx.ui()?;
        Ok((ui["drag"]["active"] == false && ui["drag"]["busy"] == false).then_some(()))
    })?;
    ctx.release()?;
    let after = frame(ctx, id)?;
    let final_ui = ctx.ui()?;
    save(
        &ctx.opt.output.join("card-escape-cancel.json"),
        &json!({"before":before,"after":after,"uiBefore":ui,"uiAfter":final_ui}),
    )?;
    ensure(
        before == after && final_ui["visible"] == true && final_ui["drag"]["commits"] == commits,
        "Card Escape moved/activated a window or committed a transfer",
    )
}
pub(super) fn disabled_press_cancel(ctx: &mut Context, id: &Value) -> Result<()> {
    ctx.ipc("reveal", &[])?;
    let before = frame(ctx, id)?;
    let ui = ctx.ui()?;
    let p = point(ctx, &format!("desktop-window-card-drag-{id}"))?;
    ctx.move_pointer(p.0, p.1)?;
    ctx.press()?;
    wait("actual pressed captured card before owner refresh", || {
        let state: Value = serde_json::from_str(&ctx.ipc("cardState", &[&id.to_string()])?)?;
        save(&ctx.opt.output.join("card-before-disabled-refresh.json"), &state)?;
        Ok((state["pressed"] == true && state["captured"] == true).then_some(()))
    })?;
    let response: Value = serde_json::from_str(&ctx.ipc("refreshCard", &[&id.to_string()])?)?;
    save(&ctx.opt.output.join("card-disabled-refresh-response.json"), &response)?;
    ensure(
        response["before"]["pressed"] == true
            && response["before"]["captured"] == true,
        "Owner refresh did not begin with an actual pressed captured card",
    )?;
    wait("owner refresh disables and retires actual pressed card", || {
        let state: Value = serde_json::from_str(&ctx.ipc("cardState", &[&id.to_string()])?)?;
        save(&ctx.opt.output.join("card-observed-disabled-refresh.json"), &state)?;
        Ok((state["enabled"] == false && state["captured"] == false).then_some(()))
    })?;
    wait(
        "Owner refresh finishes after disabling pressed card",
        || {
            let ui = ctx.ui()?;
            Ok((ui["busy"] == false).then_some(()))
        },
    )?;
    ctx.move_pointer(p.0 + 20., p.1 + 2.)?;
    ctx.release()?;
    let after = frame(ctx, id)?;
    let final_ui = ctx.ui()?;
    save(
        &ctx.opt.output.join("card-disabled-press-cancel.json"),
        &json!({"before":before,"after":after,"uiBefore":ui,"uiAfter":final_ui,"actualDisabledCard":response}),
    )?;
    ensure(
        before == after
            && final_ui["visible"] == true
            && final_ui["drag"]["active"] == false
            && final_ui["drag"]["commits"] == ui["drag"]["commits"],
        "Disabled pressed card replayed a drag/click after refresh",
    )
}
