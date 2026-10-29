//! Live enlarged Files and Calendar text and keyboard routing in a private compositor.
use super::*;

#[cfg(test)]
mod role_contract {
    use std::fs;
    use std::path::Path;

    fn shell() -> std::path::PathBuf {
        if let Some(source) = option_env!("LUMEN_SHELL_SOURCE_DIR") {
            return std::path::PathBuf::from(source);
        }
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../res/lumen-desktop/shell")
    }

    #[test]
    fn ordinary_sizes_use_named_roles() {
        let root = shell();
        let theme = fs::read_to_string(root.join("Theme.qml")).unwrap();
        for (role, size) in [("caption",10),("metadata",11),("label",12),("body",13),("subheading",15),("title",16),("dialogHeading",17),("heading",18),("display",29)] {
            assert!(theme.contains(&format!("{role}:{size}")), "missing base role {role}");
        }
        assert!(theme.contains("return textSize(typography[role]"));
        assert!(fs::read_to_string(root.join("PanelLabel.qml")).unwrap().contains("font.pixelSize: Theme.textRole(textRole)"));
        for name in ["FilesWindow.qml", "CalendarWindow.qml", "ColumnView.qml"] {
            let source = fs::read_to_string(root.join(name)).unwrap();
            for line in source.lines() {
                let text = line.trim();
                if text.contains("font.pixelSize:") && text.split("font.pixelSize:").nth(1).is_some_and(|value| value.trim_start().starts_with(|c: char| c.is_ascii_digit())) {
                    assert!(text.contains("fixed mark symbol") || text.contains("FileLinks.badge(modelData)"), "unnamed ordinary text size in {name}: {text}");
                }
            }
        }
        let calendar = fs::read_to_string(root.join("CalendarWindow.qml")).unwrap();
        for marker in ["contentWidth:main.width;contentHeight:main.height", "Math.max(calendarViewport.availableWidth,(root.view===\"Day\"?160:420)*Theme.textScale)", "onActiveFocusItemChanged()"] {
            assert!(calendar.contains(marker), "calendar lost scalable scroll contract: {marker}");
        }
    }
}

fn state(ctx: &Context, consumer: &str) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.ipc("status", &[consumer])?)?)
}

fn appearance(ctx: &Context, palette: &Value, mode: &str) -> Result<()> {
    save(&ctx.root.0.join("appearance.json"), &json!({
        "font": palette["font"], "palette": palette[mode],
        "preferences": {"textScale":1.5,"reduceMotion":true,"reduceTransparency":true}
    }))
}

