//! Actual software Qt input/paint lifecycle acceptance in the private boundary.
use lumen_files_share_menu_fixture::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn run(options: &Options) -> Result<()> {
    let mut values = private_environment()?;
    let installed = launcher_environment(&options.launcher)?;
    for key in [
        "PATH",
        "QML_IMPORT_PATH",
        "QT_PLUGIN_PATH",
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
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let mut records = vec![];
    if let Some(baseline) = &options.window_source {
        let appearance = out.join("baseline-appearance.json");
        fs::write(
            &appearance,
            json!({"font":palette["font"],"palette":palette["light"],"preferences":{}}).to_string(),
        )?;
        values.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            appearance.display().to_string(),
        );
        let baseline_assets = out.join("baseline-assets");
        copy_tree(Path::new("/source/res/lumen-desktop"), &baseline_assets)?;
        fs::copy(
            baseline,
            baseline_assets.join("shell/BatteryMenuButton.qml"),
        )?;
        let qml = out.join("baseline.qml");
        fs::write(&qml, include_str!("../../tooltip_baseline.qml"))?;
        let output = Command::new("timeout")
            .args(["5s", "quickshell", "-p"])
            .arg(qml)
            .env_clear()
            .envs(&values)
            .output()?;
        let log = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        fs::write(out.join("baseline.log"), &log)?;
        require(
            output.status.success() && log.contains("TOOLTIP_BASELINE_REPRODUCED"),
            &format!("Negative baseline failed {log}"),
        )?;
    }
    for mode in ["light", "dark"] {
        for scale in [1., 1.5] {
            let name = format!("{mode}-{scale}");
            let case = out.join(&name);
            fs::create_dir(&case)?;
            let appearance = case.join("appearance.json");
            fs::write(&appearance,json!({"font":palette["font"],"palette":palette[mode],"preferences":{"textScale":scale}}).to_string())?;
            values.insert(
                "LUMEN_APPEARANCE_STATE".into(),
                appearance.display().to_string(),
            );
            let qml = out.join(format!("{name}.qml"));
            fs::write(
                &qml,
                include_str!("../../tooltip_fixture.qml")
                    .replace("FIXTURE_IMAGE", case.join("tooltip.png").to_str().unwrap()),
            )?;
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
                &format!("Tooltip input failed {log}"),
            )?;
            require(
                ![
                    "warn",
                    "typeerror",
                    "referenceerror",
                    "binding loop",
                    "tooltip_fail",
                ]
                .iter()
                .any(|marker| log.to_lowercase().contains(marker)),
                &format!("Tooltip diagnostic {log}"),
            )?;
            let record: Value = serde_json::from_str(
                log.lines()
                    .find_map(|line| line.split_once("TOOLTIP_PASS ").map(|(_, r)| r))
                    .ok_or("No tooltip result")?,
            )?;
            let image = image::open(case.join("tooltip.png"))?.to_rgb8();
            let colors = image
                .pixels()
                .map(|p| p.0)
                .collect::<std::collections::HashSet<_>>();
            require(colors.len() > 32, "Actual tooltip frame blank")?;
            records.push(record);
        }
    }
    fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"cases":records,"scope":"Actual offscreen Qt synthetic pointer/key events and painted owned tooltip; no compositor or reference parity claim"}),
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
            inner_with_timeout(&options, 60)
        } else {
            outer(&options)
        }
    })();
    if let Err(error) = result {
        eprintln!("FAIL tooltip acceptance: {error}");
        std::process::exit(1)
    }
}
