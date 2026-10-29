//! Production essential dialogs under private native basic and effects compositors.
use super::*;
use super::physical_keyboard::Keyboard;

pub fn provider() -> Result<()> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line)?;
    let action = request["action"].as_str().ok_or("Missing provider action")?;
    let name = std::env::args_os().next().and_then(|p| PathBuf::from(p).file_name().map(|n| n.to_owned()))
        .ok_or("Missing provider executable name")?;
    let result = if action != "status" {
        if let Ok(path) = std::env::var("LUMEN_ESSENTIAL_MUTATIONS") {
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            writeln!(file, "{}", request)?;
        }
        json!({"error":"Private fixture refuses mutations"})
    } else if name == "lumen-window-pair" {
        json!({"source":{"id":101,"pid":501,"title":"Private captured document with a long descriptive title",
            "output":"HEADLESS-1","pair":{"id":0,"eligible":true,"survivor":false}},
            "candidates":[{"id":102,"pid":502,"title":"Compatible private partner with a descriptive document title",
                "app":"Private application"}]})
    } else if name == "lumen-display-settings" {
        json!({"outputs":[{"name":"HEADLESS-1","label":"Private display","mode":"700x480@60",
            "modes":["700x480@60"],"scale":1,"x":0,"y":0}],"pending":"","seconds":0})
    } else { return Err(fail("Unknown essential dialog provider")); };
    println!("{result}");
    Ok(())
}

fn state(ctx: &Context) -> Result<Value> { Ok(serde_json::from_str(&ctx.ipc("state", &[])?)?) }
fn focused(ctx: &Context, name: &str) -> Result<()> {
    wait(name, || Ok((state(ctx)?["focus"] == name).then_some(())))
}
fn bounded(row: &Value) -> Result<()> {
    let rect = &row["rect"];
    let (x,y,w,h) = (rect["x"].as_f64().ok_or("Focus x")?, rect["y"].as_f64().ok_or("Focus y")?,
        rect["width"].as_f64().ok_or("Focus width")?, rect["height"].as_f64().ok_or("Focus height")?);
    let (width,height) = (row["width"].as_f64().ok_or("Owner width")?, row["height"].as_f64().ok_or("Owner height")?);
    need(x >= -1.0 && y >= -1.0 && w > 0.0 && h > 0.0 && x+w <= width+1.0 && y+h <= height+1.0,
        format!("Essential dialog focus clipped: {row}"))
}
fn label(row: &Value, prefix: &str, size: i64) -> Result<()> {
    let item = row["labels"].as_array().and_then(|labels| labels.iter().find(|item|
        item["text"].as_str().is_some_and(|text| text.starts_with(prefix))))
        .ok_or_else(|| fail(format!("Missing production label {prefix}: {row}")))?;
    need(item["size"] == size && item["height"].as_f64().unwrap_or(0.0)+1.0 >= item["implicitHeight"].as_f64().unwrap_or(f64::INFINITY),
        format!("Essential dialog text role or wrap failed for {prefix}: {item}"))
}

