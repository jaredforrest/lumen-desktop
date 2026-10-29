//! Private offscreen acceptance for the production Files share menu.
use lumen_files_share_menu_fixture::*;
use serde_json::{Value, json};
use std::{env, fs, path::Path, process::Command};
fn run(options: &Options) -> Result<()> {
    let mut values = private_environment()?;
    require(
        env::var("DBUS_SESSION_BUS_ADDRESS")?.starts_with("unix:"),
        "Private session bus missing",
    )?;
    let installed = launcher_environment(&options.launcher)?;
    for key in [
        "PATH",
        "QML_IMPORT_PATH",
        "QT_PLUGIN_PATH",
        "LUMEN_FILES_BIN",
        "LUMEN_FILE_JOBS_BIN",
        "LUMEN_DESKTOP_ASSETS",
    ] {
        if let Some(value) = installed.get(key) {
            values.insert(key.into(), value.clone());
        }
    }
    values.insert(
        "FONTCONFIG_FILE".into(),
        options.fontconfig.display().to_string(),
    );
    let out = &options.output;
    let assets = out.join("assets");
    copy_tree(Path::new("/source/res/lumen-desktop"), &assets)?;
    let mut seams = Vec::new();
    if let Some(window) = &options.window_source {
        let mut baseline = fs::read_to_string(immutable(window)?)?;
        for (old, new) in [
            (
                "id:actions;onOpened:",
                "id:actions;objectName:\"files-actions-menu\";onOpened:",
            ),
            (
                "required property var modelData;width:208;",
                "objectName:\"files-action-\"+modelData.action;required property var modelData;width:208;",
            ),
        ] {
            require(
                baseline.matches(old).count() == 1,
                "Window source marker missing",
            )?;
            baseline = baseline.replacen(old, new, 1);
        }
        let target = assets.join("shell/FilesWindow.qml");
        let original = fs::read_to_string(&target)?;
        replace_once(
            &target,
            &original,
            &baseline,
            "Immutable candidate FilesWindow source",
            &mut seams,
        )?;
    }
    replace_once(
        &assets.join("shell/NearbySharingState.qml"),
        "readonly property bool configured:!!(Quickshell.env(\"LUMEN_NEARBY_SHARING_PROVIDER\") || \"\")",
        "readonly property bool configured:true",
        "Enable private sharing row",
        &mut seams,
    )?;
    replace_once(
        &assets.join("shell/NearbySharingState.qml"),
        "function refresh(){if(configured && !sharingReader.running)sharingReader.running=true;}",
        "function refresh(){}",
        "Prevent transport launch",
        &mut seams,
    )?;
    replace_once(
        &assets.join("shell/AppLaunch.qml"),
        "QtObject {",
        "QtObject {\n    property var recordedCommands:[]",
        "Record application commands",
        &mut seams,
    )?;
    replace_once(
        &assets.join("shell/AppLaunch.qml"),
        "function run(command, workingDirectory) {",
        "function run(command, workingDirectory) {\n        recordedCommands=recordedCommands.concat([command]);return;",
        "Record without dispatch",
        &mut seams,
    )?;
    fs::write(
        out.join("fixture-seams.json"),
        serde_json::to_vec_pretty(&seams)?,
    )?;
    let folder = Path::new("/home/fixture/Documents");
    fs::create_dir(folder)?;
    let first = folder.join("a $(literal).txt");
    let second = folder.join("b.txt");
    fs::write(&first, "first unchanged")?;
    fs::write(&second, "second unchanged")?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = out.join("appearance.json");
    fs::write(
        &appearance,
        json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}).to_string(),
    )?;
    values.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let qml = out.join("fixture.qml");
    fs::write(&qml, include_str!("../fixture.qml"))?;
    let output = Command::new("timeout")
        .args(["15s", "quickshell", "-p"])
        .arg(&qml)
        .env_clear()
        .envs(&values)
        .output()?;
    let log = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    fs::write(out.join("qml.log"), &log)?;
    require(
        output.status.success(),
        &format!("Files share QML failed: {log}"),
    )?;
    require(
        ![
            "ReferenceError",
            "TypeError",
            "binding loop",
            "FILES_SHARE_MENU_FAIL",
        ]
        .iter()
        .any(|marker| log.to_lowercase().contains(&marker.to_lowercase())),
        &format!("QML errors: {log}"),
    )?;
    let marker = "FILES_SHARE_MENU_PASS ";
    let record = log
        .lines()
        .find_map(|line| line.split_once(marker).map(|(_, value)| value))
        .ok_or("QML success record missing")?;
    let mut result: Value = serde_json::from_str(record)?;
    require(
        fs::read_to_string(&first)? == "first unchanged"
            && fs::read_to_string(&second)? == "second unchanged",
        "Disposable file bytes changed",
    )?;
    result["passed"] = json!(true);
    result["scope"] = json!(
        "Actual production Files menu signal dispatch; no physical pointer, transport or delivery claim"
    );
    result["namespace"] = json!({"status":fs::read_to_string("/proc/self/status")?,"mounts":fs::read_to_string("/proc/self/mountinfo")?});
    fs::write(out.join("result.json"), serde_json::to_vec_pretty(&result)?)?;
    println!("PASS private Files share menu dispatch");
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
    if let Err(error) = result {
        eprintln!("Files share menu fixture failed: {error}");
        std::process::exit(1)
    }
}