pub fn run(ctx: &mut Context) -> Result<()> {
    ctx.vars.insert("LUMEN_UNIFIED_OWNED_HEADERS".into(), "1".into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_CONTROLS".into(), "0".into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_ACTIONS".into(), "0".into());
    ctx.vars.insert("LUMEN_LAUNCH_SCOPES".into(), "0".into());
    ctx.vars.insert("LUMEN_NEARBY_SHARING_PROVIDER".into(), "".into());
    let qml = path(&ctx.qml);
    ctx.launch("quickshell", &["-p", &qml], "typography-shell.log", false)?;
    // Keep a virtual keyboard attached while individual key events are sent.
    // Otherwise a one-shot wtype device can retire the compositor keyboard focus.
    ctx.launch("wtype", &["-s", "60000"], "typography-keyboard.log", false)?;
    wait("typography IPC", || Ok((ctx.ipc("ready", &[])? == "true").then_some(())))?;
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    let mut checks = Vec::new();
    for mode in ["light", "dark"] {
        appearance(ctx, &palette, mode)?;
        ctx.ipc("showConsumer", &["Files"])?;
        wait("Files rows and text scale", || {
            let row = state(ctx, "Files")?;
            Ok((row["active"] == true && row["textScale"] == 1.5
                && row["rows"].as_array().is_some_and(|v| v.len() == 2)
                && row["configuredFamily"] == palette["font"]
                && row["resolvedFamily"].as_str().is_some_and(|value| !value.is_empty())
                && (ctx.opt.font_config.is_none() || row["resolvedFamily"] == palette["font"])).then_some(()))
        })?;
        let title = state(ctx, "Files")?["title"].as_str().ok_or("Missing Files title")?.to_owned();
        let node = find_node(&ctx.tree()?, |node| node["app_id"] == "org.quickshell" && node["name"] == title)
            .ok_or("Missing native Files window")?;
        let id = node["id"].as_i64().ok_or("Missing Files container ID")?;
        ctx.checked(&format!("[con_id={id}] resize set width 480 px height 320 px, focus"))?;
        wait("constrained Files window", || Ok((state(ctx, "Files")?["width"] == 480 && state(ctx, "Files")?["active"] == true).then_some(())))?;
        ctx.ipc("focusContent", &["Files"])?;
        save(&ctx.opt.output.join(format!("{mode}-before-key.json")), &state(ctx, "Files")?)?;
        save(&ctx.opt.output.join(format!("{mode}-before-tree.json")), &ctx.tree()?)?;
        ctx.key(&["-M", "ctrl", "-k", "f", "-m", "ctrl"])?;
        save(&ctx.opt.output.join(format!("{mode}-after-key.json")), &state(ctx, "Files")?)?;
        save(&ctx.opt.output.join(format!("{mode}-after-tree.json")), &ctx.tree()?)?;
        wait("physical Files search focus", || Ok((state(ctx, "Files")?["search"]["focused"] == true).then_some(())))?;
        ctx.key(&["ordinary"])?;
        wait("physical Files filter", || {
            let row = state(ctx, "Files")?;
            Ok((row["search"]["text"] == "ordinary" && row["rows"] == json!(["An ordinary document.txt"])).then_some(()))
        })?;
        ctx.key(&["-M", "ctrl", "-k", "l", "-m", "ctrl"])?;
        save(&ctx.opt.output.join(format!("{mode}-location-key.json")), &state(ctx, "Files")?)?;
        wait("Files location entry", || {
            let row = state(ctx, "Files")?;
            Ok((row["entry"].is_object() || row["editorOpen"] == true).then_some(()))
        })?;
        ctx.key(&["-k", "Escape"])?;
        wait("Files location cancellation", || Ok((state(ctx, "Files")?["editorOpen"] == false).then_some(())))?;
        ctx.picture(&format!("typography-{mode}-files.png"))?;
        checks.push(json!({"mode":mode,"consumer":"Files","state":state(ctx,"Files")?,"physicalInput":["search","filter","location","cancel"]}));

        ctx.ipc("showConsumer", &["Calendar"])?;
        wait("Calendar owned window", || Ok((state(ctx, "Calendar")?["active"] == true).then_some(())))?;
        need(state(ctx, "Calendar")?["configuredFamily"] == palette["font"]
            && state(ctx, "Calendar")?["resolvedFamily"].as_str().is_some_and(|value| !value.is_empty())
            && (ctx.opt.font_config.is_none() || state(ctx, "Calendar")?["resolvedFamily"] == palette["font"]), "Calendar font mismatch")?;
        let title = state(ctx, "Calendar")?["title"].as_str().ok_or("Missing Calendar title")?.to_owned();
        let node = find_node(&ctx.tree()?, |node| node["app_id"] == "org.quickshell" && node["name"] == title)
            .ok_or("Missing native Calendar window")?;
        let id = node["id"].as_i64().ok_or("Missing Calendar container ID")?;
        ctx.checked(&format!("[con_id={id}] resize set width 480 px height 320 px, focus"))?;
        wait("constrained Calendar window", || Ok((state(ctx, "Calendar")?["width"] == 480 && state(ctx, "Calendar")?["active"] == true).then_some(())))?;
        ctx.key(&["-M", "ctrl", "-k", "f", "-m", "ctrl"])?;
        wait("physical Calendar search focus", || Ok((state(ctx, "Calendar")?["search"]["focused"] == true).then_some(())))?;
        ctx.ipc("focusLastDay", &[])?;
        wait("last visible calendar day", || Ok((state(ctx, "Calendar")?["dayRevealed"] == true).then_some(())))?;
        let before = state(ctx, "Calendar")?["focus"]["date"].clone();
        ctx.key(&["-k", "Left"])?;
        wait("physical calendar date navigation", || {
            let row = state(ctx, "Calendar")?;
            Ok((row["focus"]["date"].is_string() && row["focus"]["date"] != before
                && row["selected"] == row["focus"]["date"] && row["dayRevealed"] == true).then_some(()))
        })?;
        let before_weeks = state(ctx, "Calendar")?["weeks"].clone();
        ctx.key(&["-k", "Right"])?;
        wait("forward month boundary", || {
            let row = state(ctx, "Calendar")?;
            Ok((row["selected"] == "2026-11-01" && row["focus"]["date"] == "2026-11-01"
                && row["dayRevealed"] == true && row["weeks"] != before_weeks).then_some(()))
        })?;
        ctx.key(&["-k", "Left"])?;
        wait("backward month boundary", || {
            let row = state(ctx, "Calendar")?;
            Ok((row["selected"] == "2026-10-31" && row["focus"]["date"] == "2026-10-31"
                && row["dayRevealed"] == true && row["weeks"] == before_weeks).then_some(()))
        })?;
        for kind in ["view", "window", "search"] {
            ctx.ipc("runFocusCancellation", &[kind])?;
            wait("queued Calendar focus cancellation", || Ok((ctx.ipc("cancellation", &[])? != "pending").then_some(())))?;
            need(ctx.ipc("cancellation", &[])? == "passed", format!("Calendar {kind} focus escaped"))?;
            if kind == "window" {
                wait("replacement Files window", || Ok((state(ctx, "Files")?["active"] == true).then_some(())))?;
                ctx.key(&["-M", "ctrl", "-k", "f", "-m", "ctrl"])?;
                wait("replacement Files search focus", || Ok((state(ctx, "Files")?["search"]["focused"] == true).then_some(())))?;
            }
            ctx.ipc("resetCalendar", &[])?;
            ctx.ipc("showConsumer", &["Calendar"])?;
            wait("Calendar after focus cancellation", || Ok((state(ctx, "Calendar")?["active"] == true).then_some(())))?;
        }
        ctx.ipc("inspect", &[])?;
        wait("Calendar inspector", || Ok((state(ctx, "Calendar")?["inspector"] == true).then_some(())))?;
        ctx.key(&["-k", "Escape"])?;
        wait("Calendar inspector cancellation", || Ok((state(ctx, "Calendar")?["inspector"] == false).then_some(())))?;
        ctx.picture(&format!("typography-{mode}-calendar.png"))?;
        checks.push(json!({"mode":mode,"consumer":"Calendar","state":state(ctx,"Calendar")?,"physicalInput":["search","day-navigation","month-boundary","focus-cancellation","inspector-cancel"]}));
    }
    let document = ctx.root.0.join("Documents/An ordinary document.txt");
    need(fs::read_to_string(document)? == "Private fixture unchanged\n", "Fixture document changed")?;
    let log = fs::read_to_string(ctx.opt.output.join("typography-shell.log"))?;
    need(!["ReferenceError", "TypeError", "binding loop", "Failed to load configuration"].iter().any(|v| log.contains(v)), format!("QML runtime warning: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":checks}))?;
    println!("PASS live owned typography keyboard and paint");
    Ok(())
}
