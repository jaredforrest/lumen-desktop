use crate::harness::{Fixture, Options, Reference, Result, fail, pause, require, wait, wait_true};
use serde_json::{Value, json};
use std::fs;
use zbus::blocking::Proxy;
use zbus::zvariant::OwnedValue;

const EXPORTER_NAME: &str = "org.lumen.TestMenuExporter";
const EXPORTER_WINDOW: &str = "Menu Exporter Fixture";

fn state(f: &Fixture) -> Result<Value> {
    f.state()
}
fn selected(f: &Fixture, id: i64) -> Result<bool> {
    Ok(state(f)?["selection"]["id"] == id)
}
fn path(f: &Fixture, ids: &[i32]) -> Result<bool> {
    Ok(state(f)?["exportPath"] == json!(ids))
}
fn events(f: &Fixture) -> Result<Vec<Value>> {
    let data = fs::read_to_string(f.options.output.join("events.jsonl")).unwrap_or_default();
    data.lines()
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}
fn native_node(node: &Value) -> Option<Value> {
    if node["name"] == EXPORTER_WINDOW {
        return Some(node.clone());
    }
    for kind in ["nodes", "floating_nodes"] {
        if let Some(children) = node[kind].as_array() {
            for child in children {
                if let Some(found) = native_node(child) {
                    return Some(found);
                }
            }
        }
    }
    None
}
fn native(f: &Fixture) -> Result<Option<Value>> {
    let tree: Value = serde_json::from_str(&f.call("swaymsg", &["-t", "get_tree"])?)?;
    Ok(native_node(&tree))
}
fn exporter_status(f: &Fixture) -> Result<Value> {
    let control = Proxy::new(&f.session, EXPORTER_NAME, "/Menu", EXPORTER_NAME)?;
    let raw: String = control.call("Status", &())?;
    Ok(serde_json::from_str(&raw)?)
}
fn exporter_call(f: &Fixture, method: &str, kind: Option<&str>) -> Result<()> {
    let control = Proxy::new(&f.session, EXPORTER_NAME, "/Menu", EXPORTER_NAME)?;
    match kind {
        Some(kind) => control.call(method, &(kind,))?,
        None => control.call(method, &())?,
    }
    Ok(())
}
fn globalmenu(f: &Fixture, method: &str, args: &[&str]) -> Result<String> {
    let mut words = vec![
        "-p",
        f.shell.to_str().ok_or("Shell path UTF-8")?,
        "ipc",
        "call",
        "globalmenu",
        method,
    ];
    words.extend_from_slice(args);
    f.call("quickshell", &words)
}
fn exported_open(f: &Fixture) -> Result<()> {
    for _ in 0..4 {
        if state(f)?["exportPath"] == json!([]) {
            break;
        }
        f.key(&["-k", "Escape"])?;
    }
    if state(f)?["menu"] != "" {
        f.key(&["-k", "Escape"])?;
    }
    f.call("swaymsg", &["[title=\"Menu Exporter Fixture\"]", "focus"])?;
    wait_true("captured exporter focus", || {
        let status = exporter_status(f)?;
        let state = state(f)?;
        Ok(state["bridge"]["supported"] == true
            && state["bridge"]["revision"] == status["revision"])
    })?;
    globalmenu(f, "open", &["10"])?;
    wait_true("exported File root", || path(f, &[10]))
}
fn heading(f: &Fixture, id: i32) -> Result<()> {
    f.key(&["-k", "Right"])?;
    wait_true(&format!("keyboard reaches heading {id}"), || path(f, &[id]))
}
fn action(f: &Fixture, reference: &Reference) -> Result<bool> {
    f.action(reference)
}
fn checked_state(f: &Fixture, reference: &Reference) -> Result<bool> {
    let item = Proxy::new(
        &f.a11y,
        reference.0.as_str(),
        reference.1.as_str(),
        "org.a11y.atspi.Accessible",
    )?;
    let states: Vec<u32> = item.call("GetState", &())?;
    Ok(states.first().is_some_and(|word| word & (1 << 4) != 0))
}
fn description(f: &Fixture, reference: &Reference) -> Result<String> {
    let properties = Proxy::new(
        &f.a11y,
        reference.0.as_str(),
        reference.1.as_str(),
        "org.freedesktop.DBus.Properties",
    )?;
    let value: OwnedValue =
        properties.call("Get", &("org.a11y.atspi.Accessible", "Description"))?;
    Ok(String::try_from(value)?)
}
fn same_identity(f: &Fixture, pid: u32) -> Result<Value> {
    let node = wait("actual exporter window", || native(f))?;
    let status = exporter_status(f)?;
    require(
        node["pid"] == pid && status["pid"] == pid && node["window"] == status["xid"],
        format!("Exporter PID/XID mismatch: {node} {status}"),
    )?;
    Ok(node)
}
fn clean_shell(f: &Fixture) -> Result<()> {
    let log = fs::read_to_string(f.options.output.join("shell.log"))?;
    for error in [
        "TypeError:",
        "ReferenceError:",
        "Binding loop",
        "Failed to load configuration",
        "Cannot assign",
    ] {
        require(!log.contains(error), format!("Shell error {error}: {log}"))?;
    }
    Ok(())
}

