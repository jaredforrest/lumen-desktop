//! Physical semantic controls across palette and contrast settings.
use super::*;

fn state(ctx: &Context) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.ipc("state", &[])?)?)
}

pub fn run(ctx: &mut Context) -> Result<()> {
    ctx.vars.insert("LUMEN_UNIFIED_OWNED_HEADERS".into(), "1".into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_CONTROLS".into(), "0".into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_ACTIONS".into(), "0".into());
    ctx.vars.insert("LUMEN_LAUNCH_SCOPES".into(), "0".into());
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    let qml = path(&ctx.qml);
    ctx.launch("quickshell", &["-p", &qml], "semantic-state-shell.log", false)?;
    ctx.launch("wtype", &["-s", "60000"], "semantic-state-keyboard.log", false)?;
    ctx.pointer_start()?;
    wait("native semantic window", || Ok(find_node(&ctx.tree()?, |node| node["name"] == "Private semantic states" && node["visible"] == true)))?;
    wait("semantic state ready", || Ok((state(ctx)?["visible"] == true).then_some(())))?;
    let initial = state(ctx)?;
    let mut checks = Vec::new();
    for (mode, contrast) in [("light", false), ("light", true), ("dark", false), ("dark", true)] {
        save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette[mode],
            "preferences":{"reduceMotion":true,"reduceTransparency":true,"increaseContrast":contrast}}))?;
        wait("semantic appearance", || Ok((state(ctx)?["dark"] == (mode == "dark") && state(ctx)?["contrast"] == contrast).then_some(())))?;
        for severity in ["error", "success", "warning"] {
            ctx.ipc("severity", &[severity])?;
            let prefix = format!("{}:", severity[..1].to_uppercase() + &severity[1..]);
            wait("visible severity and accessibility", || {
                let row = state(ctx)?;
                Ok((row["severity"] == severity
                    && row["statusTexts"].as_array().is_some_and(|texts| texts.iter().any(|text| text.as_str().is_some_and(|text| text.contains(&prefix))))
                    && row["statusName"] == format!("{prefix} The request returned a result")).then_some(()))
            })?;
        }
        need(ctx.ipc("focus", &["toggle"])? == "true", "Could not focus semantic toggle")?;
        let before = state(ctx)?["checked"].clone();
        ctx.key(&["-k", "space"])?;
        wait("physical toggle activation", || Ok((state(ctx)?["checked"] != before).then_some(())))?;
        need(state(ctx)?["thumb"] == if before == true { 2 } else { 14 }, "Toggle thumb position wrong")?;
        need(ctx.ipc("focus", &["radio"])? == "true", "Could not focus semantic radio")?;
        let before = state(ctx)?["active"].clone();
        ctx.key(&["-k", "Return"])?;
        wait("physical radio activation", || Ok((state(ctx)?["active"] != before).then_some(())))?;
        let label = if before == true { "Disconnected" } else { "Connected" };
        need(state(ctx)?["radioName"].as_str().is_some_and(|name| name.contains(label)), "Radio accessibility state wrong")?;
        ctx.ipc("availability", &["false"])?;
        wait("semantic unavailability", || {
            let row = state(ctx)?;
            Ok((row["unavailable"] == true && row["radioFocus"] == false && row["radioEnabled"] == false).then_some(()))
        })?;
        let before = state(ctx)?["active"].clone();
        ctx.key(&["-k", "Return"])?;
        need(state(ctx)?["active"] == before && state(ctx)?["deleted"] == 0
            && state(ctx)?["radioName"].as_str().is_some_and(|name| name.contains("unavailable")), "Unavailable control accepted keyboard action")?;
        ctx.key(&["-k", "Tab"])?;
        wait("Tab skips unavailable control", || Ok((state(ctx)?["toggleFocus"] == true).then_some(())))?;
        let node = find_node(&ctx.tree()?, |node| node["name"] == "Private semantic states").ok_or("Missing semantic native window")?;
        let rect = &state(ctx)?["radioRect"];
        let x = number(&node["rect"]["x"])? as f64 + number(&node["window_rect"]["x"])? as f64
            + rect["x"].as_f64().ok_or("Missing radio x")? + rect["width"].as_f64().ok_or("Missing radio width")? / 2.0;
        let y = number(&node["rect"]["y"])? as f64 + number(&node["window_rect"]["y"])? as f64
            + rect["y"].as_f64().ok_or("Missing radio y")? + rect["height"].as_f64().ok_or("Missing radio height")? / 2.0;
        let receipts = state(ctx)?["pointerReceipts"].as_array().ok_or("Missing pointer receipts")?.len();
        ctx.move_pointer((x, y))?;
        ctx.press()?;
        ctx.release()?;
        wait("unavailable pointer refusal", || Ok((state(ctx)?["pointerReceipts"].as_array().is_some_and(|rows| rows.len() == receipts + 1)).then_some(())))?;
        need(state(ctx)?["active"] == before && state(ctx)?["deleted"] == 0, "Unavailable pointer action escaped")?;
        ctx.ipc("availability", &["true"])?;
        let row = state(ctx)?;
        need(row["rect"] == initial["rect"], "Semantic native window changed size")?;
        let capture = format!("semantic-{mode}-{contrast}.png");
        ctx.picture(&capture)?;
        checks.push(json!({"mode":mode,"contrast":contrast,"state":row,"capture":capture}));
    }
    let log = fs::read_to_string(ctx.opt.output.join("semantic-state-shell.log"))?;
    need(!["ReferenceError", "TypeError", "binding loop", "Failed to load configuration"].iter().any(|v| log.contains(v)), format!("Semantic QML warning: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":checks}))?;
    println!("PASS native semantic controls across four appearance cases");
    Ok(())
}
