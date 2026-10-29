//! Private software acceptance of the real chooser, process events and failures.
use lumen_files_share_menu_fixture::*;
use serde_json::{Value, json};
use std::{env, fs, os::unix::fs::MetadataExt, path::Path, process::Command};
fn run(options: &Options) -> Result<()> {
    let mut values = private_environment()?;
    let installed = launcher_environment(&options.launcher)?;
    for key in [
        "PATH",
        "QML_IMPORT_PATH",
        "QT_PLUGIN_PATH",
        "LUMEN_DESKTOP_ASSETS",
    ] {
        if let Some(v) = installed.get(key) {
            values.insert(key.into(), v.clone());
        }
    }
    values.insert(
        "FONTCONFIG_FILE".into(),
        options.fontconfig.display().to_string(),
    );
    let out = &options.output;
    let assets = out.join("assets");
    copy_tree(Path::new("/source/res/lumen-desktop"), &assets)?;
    let mut seams = vec![];
    let helper = env::current_exe()?
        .parent()
        .unwrap()
        .join("lumen-share-qml-helper");
    let state_path = assets.join("shell/ShareDestinationsState.qml");
    let before = fs::read_to_string(&state_path)?;
    require(
        before.matches("\"lumen-share-destinations\"").count() == 2,
        "Both registry commands expected",
    )?;
    replace_once(
        &state_path,
        &before,
        &before.replace(
            "\"lumen-share-destinations\"",
            &serde_json::to_string(&helper)?,
        ),
        "Disposable helper, never a transport",
        &mut seams,
    )?;
    replace_once(
        &assets.join("shell/AppLaunch.qml"),
        "QtObject {",
        "QtObject {\n    property var recordedCommands:[]",
        "Record commands",
        &mut seams,
    )?;
    replace_once(
        &assets.join("shell/AppLaunch.qml"),
        "function run(command, workingDirectory) {",
        "function run(command, workingDirectory) {\n        recordedCommands=recordedCommands.concat([command]);return;",
        "Never dispatch user data",
        &mut seams,
    )?;
    let folder = Path::new("/home/fixture/Documents");
    fs::create_dir(folder)?;
    let original = folder.join("literal $(no-command).txt");
    fs::write(&original, "Original synthetic bytes")?;
    let info = original.metadata()?;
    let intent = json!({"path":folder,"tab":"fixture","selection":[{"path":original,"device":info.dev().to_string(),"inode":info.ino().to_string()}]});
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let mut records = vec![];
    for mode in ["light", "dark"] {
        for width in [320, 600] {
            for scale in [1., 1.5] {
                let name = format!("{mode}-{width}-{scale}");
                let case = out.join(&name);
                fs::create_dir(&case)?;
                let appearance = case.join("appearance.json");
                fs::write(&appearance,json!({"font":palette["font"],"palette":palette[mode],"preferences":{"textScale":scale}}).to_string())?;
                values.insert(
                    "LUMEN_APPEARANCE_STATE".into(),
                    appearance.display().to_string(),
                );
                values.insert(
                    "SHARE_FIXTURE_REQUEST_LOG".into(),
                    case.join("requests.json").display().to_string(),
                );
                let qml = out.join(format!("{name}.qml"));
                let source = include_str!("../../share_fixture.qml")
                    .replace("FIXTURE_INTENT", &intent.to_string())
                    .replace("FIXTURE_WIDTH", &width.to_string())
                    .replace("FIXTURE_SCALE", &scale.to_string())
                    .replace(
                        "FIXTURE_DARK",
                        if mode == "dark" { "true" } else { "false" },
                    )
                    .replace("FIXTURE_IMAGE", case.join("chooser.png").to_str().unwrap());
                fs::write(&qml, source)?;
                let output = Command::new("timeout")
                    .args(["20s", "quickshell", "-p"])
                    .arg(&qml)
                    .env_clear()
                    .envs(&values)
                    .output()?;
                let log = String::from_utf8_lossy(&output.stdout).to_string()
                    + &String::from_utf8_lossy(&output.stderr);
                fs::write(case.join("qml.log"), &log)?;
                require(
                    output.status.success(),
                    &format!("Share chooser failed {log}"),
                )?;
                require(
                    ![
                        "warn",
                        "typeerror",
                        "referenceerror",
                        "binding loop",
                        "share_destinations_fail",
                    ]
                    .iter()
                    .any(|m| log.to_lowercase().contains(m)),
                    &format!("Unexpected chooser diagnostic {log}"),
                )?;
                let result: Value = serde_json::from_str(
                    log.lines()
                        .find_map(|line| {
                            line.split_once("SHARE_DESTINATIONS_PASS ").map(|(_, r)| r)
                        })
                        .ok_or("No chooser success record")?,
                )?;
                let requests: Vec<Value> =
                    serde_json::from_slice(&fs::read(case.join("requests.json"))?)?;
                require(requests.len() == 4, "Exactly four explicit requests")?;
                require(
                    requests.iter().all(|r| r["files"] == intent["selection"]),
                    "Captured file identity unchanged",
                )?;
                require(
                    fs::read(&original)? == b"Original synthetic bytes",
                    "Original changed",
                )?;
                require(
                    fs::metadata(case.join("chooser.png"))?.len() > 1000,
                    "Software capture absent",
                )?;
                let frame = image::open(case.join("chooser.png"))?.to_rgb8();
                let colors = frame
                    .pixels()
                    .map(|pixel| pixel.0)
                    .collect::<std::collections::HashSet<_>>();
                require(
                    colors.len() > 32,
                    "Software frame must contain painted chooser content",
                )?;
                records.push(result);
            }
        }
    }
    let before = fs::read_to_string(&state_path)?;
    replace_once(
        &state_path,
        &before,
        &before.replace(
            &serde_json::to_string(&helper)?,
            "\"/artifacts/nonexistent-helper\"",
        ),
        "Actual FailedToStart negative control",
        &mut seams,
    )?;
    let qml = out.join("startup-failure.qml");
    fs::write(&qml, include_str!("../../share_startup_fixture.qml"))?;
    let output = Command::new("timeout")
        .args(["10s", "quickshell", "-p"])
        .arg(qml)
        .env_clear()
        .envs(&values)
        .output()?;
    let log = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    fs::write(out.join("startup-failure.log"), &log)?;
    require(
        output.status.success(),
        &format!("Startup negative failed {log}"),
    )?;
    let startup: Value = serde_json::from_str(
        log.lines()
            .find_map(|line| line.split_once("SHARE_STARTUP_PASS ").map(|(_, r)| r))
            .ok_or("No startup result")?,
    )?;
    require(
        startup["terminalSignals"] == 1,
        "Exactly one terminal signal",
    )?;
    require(
        log.matches("Process failed to start").count() == 2,
        "Exactly two deliberate missing-helper warnings",
    )?;
    fs::write(
        out.join("startup-failure.json"),
        serde_json::to_vec_pretty(&startup)?,
    )?;
    fs::write(
        out.join("fixture-seams.json"),
        serde_json::to_vec_pretty(&seams)?,
    )?;
    fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"cases":records,"scope":"Software chooser and actual Process event acceptance; no transport or native claim"}),
        )?,
    )?;
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let options = options()?;
        if options.run {
            run(&options)
        } else if options.inside {
            inner(&options)
        } else {
            outer(&options)
        }
    })();
    if let Err(e) = result {
        eprintln!("FAIL share chooser: {e}");
        std::process::exit(1)
    }
}