pub fn run(options: Options) -> Result<(Vec<&'static str>, Value)> {
    let mut f = Fixture::new(options)?;
    f.client("menu-source")?;
    f.open_menu("Desktop", "menu-source")?;
    f.key(&["-k", "Down", "-k", "Right"])?;
    wait_true("Right moves from Desktop to Application", || {
        Ok(state(&f)?["menu"] == "Application")
    })?;
    f.key(&["-k", "Right"])?;
    wait_true("Right moves to native Window", || {
        Ok(state(&f)?["menu"] == "Window")
    })?;
    f.key(&["-k", "Left"])?;
    wait_true("Left returns to Application", || {
        Ok(state(&f)?["menu"] == "Application")
    })?;
    f.key(&["-k", "Left"])?;
    wait_true("Left returns to Desktop", || {
        Ok(state(&f)?["menu"] == "Desktop")
    })?;
    f.key(&["-k", "Escape"])?;
    wait_true("Escape dismisses heading session", || {
        Ok(state(&f)?["menu"] == "")
    })?;
    let mut checks =
        vec!["Actual owned Desktop/Application/Window heading traversal with arrows and Escape"];

    let display_file = f.options.output.join("xdisplay");
    let executable = std::env::current_exe()?;
    let executable = executable.to_str().ok_or("Executable path UTF-8")?;
    let display_file_str = display_file.to_str().ok_or("Display path UTF-8")?;
    require(
        !executable.contains('\'') && !display_file_str.contains('\''),
        "Probe path contains quote",
    )?;
    f.call(
        "swaymsg",
        &[
            "exec",
            &format!("'{executable}' display-probe '{display_file_str}'"),
        ],
    )?;
    let display = wait("private XWayland display", || {
        Ok(fs::read_to_string(&display_file)
            .ok()
            .filter(|text| !text.is_empty()))
    })?;
    f.vars.insert("DISPLAY".into(), display);
    f.vars.insert("GDK_BACKEND".into(), "x11".into());
    let exporter_bin = f
        .options
        .exporter
        .clone()
        .ok_or("--exporter required for keyboard scenario")?;
    let output = f.options.output.display().to_string();
    let exporter = f.launch(&exporter_bin, &["--output", &output], "exporter.log", false)?;
    wait_true("same-process private exporter", || {
        crate::harness::name_owned(&f.session, EXPORTER_NAME)
    })?;
    let identity = same_identity(&f, f.processes[exporter].pid())?;
    f.call("swaymsg", &["[title=\"Menu Exporter Fixture\"]", "focus"])?;
    wait_true("installed registrar and exporter layout", || {
        let state = state(&f)?;
        Ok(state["bridge"]["supported"] == true && state["bridge"]["revision"] == 1)
    })?;
    checks.push("Actual same-PID private XWayland window, registrar identity and dynamic DBusMenu transport");
    first_half(&mut f, &mut checks)?;
    let replacement = second_half(&mut f, &mut checks, exporter)?;
    clean_shell(&f)?;
    Ok((
        checks,
        json!({"source":"Installed shell and real private same-PID DBusMenu XWayland exporter; no unexported app parity claim", "initialExporter":{"pid":identity["pid"],"xid":identity["window"]}, "replacementExporter": replacement}),
    ))
}

