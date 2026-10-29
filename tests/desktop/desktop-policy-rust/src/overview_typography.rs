//! Production Overview at enlarged text with physical search, focus, and dialog input.
use super::*;

fn state(ctx: &Context) -> Result<Value> { Ok(serde_json::from_str(&ctx.ipc("status", &[])?)?) }

fn focused(ctx: &Context, name: &str) -> Result<Value> {
    wait(name, || {
        let row = state(ctx)?;
        Ok((row["focus"] == name || name == "card" && row["focus"].as_str().is_some_and(|v| v.starts_with("A private document"))).then_some(row))
    })
}

fn revealed(row: &Value) -> Result<()> {
    let rect = &row["rect"];
    let clip = &row["clipped"];
    let width = rect["width"].as_f64().ok_or("Missing Overview focus width")?;
    let height = rect["height"].as_f64().ok_or("Missing Overview focus height")?;
    let y = rect["y"].as_f64().ok_or("Missing Overview focus y")?;
    need(clip["width"].as_f64().ok_or("Missing clipped width")? >= width - 1.0
        && clip["height"].as_f64().ok_or("Missing clipped height")? >= height - 1.0
        && y >= -1.0 && y + height <= row["height"].as_f64().ok_or("Missing Overview height")? + 1.0,
        format!("Overview focus not revealed: {row}"))
}

fn painted(ctx: &Context, shell_pid: u32, name: &str) -> Result<Value> {
    let row = state(ctx)?;
    revealed(&row)?;
    let text = &row["text"];
    need(text["resolvedFamily"].as_str().is_some_and(|v| !v.is_empty()), "Overview resolved font missing")?;
    let node = find_node(&ctx.tree()?, |node| node["pid"] == shell_pid && node["name"] == "Window Overview")
        .ok_or("Overview compositor node missing")?;
    let ox = number(&node["rect"]["x"])?;
    let oy = number(&node["rect"]["y"])?;
    let rect = &text["rect"];
    let x = ox + rect["x"].as_f64().ok_or("Missing Overview text x")?.round() as i64;
    let y = oy + rect["y"].as_f64().ok_or("Missing Overview text y")?.round() as i64;
    let width = rect["width"].as_f64().ok_or("Missing Overview text width")?.round() as i64;
    let height = rect["height"].as_f64().ok_or("Missing Overview text height")?.round() as i64;
    need(width > 0 && height > 0, "Empty Overview text crop")?;
    let capture = ctx.opt.output.join(format!("{name}.png"));
    ctx.call("grim", &[&path(&capture)])?;
    let color = text["color"].as_str().ok_or("Missing Overview text color")?;
    need(color.starts_with('#') && color.len() == 7, "Invalid Overview text color")?;
    let rgb = [u8::from_str_radix(&color[1..3], 16)?, u8::from_str_radix(&color[3..5], 16)?, u8::from_str_radix(&color[5..7], 16)?];
    let histogram = ctx.call("magick", &[&path(&capture), "-crop", &format!("{width}x{height}+{x}+{y}"),
        "+repage", "-format", "%c", "histogram:info:"])?;
    let mut pixels = 0usize;
    for line in histogram.lines() {
        let Some((count, rest)) = line.trim().split_once(':') else { continue };
        let Some((_, tuple)) = rest.split_once('(') else { continue };
        let Some((tuple, _)) = tuple.split_once(')') else { continue };
        let channels = tuple.split(',').take(3).map(|part| part.trim().parse::<i32>()).collect::<std::result::Result<Vec<_>, _>>()?;
        if channels.len() == 3 && channels.iter().enumerate().all(|(index, value)| (value - rgb[index] as i32).abs() <= 32) {
            pixels += count.trim().parse::<usize>()?;
        }
    }
    need(pixels >= 4 && (pixels as f64) < (width * height) as f64 * 0.6,
        format!("Overview focused text not painted: {name} {pixels}"))?;
    Ok(json!({"state":row,"capture":capture,"inkPixels":pixels}))
}

fn actions(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() { return Ok(Vec::new()); }
    fs::read_to_string(path)?.lines().map(|line| Ok(serde_json::from_str(line)?)).collect()
}

