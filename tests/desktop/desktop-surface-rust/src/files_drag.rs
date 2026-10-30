use crate::harness::{Fixture, Options, Result, pause, require, save, wait_true};
use serde_json::{Value, json};
use std::{fs, os::unix::ffi::OsStrExt, path::Path};

pub(super) fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            uri.push(*byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

pub(super) fn state(f: &Fixture, qml: &Path) -> Result<Value> {
    f.state(qml, "filesDragTest")
}
fn peer(f: &Fixture, qml: &Path) -> Result<Value> {
    if qml.extension().is_some_and(|ext| ext == "json") {
        Ok(serde_json::from_slice(&fs::read(qml)?)?)
    } else {
        f.state(qml, "filesPeerTest")
    }
}
fn peer_command(f: &Fixture, path: &Path, method: &str, args: &[&str]) -> Result<()> {
    if !path.extension().is_some_and(|ext| ext == "json") {
        f.ipc(path, "filesPeerTest", method, args)?;
        return Ok(());
    }
    let control = path.with_extension("control.json");
    let mut value: Value = fs::read(&control)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(json!({"generation":0,"clear":0,"actions":1,"uri":""}));
    let generation = value["generation"]
        .as_u64()
        .ok_or("GTK control generation")?
        + 1;
    value["generation"] = json!(generation);
    match method {
        "clear" => value["clear"] = json!(generation),
        "offer" => {
            value["uri"] = json!(args[0]);
            value["actions"] = json!(args[1].parse::<u64>()?);
            value["mime"] = json!(args[2]);
        }
        _ => return Err("Unknown GTK peer command".into()),
    }
    let temporary = control.with_extension("pending");
    fs::write(&temporary, serde_json::to_vec(&value)?)?;
    fs::rename(temporary, &control)?;
    wait_true("GTK control consumed", || {
        Ok(peer(f, path)?["configGeneration"] == generation)
    })
}
pub(super) fn offer(
    f: &mut Fixture,
    qml: &Path,
    peer_path: &Path,
    label: &str,
    payload: &str,
    mime: &str,
    action: u32,
    target: Option<&Path>,
    cancel: bool,
    spring: Option<&Path>,
) -> Result<Value> {
    f.ipc(qml, "filesDragTest", "clearError", &[])?;
    let previous = peer(f, peer_path)?["finished"]
        .as_u64()
        .ok_or("Peer finished count")?;
    peer_command(f, peer_path, "offer", &[payload, &action.to_string(), mime])?;
    if let Some(path) = target {
        wait_true("registered target after presentation cleanup", || {
            Ok(state(f, qml)?["targets"].as_array().is_some_and(|rows| {
                rows.iter().any(|row| {
                    row["folder"] == path.display().to_string()
                        && row["visible"] == true
                        && row["enabled"] == true
                })
            }))
        })?;
    }
    let current = state(f, qml)?;
    let title = current["title"].as_str().ok_or("Owned Files title")?;
    let (files_x, files_y) = origin(f, title)?;
    let (peer_x, peer_y) = origin(f, "FilesDropPeer")?;
    let (dx, dy) = if let Some(path) = target {
        let at = current["targets"]
            .as_array()
            .ok_or("Registered target geometry")?
            .iter()
            .find(|row| {
                row["folder"] == path.display().to_string()
                    && row["visible"] == true
                    && row["enabled"] == true
            })
            .ok_or("Visible registered target after cleanup")?;
        (
            at["x"].as_f64().ok_or("Target x")?.round() as i64,
            at["y"].as_f64().ok_or("Target y")?.round() as i64,
        )
    } else {
        (680, 650)
    };
    save(
        &f.options
            .output
            .join(format!("incoming-{label}-prepared.json")),
        &json!({"files":current,"target_folder":target,"target_point":{"x":dx,"y":dy}}),
    )?;
    let x = peer_x + 170;
    let y = peer_y + 110;
    f.cursor(x, y)?;
    for (command, delay) in [
        ("m 1 1".to_owned(), 100),
        ("p".to_owned(), 100),
        ("m 30 20".to_owned(), 150),
    ] {
        f.pointer(&format!("{command}\n"))?;
        pause(delay);
    }
    f.pointer(&format!(
        "m {} {}\n",
        files_x + dx - x - 31,
        files_y + dy - y - 21
    ))?;
    if cancel {
        wait_true("actual accepted native Copy hover", || {
            Ok(state(f, qml)?["nativeDrop"]["containsDrag"] == true)
        })?;
        f.key("Escape")?;
    } else if let Some(destination) = spring {
        wait_true("actual spring folder navigation", || {
            Ok(state(f, qml)?["path"] == destination.display().to_string())
        })?;
    } else {
        pause(200);
    }
    f.pointer("r\n")?;
    wait_true("incoming drag finished", || {
        Ok(peer(f, peer_path)?["finished"]
            .as_u64()
            .is_some_and(|count| count > previous))
    })?;
    let receipt = json!({"case":label,"requested_action":action,"mime":mime,"payload":payload,"peer":peer(f,peer_path)?,"files":state(f,qml)?});
    save(
        &f.options.output.join(format!("incoming-{label}.json")),
        &receipt,
    )?;
    Ok(receipt)
}
fn gesture(f: &mut Fixture, x: i64, y: i64, target_x: i64, target_y: i64) -> Result<()> {
    f.cursor(x, y)?;
    for (command, delay) in [
        ("m 1 1".to_owned(), 100),
        ("p".to_owned(), 100),
        ("m 30 20".to_owned(), 200),
        (
            format!("m {} {}", target_x - x - 31, target_y - y - 21),
            400,
        ),
        ("r".to_owned(), 150),
    ] {
        f.pointer(&format!("{command}\n"))?;
        pause(delay);
    }
    Ok(())
}
fn position(f: &Fixture, title: &str, x: &str, width: &str) -> Result<()> {
    f.sway(&[
        &format!("[title=\"^{title}$\"]"),
        "floating",
        "enable,",
        "resize",
        "set",
        width,
        "900,",
        "move",
        "position",
        x,
        "0",
    ])?;
    Ok(())
}
fn origin(f: &Fixture, title: &str) -> Result<(i64, i64)> {
    fn find<'a>(value: &'a Value, title: &str) -> Option<&'a Value> {
        if value["name"] == title && value["pid"].is_number() {
            return Some(value);
        }
        for key in ["nodes", "floating_nodes"] {
            for child in value[key].as_array().into_iter().flatten() {
                if let Some(found) = find(child, title) {
                    return Some(found);
                }
            }
        }
        None
    }
    let tree: Value = serde_json::from_str(&f.sway(&["-t", "get_tree"])?)?;
    let node = find(&tree, title).ok_or("Owned native window absent")?;
    let coordinate = |key: &str| -> Result<i64> {
        Ok(node["rect"][key].as_i64().ok_or("Native outer geometry")?
            + node["window_rect"][key]
                .as_i64()
                .ok_or("Native surface geometry")?)
    };
    Ok((coordinate("x")?, coordinate("y")?))
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    let folder = f.runtime.join("Files");
    fs::create_dir(&folder)?;
    let source = folder.join("drag space # café.txt");
    let second = folder.join("second.txt");
    fs::write(&source, "original outbound bytes")?;
    fs::write(&second, "second outbound bytes")?;
    let incoming = f.runtime.join("Incoming");
    fs::create_dir(&incoming)?;
    let qml = f.options.output.join("files.qml");
    std::os::unix::fs::symlink(&f.assets,f.options.output.join("assets"))?;
    let gtk = f.options.scenario == "files-drag-gtk";
    let peer_qml = f
        .options
        .output
        .join(if gtk { "peer.json" } else { "peer.qml" });
    fs::write(
        &qml,
        include_str!("../files-drag.qml")
            .replace("@ASSETS@", "./assets")
            .replace(
                "@FOLDER@",
                &serde_json::to_string(&folder.display().to_string())?,
            ),
    )?;
    if !gtk {
        fs::write(&peer_qml, include_str!("../files-drag-peer.qml"))?;
    }
    f.start(
        "output HEADLESS-1 resolution 1600x1000\nxwayland disable\nseat seat0 fallback true\n",
        "60000",
    )?;
    f.launch(
        "quickshell",
        &["-p", qml.to_str().ok_or("Files QML path")?],
        "files",
        false,
    )?;
    wait_true("actual Files rows", || {
        Ok(state(&f, &qml)?["ready"] == true
            && state(&f, &qml)?["rows"]
                .as_array()
                .is_some_and(|r| r.len() == 2))
    })?;
    if gtk {
        let binary = std::env::current_exe()?
            .parent()
            .ok_or("Fixture binary parent")?
            .join("gtk-peer");
        let control = peer_qml.with_extension("control.json");
        f.launch(
            binary.to_str().ok_or("GTK peer binary")?,
            &[
                control.to_str().ok_or("GTK control path")?,
                peer_qml.to_str().ok_or("GTK receipt path")?,
            ],
            "peer",
            false,
        )?;
    } else {
        f.launch(
            "quickshell",
            &["-p", peer_qml.to_str().ok_or("Peer QML path")?],
            "peer",
            false,
        )?;
    }
    wait_true("independent Qt URI peer", || {
        Ok(peer(&f, &peer_qml)?["received"] == json!([]))
    })?;
    position(&f, "Files — Files", "0", "760")?;
    position(&f, "FilesDropPeer", "850", "650")?;
    if f.options.scenario == "files-drag-copy-undo" {
        return crate::files_copy_undo::run(&mut f,&qml,&peer_qml,&folder,&incoming);
    }
    let expected = vec![file_uri(&source), file_uri(&second)];
    let mut results = Vec::new();
    for mode in ["list", "grid", "columns", "gallery"] {
        f.ipc(&qml, "filesDragTest", "mode", &[mode])?;
        f.ipc(&qml, "filesDragTest", "selectAll", &[])?;
        wait_true("visible source delegate", || {
            Ok(state(&f, &qml)?["points"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|r| r["path"] == source.display().to_string() && r["at"].is_object())
            }))
        })?;
        let before = state(&f, &qml)?;
        save(
            &f.options.output.join(format!("{mode}-before.json")),
            &json!({"files":before,"peer":peer(&f,&peer_qml)?,"tree":serde_json::from_str::<Value>(&f.sway(&["-t","get_tree"])?)?}),
        )?;
        f.screenshot(&format!("{mode}-before.png"), false)?;
        let at = &before["points"]
            .as_array()
            .ok_or("source geometry")?
            .iter()
            .find(|r| r["path"] == source.display().to_string())
            .ok_or("source row")?["at"];
        peer_command(&f, &peer_qml, "clear", &[])?;
        let (files_x, files_y) = origin(&f, "Files — Files")?;
        let (peer_x, peer_y) = origin(&f, "FilesDropPeer")?;
        gesture(
            &mut f,
            files_x + at["x"].as_f64().ok_or("source x")?.round() as i64,
            files_y + at["y"].as_f64().ok_or("source y")?.round() as i64,
            peer_x + 400,
            peer_y + 450,
        )?;
        save(
            &f.options.output.join(format!("{mode}-after.json")),
            &json!({"files":state(&f,&qml)?,"peer":peer(&f,&peer_qml)?}),
        )?;
        f.screenshot(&format!("{mode}-after.png"), false)?;
        wait_true("actual native multi-file URI offer", || {
            Ok(peer(&f, &peer_qml)?["received"]
                .as_array()
                .is_some_and(|r| r.len() == 2))
        })?;
        wait_true("actual receiver file reads", || {
            Ok(peer(&f, &peer_qml)?["consumed"]
                .as_array()
                .is_some_and(|rows| rows.len() == 2))
        })?;
        let received = peer(&f, &peer_qml)?;
        let mut actual = received["rawReceived"]
            .as_str()
            .ok_or("raw native URI MIME payload")?
            .lines()
            .map(|r| r.trim_end_matches('\r').to_owned())
            .collect::<Vec<_>>();
        actual.sort();
        let mut sorted = expected.clone();
        sorted.sort();
        require(
            actual == sorted && received["receiveAction"] == 1,
            format!("{mode}: native URI/action mismatch: {received}"),
        )?;
        let mut texts = received["consumed"]
            .as_array()
            .ok_or("Receiver content receipts")?
            .iter()
            .filter_map(|row| row["text"].as_str())
            .collect::<Vec<_>>();
        texts.sort();
        require(
            texts == vec!["original outbound bytes", "second outbound bytes"]
                && received["consumerError"] == "",
            format!("{mode}: receiver could not read offered file contents: {received}"),
        )?;
        require(
            fs::read_to_string(&source)? == "original outbound bytes"
                && fs::read_to_string(&second)? == "second outbound bytes",
            "Outbound native drag changed source files",
        )?;
        wait_true("Files native drag completion", || {
            Ok(state(&f, &qml)?["dragging"] == false)
        })?;
        results.push(json!({"case":"outbound","view":mode,"peer":received,"source_retained":true}));
    }
    let mut expected_jobs = 0;
    let mut source_snapshots = Vec::new();
    for (name, action) in [
        ("move-only.txt", 2),
        ("link-only.txt", 4),
        ("copy space # café.txt", 1),
    ] {
        let original = incoming.join(name);
        fs::write(&original, format!("disposable {name}"))?;
        let uri = file_uri(&original);
        let receipt = offer(
            &mut f,
            &qml,
            &peer_qml,
            name,
            &uri,
            "text/uri-list",
            action,
            None,
            false,
            None,
        )?;
        // GTK's pinned Wayland backend maps GDK_LINK to the native Copy action.
        let copies = action == 1 || (gtk && action == 4);
        if copies {
            expected_jobs += 1;
            wait_true("actual file-job copy", || {
                Ok(folder.join(name).exists() && state(&f, &qml)?["working"] == false)
            })?;
            require(
                fs::read(folder.join(name))? == fs::read(&original)?,
                "Copy payload differs",
            )?;
            require(
                peer(&f, &peer_qml)?["action"] == 1,
                "Copy offer did not finish as Copy",
            )?;
        } else {
            require(
                !folder.join(name).exists()
                    && peer(&f, &peer_qml)?["action"] == 0
                    && state(&f, &qml)?["working"] == false,
                format!("Unsupported action{action} caused a copy side effect"),
            )?;
        }
        require(
            fs::read_to_string(&original)? == format!("disposable {name}"),
            "Incoming native drop changed source",
        )?;
        source_snapshots.push((original, format!("disposable {name}")));
        results.push(json!({"case":"incoming","requested_action":action,"gtk_link_translates_to_copy":gtk && action==4,"receipt":receipt,"destination_exists":folder.join(name).exists(),"source_retained":true}));
    }
    let canceled = incoming.join("escape.txt");
    fs::write(&canceled, "retained after Escape")?;
    let receipt = offer(
        &mut f,
        &qml,
        &peer_qml,
        "escape",
        &file_uri(&canceled),
        "text/uri-list",
        1,
        None,
        true,
        None,
    )?;
    require(
        receipt["peer"]["action"] == 0 && !folder.join("escape.txt").exists(),
        "Escape accepted or copied a native offer",
    )?;
    results.push(receipt);
    source_snapshots.push((canceled, "retained after Escape".into()));
    let opaque = incoming.join("non-uri.txt");
    fs::write(&opaque, "opaque source retained")?;
    for (label, payload, mime) in [
        ("non-uri-mime", file_uri(&opaque), "text/plain"),
        ("malformed-uri", "not a URI".to_owned(), "text/uri-list"),
        (
            "relative-file-uri",
            "file:relative-path".to_owned(),
            "text/uri-list",
        ),
        (
            "remote-authority",
            file_uri(&opaque).replacen("file:///", "file://remote.invalid/", 1),
            "text/uri-list",
        ),
        (
            "remote-scheme",
            "https://remote.invalid/nope".to_owned(),
            "text/uri-list",
        ),
    ] {
        let receipt = offer(
            &mut f, &qml, &peer_qml, label, &payload, mime, 1, None, false, None,
        )?;
        if receipt["peer"]["action"] == 1 {
            wait_true("invalid URI consumer error", || {
                Ok(state(&f, &qml)?["error"]
                    .as_str()
                    .is_some_and(|e| !e.is_empty()))
            })?;
        } else {
            require(
                receipt["peer"]["action"] == 0,
                "Unsupported MIME unexpectedly negotiated another action",
            )?;
        }
        require(
            !folder.join("non-uri.txt").exists(),
            "Invalid/non-URI offer copied a source",
        )?;
        results.push(json!({"receipt":receipt,"consumer_error":state(&f,&qml)?["error"]}));
    }
    source_snapshots.push((opaque, "opaque source retained".into()));
    let collision = incoming.join("collision.txt");
    fs::write(&collision, "incoming collision")?;
    fs::write(folder.join("collision.txt"), "original destination")?;
    let receipt = offer(
        &mut f,
        &qml,
        &peer_qml,
        "conflict-skip",
        &file_uri(&collision),
        "text/uri-list",
        1,
        None,
        false,
        None,
    )?;
    expected_jobs += 1;
    wait_true("conflict job completed", || {
        Ok(state(&f, &qml)?["jobs"]
            .as_array()
            .is_some_and(|jobs| jobs.len() == expected_jobs)
            && state(&f, &qml)?["working"] == false)
    })?;
    require(
        fs::read_to_string(folder.join("collision.txt"))? == "original destination",
        "Drop conflict overwrote destination",
    )?;
    source_snapshots.push((collision, "incoming collision".into()));
    results.push(receipt);
    let destination = folder.join("Destination");
    fs::create_dir(&destination)?;
    f.ipc(&qml, "filesDragTest", "mode", &["list"])?;
    f.ipc(&qml, "filesDragTest", "refresh", &[])?;
    wait_true("actual folder target", || {
        let current = state(&f, &qml)?;
        save(&f.options.output.join("folder-readiness.json"), &current)?;
        Ok(current["busy"] == false
            && current["targets"].as_array().is_some_and(|rows| {
                rows.iter().any(|row| {
                    row["folder"] == destination.display().to_string()
                        && row["visible"] == true
                        && row["enabled"] == true
                })
            }))
    })?;
    for (label, spring) in [("folder-target", false), ("spring-target", true)] {
        let original = incoming.join(format!("{label}.txt"));
        let text = format!("payload {label}");
        fs::write(&original, &text)?;
        let receipt = offer(
            &mut f,
            &qml,
            &peer_qml,
            label,
            &file_uri(&original),
            "text/uri-list",
            1,
            Some(&destination),
            false,
            if spring { Some(&destination) } else { None },
        )?;
        expected_jobs += 1;
        wait_true("folder destination copy", || {
            Ok(destination.join(format!("{label}.txt")).exists()
                && state(&f, &qml)?["working"] == false)
        })?;
        require(
            fs::read_to_string(destination.join(format!("{label}.txt")))? == text
                && !folder.join(format!("{label}.txt")).exists(),
            "Native folder target copied to wrong destination",
        )?;
        source_snapshots.push((original, text));
        results.push(receipt);
    }
    wait_true("complete expected file-job ledger", || {
        Ok(state(&f, &qml)?["jobs"]
            .as_array()
            .is_some_and(|jobs| jobs.len() == expected_jobs)
            && state(&f, &qml)?["working"] == false)
    })?;
    let final_state = state(&f, &qml)?;
    require(
        final_state["canUndo"] == true,
        "Eligible native Copy did not retain recoverable Undo history",
    )?;
    require(
        !folder.join("move-only.txt").exists()
            && (gtk || !folder.join("link-only.txt").exists())
            && !folder.join("escape.txt").exists()
            && !folder.join("non-uri.txt").exists(),
        "Rejected offers caused delayed filesystem side effects",
    )?;
    require(
        final_state["jobs"]
            .as_array()
            .is_some_and(|jobs| jobs.len() == expected_jobs),
        format!("Rejected offers started file jobs: {final_state}"),
    )?;
    for (path, expected) in source_snapshots {
        require(
            fs::read_to_string(path)? == expected,
            "Native offer changed original source bytes",
        )?;
    }
    if gtk {
        require(
            peer(&f, &peer_qml)?["deleteRequests"] == 0,
            "GTK source received destructive delete request",
        )?;
    }
    save(
        &f.options.output.join("acceptance.json"),
        &json!({"cases":results,"files":state(&f,&qml)?,"backend":f.options.sway,"assets":f.assets}),
    )?;
    f.screenshot("final.png", false)?;
    println!(
        "PASS four Files views native URI Copy offers and incoming Copy/rejected Move/Link with real file-job/source checks"
    );
    Ok(())
}
