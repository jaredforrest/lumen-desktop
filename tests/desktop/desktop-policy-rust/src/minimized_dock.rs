use super::*;

fn dock_call(ctx: &Context, qml: &Path, action: &str, args: &[&str]) -> Result<String> {
    let name = path(qml);
    let mut command = vec!["-p", name.as_str(), "ipc", "call", "minimizedDock", action];
    command.extend_from_slice(args);
    ctx.call("quickshell", &command)
}
fn ui(ctx: &Context, qml: &Path) -> Result<Value> {
    Ok(serde_json::from_str(&dock_call(ctx, qml, "status", &[])?)?)
}
fn node(ctx: &Context, id: &Value) -> Result<Option<Value>> { ctx.find(id) }
fn make(ctx: &mut Context, key: &str) -> Result<Value> {
    let qml = ctx.root.0.join(format!("minimized-{key}.qml"));
    fs::write(&qml, include_str!("../window_controls_client.qml"))?;
    ctx.vars.insert("WINDOW_CONTROLS_TITLE".into(), "Same document title".into());
    let index = ctx.launch("quickshell", &["-p", &path(&qml)], &format!("minimized-{key}.log"), false)?;
    let pid = ctx.children[index].0.id();
    wait(&format!("{key} native document"), || Ok(find_node(&ctx.tree()?, |item|
        item["pid"] == pid && item["name"] == "Same document title")))
}
fn length(ctx: &Context, qml: &Path, key: &str, amount: usize) -> Result<Value> {
    wait(&format!("{amount} {key}"), || {
        let row = ui(ctx, qml)?;
        Ok((row[key].as_array().is_some_and(|items| items.len() == amount)).then_some(row))
    })
}
fn other_key(rows: &Value, excluded: &str) -> Result<String> {
    rows["entries"].as_array().ok_or("No minimized entries")?.iter()
        .filter_map(|item| item["key"].as_str()).find(|key| *key != excluded)
        .map(str::to_owned).ok_or_else(|| fail("No distinct minimized key"))
}
fn point(ctx: &mut Context, qml: &Path, key: &str) -> Result<()> {
    let row = ui(ctx, qml)?;
    let index = row["keys"].as_array().ok_or("No Dock keys")?.iter().position(|item| item == key)
        .ok_or("Captured Dock item is absent")? as f64;
    let width = row["width"].as_f64().ok_or("Dock width missing")?;
    let slot = row["slot"].as_f64().ok_or("Dock slot missing")?;
    let shelf = row["shelf"].as_f64().ok_or("Dock shelf missing")?;
    let x = ((1280.0-width)/2.0 + 12.0 + (index+0.5)*slot).round() as i64;
    let y = (900.0-7.0-shelf/2.0).round() as i64;
    ctx.checked(&format!("seat seat0 cursor set {x} {y}"))?;
    ctx.pointer_command("m 0 0")?;
    wait("genuine minimized Dock hover", || Ok((ui(ctx, qml)?["hovered"] == index as i64).then_some(())))
}
fn minimized(ctx: &Context, id: &Value, wanted: bool) -> Result<Value> {
    wait("exact native minimized state", || Ok(node(ctx,id)?.filter(|item| item["lumen_state"]["minimised"] == wanted)))
}
pub fn run(ctx: &mut Context) -> Result<()> {
    ctx.checked("output HEADLESS-1 resolution 1280x900 position 0 0; output HEADLESS-2 resolution 1024x768 position 1280 0; focus output HEADLESS-1")?;
    ctx.pointer_start()?;
    ctx.pointer_command("m 0 0")?;
    ctx.launch("wtype", &["-s", "120000"], "keyboard.log", false)?;
    wait("virtual pointer capability", || {
        let seats = ctx.sway(&["-t", "get_seats"])?;
        Ok(seats.as_array().and_then(|rows| rows.iter().find(|seat|
            seat["capabilities"].as_i64().is_some_and(|value| value & 1 != 0))).cloned())
    })?;
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette["dark"],
        "preferences":{"dockMagnification":false,"reduceMotion":true,"dockMinimizeDestination":"application"}}))?;
    let qml = ctx.root.0.join("minimized-dock.qml");
    fs::write(&qml, include_str!("../minimized_dock.qml"))?;
    ctx.launch("quickshell", &["-p", &path(&qml)], "minimized-dock.log", false)?;
    wait("application Dock destination", || Ok((ui(ctx, &qml)?["mode"] == "application").then_some(())))?;
    ctx.checked("focus output HEADLESS-1")?;
    let first = make(ctx, "first")?;
    length(ctx, &qml, "minimized", 1)?;
    dock_call(ctx, &qml, "observeFirst", &[])?;
    ctx.checked("focus output HEADLESS-2")?;
    let second = make(ctx, "second")?;
    length(ctx, &qml, "minimized", 2)?;
    let ids = [first["id"].clone(), second["id"].clone()];
    let boxes = [first["rect"].clone(), second["rect"].clone()];
    for id in &ids { ctx.checked(&format!("[con_id={id}] lumen_window minimize"))?; }
    wait("two independent minimized windows", || {
        let row = ui(ctx, &qml)?;
        Ok((row["minimized"].as_array().is_some_and(|items| items.len()==2 && items.iter().all(|item| item == true))).then_some(row))
    })?;
    need(ui(ctx, &qml)?["keys"] == json!(["synthetic-documents"]),
        "Default application destination did not keep one item")?;
    dock_call(ctx, &qml, "mode", &["windows"])?;
    let row = length(ctx, &qml, "entries", 2)?;
    let mut key1 = dock_call(ctx, &qml, "firstKey", &[])?;
    let mut key2 = other_key(&row, &key1)?;
    need(!key1.is_empty() && key1 != key2, "Same-title minimized windows collided")?;
    dock_call(ctx, &qml, "capture", &[&key1])?;
    dock_call(ctx, &qml, "cycleMode", &[])?;
    need(dock_call(ctx, &qml, "stale", &[])? == "false", "Same-turn mode cycle resurrected lease")?;
    key1 = dock_call(ctx, &qml, "capturedKey", &[])?;
    key2 = other_key(&ui(ctx, &qml)?, &key1)?;
    dock_call(ctx, &qml, "capture", &[&key2])?;
    let old_key2 = key2.clone();
    dock_call(ctx, &qml, "focus", &[])?;
    wait("Dock keyboard mode", || Ok(ui(ctx, &qml).ok().filter(|row| row["keyboard"]==true && row["active"]==true)))?;
    let keys = ui(ctx, &qml)?["keys"].as_array().ok_or("No Dock keys")?.clone();
    let index = keys.iter().position(|item| item == &key2).ok_or("Second handle absent")?;
    ctx.key(&["-k", "Home"])?;
    for _ in 0..index { ctx.key(&["-k", "Right"])?; }
    wait("second captured keyboard handle", || Ok((ui(ctx, &qml)?["keyboardIndex"] == index as i64).then_some(())))?;
    ctx.key(&["-k", "Return"])?;
    let restored = minimized(ctx, &ids[1], false)?;
    need(restored["focused"] == true && restored["rect"] == boxes[1] && restored["visible"] == true
        && minimized(ctx, &ids[0], true).is_ok(), "Keyboard restored wrong same-title document")?;
    ctx.checked(&format!("[con_id={}] lumen_window minimize", ids[1]))?;
    let row = length(ctx, &qml, "entries", 2)?;
    key2 = other_key(&row, &key1)?;
    need(key2 != old_key2 && dock_call(ctx, &qml, "stale", &[])? == "false",
        "Restore/reminimize reused old action lease")?;
    point(ctx, &qml, &key1)?;
    ctx.press()?; ctx.release()?;
    let restored = minimized(ctx, &ids[0], false)?;
    need(restored["focused"] == true && restored["rect"] == boxes[0] && restored["visible"] == true
        && minimized(ctx, &ids[1], true).is_ok(), "Pointer restored wrong same-title document")?;
    ctx.checked(&format!("[con_id={}] lumen_window minimize", ids[0]))?;
    let row = length(ctx, &qml, "entries", 2)?;
    key1 = other_key(&row, &key2)?;
    dock_call(ctx, &qml, "capture", &[&key1])?;
    point(ctx, &qml, &key1)?;
    ctx.press()?;
    wait("captured held Dock key", || Ok((ui(ctx, &qml)?["pressed"] == key1).then_some(())))?;
    ctx.checked(&format!("[con_id={}] kill", ids[0]))?;
    wait("retired first native document", || Ok(node(ctx, &ids[0])?.is_none().then_some(())))?;
    let third = make(ctx, "replacement")?;
    let id3 = third["id"].clone();
    ctx.checked(&format!("[con_id={id3}] lumen_window minimize"))?;
    length(ctx, &qml, "entries", 2)?;
    let attempts = ui(ctx, &qml)?["attempts"].clone();
    ctx.release()?;
    wait("retired press key cleared", || Ok((ui(ctx, &qml)?["pressed"] == "").then_some(())))?;
    need(dock_call(ctx, &qml, "stale", &[])? == "false" && minimized(ctx, &id3, true).is_ok()
        && minimized(ctx, &ids[1], true).is_ok() && ui(ctx, &qml)?["attempts"] == attempts
        && ui(ctx, &qml)?["launches"] == 0,
        "Held retired handle activated replacement or bystander")?;
    let key3 = other_key(&ui(ctx, &qml)?, &key2)?;
    point(ctx, &qml, &key3)?;
    ctx.press()?;
    wait("held replacement Dock key", || Ok((ui(ctx, &qml)?["pressed"] == key3).then_some(())))?;
    dock_call(ctx, &qml, "mode", &["application"])?;
    length(ctx, &qml, "entries", 0)?;
    ctx.release()?;
    wait("destination change clears held item", || Ok((ui(ctx, &qml)?["pressed"] == "").then_some(())))?;
    need(minimized(ctx, &ids[1], true).is_ok() && minimized(ctx, &id3, true).is_ok()
        && ui(ctx, &qml)?["launches"] == 0, "Destination change launched or restored a window")?;
    ctx.picture("application-destination.png")?;
    dock_call(ctx, &qml, "mode", &["windows"])?;
    length(ctx, &qml, "entries", 2)?;
    ctx.picture("window-destination.png")?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"nativeIds":[ids[0],ids[1],id3],
        "checks":["default application destination retains one item","same-title minimized handles remain distinct",
            "mode cycle and re-minimize retire stale leases","keyboard and pointer restore exact siblings",
            "retired held lease cannot activate replacement","destination change retires held item"]}))?;
    Ok(())
}
