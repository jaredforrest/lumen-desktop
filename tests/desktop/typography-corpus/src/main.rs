//! Exercise Qt Quick text metrics and painted typography in a private offscreen session.
use image::RgbImage;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Environment = HashMap<OsString, OsString>;

struct Args {
    probe: PathBuf,
    launcher: PathBuf,
    output: PathBuf,
    family: String,
}

fn arguments() -> Result<Args> {
    let mut probe = None;
    let mut launcher = None;
    let mut output = None;
    let mut family = "Interlude Variable".to_owned();
    let mut input = env::args_os().skip(1);
    while let Some(argument) = input.next() {
        let value = input.next().ok_or("Each option needs a value")?;
        match argument.to_str() {
            Some("--probe") => probe = Some(PathBuf::from(value)),
            Some("--launcher") => launcher = Some(PathBuf::from(value)),
            Some("--output") => output = Some(PathBuf::from(value)),
            Some("--family") => family = value.into_string().map_err(|_| "Invalid font family")?,
            _ => return Err(format!("Unknown option: {}", argument.to_string_lossy()).into()),
        }
    }
    Ok(Args {
        probe: probe.ok_or("--probe is required")?,
        launcher: launcher.ok_or("--launcher is required")?,
        output: output.ok_or("--output is required")?,
        family,
    })
}

