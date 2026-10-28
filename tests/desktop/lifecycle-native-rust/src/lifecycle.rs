use crate::harness::{Fixture, Options, Result, read, require, save, wait, wait_true};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

const APP_ID: &str = "org.example.Lifecycle";
fn state(f: &Fixture) -> Result<Value> {
    f.state("lifecycleAudit", "last-lifecycle.json")
}
fn app_state(f: &Fixture) -> Result<Value> {
    read(&f.options.output.join("client-state.json"))
}
fn app(f: &Fixture, count: Option<i64>, zero: bool) -> Result<Option<Value>> {
    Ok(state(f)?["apps"].as_array().and_then(|apps| {
        apps.iter()
            .find(|app| {
                app["appId"] == APP_ID
                    && app["zeroWindows"] == zero
                    && count.is_none_or(|count| app["windowCount"] == count)
            })
            .cloned()
    }))
}
fn painted_zero(f: &Fixture, name: &str) -> Result<bool> {
    let image = f.options.output.join(name);
    f.call("grim", &[image.to_str().ok_or("Image path UTF-8")?])?;
    let file = fs::File::open(image)?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    require(
        info.width > 720 && info.height > 500,
        "Zero-window capture too small",
    )?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err("Zero-window capture is not RGB".into()),
    };
    let offset = (500 * info.width as usize + 720) * channels;
    Ok(pixels[offset..offset + 3]
        .iter()
        .map(|byte| *byte as u32)
        .sum::<u32>()
        < 80)
}
fn clean_shell(f: &Fixture) -> Result<()> {
    for name in ["desktop.log", "desktop-restarted.log"] {
        let log = fs::read_to_string(f.options.output.join(name))?;
        for error in [
            "TypeError:",
            "ReferenceError:",
            "Failed to load configuration",
        ] {
            require(!log.contains(error), format!("Shell error {error}: {log}"))?;
        }
    }
    Ok(())
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    let qml = f.qml.clone();
    let desktop = f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().unwrap()],
        "desktop.log",
    )?;
    wait_true(
        "lifecycle desktop entry",
        || Ok(state(&f)?["entry"] == true),
    )?;
    let client = f.sibling("lifecycle-client")?;
    let output = f.options.output.display().to_string();
    let first_client = f.launch(&client, &[&output], "client.log")?;
    wait_true("two client windows", || Ok(app_state(&f)?["windows"] == 2))?;
    fs::write(
        f.options.output.join("native-tree.json"),
        f.call("swaymsg", &["-t", "get_tree"])?,
    )?;
    save(
        &f.options.output.join("initial-lifecycle.json"),
        &state(&f)?,
    )?;
    let initial = wait("running native app", || app(&f, Some(2), false))?;
    let instance_ids = initial["instanceIds"].clone();
    require(
        state(&f)?["running"] == true,
        "Initial running state missing",
    )?;
    fs::write(f.options.output.join("command"), "hide all windows")?;
    wait_true("client zero windows", || Ok(app_state(&f)?["windows"] == 0))?;
    let zero = wait("retained zero-window app", || app(&f, None, true))?;
    require(
        zero["instanceIds"] == instance_ids
            && f.processes[first_client].alive()?
            && state(&f)?["running"] == true,
        "Zero-window process identity lost",
    )?;
    require(
        state(&f)?["extras"]
            .as_array()
            .is_some_and(|extras| extras.contains(&json!(APP_ID))),
        "Dock extra missing",
    )?;
    wait_true("painted zero windows", || {
        painted_zero(&f, "zero-windows.png")
    })?;
    require(
        state(&f)?["recoveryAvailable"] == true,
        "Observer recovery unavailable",
    )?;
    f.ipc("lifecycleAudit", "restartObserver", &[])?;
    wait_true("observer unavailable during restart", || {
        Ok(state(&f)?["available"] == false)
    })?;
    let recovered = wait("observer recovered same process", || app(&f, None, true))?;
    require(
        recovered["instanceIds"] == instance_ids
            && state(&f)?["running"] == true
            && f.processes[first_client].alive()?,
        "Observer restart lost identity",
    )?;
    f.stop(desktop)?;
    require(
        f.processes[first_client].alive()?,
        "Shell restart exited external app",
    )?;
    f.launch(
        Path::new("quickshell"),
        &["-p", qml.to_str().unwrap()],
        "desktop-restarted.log",
    )?;
    let recovered = wait("shell recovered same process", || app(&f, None, true))?;
    require(
        recovered["instanceIds"] == instance_ids
            && state(&f)?["running"] == true
            && state(&f)?["extras"]
                .as_array()
                .is_some_and(|extras| extras.contains(&json!(APP_ID))),
        "Shell restart lost zero-window identity",
    )?;
    wait_true("recovered painted zero windows", || {
        painted_zero(&f, "recovered-zero-windows.png")
    })?;
    let picker: Value = serde_json::from_str(&f.ipc("desktop", "appSwitcher", &["begin"])?)?;
    require(
        picker["items"]
            .as_array()
            .is_some_and(|items| items.contains(&json!(APP_ID))),
        "Zero-window switcher item missing",
    )?;
    f.ipc("desktop", "appSwitcher", &["commit"])?;
    wait_true("switcher reopens two windows", || {
        Ok(app_state(&f)?["windows"] == 2)
    })?;
    let reopened = wait("reopened same process", || app(&f, Some(2), false))?;
    require(
        reopened["instanceIds"] == instance_ids && f.processes[first_client].alive()?,
        "Switcher opened replacement process",
    )?;

    fs::write(f.options.output.join("choice"), "cancel")?;
    f.ipc("lifecycleAudit", "quit", &[])?;
    wait_true("cancel request reached app", || {
        Ok(app_state(&f)?["requests"] == 1)
    })?;
    wait_true("cancel clears pending", || {
        Ok(state(&f)?["pending"] == false)
    })?;
    require(
        f.processes[first_client].alive()?
            && app_state(&f)?["documents"] == json!(["dirty first", "dirty second"]),
        "Cancel lost dirty documents",
    )?;
    require(
        !f.options.output.join("saved-0.txt").exists()
            && !f.options.output.join("saved-1.txt").exists(),
        "Cancel wrote documents",
    )?;
    fs::write(f.options.output.join("choice"), "save")?;
    f.ipc("lifecycleAudit", "quit", &[])?;
    wait_true("saved app exit", || Ok(!f.processes[first_client].alive()?))?;
    wait_true("removed running app", || {
        Ok(state(&f)?["apps"]
            .as_array()
            .is_some_and(|apps| !apps.iter().any(|app| app["appId"] == APP_ID)))
    })?;
    require(
        state(&f)?["running"] == false,
        "Running state retained dead app",
    )?;
    require(
        fs::read_to_string(f.options.output.join("saved-0.txt"))? == "dirty first"
            && fs::read_to_string(f.options.output.join("saved-1.txt"))? == "dirty second",
        "Save contents changed",
    )?;
    fs::remove_file(f.options.output.join("saved-0.txt"))?;
    fs::remove_file(f.options.output.join("saved-1.txt"))?;
    fs::remove_file(f.options.output.join("command"))?;
    let second_client = f.launch(&client, &[&output], "discard-client.log")?;
    wait_true("new independent process", || {
        Ok(app(&f, Some(2), false)?.is_some_and(|app| app["instanceIds"] != instance_ids))
    })?;
    fs::write(f.options.output.join("choice"), "discard")?;
    f.ipc("lifecycleAudit", "quit", &[])?;
    wait_true("discard app exit", || {
        Ok(!f.processes[second_client].alive()?)
    })?;
    require(
        !f.options.output.join("saved-0.txt").exists()
            && !f.options.output.join("saved-1.txt").exists(),
        "Discard wrote documents",
    )?;
    wait_true("running state gone", || Ok(state(&f)?["running"] == false))?;
    clean_shell(&f)?;
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":[
            "actual native compositor process identity",
            "pidfd-retained zero-window process",
            "observer restart recovers same validated zero-window process",
            "shell restart retains Dock/switcher identity without native windows",
            "Dock running entry retained without windows",
            "switcher zero-window item and activation reopen same process",
            "advertised GTK Quit action only",
            "dirty two-document cancel preserves process/data",
            "explicit save Quit commits both synthetic documents",
            "actual process exit removes running indicator/state",
            "separate instance explicit discard does not write documents"
        ]}),
    )?;
    Ok(())
}
