//! Live Calendar detail actions at enlarged text, with physical keyboard input.
use super::*;

fn state(ctx: &Context) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.ipc("state", &[])?)?)
}

fn shown(ctx: &Context, label: &str) -> Result<()> {
    wait(label, || {
        let row = state(ctx)?;
        Ok((row["focus"] == label).then_some(()))
    })
}

fn paint(ctx: &Context, shell_pid: u32, name: &str) -> Result<Value> {
    let row = state(ctx)?;
    let target = &row["target"];
    need(target["pixelSize"] == 20, format!("Calendar action text scale: {row}"))?;
    let rect = &target["rect"];
    let x = rect["x"].as_f64().ok_or("Missing Calendar action x")?;
    let y = rect["y"].as_f64().ok_or("Missing Calendar action y")?;
    let width = rect["width"].as_f64().ok_or("Missing Calendar action width")?;
    let height = rect["height"].as_f64().ok_or("Missing Calendar action height")?;
    need(x >= 0.0 && y >= 0.0 && x + width <= row["width"].as_f64().ok_or("Missing Calendar width")? + 1.0
        && y + height <= row["height"].as_f64().ok_or("Missing Calendar height")? + 1.0,
        format!("Calendar action outside owner: {row}"))?;
    let node = find_node(&ctx.tree()?, |node| node["pid"] == shell_pid && node["name"] == "Calendar")
        .ok_or("Calendar compositor node missing")?;
    let origin_x = number(&node["rect"]["x"])? + number(&node["window_rect"]["x"])?;
    let origin_y = number(&node["rect"]["y"])? + number(&node["window_rect"]["y"])?;
    let text = &target["textRect"];
    let crop_x = origin_x + text["x"].as_f64().ok_or("Missing text x")?.round() as i64;
    let crop_y = origin_y + text["y"].as_f64().ok_or("Missing text y")?.round() as i64;
    let crop_width = text["width"].as_f64().ok_or("Missing text width")?.round() as i64;
    let crop_height = text["height"].as_f64().ok_or("Missing text height")?.round() as i64;
    need(crop_width > 0 && crop_height > 0, "Empty Calendar action text crop")?;
    let capture = ctx.opt.output.join(format!("{name}.png"));
    ctx.call("grim", &["-o", "HEADLESS-1", &path(&capture)])?;
    let color = target["color"].as_str().ok_or("Missing Calendar text color")?;
    need(color.len() == 7 && color.starts_with('#'), "Invalid Calendar text color")?;
    let rgb = [u8::from_str_radix(&color[1..3], 16)?, u8::from_str_radix(&color[3..5], 16)?, u8::from_str_radix(&color[5..7], 16)?];
    let histogram = ctx.call("magick", &[&path(&capture), "-crop", &format!("{crop_width}x{crop_height}+{crop_x}+{crop_y}"),
        "+repage", "-format", "%c", "histogram:info:"])?;
    let mut pixels = 0usize;
    for line in histogram.lines() {
        let Some((count, rest)) = line.trim().split_once(':') else { continue };
        let Some((_, tuple)) = rest.split_once('(') else { continue };
        let Some((tuple, _)) = tuple.split_once(')') else { continue };
        let channels = tuple.split(',').take(3).map(|part| part.trim().parse::<i32>()).collect::<std::result::Result<Vec<_>, _>>()?;
        if channels.len() == 3 && channels.iter().enumerate().all(|(index, value)| (value - rgb[index] as i32).abs() <= 2) {
            pixels += count.trim().parse::<usize>()?;
        }
    }
    need(pixels >= 4, format!("Calendar action label absent from compositor frame {name}: {pixels}"))?;
    Ok(json!({"pid":shell_pid,"conId":node["id"],"textBox":[crop_x,crop_y,crop_width,crop_height],"inkPixels":pixels,"capture":capture}))
}

