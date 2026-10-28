use crate::harness::{Fixture, Options, Result, pause, require, wait_true};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;

fn writes(f: &Fixture) -> Result<usize> {
    let path = f.options.output.join("writes.jsonl");
    Ok(if path.exists() {
        fs::read_to_string(path)?.lines().count()
    } else {
        0
    })
}
fn keys(state: &Value) -> Vec<String> {
    state["keys"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
fn center(f: &Fixture, key: &str) -> Result<(i64, i64)> {
    let value = f.state()?;
    let index = keys(&value)
        .iter()
        .position(|item| item == key)
        .ok_or("Missing Dock item")?;
    let width = value["width"].as_f64().ok_or("Missing width")?;
    let slot = value["slot"].as_f64().ok_or("Missing slot")?;
    Ok((
        ((1920.0 - width) / 2.0 + 12.0 + (index as f64 + 0.5) * slot).round() as i64,
        1030,
    ))
}
fn button(f: &mut Fixture, code: &str) -> Result<()> {
    f.pointer(&format!("{code}\n"))?;
    pause(100);
    Ok(())
}
fn press(f: &mut Fixture, position: &mut (i64, i64), key: &str) -> Result<()> {
    let (x, y) = center(f, key)?;
    f.move_to(position, x, y)?;
    button(f, "p")
}
fn outside(f: &mut Fixture, position: &mut (i64, i64)) -> Result<()> {
    f.move_to(position, 30, 500)
}
fn reset(f: &Fixture) -> Result<()> {
    f.ipc("reset", &[])?;
    wait_true("Dock reset", || {
        let value = f.state()?;
        Ok(value["ready"] == true && value["busy"] == false)
    })?;
    pause(150);
    Ok(())
}
fn app_window<'a>(value: &'a Value, app: &str) -> Option<&'a Value> {
    if value["app_id"] == app {
        return Some(value);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = value[key].as_array() {
            for child in children {
                if let Some(found) = app_window(child, app) {
                    return Some(found);
                }
            }
        }
    }
    None
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options, include_str!("../dock-drag.qml"))?;
    f.start_sway()?;
    let service = f.install_stub()?;
    fs::write(
        f.options.output.join("Fixture A.desktop"),
        "[Desktop Entry]\nName=Fixture A\nType=Application\nExec=true\n",
    )?;
    fs::write(
        f.options.output.join("Original report.txt"),
        "original file remains",
    )?;
    fs::create_dir(f.options.output.join("Original folder"))?;
    let qml = f.qml.display().to_string();
    let mut shell = f.launch("quickshell", &["-p", &qml], "dock", false)?;
    wait_true("Dock settings", || {
        let value = f.state()?;
        Ok(value["ready"] == true && value["busy"] == false)
    })?;
    pause(500);
    f.start_pointer()?;
    let mut position = (960, 540);
    let mut checks = Vec::new();
    press(&mut f, &mut position, "files")?;
    let (x, y) = center(&f, "b")?;
    f.move_to(&mut position, x, y)?;
    button(&mut f, "r")?;
    require(
        f.state()?["activations"] == json!([]),
        "Required Dock item activated a bystander",
    )?;
    checks.push("Required-item press across neighbours cannot activate a bystander");
    let count = writes(&f)?;
    press(&mut f, &mut position, "a")?;
    let (x, y) = center(&f, "b")?;
    f.move_to(&mut position, x, y)?;
    wait_true("Held Dock drag", || Ok(f.state()?["dragging"] == true))?;
    f.call("wtype", &["-k", "Escape"])?;
    outside(&mut f, &mut position)?;
    button(&mut f, "r")?;
    require(
        writes(&f)? == count && f.state()?["activations"] == json!([]),
        "Escape wrote Dock settings",
    )?;
    checks.push("Escape cancels held drag without writes or activation");
    for (action, args) in [
        ("busy", vec!["true"]),
        ("replace", vec![]),
        ("remove", vec![]),
    ] {
        reset(&f)?;
        let count = writes(&f)?;
        press(&mut f, &mut position, "a")?;
        let (x, y) = center(&f, "b")?;
        f.move_to(&mut position, x, y)?;
        wait_true("Captured Dock drag", || Ok(f.state()?["dragging"] == true))?;
        f.ipc(action, &args)?;
        outside(&mut f, &mut position)?;
        button(&mut f, "r")?;
        require(
            writes(&f)? == count && f.state()?["activations"] == json!([]),
            format!("{action} did not cancel captured pointer"),
        )?;
    }
    checks.push("Busy, removed, and same-ID replacement cancel captured pointer intent");
    reset(&f)?;
    let count = writes(&f)?;
    press(&mut f, &mut position, "a")?;
    let (x, y) = center(&f, "b")?;
    f.move_to(&mut position, x, y)?;
    outside(&mut f, &mut position)?;
    f.screenshot("removal-cue.png")?;
    button(&mut f, "r")?;
    wait_true("Dock pin removed", || {
        Ok(writes(&f)? == count + 1 && f.state()?["busy"] == false)
    })?;
    require(
        f.state()?["pins"] == json!(["fixture-b", "unknown-retained"])
            && !keys(&f.state()?).contains(&"a".to_owned()),
        "Wrong pin removed",
    )?;
    checks.push("Cross-neighbour app drag-out persists only intended pin removal and preserves unknown pins");
    f.stop(shell);
    shell = f.launch("quickshell", &["-p", &qml], "dock-restart", false)?;
    wait_true("Dock restart", || {
        let value = f.state()?;
        Ok(value["ready"] == true && value["busy"] == false)
    })?;
    require(
        f.state()?["pins"] == json!(["fixture-b", "unknown-retained"]),
        "Pin removal lost after restart",
    )?;
    checks.push("Removal survives private shell restart");
    f.launch(
        "foot",
        &[
            "--app-id=fixture-a",
            "--title=Fixture Original App",
            "sleep",
            "120",
        ],
        "original-app",
        false,
    )?;
    wait_true("Original native app", || {
        Ok(app_window(&f.tree()?, "fixture-a").is_some())
    })?;
    let original_window =
        app_window(&f.tree()?, "fixture-a").ok_or("Missing app window")?["id"].clone();
    f.call(
        service.to_str().ok_or("Service path")?,
        &[
            "--set",
            "dockPins",
            r#"["fixture-a","fixture-b","unknown-retained"]"#,
        ],
    )?;
    reset(&f)?;
    f.ipc("running", &[])?;
    wait_true("Running pinned app", || {
        Ok(keys(&f.state()?).contains(&"a".to_owned()))
    })?;
    let count = writes(&f)?;
    press(&mut f, &mut position, "a")?;
    outside(&mut f, &mut position)?;
    button(&mut f, "r")?;
    wait_true("Running pin removal", || {
        Ok(writes(&f)? == count + 1 && f.state()?["busy"] == false)
    })?;
    require(
        keys(&f.state()?).contains(&"fixture-a".to_owned())
            && f.state()?["pins"]
                .as_array()
                .is_some_and(|pins| !pins.iter().any(|pin| pin == "fixture-a")),
        "Running app disappeared or stayed pinned",
    )?;
    require(
        app_window(&f.tree()?, "fixture-a").is_some_and(|node| node["id"] == original_window),
        "Running native window changed",
    )?;
    checks.push("Running application remains available after its pin is removed, with its actual native window unchanged");
    let count = writes(&f)?;
    press(&mut f, &mut position, "fixture-a")?;
    outside(&mut f, &mut position)?;
    button(&mut f, "r")?;
    require(
        writes(&f)? == count && keys(&f.state()?).contains(&"fixture-a".to_owned()),
        "Unpinned running app removed",
    )?;
    checks.push("An unpinned running application cannot be removed by dragging out");
    for key in [
        format!(
            "file:{}",
            f.options.output.join("Original report.txt").display()
        ),
        format!(
            "folder:{}",
            f.options.output.join("Original folder").display()
        ),
    ] {
        let count = writes(&f)?;
        press(&mut f, &mut position, &key)?;
        outside(&mut f, &mut position)?;
        button(&mut f, "r")?;
        wait_true("Dock alias removal", || {
            Ok(writes(&f)? == count + 1 && f.state()?["busy"] == false)
        })?;
        require(!keys(&f.state()?).contains(&key), "Dock alias remains")?;
    }
    require(
        f.options.output.join("Fixture A.desktop").is_file()
            && fs::read_to_string(f.options.output.join("Original report.txt"))?
                == "original file remains"
            && f.options.output.join("Original folder").is_dir()
            && f.state()?["activations"] == json!([]),
        "Alias removal changed resources",
    )?;
    checks.push(
        "File and folder drag-out changes only persisted aliases; original resources survive",
    );
    f.screenshot("accepted.png")?;
    for name in ["dock", "dock-restart"] {
        let log = fs::read_to_string(f.options.output.join(format!("{name}.log")))?;
        require(
            ![
                "ReferenceError",
                "TypeError",
                "Binding loop",
                "Failed to load",
            ]
            .iter()
            .any(|marker| log.contains(marker)),
            format!("{name} QML error"),
        )?;
    }
    let source = f.options.source.join("res/lumen-desktop/shell/Dock.qml");
    let digest = hex::encode(Sha256::digest(fs::read(source)?));
    let revision = f.call(
        "git",
        &[
            "-C",
            f.options.source.to_str().ok_or("Source path")?,
            "rev-parse",
            "HEAD",
        ],
    )?;
    let result = json!({"passed":true,"checks":checks,"sourceDigest":digest,"finalState":f.state()?,"sourceRevision":revision,"writes":writes(&f)?,"liveSocketUnchanged":env::var("SWAYSOCK").unwrap_or_default()!=f.env["SWAYSOCK"]});
    fs::write(
        f.options.output.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    let _ = shell;
    Ok(())
}
