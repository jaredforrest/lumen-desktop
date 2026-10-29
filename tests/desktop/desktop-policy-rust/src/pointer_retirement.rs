use super::*;

fn ipc(ctx: &Context, method: &str) -> Result<String> {
    ctx.call("quickshell", &["-p", &path(&ctx.qml), "ipc", "call", "fixture", method])
}
fn state(ctx: &Context) -> Result<Value> { Ok(serde_json::from_str(&ipc(ctx, "status")?)?) }
fn native(ctx: &Context, name: &str, pid: u32) -> Result<Option<Value>> {
    let title = format!("Pointer {name}");
    Ok(find_node(&ctx.tree()?, |node| node["pid"] == pid && node["name"] == title))
}
fn visible(ctx: &Context, name: &str, pid: u32) -> Result<Value> {
    wait(&format!("native Pointer {name}"), || native(ctx, name, pid))
}
fn hidden(ctx: &Context, name: &str, pid: u32) -> Result<()> {
    wait(&format!("unmapped Pointer {name}"), || Ok(native(ctx, name, pid)?.is_none().then_some(())))
}
fn button(ctx: &mut Context, code: u32, pressed: bool) -> Result<()> {
    ctx.pointer_command(&format!("b {code} {}", u8::from(pressed)))
}
fn hover(ctx: &mut Context, name: &str, pid: u32) -> Result<()> {
    let node = visible(ctx, name, pid)?;
    ctx.checked(&format!("[con_id={}] focus", node["id"]))?;
    let x = number(&node["rect"]["x"])? + number(&node["window_rect"]["x"])? + 150;
    let y = number(&node["rect"]["y"])? + number(&node["window_rect"]["y"])? + 100;
    ctx.checked(&format!("seat seat0 cursor set {} {}", x - 24, y))?;
    ctx.pointer_command("m 24 0")?;
    wait("fresh pointer hover", || Ok((state(ctx)?[name]["hovered"] == true).then_some(())))
}
fn buttons(ctx: &Context, name: &str, wanted: i64) -> Result<()> {
    wait("exact held button state", || Ok((state(ctx)?[name]["buttons"] == wanted).then_some(())))
}
pub fn run(ctx: &mut Context) -> Result<()> {
    ctx.checked("output HEADLESS-1 resolution 1000x700; output HEADLESS-2 disable")?;
    ctx.pointer_start()?;
    let shell = ctx.launch("quickshell", &["-p", &path(&ctx.qml)], "pointer-shell.log", false)?;
    let pid = ctx.children[shell].0.id();
    for (name, x) in [("A", 60), ("B", 550)] {
        let node = visible(ctx, name, pid)?;
        ctx.checked(&format!("[con_id={}] move absolute position {x} px 160 px", node["id"]))?;
    }
    let mut checks = Vec::new();
    hover(ctx, "A", pid)?;
    button(ctx, 272, true)?;
    button(ctx, 273, true)?;
    buttons(ctx, "A", 3)?;
    button(ctx, 272, false)?;
    buttons(ctx, "A", 2)?;
    ipc(ctx, "hideA")?;
    hidden(ctx, "A", pid)?;
    button(ctx, 273, false)?;
    hover(ctx, "B", pid)?;
    ipc(ctx, "showA")?;
    visible(ctx, "A", pid)?;
    hover(ctx, "A", pid)?;
    button(ctx, 272, true)?;
    buttons(ctx, "A", 1)?;
    ipc(ctx, "hideA")?;
    hidden(ctx, "A", pid)?;
    ipc(ctx, "showA")?;
    visible(ctx, "A", pid)?;
    button(ctx, 272, false)?;
    hover(ctx, "A", pid)?;
    hover(ctx, "B", pid)?;
    checks.push("Unmapping an exact surface clears remaining held buttons and permits fresh hover after reopening");

    let before = state(ctx)?["B"].clone();
    button(ctx, 272, true)?;
    buttons(ctx, "B", 1)?;
    ctx.pointer_command("m 30 0")?;
    wait("held pointer motion", || Ok((state(ctx)?["B"]["moves"].as_i64().unwrap_or(0) >
        before["moves"].as_i64().unwrap_or(0)).then_some(())))?;
    button(ctx, 272, false)?;
    wait("ordinary physical release", || {
        let b = state(ctx)?["B"].clone();
        Ok((b["buttons"] == 0 && b["releases"].as_i64().unwrap_or(0) > before["releases"].as_i64().unwrap_or(0)).then_some(()))
    })?;
    button(ctx, 272, true)?;
    buttons(ctx, "B", 1)?;
    let a = visible(ctx, "A", pid)?;
    ctx.checked(&format!("[con_id={}] focus", a["id"]))?;
    need(native(ctx, "B", pid)?.is_some(), "Ordinary focus transfer unmapped B")?;
    button(ctx, 272, false)?;
    hover(ctx, "B", pid)?;
    checks.push("Ordinary held motion and release survive focus transfer while both surfaces remain mapped");

    hover(ctx, "A", pid)?;
    button(ctx, 272, true)?;
    buttons(ctx, "A", 1)?;
    button(ctx, 272, false)?;
    buttons(ctx, "A", 0)?;
    hover(ctx, "B", pid)?;
    button(ctx, 272, true)?;
    buttons(ctx, "B", 1)?;
    let previous_moves = state(ctx)?["B"]["moves"].as_i64().ok_or("No move counter")?;
    ipc(ctx, "hideA")?;
    hidden(ctx, "A", pid)?;
    ctx.pointer_command("m 20 0")?;
    wait("new target remains held after old surface retirement", || {
        let b = state(ctx)?["B"].clone();
        Ok((b["buttons"] == 1 && b["moves"].as_i64().unwrap_or(0) > previous_moves).then_some(()))
    })?;
    button(ctx, 272, false)?;
    buttons(ctx, "B", 0)?;
    checks.push("Retiring an old guard cannot clear a later distinct target press");

    button(ctx, 272, true)?;
    buttons(ctx, "B", 1)?;
    let pointer = ctx.pointer.ok_or("Pointer process missing")?;
    ctx.children[pointer].stop(libc::SIGTERM)?;
    ipc(ctx, "hideB")?;
    hidden(ctx, "B", pid)?;
    need(ctx.children[shell].0.try_wait()?.is_none(), "Shell crashed after pointer teardown")?;
    state(ctx)?;
    checks.push("Pointer teardown before surface retirement leaves the shell responsive");
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":checks,
        "scope":"Real private Wayland pointer button, hover, surface retirement and target ownership"}))?;
    Ok(())
}
