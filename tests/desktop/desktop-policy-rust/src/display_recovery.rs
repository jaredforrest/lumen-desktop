use super::*;

fn spaces(ctx: &Context) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.call("lumen-spaces", &["status"])?)?)
}
fn ui(ctx: &Context) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.ipc("status", &[])?)?)
}
fn visible_names(value: &Value) -> Vec<String> {
    let mut names=value["docks"].as_array().into_iter().flatten().filter(|dock|dock["visible"]==true)
        .filter_map(|dock|dock["name"].as_str().map(ToOwned::to_owned)).collect::<Vec<_>>();
    names.sort();names
}
fn wait_ui(ctx: &Context,label:&str,mut check:impl FnMut(&Value)->bool)->Result<Value> {
    wait(label,||{
        let value=ui(ctx)?;
        Ok(check(&value).then_some(value))
    })
}
fn keyboard(ctx:&Context,output:&str)->Result<()> {
    ctx.checked(&format!("bindsym Mod4+d exec quickshell -p {} ipc call displaySpaceRecovery focus",path(&ctx.qml)))?;
    ctx.key(&["-M","logo","-k","d","-m","logo"])?;
    wait_ui(ctx,"keyboard Dock focus",|value|value["docks"].as_array().is_some_and(|docks|
        docks.iter().any(|dock|dock["name"]==output && dock["keyboard"]==true && dock["active"]==true)))?;
    ctx.key(&["-k","Escape"])?;
    wait_ui(ctx,"keyboard Dock release",|value|value["docks"].as_array().is_some_and(|docks|
        docks.iter().all(|dock|dock["keyboard"]==false)))?;
    Ok(())
}
pub fn run(ctx: &mut Context,legacy: bool) -> Result<()> {
    let settings=ctx.root.0.join("settings.json");
    let captures=ctx.root.0.join("display-settings-actions.jsonl");
    ctx.vars.insert("LUMEN_DISPLAY_SETTINGS_CAPTURE".into(),path(&captures));
    ctx.vars.insert("LUMEN_NIX_SETTINGS_BIN".into(),path(&env::current_exe()?));
    save(&settings,&json!({"spaces":{"separateDisplays":false}}))?;
    ctx.vars.insert("LUMEN_NIX_SETTINGS_ACTIVE".into(),path(&settings));
    if !legacy {
        need(spaces(ctx)?["displaySpaces"]["coupled"]==true,"Native compositor did not couple contexts")?;
    } else {
        need(ctx.tree()?.get("display_spaces").is_none(),"Legacy compositor advertised display spaces")?;
    }
    ctx.launch("wtype", &["-s","120000"],"keyboard.log",false)?;
    ctx.launch("quickshell", &["-p",&path(&ctx.qml)],"display-recovery.log",false)?;
    if legacy {
        let initial=wait_ui(ctx,"legacy Dock fallback",|value|
            value["accepted"]["nativeSupported"]==false && visible_names(value)==["HEADLESS-1","HEADLESS-2"])?;
        need(initial["policy"]["applied"]==false && initial["policy"]["available"]==false,
            "Legacy fallback claimed native authority")?;
        keyboard(ctx,"HEADLESS-2")?;
        ctx.ipc("terminate",&[])?;
        wait_ui(ctx,"legacy observer restart",|value|
            value["accepted"]["nativeSupported"]==false && visible_names(value)==["HEADLESS-1","HEADLESS-2"])?;
        save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"legacy":true,
            "checks":["Legacy compositor projects unapplied independent routing",
                "Observer death retains both connected Docks"]}))?;
        return Ok(());
    }
    wait_ui(ctx,"initial coupled main Dock",|value|
        value["policy"]["applied"]==true && visible_names(value)==["HEADLESS-1"])?;
    save(&settings,&json!({"spaces":{"separateDisplays":false,
        "primaryDisplay":{"kind":"connector","name":"HEADLESS-2"}}}))?;
    let pending=wait_ui(ctx,"new preference pending native registration",|value|
        value["policy"]["applied"]==false && value["policy"]["primaryDisplay"]["name"]=="HEADLESS-2")?;
    need(visible_names(&pending)==["HEADLESS-1"],"Pending preference changed Dock routing")?;
    need(spaces(ctx)?["displaySpaces"]["applied"]==true,"Native preferred main registration failed")?;
    wait_ui(ctx,"confirmed preferred main Dock",|value|
        value["policy"]["applied"]==true && visible_names(value)==["HEADLESS-2"])?;
    keyboard(ctx,"HEADLESS-2")?;
    save(&settings,&json!({"spaces":{"separateDisplays":false}}))?;
    need(spaces(ctx)?["displaySpaces"]["applied"]==true,"Automatic main registration failed")?;
    wait_ui(ctx,"automatic main Dock",|value|
        value["policy"]["applied"]==true && visible_names(value)==["HEADLESS-1"])?;
    ctx.ipc("pause",&[])?;
    let expired=wait_ui(ctx,"expired observer authority",|value|value["policy"]["applied"]==false)?;
    need(visible_names(&expired)==["HEADLESS-1"],"Expired observer lost retained Dock routing")?;
    keyboard(ctx,"HEADLESS-1")?;
    ctx.checked("output HEADLESS-1 disable")?;
    wait_ui(ctx,"connected Dock fallback",|value|visible_names(value)==["HEADLESS-2"])?;
    keyboard(ctx,"HEADLESS-2")?;
    ctx.checked("output HEADLESS-1 enable")?;
    wait_ui(ctx,"reconnected main Dock",|value|visible_names(value)==["HEADLESS-1"])?;
    need(spaces(ctx)?["displaySpaces"]["applied"]==true,"Topology recovery did not register")?;
    let generation=ctx.tree()?["display_spaces"]["generation"].clone();
    ctx.ipc("resume",&[])?;
    wait_ui(ctx,"observer resumed at current generation",|value|
        value["policy"]["applied"]==true && value["policy"]["generation"]==generation)?;
    ctx.ipc("terminate",&[])?;
    wait_ui(ctx,"observer stopped",|value|value["policy"]["applied"]==false)?;
    wait_ui(ctx,"observer restarted",|value|
        value["policy"]["applied"]==true && visible_names(value)==["HEADLESS-1"])?;
    save(&settings,&json!({}))?;
    need(spaces(ctx)?["displaySpaces"]["applied"]==true,"Independent settings registration failed")?;
    wait_ui(ctx,"independent preference view",|value|value["policy"]["applied"]==true
        && value["policy"]["separateDisplays"]==true)?;
    ctx.ipc("preferencesOpen",&[])?;
    let owner=wait("display preferences native window",||Ok(find_node(&ctx.tree()?,|row|
        row["name"]=="Display Space Preferences")))?;
    ctx.checked(&format!("[con_id={}] focus",owner["id"]))?;
    need(ctx.ipc("preferenceControl",&["settings-control-separate-display-spaces"])?=="true",
        "Separate display control could not take focus")?;
    ctx.key(&["-k","space"])?;
    wait("typed separation request",||{
        if !captures.exists(){return Ok(None)};
        let rows=fs::read_to_string(&captures)?;
        Ok((rows.lines().count()==1).then_some(()))
    })?;
    let first:Value=serde_json::from_str(fs::read_to_string(&captures)?.lines().next().ok_or("Missing separation capture")?)?;
    need(first==json!({"section":"spaces","value":{"separateDisplays":false,"primaryDisplay":null}}),
        format!("Separate display control emitted wrong intent: {first}"))?;
    need(ctx.ipc("preferenceControl",&["settings-control-main-display"])?=="true",
        "Main display control could not take focus")?;
    ctx.key(&["-k","space","-k","Home","-k","Down","-k","Return"])?;
    let second=wait("typed main display request",||{
        let rows=fs::read_to_string(&captures)?;
        Ok((rows.lines().count()==2).then_some(rows))
    })?;
    let capture:Value=serde_json::from_str(second.lines().nth(1).ok_or("Missing main display capture")?)?;
    need(capture["section"]=="spaces" && capture["value"]["separateDisplays"]==true
        && capture["value"]["primaryDisplay"]["kind"]=="connector",
        format!("Main display control emitted wrong typed intent: {capture}"))?;
    ctx.ipc("preferencesClose",&[])?;
    save(&ctx.opt.output.join("result.json"),&json!({"passed":true,"legacy":false,
        "checks":["New preferred main is pending until native registration",
            "Observer pause expires authority while preserving routing",
            "Disconnected main chooses connected Dock fallback",
            "Observer resumes at captured generation after topology recovery",
            "Observer death and restart reacquires native authority",
            "Physical preference controls emit exactly two typed private settings intents"]}))?;
    Ok(())
}
