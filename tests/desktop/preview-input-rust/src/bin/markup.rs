//! Actual Quick Look markup gestures, native chooser and immutable image export.
use lumen_preview_input_fixtures::{
    Raster, Result, Session, copy_tree, ensure, fail, keyboard::PhysicalKeyboard, pause, round, s,
    sha_bytes, sha_file, wait,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

struct Opt {
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    keyboard: PathBuf,
    output: PathBuf,
    effects: bool,
}
impl Opt {
    fn parse() -> Result<Self> {
        let (mut launcher, mut sway, mut pointer, mut keyboard, mut output, mut effects) =
            (None, None, None, None, None, false);
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            let mut next = || -> Result<PathBuf> {
                Ok(PathBuf::from(args.next().ok_or("missing argument value")?))
            };
            match arg.to_str() {
                Some("--launcher") => launcher = Some(next()?),
                Some("--sway") => sway = Some(next()?),
                Some("--pointer-tool") => pointer = Some(next()?),
                Some("--keyboard") => keyboard = Some(next()?),
                Some("--output") => output = Some(next()?),
                Some("--effects") => effects = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown markup argument: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        Ok(Self {
            launcher: launcher.ok_or("launcher")?,
            sway: sway.ok_or("sway")?,
            pointer: pointer.ok_or("pointer")?,
            keyboard: keyboard.ok_or("keyboard")?,
            output: output.ok_or("output")?,
            effects,
        })
    }
}
fn node(tree: &Value, pid: u32, title: &str) -> Option<Value> {
    if tree["pid"].as_u64() == Some(pid as u64)
        && tree["name"] == title
        && tree["app_id"] == "org.quickshell"
    {
        return Some(tree.clone());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in tree[key].as_array().into_iter().flatten() {
            if let Some(n) = node(child, pid, title) {
                return Some(n);
            }
        }
    }
    None
}
fn patch(path: &Path, marker: &str, addition: &str) -> Result<(String, String)> {
    let original = fs::read_to_string(path)?;
    ensure(
        original.matches(marker).count() == 1,
        format!("Unique markup instrumentation marker: {marker}"),
    )?;
    let modified = original.replacen(marker, addition, 1);
    ensure(
        modified.replacen(addition, marker, 1) == original,
        "Markup instrumentation roundtrip",
    )?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    fs::write(path, &modified)?;
    Ok((
        sha_bytes(original.as_bytes()),
        sha_bytes(modified.as_bytes()),
    ))
}
fn exif(orientation: u16) -> Vec<u8> {
    let mut v = b"Exif\0\0II".to_vec();
    v.extend(42_u16.to_le_bytes());
    v.extend(8_u32.to_le_bytes());
    v.extend(1_u16.to_le_bytes());
    v.extend(0x0112_u16.to_le_bytes());
    v.extend(3_u16.to_le_bytes());
    v.extend(1_u32.to_le_bytes());
    v.extend(orientation.to_le_bytes());
    v.extend(0_u16.to_le_bytes());
    v.extend(0_u32.to_le_bytes());
    v
}
fn orient_jpeg(path: &Path, orientation: u16) -> Result<()> {
    let bytes = fs::read(path)?;
    ensure(bytes.starts_with(&[0xff, 0xd8]), "JPEG signature")?;
    let payload = exif(orientation);
    let mut result = vec![0xff, 0xd8, 0xff, 0xe1];
    result.extend(((payload.len() + 2) as u16).to_be_bytes());
    result.extend(payload);
    result.extend(&bytes[2..]);
    fs::write(path, result)?;
    Ok(())
}
fn orient_webp(path: &Path, orientation: u16) -> Result<()> {
    let bytes = fs::read(path)?;
    ensure(
        bytes.len() > 20 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "WebP signature",
    )?;
    let mut result = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
    result.extend(10_u32.to_le_bytes());
    result.extend([0x08, 0, 0, 0, 79, 0, 0, 39, 0, 0]);
    result.extend(&bytes[12..]);
    result.extend(b"EXIF");
    let payload = exif(orientation);
    result.extend((payload.len() as u32).to_le_bytes());
    result.extend(payload);
    if result.len() % 2 != 0 {
        result.push(0)
    }
    let len = (result.len() - 8) as u32;
    result[4..8].copy_from_slice(&len.to_le_bytes());
    fs::write(path, result)?;
    Ok(())
}
fn oriented(t: &Test, extension: &str, orientation: u16) -> Result<PathBuf> {
    let root = t.f.root.path();
    let raw = root.join("quadrants.png");
    let mut pixels = vec![0; 80 * 40 * 3];
    for y in 0..40 {
        for x in 0..80 {
            let rgb = match (x < 40, y < 20) {
                (true, true) => [255, 0, 0],
                (false, true) => [0, 255, 0],
                (true, false) => [0, 0, 255],
                (false, false) => [255, 255, 0],
            };
            pixels[(y * 80 + x) * 3..(y * 80 + x) * 3 + 3].copy_from_slice(&rgb)
        }
    }
    Raster {
        width: 80,
        height: 40,
        pixels,
    }
    .save(&raw)?;
    let target = root.join(format!("oriented.{extension}"));
    let format = if extension == "jpg" { "JPEG" } else { "WEBP" };
    t.f.call(
        "magick",
        &[
            &s(&raw),
            "-define",
            "webp:lossless=true",
            "-quality",
            "95",
            &format!("{format}:{}", s(&target)),
        ],
    )?;
    if extension == "jpg" {
        orient_jpeg(&target, orientation)?
    } else {
        orient_webp(&target, orientation)?
    };
    Ok(target)
}
fn close_colour(actual: [u8; 3], expected: [u8; 3]) -> bool {
    actual
        .into_iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) < 24)
}
struct Test {
    f: Session,
    o: Opt,
    qml: PathBuf,
    shell: u32,
    pointer: usize,
    keyboard: PhysicalKeyboard,
    source: PathBuf,
    original: Vec<u8>,
    checks: Vec<&'static str>,
}
impl Test {
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut argv = vec![
            "-p",
            self.qml.to_str().ok_or("QML path")?,
            "ipc",
            "call",
            "fixture",
            method,
        ];
        argv.extend(args);
        self.f.call("quickshell", &argv)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("status", &[])?)?)
    }
    fn button(&self, name: &str) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("button", &[name])?)?)
    }
    fn move_to(&mut self, name: &str, dx: f64, dy: f64) -> Result<Value> {
        let b = wait(&format!("actual {name} control"), 12, || {
            let v = self.button(name)?;
            Ok(v["title"].is_string().then_some(v))
        })?;
        let title = b["title"].as_str().ok_or("control title")?.to_owned();
        let n = wait("exact markup control owner", 12, || {
            Ok(node(&self.f.tree()?, self.shell, &title))
        })?;
        let id = n["id"].as_i64().ok_or("control window id")?;
        self.f.ipc(&format!("[con_id={id}] focus"))?;
        let (n, b) = wait("focused markup owner", 12, || {
            let n = node(&self.f.tree()?, self.shell, &title);
            let b = self.button(name)?;
            Ok(
                n.filter(|v| v["id"] == id && v["focused"] == true && b["active"] == true)
                    .map(|v| (v, b)),
            )
        })?;
        let x = round(
            n["rect"]["x"].as_f64().unwrap_or(0.)
                + n["window_rect"]["x"].as_f64().unwrap_or(0.)
                + b["x"].as_f64().ok_or("control x")?
                + dx,
        );
        let y = round(
            n["rect"]["y"].as_f64().unwrap_or(0.)
                + n["window_rect"]["y"].as_f64().unwrap_or(0.)
                + b["y"].as_f64().ok_or("control y")?
                + dy,
        );
        self.f.ipc(&format!("seat seat0 cursor set {x} {y}"))?;
        self.f.write(self.pointer, "m 0.5 0.5\n")?;
        if name != "markup-drawing-surface" {
            wait("hovered markup control", 12, || {
                Ok(self.button(name)?["hovered"].as_bool().filter(|v| *v))
            })?;
        }
        Ok(b)
    }
    fn click(&mut self, name: &str) -> Result<()> {
        self.move_to(name, 0., 0.)?;
        self.f.write(self.pointer, "p\n")?;
        wait("pressed markup control", 12, || {
            Ok(self.button(name)?["down"].as_bool().filter(|v| *v))
        })?;
        self.f.write(self.pointer, "r\n")?;
        pause(120);
        Ok(())
    }
    fn begin(&mut self) -> Result<()> {
        self.click("•••")?;
        wait("annotation command", 12, || {
            Ok(self.button("Annotate a Copy…")?["title"]
                .is_string()
                .then_some(()))
        })?;
        self.click("Annotate a Copy…")?;
        wait("markup popup", 12, || {
            Ok((self.state()?["markup"] == true).then_some(()))
        })?;
        Ok(())
    }
    fn start(&self, path: &Path) -> Result<()> {
        wait("Quick Look IPC ready", 12, || {
            Ok(self.ipc("startPreview", &[&s(path)]).ok().map(|_| ()))
        })?;
        wait("image preview", 12, || {
            Ok((self.state()?["result"]["path"] == s(path)).then_some(()))
        })?;
        Ok(())
    }
    fn draw_line(&mut self) -> Result<()> {
        self.move_to("markup-drawing-surface", -80., 0.)?;
        self.f.write(self.pointer, "p\n")?;
        wait("drawing press", 12, || {
            Ok((self.state()?["drawing"] == true).then_some(()))
        })?;
        self.f.write(self.pointer, "m 160 0\n")?;
        wait("freehand movement", 12, || {
            Ok((self.state()?["draft"]
                .as_array()
                .is_some_and(|v| v.len() > 2))
            .then_some(()))
        })?;
        self.f.write(self.pointer, "r\n")?;
        wait("freehand stroke", 12, || {
            Ok((self.state()?["strokes"]
                .as_array()
                .is_some_and(|v| v.len() == 1))
            .then_some(()))
        })?;
        Ok(())
    }
    fn chooser(&mut self) -> Result<Value> {
        self.click("Save Copy…")?;
        let n = wait("native Save Annotated Copy chooser", 12, || {
            Ok(node(&self.f.tree()?, self.shell, "Save Annotated Copy"))
        })?;
        self.f.ipc(&format!(
            "[con_id={}] focus",
            n["id"].as_i64().ok_or("chooser id")?
        ))?;
        Ok(n)
    }
    fn chooser_point(&mut self, n: &Value, x: f64, y: f64) -> Result<()> {
        let x = round(
            n["rect"]["x"].as_f64().ok_or("chooser x")?
                + n["window_rect"]["x"].as_f64().unwrap_or(0.)
                + x,
        );
        let y = round(
            n["rect"]["y"].as_f64().ok_or("chooser y")?
                + n["window_rect"]["y"].as_f64().unwrap_or(0.)
                + y,
        );
        self.f.ipc(&format!("seat seat0 cursor set {x} {y}"))?;
        self.f.write(self.pointer, "m 0.5 0.5\n")?;
        Ok(())
    }
    fn save_path(&mut self, path: &Path, hold: bool) -> Result<()> {
        ensure(
            path.parent() == self.source.parent(),
            "Save target outside private source folder",
        )?;
        let chooser = self.chooser()?;
        ensure(chooser["focused"] == true, "Chooser did not focus")?;
        // Click the visible native filename field before sending acknowledged evdev keys.
        // These positions are relative to the fixed 600x505 Qt native chooser in this
        // private 1200x900 display. The window identity is verified above.
        ensure(
            chooser["window_rect"]["width"].as_i64() == Some(598)
                && chooser["window_rect"]["height"].as_i64() == Some(472),
            "Native chooser geometry changed",
        )?;
        self.chooser_point(&chooser, 242., 416.)?;
        self.f.write(self.pointer, "p\nr\n")?;
        self.keyboard
            .key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        self.keyboard.type_text(
            path.file_name()
                .ok_or("filename")?
                .to_str()
                .ok_or("filename UTF-8")?,
        )?;
        self.chooser_point(&chooser, 446., 449.)?;
        self.f.write(self.pointer, "p\n")?;
        pause(80);
        if !hold {
            self.f.write(self.pointer, "r\n")?;
        }
        fs::write(
            self.f.out.join("after-save-state.json"),
            serde_json::to_vec_pretty(&self.state()?)?,
        )?;
        fs::write(
            self.f.out.join("after-save-tree.json"),
            serde_json::to_vec_pretty(&self.f.tree()?)?,
        )?;
        self.f
            .capture(&self.f.out.join("after-save.png"), &["-o", "HEADLESS-1"])?;
        Ok(())
    }
    fn check(&mut self, name: &'static str) {
        self.checks.push(name)
    }
}
fn setup(o: Opt) -> Result<Test> {
    let mut f = Session::new(&o.output, &o.launcher, &o.sway, "lumen-markup-native-")?;
    f.set_many(&[
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "1"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1"),
        ("NO_AT_BRIDGE", "0"),
        ("GTK_A11Y", "atspi"),
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if o.effects { "gles2" } else { "pixman" }),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if o.effects { "1" } else { "0" },
        ),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
    ]);
    let source = f.root.path().join("original.png");
    Raster {
        width: 160,
        height: 100,
        pixels: [255, 255, 255].repeat(160 * 100),
    }
    .save(&source)?;
    let original = fs::read(&source)?;
    let assets = f.out.join("assets");
    let installed = PathBuf::from(
        f.env
            .get("LUMEN_DESKTOP_ASSETS")
            .ok_or("installed assets")?,
    );
    copy_tree(&installed, &assets)?;
    let quick = assets.join("shell/QuickLookWindow.qml");
    let (quick_original, quick_modified) = patch(
        &quick,
        "    id:root\n",
        "    id:root\n    readonly property var fixtureMenu:previewActions\n    readonly property var fixtureMarkup:markup\n",
    )?;
    let markup = assets.join("shell/ImageMarkupDialog.qml");
    let (markup_original, _) = patch(
        &markup,
        "    id:root\n",
        "    id:root\n    readonly property var fixtureImage:image\n",
    )?;
    let _ = patch(
        &markup,
        "function saveTo(value,intent){",
        "function saveTo(value,intent){console.log(\"MARKUP_SELECTED_DESTINATION \"+value);",
    )?;
    let markup_modified = sha_file(&markup)?;
    fs::write(
        f.out.join("instrumentation.json"),
        serde_json::to_vec_pretty(
            &json!({"quickOriginalSha256":quick_original,"quickModifiedSha256":quick_modified,"markupOriginalSha256":markup_original,"markupModifiedSha256":markup_modified,"scope":"Read-only actual Quick Look menu, markup and Image access; no action implementation changed"}),
        )?,
    )?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = f.root.path().join("appearance.json");
    fs::write(
        &appearance,
        serde_json::to_vec(
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}),
        )?,
    )?;
    f.set("LUMEN_APPEARANCE_STATE", s(&appearance));
    let qml = f.out.join("shell.qml");
    fs::write(&qml, include_str!("../../markup.qml"))?;
    f.start_bus()?;
    let config = f.out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1200x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n",
    )?;
    f.launch(
        &s(&o.sway),
        &["--unsupported-gpu", "-c", &s(&config)],
        "sway.log",
        false,
    )?;
    f.wait_sway(1)?;
    let pointer = f.launch(
        &s(&o.pointer),
        &["pointer", "move", "0", "0"],
        "pointer.log",
        true,
    )?;
    let keyboard = PhysicalKeyboard::start(&o.keyboard, &f.env, &f.out.join("keyboard.log"))?;
    let shell = f.launch("quickshell", &["-p", &s(&qml)], "shell.log", false)?;
    let shell = f.child_pid(shell);
    Ok(Test {
        f,
        o,
        qml,
        shell,
        pointer,
        keyboard,
        source,
        original,
        checks: vec![],
    })
}
fn test(t: &mut Test) -> Result<()> {
    t.start(&t.source)?;
    let state = t.state()?;
    ensure(
        state["result"]["type"] == "image",
        "Installed Quick Look did not open image",
    )?;
    ensure(
        state["result"]["resourcePath"] != s(&t.source),
        "Preview did not use immutable resource",
    )?;
    let runtime = PathBuf::from(
        t.f.env
            .get("LUMEN_NOTIFICATION_HELPER")
            .ok_or("notification helper")?,
    )
    .parent()
    .ok_or("helper bin")?
    .parent()
    .ok_or("helper package")?
    .join("share/lumen-desktop/qt-wayland-runtime");
    if runtime.exists() {
        let root = fs::read_to_string(runtime)?.trim().to_owned();
        let maps = fs::read_to_string(format!("/proc/{}/maps", t.shell))?;
        let loaded = maps
            .lines()
            .filter(|v| v.contains("libQt6WaylandClient.so"))
            .collect::<Vec<_>>();
        ensure(
            !root.is_empty()
                && !loaded.is_empty()
                && loaded.into_iter().all(|v| {
                    v.split_whitespace()
                        .last()
                        .is_some_and(|p| p.starts_with(&root))
                }),
            "Qt Wayland runtime outside owned root",
        )?;
    }
    t.begin()?;
    t.click("Add Rectangle")?;
    wait("rectangle stroke", 12, || {
        Ok((t.state()?["strokes"]
            .as_array()
            .is_some_and(|v| v.len() == 1))
        .then_some(()))
    })?;
    t.click("Cancel")?;
    wait("cancelled markup", 12, || {
        Ok((t.state()?["markup"] == false).then_some(()))
    })?;
    ensure(fs::read(&t.source)? == t.original, "Cancel changed source")?;
    t.check("Installed Quick Look menu, rectangle and Cancel preserve source");
    t.begin()?;
    t.click("Add Rectangle")?;
    t.f.capture(&t.f.out.join("editor.png"), &["-o", "HEADLESS-1"])?;
    t.chooser()?;
    t.keyboard.key(&["-k", "Escape"])?;
    wait("chooser cancellation preserves edits", 12, || {
        Ok((t.state()?["markup"] == true
            && t.state()?["strokes"]
                .as_array()
                .is_some_and(|v| v.len() == 1))
        .then_some(()))
    })?;
    t.check("Native chooser Cancel returns to existing annotations");
    let collision = t.source.with_file_name("collision.png");
    fs::write(&collision, b"untouched")?;
    t.save_path(&collision, false)?;
    wait("collision error", 12, || {
        Ok(
            (t.state()?["error"] == "That filename already exists; choose a new name")
                .then_some(()),
        )
    })?;
    ensure(
        fs::read(&collision)? == b"untouched" && t.state()?["markup"] == true,
        "Collision changed file or editor",
    )?;
    t.check("Existing destination stays byte identical after Save refusal");
    let target = t.source.with_file_name("annotated-copy.png");
    t.save_path(&target, false)?;
    wait("confirmed markup copy", 12, || {
        Ok((target.exists() && t.state()?["markup"] == false).then_some(()))
    })?;
    ensure(fs::read(&t.source)? == t.original, "Save changed source")?;
    ensure(
        Raster::read(&target)?.pixel(32, 20) == [255, 59, 48],
        "Annotated pixel mismatch",
    )?;
    fs::copy(&target, t.f.out.join("annotated-copy.png"))?;
    t.check("Actual native Save writes annotated PNG pixel and preserves source bytes");
    t.begin()?;
    t.draw_line()?;
    let replacement = t.source.with_file_name("replacement.png");
    Raster {
        width: 160,
        height: 100,
        pixels: [0, 0, 255].repeat(160 * 100),
    }
    .save(&replacement)?;
    fs::rename(&replacement, &t.source)?;
    let denied = t.source.with_file_name("replaced-denied.png");
    t.save_path(&denied, false)?;
    wait("changed original refusal", 12, || {
        Ok((t.state()?["error"] == "The selected file changed").then_some(()))
    })?;
    ensure(
        !denied.exists()
            && !fs::read_dir(t.f.root.path())?
                .filter_map(|v| v.ok())
                .any(|v| {
                    v.file_name()
                        .to_string_lossy()
                        .starts_with(".lumen-markup-")
                }),
        "Changed source produced output or partial file",
    )?;
    t.check("Actual freehand stroke cannot export after source replacement");
    t.click("Cancel")?;
    let source = t.source.clone();
    t.start(&source)?;
    t.begin()?;
    t.click("Add Rectangle")?;
    let stale = source.with_file_name("stale-copy.png");
    t.save_path(&stale, true)?;
    let other = source.with_file_name("other.png");
    Raster {
        width: 160,
        height: 100,
        pixels: [0, 255, 0].repeat(160 * 100),
    }
    .save(&other)?;
    t.start(&other)?;
    wait("stale chooser retired", 12, || {
        Ok((t.state()?["markup"] == false
            && node(&t.f.tree()?, t.shell, "Save Annotated Copy").is_none())
        .then_some(()))
    })?;
    t.f.write(t.pointer, "r\n")?;
    pause(150);
    ensure(!stale.exists(), "Held Save retargeted fresh preview")?;
    t.begin()?;
    t.draw_line()?;
    t.click("Cancel")?;
    t.check("Held chooser Save release cannot retarget a new preview; fresh controls still work");
    for (extension, orientation) in [("jpg", 6_u16), ("webp", 8_u16)] {
        let source = oriented(t, extension, orientation)?;
        let before = fs::read(&source)?;
        t.start(&source)?;
        t.begin()?;
        let g = wait("oriented editor Image ready", 12, || {
            let v: Value = serde_json::from_str(&t.ipc("imageGeometry", &[])?)?;
            Ok((v["ready"] == true).then_some(v))
        })?;
        ensure(
            g["autoTransform"] == false,
            "Editor unexpectedly auto-transformed EXIF",
        )?;
        let aspect = g["paintedWidth"].as_f64().ok_or("painted width")?
            / g["paintedHeight"].as_f64().ok_or("painted height")?;
        ensure(
            (aspect - 2.).abs() < 0.01,
            "Editor did not paint raw orientation",
        )?;
        let n = node(&t.f.tree()?, t.shell, "Quick Look").ok_or("Quick Look node")?;
        let capture = t.f.out.join(format!("oriented-{extension}-editor.png"));
        t.f.capture(&capture, &["-o", "HEADLESS-1"])?;
        let painted = Raster::read(&capture)?;
        let expected = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
        for ((xf, yf), colour) in [(0.1, 0.1), (0.9, 0.1), (0.1, 0.9), (0.9, 0.9)]
            .into_iter()
            .zip(expected)
        {
            let x = round(
                n["rect"]["x"].as_f64().unwrap_or(0.)
                    + n["window_rect"]["x"].as_f64().unwrap_or(0.)
                    + g["x"].as_f64().ok_or("Image x")?
                    + (g["width"].as_f64().ok_or("Image width")?
                        - g["paintedWidth"].as_f64().ok_or("painted width")?)
                        / 2.
                    + g["paintedWidth"].as_f64().ok_or("painted width")? * xf,
            );
            let y = round(
                n["rect"]["y"].as_f64().unwrap_or(0.)
                    + n["window_rect"]["y"].as_f64().unwrap_or(0.)
                    + g["y"].as_f64().ok_or("Image y")?
                    + (g["height"].as_f64().ok_or("Image height")?
                        - g["paintedHeight"].as_f64().ok_or("painted height")?)
                        / 2.
                    + g["paintedHeight"].as_f64().ok_or("painted height")? * yf,
            );
            ensure(
                close_colour(painted.pixel(x, y), colour),
                format!("{extension} editor pixel mismatch at {x},{y}"),
            )?;
        }
        t.click("Add Rectangle")?;
        let export = source.with_file_name(format!("oriented-{extension}.png"));
        t.save_path(&export, false)?;
        wait("oriented PNG saved", 12, || {
            Ok((export.exists() && t.state()?["markup"] == false).then_some(()))
        })?;
        let encoded = Raster::read(&export)?;
        ensure(
            encoded.width == 80 && encoded.height == 40,
            "Oriented export dimensions",
        )?;
        for ((x, y), colour) in [(8, 4), (72, 4), (8, 36), (72, 36)]
            .into_iter()
            .zip(expected)
        {
            ensure(
                close_colour(encoded.pixel(x, y), colour),
                format!("{extension} exported corner mismatch at {x},{y}"),
            )?;
        }
        ensure(fs::read(&source)? == before, "Oriented original changed")?;
        fs::copy(
            &export,
            t.f.out.join(export.file_name().ok_or("export filename")?),
        )?;
        t.check(if extension == "jpg" {
            "JPEG EXIF orientation preserves raw painted and exported corners and source bytes"
        } else {
            "WebP EXIF orientation preserves raw painted and exported corners and source bytes"
        });
    }
    t.f.capture(&t.f.out.join("final.png"), &["-o", "HEADLESS-1"])?;
    let result = json!({"passed":true,"checks":t.checks,"launcher":s(&t.o.launcher),"effects":t.o.effects,"originalSha256":sha_bytes(&t.original),"copySha256":sha_file(&target)?});
    fs::write(
        t.f.out.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{result}");
    Ok(())
}
fn main() {
    if let Err(e) = (|| -> Result<()> {
        let mut t = setup(Opt::parse()?)?;
        test(&mut t)
    })() {
        eprintln!("Markup fixture: {e}");
        std::process::exit(1)
    }
}
