//! Live palette and material changes on production owned overlays.
use super::*;

fn state(ctx: &Context) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.ipc("status", &[])?)?)
}

fn color(value: &Value) -> Result<[i32; 3]> {
    let text = value.as_str().ok_or("Missing palette color")?;
    need(text.len() == 7 && text.starts_with('#'), "Invalid palette color")?;
    Ok([i32::from_str_radix(&text[1..3], 16)?, i32::from_str_radix(&text[3..5], 16)?, i32::from_str_radix(&text[5..7], 16)?])
}

fn counts(ctx: &Context, image: &Path, colors: &[[i32; 3]]) -> Result<Vec<usize>> {
    let histogram = ctx.call("magick", &[&path(image), "-format", "%c", "histogram:info:"])?;
    let mut totals = vec![0usize; colors.len()];
    for line in histogram.lines() {
        let Some((count, rest)) = line.trim().split_once(':') else { continue };
        let Some((_, tuple)) = rest.split_once('(') else { continue };
        let Some((tuple, _)) = tuple.split_once(')') else { continue };
        let channels = tuple.split(',').take(3).map(|v| v.trim().parse::<i32>()).collect::<std::result::Result<Vec<_>, _>>()?;
        if channels.len() != 3 { continue }
        let count = count.trim().parse::<usize>()?;
        for (index, target) in colors.iter().enumerate() {
            if channels.iter().zip(target).all(|(actual, expected)| (actual - expected).abs() <= 2) {
                totals[index] += count;
            }
        }
    }
    Ok(totals)
}