pub fn run(ctx: &mut Context) -> Result<()> {
    for (key, value) in [
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"), ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"), ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"), ("LUMEN_NEARBY_SHARING_PROVIDER", ""),
        ("LUMEN_FIXTURE_CALENDAR_NATIVE", "true"),
    ] { ctx.vars.insert(key.into(), value.into()); }
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette["light"],
        "preferences":{"textScale":1.5,"reduceMotion":true,"reduceTransparency":true}}))?;
    let source = fs::read_to_string(&ctx.qml)?;
    fs::write(&ctx.qml, source.replace("__PALETTES__", &serde_json::to_string(&palette)?))?;
    let bin = ctx.root.0.join("bin");
    fs::create_dir(&bin)?;
    let provider = std::env::current_exe()?.parent().ok_or("Fixture bin directory missing")?.join("calendar_detail_provider");
    symlink(provider, bin.join("lumen-calendar"))?;
    let old_path = ctx.vars.get("PATH").cloned().unwrap_or_default();
    ctx.vars.insert("PATH".into(), format!("{}:{old_path}", bin.display()));
    let actions = ctx.opt.output.join("provider-actions.jsonl");
    ctx.vars.insert("LUMEN_FIXTURE_CALENDAR_ACTIONS".into(), path(&actions));
    let qml = path(&ctx.qml);
    let shell = ctx.launch("quickshell", &["--no-duplicate", "-p", &qml], "calendar-detail-typography-shell.log", false)?;
    ctx.launch("wtype", &["-s", "60000"], "calendar-detail-typography-keyboard.log", false)?;
    wait("Calendar details IPC", || Ok((ctx.ipc("ready", &[])? == "true").then_some(())))?;
    let shell_pid = ctx.children[shell].0.id();
    let mut cases = Vec::new();
    for mode in ["light", "dark"] {
        ctx.ipc("open", &[mode])?;
        wait("Calendar event loaded", || { let row = state(ctx)?;
            Ok((row["active"] == true && row["loaded"] == true && row["busy"] == false && row["events"] == 1).then_some(())) })?;
        ctx.ipc("details", &[])?;
        wait("Calendar details shown", || { let row = state(ctx)?;
            Ok((row["detail"] == "private-event" && row["inspector"] == true).then_some(())) })?;
        ctx.ipc("focus", &["New Event…"])?; shown(ctx, "New Event…")?;
        let receipt = paint(ctx, shell_pid, &format!("{mode}-new-event"))?;
        ctx.key(&["-k", "Return"])?;
        wait("New Event editor", || { let row = state(ctx)?;
            Ok((row["editing"] == true && row["focus"] == "Event title").then_some(())) })?;
        cases.push(json!({"mode":mode,"case":"new-event","painted":receipt,"state":state(ctx)?}));
        ctx.ipc("focus", &["Cancel"])?; shown(ctx, "Cancel")?;
        ctx.key(&["-k", "Return"])?;
        wait("Cancel returns to details", || Ok((state(ctx)?["editing"] == false).then_some(())))?;
        need(state(ctx)?["detail"] == "private-event", "Cancel lost selected Calendar event")?;
        cases.push(json!({"mode":mode,"case":"cancel","state":state(ctx)?}));
        ctx.ipc("focus", &["Edit Occurrence…"])?; shown(ctx, "Edit Occurrence…")?;
        let receipt = paint(ctx, shell_pid, &format!("{mode}-edit-event"))?;
        ctx.key(&["-k", "Return"])?;
        wait("Edit Occurrence editor", || { let row = state(ctx)?;
            Ok((row["editing"] == true && row["focus"] == "Event title").then_some(())) })?;
        ctx.key(&["-k", "Escape"])?;
        wait("Escape closes Calendar details", || { let row = state(ctx)?;
            Ok((row["editing"] == false && row["inspector"] == false).then_some(())) })?;
        cases.push(json!({"mode":mode,"case":"edit-escape","painted":receipt,"state":state(ctx)?}));
        ctx.ipc("details", &[])?;
        ctx.ipc("focus", &["iCloud Accounts…"])?; shown(ctx, "iCloud Accounts…")?;
        ctx.key(&["-k", "Tab"])?; shown(ctx, "Google Accounts…")?;
        let receipt = paint(ctx, shell_pid, &format!("{mode}-accounts"))?;
        let before = number(&state(ctx)?["accounts"])?;
        ctx.key(&["-k", "Return"])?;
        wait("Google account stub", || Ok((state(ctx)?["accounts"] == before + 1).then_some(())))?;
        cases.push(json!({"mode":mode,"case":"accounts-tab","painted":receipt,"state":state(ctx)?}));
        ctx.key(&["-k", "Escape"])?;
        wait("Calendar details Escape", || Ok((state(ctx)?["inspector"] == false).then_some(())))?;
    }
    for line in fs::read_to_string(&actions)?.lines() {
        let request: Value = serde_json::from_str(line)?;
        need(request["action"] == "list" || request["action"] == "load", format!("Mutating Calendar request: {request}"))?;
    }
    let log = fs::read_to_string(ctx.opt.output.join("calendar-detail-typography-shell.log"))?;
    need(!["ReferenceError", "TypeError", "binding loop"].iter().any(|marker| log.contains(marker)), format!("Calendar QML errors: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"cases":cases}))?;
    println!("PASS Calendar detail native keyboard and paint in two modes");
    Ok(())
}
