//! Live desktop captions, layout persistence, and physical input at enlarged text.
use super::*;
use std::os::unix::fs::MetadataExt;

const NAMES: [&str; 4] = [
    "0 Private document.txt",
    "1 Private image.png",
    "2 Another private document.txt",
    "3 Another private image.png",
];

fn state(ctx: &Context) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.ipc("status", &[])?)?)
}

fn item(ctx: &Context, name: &str) -> Result<Value> {
    let target = path(&ctx.root.0.join("HOME/Desktop").join(name));
    state(ctx)?["items"].as_array().ok_or("Missing desktop items")?.iter()
        .find(|row| row["path"] == target).cloned().ok_or_else(|| fail(format!("Missing desktop item {name}")))
}

fn click(ctx: &mut Context, row: &Value) -> Result<()> {
    let rect = &row["rect"];
    let x = rect["x"].as_f64().ok_or("Missing desktop item x")? + rect["width"].as_f64().ok_or("Missing item width")? / 2.0;
    let y = rect["y"].as_f64().ok_or("Missing desktop item y")? + 25.0;
    ctx.move_pointer((x, y))?;
    ctx.press()?;
    ctx.release()?;
    Ok(())
}

fn painted_caption(ctx: &Context, capture: &Path, caption: &Value) -> Result<bool> {
    let color = caption["color"].as_str().ok_or("Missing caption ink")?;
    need(color.len() == 7 && color.starts_with('#'), "Invalid caption ink")?;
    let ink = [u8::from_str_radix(&color[1..3], 16)?, u8::from_str_radix(&color[3..5], 16)?, u8::from_str_radix(&color[5..7], 16)?];
    let rect = &caption["rect"];
    let x = rect["x"].as_f64().ok_or("Missing caption x")?.floor() as i64;
    let y = rect["y"].as_f64().ok_or("Missing caption y")?.floor() as i64;
    let width = (rect["width"].as_f64().ok_or("Missing caption width")? + 1.0) as i64;
    let height = (rect["height"].as_f64().ok_or("Missing caption height")? + 1.0) as i64;
    let histogram = ctx.call("magick", &[&path(capture), "-crop", &format!("{width}x{height}+{x}+{y}"), "+repage", "-format", "%c", "histogram:info:"])?;
    let mut pixels = 0usize;
    for line in histogram.lines() {
        let Some((count, rest)) = line.trim().split_once(':') else { continue };
        let Some((_, tuple)) = rest.split_once('(') else { continue };
        let Some((tuple, _)) = tuple.split_once(')') else { continue };
        let channels = tuple.split(',').take(3).map(|part| part.trim().parse::<i32>()).collect::<std::result::Result<Vec<_>, _>>()?;
        if channels.len() == 3 && channels.iter().enumerate().all(|(index, value)| (value - ink[index] as i32).abs() < 30) {
            pixels += count.trim().parse::<usize>()?;
        }
    }
    Ok(pixels >= 10)
}

pub fn prepare(ctx: &mut Context, palette: &Value) -> Result<()> {
    let desktop = ctx.root.0.join("HOME/Desktop");
    fs::create_dir(&desktop)?;
    for (index, name) in NAMES.iter().enumerate() {
        let target = desktop.join(name);
        if name.ends_with(".png") {
            ctx.call("magick", &["-size", "8x8", &format!("xc:rgb({},{},{})", 20 + index * 20, 80, 120), &path(&target)])?;
        } else {
            fs::write(target, format!("Disposable private desktop text {index}"))?;
        }
    }
    fs::write(ctx.root.0.join("XDG_CONFIG_HOME/user-dirs.dirs"), format!("XDG_DESKTOP_DIR=\"{}\"\n", desktop.display()))?;
    ctx.vars.insert("LUMEN_FIXTURE_PALETTES".into(), serde_json::to_string(palette)?);
    let identities = NAMES.iter().map(|name| {
        let facts = fs::metadata(desktop.join(name))?;
        Ok::<_, Box<dyn Error>>(format!("{}:{}", facts.dev(), facts.ino()))
    }).collect::<Result<Vec<_>>>()?;
    let positions = [[0.8, 0.2], [0.8, 0.7], [0.2, 0.2], [0.2, 0.7]];
    let layout = identities.iter().zip(positions).map(|(key, position)| (key.clone(), json!(position))).collect::<serde_json::Map<_, _>>();
    let saved = ctx.input("lumen-desktop-layout", &json!({"action":"save","directory":desktop,"items":identities,"layout":{"grouping":"none","positions":layout}}))?;
    need(saved.get("error").is_none(), format!("Desktop arrangement setup failed: {saved}"))?;
    Ok(())
}

