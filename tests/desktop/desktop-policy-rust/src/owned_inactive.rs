use super::*;

fn ipc(ctx: &Context, kind: &str, method: &str, args: &[&str]) -> Result<String> {
    let qml = ctx.root.0.join(format!("{kind}.qml"));
    let name = path(&qml);
    let mut command = vec!["-p", name.as_str(), "ipc", "call", "ownedInactive", method];
    command.extend_from_slice(args);
    ctx.call("quickshell", &command)
}
fn state(ctx: &Context, kind: &str, utility: bool) -> Result<Value> {
    let result: Value = serde_json::from_str(&ipc(ctx, kind, "state", &[if utility {"true"} else {"false"}])?)?;
    Ok(result)
}
fn exact_node(ctx: &Context, kind: &str, utility: bool, pid: u32) -> Result<Option<Value>> {
    let title = state(ctx, kind, utility)?["title"].as_str().unwrap_or("").to_owned();
    Ok(find_node(&ctx.tree()?, |node| node["pid"] == pid && node["name"] == title))
}
fn palette(ctx: &mut Context, palettes: &Value, mode: &str, opaque: bool) -> Result<()> {
    save(&ctx.root.0.join("appearance.json"), &json!({
        "font": palettes["font"], "palette": palettes[mode],
        "preferences": {"reduceMotion": true, "reduceTransparency": opaque}
    }))
}
fn rgb(color: &str) -> Result<[i64; 3]> {
    if let Some(hex) = color.strip_prefix('#') {
        need(hex.len() >= 6, format!("Short color {color}"))?;
        return Ok([0, 2, 4].map(|offset| i64::from(u8::from_str_radix(&hex[offset..offset+2], 16).unwrap())));
    }
    let data = color.strip_prefix("srgb(").and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| fail(format!("Unknown pixel color {color}")))?;
    let components = data.split(',').map(|value| value.parse::<i64>()).collect::<std::result::Result<Vec<_>,_>>()?;
    need(components.len() == 3, format!("Bad pixel {color}"))?;
    Ok([components[0], components[1], components[2]])
}
fn pixel(ctx: &Context, capture: &Path, x: i64, y: i64) -> Result<[i64; 3]> {
    rgb(&ctx.call("magick", &[&path(capture), "-format", &format!("%[pixel:p{{{x},{y}}}]"), "info:"])? )
}
fn matching_pixels(ctx: &Context, capture: &Path, bounds: &Value, origin_x: i64, origin_y: i64, color: &str) -> Result<u64> {
    let x = origin_x + bounds["x"].as_f64().ok_or("Text x missing")?.round() as i64;
    let y = origin_y + bounds["y"].as_f64().ok_or("Text y missing")?.round() as i64;
    let width = bounds["width"].as_f64().ok_or("Text width missing")?.round() as i64;
    let height = bounds["height"].as_f64().ok_or("Text height missing")?.round() as i64;
    need(width > 0 && height > 0, "Primary text has empty bounds")?;
    let lines = ctx.call("magick", &[&path(capture), "-crop", &format!("{width}x{height}+{x}+{y}"),
        "+repage", "-format", "%c", "histogram:info:"])?;
    let expected = rgb(color)?;
    let mut count = 0;
    for line in lines.lines() {
        let Some((amount, rest)) = line.split_once(':') else {continue};
        let Some(hex) = rest.split_whitespace().find(|part| part.starts_with('#')) else {continue};
        let actual = rgb(hex)?;
        if expected.iter().zip(actual).all(|(left,right)| (*left-right).abs() <= 2) {
            count += amount.trim().parse::<u64>()?;
        }
    }
    Ok(count)
}
fn painted(ctx: &Context, capture: &Path, snapshots: &[Value; 4], clients: &BTreeMap<&str, usize>, native: bool) -> Result<Vec<Value>> {
    let tree = ctx.tree()?;
    let mut receipt = Vec::new();
    for row in snapshots {
        let kind = row["kind"].as_str().ok_or("Missing owned kind")?;
        let pid = ctx.children[clients[kind]].0.id();
        let title = row["title"].as_str().ok_or("Missing owned title")?;
        let node = find_node(&tree, |item| item["pid"] == pid && item["name"] == title)
            .ok_or_else(|| fail(format!("Native painted owner missing: {kind} {title}")))?;
        let origin_x = number(&node["rect"]["x"])? + number(&node["window_rect"]["x"])?;
        let origin_y = number(&node["rect"]["y"])? + number(&node["window_rect"]["y"])?;
        let primary = &row["primary"];
        need(!primary.is_null(), format!("Production {kind} primary text missing"))?;
        let text_pixels = matching_pixels(ctx, capture, &primary["rect"], origin_x, origin_y,
            primary["color"].as_str().ok_or("Primary text color missing")?)?;
        need(text_pixels >= 4, format!("Production {kind} primary text not painted: {text_pixels} pixels"))?;
        receipt.push(json!({"kind":kind,"utility":row["utility"],"surface":"readable-primary-text",
            "matchingPixels":text_pixels,"minimumPixels":4}));
        if native {
            let deco = &node["deco_rect"];
            let x = number(&deco["x"])? + 20;
            let y = number(&deco["y"])? + 16;
            let expected = if row["active"] == true { [255,95,87] } else { [140,140,145] };
            let actual = pixel(ctx, capture, x, y)?;
            need(expected.iter().zip(actual).all(|(left,right)| (*left-right).abs() <= 2),
                format!("Painted {kind} native close mismatch at {x},{y}: {expected:?} {actual:?}"))?;
            receipt.push(json!({"kind":kind,"utility":row["utility"],"surface":"native-close",
                "point":[x,y],"expected":expected,"actual":actual}));
        } else {
            for button in row["controls"].as_array().ok_or("Missing controls")? {
                if button["enabled"] != true { continue; }
                let bounds = &button["rect"];
                let x = origin_x + bounds["x"].as_f64().unwrap_or(0.0).round() as i64 + 2;
                let y = origin_y + (bounds["y"].as_f64().unwrap_or(0.0) + bounds["height"].as_f64().unwrap_or(0.0)/2.0).round() as i64;
                let expected = rgb(button["color"].as_str().ok_or("No control color")?)?;
                let actual = pixel(ctx, capture, x, y)?;
                need(expected.iter().zip(actual).all(|(left,right)| (*left-right).abs() <= 2),
                    format!("Painted {kind} owned control mismatch at {x},{y}: {expected:?} {actual:?}"))?;
                receipt.push(json!({"kind":kind,"utility":row["utility"],"surface":"owned-control",
                    "point":[x,y],"expected":expected,"actual":actual}));
            }
        }
        if !row["selectionRect"].is_null() {
            let bounds = &row["selectionRect"];
            let x = origin_x + (bounds["x"].as_f64().unwrap_or(0.0) + bounds["width"].as_f64().unwrap_or(0.0) - 8.0).round() as i64;
            let y = origin_y + (bounds["y"].as_f64().unwrap_or(0.0) + bounds["height"].as_f64().unwrap_or(0.0)/2.0).round() as i64;
            let expected = rgb(row["selectionFill"].as_str().ok_or("No selection color")?)?;
            let actual = pixel(ctx, capture, x, y)?;
            need(expected.iter().zip(actual).all(|(left,right)| (*left-right).abs() <= 2),
                format!("Painted {kind} selection mismatch at {x},{y}: {expected:?} {actual:?}"))?;
            receipt.push(json!({"kind":kind,"utility":row["utility"],"surface":"selection",
                "point":[x,y],"expected":expected,"actual":actual}));
        }
    }
    Ok(receipt)
}
pub fn run(ctx: &mut Context, native: bool) -> Result<()> {
    let palette_data = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    palette(ctx, &palette_data, "light", false)?;
    ctx.checked("output HEADLESS-1 resolution 2000x900; output HEADLESS-2 disable; focus output HEADLESS-1")?;
    ctx.vars.insert("LUMEN_UNIFIED_OWNED_HEADERS".into(), if native {"0"} else {"1"}.into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_CONTROLS".into(), if native {"1"} else {"0"}.into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_ACTIONS".into(), "0".into());
    ctx.vars.insert("LUMEN_FILE_SERVICE".into(), "0".into());
    ctx.vars.insert("LUMEN_LAUNCH_SCOPES".into(), "0".into());
    ctx.vars.insert("LUMEN_NEARBY_SHARING_PROVIDER".into(), "".into());
    let keyboard = path(ctx.opt.keyboard.as_ref().ok_or("Missing physical keyboard driver")?);
    let keyboard_index = ctx.launch(&keyboard, &[], "keyboard.log", true)?;
    wait("virtual keyboard ready", || Ok((fs::read_to_string(ctx.opt.output.join("keyboard.log"))?.contains("ready")).then_some(())))?;
    let folder = ctx.root.0.join("HOME/Documents");
    fs::create_dir_all(&folder)?;
    fs::write(folder.join("Private document.txt"), "Unchanged private fixture\n")?;
    let mut clients = BTreeMap::new();
    for (column, kind) in ["Files", "Settings", "Calendar"].into_iter().enumerate() {
        let qml = ctx.root.0.join(format!("{kind}.qml"));
        fs::copy(&ctx.qml, &qml)?;
        ctx.vars.insert("OWNED_INACTIVE_KIND".into(), kind.into());
        let index = ctx.launch("quickshell", &["--no-duplicate", "-p", &path(&qml)], &format!("{kind}.log"), false)?;
        clients.insert(kind, index);
        wait(&format!("{kind} fixture listing"), || Ok((ipc(ctx, kind, "ready", &[])? == "ready").then_some(())))?;
        let pid = ctx.children[index].0.id();
        let node = wait(&format!("{kind} native client"), || exact_node(ctx, kind, false, pid))?;
        ctx.checked(&format!("[con_id={}] resize set width 480 px height 650 px, move absolute position {} px 100 px", node["id"], column * 500 + 10))?;
    }
    ipc(ctx, "Files", "selectDocument", &[])?;
    wait("private document selection", || Ok((state(ctx, "Files", false)?["selectedPaths"] == json!([path(&folder.join("Private document.txt"))])).then_some(())))?;
    ipc(ctx, "Files", "openUtility", &[])?;
    let files_pid = ctx.children[clients["Files"]].0.id();
    let utility = wait("File Info native client", || exact_node(ctx, "Files", true, files_pid))?;
    ctx.checked(&format!("[con_id={}] move absolute position 1520 px 100 px", utility["id"]))?;
    let mut cases = Vec::new();
    for (mode, opaque) in [("light", false), ("dark", false), ("light", true), ("dark", true)] {
        palette(ctx, &palette_data, mode, opaque)?;
        wait("all clients use live appearance", || {
            let matching = ["Files", "Settings", "Calendar"].into_iter().all(|kind| {
                state(ctx, kind, false).is_ok_and(|value| value["dark"] == (mode == "dark") && value["opaque"] == opaque)
            });
            Ok(matching.then_some(()))
        })?;
        for (kind, is_utility) in [("Files", false), ("Settings", false), ("Calendar", false), ("Files", true)] {
            let pid = ctx.children[clients[kind]].0.id();
            let target = exact_node(ctx, kind, is_utility, pid)?.ok_or("Owned native target missing")?;
            ctx.checked(&format!("[con_id={}] focus", target["id"]))?;
            wait("actual active target", || Ok((state(ctx, kind, is_utility)?["active"] == true).then_some(())))?;
            let snapshots = [state(ctx, "Files", false)?, state(ctx, "Settings", false)?,
                state(ctx, "Calendar", false)?, state(ctx, "Files", true)?];
            need(snapshots.iter().filter(|value| value["active"] == true).count() == 1,
                format!("Owned key focus was not exclusive: {snapshots:?}"))?;
            for row in &snapshots {
                if !row["selectionActive"].is_null() {
                    need(row["selectionActive"] == row["active"], format!("Inactive selection mismatch: {row}"))?;
                }
                need(row["selectedPage"] == "Appearance", "Settings page changed during focus")?;
                if row["kind"] == "Files" {
                    need(row["selectedPaths"] == json!([path(&folder.join("Private document.txt"))]),
                        "Selected file changed during focus")?;
                }
                if !native {
                    let expected = if row["active"] == true {
                        ["#ff5f57", "#febc2e", "#28c840"].to_vec()
                    } else if mode == "dark" { vec!["#555559"] } else { vec!["#ceced2"] };
                    for button in row["controls"].as_array().ok_or("Controls were not an array")? {
                        if button["enabled"] == true {
                            need(button["color"].as_str().is_some_and(|color| expected.contains(&color)),
                                format!("Owned control inactive paint mismatch: {button}"))?;
                        }
                    }
                }
            }
            if is_utility {
                need(snapshots[3]["context"] == json!({"keyMain":false,"keyUtility":true,"mainFiles":true,"application":"lumen-files","role":"utility"}),
                    format!("Utility key context mismatch: {}", snapshots[3]))?;
            }
            ipc(ctx, kind, "focusControl", &[if is_utility {"true"} else {"false"}])?;
            let before = wait("production focused field", || {
                let focus = state(ctx, kind, is_utility)?["focus"].clone();
                Ok(focus["name"].as_str().is_some_and(|name| !name.is_empty()).then_some(focus))
            })?;
            ctx.write(keyboard_index, "k 1 15 1\nk 2 15 0\n")?;
            save(&ctx.opt.output.join("tab-before.json"), &json!({"kind":kind,"utility":is_utility,"focus":before}))?;
            let after = wait("physical Tab changes exact recipient", || {
                let row = state(ctx, kind, is_utility)?;
                let _ = save(&ctx.opt.output.join("tab-after.json"), &row);
                let focus = row["focus"].clone();
                Ok((row["active"] == true && focus["name"].as_str().is_some_and(|name| !name.is_empty())
                    && focus["name"] != before["name"]).then_some(focus))
            })?;
            let name = format!("{mode}-{}-{kind}{}.png", if opaque {"opaque"} else {"material"}, if is_utility {"-utility"} else {""});
            ctx.picture(&name)?;
            let paint = painted(ctx, &ctx.opt.output.join(&name), &snapshots, &clients, native)?;
            cases.push(json!({"mode":mode,"opaque":opaque,"kind":kind,"utility":is_utility,
                "pid":pid,"conId":target["id"],"states":snapshots,"focusBefore":before,"focusAfter":after,"painted":paint}));
        }
    }
    need(fs::read_to_string(folder.join("Private document.txt"))? == "Unchanged private fixture\n",
        "Private document contents changed")?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"frame":if native {"native"} else {"unified"},
        "cases":cases,"independentClients":3,"utilityOwner":"Files",
        "capabilities":{"independentOwnedClients":true,"utilityContextAndSelection":true,
            "liveInactiveSelections":true,"physicalTabRecipient":true,"compositorCapture":true}}))?;
    Ok(())
}
