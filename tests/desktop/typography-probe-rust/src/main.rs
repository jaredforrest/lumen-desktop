//! Run the typography specimen in Qt Quick and report its rendered metrics.
use serde_json::Value;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn run(input: &Path, picture: &Path) -> Result<(), String> {
    let source = fs::read(input).map_err(|error| error.to_string())?;
    let config: Value = serde_json::from_slice(&source).map_err(|error| error.to_string())?;
    let cases = config["cases"].as_array().ok_or("Missing cases")?;
    let scale = config["scale"].as_i64().unwrap_or(1);
    if cases.is_empty() || !(1..=2).contains(&scale) {
        return Err("Expected cases and scale 1 or 2".to_owned());
    }
    let mut qml = include_str!("../probe.qml").to_owned();
    qml = qml.replace("CONFIG_JSON", &config.to_string());
    qml = qml.replace("OUTPUT_JSON", &Value::String(picture.to_string_lossy().into_owned()).to_string());
    qml = qml.replace("QT_VERSION", &Value::String(env!("LUMEN_TYPOGRAPHY_QT_VERSION").to_owned()).to_string());
    let source_path = picture.with_extension("qml");
    fs::write(&source_path, qml).map_err(|error| error.to_string())?;
    let output = Command::new(env!("LUMEN_TYPOGRAPHY_QML"))
        .arg("--software")
        .arg(&source_path)
        .env("FONTCONFIG_FILE", env!("LUMEN_TYPOGRAPHY_FONTCONFIG"))
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QPA_PLATFORMTHEME", "generic")
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .env("QT_ACCESSIBILITY", "0")
        .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0")
        .env("NO_AT_BRIDGE", "1")
        .output()
        .map_err(|error| error.to_string())?;
    let _ = fs::remove_file(&source_path);
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    let result = diagnostics.lines().find_map(|line| line.split_once("LUMEN_TYPOGRAPHY_RESULT ").map(|(_, json)| json));
    if !output.status.success() || !picture.is_file() || result.is_none() {
        return Err(format!("Qt Quick specimen failed: {diagnostics}"));
    }
    let parsed: Value = serde_json::from_str(result.unwrap()).map_err(|error| error.to_string())?;
    println!("{parsed}");
    Ok(())
}

fn main() {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.len() != 2 {
        eprintln!("Usage: typography-probe INPUT.json OUTPUT.png");
        std::process::exit(2);
    }
    if let Err(error) = run(Path::new(&arguments[0]), Path::new(&arguments[1])) {
        eprintln!("typography-probe: {error}");
        std::process::exit(1);
    }
}