pub fn run(ctx: &mut Context) -> Result<()> {
    ctx.vars.insert("LUMEN_UNIFIED_OWNED_HEADERS".into(), "1".into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_CONTROLS".into(), "0".into());
    ctx.vars.insert("LUMEN_NATIVE_WINDOW_ACTIONS".into(), "0".into());
    ctx.vars.insert("LUMEN_LAUNCH_SCOPES".into(), "0".into());
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    let original = NAMES.iter().map(|name| Ok::<_, Box<dyn Error>>(((*name).to_owned(), hash(&ctx.root.0.join("HOME/Desktop").join(name))?))).collect::<Result<BTreeMap<_, _>>>()?;
    let qml = path(&ctx.qml);
    let shell = ctx.launch("quickshell", &["-p", &qml], "desktop-typography-shell.log", false)?;
    ctx.launch("wtype", &["-s", "60000"], "desktop-typography-keyboard.log", false)?;
    ctx.pointer_start()?;
    wait("desktop typography IPC", || Ok((ctx.ipc("ready", &[])? == "true").then_some(())))?;
    wait("four real desktop files", || {
        let row = state(ctx)?;
        Ok((row["listingComplete"] == true && row["count"] == 4 && row["layout"]["positions"].as_object().is_some_and(|items| items.len() == 4)).then_some(()))
    })?;
    let baseline = state(ctx)?;
    need(baseline["items"].as_array().is_some_and(|items| items.iter().filter(|item| item["overflow"] == false).all(|item| item["caption"]["resolvedFamily"].as_str().is_some_and(|font| !font.is_empty())
        && (ctx.opt.font_config.is_none() || item["caption"]["resolvedFamily"] == palette["font"]))), "Desktop caption font missing")?;
    let mut checks = Vec::new();
    click(ctx, &item(ctx, NAMES[2])?)?;
    wait("desktop pointer selection", || Ok((state(ctx)?["currentPath"] == path(&ctx.root.0.join("HOME/Desktop").join(NAMES[2]))).then_some(())))?;
    let before = state(ctx)?;
    ctx.key(&["-k", "Right"])?;
    ctx.key(&["-k", "Down"])?;
    wait("physical desktop spatial navigation", || Ok((state(ctx)?["currentPath"] == path(&ctx.root.0.join("HOME/Desktop").join(NAMES[1]))).then_some(())))?;
    checks.push(json!({"case":"pointer-and-spatial-keyboard","before":before,"after":state(ctx)?}));
    let desktop = ctx.root.0.join("HOME/Desktop");
    ctx.ipc("navigate", &[&path(&desktop.join("missing-folder"))])?;
    wait("failed desktop listing", || Ok((state(ctx)?["error"].as_str().is_some_and(|v| !v.is_empty()) && state(ctx)?["listingCurrent"] == false).then_some(())))?;
    need(state(ctx)?["layout"]["positions"] == baseline["layout"]["positions"], "Failed listing discarded desktop arrangement")?;
    ctx.ipc("navigate", &[&path(&desktop)])?;
    wait("desktop listing recovery", || Ok((state(ctx)?["listingComplete"] == true && state(ctx)?["count"] == 4).then_some(())))?;
    checks.push(json!({"case":"failed-list-retains-layout","state":state(ctx)?}));
    for dimension in ["1200x900", "700x600"] {
        ctx.checked(&format!("output HEADLESS-1 resolution {dimension}"))?;
        save(&ctx.opt.output.join(format!("state-after-{dimension}.json")), &state(ctx)?)?;
        let width: i64 = dimension.split('x').next().ok_or("Missing resolution")?.parse()?;
        wait("desktop resize", || Ok((state(ctx)?["width"] == width).then_some(())))?;
        for mode in ["light", "dark"] {
            let mut baseline_widths = BTreeMap::<String, f64>::new();
            for scale in [1.0, 1.15, 1.3, 1.5] {
                ctx.ipc("configure", &[mode, &scale.to_string()])?;
                let expected = (11.0 * scale + 0.5) as i64;
                wait("desktop caption scaling", || {
                    let row = state(ctx)?;
                    Ok((row["items"].as_array().is_some_and(|items| items.iter().filter(|item| item["overflow"] == false).all(|item| item["caption"]["size"] == expected))).then_some(()))
                })?;
                let row = state(ctx)?;
                need(row["items"].as_array().is_some_and(|items| items.iter().filter(|item| item["overflow"] == false).all(|item| item["icon"]["width"] == 46 && item["icon"]["height"] == 46)), "Desktop icon optical size changed")?;
                for item in row["items"].as_array().ok_or("Missing desktop items")?.iter().filter(|item| item["overflow"] == false) {
                    let key = item["key"].as_str().ok_or("Missing desktop key")?.to_owned();
                    let caption = &item["caption"];
                    let ratio = caption["naturalWidth"].as_f64().ok_or("Missing natural caption width")?
                        / caption["rect"]["width"].as_f64().ok_or("Missing caption width")?;
                    if scale == 1.0 {
                        baseline_widths.insert(key, ratio);
                    } else {
                        need(ratio <= baseline_widths.get(&key).copied().ok_or("Missing baseline caption")? + 0.015,
                            format!("Caption capacity narrowed: {dimension} {mode} {scale} {key}"))?;
                    }
                }
                let capture = format!("desktop-{dimension}-{mode}-{scale}.png");
                ctx.picture(&capture)?;
                for item in row["items"].as_array().ok_or("Missing desktop items")?.iter().filter(|item| item["overflow"] == false) {
                    need(painted_caption(ctx, &ctx.opt.output.join(&capture), &item["caption"])?
                        , format!("Desktop caption not painted: {dimension} {mode} {scale}: {}", item["path"]))?;
                }
                checks.push(json!({"mode":mode,"scale":scale,"dimension":dimension,"state":row,"capture":capture}));
            }
        }
    }
    let target = item(ctx, NAMES[0])?;
    let rect = &target["caption"]["rect"];
    let background_clicks = state(ctx)?["backgroundClicks"].clone();
    let x = rect["x"].as_f64().ok_or("Missing caption x")? + 120.0;
    let y = rect["y"].as_f64().ok_or("Missing caption y")? + 10.0;
    ctx.move_pointer((x, y))?;
    ctx.press()?;
    ctx.release()?;
    wait("expanded desktop caption hit target", || Ok((state(ctx)?["currentPath"] == path(&desktop.join(NAMES[0]))).then_some(())))?;
    need(state(ctx)?["backgroundClicks"] == background_clicks, "Caption hit leaked to background")?;
    ctx.key(&["-k", "space"])?;
    wait("captured desktop Quick Look", || Ok((state(ctx)?["quickLooks"] == json!([path(&desktop.join(NAMES[0]))])).then_some(())))?;
    checks.push(json!({"case":"expanded-caption-mask-and-quicklook","state":state(ctx)?}));
    ctx.ipc("group", &["kind"])?;
    wait("desktop kind stack", || Ok((state(ctx)?["items"].as_array().is_some_and(|rows| rows.iter().any(|row| row["stack"] == true))).then_some(())))?;
    let stack = state(ctx)?["items"].as_array().ok_or("Missing stacked items")?.iter().find(|row| row["stack"] == true && row["overflow"] == false).cloned().ok_or("Missing visible stack")?;
    click(ctx, &stack)?;
    wait("desktop stack focus", || Ok((state(ctx)?["currentKey"] == stack["key"]).then_some(())))?;
    ctx.key(&["-k", "space"])?;
    wait("physical stack collapse", || Ok((state(ctx)?["items"].as_array().is_some_and(|rows| rows.len() == 2)).then_some(())))?;
    need(state(ctx)?["currentPath"] == "" && state(ctx)?["selected"] == json!([]), "Stack collapse triggered file action")?;
    checks.push(json!({"case":"stack-header-space-collapse","state":state(ctx)?}));
    ctx.ipc("group", &["none"])?;
    wait("desktop ungrouped", || Ok((state(ctx)?["count"] == 4 && state(ctx)?["layout"]["grouping"] == "none").then_some(())))?;
    ctx.children[shell].stop(libc::SIGTERM)?;
    ctx.launch("quickshell", &["-p", &qml], "desktop-typography-shell-restart.log", false)?;
    wait("desktop fixture restart", || Ok((ctx.ipc("ready", &[])? == "true").then_some(())))?;
    wait("durable desktop arrangement after restart", || Ok((state(ctx)?["listingComplete"] == true
        && state(ctx)?["count"] == 4
        && state(ctx)?["layout"]["positions"] == baseline["layout"]["positions"]).then_some(())))?;
    checks.push(json!({"case":"restart-retains-placement-identities","state":state(ctx)?}));
    for (name, digest) in original { need(hash(&ctx.root.0.join("HOME/Desktop").join(name))? == digest, "Desktop fixture file changed")?; }
    let log = fs::read_to_string(ctx.opt.output.join("desktop-typography-shell.log"))?
        + &fs::read_to_string(ctx.opt.output.join("desktop-typography-shell-restart.log"))?;
    need(!["ReferenceError", "TypeError", "binding loop", "Failed to load configuration"].iter().any(|v| log.contains(v)), format!("Desktop QML warning: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":checks,"baseline":baseline,"configuredFont":palette["font"]}))?;
    println!("PASS live desktop typography: {} size, mode, and scale cases", checks.len());
    Ok(())
}