fn first_half(f: &mut Fixture, checks: &mut Vec<&'static str>) -> Result<()> {
    exported_open(f)?;
    f.key(&["-k", "Down"])?;
    wait_true("skip disabled and separator", || selected(f, 13))?;
    let checked = wait("semantic checked item", || f.named("Checked Command"))?;
    require(
        checked.role == "menu item" && checked.enabled && checked_state(f, &checked.reference)?,
        "Checked item semantics missing",
    )?;
    require(
        description(f, &checked.reference)?.contains('K'),
        "Shortcut accessibility description missing",
    )?;
    let unavailable = wait("semantic unavailable row", || f.named("Unavailable"))?;
    require(!unavailable.enabled, "Unavailable row enabled")?;
    f.key(&["-k", "Right"])?;
    wait_true("Right skips disabled heading", || path(f, &[20]))?;
    f.key(&["-k", "Left"])?;
    wait_true("Left returns enabled heading", || path(f, &[10]))?;
    f.key(&["-k", "Left"])?;
    wait_true("exported to owned application heading", || {
        let state = state(f)?;
        Ok(state["menu"] == "Application" && state["exportPath"] == json!([]))
    })?;
    require(
        state(f)?["target"] == EXPORTER_WINDOW,
        "Owned heading lost exporter target",
    )?;
    f.key(&["-k", "Right"])?;
    wait_true("owned heading returns to captured exporter", || {
        path(f, &[10])
    })?;
    require(
        events(f)?.is_empty(),
        "Heading traversal dispatched an event",
    )?;
    checks.push("Physical heading ring spans owned and exported headings, skips disabled items and retains invoking window");

    f.key(&["-k", "Left"])?;
    wait_true("captured owned heading before exporter change", || {
        Ok(state(f)?["menu"] == "Application")
    })?;
    let revision = state(f)?["bridge"]["revision"]
        .as_u64()
        .ok_or("Missing revision")?;
    exporter_call(f, "Change", Some("revision"))?;
    wait_true("latest exporter generation while display held", || {
        Ok(state(f)?["latestBridge"]["revision"] == revision + 1)
    })?;
    f.key(&["-k", "Right"])?;
    wait_true("changed heading context cancels", || {
        let state = state(f)?;
        Ok(state["menu"] == "" && state["exportPath"] == json!([]))
    })?;
    require(events(f)?.is_empty(), "Changed heading dispatched")?;
    checks.push("Exporter revision change while an owned heading holds display cancels captured heading traversal");

    exported_open(f)?;
    f.key(&["-k", "Down"])?;
    for _ in 0..6 {
        if selected(f, 14)? {
            break;
        }
        f.key(&["-k", "Down"])?;
    }
    require(selected(f, 14)?, "Nested parent not selected")?;
    f.key(&["-k", "Right"])?;
    wait_true("Right enters child", || {
        Ok(path(f, &[10, 14])? && selected(f, 141)?)
    })?;
    f.key(&["-k", "Left"])?;
    wait_true("Left restores parent selection", || {
        Ok(path(f, &[10])? && selected(f, 14)?)
    })?;
    f.key(&["-k", "Right"])?;
    wait_true("re-enter child", || path(f, &[10, 14]))?;
    f.key(&["-k", "Escape"])?;
    wait_true("Escape closes one child level", || path(f, &[10]))?;
    f.key(&["-k", "Escape"])?;
    wait_true("Escape dismisses root", || path(f, &[]))?;
    let before = exporter_status(f)?["keys"]
        .as_array()
        .ok_or("No key list")?
        .len();
    f.key(&["-k", "k"])?;
    wait_true("exact original client input after nested Escape", || {
        Ok(exporter_status(f)?["keys"]
            .as_array()
            .is_some_and(|keys| keys.len() == before + 1))
    })?;
    require(events(f)?.is_empty(), "Nested navigation dispatched")?;
    checks.push("Nested Right/Left and one-level Escape retain parent selection and restore original native input");

    f.call(
        "swaymsg",
        &["output", "HEADLESS-1", "resolution", "640x800"],
    )?;
    wait_true("narrow output overflows headings", || {
        let state = state(f)?;
        Ok(state["headingLayout"]["width"] == 640
            && state["headingLayout"]["visible"].as_i64().unwrap_or(0)
                < state["headingLayout"]["headings"].as_i64().unwrap_or(0))
    })?;
    exported_open(f)?;
    for id in std::iter::once(20).chain(30..38) {
        heading(f, id)?;
    }
    let layout: Value = serde_json::from_str(&globalmenu(f, "layout", &[])?)?;
    let card = layout
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or("Overflow card missing")?;
    let x = card["x"].as_f64().ok_or("Overflow card x missing")?;
    let width = card["width"]
        .as_f64()
        .ok_or("Overflow card width missing")?;
    require(
        layout.as_array().is_some_and(|rows| rows.len() == 1)
            && x >= 0.
            && x <= 640. - width
            && state(f)?["hiddenAnchor"].as_f64().unwrap_or(0.) > 0.,
        "Overflow anchor is off screen",
    )?;
    f.call(
        "grim",
        &[f.options.output.join("overflow-menu.png").to_str().unwrap()],
    )?;
    f.key(&["-k", "Escape"])?;
    wait_true("native invocation context before pointer overflow", || {
        let state = state(f)?;
        Ok(state["active"] == state["bridge"]["appName"] && state["active"] != "")
    })?;
    let more = wait("actual overflow heading button", || {
        f.named("More menus and status controls")
    })?;
    f.point(&more.reference)?;
    f.pointer_command("p")?;
    f.pointer_command("r")?;
    wait_true("pointer overflow menu", || {
        Ok(state(f)?["menu"] == "Overflow")
    })?;
    let hidden = wait("semantic overflow menu row", || {
        f.named("Additional Heading 8")
    })?;
    let revision = state(f)?["bridge"]["revision"]
        .as_u64()
        .ok_or("Missing revision")?;
    exporter_call(f, "Change", Some("revision"))?;
    wait_true("held overflow observes new exporter revision", || {
        Ok(state(f)?["latestBridge"]["revision"] == revision + 1)
    })?;
    require(
        action(f, &hidden.reference)?,
        "Stale overflow action declined",
    )?;
    wait_true("stale overflow activation cancels", || {
        let state = state(f)?;
        Ok(state["menu"] == "" && state["exportPath"] == json!([]))
    })?;
    require(events(f)?.is_empty(), "Stale overflow dispatched")?;
    f.call("swaymsg", &["[title=\"Menu Exporter Fixture\"]", "focus"])?;
    wait_true("fresh overflow invoking context", || {
        Ok(state(f)?["bridge"]["revision"] == exporter_status(f)?["revision"])
    })?;
    let more = wait("fresh overflow button", || {
        f.named("More menus and status controls")
    })?;
    f.point(&more.reference)?;
    f.pointer_command("p")?;
    f.pointer_command("r")?;
    wait_true("fresh pointer overflow", || {
        Ok(state(f)?["menu"] == "Overflow")
    })?;
    let hidden = wait("fresh semantic overflow row", || {
        f.named("Additional Heading 8")
    })?;
    require(
        action(f, &hidden.reference)?,
        "Fresh overflow action declined",
    )?;
    wait_true("captured pointer overflow transition", || path(f, &[37]))?;
    require(
        state(f)?["exportTarget"] == EXPORTER_WINDOW,
        "Overflow lost exporter target",
    )?;
    for _ in 0..10 {
        f.key(&["-k", "Left"])?;
    }
    wait_true("overflow child returns through heading ring", || {
        Ok(state(f)?["menu"] == "Application")
    })?;
    require(
        state(f)?["target"] == EXPORTER_WINDOW,
        "Overflow heading lost target",
    )?;
    f.key(&["-k", "Escape"])?;
    f.call(
        "swaymsg",
        &["output", "HEADLESS-1", "resolution", "1280x1000"],
    )?;
    wait_true("restore private output", || {
        Ok(state(f)?["headingLayout"]["width"] == 1280)
    })?;
    require(events(f)?.is_empty(), "Overflow travel dispatched")?;
    checks.push("Physical arrows reach enabled overflow headings on a narrow output with an actual on-screen anchor");
    Ok(())
}
fn second_half(f: &mut Fixture, checks: &mut Vec<&'static str>, exporter: usize) -> Result<Value> {
    exported_open(f)?;
    f.key(&["-k", "Down"])?;
    wait_true("captured same-ID row", || selected(f, 13))?;
    let stale = wait("old accessibility generation", || {
        f.named("Checked Command")
    })?;
    let revision = state(f)?["bridge"]["revision"]
        .as_u64()
        .ok_or("Missing revision")?;
    exporter_call(f, "Change", Some("revision"))?;
    wait_true("same-ID generation invalidates selection", || {
        let state = state(f)?;
        Ok(state["bridge"]["revision"] == revision + 1 && state["selection"].is_null())
    })?;
    f.key(&["-k", "Return"])?;
    require(events(f)?.is_empty(), "Changed same-ID row dispatched")?;
    let _ = action(f, &stale.reference);
    require(
        events(f)?.is_empty(),
        "Stale accessibility generation dispatched",
    )?;
    checks.push("Real same-ID unchanged-label revision invalidates keyboard and stale accessibility actions");
    let checked = wait("fresh row", || f.named("Checked Command"))?;
    f.point(&checked.reference)?;
    f.pointer_command("p")?;
    let revision = state(f)?["bridge"]["revision"]
        .as_u64()
        .ok_or("Missing revision")?;
    exporter_call(f, "Change", Some("disable"))?;
    wait_true("held row disabled", || {
        Ok(state(f)?["bridge"]["revision"] == revision + 1)
    })?;
    f.pointer_command("r")?;
    require(events(f)?.is_empty(), "Disabled pressed row dispatched")?;
    checks.push(
        "Real pointer press followed by exporter update cannot activate a replaced disabled row",
    );

    f.key(&["-k", "Escape"])?;
    exported_open(f)?;
    let nested = wait("pointer submenu parent", || f.named("Nested"))?;
    f.point(&nested.reference)?;
    wait_true("physical hover opens child", || path(f, &[10, 14]))?;
    let deep = wait("pointer child row", || f.named("Deep Command"))?;
    f.point(&deep.reference)?;
    wait_true("pointer enters actual child card", || hovered(f, 141))?;
    require(
        path(f, &[10, 14])? && events(f)?.is_empty(),
        "Child hover dispatched or closed",
    )?;
    let nested = wait("current parent delegate after child expansion", || {
        f.named("Nested")
    })?;
    f.point(&nested.reference)?;
    wait_true("pointer returns to parent row", || hovered(f, 14))?;
    require(
        path(f, &[10, 14])? && events(f)?.is_empty(),
        "Parent hover dispatched or closed",
    )?;
    f.key(&["-k", "Escape"])?;
    wait_true("pointer child dismisses one level", || path(f, &[10]))?;
    pause(600);
    let parent = state(f)?;
    crate::harness::save(&f.options.output.join("stationary-parent.json"), &parent)?;
    require(
        parent["exportPath"] == json!([10]) && parent["selection"]["id"] == 14,
        "Stationary pointer reopened dismissed submenu",
    )?;
    f.call(
        "swaymsg",
        &["seat", "seat0", "cursor", "set", "1000", "700"],
    )?;
    f.pointer_command("m 0.5 0.5")?;
    wait_true("pointer departure clears dismissed-row suppression", || {
        Ok(state(f)?["hoverBlock"].is_null())
    })?;
    let nested = wait("fresh row after stationary dismissal", || f.named("Nested"))?;
    f.point(&nested.reference)?;
    wait_true("genuine pointer re-entry reopens submenu", || {
        path(f, &[10, 14])
    })?;
    f.key(&["-k", "Escape"])?;
    wait_true("re-entered submenu dismisses one level", || path(f, &[10]))?;
    pause(600);
    require(path(f, &[10])?, "Stationary pointer reopened child")?;
    f.key(&["-k", "Right"])?;
    wait_true("explicit keyboard reopens stationary parent", || {
        Ok(path(f, &[10, 14])? && selected(f, 141)?)
    })?;
    f.key(&["-k", "Escape"])?;
    wait_true("keyboard child dismissal", || path(f, &[10]))?;
    pause(600);
    require(path(f, &[10])?, "Keyboard child dismissal reopened")?;
    f.pointer_command("p")?;
    f.pointer_command("r")?;
    wait_true("explicit stationary pointer click reopens parent", || {
        path(f, &[10, 14])
    })?;
    f.key(&["-k", "Escape"])?;
    wait_true("clicked child dismissal", || path(f, &[10]))?;
    pause(600);
    require(
        path(f, &[10])? && events(f)?.is_empty(),
        "Clicked child reopened or dispatched",
    )?;
    f.key(&["-k", "Escape"])?;
    wait_true("pointer parent dismisses", || path(f, &[]))?;
    let before = exporter_status(f)?["keys"]
        .as_array()
        .ok_or("No key list")?
        .len();
    f.key(&["-k", "k"])?;
    wait_true("pointer submenu returns original native focus", || {
        Ok(exporter_status(f)?["keys"]
            .as_array()
            .is_some_and(|keys| keys.len() == before + 1))
    })?;
    checks.push("Physical submenu hover, child entry and parent return preserve hierarchy without dispatch and restore native focus");
    finish(f, checks, exporter)
}
fn hovered(f: &Fixture, id: i64) -> Result<bool> {
    let layout: Value = serde_json::from_str(&globalmenu(f, "layout", &[])?)?;
    Ok(layout.as_array().is_some_and(|cards| {
        cards.iter().any(|card| {
            card["rows"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|row| row["id"] == id && row["hover"] == true)
            })
        })
    }))
}
fn painted_cards(f: &Fixture) -> Result<bool> {
    let layout: Value = serde_json::from_str(&globalmenu(f, "layout", &[])?)?;
    let target = f.options.output.join("nested-menu.png");
    f.call("grim", &[target.to_str().ok_or("Capture path UTF-8")?])?;
    let file = fs::File::open(&target)?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        _ => return Err(fail("Unexpected PNG color type")),
    };
    let cards = layout.as_array().ok_or("No menu cards")?;
    if cards.len() != 2 {
        return Ok(false);
    }
    for card in cards {
        let coordinate = |name: &str| -> Result<i64> {
            Ok(card[name]
                .as_f64()
                .ok_or_else(|| fail(format!("Missing card {name}")))?
                .round() as i64)
        };
        let x = coordinate("x")?;
        let y = coordinate("y")?;
        let right = coordinate("x")? + coordinate("width")?;
        let bottom = coordinate("y")? + coordinate("height")?;
        if x < 0
            || y < 0
            || right > info.width as i64
            || bottom > info.height as i64
            || right <= x
            || bottom <= y
        {
            return Ok(false);
        }
        let mut count = 0f64;
        let mut sum = [0f64; 3];
        let mut sum_square = [0f64; 3];
        for row in y..bottom {
            for column in x..right {
                let offset = (row as usize * info.width as usize + column as usize) * channels;
                for color in 0..3 {
                    let channel = if channels < 3 { 0 } else { color };
                    let value = pixels[offset + channel] as f64;
                    sum[color] += value;
                    sum_square[color] += value * value;
                }
                count += 1.;
            }
        }
        let spread = (0..3)
            .map(|color| {
                (sum_square[color] / count - (sum[color] / count).powi(2))
                    .max(0.)
                    .sqrt()
            })
            .fold(0f64, f64::max);
        if spread <= 8. {
            return Ok(false);
        }
    }
    Ok(true)
}
fn finish(f: &mut Fixture, checks: &mut Vec<&'static str>, exporter: usize) -> Result<Value> {
    f.call(
        "swaymsg",
        &["seat", "seat0", "cursor", "set", "1100", "700"],
    )?;
    f.pointer_command("m 0.5 0.5")?;
    exported_open(f)?;
    f.key(&["-k", "Down"])?;
    wait_true("disabled command skipped", || selected(f, 14))?;
    f.key(&["-k", "Right"])?;
    wait_true("deep semantic menu", || path(f, &[10, 14]))?;
    let deep = wait("actual deep semantic action", || f.named("Deep Command"))?;
    require(deep.role == "menu item", "Deep action role missing")?;
    wait_true("actually painted parent and nested menu cards", || {
        painted_cards(f)
    })?;
    require(
        action(f, &deep.reference)?,
        "Deep assistive action declined",
    )?;
    wait_true("captured deep command once", || {
        Ok(events(f)?.last().is_some_and(|event| event["id"] == 141) && path(f, &[])?)
    })?;
    require(events(f)?.len() == 1, "Deep command repeated")?;
    checks.push("Assistive submenu command dispatches exactly once to the captured exporter");

    exported_open(f)?;
    f.call(
        "swaymsg",
        &["seat", "seat0", "cursor", "set", "1100", "700"],
    )?;
    f.pointer_command("m 0.5 0.5")?;
    f.pointer_command("p")?;
    f.pointer_command("r")?;
    wait_true("outside click dismissal", || path(f, &[]))?;
    require(events(f)?.len() == 1, "Outside click dispatched")?;
    exported_open(f)?;
    f.key(&["-k", "Down"])?;
    let old_token = state(f)?["bridge"]["token"].clone();
    exporter_call(f, "HideWindow", None)?;
    wait_true("native window loss while exporter lives", || {
        Ok(native(f)?.is_none() && path(f, &[])?)
    })?;
    require(
        f.processes[exporter].0.try_wait()?.is_none()
            && crate::harness::name_owned(&f.session, EXPORTER_NAME)?,
        "Exporter died on window hide",
    )?;
    f.key(&["-k", "Return"])?;
    require(events(f)?.len() == 1, "Hidden window command dispatched")?;
    exporter_call(f, "ShowWindow", None)?;
    wait_true("same process remapped native window", || {
        Ok(native(f)?.is_some_and(|node| node["pid"] == f.processes[exporter].pid()))
    })?;
    f.call("swaymsg", &["[title=\"Menu Exporter Fixture\"]", "focus"])?;
    wait_true("new native window generation in same exporter", || {
        let state = state(f)?;
        Ok(state["bridge"]["supported"] == true && state["bridge"]["token"] != old_token)
    })?;
    checks.push("Unmapping the invoking window cancels its menu even while the exporter stays alive; remapping requires a new native token");

    exported_open(f)?;
    f.key(&["-k", "Down"])?;
    let stale_nested = wait("captured exporter generation", || f.named("Nested"))?;
    let old_token = state(f)?["bridge"]["token"].clone();
    f.stop(exporter)?;
    wait_true("exporter death cancels menu", || {
        let state = state(f)?;
        Ok(state["bridge"]["supported"] == false && state["exportPath"] == json!([]))
    })?;
    f.key(&["-k", "Return"])?;
    require(events(f)?.len() == 1, "Dead exporter command dispatched")?;
    let exporter_bin = f.options.exporter.clone().ok_or("Exporter missing")?;
    let output = f.options.output.display().to_string();
    let replacement = f.launch(
        &exporter_bin,
        &["--output", &output],
        "replacement-exporter.log",
        false,
    )?;
    wait_true("replacement actual window", || {
        Ok(native(f)?.is_some_and(|node| node["pid"] == f.processes[replacement].pid()))
    })?;
    f.call("swaymsg", &["[title=\"Menu Exporter Fixture\"]", "focus"])?;
    wait_true("replacement owner and native identity", || {
        let state = state(f)?;
        Ok(state["bridge"]["supported"] == true && state["bridge"]["token"] != old_token)
    })?;
    let _ = action(f, &stale_nested.reference);
    require(
        events(f)?.len() == 1 && path(f, &[])?,
        "Stale exporter reference dispatched",
    )?;
    exported_open(f)?;
    let fresh = wait("replacement semantic row", || f.named("Checked Command"))?;
    require(f.focus(&fresh.reference)?, "Replacement row focus declined")?;
    wait_true("actual assistive row focus", || {
        Ok(f.named("Checked Command")?.is_some_and(|row| row.focused))
    })?;
    f.key(&["-k", "Return"])?;
    wait_true("Return dispatches to replacement generation", || {
        let events = events(f)?;
        Ok(events.len() == 2
            && events.last().is_some_and(|event| {
                event["id"] == 13 && event["pid"] == f.processes[replacement].pid()
            }))
    })?;
    checks.push("Outside click and exporter/window replacement cancel stale intents; fresh assistive focus plus Return dispatches once to the exact new owner");
    f.call(
        "grim",
        &[f.options.output.join("accepted.png").to_str().unwrap()],
    )?;
    exporter_status(f)
}
