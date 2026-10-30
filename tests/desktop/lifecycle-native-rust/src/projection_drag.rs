//! Physical projection gestures retain the original structural ownership.
use super::*;
fn number(node: &Value, section: &str, key: &str) -> Result<i64> {
    node[section][key]
        .as_i64()
        .ok_or("Native gesture geometry unavailable".into())
}
fn origin(f: &Fixture, id: u64) -> Result<Value> {
    let wanted = view(f, id)?["lumen_all_desktops"]["origin_workspace"]["id"]
        .as_str()
        .ok_or("Origin id")?
        .parse::<u64>()?;
    nodes(f)?
        .into_iter()
        .find(|n| n["type"] == "workspace" && n["id"] == wanted)
        .ok_or("Structural origin missing".into())
}
fn gesture(
    f: &mut Fixture,
    pointer: usize,
    log: &std::path::Path,
    serial: &mut u32,
    start: (i64, i64),
    end: (i64, i64),
    dwell: bool,
) -> Result<()> {
    request(f, "seat seat0 cursor set 0 0", None)?;
    *serial += 1;
    lock::pointer(
        f,
        pointer,
        log,
        *serial,
        &format!("m {} {}\np\n", start.0, start.1),
    )?;
    *serial += 1;
    lock::pointer(
        f,
        pointer,
        log,
        *serial,
        &format!("m {} {}\n", end.0 - start.0, end.1 - start.1),
    )?;
    if dwell {
        std::thread::sleep(std::time::Duration::from_millis(350));
    }
    *serial += 1;
    lock::pointer(f, pointer, log, *serial, "r\n")
}
fn frame(node: &Value) -> Result<(i64, i64)> {
    Ok((
        number(node, "rect", "x")? + number(node, "rect", "width")? / 2,
        number(node, "rect", "y")? - number(node, "deco_rect", "height")? / 2,
    ))
}
fn raw(f: &Fixture, label: &str, before: &Value) -> Result<()> {
    save(
        &f.options.output.join(format!("{label}-raw.json")),
        &json!({"before":before,"after":f.tree()?}),
    )
}
fn layout_events(path: &std::path::Path) -> Result<Vec<Value>> {
    let bytes = fs::read(path)?;
    require(bytes.len() < 65536, "Gesture event journal bound")?;
    let mut result = Vec::new();
    for line in std::str::from_utf8(&bytes)?.lines() {
        let row: Value = serde_json::from_str(line)?;
        if let Some(payload) = row["payload"].as_str().filter(|p| p.starts_with('{')) {
            let event: Value = serde_json::from_str(payload)?;
            if event["lumen_drag_layout"] == 1 {
                result.push(event);
            }
        }
    }
    Ok(result)
}
pub(super) fn run(f: &mut Fixture, id: u64) -> Result<()> {
    let driver = lock::pinned(f, "wlrctl")?;
    let log = f.options.output.join("projection-drag-pointer.log");
    let pointer = f.launch_input_args(
        &driver,
        &["pointer", "move", "0", "0"],
        "projection-drag-pointer.log",
    )?;
    lock::pointer(f, pointer, &log, 1, "m 0 0\n")?;
    request(f, "seat seat0 cursor set 0 0", None)?;
    let node = view(f, id)?;
    f.call(
        "grim",
        &[f.options
            .output
            .join("projected-gesture-frame.png")
            .to_str()
            .ok_or("Frame screenshot")?],
    )?;
    // Pinned IPC excludes decoration from rect.y/height; the native scene frame
    // starts one decoration height above this reported content rectangle.
    let green_x = number(&node, "rect", "x")? + 60;
    let green_y = number(&node, "rect", "y")? - number(&node, "deco_rect", "height")? / 2;
    lock::pointer(
        f,
        pointer,
        &log,
        2,
        &format!("m {green_x} {green_y}\np\nr\n"),
    )?;
    wait_true(
        "physical projected green button activates usable fullscreen origin",
        || {
            save(
                &f.options.output.join("physical-green-fullscreen.json"),
                &json!({"before":node,"after":f.tree()?,"current":current_workspace(f)?}),
            )?;
            Ok(view(f, id)?["fullscreen_mode"] == 1
                && current_workspace(f)?["name"]
                    == node["lumen_all_desktops"]["origin_workspace"]["name"])
        },
    )?;
    request(f, &format!("[con_id={id}] fullscreen disable"), None)?;
    request(f, "workspace 2", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    request(f, "seat seat0 cursor set 0 0", None)?;
    let node = view(f, id)?;
    let x = number(&node, "rect", "x")? + number(&node, "rect", "width")? / 2;
    let y = number(&node, "rect", "y")? - number(&node, "deco_rect", "height")? / 2;
    let before = f.tree()?;
    let layout = structural(&origin(f, id)?);
    lock::pointer(f, pointer, &log, 3, &format!("m {x} {y}\np\n"))?;
    // The first motion crosses Sway's drag threshold; a subsequent actual
    // motion updates the drop target in the newly entered seat operation.
    lock::pointer(f, pointer, &log, 4, "m 30 20\n")?;
    lock::pointer(
        f,
        pointer,
        &log,
        5,
        &format!("m {} {}\n", 1100 - x - 30, 700 - y - 20),
    )?;
    lock::pointer(f, pointer, &log, 6, "r\n")?;
    let after = f.tree()?;
    save(
        &f.options
            .output
            .join("tiled-projection-physical-drag-raw.json"),
        &json!({"before":before,"after":after,"targetId":id,"pointerLog":fs::read_to_string(&log)?}),
    )?;
    save(
        &f.options.output.join("tiled-projection-physical-drag.json"),
        &json!({"before":before,"after":after,"originBefore":layout,"originAfter":structural(&origin(f,id)?),"target":view(f,id)?,"current":current_workspace(f)?,"focus":focused(f)?,"pointerLog":fs::read_to_string(&log)?}),
    )?;
    require(
        layout == structural(&origin(f, id)?)
            && current_workspace(f)?["name"] == "2"
            && focused(f)? == id,
        "Off-origin tiled pointer drag changed structural layout or active presentation",
    )?;
    let mut serial = 6;
    let node = view(f, id)?;
    let start = (
        number(&node, "rect", "x")? + number(&node, "rect", "width")? - 1,
        number(&node, "rect", "y")? + 120,
    );
    let before = f.tree()?;
    let tiled = structural(&origin(f, id)?);
    gesture(
        f,
        pointer,
        &log,
        &mut serial,
        start,
        (start.0 + 80, start.1),
        false,
    )?;
    raw(f, "tiled-border-resize", &before)?;
    require(
        tiled == structural(&origin(f, id)?)
            && focused(f)? == id
            && current_workspace(f)?["name"] == "2",
        "Off-origin tiled border resize changed retained layout",
    )?;

    let original = view(f, id)?["lumen_all_desktops"]["origin_workspace"]["name"].clone();
    request(f, &format!("workspace {original}"), None)?;
    save(
        &f.options
            .output
            .join("explicit-origin-desktop-navigation.json"),
        &json!({"tree":f.tree()?,"current":current_workspace(f)?,"target":view(f,id)?}),
    )?;
    require(
        current_workspace(f)?["name"] == original
            && view(f, id)?["lumen_all_desktops"]["projected"] == false,
        "Explicit origin desktop navigation kept the already focused view on another desktop",
    )?;
    request(f, "workspace_auto_back_and_forth yes", None)?;
    request(f, "workspace 2", None)?;
    request(f, &format!("workspace {original}"), None)?;
    save(
        &f.options
            .output
            .join("effective-desktop-auto-back-and-forth.json"),
        &json!({"tree":f.tree()?,"current":current_workspace(f)?,"target":view(f,id)?}),
    )?;
    require(
        current_workspace(f)?["name"] == original,
        "Auto back-and-forth treated structural origin as the current effective desktop",
    )?;
    request(f, "workspace_auto_back_and_forth no", None)?;
    for action in [
        "floating enable",
        "resize set 400 300",
        "move position 300 200",
    ] {
        request(f, &format!("[con_id={id}] {action}"), None)?;
    }
    request(f, "workspace 2", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    let before = f.tree()?;
    let floating = view(f, id)?;
    let start = frame(&floating)?;
    gesture(
        f,
        pointer,
        &log,
        &mut serial,
        start,
        (start.0 + 70, start.1 + 45),
        false,
    )?;
    raw(f, "floating-same-output-move", &before)?;
    let moved = view(f, id)?;
    require(
        moved["lumen_all_desktops"]["origin_workspace"]
            == floating["lumen_all_desktops"]["origin_workspace"]
            && number(&moved, "rect", "x")? == number(&floating, "rect", "x")? + 70
            && number(&moved, "rect", "y")? == number(&floating, "rect", "y")? + 45
            && focused(f)? == id
            && current_workspace(f)?["name"] == "2",
        "Ordinary projected floating move rewrote origin or failed real geometry change",
    )?;
    let before = f.tree()?;
    let start = (
        number(&moved, "rect", "x")? + number(&moved, "rect", "width")? - 1,
        number(&moved, "rect", "y")? + 100,
    );
    gesture(
        f,
        pointer,
        &log,
        &mut serial,
        start,
        (start.0 + 60, start.1),
        false,
    )?;
    raw(f, "floating-projection-resize", &before)?;
    let resized = view(f, id)?;
    require(
        resized["lumen_all_desktops"]["origin_workspace"]
            == moved["lumen_all_desktops"]["origin_workspace"]
            && number(&resized, "rect", "width")? == number(&moved, "rect", "width")? + 60,
        "Direct projected floating resize was blocked or rewrote origin",
    )?;

    let events = f.options.output.join("projection-layout-events.log");
    let subscriber = f.launch(
        std::path::Path::new("swaymsg"),
        &["-r", "-m", "-t", "subscribe", "[\"tick\"]"],
        "projection-layout-events.log",
    )?;
    wait_true("real layout journal initial subscription event", || {
        let bytes = fs::read(&events)?;
        require(bytes.len() < 65536, "Gesture subscription journal bound")?;
        Ok(std::str::from_utf8(&bytes)?.lines().any(|line| {
            serde_json::from_str::<Value>(line)
                .is_ok_and(|row| row["first"] == true && row["payload"] == "")
        }))
    })?;
    let before = f.tree()?;
    let start = (
        number(&resized, "rect", "x")? + number(&resized, "rect", "width")? - 50,
        frame(&resized)?.1,
    );
    gesture(f, pointer, &log, &mut serial, start, (0, 500), true)?;
    f.call(
        "swaymsg",
        &["-t", "send_tick", "projection-gesture-off-origin-barrier"],
    )?;
    wait_true("layout journal covers full off-origin dwell", || {
        Ok(fs::read_to_string(&events)?.contains("projection-gesture-off-origin-barrier"))
    })?;
    raw(f, "off-origin-edge-suppression", &before)?;
    let off_events = layout_events(&events)?;
    save(
        &f.options.output.join("off-origin-edge-events.json"),
        &json!({"events":off_events,"target":view(f,id)?}),
    )?;
    require(
        !off_events
            .iter()
            .any(|e| e["id"] == id && (e["phase"] == "preview" || e["phase"] == "commit"))
            && view(f, id)?["lumen_state"]["filled"] == false
            && view(f, id)?["lumen_all_desktops"]["origin_workspace"]
                == resized["lumen_all_desktops"]["origin_workspace"]
            && focused(f)? == id
            && current_workspace(f)?["name"] == "2",
        "Off-origin edge gesture previewed/committed a structural layout action",
    )?;
    request(f, &format!("workspace {original}"), None)?;
    request(f, &format!("[con_id={id}] move position 300 200"), None)?;
    let normal = view(f, id)?;
    let start = (
        number(&normal, "rect", "x")? + number(&normal, "rect", "width")? - 50,
        frame(&normal)?.1,
    );
    gesture(f, pointer, &log, &mut serial, start, (0, 500), true)?;
    f.call(
        "swaymsg",
        &["-t", "send_tick", "projection-gesture-origin-barrier"],
    )?;
    wait_true("layout journal covers ordinary origin gesture", || {
        Ok(fs::read_to_string(&events)?.contains("projection-gesture-origin-barrier"))
    })?;
    let normal_events = layout_events(&events)?;
    save(
        &f.options.output.join("ordinary-origin-edge-events.json"),
        &json!({"events":normal_events,"tree":f.tree()?}),
    )?;
    require(
        normal_events
            .iter()
            .any(|e| e["id"] == id && e["phase"] == "preview")
            && normal_events
                .iter()
                .any(|e| e["id"] == id && e["phase"] == "commit"),
        "Ordinary origin edge gesture must still preview and commit",
    )?;
    require(
        f.processes[subscriber].alive()?,
        "Layout event observer exited",
    )?;
    request(f, &format!("[con_id={id}] lumen_window restore"), None)?;
    request(f, &format!("[con_id={id}] move position 300 200"), None)?;
    request(f, "workspace 2", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    request(f, "create_output", None)?;
    request(f, "output HEADLESS-2 position 1440 0", None)?;
    let before = f.tree()?;
    let old = view(f, id)?;
    let start = frame(&old)?;
    gesture(f, pointer, &log, &mut serial, start, (1700, 300), false)?;
    raw(f, "explicit-cross-output-pointer-move", &before)?;
    let cross = view(f, id)?;
    require(
        cross["lumen_all_desktops"]["origin_output"]["name"] == "HEADLESS-2"
            && cross["lumen_all_desktops"]["revision"] != old["lumen_all_desktops"]["revision"]
            && focused(f)? == id
            && cross["visible"] == true,
        "Explicit cross-output pointer move failed owning-output/revision transition",
    )?;
    let before = f.tree()?;
    request(
        f,
        &operation(&old, false)?,
        Some("Projection witness unavailable"),
    )?;
    raw(f, "cross-output-old-witness-refusal", &before)?;
    require(
        before == f.tree()?,
        "Old pointer origin witness mutated current view",
    )?;

    request(f, &format!("workspace {original}"), None)?;
    request(
        f,
        &format!("[con_id={id}] move container to workspace {original}"),
        None,
    )?;
    request(f, &format!("[con_id={id}] floating disable"), None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    request(f, "split v", None)?;
    let ancestor = nodes(f)?
        .into_iter()
        .find(|n| {
            n["type"] == "con"
                && n["pid"].is_null()
                && n["nodes"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|n| n["id"] == id))
        })
        .ok_or("Actual nonview ancestor missing")?;
    let ancestor_id = ancestor["id"].as_u64().ok_or("Ancestor id")?;
    request(f, &format!("[con_id={ancestor_id}] floating enable"), None)?;
    request(f, "workspace 2", None)?;
    request(f, &format!("[con_id={id}] focus"), None)?;
    require(
        view(f, id)?["lumen_all_desktops"]["projected"] == true,
        "Ancestor fixture requires real projected leaf",
    )?;
    let before = f.tree()?;
    let layout = structural(&origin(f, id)?);
    let node = view(f, id)?;
    let start = frame(&node)?;
    gesture(
        f,
        pointer,
        &log,
        &mut serial,
        start,
        (start.0 + 80, start.1 + 50),
        false,
    )?;
    raw(f, "nonview-floating-ancestor-move", &before)?;
    require(
        layout == structural(&origin(f, id)?)
            && focused(f)? == id
            && current_workspace(f)?["name"] == "2",
        "Ambiguous nonview ancestor pointer move changed projected structural owner",
    )?;
    let before = f.tree()?;
    let start = (
        number(&node, "rect", "x")? + number(&node, "rect", "width")? - 1,
        number(&node, "rect", "y")? + 100,
    );
    gesture(
        f,
        pointer,
        &log,
        &mut serial,
        start,
        (start.0 + 80, start.1),
        false,
    )?;
    raw(f, "nonview-floating-ancestor-resize", &before)?;
    require(
        layout == structural(&origin(f, id)?)
            && focused(f)? == id
            && current_workspace(f)?["name"] == "2",
        "Ambiguous nonview ancestor pointer resize changed projected structural owner",
    )?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":13}),
    )
}
