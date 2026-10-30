//! Actual managed X11 transfer using the production typed capture and codec.
use super::*;
use lumen_spaces::transfer as policy;
macro_rules! ipc { ($f:expr,$($arg:tt)*) => {{ let request=format!($($arg)*); $f.ipc(&request) }}; }

fn save(label: &str, value: &Value) -> Result<()> {
    fs::write(
        Path::new("/artifacts").join(format!("transfer-{label}.json")),
        serde_json::to_vec_pretty(value)?,
    )?;
    Ok(())
}
fn stable(f: &Fixture) -> Result<Value> {
    let mut previous = f.tree()?;
    wait("settled native transfer observation", || {
        let tree = f.tree()?;
        let ready = tree == previous;
        previous = tree.clone();
        Ok(ready.then_some(tree))
    })
}
fn origin(
    tree: &Value,
    id: &Value,
    workspace: Option<&Value>,
    output: Option<&Value>,
) -> Option<(Value, Value)> {
    let workspace = if tree["type"] == "workspace" {
        Some(tree)
    } else {
        workspace
    };
    let output = if tree["type"] == "output" {
        Some(tree)
    } else {
        output
    };
    if tree["id"] == *id {
        return Some((workspace?.clone(), output?.clone()));
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = tree[key].as_array() {
            for child in children {
                if let Some(found) = origin(child, id, workspace, output) {
                    return Some(found);
                }
            }
        }
    }
    None
}
fn raw_source(tree: &Value, node: &Value) -> Result<Value> {
    let (workspace, output) =
        origin(tree, &node["id"], None, None).ok_or("Missing native origin")?;
    Ok(
        json!({"id":node["id"],"pid":node["pid"],"foreign":node["foreign_toplevel_identifier"],"workspace":workspace["id"],"output":output["id"],"rect":[node["rect"]["x"],node["rect"]["y"],node["rect"]["width"],node["rect"]["height"]]}),
    )
}
fn target(f: &Fixture, tree: &Value, name: &str) -> Result<Value> {
    let (status, raw) = f.call(&["-r", "-t", "get_workspaces"])?;
    require(status.success(), "Workspace observation failed")?;
    let rows: Value = serde_json::from_str(&raw)?;
    let live = rows
        .as_array()
        .ok_or("Missing native workspace array")?
        .iter()
        .find(|row| row["name"] == name);
    Ok(policy::destination(
        tree,
        &json!({"logicalId":format!("x11-transfer-{name}"),"name":name,"id":live.map(|row|row["id"].clone()),"output":live.map(|row|row["output"].clone()).unwrap_or(json!("HEADLESS-1"))}),
    )?)
}
fn capture(f: &Fixture, node: &Value, name: &str, forged: bool) -> Result<String> {
    let tree = stable(f)?;
    let source = if forged {
        let current = find_node(&tree, &|candidate| candidate["id"] == node["id"])
            .ok_or("Forged-packet source retired before fresh observation")?;
        raw_source(&tree, &current)?
    } else {
        policy::source(&tree, &node["id"])?
    };
    Ok(policy::command(
        &policy::snapshot(&tree)?,
        &source,
        &target(f, &tree, name)?,
    )?)
}
fn refuse(f: &Fixture, command: &str, label: &str) -> Result<()> {
    let before = stable(f)?;
    let (status, raw) = f.call(&["-r", command])?;
    let reply: Value = serde_json::from_str(&raw)?;
    let after = f.tree()?;
    save(
        label,
        &json!({"command":command,"status":status.code(),"reply":reply,"before":before,"after":after}),
    )?;
    require(
        !status.success()
            && reply.as_array().is_some_and(|rows| {
                rows.len() == 1
                    && rows[0]["success"] == false
                    && rows[0]["error"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("LST_REJECTED:"))
            }),
        "Expected exact native transfer refusal",
    )?;
    require(
        before == after,
        "Rejected transfer mutated full native state",
    )
}
fn current(f: &Fixture) -> Result<String> {
    let (status, raw) = f.call(&["-r", "-t", "get_workspaces"])?;
    require(status.success(), "Workspace observation failed")?;
    let rows: Value = serde_json::from_str(&raw)?;
    rows.as_array()
        .and_then(|rows| rows.iter().find(|row| row["focused"] == true))
        .and_then(|row| row["name"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| fail("Focused workspace missing"))
}
fn identity(node: &Value) -> Value {
    json!({"id":node["id"],"pid":node["pid"],"foreign":node["foreign_toplevel_identifier"],"xid":node["window"],"parent":node["lumen_relationships"]["parent"]})
}
fn move_family(f: &mut Fixture, name: &str, child: bool, label: &str) -> Result<()> {
    let root = f.mapped(0, 0)?;
    let before_child = if child { Some(f.mapped(0, 1)?) } else { None };
    let visible = current(f)?;
    let command = capture(f, &root, name, false)?;
    let reply = f.ipc(&command)?;
    require(
        reply
            .as_array()
            .is_some_and(|r| r.len() == 1 && r[0]["success"] == true),
        "Transfer not acknowledged",
    )?;
    wait("exact transferred native family", || {
        Ok((f.location(0, 0)?.as_deref() == Some(name)
            && (!child || f.location(0, 1)?.as_deref() == Some(name)))
        .then_some(()))
    })?;
    let after = f.mapped(0, 0)?;
    let after_child = if child { Some(f.mapped(0, 1)?) } else { None };
    require(
        identity(&root) == identity(&after),
        "Transfer replaced native root identity",
    )?;
    require(
        current(f)? == visible,
        "Transfer activated destination workspace",
    )?;
    if let (Some(before), Some(after_child)) = (&before_child, &after_child) {
        require(
            identity(before) == identity(after_child),
            "Transfer replaced native child identity/parent",
        )?;
        for axis in ["x", "y"] {
            require(
                i(&before["rect"], axis)? - i(&root["rect"], axis)?
                    == i(&after_child["rect"], axis)? - i(&after["rect"], axis)?,
                "Family offset changed",
            )?;
        }
        for size in ["width", "height"] {
            require(
                before["rect"][size] == after_child["rect"][size],
                "Child dimensions changed",
            )?;
        }
    }
    save(
        label,
        &json!({"rootBefore":root,"rootAfter":after,"childBefore":before_child,"childAfter":after_child,"focusedWorkspaceBefore":visible,"focusedWorkspaceAfter":current(f)?}),
    )
}
fn projection_command(node: &Value, enabled: bool) -> Result<String> {
    let _ = lumen_window_policy::projection::parse(node)?.ok_or("Projection metadata missing")?;
    let p = &node["lumen_all_desktops"];
    Ok(format!(
        "lumen_projection {} {} {} {} {} {} {}",
        i(node, "id")?,
        node["foreign_toplevel_identifier"]
            .as_str()
            .ok_or("Foreign identity missing")?,
        i(node, "pid")?,
        p["origin_workspace"]["id"]
            .as_str()
            .ok_or("Projection origin missing")?,
        p["origin_output"]["id"]
            .as_str()
            .ok_or("Projection output missing")?,
        p["revision"]
            .as_str()
            .ok_or("Projection revision missing")?,
        if enabled { "on" } else { "off" }
    ))
}
pub(super) fn run(f: &mut Fixture, options: &Options) -> Result<()> {
    let root = f.mapped(0, 0)?;
    if options.without_xres {
        require(
            root["pid"] == 0,
            "XRes-disabled root did not retain unknown PID",
        )?;
        let tree = stable(f)?;
        policy::snapshot(&tree)?;
        require(
            policy::source(&tree, &root["id"]).is_err(),
            "Unknown XRes source offered transfer",
        )?;
        save("unknown-xres", &tree)?;
        f.checks
            .push("Actual XRes-disabled X11 root has no typed transfer source");
        return Ok(());
    }
    require(
        root["shell"] == "xwayland" && i(&root, "pid")? == i(&f.clients[0].state()?, "pid")?,
        "Actual XRes owner identity missing",
    )?;
    let original = f.location(0, 0)?.ok_or("Original workspace missing")?;
    move_family(f, "transfer-floating", false, "floating")?;
    move_family(f, &original, false, "floating-return")?;
    f.checks.push("Actual floating X11 transfer and return preserve native PID/foreign/XID and visible workspace");
    let xid = f.xid(0, 0)?;
    f.command(0, "parent", 1, xid)?;
    f.command(0, "property", 1, 0)?;
    f.map(0, 1)?;
    f.relation_wait(
        "known same-process nonmodal child",
        |r| r["parent"] == root["id"] && r["parent_supported"] == true && r["modal"] == false,
        0,
        1,
    )?;
    ipc!(
        f,
        "[con_id={}] floating enable, resize set 300 180, move position 200 180",
        i(&f.mapped(0, 1)?, "id")?
    )?;
    ipc!(
        f,
        "[con_id={}] move container to workspace {}",
        i(&f.mapped(0, 1)?, "id")?,
        original
    )?;
    move_family(f, "transfer-family", true, "floating-family")?;
    move_family(f, &original, true, "floating-family-return")?;
    f.checks.push("Actual same-PID nonmodal X11 family moves atomically with exact identity and child offset/size preservation");
    ipc!(f, "[con_id={}] floating disable", i(&root, "id")?)?;
    f.node_wait(
        "tiled root",
        |n| n.is_some_and(|n| matches!(n["floating"].as_str(), Some("auto_off" | "user_off"))),
        0,
        0,
    )?;
    move_family(f, "transfer-tiled", true, "tiled-family")?;
    move_family(f, &original, true, "tiled-family-return")?;
    f.checks.push("Actual tiled X11 root carries ordinary floating descendants and preserves their relative placement");
    ipc!(f, "[con_id={}] floating enable", i(&root, "id")?)?;
    let current_root = f.mapped(0, 0)?;
    let stale = capture(f, &current_root, "transfer-stale", false)?;
    ipc!(f, "[con_id={}] resize set 540 330", i(&root, "id")?)?;
    refuse(f, &stale, "stale-root-frame")?;
    let current_root = f.mapped(0, 0)?;
    let stale = capture(f, &current_root, "transfer-stale", false)?;
    ipc!(
        f,
        "[con_id={}] resize set 340 210",
        i(&f.mapped(0, 1)?, "id")?
    )?;
    refuse(f, &stale, "stale-child-frame")?;
    f.checks.push(
        "Captured X11 root and descendant geometry races refuse with full native tree unchanged",
    );
    for (index, label) in [(0, "projected-root"), (1, "projected-child")] {
        let node = f.mapped(0, index)?;
        f.ipc(&projection_command(&node, true)?)?;
        wait("actual unowned native projection enabled", || {
            Ok((f.mapped(0, index)?["lumen_all_desktops"]["enabled"] == true).then_some(()))
        })?;
        let tree = stable(f)?;
        require(
            policy::source(&tree, &root["id"]).is_err(),
            "Enabled native projection offered transfer",
        )?;
        let current_root = f.mapped(0, 0)?;
        let forged = capture(f, &current_root, "transfer-projected", true)?;
        refuse(f, &forged, label)?;
        let node = f.mapped(0, index)?;
        f.ipc(&projection_command(&node, false)?)?;
        wait("explicit projection cleared", || {
            Ok((f.mapped(0, index)?["lumen_all_desktops"]["enabled"] == false).then_some(()))
        })?;
        require(
            policy::source(&stable(f)?, &root["id"]).is_ok(),
            "Explicitly cleared native projection still prevented transfer",
        )?;
    }
    f.checks.push("Unowned enabled root/member projection refuses typed and forged fresh native transfer until explicit clear");
    f.command(0, "unmap", 1, 0)?;
    f.node_wait("old child retired", |node| node.is_none(), 0, 1)?;
    f.command(1, "parent", 1, xid)?;
    f.command(1, "property", 1, 0)?;
    f.map(1, 1)?;
    f.relation_wait(
        "cross-process ancestry unsupported",
        |r| r["parent_supported"] == false && r["parent"].is_null(),
        1,
        1,
    )?;
    let tree = stable(f)?;
    let child = f.mapped(1, 1)?;
    require(
        child["window_properties"]["transient_for"] == root["window"]
            && child["pid"] != root["pid"],
        "Cross-PID raw native edge not established",
    )?;
    require(
        policy::source(&tree, &root["id"]).is_err(),
        "Unsupported raw child silently omitted by typed family",
    )?;
    let current_root = f.mapped(0, 0)?;
    let forged = capture(f, &current_root, "transfer-cross-pid", true)?;
    refuse(f, &forged, "cross-pid-child")?;
    f.checks.push("Actual unsupported cross-PID transient edge refuses status and forged fresh native transfer without mutation");
    f.command(1, "unmap", 1, 0)?;
    f.node_wait("cross-process child retired", |node| node.is_none(), 1, 1)?;
    let current_root = f.mapped(0, 0)?;
    let stale = capture(f, &current_root, "transfer-replaced", false)?;
    let old = f.mapped(0, 0)?;
    f.command(0, "destroy", 0, 0)?;
    f.node_wait("captured root retired", |node| node.is_none(), 0, 0)?;
    f.command(0, "new", 0, 0)?;
    let replacement = f.map(0, 0)?;
    require(
        identity(&old) != identity(&replacement),
        "Replacement did not change native identity",
    )?;
    refuse(f, &stale, "retired-replaced-root")?;
    f.checks.push("Closed/replaced X11 capture cannot transfer its replacement through stale immutable authority");
    Ok(())
}
