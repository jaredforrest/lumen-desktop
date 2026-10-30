//! Exact non-view fullscreen source and destination refusal journeys.
use super::desktop_transfer::{
    apply_role_refusal, capture, command, frame, location, owner, stable,
};
use super::*;
use lumen_spaces::transfer;
fn native_node(ctx: &Context, id: &Value) -> Result<Value> {
    let tree = ctx.native("get_tree")?;
    let mut all = Vec::new();
    nodes(&tree, &mut all);
    all.into_iter()
        .find(|node| node["id"] == *id)
        .cloned()
        .ok_or_else(|| fail("Native captured node retired"))
}
fn native_parent(ctx: &Context, id: &Value) -> Result<Value> {
    fn find(node: &Value, id: &Value) -> Option<Value> {
        for key in ["nodes", "floating_nodes"] {
            for child in node[key].as_array().into_iter().flatten() {
                if child["id"] == *id {
                    return Some(node.clone());
                }
                if let Some(parent) = find(child, id) {
                    return Some(parent);
                }
            }
        }
        None
    }
    find(&ctx.native("get_tree")?, id).ok_or_else(|| fail("Native parent unavailable"))
}
fn restore_floating(ctx: &Context, frame: &Value) -> Result<()> {
    let id = &frame["id"];
    ctx.sway(&format!("[con_id={id}] floating enable; [con_id={id}] resize set width {} px height {} px; [con_id={id}] move position {} px {} px; [con_id={id}] move container to workspace {}",frame["rect"]["width"],frame["rect"]["height"],frame["rect"]["x"],frame["rect"]["y"],frame["location"]["name"]),true)
}
pub(super) fn run(ctx: &mut Context, id: &Value) -> Result<()> {
    let a = ctx.captured[0]["id"].clone();
    let b = ctx.captured[2]["id"].clone();
    let peer = ctx.captured[3]["id"].clone();
    let source_before = frame(ctx, id)?;
    let a_before = frame(ctx, &a)?;
    let b_before = frame(ctx, &b)?;
    let peer_before = frame(ctx, &peer)?;
    ctx.sway(&format!("[con_id={a}] floating disable; [con_id={b}] floating disable; [con_id={a}] move container to workspace 3; [con_id={b}] move container to workspace 3; [con_id={a}] split v"),true)?;
    let parent = wait("actual nonview split on destination", || {
        let parent = native_parent(ctx, &a)?;
        Ok((parent["type"] == "con"
            && parent["pid"].is_null()
            && parent["nodes"]
                .as_array()
                .is_some_and(|nodes| !nodes.is_empty()))
        .then_some(parent))
    })?;
    let (_, source, destination, _) = capture(ctx, id, "3")?;
    ctx.sway(
        &format!("[con_id={}] fullscreen enable", parent["id"]),
        true,
    )?;
    wait("nonview destination actually owns fullscreen", || {
        Ok((native_node(ctx, &parent["id"])?["fullscreen_mode"] == 1
            && native_node(ctx, &a)?["fullscreen_mode"] == 0)
            .then_some(()))
    })?;
    let tree = ctx.native("get_tree")?;
    ensure(transfer::destination(&tree,&json!({"logicalId":destination["logicalId"],"id":destination["workspace"],"name":destination["name"],"output":"HEADLESS-2"})).is_err(),"Nonview fullscreen destination still offered")?;
    save(
        &ctx.opt.output.join("nonview-fullscreen-destination.json"),
        &json!({"parent":native_node(ctx,&parent["id"])?,"leaf":native_node(ctx,&a)?}),
    )?;
    let tree = stable(ctx)?;
    ensure(
        transfer::source(&tree, &source["id"])? == source,
        "Destination setup changed the independent source",
    )?;
    let wire = transfer::packet(&transfer::snapshot(&tree)?, &source, &destination)?;
    command(ctx, &wire, false, "nonview-fullscreen-destination-fresh")?;
    ctx.sway(
        &format!("[con_id={}] fullscreen disable", parent["id"]),
        true,
    )?;
    restore_floating(ctx, &a_before)?;
    restore_floating(ctx, &b_before)?;
    ctx.sway(&format!("[con_id={id}] floating disable; [con_id={peer}] floating disable; [con_id={id}] split v"),true)?;
    let parent = wait("actual nonview source ancestor", || {
        let parent = native_parent(ctx, id)?;
        Ok((parent["type"] == "con" && parent["pid"].is_null()).then_some(parent))
    })?;
    let (_, mut source, destination, _) = capture(ctx, id, "1")?;
    ctx.sway(
        &format!("[con_id={}] fullscreen enable", parent["id"]),
        true,
    )?;
    wait("nonview source ancestor actually owns fullscreen", || {
        Ok((native_node(ctx, &parent["id"])?["fullscreen_mode"] == 1
            && native_node(ctx, id)?["fullscreen_mode"] == 0)
            .then_some(()))
    })?;
    let leaf = native_node(ctx, id)?;
    source["rect"] = json!([
        leaf["rect"]["x"],
        leaf["rect"]["y"],
        leaf["rect"]["width"],
        leaf["rect"]["height"]
    ]);
    ensure(
        transfer::source(&ctx.native("get_tree")?, id).is_err(),
        "Fullscreen source ancestry still offered",
    )?;
    save(
        &ctx.opt.output.join("nonview-fullscreen-source.json"),
        &json!({"parent":native_node(ctx,&parent["id"])?,"leaf":leaf,"forgedCurrentSource":source}),
    )?;
    apply_role_refusal(
        ctx,
        &source,
        &destination,
        "nonview-fullscreen-source-fresh",
    )?;
    ctx.sway(
        &format!("[con_id={}] fullscreen disable", parent["id"]),
        true,
    )?;
    restore_floating(ctx, &source_before)?;
    restore_floating(ctx, &peer_before)?;
    ctx.sway(&format!("[con_id={}] focus", owner(ctx)?["id"]), true)?;
    let (_, _, _, positive) = capture(ctx, id, "1")?;
    command(ctx, &positive, true, "ordinary-after-fullscreen-refusals")?;
    ensure(
        owner(ctx)?["focused"] == true && location(ctx, id)?["name"] == "1",
        "Ordinary transfer after fullscreen refusals failed or stole focus",
    )?;
    ctx.sway(
        &format!("[con_id={id}] move container to workspace 2"),
        true,
    )?;
    Ok(())
}
