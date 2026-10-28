//! Live translucency policy on mapped native windows, popovers, and Dock.
use lumen_display_visual_fixtures::{
    Raster, Result, Session, copy_tree, ensure, fail, number, pause, round, s, sha_file, wait,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const ALL: [&str; 7] = [
    "remote",
    "help",
    "session",
    "panel",
    "calendar",
    "notification",
    "dock",
];
struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    output: PathBuf,
    source: PathBuf,
    effects: bool,
    surfaces: Vec<String>,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut launcher = None;
        let mut sway = None;
        let mut pointer = None;
        let mut output = None;
        let mut source = env::current_dir()?.join("res/lumen-desktop");
        let mut effects = false;
        let mut surfaces = Vec::new();
        let mut args = env::args_os().skip(1).peekable();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--source") => source = PathBuf::from(args.next().ok_or("Missing source")?),
                Some("--effects") => effects = true,
                Some("--surfaces") => {
                    while args
                        .peek()
                        .is_some_and(|v| !v.to_string_lossy().starts_with("--"))
                    {
                        let value = args.next().unwrap().to_string_lossy().into_owned();
                        ensure(
                            ALL.contains(&value.as_str()),
                            format!("Unknown surface {value}"),
                        )?;
                        surfaces.push(value);
                    }
                }
                _ => {
                    return Err(fail(format!(
                        "Unknown transparency option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        let output = output.ok_or("--output required")?;
        Ok(Self {
            launcher: launcher.ok_or("--launcher required")?,
            sway: sway.ok_or("--sway required")?,
            pointer: pointer.ok_or("--pointer-tool required")?,
            output: if output.is_absolute() {
                output
            } else {
                env::current_dir()?.join(output)
            },
            source,
            effects,
            surfaces: if surfaces.is_empty() {
                ALL.iter().map(|s| (*s).to_owned()).collect()
            } else {
                surfaces
            },
        })
    }
}
fn theme(path: &Path, palette: &Value, mode: &str, reduced: bool) -> Result<()> {
    let value = json!({"font":palette["font"],"palette":palette[mode],"preferences":{"mode":mode,"reduceMotion":true,"reduceTransparency":reduced,"dockMagnification":false,"dockSize":48,"dockAutoHide":false}});
    let pending = path.with_extension("pending");
    fs::write(&pending, serde_json::to_vec(&value)?)?;
    fs::rename(pending, path)?;
    Ok(())
}
fn ipc(f: &Session, qml: &Path, method: &str, args: &[&str]) -> Result<String> {
    let mut parts = vec![
        "-p",
        qml.to_str().ok_or("Non UTF-8 QML")?,
        "ipc",
        "call",
        "transparencyAudit",
        method,
    ];
    parts.extend_from_slice(args);
    f.call("quickshell", &parts)
}
fn native(f: &Session, title: &str) -> Result<Option<Value>> {
    Ok(f.window(title)?.filter(|v| v["app_id"] == "org.quickshell"))
}
fn title(name: &str) -> Option<&'static str> {
    match name {
        "remote" => Some("Remote Desktop"),
        "help" => Some("Desktop Help"),
        "session" => Some("Session Options"),
        _ => None,
    }
}
fn shift_rect(rect: &mut Value, x: f64, y: f64) {
    if rect.is_object() {
        rect["x"] = json!(rect["x"].as_f64().unwrap_or(0.0) + x);
        rect["y"] = json!(rect["y"].as_f64().unwrap_or(0.0) + y);
    }
}
fn state(f: &Session, qml: &Path, output_rect: &Value) -> Result<Value> {
    let mut value: Value = serde_json::from_str(&ipc(f, qml, "state", &[])?)?;
    let name = value["active"].as_str().unwrap_or("").to_owned();
    let origin = if let Some(t) = title(&name) {
        if let Some(observed) = native(f, t)? {
            (
                number(&observed["rect"], "x")?,
                number(&observed["rect"], "y")?,
            )
        } else {
            value["window"] = Value::Null;
            (0.0, 0.0)
        }
    } else if name == "dock" {
        (
            number(output_rect, "x")?
                + (number(output_rect, "width")? - number(&value["dock"], "width")?) / 2.0,
            number(output_rect, "y")? + number(output_rect, "height")?
                - number(&value["dock"], "height")?
                - number(&value["dock"], "marginBottom")?,
        )
    } else {
        (
            number(output_rect, "x")? + number(output_rect, "width")?
                - value["window"]["width"].as_f64().unwrap_or(0.0)
                - value["layerMargins"]["right"].as_f64().unwrap_or(0.0),
            number(output_rect, "y")? + value["layerMargins"]["top"].as_f64().unwrap_or(0.0),
        )
    };
    shift_rect(&mut value["window"], origin.0, origin.1);
    shift_rect(&mut value["target"], origin.0, origin.1);
    for key in ["actions", "texts"] {
        if let Some(rows) = value[key].as_array_mut() {
            for row in rows {
                shift_rect(&mut row["rect"], origin.0, origin.1);
            }
        }
    }
    let dock_origin = (
        number(output_rect, "x")?
            + (number(output_rect, "width")? - number(&value["dock"], "width")?) / 2.0,
        number(output_rect, "y")? + number(output_rect, "height")?
            - number(&value["dock"], "height")?
            - number(&value["dock"], "marginBottom")?,
    );
    shift_rect(&mut value["dock"]["rect"], dock_origin.0, dock_origin.1);
    fs::write(
        f.out.join("last-state.json"),
        serde_json::to_string_pretty(&value)?,
    )?;
    Ok(value)
}
fn geometry(value: &Value) -> Value {
    json!({"window":value["window"],"target":value["target"],"actions":value["actions"],"texts":value["texts"]})
}
fn pointer_event(f: &mut Session, pointer: usize, event: &str) -> Result<()> {
    f.write(pointer, &format!("{event}\n"))?;
    pause(50);
    Ok(())
}
fn move_pointer(f: &mut Session, pointer: usize, x: f64, y: f64) -> Result<()> {
    f.ipc(&format!("seat seat0 cursor set {} {}", x as i64, y as i64))?;
    pointer_event(f, pointer, "m 0.1 0.1")
}
fn rgb(hex: &str) -> Result<[u8; 3]> {
    ensure(
        hex.len() == 7 && hex.starts_with('#'),
        format!("Invalid palette color {hex}"),
    )?;
    Ok([
        u8::from_str_radix(&hex[1..3], 16)?,
        u8::from_str_radix(&hex[3..5], 16)?,
        u8::from_str_radix(&hex[5..7], 16)?,
    ])
}
fn linear(value: u8) -> f64 {
    let x = value as f64 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
fn lightness(color: [u8; 3]) -> f64 {
    linear(color[0]) * 0.2126 + linear(color[1]) * 0.7152 + linear(color[2]) * 0.0722
}

fn paint(
    f: &Session,
    path: &Path,
    name: &str,
    mode: &str,
    current: &Value,
    palette: &Value,
) -> Result<Option<Value>> {
    f.capture(path, &[])?;
    let image = Raster::read(path)?;
    let window = &current["window"];
    let (x, y, w, h) = (
        round(number(window, "x")?),
        round(number(window, "y")?),
        round(number(window, "width")?),
        round(number(window, "height")?),
    );
    let crop = image.crop(x, y, x + w, y + h);
    crop.save(&path.with_file_name(format!(
        "{}-content.png",
        path.file_stem().unwrap().to_string_lossy()
    )))?;
    let color = &current["material"];
    let alpha = number(color, "a")?;
    let channels = [
        number(color, "r")? * 255.0,
        number(color, "g")? * 255.0,
        number(color, "b")? * 255.0,
    ];
    let mut points = Vec::new();
    for dx in (-32..=32).step_by(4) {
        let px = round(
            number(window, "x")?
                + if name == "remote" {
                    75.0
                } else {
                    number(window, "width")? / 2.0
                }
                + dx as f64,
        );
        let py = round(
            number(window, "y")?
                + if name == "dock" {
                    number(window, "height")? - 4.0
                } else {
                    8.0
                },
        );
        let under = if name == "remote" {
            rgb(palette[mode]["background"]
                .as_str()
                .ok_or("Missing palette background")?)?
        } else if (px.div_euclid(32) + py.div_euclid(32)) % 2 != 0 {
            [224, 32, 160]
        } else {
            [16, 208, 224]
        };
        let expected =
            [0, 1, 2].map(|i| round(channels[i] * alpha + under[i] as f64 * (1.0 - alpha)) as u8);
        let actual = image.pixel(px, py);
        let error = (0..3)
            .map(|i| (actual[i] as i64 - expected[i] as i64).abs())
            .max()
            .unwrap_or(0);
        points.push(json!({"x":px,"y":py,"actual":actual,"expected":expected,"error":error}));
    }
    let matches = points
        .iter()
        .filter(|v| v["error"].as_i64().unwrap_or(100) <= 3)
        .count();
    let foreground = rgb(palette[mode]["text"]
        .as_str()
        .ok_or("Missing palette text")?)?;
    let text_pixels = if name == "dock" {
        0
    } else {
        crop.each()
            .filter(|c| (0..3).all(|i| (c[i] as i64 - foreground[i] as i64).abs() <= 3))
            .count()
    };
    let ink = lightness(foreground);
    let contrasts = points
        .iter()
        .map(|p| {
            let a = p["actual"].as_array().unwrap();
            let color = [
                a[0].as_u64().unwrap() as u8,
                a[1].as_u64().unwrap() as u8,
                a[2].as_u64().unwrap() as u8,
            ];
            let bg = lightness(color);
            (ink.max(bg) + 0.05) / (ink.min(bg) + 0.05)
        })
        .collect::<Vec<_>>();
    let minimum = contrasts.iter().copied().fold(f64::INFINITY, f64::min);
    let diagnostics = json!({"points":points,"matches":matches,"textPixels":text_pixels,"material":color,"minimumTextContrast":minimum});
    fs::write(
        f.out.join("last-paint.json"),
        serde_json::to_string_pretty(&diagnostics)?,
    )?;
    if (matches as f64) < (points.len() as f64) * 0.9 || (name != "dock" && text_pixels < 10) {
        return Ok(None);
    }
    if name != "dock" {
        ensure(
            minimum >= 4.5,
            format!("{name} foreground contrast {minimum}"),
        )?;
    }
    ensure(
        crop.deviation().into_iter().fold(0.0, f64::max) > 2.0,
        format!("{name} unpainted interior"),
    )?;
    Ok(Some(diagnostics))
}

fn check_pointer_action(name: &str, after: &Value) -> Result<()> {
    match name {
        "remote" => ensure(after["filter"] == "VNC", "Remote filter click failed"),
        "help" => ensure(after["focused"] == true, "Help focus click failed"),
        "session" => ensure(
            after["visible"] == false && after["records"].as_array().is_some_and(Vec::is_empty),
            "Session cancel failed",
        ),
        _ => {
            let expected = match name {
                "panel" => "panel",
                "calendar" => "calendar:2026-10-20",
                "notification" => "notification:7:default",
                "dock" => "dock:two",
                _ => return Err(fail("Unknown surface")),
            };
            ensure(
                after["records"] == json!([expected]),
                format!("{name} pointer action failed: {after}"),
            )
        }
    }
}
fn check_keyboard_action(
    f: &mut Session,
    qml: &Path,
    output_rect: &Value,
    name: &str,
) -> Result<()> {
    match name {
        "help" => {
            f.call("wtype", &["k"])?;
            wait("Help query", 12, || {
                Ok((state(f, qml, output_rect)?["helpQuery"] == "k").then_some(()))
            })?;
        }
        "panel" | "calendar" | "notification" => {
            if name == "notification" {
                ensure(
                    ipc(f, qml, "focusTarget", &[])? == "true",
                    "Notification focus failed",
                )?;
            }
            f.call("wtype", &["-k", "space"])?;
            wait("keyboard action", 12, || {
                Ok((state(f, qml, output_rect)?["records"]
                    .as_array()
                    .map_or(0, Vec::len)
                    == 2)
                    .then_some(()))
            })?;
            let value = state(f, qml, output_rect)?;
            let rows = value["records"].as_array().ok_or("Records absent")?;
            ensure(
                rows[0] == rows[1],
                format!("{name} pointer and keyboard actions differ"),
            )?;
        }
        "dock" => {
            ipc(f, qml, "focusDock", &[])?;
            wait("Dock keyboard mode", 12, || {
                Ok((state(f, qml, output_rect)?["dock"]["keyboard"] == true).then_some(()))
            })?;
            f.call("wtype", &["-k", "Home", "-k", "Right", "-k", "Return"])?;
            wait("Dock keyboard action", 12, || {
                Ok((state(f, qml, output_rect)?["records"]
                    .as_array()
                    .map_or(0, Vec::len)
                    == 2)
                    .then_some(()))
            })?;
            ensure(
                state(f, qml, output_rect)?["records"] == json!(["dock:two", "dock:two"]),
                "Dock keyboard action mismatch",
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn main() {
    if let Err(error) = Options::parse().and_then(run) {
        eprintln!("live transparency: {error}");
        std::process::exit(1)
    }
}
fn run(opt: Options) -> Result<()> {
    let original_socket = env::var("SWAYSOCK").ok();
    let mut f = Session::new(&opt.output, &opt.launcher, &opt.sway, "lumen-materials-")?;
    f.set_many(&[
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if opt.effects { "gles2" } else { "pixman" }),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if opt.effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        (
            "LUMEN_COMPOSITOR_EFFECTS",
            if opt.effects { "1" } else { "0" },
        ),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
    ]);
    let assets = f.root.path().join("assets");
    copy_tree(&opt.source, &assets)?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = f.root.path().join("appearance.json");
    f.set("LUMEN_APPEARANCE_STATE", s(&appearance));
    theme(&appearance, &palette, "dark", false)?;
    let qml = f.root.path().join("fixture.qml");
    fs::write(&qml, include_str!("../../transparency.qml"))?;
    f.start_bus()?;
    let config = f.root.path().join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1600x1000\nseat seat0 fallback true\nfocus_follows_mouse no\nxwayland disable\nfor_window [all] floating enable, border pixel 0, move position 40 60\n",
    )?;
    f.launch(
        &s(&opt.sway),
        &["--unsupported-gpu", "-c", &s(&config)],
        "sway.log",
        false,
    )?;
    f.wait_sway(1)?;
    let output_rect = f
        .outputs()?
        .into_iter()
        .find(|v| v["name"] == "HEADLESS-1")
        .ok_or("HEADLESS-1 missing")?["rect"]
        .clone();
    f.launch("wtype", &["-s", "120000"], "keyboard.log", false)?;
    let pointer = f.launch(
        &s(&opt.pointer),
        &["pointer", "move", "0", "0"],
        "pointer.log",
        true,
    )?;
    f.launch("quickshell", &["-p", &s(&qml)], "shell.log", false)?;
    wait("transparency IPC", 12, || {
        Ok((!ipc(&f, &qml, "state", &[])?.is_empty()).then_some(()))
    })?;
    let mut results = Vec::new();
    for mode in ["light", "dark"] {
        for name in &opt.surfaces {
            theme(&appearance, &palette, mode, false)?;
            wait("theme application", 12, || {
                let value = state(&f, &qml, &output_rect)?;
                Ok((value["dark"] == (mode == "dark") && value["reduced"] == false).then_some(()))
            })?;
            ipc(&f, &qml, "open", &[name])?;
            let before = wait("surface and target", 12, || {
                let v = state(&f, &qml, &output_rect)?;
                Ok((v["window"]["width"].as_f64().unwrap_or(0.0) > 0.0
                    && (name == "dock" || !v["target"].is_null()))
                .then_some(v))
            })?;
            let native_before = if let Some(t) = title(name) {
                Some(wait("native surface", 12, || native(&f, t))?)
            } else {
                None
            };
            let identifier = native_before.as_ref().map(|n| n["id"].clone());
            let point = if name == "dock" {
                let d = &before["dock"];
                (
                    number(&d["rect"], "x")? + 12.0 + 1.5 * number(d, "slot")?,
                    number(&d["rect"], "y")? + number(d, "height")? - 35.0,
                )
            } else {
                let r = &before["target"];
                (
                    number(r, "x")? + number(r, "width")? / 2.0,
                    number(r, "y")?
                        + if name == "notification" {
                            number(r, "height")? - 8.0
                        } else {
                            number(r, "height")? / 2.0
                        },
                )
            };
            move_pointer(&mut f, pointer, point.0, point.1)?;
            pointer_event(&mut f, pointer, "p")?;
            let before = state(&f, &qml, &output_rect)?;
            let mut frames = Vec::new();
            for (index, reduced) in [false, true, false].into_iter().enumerate() {
                theme(&appearance, &palette, mode, reduced)?;
                wait("transparency mode", 12, || {
                    Ok((state(&f, &qml, &output_rect)?["reduced"] == reduced).then_some(()))
                })?;
                let current = state(&f, &qml, &output_rect)?;
                ensure(
                    geometry(&current) == geometry(&before),
                    format!("{name} {mode} {reduced} interaction geometry changed"),
                )?;
                ensure(
                    current["focused"] == before["focused"],
                    format!("{name} keyboard focus changed"),
                )?;
                if let (Some(t), Some(id), Some(previous)) =
                    (title(name), &identifier, &native_before)
                {
                    let observed = native(&f, t)?.ok_or("Native window vanished")?;
                    ensure(
                        observed["id"] == *id && observed["rect"] == previous["rect"],
                        format!("{name} native identity/geometry changed"),
                    )?;
                }
                if name == "dock" {
                    ensure(
                        current["dock"] == before["dock"],
                        "Dock geometry or mask changed",
                    )?;
                }
                let path = f.out.join(format!("{name}-{mode}-{index}.png"));
                let evidence = wait("painted transparency", 12, || {
                    paint(&f, &path, name, mode, &current, &palette)
                })?;
                frames.push(
                    json!({"reduced":reduced,"state":current,"capture":s(&path),"paint":evidence}),
                );
                fs::write(
                    f.out.join("last-frames.json"),
                    serde_json::to_string_pretty(&frames)?,
                )?;
                if reduced {
                    ensure(
                        current["material"]["a"] == 1,
                        "Global material fallback remains translucent",
                    )?;
                }
            }
            pointer_event(&mut f, pointer, "r")?;
            let after = state(&f, &qml, &output_rect)?;
            check_pointer_action(name, &after)?;
            check_keyboard_action(&mut f, &qml, &output_rect, name)?;
            results.push(json!({"surface":name,"mode":mode,"nativeId":identifier,"frames":frames,"after":after,"afterKeyboard":state(&f,&qml,&output_rect)?}));
        }
    }
    let log = fs::read_to_string(f.out.join("shell.log"))?;
    for bad in [
        "ReferenceError",
        "TypeError",
        "Binding loop",
        "Failed to load",
        "PyGIDeprecationWarning",
        "WARN scene:",
    ] {
        ensure(
            !log.contains(bad),
            format!("Shell log contains {bad}: {log}"),
        )?;
    }
    let mut hashes = BTreeMap::new();
    for name in [
        "CalendarCard.qml",
        "NotificationCard.qml",
        "Theme.qml",
        "Dock.qml",
    ] {
        hashes.insert(
            format!("shell/{name}"),
            sha_file(&assets.join("shell").join(name))?,
        );
    }
    let result = json!({"passed":true,"effects":opt.effects,"checks":results,"sourceHashes":hashes,"liveSocketUnchanged":env::var("SWAYSOCK").ok()==original_socket});
    fs::write(
        f.out.join("result.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    println!(
        "PASS live transparency {}",
        result["checks"].as_array().unwrap().len()
    );
    Ok(())
}
