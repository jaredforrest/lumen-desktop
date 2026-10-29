//! Physical Dock motion, tooltip, and persisted reorder acceptance.
use super::*;

fn state(ctx: &Context) -> Result<Value> {
    ctx.ui()
}
fn keys(row: &Value) -> Result<Vec<String>> {
    Ok(row["keys"].as_array().ok_or("Missing Dock keys")?.iter()
        .map(|v| v.as_str().unwrap_or("").to_owned()).collect())
}
fn index(row: &Value, key: &str) -> Result<usize> {
    keys(row)?.iter().position(|item| item == key).ok_or_else(|| fail(format!("Missing Dock key {key}")))
}
fn center(ctx: &Context, key: &str) -> Result<(f64, f64)> {
    let row = state(ctx)?;
    let i = index(&row, key)? as f64;
    Ok(((1280.0 - row["width"].as_f64().ok_or("Dock width")?)/2.0 + 12.0
        + (i + 0.5)*row["slot"].as_f64().ok_or("Dock slot")?,
        900.0 - 7.0 - row["shelf"].as_f64().ok_or("Dock shelf")?/2.0))
}
fn settled(row: &Value) -> bool {
    row["tiles"].as_array().is_some_and(|tiles| tiles.iter().all(|tile| {
        let (Some(width), Some(base), Some(mag)) = (tile["width"].as_f64(),tile["base"].as_f64(),tile["magnification"].as_f64()) else { return false };
        (width-base-18.0*mag).abs()<0.15
    }))
}
fn writes(ctx: &Context) -> Result<Vec<Value>> {
    let file = ctx.opt.output.join("writes.jsonl");
    if !file.exists() { return Ok(vec![]) }
    fs::read_to_string(file)?.lines().map(|line| Ok(serde_json::from_str(line)?)).collect()
}
fn point(ctx: &mut Context, key: &str) -> Result<Value> {
    let location = center(ctx,key)?;
    ctx.move_pointer(location)?;
    wait("Dock hover",|| {
        let row=state(ctx)?;
        Ok((row["hovered"].as_i64()==Some(index(&row,key)? as i64) && row["contains"]==true).then_some(row))
    })
}
fn drag(ctx: &mut Context, source: &str, target: &str) -> Result<()> {
    point(ctx,source)?;
    ctx.press()?;
    wait("Dock held pointer", || Ok((state(ctx)?["pressed"]==true).then_some(())))?;
    let location = center(ctx,target)?;
    ctx.move_pointer(location)?;
    wait("Dock drag target", || {
        let row=state(ctx)?;
        Ok((row["dragging"]==true && row["hovered"].as_i64()==Some(index(&row,target)? as i64)).then_some(()))
    })?;
    Ok(())
}
fn pins(row: &Value) -> Result<Vec<String>> {
    Ok(row["pins"].as_array().ok_or("Missing pins")?.iter().map(|v|v.as_str().unwrap_or("").into()).collect())
}
fn move_continuous(ctx: &mut Context, location: (f64,f64), delay_ms: u64) -> Result<()> {
    let steps=(((location.0-ctx.position.0).abs()/8.0).floor() as usize).max(1);
    let dx=(location.0-ctx.position.0)/steps as f64;
    let dy=(location.1-ctx.position.1)/steps as f64;
    for _ in 0..steps {
        ctx.write(ctx.pointer.ok_or("Pointer missing")?, &format!("m {dx:.5} {dy:.5}\n"))?;
        ctx.position.0+=dx; ctx.position.1+=dy;
        pause(delay_ms);
    }
    Ok(())
}
pub fn run(ctx: &mut Context) -> Result<()> {
    for (key,value) in [("LUMEN_LAUNCH_SCOPES","0"),("DBUS_SYSTEM_BUS_ADDRESS","unix:path=/nonexistent"),("PULSE_SERVER","unix:/nonexistent")] {
        ctx.vars.insert(key.into(),value.into());
    }
    ctx.checked("output HEADLESS-1 resolution 1280x900 position 0 0")?;
    ctx.checked("output HEADLESS-2 disable")?;
    save(&ctx.root.0.join("settings.json"),&json!({}))?;
    ctx.vars.insert("LUMEN_NIX_SETTINGS_ACTIVE".into(),path(&ctx.root.0.join("settings.json")));
    ctx.vars.insert("LUMEN_DOCK_MOTION_STORE".into(),path(&ctx.root.0.join("preferences.json")));
    ctx.vars.insert("LUMEN_DOCK_MOTION_WRITES".into(),path(&ctx.opt.output.join("writes.jsonl")));
    let bin=ctx.root.0.join("bin"); fs::create_dir(&bin)?;
    let exe=env::current_exe()?.parent().ok_or("Fixture binary directory")?.join("dock_motion_settings");
    need(exe.is_file(),format!("Missing Rust settings fixture {}",exe.display()))?;
    symlink(exe,bin.join("lumen-appearance"))?;
    let prior=ctx.vars.get("PATH").cloned().unwrap_or_default();
    ctx.vars.insert("PATH".into(),format!("{}:{prior}",bin.display()));
    let palette=load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(&ctx.root.0.join("appearance.json"),&json!({"font":palette["font"],"palette":palette["dark"],
        "preferences":{"dockSize":48,"dockMagnification":true,"reduceMotion":false,"reduceTransparency":true}}))?;
    let target=ctx.root.0.join("assets/shell/Dock.qml");
    let original=fs::read_to_string(&target)?;
    let needle="    id: dock";
    let tooltip="        screen:dock.screen;visible:dock.displayAllowed && dock.hovered>=0";
    need(original.matches(needle).count()==1 && original.matches(tooltip).count()==1,"Dock instrumentation anchors changed")?;
    let addition=r#"
    property alias fixturePointer:pointerArea
    property alias fixtureTip:tip
    property alias fixtureTipWindow:fixtureTooltipWindow
    function fixtureTiles(){
        const result=[];
        function visit(item){if(item.magnification!==undefined && item.modelData && item.index!==undefined){const icon=item.children.find(child=>child.iconSource!==undefined);result.push({key:item.modelData.key,index:item.index,magnification:item.magnification,width:icon.width,base:item.modelData.canvas*dock.artworkScale});}for(const child of item.children || [])visit(child);}
        visit(surface);return result.sort((a,b)=>a.index-b.index);
    }
"#;
    let modified=original.replacen(needle,&format!("{needle}{addition}"),1).replacen(tooltip,&format!("        id:fixtureTooltipWindow\n{tooltip}"),1);
    fs::write(&target,&modified)?;
    save(&ctx.opt.output.join("instrumentation.json"),&json!({
        "originalSha256":format!("{:x}",Sha256::digest(original.as_bytes())),
        "modifiedSha256":format!("{:x}",Sha256::digest(modified.as_bytes())),
        "roundtrip":modified.replacen(addition,"",1).replacen("        id:fixtureTooltipWindow\n","",1)==original,
        "scope":"Read-only pointer/tooltip aliases, tile geometry collector, and tooltip id only"}))?;
    let qml=path(&ctx.qml);
    let mut shell=ctx.launch("quickshell", &["-p",&qml],"shell.log",false)?;
    ctx.pointer_start()?;
    wait("Dock readiness",|| {
        let row=state(ctx)?;
        Ok((row["ready"]==true && row["busy"]==false && row["tiles"].as_array().is_some_and(|t|t.len()==keys(&row).unwrap_or_default().len())).then_some(()))
    })?;
    let initial=state(ctx)?; let all=keys(&initial)?;
    need(initial["separators"][index(&initial,"fixture-recent")?]==true &&
        initial["separators"][index(&initial,"folder:/synthetic/folder")?]==true,"Dock section separators missing")?;
    let mut sweeps=vec![];
    for (delay,reverse) in [(40,false),(16,true),(4,false)] {
        let mut order=all.clone(); if reverse {order.reverse()};
        point(ctx,&order[0])?;
        let mut samples=vec![];
        for key in order.iter().skip(1) {
            move_continuous(ctx,center(ctx,key)?,delay)?;
            let row=wait("Dock neighbour tooltip",|| {
                let row=state(ctx)?;
                Ok((row["hovered"].as_i64()==Some(index(&row,key)? as i64) && row["tipVisible"]==true && row["tip"]==*key).then_some(row))
            })?;
            samples.push(json!({"key":key,"hovered":row["hovered"],"pointerSlot":row["pointerSlot"],"tip":row["tip"],"tiles":row["tiles"]}));
        }
        let row=wait("Dock endpoint settlement",|| {let row=state(ctx)?; Ok(settled(&row).then_some(row))})?;
        need(row["hovered"].as_i64()==Some(index(&row,order.last().ok_or("No Dock items")?)? as i64) &&
            (row["pointerSlot"].as_f64().ok_or("Pointer slot")?-row["hovered"].as_f64().ok_or("Hover")?).abs()<0.03,
            "Dock endpoint did not settle")?;
        sweeps.push(json!({"stepDelayMs":delay,"reverse":reverse,"samples":samples,"settled":row}));
        ctx.move_pointer((30.0,400.0))?;
        wait("Dock hover clear",|| {let row=state(ctx)?; Ok((row["hovered"]==-1 && row["pointerSlot"]==-1 && row["tipVisible"]==false && row["tip"]=="").then_some(()))})?;
        let row=wait("Dock tile reset",|| {let row=state(ctx)?; Ok(settled(&row).then_some(row))})?;
        need(row["tiles"].as_array().ok_or("Tiles")?.iter().all(|t|(t["width"].as_f64().unwrap_or(0.)-t["base"].as_f64().unwrap_or(0.)).abs()<0.15),"Dock tile did not reset")?;
    }
    need(writes(ctx)?.is_empty() && state(ctx)?["activations"].as_array().is_some_and(Vec::is_empty),"Motion mutated Dock")?;
    let mut checks=vec!["Three continuous sweeps vary motion rate, neighbour tooltips and endpoint magnification, then clear without actions"];
    drag(ctx,"a","c")?; ctx.release()?;
    let mut expected=vec!["fixture-b","fixture-a","fixture-c","unknown-retained"];
    wait("Dock pin reorder",|| Ok((pins(&state(ctx)?)?==expected && state(ctx)?["busy"]==false).then_some(())))?;
    need(writes(ctx)?.len()==1 && writes(ctx)?[0]==json!({"key":"dockPins","value":expected}) && state(ctx)?["activations"].as_array().is_some_and(Vec::is_empty),"First Dock write invalid")?;
    checks.push("Physical app reorder emits one captured write and preserves unknown pins");
    ctx.children[shell].stop(libc::SIGTERM)?;
    ctx.move_pointer((30.0,400.0))?;
    shell=ctx.launch("quickshell", &["-p",&qml],"shell-restart.log",false)?;
    let _=shell;
    wait("Dock restart",|| Ok((state(ctx)?["ready"]==true && state(ctx)?["busy"]==false).then_some(())))?;
    need(pins(&state(ctx)?)?==expected,"Dock restart lost order")?;
    checks.push("Production Dock and SettingsState reconstruct captured order after restart");
    drag(ctx,"c","folder:/synthetic/folder")?; ctx.release()?;
    wait("Dock invalid drop",|| Ok((state(ctx)?["dragging"]==false).then_some(())))?;
    need(writes(ctx)?.len()==1 && pins(&state(ctx)?)?==expected && state(ctx)?["activations"].as_array().is_some_and(Vec::is_empty),"Document drop mutated Dock")?;
    checks.push("Document section drop does not reorder, remove, activate, or retarget");
    drag(ctx,"c","b")?; ctx.release()?;
    expected=vec!["fixture-c","fixture-b","fixture-a","unknown-retained"];
    wait("Dock reverse reorder",|| Ok((pins(&state(ctx)?)?==expected && state(ctx)?["busy"]==false).then_some(())))?;
    need(writes(ctx)?.len()==2 && writes(ctx)?[1]["value"]==json!(expected),"Reverse Dock write invalid")?;
    checks.push("Reverse physical reorder captures the reconstructed source and destination");
    ctx.picture("accepted.png")?;
    for file in ["shell.log","shell-restart.log"] {
        let log=fs::read_to_string(ctx.opt.output.join(file))?;
        need(!["ReferenceError","TypeError","Binding loop","Failed to load"].iter().any(|marker|log.contains(marker)),format!("Dock QML errors in {file}: {log}"))?;
    }
    save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"checks":checks,"sweeps":sweeps,"writes":writes(ctx)?,
        "final":state(ctx)?,"settingsScope":"Synthetic private settings transport with production Dock, SettingsState and model",
        "liveSocketUnchanged":env::var("SWAYSOCK").ok()==ctx.original_socket}))?;
    println!("PASS physical Dock motion and persisted reorder");
    Ok(())
}