fn private_environment(private: &Path) -> Environment {
    let mut values: Environment = env::vars_os().collect();
    for key in [
        "DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        values.remove(OsStr::new(key));
    }
    for (key, value) in [
        ("HOME", private.to_path_buf()),
        ("XDG_RUNTIME_DIR", private.to_path_buf()),
        ("XDG_CONFIG_HOME", private.join("config")),
        ("XDG_STATE_HOME", private.join("state")),
        ("XDG_CACHE_HOME", private.join("cache")),
        ("XDG_DATA_HOME", private.join("data")),
    ] {
        values.insert(key.into(), value.into_os_string());
    }
    for (key, value) in [
        ("QT_QPA_PLATFORM", "offscreen"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        values.insert(key.into(), value.into());
    }
    values
}

fn export_environment(launcher: &Path, values: &mut Environment) -> Result<()> {
    let output = Command::new(launcher)
        .arg("--print-env0")
        .env_clear()
        .envs(values.iter())
        .output()?;
    if !output.status.success() {
        return Err("Desktop launcher did not export its environment".into());
    }
    for entry in output.stdout.split(|byte| *byte == 0) {
        if let Some(position) = entry.iter().position(|byte| *byte == b'=') {
            let key = String::from_utf8(entry[..position].to_vec())?;
            let value = String::from_utf8(entry[position + 1..].to_vec())?;
            values.insert(key.into(), value.into());
        }
    }
    values.remove(OsStr::new("QT_PLUGIN_PATH"));
    values.remove(OsStr::new("QML_IMPORT_PATH"));
    for (key, value) in [
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QPA_PLATFORM", "offscreen"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        values.insert(key.into(), value.into());
    }
    Ok(())
}

fn installed_appearance(values: &Environment) -> Result<PathBuf> {
    let path = values.get(OsStr::new("PATH")).ok_or("Launcher PATH is missing")?;
    for directory in env::split_paths(path) {
        let candidate = directory.join("lumen-appearance");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("Installed Appearance command is unavailable".into())
}

fn command_output(program: &Path, args: &[&OsStr], values: &Environment, timeout: Duration) -> Result<Output> {
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .envs(values.iter())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(child.wait_with_output()?);
        }
        if Instant::now() >= deadline {
            child.kill()?;
            let _ = child.wait_with_output();
            return Err(format!("Timed out: {}", program.display()).into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn cases() -> Vec<Value> {
    vec![
        json!({"name":"Ordinary time and digits","text":"00:00  09:41  12:34:56  23:59:59  0123456789","size":24}),
        json!({"name":"Tabular digits","text":"000000 111111 222222 333333 444444 555555 666666 777777 888888 999999","size":20,"tabular":true}),
        json!({"name":"Title","text":"Jérôme’s résumé — Søren, Łódź, Zürich & İstanbul","size":26}),
        json!({"name":"Menu and punctuation","text":"File  Edit  View  “Open…”  Save As…  ⌘ ⌥ ⇧ ⌃ ↩ ⎋ • — – ([]{})","size":20}),
        json!({"name":"Message and accented names","text":"São Paulo · François · Dvořák · Smørrebrød · naïve · Ångström","size":20}),
        json!({"name":"Greek and Cyrillic","text":"Καλημέρα κόσμε · Ελληνικά · Добро пожаловать · Кириллица","size":22}),
        json!({"name":"Arabic and Latin numerals","text":"مرحبا بالعالم — العربية ١٢٣٤٥٦٧٨٩٠ / 09:41","size":24}),
        json!({"name":"Devanagari","text":"नमस्ते दुनिया · हिन्दी · १२३४५६७८९०","size":24}),
        json!({"name":"Japanese and Chinese","text":"日本語の名前 · 中文字符 · 2026年10月19日","size":24}),
        json!({"name":"Korean","text":"안녕하세요 · 한국어 · 2026년 10월 19일","size":24}),
        json!({"name":"Explicit text presentation","text":"1\u{fe0e} 2\u{fe0e} #\u{fe0e} *\u{fe0e} 0123456789","size":24}),
        json!({"name":"Explicit emoji presentation","text":"😀 🌍 🇬🇧 👍🏽 👩\u{200d}💻 1\u{fe0f}\u{20e3}","size":30,"emoji":true}),
    ]
}

fn validate_metrics(requested: &[Value], rendered: &Value) -> Result<()> {
    let shaped = rendered["cases"].as_array().ok_or("Probe cases are missing")?;
    if shaped.len() != requested.len() {
        return Err("Probe returned the wrong number of cases".into());
    }
    for (case, result) in requested.iter().zip(shaped) {
        let name = case["name"].as_str().ok_or("Case name is missing")?;
        if result["name"] != case["name"] || result["text"] != case["text"] {
            return Err(format!("Text specimen changed: {name}").into());
        }
        if result["width"].as_f64().unwrap_or(0.0) <= 0.0 {
            return Err(format!("Text width is empty: {name}").into());
        }
        if result["height"].as_f64().unwrap_or(0.0) <= 0.0 {
            return Err(format!("Text height is empty: {name}").into());
        }
        if case["tabular"] == true {
            let widths = result["digitWidths"].as_array().ok_or("Digit widths are missing")?;
            let digits: Vec<f64> = widths.iter().map(|width| width.as_f64().unwrap_or(f64::NAN)).collect();
            if digits.len() != 10 || digits.iter().any(|width| !width.is_finite()) {
                return Err("Tabular digit measurements are incomplete".into());
            }
            let min = digits.iter().copied().fold(f64::INFINITY, f64::min);
            let max = digits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if max - min >= 0.02 {
                return Err(format!("Tabular digit widths diverged: {name}").into());
            }
        }
    }
    Ok(())
}

fn painted_row(image: &RgbImage, index: usize, scale: u32) -> Result<bool> {
    let x0 = 24 * scale;
    let x1 = 1376 * scale;
    let y0 = (37 + index as u32 * 76) * scale;
    let y1 = (75 + index as u32 * 76) * scale;
    if x1 > image.width() || y1 > image.height() {
        return Err("Probe image is smaller than the declared corpus".into());
    }
    let mut sum = [0.0_f64; 3];
    let mut squares = [0.0_f64; 3];
    let count = f64::from((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        for x in x0..x1 {
            let pixel = image.get_pixel(x, y).0;
            for channel in 0..3 {
                let value = f64::from(pixel[channel]);
                sum[channel] += value;
                squares[channel] += value * value;
            }
        }
    }
    Ok((0..3).any(|channel| {
        let mean = sum[channel] / count;
        ((squares[channel] / count) - mean * mean).max(0.0).sqrt() > 2.0
    }))
}

fn color_emoji_in_row(image: &RgbImage, index: usize, scale: u32) -> bool {
    let x0 = 24 * scale;
    let x1 = 1376 * scale;
    let y0 = (37 + index as u32 * 76) * scale;
    let y1 = (75 + index as u32 * 76) * scale;
    (y0..y1).any(|y| (x0..x1).any(|x| {
        let pixel = image.get_pixel(x, y).0;
        let low = *pixel.iter().min().unwrap();
        let high = *pixel.iter().max().unwrap();
        high - low > 45
    }))
}

fn run() -> Result<()> {
    let args = arguments()?;
    fs::create_dir(&args.output)?;
    let output = args.output.canonicalize()?;
    let private = tempfile::Builder::new().prefix("lumen-typography-").tempdir()?;
    let mut values = private_environment(private.path());
    export_environment(&args.launcher, &mut values)?;
    let appearance = installed_appearance(&values)?;
    let requested = cases();
    let mut results = Vec::new();
    for mode in ["light", "dark"] {
        let palette_output = command_output(
            &appearance,
            &[OsStr::new("--resolve-palette"), OsStr::new(mode), OsStr::new("blue"), OsStr::new("false")],
            &values,
            Duration::from_secs(5),
        )?;
        if !palette_output.status.success() {
            return Err(format!("Appearance palette failed: {mode}: {}", String::from_utf8_lossy(&palette_output.stderr)).into());
        }
        let palette: Value = serde_json::from_slice(&palette_output.stdout)?;
        for scale in [1_u32, 2] {
            let stem = format!("{mode}-{scale}x");
            let config = json!({
                "family": &args.family,
                "background": palette["background"],
                "foreground": palette["text"],
                "muted": palette["muted"],
                "scale": scale,
                "cases": &requested,
            });
            let input = output.join(format!("{stem}.json"));
            let picture = output.join(format!("{stem}.png"));
            fs::write(&input, serde_json::to_vec(&config)?)?;
            let rendered = command_output(
                &args.probe,
                &[input.as_os_str(), picture.as_os_str()],
                &values,
                Duration::from_secs(30),
            )?;
            let diagnostics = String::from_utf8(rendered.stderr)?;
            fs::write(output.join(format!("{stem}.log")), &diagnostics)?;
            if !rendered.status.success() || diagnostics.to_lowercase().contains("warning") {
                return Err(format!("Typography probe failed: {stem}: {diagnostics}").into());
            }
            let mut shaped: Value = serde_json::from_slice(&rendered.stdout)?;
            validate_metrics(&requested, &shaped)?;
            let image = image::open(&picture)?.into_rgb8();
            for (index, case) in requested.iter().enumerate() {
                if !painted_row(&image, index, scale)? {
                    return Err(format!("Unpainted corpus row: {}", case["name"]).into());
                }
                if color_emoji_in_row(&image, index, scale) != case["emoji"].as_bool().unwrap_or(false) {
                    return Err(format!("Unexpected color emoji pixels: {}", case["name"]).into());
                }
            }
            shaped.as_object_mut().ok_or("Probe result is not an object")?
                .insert("mode".to_owned(), json!(mode));
            results.push(shaped);
        }
    }
    let report = json!({
        "passed": true,
        "renderer": "Qt Quick native text, software scene graph",
        "scope": "Qt Quick text metrics, tabular widths, painted corpus and color emoji placement; glyph-run fallback identity and missing-glyph counts are not exposed by QML",
        "results": results,
    });
    fs::write(output.join("result.json"), format!("{}\n", serde_json::to_string_pretty(&report)?))?;
    println!("PASS typography corpus: light/dark at 1x/2x, painted rows, color emoji placement and tabular digit widths");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("typography corpus: {error}");
        std::process::exit(1);
    }
}
