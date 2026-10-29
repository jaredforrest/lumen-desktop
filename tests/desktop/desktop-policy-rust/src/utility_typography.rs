//! Production utility windows: enlarged text, keyboard reachability and cancellation.
use super::*;

fn state(ctx: &Context) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.ipc("state", &[])?)?)
}

fn reachable(row: &Value) -> Result<()> {
    let rect = &row["focusRect"];
    let x = rect["x"].as_f64().ok_or("Missing focused x")?;
    let y = rect["y"].as_f64().ok_or("Missing focused y")?;
    let width = rect["width"].as_f64().ok_or("Missing focused width")?;
    let height = rect["height"].as_f64().ok_or("Missing focused height")?;
    need(x >= 0.0 && y >= 0.0 && x + width <= row["width"].as_f64().ok_or("Missing owner width")? + 1.0
        && y + height <= row["height"].as_f64().ok_or("Missing owner height")? + 1.0,
        format!("Focused utility control outside owner: {row}"))
}

pub fn run(ctx: &mut Context) -> Result<()> {
    for (key, value) in [
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NEARBY_SHARING_PROVIDER", ""),
    ] { ctx.vars.insert(key.into(), value.into()); }
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette["light"],
        "preferences":{"textScale":1.5,"reduceMotion":true,"reduceTransparency":true}}))?;
    let qml = path(&ctx.qml);
    ctx.launch("quickshell", &["--no-duplicate", "-p", &qml], "utility-typography-shell.log", false)?;
    ctx.launch("wtype", &["-s", "60000"], "utility-typography-keyboard.log", false)?;
    wait("utility typography IPC", || Ok((ctx.ipc("state", &[])? == "{}").then_some(())))?;
    let mut cases = Vec::new();
    for (kind, initial, after) in [
        ("TextTools", "Menu command path", "Save Application Shortcut"),
        ("Remote", "Remote computer address", "Open"),
        ("Info", "General", "Sharing & Permissions"),
    ] {
        ctx.ipc("open", &[kind])?;
        wait("focused utility input", || {
            let row = state(ctx)?;
            Ok((row["active"] == true && row["focus"] == initial).then_some(()))
        })?;
        need(state(ctx)?["scale"] == 1.5, format!("Utility scale missing: {kind}"))?;
        if kind != "Info" {
            ctx.call("wtype", &["Private"])?;
            wait("utility physical text", || Ok((state(ctx)?["value"] == "Private").then_some(())))?;
        }
        ctx.key(&["-k", "Tab"])?;
        wait("utility Tab target", || Ok((state(ctx)?["focus"] == after).then_some(())))?;
        let row = state(ctx)?;
        reachable(&row)?;
        if kind == "Info" {
            ctx.key(&["-k", "space"])?;
            wait("physical section collapse", || {
                let now = state(ctx)?;
                Ok((now["expanded"] == "Expanded" && now["sections"].as_array().is_some_and(|items|
                    items.iter().any(|item| item["name"] == "Sharing & Permissions" && item["state"] == "Collapsed"))).then_some(()))
            })?;
        }
        ctx.call("grim", &["-o", "HEADLESS-1", &path(&ctx.opt.output.join(format!("{kind}.png")))])?;
        ctx.key(&["-k", "Escape"])?;
        wait("utility Escape", || Ok((state(ctx)?["visible"] == false).then_some(())))?;
        cases.push(json!({"consumer":kind,"before":initial,"after":row}));
    }
    let log = fs::read_to_string(ctx.opt.output.join("utility-typography-shell.log"))?;
    need(!["ReferenceError", "TypeError", "binding loop"].iter().any(|marker| log.contains(marker)), format!("Utility QML errors: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"cases":cases,
        "scope":"Physical scaled input, Tab reachability and Escape in three production utility families"}))?;
    println!("PASS three production utility keyboard families");
    Ok(())
}