pub fn run(ctx: &mut Context) -> Result<()> {
    for (key, value) in [
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"), ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"), ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NEARBY_SHARING_PROVIDER", ""), ("LUMEN_FIXTURE_OVERVIEW_NATIVE", "true"),
    ] { ctx.vars.insert(key.into(), value.into()); }
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    ctx.vars.insert("LUMEN_FIXTURE_PALETTES".into(), serde_json::to_string(&palette)?);
    save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette["light"],
        "preferences":{"textScale":1.5,"reduceMotion":true,"reduceTransparency":true}}))?;
    save(&ctx.root.0.join("settings.json"), &json!({"search":{"historyEnabled":false},"clipboard":{"enabled":false}}))?;
    ctx.vars.insert("LUMEN_NIX_SETTINGS_ACTIVE".into(), path(&ctx.root.0.join("settings.json")));
    let bin = ctx.root.0.join("bin"); fs::create_dir(&bin)?;
    symlink(std::env::current_exe()?.parent().ok_or("Fixture bin missing")?.join("overview_typography_provider"), bin.join("lumen-window-overview"))?;
    symlink(std::env::current_exe()?.parent().ok_or("Fixture bin missing")?.join("live_overlay_system_controls_stub"), bin.join("lumen-system-controls"))?;
    let old_path = ctx.vars.get("PATH").cloned().unwrap_or_default();
    ctx.vars.insert("PATH".into(), format!("{}:{old_path}", bin.display()));
    let action_path = ctx.opt.output.join("overview-actions.jsonl");
    ctx.vars.insert("LUMEN_FIXTURE_OVERVIEW_ACTIONS".into(), path(&action_path));
    ctx.checked("output HEADLESS-1 resolution 1200x900")?;
    let qml = path(&ctx.qml);
    let shell = ctx.launch("quickshell", &["-p", &qml], "overview-typography-shell.log", false)?;
    ctx.launch("wtype", &["-s", "60000"], "overview-typography-keyboard.log", false)?;
    ctx.pointer_start()?;
    wait("Overview IPC", || Ok((ctx.ipc("ready", &[])? == "true").then_some(())))?;
    let shell_pid = ctx.children[shell].0.id();
    let mut checks = Vec::new();
    for small in [false, true] {
        for mode in ["light", "dark"] {
            ctx.ipc("open", &[mode, if small { "true" } else { "false" }])?;
            wait("Overview two private windows", || { let row = state(ctx)?;
                Ok((row["visible"] == true && row["active"] == true && row["count"] == 2
                    && row["width"] == if small { 560 } else { 880 }).then_some(())) })?;
            ctx.ipc("focus", &["Find a window"])?; focused(ctx, "Find a window")?;
            ctx.call("wtype", &["private"])?;
            wait("physical Overview search", || Ok((state(ctx)?["query"] == "private" && state(ctx)?["count"] == 2).then_some(())))?;
            ctx.key(&["-k", "Down"])?;
            let before = actions(&action_path)?.len();
            ctx.key(&["-k", "Return"])?;
            wait("captured Overview focus", || Ok((actions(&action_path)?.len() == before + 1 && state(ctx)?["visible"] == false).then_some(())))?;
            need(actions(&action_path)?.last() == Some(&json!({"action":"focus","id":102,"pid":12})), "Wrong Overview focus intent")?;
            checks.push(json!({"mode":mode,"small":small,"case":"physical-search-selection-captured-focus"}));
            ctx.ipc("open", &[mode, if small { "true" } else { "false" }])?;
            wait("Overview reopened", || Ok((state(ctx)?["active"] == true && state(ctx)?["visible"] == true).then_some(())))?;
            ctx.ipc("focus", &["Assign Application"])?;
            let row = focused(ctx, "Assign Application")?; revealed(&row)?;
            let paint = painted(ctx, shell_pid, &format!("{mode}-{small}-footer"))?;
            ctx.key(&["-k", "Tab"])?;
            let row = focused(ctx, "Order desktops by recent use")?; revealed(&row)?;
            let recent = painted(ctx, shell_pid, &format!("{mode}-{small}-recent-order"))?;
            ctx.key(&["-M", "shift", "-k", "Tab", "-m", "shift"])?;
            focused(ctx, "Assign Application")?;
            checks.push(json!({"mode":mode,"small":small,"case":"footer-tab-and-backtab-revealed","paint":paint,"recent":recent}));
            if small {
                let before = state(ctx)?["viewport"]["contentY"].as_f64().ok_or("Missing Overview scroll")?;
                need(before > 0.0, "Constrained Overview should scroll to footer")?;
                let viewport = &state(ctx)?["viewport"]["rect"];
                let (x, y) = (viewport["x"].as_f64().ok_or("Missing viewport x")? + viewport["width"].as_f64().ok_or("Missing viewport width")? / 2.0,
                    viewport["y"].as_f64().ok_or("Missing viewport y")? + viewport["height"].as_f64().ok_or("Missing viewport height")? / 2.0);
                let node = find_node(&ctx.tree()?, |node| node["pid"] == shell_pid && node["name"] == "Window Overview").ok_or("Missing Overview node")?;
                ctx.move_pointer((number(&node["rect"]["x"])? as f64 + x, number(&node["rect"]["y"])? as f64 + y))?;
                ctx.pointer_command("w -1")?;
                wait("Overview wheel scroll", || Ok((state(ctx)?["viewport"]["contentY"].as_f64().is_some_and(|v| v < before)).then_some(())))?;
                checks.push(json!({"mode":mode,"small":small,"case":"real-wheel-scroll","before":before,"after":state(ctx)?}));
            }
            ctx.ipc("focus", &["Manage window groups"])?; focused(ctx, "Manage window groups")?;
            ctx.key(&["-k", "space"])?;
            wait("Overview groups dialog", || Ok((state(ctx)?["groupOpen"] == true && state(ctx)?["focus"] == "Window group").then_some(())))?;
            for name in ["Select window group 1", "Add selected window to group 1", "Create group from selected window", "Close window groups"] {
                ctx.key(&["-k", "Tab"])?; focused(ctx, name)?;
            }
            let paint = painted(ctx, shell_pid, &format!("{mode}-{small}-groups"))?;
            ctx.key(&["-k", "Escape"])?;
            wait("Overview dialog Escape", || Ok((state(ctx)?["groupOpen"] == false && state(ctx)?["focus"] == "Manage window groups").then_some(())))?;
            ctx.key(&["-k", "Escape"])?;
            wait("Overview Escape", || Ok((state(ctx)?["visible"] == false).then_some(())))?;
            checks.push(json!({"mode":mode,"small":small,"case":"physical-group-dialog-tab-escape-focus-return","paint":paint}));
        }
    }
    let log = fs::read_to_string(ctx.opt.output.join("overview-typography-shell.log"))?;
    need(!["ReferenceError", "TypeError", "binding loop", "Failed to load configuration"].iter().any(|v| log.contains(v)), format!("Overview QML errors: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":checks}))?;
    println!("PASS live Overview native typography: {} checks", checks.len());
    Ok(())
}