pub fn run(ctx: &mut Context) -> Result<()> {
    for (key,value) in [("LUMEN_UNIFIED_OWNED_HEADERS","1"),("LUMEN_NATIVE_WINDOW_CONTROLS","0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS","0"),("LUMEN_FILE_SERVICE","0"),("LUMEN_LAUNCH_SCOPES","0")] {
        ctx.vars.insert(key.into(), value.into());
    }
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    let qml = fs::read_to_string(&ctx.qml)?.replace("__PALETTES__", &serde_json::to_string(&palette)?);
    fs::write(&ctx.qml, qml)?;
    let mutations = ctx.opt.output.join("mutations.jsonl");
    ctx.vars.insert("LUMEN_ESSENTIAL_MUTATIONS".into(), path(&mutations));
    let bin = ctx.root.0.join("bin"); fs::create_dir(&bin)?;
    let executable = std::env::current_exe()?;
    for name in ["lumen-window-pair", "lumen-display-settings"] { symlink(&executable, bin.join(name))?; }
    ctx.vars.insert("PATH".into(), format!("{}:{}", bin.display(), ctx.vars.get("PATH").cloned().unwrap_or_default()));
    let qml = path(&ctx.qml);
    ctx.launch("quickshell", &["--no-duplicate","-p",&qml], "essential-dialog-shell.log", false)?;
    let mut keyboard = Keyboard::start(ctx, "essential-dialog-keyboard.log")?;
    wait("essential dialog IPC", || Ok((ctx.ipc("state", &[])? == "{}").then_some(())))?;
    let mut cases = Vec::new();
    for mode in ["light", "dark"] {
        ctx.ipc("open", &["Force",mode])?;
        focused(ctx,"Close")?;
        let row=state(ctx)?;
        need(row["visible"]==true && row["scale"]==1.5 && row["phase"]=="error", format!("Force state: {row}"))?;
        bounded(&row)?;
        label(&row,"Force Quit Private",26)?;
        label(&row,"The captured application changed",20)?;
        ctx.picture(&format!("Force-{mode}.png"))?;
        keyboard.key("Return")?;
        wait("Force Return closes without confirmation",||Ok((state(ctx)?["visible"]==false).then_some(())))?;
        cases.push(json!({"mode":mode,"consumer":"Force","focus":row}));

        ctx.ipc("open", &["Pair",mode])?;
        wait("Pair private status",||Ok((state(ctx)?["pairReady"]==true).then_some(())))?;
        let candidate="Compatible private partner with a descriptive document title · Private application";
        ctx.ipc("focus", &[candidate])?; focused(ctx,candidate)?;
        keyboard.key("space")?;
        wait("Pair physical selection",||Ok((state(ctx)?["selected"]==0).then_some(())))?;
        ctx.ipc("focus", &["Cancel"])?; focused(ctx,"Cancel")?;
        let row=state(ctx)?; bounded(&row)?;
        label(&row,"Choose a compatible partner",18)?;
        ctx.picture(&format!("Pair-{mode}.png"))?;
        keyboard.key("Return")?;
        wait("Pair Return cancels without dispatch",||Ok((state(ctx)?["visible"]==false).then_some(())))?;
        cases.push(json!({"mode":mode,"consumer":"Pair","focus":row}));

        ctx.ipc("open", &["Displays",mode])?;
        wait("Display active",||Ok((state(ctx)?["active"]==true).then_some(())))?;
        ctx.ipc("focus", &["Display x position"])?; focused(ctx,"Display x position")?;
        ctx.call("wtype", &["-M","ctrl","-k","a","-m","ctrl"])?;
        ctx.call("wtype", &["12"])?;
        wait("Display physical edit",||Ok((state(ctx)?["positionX"]=="12").then_some(())))?;
        ctx.ipc("focus", &["Forget Saved Display Layout"])?;
        focused(ctx,"Forget Saved Display Layout")?;
        let row=state(ctx)?; bounded(&row)?;
        label(&row,"Select a display, then drag",17)?;
        label(&row,"Resolution and refresh rate",18)?;
        label(&row,"Confirmed settings update",17)?;
        ctx.picture(&format!("Displays-{mode}.png"))?;
        keyboard.key("Escape")?;
        cases.push(json!({"mode":mode,"consumer":"Displays","focus":row}));
    }
    need(!mutations.exists(), format!("Essential dialog provider mutation: {}", fs::read_to_string(&mutations).unwrap_or_default()))?;
    let log=fs::read_to_string(ctx.opt.output.join("essential-dialog-shell.log"))?;
    need(!["ReferenceError","TypeError","binding loop"].iter().any(|marker|log.contains(marker)), format!("Essential dialog QML diagnostic: {log}"))?;
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"effects":ctx.opt.effects,"cases":cases,
        "scope":"Production native Force, Pair and Display controls under both palettes with physical Return and private read-only providers"}))?;
    println!("PASS essential dialogs: {} native cases",cases.len());
    Ok(())
}
