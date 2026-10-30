//! Actual exclusive-layer keyboard ownership across captured family movement.
use super::*;
use lumen_spaces::transfer;

fn layer_status(ctx: &Context, path: &Path) -> Result<Value> {
    Ok(serde_json::from_str(&ctx.named(
        "quickshell",
        &["-p", &text(path), "ipc", "call", "transferLayer", "status"],
    )?)?)
}
fn focused(tree: &Value) -> Vec<Value> {
    let mut all = Vec::new();
    nodes(tree, &mut all);
    all.into_iter()
        .filter(|node| node["focused"] == true && node["pid"].as_i64().is_some_and(|pid| pid > 0))
        .cloned()
        .collect()
}
fn presses(ctx: &Context, start: usize) -> Result<Vec<String>> {
    let content = fs::read_to_string(ctx.opt.output.join("client.log"))?;
    ensure(
        content.len() >= start,
        "Native client log retired unexpectedly",
    )?;
    Ok(content[start..]
        .lines()
        .filter(|line| line.starts_with("key ") && line.ends_with(" 1"))
        .map(str::to_owned)
        .collect())
}
fn layer_key(ctx: &Context, path: &Path, key: &str, label: &str) -> Result<()> {
    let before = layer_status(ctx, path)?;
    ensure(
        before["active"] == true && before["visible"] == true,
        "Actual layer must own keyboard before key",
    )?;
    let client_start = fs::read_to_string(ctx.opt.output.join("client.log"))?.len();
    ctx.key(&[key])?;
    let after = wait("actual layer key press", || {
        let now = layer_status(ctx, path)?;
        Ok(
            (now["pressed"].as_i64() == before["pressed"].as_i64().map(|n| n + 1)
                && now["lastText"] == key)
                .then_some(now),
        )
    })?;
    let clients = presses(ctx, client_start)?;
    save(
        &ctx.opt.output.join(format!("layer-{label}-key.json")),
        &json!({"before":before,"after":after,"nativeClientPresses":clients}),
    )?;
    ensure(
        clients.is_empty(),
        "Exclusive-layer key reached a native client",
    )
}
fn visible_outputs(ctx: &Context) -> Result<Value> {
    let rows = ctx.native("get_outputs")?;
    Ok(Value::Array(rows.as_array().ok_or("Native output list absent")?.iter()
        .map(|row|json!({"id":row["id"],"name":row["name"],"current_workspace":row["current_workspace"]})).collect()))
}
fn identity(node: &Value) -> Value {
    json!({"id":node["id"],"pid":node["pid"],"foreign":node["foreign_toplevel_identifier"],"parent":node["lumen_relationships"]["parent"],"floating":node["floating"],"state":node["lumen_state"]})
}
pub(super) fn run(ctx: &mut Context) -> Result<()> {
    if ctx.opt.effects {
        ensure(
            fs::read_to_string(ctx.opt.output.join("sway.log"))?.contains("llvmpipe"),
            "Actual software effects renderer missing",
        )?;
    }
    let registered = ctx.named("lumen-spaces", &["status"])?;
    fs::write(
        ctx.opt.output.join("layer-display-registration.json"),
        registered,
    )?;
    let source = ctx.captured[0]["id"].clone();
    let remaining = ctx.captured[1]["id"].clone();
    let child = ctx.captured[2]["id"].clone();
    ctx.sway(&format!("[con_id={source}] focus"), true)?;
    wait("exact native source focus before layer", || {
        let views = focused(&ctx.native("get_tree")?);
        Ok((views.len() == 1 && views[0]["id"] == source).then_some(()))
    })?;
    let before_outputs = visible_outputs(ctx)?;
    let layer_path = ctx.root.0.join("transfer-layer.qml");
    fs::write(&layer_path, include_str!("../desktop-transfer-layer.qml"))?;
    ctx.launch(
        Path::new("quickshell"),
        &["-p", &text(&layer_path)],
        "transfer-layer.log",
        false,
    )?;
    wait("actual exclusive layer focus", || {
        let state = layer_status(ctx, &layer_path)?;
        Ok((state["active"] == true
            && state["visible"] == true
            && focused(&ctx.native("get_tree")?).is_empty())
        .then_some(()))
    })?;
    layer_key(ctx, &layer_path, "x", "before-transfer")?;
    let tree = desktop_transfer::stable(ctx)?;
    let (capture, members) = transfer::source_family(&tree, &source)?;
    ensure(
        members.len() == 2 && members.iter().any(|node| node["id"] == child),
        "Actual root and nonmodal child must form the captured family",
    )?;
    let destination = transfer::destination(
        &tree,
        &json!({"logicalId":"exclusive-layer-target","id":null,"name":"exclusive-layer-target","output":"HEADLESS-1"}),
    )?;
    let snapshot = transfer::snapshot(&tree)?;
    let wire = transfer::packet(&snapshot, &capture, &destination)?;
    save(
        &ctx.opt.output.join("layer-captured-family.json"),
        &json!({"tree":tree,"snapshot":snapshot,"source":capture,"destination":destination,"members":members,"layer":layer_status(ctx,&layer_path)?}),
    )?;
    desktop_transfer::command(ctx, &wire, true, "exclusive-layer-family")?;
    wait("actual family reaches inactive target", || {
        Ok(
            (desktop_transfer::location(ctx, &source)?["name"] == "exclusive-layer-target"
                && desktop_transfer::location(ctx, &child)?["name"] == "exclusive-layer-target")
                .then_some(()),
        )
    })?;
    let after = desktop_transfer::stable(ctx)?;
    let mut all = Vec::new();
    nodes(&after, &mut all);
    let moved = members
        .iter()
        .map(|before| {
            let now = all
                .iter()
                .find(|node| node["id"] == before["id"])
                .ok_or("Transferred family member missing")?;
            Ok((before, *now))
        })
        .collect::<Result<Vec<_>>>()?;
    for (before, now) in &moved {
        ensure(
            identity(before) == identity(now),
            "Transfer changed exact family identity/role/hide state",
        )?;
        for size in ["width", "height"] {
            ensure(
                before["rect"][size] == now["rect"][size],
                "Family dimensions changed",
            )?;
        }
    }
    let root_before = &members[0];
    let root_after = all
        .iter()
        .find(|node| node["id"] == source)
        .ok_or("Transferred root missing")?;
    for (before, now) in &moved {
        for axis in ["x", "y"] {
            ensure(
                integer(&before["rect"][axis])? - integer(&root_before["rect"][axis])?
                    == integer(&now["rect"][axis])? - integer(&root_after["rect"][axis])?,
                "Family offset changed",
            )?;
        }
    }
    ensure(
        focused(&after).is_empty() && visible_outputs(ctx)? == before_outputs,
        "Transfer stole exclusive keyboard ownership or changed visible output",
    )?;
    layer_key(ctx, &layer_path, "y", "after-transfer")?;
    save(
        &ctx.opt.output.join("layer-still-owns-input.json"),
        &json!({"tree":after,"layer":layer_status(ctx,&layer_path)?,"outputs":visible_outputs(ctx)?}),
    )?;
    ctx.named(
        "quickshell",
        &[
            "-p",
            &text(&layer_path),
            "ipc",
            "call",
            "transferLayer",
            "closeLayer",
        ],
    )?;
    let restored = wait("layer closes and native keyboard focus returns", || {
        let state = layer_status(ctx, &layer_path)?;
        let tree = ctx.native("get_tree")?;
        Ok((state["visible"] == false && !focused(&tree).is_empty()).then_some(tree))
    })?;
    let client_start = fs::read_to_string(ctx.opt.output.join("client.log"))?.len();
    ctx.key(&["z"])?;
    let key_receipts = wait("actual native key recipient after layer close", || {
        let receipts = presses(ctx, client_start)?;
        Ok((!receipts.is_empty()).then_some(receipts))
    })?;
    let outputs = visible_outputs(ctx)?;
    let focused_views = focused(&restored);
    save(
        &ctx.opt.output.join("layer-close-outcome.json"),
        &json!({"tree":restored,"focusedViews":focused_views,"outputsBefore":before_outputs,"outputsAfter":outputs,"keyPresses":key_receipts,"expectedRemaining":remaining,"source":source,"child":child,"layer":layer_status(ctx,&layer_path)?}),
    )?;
    ensure(
        focused_views.len() == 1 && focused_views[0]["id"] == remaining,
        "Closing exclusive layer restored a moved/offscreen family instead of the remaining source view",
    )?;
    ensure(
        outputs == before_outputs,
        "Closing exclusive layer activated the transfer destination",
    )?;
    ensure(
        key_receipts
            .iter()
            .all(|line| line.split_whitespace().nth(1) == Some("1")),
        "Post-layer physical key reached transferred family",
    )?;
    ctx.result(&["Real exclusive layer owns physical keys before and after captured family movement", "Transferred family retains exact native identity, roles, frames and child offsets without target activation", "Layer closure restores remaining origin view and physical input without reactivating moved family"])
}
