use super::*;

fn native(ctx: &Context, id: &Value) -> Result<Option<Value>> { ctx.find(id) }
fn named(ctx: &Context, title: &str) -> Result<Option<Value>> {
    Ok(find_node(&ctx.tree()?, |node| node["name"] == title && node["rect"]["width"].as_i64().unwrap_or(0) > 0))
}
fn settled(ctx: &Context, id: &Value) -> Result<Value> {
    wait("settled native title frame", || {
        let node = native(ctx, id)?;
        Ok(node.filter(|node| node["deco_rect"]["height"].as_i64().unwrap_or(0) >= 32
            && node["rect"]["width"].as_i64().unwrap_or(0) > 100))
    })
}
fn click(ctx: &mut Context, id: &Value, index: i64) -> Result<()> {
    ctx.checked(&format!("[con_id={id}] focus"))?;
    wait("focused native control owner", || Ok(native(ctx, id)?.filter(|node| node["focused"] == true)))?;
    let node = settled(ctx, id)?;
    let x = number(&node["rect"]["x"])? + 20 + index * 20;
    let y = number(&node["rect"]["y"])? - number(&node["deco_rect"]["height"])? + 16;
    ctx.checked(&format!("seat seat0 cursor set {x} {y}"))?;
    ctx.pointer_command("m 0 0")?;
    ctx.press()?;
    ctx.release()
}
fn rejected(ctx: &Context, id: &Value, action: &str) -> Result<()> {
    let result = Command::new(&ctx.swaymsg).args(["-r", &format!("[con_id={id}] lumen_window {action}")])
        .env_clear().envs(&ctx.vars).output()?;
    let reply: Value = serde_json::from_slice(&result.stdout)?;
    need(result.status.code() == Some(2) && reply.as_array().is_some_and(|rows|
        !rows.is_empty() && rows.iter().all(|row| row["success"] == false)),
        format!("Unavailable {action} unexpectedly accepted: {reply}"))
}
fn open(ctx: &mut Context, title: &str) -> Result<(usize, Value)> {
    let qml = ctx.root.0.join(format!("{}.qml", title.replace(' ', "-")));
    fs::write(&qml, include_str!("../window_controls_client.qml"))?;
    ctx.vars.insert("WINDOW_CONTROLS_TITLE".into(), title.into());
    let index = ctx.launch("quickshell", &["-p", &path(&qml)], &format!("{}.log", title.replace(' ', "-")), false)?;
    let node = wait(&format!("{title} native client"), || named(ctx, title))?;
    Ok((index, node))
}
fn dock_state(ctx: &Context, qml: &Path) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.call("quickshell", &["-p", &path(qml), "ipc", "call", "windowContracts", "status"])?)?)
}
fn dock_open(ctx: &Context, qml: &Path) -> Result<()> {
    ctx.call("quickshell", &["-p", &path(qml), "ipc", "call", "windowContracts", "open"])?;
    Ok(())
}
pub fn run(ctx: &mut Context) -> Result<()> {
    ctx.checked("output HEADLESS-2 disable; output HEADLESS-1 resolution 1280x900; focus output HEADLESS-1")?;
    ctx.checked("default_floating_border normal 1; for_window [all] border normal 1")?;
    ctx.pointer_start()?;
    ctx.pointer_command("m 0 0")?;
    ctx.launch("wtype", &["-s", "120000"], "keyboard.log", false)?;
    wait("virtual seat pointer capability", || {
        let seats = ctx.sway(&["-t", "get_seats"])?;
        Ok(seats.as_array().and_then(|rows| rows.iter().find(|seat|
            seat["capabilities"].as_i64().is_some_and(|value| value & 1 != 0))).cloned())
    })?;
    let client = path(ctx.opt.client.as_ref().ok_or("Missing constraint client")?);
    let fixed_index = ctx.launch(&client, &["fixed"], "fixed.log", true)?;
    let fixed = wait("fixed-size constrained client", || named(ctx, "Constraint fixture"))?;
    let id = &fixed["id"];
    let original = settled(ctx, id)?["rect"].clone();
    click(ctx, id, 2)?;
    ctx.key(&["-k", "Escape"])?;
    let after = native(ctx, id)?.ok_or("Fixed client disappeared")?;
    need(after["rect"] == original && after["fullscreen_mode"].as_i64().unwrap_or(0) == 0,
        "Disabled fixed-size green control changed geometry")?;
    ctx.picture("fixed-disabled.png")?;
    for action in ["fullscreen","fill","zoom","left","right","top","bottom","top-left","top-right","bottom-left","bottom-right"] {
        rejected(ctx, id, action)?;
        let after = native(ctx, id)?.ok_or("Fixed client disappeared")?;
        need(after["rect"] == original && after["fullscreen_mode"].as_i64().unwrap_or(0) == 0,
            format!("Unavailable {action} changed fixed-size client"))?;
    }
    click(ctx, id, 1)?;
    wait("fixed-size native minimize", || Ok(native(ctx, id)?.filter(|node| node["lumen_state"]["minimised"] == true)))?;
    ctx.checked(&format!("[con_id={id}] scratchpad show"))?;
    wait("fixed-size native restore", || Ok(native(ctx, id)?.filter(|node| node["lumen_state"]["minimised"] == false)))?;
    need(native(ctx, id)?.ok_or("Fixed client vanished")?["rect"] == original,
        "Fixed-size minimize changed geometry")?;
    ctx.write(fixed_index, "q\n")?;
    ctx.children[fixed_index].stop(libc::SIGTERM)?;
    wait("retired fixed client", || Ok(native(ctx, id)?.is_none().then_some(())))?;

    let (_, utility) = open(ctx, "Quick Look")?;
    let utility_id = &utility["id"];
    let utility_rect = settled(ctx, utility_id)?["rect"].clone();
    for index in [1, 2] {
        click(ctx, utility_id, index)?;
        ctx.key(&["-k", "Escape"])?;
        let row = native(ctx, utility_id)?.ok_or("Utility disappeared")?;
        need(row["rect"] == utility_rect && row["fullscreen_mode"].as_i64().unwrap_or(0) == 0
            && row["lumen_state"]["minimised"] == false,
            "Unavailable utility control mutated window")?;
    }
    for action in ["minimize","fullscreen"] {
        rejected(ctx, utility_id, action)?;
        let row = native(ctx, utility_id)?.ok_or("Utility disappeared")?;
        need(row["rect"] == utility_rect && row["lumen_state"]["minimised"] == false,
            "Unavailable utility action changed window")?;
    }
    ctx.picture("utility-disabled.png")?;
    click(ctx, utility_id, 0)?;
    wait("utility close", || Ok(native(ctx, utility_id)?.is_none().then_some(())))?;

    let (_, first) = open(ctx, "Document One")?;
    let (_, second) = open(ctx, "Document Two")?;
    let ids = [first["id"].clone(), second["id"].clone()];
    for (index, id) in ids.iter().enumerate() {
        ctx.checked(&format!("[con_id={id}] move absolute position {} px {} px, resize set width {} px height {} px",
            100 + index*500, 120 + index*100, 400 + index*20, 260 + index*20))?;
    }
    let before = [settled(ctx, &ids[0])?["rect"].clone(), settled(ctx, &ids[1])?["rect"].clone()];
    for id in &ids {
        click(ctx, id, 1)?;
        wait("independent native minimize", || Ok(native(ctx, id)?.filter(|node| node["lumen_state"]["minimised"] == true)))?;
    }
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette["dark"],
        "preferences":{"dockMagnification":false,"reduceMotion":true}}))?;
    let dock_qml = ctx.root.0.join("dock.qml");
    fs::write(&dock_qml, include_str!("../window_controls_dock.qml"))?;
    ctx.launch("quickshell", &["-p", &path(&dock_qml)], "dock.log", false)?;
    wait("two minimized Dock rows", || Ok(dock_state(ctx, &dock_qml).ok().filter(|value|
        value["windows"].as_array().is_some_and(|rows| rows.len() == 2 && rows.iter().all(|row| row["minimized"] == true)))))?;
    for id in ids.iter().rev() {
        let title = if *id == ids[0] { "Document One" } else { "Document Two" };
        dock_open(ctx, &dock_qml)?;
        wait("Dock context opened", || Ok((dock_state(ctx, &dock_qml)?["open"] == true).then_some(())))?;
        ctx.key(&["-k", "Down"])?;
        ctx.key(&["-k", "Down"])?;
        if *id == ids[1] { ctx.key(&["-k", "Down"])?; }
        wait("captured minimized Dock row", || Ok((dock_state(ctx, &dock_qml)?["selected"] == format!("Minimized — {title}")).then_some(())))?;
        ctx.key(&["-k", "Return"])?;
        wait("exact Dock restoration", || Ok(native(ctx, id)?.filter(|node|
            node["lumen_state"]["minimised"] == false && node["focused"] == true)))?;
        let row = native(ctx, id)?.ok_or("Restored document disappeared")?;
        let original = if *id == ids[0] { &before[0] } else { &before[1] };
        need(row["rect"] == *original && row["name"] == title,
            "Dock restored wrong window identity or geometry")?;
    }
    need(dock_state(ctx, &dock_qml)?["focuses"] == json!(["Document Two","Document One"]),
        "Dock focus order changed")?;
    ctx.picture("restored-documents.png")?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":[
        "fixed-size disabled controls and eleven commands refuse without mutation",
        "fixed-size minimize and restore remain available",
        "utility unsupported controls refuse and Close works",
        "two independent minimized documents restore through production Dock rows with exact geometry and focus"]}))?;
    Ok(())
}
