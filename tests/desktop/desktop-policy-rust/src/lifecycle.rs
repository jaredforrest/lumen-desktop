use super::*;

fn logical(ctx: &Context, name: &str) -> Result<Value> {
    ctx.status()?["workspaces"]
        .as_array()
        .ok_or("Missing workspaces")?
        .iter()
        .find(|item| item["name"] == name)
        .cloned()
        .ok_or_else(|| fail(format!("Missing logical desktop {name}")))
}
fn native(ctx: &Context, name: &str) -> Result<Value> {
    ctx.native(name)?
        .ok_or_else(|| fail(format!("Missing native window {name}")))
}
fn focused_workspace(ctx: &Context) -> Result<Value> {
    ctx.sway(&["-t", "get_workspaces"])?
        .as_array()
        .ok_or("Missing native workspaces")?
        .iter()
        .find(|item| item["focused"] == true)
        .cloned()
        .ok_or("Missing focused workspace".into())
}
fn point(ctx: &Context, name: &str, ratio: f64) -> Result<(f64, f64)> {
    let value: Value = serde_json::from_str(&ctx.ipc("geometry", &[name])?)?;
    need(value.is_object(), format!("Control {name} has no geometry"))?;
    let overview = find_node(&ctx.tree()?, |node| node["name"] == "Window Overview")
        .ok_or("Native Overview window absent")?;
    let rect = &overview["rect"];
    Ok((
        rect["x"].as_f64().ok_or("Overview x")?
            + value["x"].as_f64().ok_or("Control x")?
            + value["width"].as_f64().ok_or("Control width")? * ratio,
        rect["y"].as_f64().ok_or("Overview y")?
            + value["y"].as_f64().ok_or("Control y")?
            + value["height"].as_f64().ok_or("Control height")? * 0.5,
    ))
}
fn order(ctx: &Context, output: &Value) -> Result<Vec<Value>> {
    Ok(ctx.status()?["workspaces"]
        .as_array()
        .ok_or("Missing workspaces")?
        .iter()
        .filter(|item| item["outputId"] == *output)
        .map(|item| item["logicalId"].clone())
        .collect())
}
fn ui_ready(ctx: &Context) -> Result<bool> {
    let state = ctx.ui()?;
    Ok(state["visible"] == true
        && state["busy"] == false
        && state["workspaces"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()))
}
fn workspace_names(ctx: &Context) -> Result<BTreeSet<String>> {
    Ok(ctx
        .sway(&["-t", "get_workspaces"])?
        .as_array()
        .ok_or("Missing native workspaces")?
        .iter()
        .filter_map(|v| v["name"].as_str().map(str::to_owned))
        .collect())
}
pub fn run(ctx: &mut Context) -> Result<()> {
    ctx.launch(
        "foot",
        &["--app-id=desktop-visible", "sleep", "180"],
        "visible.log",
        false,
    )?;
    let first = wait("visible native window", || ctx.native("desktop-visible"))?;
    let visible_pid = number(&first["pid"])?;
    ctx.launch(
        "foot",
        &["--app-id=desktop-minimized", "sleep", "180"],
        "minimized.log",
        false,
    )?;
    let mini = wait("minimized native window", || {
        ctx.native("desktop-minimized")
    })?;
    let mini_pid = number(&mini["pid"])?;
    ctx.launch(
        "foot",
        &["--app-id=desktop-hidden", "sleep", "180"],
        "hidden.log",
        false,
    )?;
    let hidden = wait("hidden native window", || ctx.native("desktop-hidden"))?;
    let hidden_pid = number(&hidden["pid"])?;
    ctx.checked(&format!("[con_id={}] lumen_window minimize", mini["id"]))?;
    ctx.checked(&format!("[con_id={}] lumen_window hide", hidden["id"]))?;
    let source = logical(ctx, "1")?;
    let output = source["outputId"].clone();
    let focused = focused_workspace(ctx)?["name"].clone();
    let created = ctx.mutate("desktop-create", json!({"outputId":output}))?["created"].clone();
    need(
        created["id"].is_null()
            && !workspace_names(ctx)?
                .contains(created["name"].as_str().ok_or("Created name missing")?),
        "Created desktop unexpectedly native",
    )?;
    need(
        focused_workspace(ctx)?["name"] == focused,
        "Creating desktop changed focus",
    )?;
    need(
        logical(ctx, created["name"].as_str().ok_or("Created name")?)?["logicalId"]
            == created["logicalId"],
        "Created desktop missing from fresh backend",
    )?;
    let _ = ctx.status()?;
    let before: BTreeMap<&str, Value> = [
        ("desktop-visible", first["id"].clone()),
        ("desktop-minimized", mini["id"].clone()),
        ("desktop-hidden", hidden["id"].clone()),
    ]
    .into_iter()
    .collect();
    ctx.mutate(
        "desktop-remove",
        json!({"source":source["logicalId"],"destination":created["logicalId"]}),
    )?;
    need(
        !ctx.status()?["workspaces"]
            .as_array()
            .ok_or("Missing workspaces")?
            .iter()
            .any(|item| item["logicalId"] == source["logicalId"]),
        "Removed desktop retained",
    )?;
    for (name, id) in &before {
        need(
            native(ctx, name)?["id"] == *id,
            format!("{name} identity changed"),
        )?;
    }
    need(
        native(ctx, "desktop-minimized")?["lumen_state"]["minimised"] == true
            && native(ctx, "desktop-hidden")?["lumen_state"]["hidden"] == true,
        "Saved hidden/minimized state changed",
    )?;
    need(
        native(ctx, "desktop-minimized")?["lumen_state"]["restore_workspace"] == created["name"]
            && native(ctx, "desktop-hidden")?["lumen_state"]["restore_workspace"]
                == created["name"],
        "Restore workspace not updated",
    )?;
    for pid in [visible_pid, mini_pid, hidden_pid] {
        need(
            unsafe { libc::kill(pid as i32, 0) } == 0,
            "Native window process died",
        )?;
    }
    ctx.checked(&format!("[con_id={}] scratchpad show", mini["id"]))?;
    ctx.checked(&format!("[con_id={}] lumen_window unhide", hidden["id"]))?;
    wait("restored windows on destination", || {
        Ok(ctx.status()?["windows"]
            .as_array()
            .is_some_and(|rows| {
                rows.iter()
                    .filter(|row| before.values().any(|id| *id == row["id"]))
                    .all(|row| row["workspace"] == created["name"])
            })
            .then_some(()))
    })?;
    let mut checks = vec![
        "Empty logical desktop survives separate backend processes without native focus change",
        "Populated removal migrates visible, minimized and hidden saved placements without closing or changing PID/container identity",
        "Restoring minimized and hidden windows uses surviving desktop",
    ];
    for id in before.values() {
        ctx.checked(&format!("[con_id={id}] lumen_window minimize"))?;
    }
    ctx.checked("focus output HEADLESS-2")?;
    let dormant = ctx.mutate("desktop-create", json!({"outputId":output}))?["created"].clone();
    ctx.mutate(
        "desktop-remove",
        json!({"source":created["logicalId"],"destination":dormant["logicalId"]}),
    )?;
    need(
        focused_workspace(ctx)?["output"] == "HEADLESS-2",
        "Removal changed other display focus",
    )?;
    ctx.checked(
        "focus output HEADLESS-1; workspace \"dormant-test-away\"; focus output HEADLESS-2",
    )?;
    need(
        !workspace_names(ctx)?.contains(dormant["name"].as_str().ok_or("Dormant name")?),
        "Dormant desktop materialized too soon",
    )?;
    ctx.checked(&format!("[con_id={}] scratchpad show", mini["id"]))?;
    wait("dormant desktop restore", || {
        Ok(ctx.status()?["windows"]
            .as_array()
            .is_some_and(|rows| {
                rows.iter().any(|row| {
                    row["id"] == mini["id"]
                        && row["workspace"] == dormant["name"]
                        && row["output"] == "HEADLESS-1"
                })
            })
            .then_some(()))
    })?;
    for id in [&first["id"], &hidden["id"]] {
        ctx.checked(&format!("[con_id={id}] scratchpad show"))?;
    }
    checks.push("Removing a fully minimized desktop preserves hidden state; restore recreates dormant destination on its saved output even when another display was focused");
    let _second = ctx.mutate("desktop-create", json!({"outputId":output}))?["created"].clone();
    let _third = ctx.mutate("desktop-create", json!({"outputId":output}))?["created"].clone();
    let current = order(ctx, &output)?;
    let reversed: Vec<Value> = current.iter().rev().cloned().collect();
    ctx.mutate(
        "desktop-reorder",
        json!({"outputId":output,"order":reversed}),
    )?;
    need(
        ctx.status()?["auto_reorder"] == false && order(ctx, &output)? == reversed,
        "Manual order not preserved",
    )?;
    let output_b = ctx.status()?["outputs"]
        .as_array()
        .ok_or("Missing outputs")?
        .iter()
        .find(|v| v["name"] == "HEADLESS-2")
        .ok_or("Second output missing")?["logicalId"]
        .clone();
    let remote = ctx.mutate("desktop-create", json!({"outputId":output_b}))?["created"].clone();
    ctx.checked("output HEADLESS-2 disable")?;
    wait("remote desktop evacuation", || {
        Ok(
            (logical(ctx, remote["name"].as_str().ok_or("Remote name")?)?["output"]
                == "HEADLESS-1")
                .then_some(()),
        )
    })?;
    need(
        logical(ctx, remote["name"].as_str().ok_or("Remote name")?)?["available"] == true,
        "Remote desktop not reachable",
    )?;
    checks.push("Manual order is identity-preserving and disables recent order; dormant desktop remains reachable after output disconnect");
    save(
        &ctx.opt.output.join("backend-checks.json"),
        &json!({"passed":true,"checks":checks}),
    )?;
    let probe = ctx.root.0.join("assets/shell/WindowOverview.qml");
    let source = fs::read_to_string(&probe)?;
    need(
        source.contains("Process {id:worker;stdinEnabled:true")
            && source.contains("stdout:StdioCollector {onStreamFinished:{try{"),
        "Overview worker source changed",
    )?;
    fs::write(&probe,source.replace("Process {id:worker;stdinEnabled:true","Process {id:worker;stdinEnabled:true\n        stderr:StdioCollector {onStreamFinished:console.log(\"WORKER_STDERR:\"+text)}").replace("stdout:StdioCollector {onStreamFinished:{try{","stdout:StdioCollector {onStreamFinished:{console.log(\"WORKER_STDOUT:\"+text);try{"))?;
    let qml = path(&ctx.qml);
    ctx.launch("quickshell", &["-p", &qml], "overview.log", false)?;
    wait("hidden Overview", || {
        Ok((ctx.ui()?["visible"] == false).then_some(()))
    })?;
    need(
        ctx.ipc("openOverview", &[&first["id"].to_string()])? == "true",
        "Overview did not open",
    )?;
    wait("populated Overview", || Ok(ui_ready(ctx)?.then_some(())))?;
    need(
        ctx.ui()?["error"].as_str().unwrap_or("").is_empty(),
        "Overview reported error",
    )?;
    ctx.picture("overview.png")?;
    ctx.launch("wtype", &["-s", "60000"], "keyboard.log", false)?;
    ctx.pointer_start()?;
    let original_order = order(ctx, &output)?;
    let first_id = original_order
        .first()
        .ok_or("Missing first desktop")?
        .clone();
    let last_id = original_order.last().ok_or("Missing last desktop")?.clone();
    let first_name = format!(
        "desktop-{}",
        first_id.as_str().ok_or("First logical ID is not text")?
    );
    let last_name = format!(
        "desktop-{}",
        last_id.as_str().ok_or("Last logical ID is not text")?
    );
    ctx.move_pointer(point(ctx, &first_name, 0.5)?)?;
    ctx.press()?;
    ctx.move_pointer(point(ctx, &last_name, 0.5)?)?;
    wait("editing drag", || {
        Ok((ctx.ui()?["editing"] == true).then_some(()))
    })?;
    ctx.key(&["-k", "Escape"])?;
    ctx.release()?;
    need(
        order(ctx, &output)? == original_order && ctx.ui()?["visible"] == true,
        "Escape drag changed order",
    )?;
    checks.push("Escape during genuine held desktop drag cancels order and leaves Overview open");
    ctx.move_pointer(point(ctx, &first_name, 0.5)?)?;
    ctx.press()?;
    let end = point(ctx, &last_name, 0.5)?;
    ctx.move_pointer(end)?;
    ctx.move_pointer((end.0, end.1 + 70.))?;
    ctx.release()?;
    need(
        order(ctx, &output)? == original_order && ctx.ui()?["visible"] == true,
        "Outside release changed order",
    )?;
    checks.push(
        "Releasing a genuine reorder outside the desktop strip cancels without policy writes",
    );
    ctx.move_pointer(point(ctx, &first_name, 0.5)?)?;
    ctx.press()?;
    ctx.move_pointer(point(ctx, &last_name, 0.5)?)?;
    need(
        order(ctx, &output)? == original_order,
        "Drag preview wrote policy",
    )?;
    ctx.release()?;
    wait("committed pointer order", || {
        Ok((order(ctx, &output)? != original_order).then_some(()))
    })?;
    wait("Overview after pointer reorder", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    let reordered = order(ctx, &output)?;
    checks.push("Native pointer release commits exact same-output order; no preview writes policy");
    need(
        ctx.ipc("click", &["Move desktop left"])? == "true",
        "Keyboard reorder control absent",
    )?;
    wait("keyboard reorder preview", || {
        Ok((ctx.ui()?["editing"] == true).then_some(()))
    })?;
    ctx.key(&["-k", "Escape"])?;
    need(
        order(ctx, &output)? == reordered,
        "Escape wrote keyboard order",
    )?;
    need(
        ctx.ipc("click", &["Move desktop left"])? == "true",
        "Keyboard reorder control absent",
    )?;
    ctx.key(&["-k", "Return"])?;
    wait("keyboard preview closed", || {
        Ok((ctx.ui()?["editing"] == false).then_some(()))
    })?;
    wait("keyboard Overview settled", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    wait("keyboard order committed", || {
        Ok((order(ctx, &output)? != reordered).then_some(()))
    })?;
    checks.push("Keyboard order preview cancels on Escape and commits on Return");
    ctx.move_pointer(point(ctx, "window-groups-open", 0.5)?)?;
    ctx.press()?;
    ctx.release()?;
    ctx.picture("groups-open.png")?;
    wait("groups dialog control", || {
        Ok(
            serde_json::from_str::<Value>(&ctx.ipc("geometry", &["window-groups-toggle"])?)?
                .is_object()
                .then_some(()),
        )
    })?;
    wait("groups dialog idle", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    ctx.move_pointer(point(ctx, "window-groups-toggle", 0.5)?)?;
    ctx.press()?;
    ctx.release()?;
    wait("groups enabled", || {
        Ok((ctx.status()?["windowGroups"]["enabled"] == true).then_some(()))
    })?;
    wait("groups enabled UI", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    need(
        ctx.ui()?["error"].as_str().unwrap_or("").is_empty(),
        "Group UI error",
    )?;
    ctx.picture("window-groups.png")?;
    let groups = ctx.status()?["windowGroups"].clone();
    let member = groups["groups"][0]["members"][0].clone();
    need(
        ctx.ipc("selectWindow", &[&member["id"].to_string()])? == "true",
        "Group member not selectable",
    )?;
    let revision = number(&groups["revision"])?;
    let start = point(ctx, "window-group-create", 0.5)?;
    ctx.move_pointer(start)?;
    ctx.press()?;
    ctx.move_pointer((start.0 + 260., start.1 + 70.))?;
    ctx.release()?;
    need(
        ctx.status()?["windowGroups"]["revision"] == revision,
        "Outside group release mutated group",
    )?;
    ctx.move_pointer(point(ctx, "window-group-create", 0.5)?)?;
    ctx.press()?;
    ctx.release()?;
    wait("group created", || {
        Ok((ctx.status()?["windowGroups"]["revision"]
            .as_i64()
            .unwrap_or(0)
            > revision)
            .then_some(()))
    })?;
    wait("group UI idle", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    let all = ctx.status()?["windowGroups"]["groups"]
        .as_array()
        .ok_or("Missing groups")?
        .clone();
    need(
        all.iter().any(|group| {
            group["members"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|row| row["id"] == member["id"] && row["pid"] == member["pid"])
            })
        }),
        "Group member lost",
    )?;
    checks.push("Production New Group commits on release; releasing outside cancels without native membership writes");
    need(
        ctx.ipc("focusControl", &["window-groups-toggle"])? == "true",
        "Group toggle not focusable",
    )?;
    ctx.key(&["-k", "space"])?;
    wait("groups disabled", || {
        Ok((ctx.status()?["windowGroups"]["enabled"] == false).then_some(()))
    })?;
    wait("group UI idle", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    ctx.key(&["-k", "Escape"])?;
    need(ctx.ui()?["visible"] == true, "Escape closed Overview")?;
    checks.push("Production Window Groups dialog enables by native pointer and disables by native keyboard without closing Overview");
    let count = ctx.status()?["workspaces"]
        .as_array()
        .ok_or("Missing workspaces")?
        .len();
    need(
        ctx.ipc("click", &["Create desktop"])? == "true",
        "Create desktop control missing",
    )?;
    wait("new durable desktop", || {
        Ok(ctx.status()?["workspaces"]
            .as_array()
            .is_some_and(|rows| rows.len() == count + 1)
            .then_some(()))
    })?;
    wait("create UI idle", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    ctx.ipc("refreshOverview", &[])?;
    wait("refresh UI idle", || {
        Ok((ctx.ui()?["busy"] == false).then_some(()))
    })?;
    need(
        ctx.ipc("click", &["Remove desktop"])? == "true",
        "Remove desktop control missing",
    )?;
    ctx.key(&["-k", "Escape"])?;
    need(
        ctx.status()?["workspaces"]
            .as_array()
            .is_some_and(|rows| rows.len() == count + 1),
        "Cancel removed desktop",
    )?;
    checks.push("Overview creates a durable desktop; Escape from removal confirmation preserves every desktop");
    save(
        &ctx.opt.output.join("result.json"),
        &json!({"passed":true,"checks":checks,"final":ctx.status()?}),
    )?;
    Ok(())
}
