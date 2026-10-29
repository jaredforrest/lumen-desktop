use super::*;

fn status(ctx: &Context) -> Result<Value> {
    let value: Value = serde_json::from_str(&ctx.call("lumen-spaces", &["status"])?)?;
    need(value["displaySpaces"]["error"].as_str().unwrap_or("").is_empty(),
        format!("Display policy was not applied: {}", value["displaySpaces"]))?;
    Ok(value)
}
fn visible(ctx: &Context) -> Result<BTreeMap<String, String>> {
    Ok(ctx.sway(&["-t", "get_workspaces"])?.as_array().ok_or("No workspaces")?
        .iter().filter(|row| row["visible"] == true)
        .filter_map(|row| Some((row["output"].as_str()?.to_owned(), row["name"].as_str()?.to_owned())))
        .collect())
}
fn hex(value: &Value) -> String {
    value.to_string().as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
}
fn configure(ctx: &Context, settings: &Path, value: &Value) -> Result<Value> {
    save(settings, value)?;
    status(ctx)
}
fn policy(ctx: &Context) -> Result<Value> {
    load(&ctx.root.0.join("XDG_STATE_HOME/lumen-desktop/spaces.json"))
}
fn workspace(ctx: &mut Context, name: &str, output: &str) -> Result<Value> {
    ctx.checked(&format!("focus output {output}; workspace {}; move workspace to output {output}", json!(name)))?;
    ctx.foot(&format!("display-{name}"), name)
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let settings = ctx.root.0.join("settings.json");
    save(&settings, &json!({}))?;
    ctx.vars.insert("LUMEN_NIX_SETTINGS_ACTIVE".into(), path(&settings));
    let first = workspace(ctx, "first", "HEADLESS-1")?;
    let second = workspace(ctx, "second", "HEADLESS-1")?;
    let other = workspace(ctx, "other", "HEADLESS-2")?;
    let first_state = status(ctx)?;
    need(first_state["displaySpaces"]["applied"] == true
        && first_state["displaySpaces"]["separateDisplays"] == true,
        "Default independent display policy was not registered")?;
    ctx.checked(&format!("[con_id={}] focus", first["id"]))?;
    need(visible(ctx)?["HEADLESS-2"] == "other", "Independent focus changed the other output")?;
    let initial = visible(ctx)?;
    let coupled = configure(ctx, &settings, &json!({"spaces":{"separateDisplays":false}}))?;
    need(coupled["displaySpaces"]["coupled"] == true
        && coupled["displaySpaces"]["primary"] == "HEADLESS-1", "Coupled policy chose the wrong main")?;
    need(visible(ctx)? == initial, "Coupling changed the visible composition")?;
    let state = policy(ctx)?;
    let cohorts = state["display_cohorts"].as_array().ok_or("No coupled contexts")?;
    need(cohorts.len() >= 2, "Second logical cohort was not registered")?;
    let desktops = coupled["desktops"].as_object().ok_or("No logical desktop catalog")?;
    let second_cohort = cohorts.iter().find(|cohort| cohort["members"].as_object().is_some_and(|members|
        members.values().any(|logical| logical.as_str().is_some_and(|key| desktops.get(key).is_some_and(|item| item["name"] == "second")))))
        .ok_or("Second cohort was not found")?;
    let expected: BTreeMap<String, String> = second_cohort["members"].as_object().unwrap().values()
        .filter_map(|logical| {
            let item = desktops.get(logical.as_str()?)?;
            Some((item["output"].as_str()?.to_owned(), item["name"].as_str()?.to_owned()))
        }).collect();
    ctx.checked(&format!("[con_id={}] focus", second["id"]))?;
    wait("native cohort focus", || Ok((visible(ctx)? == expected).then_some(())))?;
    let native = &ctx.tree()?["display_spaces"];
    let peer = expected.get("HEADLESS-2").ok_or("No peer desktop")?;
    let peer_id = ctx.sway(&["-t", "get_workspaces"])?.as_array().unwrap().iter()
        .find(|row| row["name"] == peer.as_str()).and_then(|row| row["id"].as_i64())
        .ok_or("No peer workspace identity")?;
    let packet = json!({"session":native["session"],"expectedGeneration":native["generation"],
        "cohort":second_cohort["id"],"output":"HEADLESS-2","workspace":peer,
        "workspaceId":peer_id.to_string()});
    ctx.checked(&format!("lumen_window display-spaces activate {}", hex(&packet)))?;
    need(visible(ctx)? == expected, "Captured activation changed cohort composition")?;
    let mut stale = packet.clone();
    stale["expectedGeneration"] = json!((native["generation"].as_str().ok_or("No generation")?.parse::<u64>()? + 1).to_string());
    let stale_command = format!("lumen_window display-spaces activate {}", hex(&stale));
    let rejected = Command::new(&ctx.swaymsg).args(["-r", &stale_command]).env_clear().envs(&ctx.vars).output()?;
    need(!rejected.status.success() && visible(ctx)? == expected, "Stale activation was not atomic")?;
    let mut mismatched = packet.clone();
    mismatched["workspace"] = json!("wrong captured member");
    let wrong_command = format!("lumen_window display-spaces activate {}", hex(&mismatched));
    let rejected = Command::new(&ctx.swaymsg).args(["-r", &wrong_command]).env_clear().envs(&ctx.vars).output()?;
    need(!rejected.status.success() && visible(ctx)? == expected, "Mismatched member was not rejected")?;
    let before_ids: BTreeSet<String> = coupled["desktops"].as_object().unwrap().keys().cloned().collect();
    let desired = json!({"spaces":{"separateDisplays":false,
        "primaryDisplay":{"kind":"connector","name":"HEADLESS-2"}}});
    need(configure(ctx, &settings, &desired)?["displaySpaces"]["primary"] == "HEADLESS-2",
        "Explicit main was not selected")?;
    ctx.checked("output HEADLESS-2 disable")?;
    let loss = status(ctx)?;
    need(loss["displaySpaces"]["applied"] == true && loss["displaySpaces"]["fallback"] == true
        && loss["displaySpaces"]["primary"] == "HEADLESS-1", "Output-loss fallback failed")?;
    need(before_ids.iter().all(|key| loss["desktops"].get(key).is_some()), "Output loss retired a logical desktop")?;
    ctx.checked("output HEADLESS-2 enable")?;
    let restored = status(ctx)?;
    need(restored["displaySpaces"]["primary"] == "HEADLESS-2"
        && restored["displaySpaces"]["fallback"] == false, "Preferred main did not return")?;
    let independent = configure(ctx, &settings, &json!({}))?;
    need(independent["displaySpaces"]["separateDisplays"] == true,
        "Independent mode did not return")?;
    for client in [&first, &second, &other] {
        need(ctx.find(&client["id"])?.is_some(), "A client disappeared after display policy transitions")?;
    }
    ctx.checked("focus output HEADLESS-2; workspace \"pair-source\"; move workspace to output HEADLESS-2")?;
    let pair_a=ctx.foot("display-pair","pair-a")?;
    let pair_b=ctx.foot("display-pair","pair-b")?;
    let original_a=ctx.find(&pair_a["id"])?.ok_or("Pair A missing")?["rect"].clone();
    let original_b=ctx.find(&pair_b["id"])?.ok_or("Pair B missing")?["rect"].clone();
    let pair_reply=Command::new(&ctx.swaymsg)
        .args(["-r",&format!("lumen_pair create {} {}",pair_a["id"],pair_b["id"])])
        .env_clear().envs(&ctx.vars).output()?;
    need(pair_reply.status.success(),format!("Secondary pair refused before coupling: {}",String::from_utf8_lossy(&pair_reply.stdout)))?;
    let pair_id=ctx.find(&pair_a["id"])?.ok_or("Paired A missing")?["lumen_pair"]["id"].clone();
    need(!pair_id.is_null(),"Secondary pair was not created before coupling")?;
    configure(ctx,&settings,&json!({"spaces":{"separateDisplays":false,
        "primaryDisplay":{"kind":"connector","name":"HEADLESS-1"}}}))?;
    need(ctx.find(&pair_a["id"])?.ok_or("Pair lost")?["lumen_pair"]["id"]==pair_id,
        "Existing secondary pair lost identity during coupling")?;
    ctx.checked(&format!("lumen_pair ratio {} 0.6",pair_a["id"]))?;
    ctx.checked(&format!("lumen_pair leave {}",pair_a["id"]))?;
    ctx.checked(&format!("lumen_pair leave {}",pair_b["id"]))?;
    wait("secondary pair geometry restoration",||{
        let a=ctx.find(&pair_a["id"])?.ok_or("Pair A disappeared")?;
        let b=ctx.find(&pair_b["id"])?.ok_or("Pair B disappeared")?;
        Ok((a["rect"]==original_a && b["rect"]==original_b).then_some(()))
    })?;
    let rejected=Command::new(&ctx.swaymsg)
        .args(["-r",&format!("lumen_pair create {} {}",pair_a["id"],pair_b["id"])])
        .env_clear().envs(&ctx.vars).output()?;
    need(!rejected.status.success(),"New secondary pair was permitted in coupled mode")?;
    ctx.checked("focus output HEADLESS-1; workspace \"primary-pair-source\"; move workspace to output HEADLESS-1")?;
    let main_a=ctx.foot("display-main-pair","main-pair-a")?;
    let main_b=ctx.foot("display-main-pair","main-pair-b")?;
    status(ctx)?;
    let main_before_a=ctx.find(&main_a["id"])?.ok_or("Main pair A missing")?["rect"].clone();
    let main_before_b=ctx.find(&main_b["id"])?.ok_or("Main pair B missing")?["rect"].clone();
    let main_reply=Command::new(&ctx.swaymsg)
        .args(["-r",&format!("lumen_pair create {} {}",main_a["id"],main_b["id"])])
        .env_clear().envs(&ctx.vars).output()?;
    need(main_reply.status.success(),format!("Primary pair refused in coupled mode: {}",String::from_utf8_lossy(&main_reply.stdout)))?;
    need(ctx.find(&main_a["id"])?.ok_or("Main pair missing")?["lumen_pair"]["partner"]==main_b["id"],
        "Primary pair was unavailable during coupling")?;
    ctx.checked(&format!("lumen_pair leave {}",main_a["id"]))?;
    ctx.checked(&format!("lumen_pair leave {}",main_b["id"]))?;
    wait("primary pair geometry restoration",||{
        let a=ctx.find(&main_a["id"])?.ok_or("Main pair A disappeared")?;
        let b=ctx.find(&main_b["id"])?.ok_or("Main pair B disappeared")?;
        Ok((a["rect"]==main_before_a && b["rect"]==main_before_b).then_some(()))
    })?;
    let client=ctx.opt.client.clone().ok_or("Display scenario requires pinned relationship client")?;
    {
        configure(ctx,&settings,&json!({}))?;
        ctx.launch(&path(&client),&[],"display-relationship-client.log",true)?;
        wait("relationship client",||Ok(find_node(&ctx.tree()?,|row|
            row["app_id"].as_str().is_some_and(|app|app.starts_with("org.example."))).is_some().then_some(())))?;
        let mut groups=ctx.tree()?["window_groups"].clone();
        ctx.checked(&format!("lumen_window groups {} on",groups["revision"]))?;
        groups=ctx.tree()?["window_groups"].clone();
        need(groups["enabled"]==true,"Groups were not enabled")?;
        let ids=groups["groups"].as_array().ok_or("No groups")?.iter()
            .map(|row|row["id"].clone()).collect::<Vec<_>>();
        need(!ids.is_empty(),"Relationship client created no native group")?;
        configure(ctx,&settings,&json!({"spaces":{"separateDisplays":false}}))?;
        groups=ctx.tree()?["window_groups"].clone();
        need(groups["enabled"]==true && groups["separate_displays"]==false
            && groups["groups"].as_array().is_some_and(|rows|rows.iter().all(|row|row["suspended"]==true))
            && groups["groups"].as_array().unwrap().iter().map(|row|row["id"].clone()).collect::<Vec<_>>()==ids,
            "Coupling did not suspend existing groups while retaining identities")?;
        configure(ctx,&settings,&json!({}))?;
        groups=ctx.tree()?["window_groups"].clone();
        need(groups["separate_displays"]==true
            && groups["groups"].as_array().unwrap().iter().map(|row|row["id"].clone()).collect::<Vec<_>>()==ids,
            "Returning to independent displays lost group identities")?;
        ctx.checked(&format!("lumen_window groups {} off",groups["revision"]))?;
    }
    save(&ctx.opt.output.join("result.json"), &json!({"passed":true,"checks":[
        "Independent focus and coupled visible composition",
        "Native cohort focus and captured activation",
        "Stale generation and mismatched member reject without visibility change",
        "Output loss and reconnect preserve logical identities and preferred main",
        "Returning to independent mode preserves clients",
        "Secondary pair survives coupling while new secondary pairs refuse; primary pairs remain usable",
        "Existing groups suspend in coupled mode and retain identities on return to independent displays"]}))?;
    Ok(())
}
