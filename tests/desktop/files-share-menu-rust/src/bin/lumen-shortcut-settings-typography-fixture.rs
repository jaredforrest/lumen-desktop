//! Real Settings parent scrolling, shortcut controls, text layout and paint.
use lumen_files_share_menu_fixture::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn run(options: &Options) -> Result<()> {
    let mut values = private_environment()?;
    let root = &options.output;
    let assets = root.join("assets");
    copy_tree(Path::new("/source/res/lumen-desktop"), &assets)?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = root.join("appearance.json");
    fs::write(
        &appearance,
        serde_json::to_vec(
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{"textScale":1}}),
        )?,
    )?;
    values.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let qml = root.join("settings-typography.qml");
    fs::write(
        &qml,
        include_str!("../../shortcut_settings_typography.qml")
            .replace("__PALETTES__", &palette.to_string())
            .replace("__OUTPUT__", root.to_str().ok_or("Output path")?),
    )?;
    values.insert(
        "FONTCONFIG_FILE".into(),
        immutable(&options.fontconfig)?.display().to_string(),
    );
    let output = Command::new("timeout")
        .args(["55s", "quickshell", "-p"])
        .arg(&qml)
        .env_clear()
        .envs(values)
        .output()?;
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(root.join("quickshell.log"), &log)?;
    // Quickshell's pinned ProxyWindowBase::onPolished unconditionally calls
    // QWindow::setMask, even for its default null mask. Qt's offscreen provider
    // cannot implement that platform operation. Preserve and count this exact
    // provider diagnostic; all other warnings remain fatal.
    let mask_diagnostics = log
        .lines()
        .filter(|line| line.ends_with("This plugin does not support setting window masks"))
        .count();
    require(
        mask_diagnostics == 8,
        "Unexpected offscreen mask diagnostic count",
    )?;
    let checked_log = log
        .lines()
        .filter(|line| {
            !line.ends_with("This plugin does not support setting window masks")
                && !line.contains("SHORTCUT_SETTINGS_TYPOGRAPHY_PASS ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    require(
        output.status.success() && log.contains("SHORTCUT_SETTINGS_TYPOGRAPHY_PASS "),
        "Actual Settings typography failed; inspect quickshell.log",
    )?;
    require(
        ![
            "warn",
            "typeerror",
            "referenceerror",
            "binding loop",
            "typography_fail",
            "typography_timeout",
        ]
        .iter()
        .any(|marker| checked_log.to_lowercase().contains(marker)),
        "Settings typography emitted a diagnostic",
    )?;
    let records: Value = serde_json::from_str(
        log.lines()
            .find_map(|line| {
                line.split_once("SHORTCUT_SETTINGS_TYPOGRAPHY_PASS ")
                    .map(|(_, v)| v)
            })
            .ok_or("Missing typography observations")?,
    )?;
    let cases = records.as_array().ok_or("Cases")?;
    require(cases.len() == 8, "Incomplete Settings typography matrix")?;
    for case in cases {
        let frames = case["frames"].as_array().ok_or("Frames")?;
        require(frames.len() == 6, "Incomplete keyboard control corpus")?;
        for frame in frames {
            let image = image::open(frame["frame"].as_str().ok_or("Frame path")?)?.to_rgb8();
            let bounds = &frame["bounds"];
            let color = frame["color"]
                .as_str()
                .ok_or("Foreground color")?
                .trim_start_matches('#');
            require(color.len() == 6, "Expected opaque foreground")?;
            let foreground = [
                u8::from_str_radix(&color[0..2], 16)?,
                u8::from_str_radix(&color[2..4], 16)?,
                u8::from_str_radix(&color[4..6], 16)?,
            ];
            let left = bounds["x"].as_f64().ok_or("x")?.max(0.) as u32;
            let top = bounds["y"].as_f64().ok_or("y")?.max(0.) as u32;
            let right = ((bounds["x"].as_f64().unwrap()
                + bounds["width"].as_f64().ok_or("width")?)
            .ceil() as u32)
                .min(image.width());
            let bottom = ((bounds["y"].as_f64().unwrap()
                + bounds["height"].as_f64().ok_or("height")?)
            .ceil() as u32)
                .min(image.height());
            require(
                left < right && top < bottom,
                "Focused control absent from actual paint",
            )?;
            let ink = (top..bottom)
                .flat_map(|y| (left..right).map(move |x| (x, y)))
                .filter(|(x, y)| {
                    image
                        .get_pixel(*x, *y)
                        .0
                        .iter()
                        .zip(foreground)
                        .all(|(a, b)| a.abs_diff(b) <= 16)
                })
                .count();
            require(
                ink >= 3,
                &format!(
                    "Focused text/control foreground absent: {}",
                    frame["target"]
                ),
            )?;
        }
    }
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"cases":records,"frames":48,"offscreenProvider":{"diagnostic":"This plugin does not support setting window masks","count":mask_diagnostics,"reason":"Pinned Quickshell ProxyWindowBase::onPolished unconditionally invokes QWindow::setMask even for default null mask; Qt offscreen does not implement platform input masks. No other diagnostics allowed."},"scope":"Actual SettingsWindow, parent Flickable focus reveal and production shortcut controls; synthetic UI-only service rejecting mutations; real Qt Tab/Backtab/type/discard and painted foreground. No native compositor, deployment or reference-exact claim."}),
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
        eprintln!("FAIL Settings shortcut typography: {error}");
        std::process::exit(1);
    }
}
