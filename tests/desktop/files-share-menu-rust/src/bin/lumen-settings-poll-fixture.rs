//! Real SettingsState read/apply and capture-identity acceptance, software only.
use lumen_files_share_menu_fixture::*;
use serde_json::{Value, json};
use std::{env, fs, path::Path, process::Command};
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
    let target = assets.join("shell/SettingsState.qml");
    let mut seams = vec![];
    if let Some(baseline) = &options.window_source {
        let sources: Value = serde_json::from_slice(&fs::read(baseline)?)?;
        for name in ["SettingsState.qml", "Theme.qml"] {
            let original = immutable(Path::new(
                sources[name]
                    .as_str()
                    .ok_or("Missing immutable baseline source")?,
            ))?;
            let path = assets.join("shell").join(name);
            let before = fs::read_to_string(&path)?;
            replace_once(
                &path,
                &before,
                &fs::read_to_string(original)?,
                "Immutable committed baseline component",
                &mut seams,
            )?;
        }
    }
    let before = fs::read_to_string(&target)?;
    require(
        before.matches("\"lumen-appearance\"").count() == 2,
        "Actual read/write command sites expected",
    )?;
    let helper = env::current_exe()?
        .parent()
        .unwrap()
        .join("lumen-settings-poll-provider");
    replace_once(
        &target,
        &before,
        &before.replace("\"lumen-appearance\"", &serde_json::to_string(&helper)?),
        "Real disposable protocol with local recorded requests only",
        &mut seams,
    )?;
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
    values.insert(
        "SETTINGS_POLL_STORE".into(),
        out.join("provider-values.json").display().to_string(),
    );
    values.insert(
        "SETTINGS_POLL_LOG".into(),
        out.join("provider-requests.jsonl").display().to_string(),
    );
    let qml = out.join("fixture.qml");
    fs::write(
        &qml,
        include_str!("../../settings_poll_fixture.qml").replace(
            "EXPECTED_BASELINE",
            if options.window_source.is_some() {
                "true"
            } else {
                "false"
            },
        ),
    )?;
    let output = Command::new("timeout")
        .args(["10s", "quickshell", "-p"])
        .arg(qml)
        .env_clear()
        .envs(&values)
        .output()?;
    let log = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    fs::write(out.join("qml.log"), &log)?;
    require(
        output.status.success(),
        &format!("Settings poll failed {log}"),
    )?;
    require(
        ![
            "warn",
            "typeerror",
            "referenceerror",
            "binding loop",
            "settings_poll_fail",
        ]
        .iter()
        .any(|m| log.to_lowercase().contains(m)),
        &format!("Settings poll diagnostic {log}"),
    )?;
    let mut result: Value = serde_json::from_str(
        log.lines()
            .find_map(|line| line.split_once("SETTINGS_POLL_PASS ").map(|(_, r)| r))
            .ok_or("No actual SettingsState result")?,
    )?;
    let requests = fs::read_to_string(out.join("provider-requests.jsonl"))?
        .lines()
        .map(serde_json::from_str::<Vec<String>>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    require(
        requests.len() == 4 && requests.iter().filter(|r| r[0] == "--set").count() == 1,
        "Exactly three actual reads and one explicit queued edit",
    )?;
    result["actualRequests"] = json!(requests);
    result["passed"] = json!(true);
    fs::write(out.join("result.json"), serde_json::to_vec_pretty(&result)?)?;
    fs::write(
        out.join("fixture-seams.json"),
        serde_json::to_vec_pretty(&seams)?,
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
        eprintln!("FAIL settings poll: {e}");
        std::process::exit(1)
    }
}
