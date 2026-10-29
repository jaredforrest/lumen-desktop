use super::*;

fn sticky(ctx: &Context, name: &str, count: usize) -> Result<bool> {
    let rows = ctx.app(name)?;
    Ok(rows.len() == count && rows.iter().all(|row| row["sticky"] == true))
}
fn focused(ctx: &Context) -> Result<Option<Value>> {
    Ok(find_node(&ctx.tree()?, |n| {
        n["pid"].as_i64().is_some_and(|p| p > 0) && n["focused"] == true
    })
    .map(|n| n["id"].clone()))
}
fn assign(ctx: &Context, node: &Value, kind: &str, target: Option<Value>) -> Result<Value> {
    let before = focused(ctx)?;
    let state = ctx.status()?;
    let result=ctx.request("assign",json!({"id":node["id"],"pid":node["pid"],"kind":kind,"target":target,"sessionEpoch":state["sessionEpoch"]}))?;
    need(
        result.get("error").is_none(),
        format!("Assignment failed: {result}"),
    )?;
    need(focused(ctx)? == before, "Assignment stole native focus")?;
    Ok(result)
}
fn active_ui(ctx: &Context) -> Result<bool> {
    let value = ctx.ui()?;
    Ok(value["visible"] == true && value["busy"] == false && value["current"].is_object())
}
fn ui_ipc(ctx: &Context, method: &str, args: &[&str]) -> Result<String> {
    ctx.ipc(method, args)
}
fn watcher(ctx: &mut Context, name: &str) -> Result<usize> {
    let index = ctx.launch("lumen-spaces", &["watch"], &format!("{name}.log"), false)?;
    let log = ctx.opt.output.join(format!("{name}.log"));
    wait("spaces watcher ready", || {
        Ok((log.exists() && fs::read_to_string(&log)?.contains("ready")).then_some(()))
    })?;
    Ok(index)
}
fn native_id(ctx: &Context, id: &Value) -> Result<Value> {
    ctx.find(id)?
        .ok_or_else(|| fail(format!("Native window {id} disappeared")))
}
pub fn run(ctx: &mut Context) -> Result<()> {
    let first = ctx.foot("membership-editor", "first")?;
    let preexisting = ctx.foot("membership-editor", "preexisting")?;
    ctx.checked(&format!("[con_id={}] sticky enable", preexisting["id"]))?;
    let watch = watcher(ctx, "watch")?;
    ctx.launch("wtype", &["-s", "120000"], "keyboard.log", false)?;
    let qml = path(&ctx.qml);
    ctx.launch("quickshell", &["-p", &qml], "overview.log", false)?;
    wait("Overview IPC", || Ok(ctx.ipc("status", &[]).ok()))?;
    ui_ipc(ctx, "open", &[&first["id"].to_string()])?;
    wait("application Overview", || Ok(active_ui(ctx)?.then_some(())))?;
    let overview = wait("Overview native window", || {
        Ok(find_node(&ctx.tree()?, |n| n["name"] == "Window Overview"))
    })?;
    ctx.checked(&format!("[con_id={}] focus", overview["id"]))?;
    need(
        ui_ipc(ctx, "prepare", &[])? == "true",
        "Application assignment control not focused",
    )?;
    ctx.key(&["-k", "Return"])?;
    wait("two sticky editor windows", || {
        Ok(sticky(ctx, "membership-editor", 2)?.then_some(()))
    })?;
    wait("Overview settled", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    ui_ipc(ctx, "refresh", &[])?;
    wait("active membership Overview", || {
        let value = ctx.ui()?;
        Ok((value["busy"] == false
            && value["current"]["allDesktopMembership"]["status"] == "active")
            .then_some(()))
    })?;
    need(
        ctx.ui()?["error"].as_str().unwrap_or("").is_empty(),
        "Overview reported error",
    )?;
    ctx.picture("overview.png")?;
    ui_ipc(ctx, "close", &[])?;
    let mut checks = vec![
        "Production Overview exposes one typed all-desktop choice and real Return assigns all current eligible instances without replacing application identity",
    ];
    ctx.checked("workspace \"2\"; move workspace to output HEADLESS-1")?;
    wait("editor workspace two", || {
        Ok((ctx.row(&first["id"])?["workspace"] == "2").then_some(()))
    })?;
    need(
        ctx.row(&first["id"])?["output"] == "HEADLESS-1",
        "Editor moved output",
    )?;
    let third = ctx.foot("membership-editor", "future")?;
    wait("third sticky editor", || {
        Ok(sticky(ctx, "membership-editor", 3)?.then_some(()))
    })?;
    ctx.checked("focus output HEADLESS-2; workspace \"3\"; move workspace to output HEADLESS-2")?;
    let fourth = ctx.foot("membership-editor", "other-output")?;
    wait("fourth sticky editor", || {
        Ok(sticky(ctx, "membership-editor", 4)?.then_some(()))
    })?;
    need(
        ctx.row(&first["id"])?["output"] == "HEADLESS-1",
        "First editor cloned across displays",
    )?;
    ctx.checked("workspace \"4\"; move workspace to output HEADLESS-2")?;
    wait("fourth editor workspace", || {
        Ok((ctx.row(&fourth["id"])?["workspace"] == "4").then_some(()))
    })?;
    need(
        ctx.row(&first["id"])?["workspace"] == "2",
        "First editor workspace changed",
    )?;
    checks.push("Future windows and separate application instances follow every desktop only on their own output, without physical-display cloning");
    ctx.children[watch].stop(libc::SIGTERM)?;
    watcher(ctx, "watch-restarted")?;
    assign(ctx, &first, "none", None)?;
    wait("managed stickiness cleared", || {
        Ok([&first, &third, &fourth]
            .iter()
            .all(|n| native_id(ctx, &n["id"]).is_ok_and(|row| row["sticky"] == false))
            .then_some(()))
    })?;
    need(
        native_id(ctx, &preexisting["id"])?["sticky"] == true,
        "Preexisting sticky state lost",
    )?;
    checks.push("Watcher restart retains exact session/container/process-birth ownership; clearing removes only managed stickiness and preserves preexisting sticky");
    assign(ctx, &first, "all", None)?;
    wait("all editors sticky", || {
        Ok(sticky(ctx, "membership-editor", 4)?.then_some(()))
    })?;
    let target = ctx.status()?["workspaces"]
        .as_array()
        .ok_or("Missing workspaces")?
        .iter()
        .find(|w| w["name"] == "2")
        .cloned()
        .ok_or("Missing workspace two")?;
    assign(ctx, &first, "desktop", Some(target["logicalId"].clone()))?;
    wait("first editor unsticky", || {
        Ok((native_id(ctx, &first["id"])?["sticky"] == false).then_some(()))
    })?;
    need(
        native_id(ctx, &preexisting["id"])?["sticky"] == true,
        "Preexisting sticky state changed",
    )?;
    need(
        ctx.status()?["assignments"]["wayland:membership-editor"]
            == json!({"kind":"desktop","target_id":target["logicalId"]}),
        "Desktop assignment changed",
    )?;
    let fresh = ctx.foot("membership-editor", "desktop-future")?;
    wait("future desktop placement", || {
        Ok((ctx.row(&fresh["id"])?["workspace"] == "2").then_some(()))
    })?;
    checks.push("Changing to an explicit desktop preserves its logical target and future-window placement, removes only policy stickiness, and preserves native windows");
    let tiled = ctx.foot("membership-tiled", "tiled")?;
    ctx.checked(&format!("[con_id={}] floating disable", tiled["id"]))?;
    assign(ctx, &tiled, "all", None)?;
    let row = ctx.row(&tiled["id"])?;
    need(
        row["allDesktopMembership"]["status"] == "pending"
            && row["allDesktopMembership"]["reason"]
                .as_str()
                .unwrap_or("")
                .contains("Tiled")
            && native_id(ctx, &tiled["id"])?["sticky"] == false,
        "Tiled state not pending",
    )?;
    ui_ipc(ctx, "open", &[&tiled["id"].to_string()])?;
    wait("tiled Overview", || {
        let value = ctx.ui()?;
        Ok((active_ui(ctx)? && value["current"]["id"] == tiled["id"]).then_some(()))
    })?;
    need(
        ctx.ui()?["current"]["allDesktopMembership"]["status"] == "pending",
        "Tiled UI status changed",
    )?;
    ctx.picture("pending.png")?;
    ui_ipc(ctx, "close", &[])?;
    ctx.checked(&format!("[con_id={}] floating enable", tiled["id"]))?;
    wait("floating reconciled", || {
        Ok((native_id(ctx, &tiled["id"])?["sticky"] == true).then_some(()))
    })?;
    ctx.checked(&format!("[con_id={}] fullscreen enable", tiled["id"]))?;
    wait("fullscreen pending", || {
        Ok((ctx.row(&tiled["id"])?["allDesktopMembership"]["status"] == "pending").then_some(()))
    })?;
    need(
        native_id(ctx, &tiled["id"])?["fullscreen_mode"] == 1,
        "Fullscreen state changed",
    )?;
    ctx.checked(&format!("[con_id={}] fullscreen disable", tiled["id"]))?;
    wait("fullscreen reconcile", || {
        Ok((native_id(ctx, &tiled["id"])?["sticky"] == true).then_some(()))
    })?;
    checks.push("Tiled and fullscreen instances report pending without forced layout conversion; leaving unsupported state reconciles the persistent application policy");
    let client = path(
        ctx.opt
            .client
            .as_ref()
            .ok_or("Missing native relationship client")?,
    );
    let relation = ctx.launch(&client, &[], "relationships.log", true)?;
    let owned = wait("owned utility", || {
        Ok(ctx.app("org.example.Utility")?.into_iter().next())
    })?;
    assign(ctx, &owned, "all", None)?;
    need(
        native_id(ctx, &owned["id"])?["sticky"] == false
            && ctx.row(&owned["id"])?["allDesktopMembership"]["reason"]
                .as_str()
                .unwrap_or("")
                .contains("Owned"),
        "Owned utility policy changed",
    )?;
    ctx.write(relation, "modal\n")?;
    wait("modal utility", || {
        Ok((native_id(ctx, &owned["id"])?["lumen_relationships"]["modal"] == true).then_some(()))
    })?;
    need(
        native_id(ctx, &owned["id"])?["sticky"] == false,
        "Modal utility became sticky",
    )?;
    ctx.write(relation, "unmodal\n")?;
    wait("unmodal utility", || {
        Ok((native_id(ctx, &owned["id"])?["lumen_relationships"]["modal"] == false).then_some(()))
    })?;
    ctx.write(relation, "unparent\n")?;
    wait("unparented sticky utility", || {
        Ok((native_id(ctx, &owned["id"])?["sticky"] == true).then_some(()))
    })?;
    save(
        &ctx.opt.output.join("unparented-window.json"),
        &native_id(ctx, &owned["id"])?,
    )?;
    checks.push("Protocol-owned and modal windows remain with their parent with an explicit reason; role changes without window events reconcile only after the native parent/modal constraint clears");
    ctx.write(relation, "unparent-unknown\n")?;
    need(
        ctx.children[relation].0.wait()?.code() == Some(8),
        "Native helper accepted unknown command",
    )?;
    let relation_log = fs::read_to_string(ctx.opt.output.join("relationships.log"))?;
    need(
        relation_log.contains("unknown command") && !relation_log.contains("ack unparent-unknown"),
        "Native helper falsely acknowledged unknown command",
    )?;
    checks.push("Exact native helper rejects an unknown command with nonzero exit and no false acknowledgement");
    let grouped = ctx.foot("membership-grouped", "grouped")?;
    let groups = ctx.tree()?["window_groups"].clone();
    ctx.checked(&format!("lumen_window groups {} on", groups["revision"]))?;
    wait("grouped member", || {
        let state = ctx.tree()?;
        Ok(state["window_groups"]["groups"]
            .as_array()
            .is_some_and(|rows| {
                rows.iter().any(|group| {
                    group["members"]
                        .as_array()
                        .is_some_and(|members| members.iter().any(|m| m["id"] == grouped["id"]))
                })
            })
            .then_some(()))
    })?;
    let before = ctx.status()?["assignments"]
        .get("wayland:membership-grouped")
        .cloned();
    let state = ctx.status()?;
    let rejected=ctx.request("assign",json!({"id":grouped["id"],"pid":grouped["pid"],"kind":"all","sessionEpoch":state["sessionEpoch"]}))?;
    need(
        rejected["error"].as_str().unwrap_or("").contains("groups")
            && ctx.status()?["assignments"]
                .get("wayland:membership-grouped")
                .cloned()
                == before
            && native_id(ctx, &grouped["id"])?["sticky"] == false,
        "Grouped assignment was not atomic",
    )?;
    let groups = ctx.tree()?["window_groups"].clone();
    ctx.checked(&format!("lumen_window groups {} off", groups["revision"]))?;
    checks.push("Existing group membership rejects the complete assignment transaction before persistence; no implicit ungroup or partial application update");
    assign(ctx, &first, "all", None)?;
    wait("all five editors sticky", || {
        Ok(sticky(ctx, "membership-editor", 5)?.then_some(()))
    })?;
    ctx.checked(&format!(
        "[con_id={}] move container to output HEADLESS-2",
        first["id"]
    ))?;
    wait("migrated output", || {
        Ok((ctx.row(&first["id"])?["output"] == "HEADLESS-2").then_some(()))
    })?;
    need(
        native_id(ctx, &first["id"])?["sticky"] == true,
        "Migration removed stickiness",
    )?;
    ctx.checked("output HEADLESS-2 disable")?;
    wait("disconnected output", || {
        Ok((ctx.row(&first["id"])?["output"] == "HEADLESS-1").then_some(()))
    })?;
    need(
        native_id(ctx, &first["id"])?["sticky"] == true,
        "Disconnect removed stickiness",
    )?;
    ctx.checked("output HEADLESS-2 enable")?;
    wait("reconnected output", || {
        Ok(ctx.status()?["outputs"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["name"] == "HEADLESS-2"))
            .then_some(()))
    })?;
    need(
        native_id(ctx, &first["id"])?["sticky"] == true,
        "Reconnect removed stickiness",
    )?;
    ctx.checked("create_output")?;
    wait("third output", || {
        Ok(ctx.status()?["outputs"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["name"] == "HEADLESS-3"))
            .then_some(()))
    })?;
    need(
        ctx.app("membership-editor")?.len() == 5,
        "Output change cloned editor windows",
    )?;
    checks.push("Explicit output migration, disconnect fallback, reconnect and output addition retain exactly the existing instances and sticky policy, never clone windows or steal focus");
    for row in [&first, &preexisting, &third, &fourth, &fresh] {
        let pid = number(&row["pid"])?;
        need(
            unsafe { libc::kill(pid as i32, 0) } == 0,
            "Native editor process exited",
        )?;
    }
    save(&ctx.opt.output.join("final-tree.json"), &ctx.tree()?)?;
    for entry in fs::read_dir(&ctx.opt.output)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|v| v == "log") {
            let content = fs::read_to_string(&path)?;
            need(
                ![
                    "ResourceWarning",
                    "Exception ignored",
                    "Binding loop",
                    "ReferenceError",
                    "TypeError",
                ]
                .iter()
                .any(|marker| content.contains(marker)),
                format!("Fixture log problem in {}", path.display()),
            )?;
        }
    }
    let client = ctx.opt.client.as_ref().ok_or("Missing client")?;
    let mut sources = serde_json::Map::new();
    for relative in [
        "res/lumen-desktop/spaces/src/main.rs",
        "res/lumen-desktop/spaces/src/lib.rs",
        "res/lumen-desktop/spaces/src/bin/lumen-window-overview.rs",
        "res/lumen-desktop/shell/WindowOverview.qml",
    ] {
        sources.insert(relative.into(), json!(hash(&ctx.repo.join(relative))?));
    }
    save(
        &ctx.opt.output.join("result.json"),
        &json!({"passed":true,"checks":checks,"liveSocketUnchanged":env::var("SWAYSOCK").ok()==ctx.original_socket,"effects":ctx.opt.effects,"helper":{"path":path(client),"sha256":hash(client)?,"sourceSha256":hash(&ctx.repo.join("tests/desktop/window_relationship_client.c"))?},"sourceHashes":sources}),
    )?;
    Ok(())
}
