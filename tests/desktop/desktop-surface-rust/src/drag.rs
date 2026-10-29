use crate::harness::{Fixture, Options, Result, pause, require, save, wait_true};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

fn state(f: &Fixture, path: &Path) -> Result<Value> {
    f.state(path, "dragTest")
}
fn ipc(f: &Fixture, path: &Path, method: &str) -> Result<String> {
    f.ipc(path, "dragTest", method, &[])
}
fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}
fn received(value: &Value) -> Vec<String> {
    value["received"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
fn mask(value: &Value) -> usize {
    value["mask"].as_array().map_or(0, Vec::len)
}
fn gesture(
    f: &mut Fixture,
    desktop: &Path,
    peer: &Path,
    x: i64,
    y: i64,
    dx: i64,
    dy: i64,
    cancel: bool,
) -> Result<()> {
    f.cursor(x, y)?;
    for (command, delay) in [
        ("m 1 1".to_owned(), 150),
        ("p".to_owned(), 150),
        ("m 30 20".to_owned(), 150),
        (format!("m {dx} {dy}"), 400),
    ] {
        f.pointer(&format!("{command}\n"))?;
        pause(delay);
    }
    let value = json!({"desktop":state(f,desktop)?,"peer":state(f,peer)?,"seats":serde_json::from_str::<Value>(&f.sway(&["-t","get_seats"])?)?,"version":serde_json::from_str::<Value>(&f.sway(&["-t","get_version"])?)?});
    save(&f.options.output.join("last-gesture.json"), &value)?;
    if cancel {
        save(
            &f.options.output.join("before-cancel.json"),
            &json!({"desktop":state(f,desktop)?,"peer":state(f,peer)?,"tree":serde_json::from_str::<Value>(&f.sway(&["-t","get_tree"])?)?}),
        )?;
        f.key("Escape")?;
        pause(200);
    }
    f.pointer("r\n")?;
    pause(300);
    Ok(())
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    let folder = f.desktop.join("Folder");
    fs::create_dir(&folder)?;
    let document = f.desktop.join("document.txt");
    fs::write(&document, "desktop original")?;
    let incoming = f.runtime.join("Incoming");
    fs::create_dir(&incoming)?;
    let collision = incoming.join("collision.txt");
    fs::write(&collision, "source collision")?;
    let fresh = incoming.join("fresh.txt");
    fs::write(&fresh, "source fresh")?;
    fs::write(folder.join("collision.txt"), "existing destination")?;
    let desktop = f.options.output.join("desktop.qml");
    let peer = f.options.output.join("peer.qml");
    fs::write(
        &desktop,
        include_str!("../drag-desktop.qml").replace("@ASSETS@", &f.assets.display().to_string()),
    )?;
    fs::write(
        &peer,
        include_str!("../drag-peer.qml")
            .replace("@COLLISION_URI@", &uri(&collision))
            .replace("@FRESH_URI@", &uri(&fresh)),
    )?;
    f.start(
        "output HEADLESS-1 resolution 1440x1000\nxwayland disable\nseat seat0 fallback true\n",
        "60000",
    )?;
    f.launch(
        "quickshell",
        &["-p", desktop.to_str().ok_or("Desktop QML path")?],
        "desktop",
        false,
    )?;
    wait_true("Desktop native rows", || {
        Ok(state(&f, &desktop)?["count"] == 2)
    })?;
    f.launch(
        "quickshell",
        &["-p", peer.to_str().ok_or("Peer QML path")?],
        "peer",
        false,
    )?;
    wait_true("Native peer", || {
        Ok(state(&f, &peer)?["received"] == json!([]))
    })?;
    f.sway(&[
        "[title=\"NativeFilePeer\"]",
        "floating",
        "enable,",
        "resize",
        "set",
        "720",
        "960,",
        "move",
        "position",
        "0",
        "0",
    ])?;
    pause(300);
    f.sway(&[
        "-t",
        "send_tick",
        &json!({"kind":"lumen-drag","version":1,"seat":"seat0","active":true,"serial":999999})
            .to_string(),
    ])?;
    pause(200);
    let status = state(&f, &desktop)?;
    require(
        status["external"] == false && mask(&status) == 2,
        "Forged tick activated desktop drag",
    )?;
    let original_layout = status["layout"].clone();
    if !f.options.skip_native_escape {
        gesture(&mut f, &desktop, &peer, 1360, 180, -800, 150, true)?;
        wait_true("Native Escape", || {
            Ok(state(&f, &desktop)?["native"] == false)
        })?;
        require(
            state(&f, &desktop)?["layout"] == original_layout
                && received(&state(&f, &peer)?).is_empty()
                && fs::read_to_string(&document)? == "desktop original",
            "Escape altered native drag state",
        )?;
    }
    gesture(&mut f, &desktop, &peer, 1360, 180, -800, 150, false)?;
    wait_true("Desktop to peer offer", || {
        Ok(received(&state(&f, &peer)?) == vec![uri(&document)])
    })?;
    require(
        state(&f, &desktop)?["layout"] == original_layout
            && fs::read_to_string(&document)? == "desktop original",
        "Native offer altered original",
    )?;
    ipc(&f, &peer, "clear")?;
    ipc(&f, &desktop, "selectAll")?;
    gesture(&mut f, &desktop, &peer, 1360, 180, -800, 150, false)?;
    wait_true("Multi-item offer", || {
        Ok(received(&state(&f, &peer)?).len() == 2)
    })?;
    let mut actual = received(&state(&f, &peer)?);
    actual.sort();
    let mut expected = vec![uri(&document), uri(&folder)];
    expected.sort();
    require(
        actual == expected && state(&f, &desktop)?["layout"] == original_layout,
        "Multi-item offer changed originals",
    )?;
    gesture(&mut f, &desktop, &peer, 150, 120, 1180, -60, false)?;
    wait_true("Incoming confirmation", || {
        Ok(state(&f, &desktop)?["dropPrompt"] == true)
    })?;
    require(
        !folder.join("fresh.txt").exists()
            && fs::read_to_string(folder.join("collision.txt"))? == "existing destination"
            && fs::read_to_string(&collision)? == "source collision",
        "Incoming copy before confirmation",
    )?;
    f.screenshot("incoming-confirmation.png", false)?;
    f.key("Escape")?;
    wait_true("Incoming Escape", || {
        Ok(state(&f, &desktop)?["dropPrompt"] == false)
    })?;
    require(
        !folder.join("fresh.txt").exists(),
        "Escape copied incoming file",
    )?;
    pause(700);
    gesture(&mut f, &desktop, &peer, 150, 120, 1180, -60, false)?;
    wait_true("Incoming second confirmation", || {
        Ok(state(&f, &desktop)?["dropPrompt"] == true)
    })?;
    ipc(&f, &desktop, "confirm")?;
    wait_true("Confirmed incoming copy", || {
        Ok(folder.join("fresh.txt").exists())
    })?;
    require(
        fs::read_to_string(folder.join("fresh.txt"))? == "source fresh"
            && fs::read_to_string(folder.join("collision.txt"))? == "existing destination"
            && fs::read_to_string(&fresh)? == "source fresh"
            && fs::read_to_string(&collision)? == "source collision"
            && state(&f, &peer)?["action"] == 1,
        "Confirmed copy changed sources or collision",
    )?;
    pause(700);
    gesture(&mut f, &desktop, &peer, 150, 120, 1000, 550, false)?;
    f.screenshot("background-drop.png", false)?;
    wait_true("Background incoming confirmation", || {
        Ok(state(&f, &desktop)?["dropPrompt"] == true)
    })?;
    require(
        !f.desktop.join("fresh.txt").exists(),
        "Background copy before confirmation",
    )?;
    f.key("Escape")?;
    wait_true("Background Escape", || {
        Ok(state(&f, &desktop)?["dropPrompt"] == false)
    })?;
    require(
        !f.desktop.join("fresh.txt").exists(),
        "Background Escape copied file",
    )?;
    wait_true("Restored drag input mask", || {
        let value = state(&f, &desktop)?;
        Ok(value["external"] == false && mask(&value) == 2)
    })?;
    let clicks = state(&f, &desktop)?["backgroundClicks"]
        .as_i64()
        .ok_or("Missing click count")?;
    f.cursor(1100, 750)?;
    f.pointer("m 1 1\np\nr\n")?;
    wait_true("First post-drop background click", || {
        Ok(state(&f, &desktop)?["backgroundClicks"] == clicks + 1)
    })?;
    require(
        mask(&state(&f, &desktop)?) == 2,
        "Background click wedged mask",
    )?;
    for name in ["desktop", "peer"] {
        let log = fs::read_to_string(f.options.output.join(format!("{name}.log")))?;
        require(
            !["TypeError:", "ReferenceError:", "Failed to load"]
                .iter()
                .any(|marker| log.contains(marker)),
            format!("{name} QML error"),
        )?;
    }
    let mut checks = vec![
        "forged tick cannot activate or wedge desktop drag mask",
        "production desktop native offer to separate Qt client",
        "external native offer to desktop folder",
        "confirmation gates copying",
        "Escape cancels incoming copy",
        "confirmed copy creates fresh file",
        "name collisions skipped without overwrite",
        "source originals preserved",
        "source accepted CopyAction",
        "multi-item offer preserves originals",
        "incoming background offer gets cancellable confirmation",
        "first post-drop empty click reaches underlying background",
    ];
    if !f.options.skip_native_escape {
        checks.push("Escape cancels native offer and preserves metadata/files");
    }
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":checks}),
    )?;
    Ok(())
}
