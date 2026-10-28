//! Exercise per-device preferences through genuine disposable virtual devices.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn save_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn number(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| fail(format!("Missing {key}: {value}")))
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() > end {
            return Err(fail(format!("Timed out waiting for {label}: {last}")));
        }
        pause(100);
    }
}
fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && !name.ends_with(suffix)
            })
        }))
}
fn find_node<'a>(tree: &'a Value, predicate: &impl Fn(&Value) -> bool) -> Option<&'a Value> {
    if predicate(tree) {
        return Some(tree);
    }
    for key in ["nodes", "floating_nodes"] {
        for child in tree[key].as_array().into_iter().flatten() {
            if let Some(found) = find_node(child, predicate) {
                return Some(found);
            }
        }
    }
    None
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let to = target.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &to)?;
        } else if path.is_file() {
            fs::copy(&path, &to)?;
        } else {
            return Err(fail("Unsupported asset"));
        }
    }
    Ok(())
}
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    require(
        output.status.success(),
        "Packaged launcher environment unavailable",
    )?;
    let mut vars = BTreeMap::new();
    for part in output.stdout.split(|byte| *byte == 0) {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    Ok(vars)
}
fn resolve_command(program: &Path, path: &str) -> Result<PathBuf> {
    if program.components().count() > 1 {
        return Ok(program.to_path_buf());
    }
    for directory in env::split_paths(&OsString::from(path)) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("Command unavailable: {}", program.display())))
}
struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    pointer_tool: PathBuf,
    output: PathBuf,
    assets: Option<PathBuf>,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut pointer_tool = None;
    let mut output = None;
    let mut assets = None;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
            Some("--pointer-tool") => {
                pointer_tool = Some(PathBuf::from(args.next().ok_or("Missing pointer tool")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--assets") => assets = Some(PathBuf::from(args.next().ok_or("Missing assets")?)),
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown input-settings fixture option")),
        }
    }
    let output = output.ok_or("--output required")?;
    let output = if output.is_absolute() {
        output
    } else {
        env::current_dir()?.join(output)
    };
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        pointer_tool: pointer_tool.ok_or("--pointer-tool required")?,
        output,
        assets,
        inside,
    })
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = Path::new("/tmp").join(format!("linp-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        Ok(Self(path))
    }
}
impl Drop for PrivateRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn private_session() -> Result<()> {
    let root = PrivateRoot::new()?;
    let config = root.0.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            root.0.display()
        ),
    )?;
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    vars.insert("HOME".into(), root.0.display().to_string());
    vars.insert("XDG_RUNTIME_DIR".into(), root.0.display().to_string());
    for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
        vars.insert(
            format!("XDG_{key}_HOME"),
            root.0.join(key.to_lowercase()).display().to_string(),
        );
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
    let status = Command::new("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .args(env::args_os().skip(1))
        .arg("--inside")
        .env_clear()
        .envs(&vars)
        .status()?;
    require(
        status.success(),
        format!("Private input fixture exited {status}"),
    )
}
struct Managed(Child);
impl Managed {
    fn launch(
        program: &Path,
        args: &[&str],
        vars: &BTreeMap<String, String>,
        log: &Path,
        pipe: bool,
    ) -> Result<Self> {
        let file = File::create(log)?;
        Ok(Self(
            Command::new(program)
                .args(args)
                .env_clear()
                .envs(vars)
                .stdin(if pipe { Stdio::piped() } else { Stdio::null() })
                .stdout(Stdio::from(file.try_clone()?))
                .stderr(Stdio::from(file))
                .process_group(0)
                .spawn()?,
        ))
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
            let end = Instant::now() + Duration::from_secs(4);
            while self.0.try_wait().ok().flatten().is_none() {
                if Instant::now() > end {
                    unsafe {
                        libc::killpg(self.0.id() as i32, libc::SIGKILL);
                    }
                    let _ = self.0.wait();
                    break;
                }
                pause(30);
            }
        }
    }
}
struct Driver {
    opt: Options,
    vars: BTreeMap<String, String>,
    runtime: PathBuf,
    out: PathBuf,
    qml: PathBuf,
    processes: Vec<Managed>,
    pointer: Option<Managed>,
}
impl Driver {
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .output()?;
        require(
            output.status.success(),
            format!(
                "{program} {:?}: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut words = vec![
            "-p",
            self.qml.to_str().ok_or("Non-UTF8 QML path")?,
            "ipc",
            "call",
            "filesAudit",
            method,
        ];
        words.extend_from_slice(args);
        self.call("quickshell", &words)
    }
    fn ipc_json(&self, method: &str, args: &[&str]) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc(method, args)?)?)
    }
    fn state(&self) -> Result<Value> {
        self.ipc_json("state", &[])
    }
    fn wait_for(&self, label: &str, mut probe: impl FnMut() -> Result<bool>) -> Result<()> {
        wait(label, || probe().map(|value| value.then_some(())))?;
        Ok(())
    }
    fn launch(&mut self, program: &Path, args: &[&str], name: &str) -> Result<()> {
        self.processes.push(Managed::launch(
            program,
            args,
            &self.vars,
            &self.out.join(name),
            false,
        )?);
        Ok(())
    }
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn window(&self) -> Result<Value> {
        find_node(&self.tree()?, &|node| {
            node["app_id"] == "org.quickshell" && node["name"] == "Input fixture"
        })
        .cloned()
        .ok_or_else(|| fail("Input window unavailable"))
    }
    fn point(&self, name: &str, rx: f64, ry: f64) -> Result<(f64, f64)> {
        let geometry = self.ipc_json("geometry", &[name])?;
        require(!geometry.is_null(), format!("Input control absent: {name}"))?;
        let window = self.window()?;
        let rect = &window["rect"];
        Ok((
            number(rect, "x")? + number(&geometry, "x")? + number(&geometry, "width")? * rx,
            number(rect, "y")?
                + number(&geometry, "y")?
                + number(&geometry, "height")?.min(25.) * ry,
        ))
    }
    fn command(&mut self, value: &str) -> Result<()> {
        let pointer = self.pointer.as_mut().ok_or("Pointer unavailable")?;
        let input = pointer
            .0
            .stdin
            .as_mut()
            .ok_or("Pointer stdin unavailable")?;
        input.write_all(value.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        pause(120);
        Ok(())
    }
    fn move_to(&mut self, x: f64, y: f64) -> Result<()> {
        self.call(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &x.round().to_string(),
                &y.round().to_string(),
            ],
        )?;
        self.command("m 0.5 0.5")
    }
    fn click(&mut self, position: (f64, f64)) -> Result<()> {
        self.move_to(position.0, position.1)?;
        self.command("p")?;
        self.command("r")
    }
    fn inputs(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_inputs"])?,
        )?)
    }
}
fn setup(opt: Options) -> Result<Driver> {
    fs::create_dir(&opt.output)?;
    let mut vars = launcher_environment(&opt.launcher)?;
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", "pixman"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
    ] {
        vars.insert(key.into(), value.into());
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
    let runtime = PathBuf::from(
        vars.get("XDG_RUNTIME_DIR")
            .ok_or("Private runtime missing")?,
    );
    let source_assets = if let Some(path) = &opt.assets {
        fs::canonicalize(path)?
    } else {
        fs::canonicalize(
            vars.get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Packaged assets missing")?,
        )?
    };
    let assets = opt.output.join("assets");
    copy_tree(&source_assets, &assets)?;
    let palette = read_json(&assets.join("appearance/palette.json"))?;
    let appearance = opt.output.join("appearance.json");
    save_json(
        &appearance,
        &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}),
    )?;
    vars.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let package = env::current_exe()?
        .parent()
        .ok_or("Executable parent missing")?
        .parent()
        .ok_or("Executable root missing")?
        .join("share/lumen-input-settings-fixture");
    let fixture = package.join("settings_fixture.nu");
    let output = Command::new("nu")
        .arg(&fixture)
        .arg(&opt.output)
        .arg(vars.get("PATH").ok_or("Packaged PATH missing")?)
        .env_clear()
        .envs(&vars)
        .output()?;
    require(
        output.status.success(),
        format!(
            "Nushell settings fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    let overlay: Value = serde_json::from_slice(&output.stdout)?;
    for key in ["PATH", "LUMEN_NIX_SETTINGS_ACTIVE"] {
        vars.insert(
            key.into(),
            overlay[key]
                .as_str()
                .ok_or("Settings fixture export missing")?
                .into(),
        );
    }
    let qml = opt.output.join("shell.qml");
    fs::copy(package.join("fixture.qml"), &qml)?;
    let out = opt.output.clone();
    Ok(Driver {
        opt,
        vars,
        runtime,
        out,
        qml,
        processes: Vec::new(),
        pointer: None,
    })
}
fn screenshot_moments(path: &Path, rect: [usize; 4]) -> Result<([f64; 3], [f64; 3])> {
    let mut decoder = png::Decoder::new(File::open(path)?);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut bytes)?;
    let channels = match frame.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(fail(format!("Unsupported PNG type: {other:?}"))),
    };
    require(
        rect[0] < rect[2]
            && rect[1] < rect[3]
            && rect[2] <= frame.width as usize
            && rect[3] <= frame.height as usize,
        "Invalid input pane crop",
    )?;
    let count = ((rect[2] - rect[0]) * (rect[3] - rect[1])) as f64;
    let mut sums = [0.; 3];
    let mut squares = [0.; 3];
    for y in rect[1]..rect[3] {
        for x in rect[0]..rect[2] {
            let offset = (y * frame.width as usize + x) * channels;
            for channel in 0..3 {
                let value = bytes[offset + channel] as f64;
                sums[channel] += value;
                squares[channel] += value * value;
            }
        }
    }
    let mut means = [0.; 3];
    let mut deviations = [0.; 3];
    for channel in 0..3 {
        means[channel] = sums[channel] / count;
        deviations[channel] = (squares[channel] / count - means[channel] * means[channel])
            .max(0.)
            .sqrt();
    }
    Ok((means, deviations))
}
impl Driver {
    fn start(&mut self) -> Result<()> {
        let config = self.out.join("sway.conf");
        fs::write(
            &config,
            "output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n",
        )?;
        let sway = resolve_command(
            &self.opt.sway,
            self.vars.get("PATH").ok_or("Packaged PATH missing")?,
        )?;
        self.launch(
            &sway,
            &[
                "--unsupported-gpu",
                "-c",
                config.to_str().ok_or("Non-UTF8 Sway config")?,
            ],
            "sway.log",
        )?;
        let sock = wait("Sway IPC", || {
            first_file(&self.runtime, "sway-ipc", ".lock")
        })?;
        let display = wait("Wayland display", || {
            first_file(&self.runtime, "wayland-", ".lock")
        })?;
        self.vars
            .insert("SWAYSOCK".into(), sock.display().to_string());
        self.vars.insert(
            "WAYLAND_DISPLAY".into(),
            display
                .file_name()
                .ok_or("Display name missing")?
                .to_string_lossy()
                .into(),
        );
        self.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log")?;
        self.pointer = Some(Managed::launch(
            &self.opt.pointer_tool,
            &["pointer", "move", "0", "0"],
            &self.vars,
            &self.out.join("pointer.log"),
            true,
        )?);
        self.command("m 0.5 0.5")?;
        let qml = self.qml.to_str().ok_or("Non-UTF8 QML path")?.to_owned();
        self.launch(Path::new("quickshell"), &["-p", &qml], "shell.log")?;
        self.wait_for("input window", || Ok(self.window().is_ok()))?;
        self.ipc("refresh", &[])?;
        self.wait_for("keyboard row", || {
            let state = self.state()?;
            Ok(state["ready"] == true
                && state["rows"].as_array().is_some_and(|rows| {
                    rows.iter().any(|row| row["identity"]["type"] == "keyboard")
                }))
        })?;
        self.ipc("selectKeyboard", &[])?;
        pause(100);
        Ok(())
    }
    fn check(&mut self) -> Result<()> {
        self.start()?;
        let before = self.inputs()?;
        let keyboard = before
            .as_array()
            .ok_or("Sway inputs not array")?
            .iter()
            .find(|device| device["type"] == "keyboard")
            .ok_or("Virtual keyboard missing")?
            .clone();
        let previous = keyboard["repeat_rate"].clone();
        let slider = self.point("input-slider-repeat_rate", 0.8, 0.5)?;
        self.click(slider)?;
        self.wait_for("saved keyboard override", || {
            Ok(self.state()?["ready"] == true && self.state()?["selected"]["saved"] == true)
        })?;
        let after = self.inputs()?;
        let changed = after
            .as_array()
            .ok_or("Sway inputs not array")?
            .iter()
            .find(|device| device["identifier"] == keyboard["identifier"])
            .ok_or("Keyboard identity lost")?;
        let state = self.state()?;
        require(
            changed["repeat_rate"] != previous && state["error"].as_str().unwrap_or("").is_empty(),
            format!("Keyboard override failed: {state}"),
        )?;
        save_json(&self.out.join("devices-before.json"), &before)?;
        save_json(&self.out.join("devices-after.json"), &after)?;
        let before_other: Vec<_> = before
            .as_array()
            .unwrap()
            .iter()
            .filter(|device| device["type"] != "keyboard")
            .collect();
        let after_other: Vec<_> = after
            .as_array()
            .unwrap()
            .iter()
            .filter(|device| device["type"] != "keyboard")
            .collect();
        require(before_other == after_other, "Nonkeyboard device changed")?;
        let settings = read_json(&self.out.join("settings.json"))?;
        let selected_key = state["selected"]["key"]
            .as_str()
            .ok_or("Selected device key missing")?;
        let record = &settings["inputs"][selected_key];
        require(
            record["values"]["repeat_rate"] == changed["repeat_rate"]
                && record["baseline"]["repeat_rate"] == previous,
            format!("Settings record changed: {record}"),
        )?;
        let mut checks = vec![
            "Actual slider changes only the uniquely identified disposable keyboard and persists its previous value",
        ];
        let capture = self.out.join("input-keyboard-dark.png");
        self.call("grim", &[capture.to_str().ok_or("Non-UTF8 capture path")?])?;
        let forget = self.point("input-forget-profile", 0.5, 0.5)?;
        self.click(forget)?;
        let cancel = self.point("input-forget-cancel", 0.5, 0.5)?;
        self.click(cancel)?;
        require(
            self.state()?["selected"]["saved"] == true,
            "Cancel changed saved profile",
        )?;
        checks.push("Actual Forget confirmation can be cancelled without changing the profile or current device");
        let restore = self.point("input-restore-repeat_rate", 0.5, 0.5)?;
        self.click(restore)?;
        self.wait_for("restored keyboard repeat", || {
            Ok(self.state()?["ready"] == true && self.state()?["selected"]["saved"] == false)
        })?;
        let restored = self.inputs()?;
        let device = restored
            .as_array()
            .ok_or("Sway inputs not array")?
            .iter()
            .find(|device| device["identifier"] == keyboard["identifier"])
            .ok_or("Restored keyboard missing")?;
        require(
            device["repeat_rate"] == previous,
            "Restore did not reverse repeat rate",
        )?;
        checks.push(
            "Actual Restore button reverses the device setting and removes its saved override",
        );
        let window = self.window()?;
        let rect = &window["rect"];
        let crop = [
            (number(rect, "x")? + 10.) as usize,
            (number(rect, "y")? + 20.) as usize,
            (number(rect, "x")? + number(rect, "width")? - 10.) as usize,
            (number(rect, "y")? + number(rect, "height")? - 10.) as usize,
        ];
        let (means, deviations) = screenshot_moments(&capture, crop)?;
        require(
            means.iter().copied().fold(0., f64::max) > 10.
                && deviations.iter().copied().fold(0., f64::max) > 2.,
            "Blank input pane",
        )?;
        let log = fs::read_to_string(self.out.join("shell.log"))?;
        for marker in [
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Failed to load configuration",
            "Cannot assign",
        ] {
            require(!log.contains(marker), format!("QML error {marker}: {log}"))?;
        }
        save_json(
            &self.out.join("result.json"),
            &json!({"passed":true,"checks":checks,"final":self.state()?,"scope":"Genuine isolated compositor and virtual keyboard/pointer; disposable settings adapter; no host device changes"}),
        )?;
        println!(
            "PASS native per-device input preference and reversal on disposable virtual keyboard"
        );
        Ok(())
    }
}
fn run() -> Result<()> {
    let opt = options()?;
    if !opt.inside {
        return private_session();
    }
    setup(opt)?.check()
}
fn main() {
    if let Err(error) = run() {
        eprintln!("input-settings fixture: {error}");
        std::process::exit(1);
    }
}
