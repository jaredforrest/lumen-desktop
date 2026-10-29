//! Native mixed-output scale geometry, paint, and positive input acceptance.
use lumen_display_visual_fixtures::{
    Raster, Result, Session, copy_tree, ensure, fail, integer, number, round, s, sha_assets,
    sha_bytes, wait,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    output: PathBuf,
    pointer: PathBuf,
    effects: bool,
    source: PathBuf,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut launcher = None;
        let mut sway = None;
        let mut output = None;
        let mut pointer = PathBuf::from("wlrctl");
        let mut effects = false;
        let mut source = env::current_dir()?.join("res/lumen-desktop");
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--pointer-tool") => {
                    pointer = PathBuf::from(args.next().ok_or("Missing pointer")?)
                }
                Some("--source") => source = PathBuf::from(args.next().ok_or("Missing source")?),
                Some("--effects") => effects = true,
                _ => return Err(fail("Unknown mixed-output option")),
            }
        }
        let output = output.ok_or("--output required")?;
        Ok(Self {
            launcher: launcher.ok_or("--launcher required")?,
            sway: sway.ok_or("--sway required")?,
            output: if output.is_absolute() {
                output
            } else {
                env::current_dir()?.join(output)
            },
            pointer,
            effects,
            source,
        })
    }
}
fn state(f: &Session, qml: &Path) -> Result<Value> {
    serde_json::from_str(&f.call(
        "quickshell",
        &["-p", &s(qml), "ipc", "call", "scale", "state"],
    )?)
    .map_err(Into::into)
}
fn open(f: &Session, qml: &Path, kind: &str) -> Result<()> {
    f.call(
        "quickshell",
        &["-p", &s(qml), "ipc", "call", "scale", "openWindow", kind],
    )?;
    Ok(())
}
fn output_map(f: &Session) -> Result<BTreeMap<String, Value>> {
    Ok(f.outputs()?
        .into_iter()
        .filter_map(|v| v["name"].as_str().map(|n| (n.to_owned(), v.clone())))
        .collect())
}
fn click(f: &mut Session, pointer: usize, x: f64, y: f64) -> Result<()> {
    f.ipc(&format!("seat seat0 cursor set {} {}", round(x), round(y)))?;
    f.write(pointer, "m 0 0\np\nr\n")
}
fn logical(state: &Value) -> Value {
    json!({"icon":state["icon"],"label":state["label"],"action":state["action"],"help":state["help"]})
}
fn crop_from(
    raster: &Raster,
    box_: &Value,
    display_rect: &Value,
    scale: f64,
    local: [f64; 4],
) -> Result<Raster> {
    let x = number(box_, "x")? - number(display_rect, "x")? + local[0];
    let y = number(box_, "y")? - number(display_rect, "y")? + local[1];
    Ok(raster.crop(
        round(x * scale),
        round(y * scale),
        round((x + local[2]) * scale),
        round((y + local[3]) * scale),
    ))
}
fn paint(
    f: &Session,
    path: &Path,
    kind: &str,
    output: &str,
    scale: f64,
    display: &Value,
    box_: &Value,
    before: &Value,
) -> Result<Option<Value>> {
    f.capture(path, &["-o", output, "-s", &scale.to_string()])?;
    let image = Raster::read(path)?;
    let rect = &display["rect"];
    ensure(
        (image.width as f64 - number(rect, "width")? * scale).abs() <= 1.0
            && (image.height as f64 - number(rect, "height")? * scale).abs() <= 1.0,
        format!(
            "Capture physical size does not match output: {}x{} {rect} scale {scale}",
            image.width, image.height
        ),
    )?;
    let mode = &display["current_mode"];
    ensure(
        (image.width as f64 - number(mode, "width")?).abs() <= 1.0
            && (image.height as f64 - number(mode, "height")?).abs() <= 1.0,
        "Capture physical size does not match output mode",
    )?;
    let body = if kind == "native" {
        crop_from(&image, box_, rect, scale, [300.0, 250.0, 20.0, 20.0])?
    } else {
        crop_from(&image, box_, rect, scale, [500.0, 20.0, 20.0, 20.0])?
    };
    let painted = body
        .each()
        .filter(|p| {
            p.iter().max().unwrap() - p.iter().min().unwrap() < 12 && *p.iter().min().unwrap() > 200
        })
        .count();
    if body.width * body.height == 0
        || (painted as f64) < ((body.width * body.height) as f64) * 0.95
    {
        return Ok(None);
    }
    let mut regions = serde_json::Map::new();
    if kind == "native" {
        let mut controls = Vec::new();
        for (offset, expected) in [
            (20, [255, 95, 87]),
            (40, [254, 188, 46]),
            (60, [40, 200, 64]),
        ] {
            let x = round((number(box_, "x")? - number(rect, "x")? + offset as f64) * scale);
            let y = round((number(box_, "y")? - number(rect, "y")? - 16.0) * scale);
            let color = image.pixel(x, y);
            if (0..3).any(|c| (color[c] as i64 - expected[c]).abs() > 1) {
                return Ok(None);
            }
            controls.push(json!({"pixel":[x,y],"rgb":color}));
        }
        regions.insert("nativeControls".into(), Value::Array(controls));
        for name in ["label", "icon"] {
            let target = &before[name];
            let crop = crop_from(
                &image,
                box_,
                rect,
                scale,
                [
                    number(target, "x")?,
                    number(target, "y")?,
                    number(target, "width")?,
                    number(target, "height")?,
                ],
            )?;
            let count = crop
                .each()
                .filter(|p| *p.iter().min().unwrap() < 180)
                .count();
            if (count as f64) < 10.0 * scale * scale {
                return Ok(None);
            }
            regions.insert(
                name.into(),
                json!({"physical":[crop.width,crop.height],"paintedPixels":count}),
            );
        }
    }
    let mut samples = serde_json::Map::new();
    for (name, x, y) in [
        ("topLeft", 0.0, 0.0),
        ("innerCorner", 10.0, 10.0),
        ("leftInterior", 4.0, 180.0),
        ("rightInterior", number(box_, "width")? - 5.0, 180.0),
    ] {
        let px = round((number(box_, "x")? - number(rect, "x")? + x) * scale);
        let py = round((number(box_, "y")? - number(rect, "y")? + y) * scale);
        ensure(
            px >= 0 && py >= 0 && px < image.width && py < image.height,
            "Edge sample outside capture",
        )?;
        samples.insert(
            name.into(),
            json!({"pixel":[px,py],"rgb":image.pixel(px,py)}),
        );
    }
    Ok(Some(
        json!({"physical":[image.width,image.height],"paint":regions,"edgeSamples":samples}),
    ))
}
fn run(opt: Options) -> Result<()> {
    let mut f = Session::new(&opt.output, &opt.launcher, &opt.sway, "lumen-mixed-scale-")?;
    f.set_many(&[
        ("WLR_BACKENDS", "headless"),
        ("WLR_HEADLESS_OUTPUTS", "2"),
        ("WLR_RENDERER", if opt.effects { "gles2" } else { "pixman" }),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if opt.effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "1"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
    ]);
    let repo = fs::canonicalize(&opt.source)?
        .parent()
        .ok_or("Source has no repo root")?
        .parent()
        .ok_or("Source has no repo root")?
        .to_owned();
    let assets = f.out.join("assets");
    copy_tree(&opt.source, &assets)?;
    let appearance = f.out.join("appearance.json");
    fs::write(
        &appearance,
        serde_json::to_vec(
            &json!({"font":"Interlude Variable","palette":{"background":"#f2f2f4","surface":"#ffffff","field":"#e5e5e8","text":"#202023","muted":"#626267","border":"#c8c8ce","hover":"#dedee3","accent":"#007aff","selectedText":"#000000"},"preferences":{"mode":"light","reduceMotion":true,"reduceTransparency":true}}),
        )?,
    )?;
    f.set("LUMEN_APPEARANCE_STATE", s(&appearance));
    let qml = f.out.join("shell.qml");
    fs::write(&qml, include_str!("../../mixed.qml"))?;
    let mut config = String::from(
        "font sans 10\noutput * resolution 1600x1200\noutput HEADLESS-1 position 0 0 scale 1\noutput HEADLESS-2 position 1600 0 scale 1.25\noutput * bg #29445b solid_color\ndefault_border pixel 0\ndefault_floating_border pixel 0\nfor_window [all] floating enable\nfor_window [title=\"^Scale specimen$\"] border normal 1\n",
    );
    if opt.effects {
        config.push_str("corner_radius 12\nshadows enable\nanimation_duration_ms 0\n");
    }
    let config_path = f.out.join("sway.conf");
    fs::write(&config_path, config)?;
    let result = (|| -> Result<Value> {
        f.start_bus()?;
        f.launch(
            &s(&opt.sway),
            &["-d", "--unsupported-gpu", "-c", &s(&config_path)],
            "compositor.log",
            false,
        )?;
        f.wait_sway(2)?;
        f.ipc("lumen_window margins disable")?;
        f.launch("quickshell", &["-p", &s(&qml)], "shell.log", false)?;
        wait("scale IPC", 15, || {
            Ok((!state(&f, &qml)?.is_null()).then_some(()))
        })?;
        let pointer = f.launch(
            &s(&opt.pointer),
            &["pointer", "move", "0", "0"],
            "pointer.log",
            true,
        )?;
        f.launch("wtype", &["-s", "60000"], "keyboard.log", false)?;
        f.write(pointer, "m 0 0\n")?;
        let plan = [
            ("HEADLESS-1", 1.0),
            ("HEADLESS-2", 1.25),
            ("HEADLESS-2", 1.5),
            ("HEADLESS-2", 2.0),
            ("HEADLESS-1", 1.0),
            ("HEADLESS-1", 1.25),
            ("HEADLESS-1", 1.5),
            ("HEADLESS-2", 2.0),
            ("HEADLESS-2", 1.25),
            ("HEADLESS-1", 1.5),
        ];
        let mut records = Vec::new();
        for (kind, title) in [("native", "Scale specimen"), ("owned", "Desktop Help")] {
            open(&f, &qml, kind)?;
            let initial = wait(title, 15, || {
                Ok(f.window(title)?
                    .filter(|n| kind != "native" || n["deco_rect"]["height"] == 32))
            })?;
            let identity = initial["id"].as_i64().ok_or("Window id missing")?;
            let mut baseline = None;
            for (index, (output, scale)) in plan.iter().enumerate() {
                f.ipc(&format!("output {output} scale {scale}"))?;
                f.ipc(&format!(
                    "[con_id={identity}] move container to output {output}"
                ))?;
                f.ipc(&format!("[con_id={identity}] move position center"))?;
                wait("moved window", 15, || {
                    Ok(f.window(title)?.filter(|n| n["id"] == identity))
                })?;
                f.ipc(&format!("[con_id={identity}] focus"))?;
                let current = f.window(title)?.ok_or("Window vanished")?;
                ensure(
                    current["window_rect"]["width"] == initial["window_rect"]["width"]
                        && current["window_rect"]["height"] == initial["window_rect"]["height"],
                    format!("{kind} {index} window_rect changed"),
                )?;
                let before = state(&f, &qml)?;
                let logical = logical(&before);
                if let Some(base) = &baseline {
                    ensure(
                        *base == logical,
                        format!("{kind} {index} logical layout changed: {base} {logical}"),
                    )?
                } else {
                    baseline = Some(logical.clone())
                }
                let display = output_map(&f)?
                    .remove(*output)
                    .ok_or("Active output missing")?;
                let rect = &display["rect"];
                let box_ = &current["rect"];
                ensure(
                    number(rect, "x")? <= number(box_, "x")?
                        && number(box_, "x")? + number(box_, "width")?
                            <= number(rect, "x")? + number(rect, "width")?
                        && number(rect, "y")? <= number(box_, "y")?
                        && number(box_, "y")? + number(box_, "height")?
                            <= number(rect, "y")? + number(rect, "height")?,
                    format!("Window escaped output: {display} {current}"),
                )?;
                let image = f.out.join(format!("{kind}-{index}-{scale}.png"));
                let painted = wait("painted mixed output", 15, || {
                    paint(&f, &image, kind, output, *scale, &display, box_, &before)
                })?;
                if kind == "native" {
                    let target = &before["action"];
                    click(
                        &mut f,
                        pointer,
                        number(box_, "x")? + number(target, "x")? + number(target, "width")? / 2.0,
                        number(box_, "y")? + number(target, "y")? + number(target, "height")? / 2.0,
                    )?;
                    wait("native action", 15, || {
                        Ok((state(&f, &qml)?["actions"].as_i64()
                            == Some(before["actions"].as_i64().unwrap_or(0) + 1))
                        .then_some(()))
                    })?;
                    click(
                        &mut f,
                        pointer,
                        number(box_, "x")? + 60.0,
                        number(box_, "y")? - 16.0,
                    )?;
                    wait("native fullscreen", 15, || {
                        Ok(f.window(title)?.filter(|n| n["fullscreen_mode"] != 0))
                    })?;
                    f.ipc(&format!("[con_id={identity}] lumen_window fullscreen"))?;
                    wait("native fullscreen restore", 15, || {
                        Ok(f.window(title)?
                            .filter(|n| n["fullscreen_mode"] == 0 && n["rect"] == current["rect"]))
                    })?;
                } else {
                    let target = &before["search"];
                    ensure(!target.is_null(), "Help search target missing")?;
                    click(
                        &mut f,
                        pointer,
                        number(box_, "x")? + number(target, "x")? + number(target, "width")? / 2.0,
                        number(box_, "y")? + number(target, "y")? + number(target, "height")? / 2.0,
                    )?;
                    wait("owned search focus", 15, || {
                        Ok((state(&f, &qml)?["search"]["focused"] == true
                            && f.window(title)?.is_some_and(|n| n["focused"] == true))
                        .then_some(()))
                    })?;
                    f.call("wtype", &["-k", "End", "-k", "a"])?;
                    wait("owned typed query", 15, || {
                        Ok((state(&f, &qml)?["query"].as_str()
                            == Some(&format!("{}a", before["query"].as_str().unwrap_or(""))))
                        .then_some(()))
                    })?;
                }
                let record = json!({"kind":kind,"index":index,"scale":scale,"output":display,"window":current,"physical":painted["physical"],"logical":logical,"paint":painted,"state":state(&f,&qml)?,"capture":s(&image)});
                fs::write(
                    f.out.join("last-state.json"),
                    serde_json::to_string_pretty(&record)?,
                )?;
                records.push(record);
            }
            if kind == "owned" {
                let box_ = f.window(title)?.ok_or("Owned window missing")?["rect"].clone();
                f.ipc(&format!(
                    "seat seat0 cursor set {} {}",
                    integer(&box_, "x")? + 20,
                    integer(&box_, "y")? + 24
                ))?;
                f.write(pointer, "m 0 0\np\n")?;
                wait("held close button", 15, || {
                    Ok((state(&f, &qml)?["held"] == true).then_some(()))
                })?;
                f.write(pointer, "m 100 0\nm 0 0\nr\n")?;
                wait("close cancellation", 15, || {
                    Ok((state(&f, &qml)?["held"] == false).then_some(()))
                })?;
                ensure(
                    f.window(title)?.is_some(),
                    "Owned window closed after cancelled press",
                )?;
                click(
                    &mut f,
                    pointer,
                    number(&box_, "x")? + 20.0,
                    number(&box_, "y")? + 24.0,
                )?;
                wait("owned close", 15, || {
                    Ok(f.window(title)?.is_none().then_some(()))
                })?;
            }
        }
        let source = f.call("git", &["-C", &s(&repo), "rev-parse", "HEAD"])?;
        Ok(
            json!({"passed":true,"cases":records,"source":source,"launcher":s(&opt.launcher),"compositor":s(&opt.sway),"fixtureSha256":sha_bytes(include_bytes!("mixed-output.rs")),"assetsSha256":sha_assets(&assets)?,"scope":"Qt software content on native mixed-scale compositor outputs; engineering geometry/input invariants"}),
        )
    })();
    fs::write(
        f.out.join("process-status.json"),
        serde_json::to_string_pretty(&f.process_status())?,
    )?;
    if let Ok(value) = &result {
        fs::write(
            f.out.join("result.json"),
            serde_json::to_string_pretty(value)?,
        )?;
    }
    result.map(|_| ())
}
fn main() {
    if let Err(error) = Options::parse().and_then(run) {
        eprintln!("mixed output: {error}");
        std::process::exit(1)
    }
}