pub fn run(ctx: &mut Context) -> Result<()> {
    ctx.vars.insert("LUMEN_UNIFIED_OWNED_HEADERS".into(), "1".into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_CONTROLS".into(), "0".into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_ACTIONS".into(), "0".into());
    ctx.vars.insert("LUMEN_LAUNCH_SCOPES".into(), "0".into());
    ctx.vars.insert("LUMEN_NEARBY_SHARING_PROVIDER".into(), "".into());
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette["light"],
        "preferences":{"mode":"light","reduceMotion":true,"reduceTransparency":false},
        "searchPreferences":{"disabledProviders":["files","clipboard"]}}))?;
    let bin = ctx.root.0.join("bin");
    fs::create_dir(&bin)?;
    let stub = env::current_exe()?.with_file_name("live_overlay_overview_stub");
    symlink(&stub, bin.join("lumen-window-overview"))?;
    symlink(env::current_exe()?.with_file_name("live_overlay_system_controls_stub"), bin.join("lumen-system-controls"))?;
    let old_path = ctx.vars.get("PATH").cloned().unwrap_or_default();
    ctx.vars.insert("PATH".into(), format!("{}:{old_path}", bin.display()));
    let qml = path(&ctx.qml);
    ctx.launch("quickshell", &["-p", &qml], "live-overlay-shell.log", false)?;
    ctx.launch("wtype", &["-s", "60000"], "live-overlay-keyboard.log", false)?;
    wait("live overlay IPC", || Ok(state(ctx).ok().map(|_| ())))?;
    let mut checks = Vec::new();
    for family in ["launcher", "switcher", "overview", "controls", "notifications"] {
        ctx.ipc("open", &[family])?;
        wait("mapped live overlay", || {
            let row = state(ctx)?;
            Ok((row["family"] == family && row["visible"] == true && row["body"]["width"].as_f64().is_some_and(|width| width > 0.0)).then_some(()))
        })?;
        if family == "switcher" {
            ctx.key(&["-k", "Right"])?;
            wait("physical switcher selection", || Ok((state(ctx)?["selected"] == 1).then_some(())))?;
        } else {
            wait("overlay focus target", || Ok((ctx.ipc("focusTarget", &[])? == "true").then_some(())))?;
            if family == "launcher" || family == "overview" {
                ctx.key(&["First"])?;
                wait("physical overlay query", || Ok((state(ctx)?["query"] == "First").then_some(())))?;
            }
        }
        let initial = state(ctx)?;
        let identity = if family == "overview" {
            Some(wait("native Overview window", || Ok(find_node(&ctx.tree()?, |node| node["name"] == "Window Overview" && node["visible"] == true)
                .and_then(|node| node["id"].as_i64())))?)
        } else { None };
        let geometry = initial["texts"].as_array().ok_or("Missing overlay texts")?.iter()
            .map(|row| (row["text"].clone(), row["rect"].clone())).collect::<Vec<_>>();
        for (step, (mode, reduced)) in [("light", false), ("dark", false), ("dark", true), ("light", true), ("light", false)].into_iter().enumerate() {
            save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette[mode],
                "preferences":{"mode":mode,"reduceMotion":true,"reduceTransparency":reduced},
                "searchPreferences":{"disabledProviders":["files","clipboard"]}}))?;
            wait("live overlay appearance", || Ok((state(ctx)?["dark"] == (mode == "dark") && state(ctx)?["reduced"] == reduced).then_some(())))?;
            let row = state(ctx)?;
            need(row["visible"] == true && row["body"] == initial["body"] && row["width"] == initial["width"] && row["height"] == initial["height"], "Appearance update changed overlay geometry")?;
            let current_geometry = row["texts"].as_array().ok_or("Missing overlay texts")?.iter().map(|item| (item["text"].clone(), item["rect"].clone())).collect::<Vec<_>>();
            if current_geometry != geometry {
                save(&ctx.opt.output.join("geometry-failure.json"), &json!({"family":family,"step":step,"mode":mode,"reduced":reduced,"initial":geometry,"current":current_geometry,"state":row}))?;
                return Err(fail("Appearance update moved overlay text"));
            }
            if family == "switcher" { need(row["selected"] == 1, "Appearance lost switcher selection")?; }
            else {
                need(row["targetFocus"] == true, "Appearance lost overlay focus")?;
                if family == "launcher" || family == "overview" { need(row["query"] == "First", "Appearance lost search query")?; }
            }
            if let Some(id) = identity {
                need(find_node(&ctx.tree()?, |node| node["id"] == id && node["visible"] == true).is_some(), "Appearance recreated Overview window")?;
            }
            let other = if mode == "light" { "dark" } else { "light" };
            let fresh_text = [palette[mode]["text"].clone(), palette[mode]["muted"].clone()];
            let stale_text = [palette[other]["text"].clone(), palette[other]["muted"].clone()];
            need(row["texts"].as_array().ok_or("Missing overlay texts")?.iter().all(|item| !stale_text.contains(&item["color"]) || fresh_text.contains(&item["color"])), "Stale overlay text color")?;
            if reduced || !ctx.opt.effects {
                need(row["rectangles"].as_array().ok_or("Missing overlay materials")?.iter().all(|item| item["alpha"].as_f64().is_some_and(|alpha| alpha >= 0.999 || alpha == 0.0)), "Unexpected translucent material")?;
            }
            let capture = format!("overlay-{family}-{step}.png");
            ctx.picture(&capture)?;
            let fresh = color(&palette[mode]["background"])?;
            let stale = color(&palette[other]["background"])?;
            let ink = color(&palette[mode]["text"])?;
            let pixels = counts(ctx, &ctx.opt.output.join(&capture), &[fresh, stale, ink])?;
            need(pixels[0] > 500 && pixels[1] < 500 && pixels[2] > 20,
                format!("Overlay palette not painted: {family} {mode} {reduced}: {pixels:?}"))?;
            checks.push(json!({"family":family,"mode":mode,"reduced":reduced,"state":row,"capture":capture,"pixels":pixels,"nativeId":identity}));
        }
        need(state(ctx)?["actions"] == json!([]), "Overlay invoked action during appearance changes")?;
        if family == "controls" || family == "notifications" {
            ctx.key(&["-k", "Return"])?;
            let expected = if family == "controls" { "settings" } else { "default" };
            wait("captured overlay action", || Ok((state(ctx)?["actions"] == json!([expected])).then_some(())))?;
        } else {
            ctx.key(&["-k", "Escape"])?;
            wait("overlay Escape cancellation", || Ok((state(ctx)?["visible"] == false).then_some(())))?;
        }
    }
    let log = fs::read_to_string(ctx.opt.output.join("live-overlay-shell.log"))?;
    need(!["ReferenceError", "TypeError", "binding loop", "Failed to load configuration"].iter().any(|v| log.contains(v)), format!("Overlay QML warning: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":checks}))?;
    println!("PASS live appearance across five overlay families");
    Ok(())
}
