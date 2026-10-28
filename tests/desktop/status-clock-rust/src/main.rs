//! Real virtual pointer and keyboard checks for the packaged StatusStrip.
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
fn save_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn integer(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| fail(format!("Missing {key}: {value}")))
}
fn number(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| fail(format!("Missing {key}: {value}")))
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
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
        if Instant::now() >= end {
            return Err(fail(format!("Timed out waiting for {label}: {last}")));
        }
        pause(100);
    }
}
fn truthy(value: bool) -> Option<()> {
    value.then_some(())
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
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    require(output.status.success(), "Launcher environment unavailable")?;
    let mut vars = BTreeMap::new();
    for part in output.stdout.split(|byte| *byte == 0) {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    Ok(vars)
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
            return Err(fail(format!("Unsupported asset {}", path.display())));
        }
    }
    Ok(())
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
            _ => return Err(fail("Unknown status-clock fixture option")),
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
        let path = Path::new("/tmp").join(format!("lstatus-{}-{nonce}", std::process::id()));
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
fn private_session(opt: &Options) -> Result<()> {
    let private = PrivateRoot::new()?;
    let config = private.0.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            private.0.display()
        ),
    )?;
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    vars.insert("HOME".into(), private.0.display().to_string());
    vars.insert("XDG_RUNTIME_DIR".into(), private.0.display().to_string());
    for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
        vars.insert(
            format!("XDG_{key}_HOME"),
            private.0.join(key.to_lowercase()).display().to_string(),
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
        format!("Private status fixture exited {status}"),
    )?;
    require(!opt.output.as_os_str().is_empty(), "Output missing")
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
        wait(label, || probe().map(truthy))?;
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
            node["app_id"] == "org.quickshell" && node["name"] == "Status fixture"
        })
        .cloned()
        .ok_or_else(|| fail("Status window unavailable"))
    }
    fn point(&self, name: &str, rx: f64, ry: f64) -> Result<(f64, f64)> {
        let geometry = self.ipc_json("geometry", &[name])?;
        require(!geometry.is_null(), format!("Status item absent: {name}"))?;
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
    fn key(&self, words: &[&str]) -> Result<()> {
        self.call("wtype", words)?;
        pause(150);
        Ok(())
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
    let order = opt.output.join("order.json");
    fs::write(&order, "[]")?;
    vars.insert(
        "LUMEN_STATUS_ORDER_PATH".into(),
        order.display().to_string(),
    );
    let qml = opt.output.join("shell.qml");
    let source_qml = env::current_exe()?
        .parent()
        .ok_or("Executable parent missing")?
        .parent()
        .ok_or("Executable root missing")?
        .join("share/lumen-status-clock-fixture/fixture.qml");
    fs::copy(source_qml, &qml)?;
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
        "Invalid status crop",
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
        let qml = self.qml.to_str().ok_or("Non-UTF8 QML path")?.to_owned();
        self.launch(Path::new("quickshell"), &["-p", &qml], "shell.log")?;
        self.wait_for("status fixture", || Ok(self.state()?["ready"] == true))?;
        self.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log")?;
        self.pointer = Some(Managed::launch(
            &self.opt.pointer_tool,
            &["pointer", "move", "0", "0"],
            &self.vars,
            &self.out.join("pointer.log"),
            true,
        )?);
        pause(200);
        Ok(())
    }
    fn check(&mut self) -> Result<()> {
        self.start()?;
        let mut checks = Vec::new();
        let sound = self.point("status-item-sound", 0.5, 0.5)?;
        self.click(sound)?;
        self.wait_for("sound activation", || Ok(self.state()?["activations"] == 1))?;
        let first = self.point("status-item-battery", 0.5, 0.5)?;
        let last = self.point("status-item-search", 0.5, 0.5)?;
        self.move_to(last.0, last.1)?;
        self.key(&["-M", "logo"])?;
        self.command("p")?;
        for _ in 0..8 {
            self.command(&format!("m {} 0", (first.0 - last.0) / 8.))?;
        }
        self.command("r")?;
        self.key(&["-m", "logo"])?;
        let pointer_order = json!(["search", "battery", "sound", "focus", "controls"]);
        self.wait_for("pointer reorder", || {
            Ok(self.state()?["order"] == pointer_order)
        })?;
        require(
            self.state()?["activations"] == 1,
            "Pointer reorder activated item",
        )?;
        checks.push("Modifier pointer drag reorders without activating a menu");
        self.key(&[
            "-M", "ctrl", "-M", "shift", "-k", "Right", "-m", "shift", "-m", "ctrl",
        ])?;
        let keyboard_order = json!(["battery", "search", "sound", "focus", "controls"]);
        self.wait_for("keyboard reorder", || {
            Ok(self.state()?["order"] == keyboard_order)
        })?;
        checks.push("Real keyboard reorder preserves focused item identity");
        let before = self.state()?["order"].clone();
        let search = self.point("status-item-search", 0.5, 0.5)?;
        self.move_to(search.0, search.1)?;
        self.key(&["-M", "logo"])?;
        self.command("p")?;
        self.command("m 50 0")?;
        self.key(&["-k", "Escape"])?;
        self.command("m 10 0")?;
        self.command("r")?;
        self.key(&["-m", "logo"])?;
        require(
            self.state()?["order"] == before,
            "Escape did not cancel drag",
        )?;
        checks.push("Escape cancels modifier drag even after further held motion");
        let before = self.state()?["order"].clone();
        let search = self.point("status-item-search", 0.5, 0.5)?;
        self.move_to(search.0, search.1)?;
        self.key(&["-M", "logo"])?;
        self.command("p")?;
        self.command("m 0 60")?;
        self.command("r")?;
        self.key(&["-m", "logo"])?;
        let state = self.state()?;
        require(
            state["order"] == before && state["activations"] == 1,
            format!("Release outside changed state: {state}"),
        )?;
        checks.push("Release outside strip cancels without activation or reorder");
        let saved = self.state()?["order"].clone();
        require(
            read_json(&self.out.join("order.json"))? == saved,
            "Order persistence changed presentation order",
        )?;
        checks.push("Presentation order write survives JSON persistence round trip");
        self.ipc("unavailable", &[])?;
        let state = self.state()?;
        require(
            state["order"] == saved && state["visible"] == json!(["search", "sound", "controls"]),
            format!("Unavailable state: {state}"),
        )?;
        self.ipc("narrow", &[])?;
        let state = self.state()?;
        require(
            state["visible"] == json!(["sound", "controls"])
                && state["overflow"] == json!(["search"]),
            format!("Narrow state: {state}"),
        )?;
        self.ipc("restore", &[])?;
        let state = self.state()?;
        require(
            state["order"] == saved
                && state["visible"]
                    .as_array()
                    .is_some_and(|items| items.len() == 5),
            format!("Restored state: {state}"),
        )?;
        checks.push(
            "Unavailable items retain saved slots and narrow overflow restores complete order",
        );
        let capture = self.out.join("status-strip.png");
        self.call("grim", &[capture.to_str().ok_or("Non-UTF8 capture path")?])?;
        let rect = self.window()?["rect"].clone();
        let crop = [
            (integer(&rect, "x")? + 20) as usize,
            (integer(&rect, "y")? + 40) as usize,
            (integer(&rect, "x")? + 210) as usize,
            (integer(&rect, "y")? + 64) as usize,
        ];
        let (mean, stdev) = screenshot_moments(&capture, crop)?;
        require(
            mean.iter().copied().fold(0., f64::max) > 10.
                && stdev.iter().copied().fold(0., f64::max) > 2.,
            "Blank status interior",
        )?;
        self.call("swaymsg", &["output", "HEADLESS-1", "scale", "1.5"])?;
        pause(300);
        let state = self.state()?;
        require(
            state["order"] == saved
                && state["visible"]
                    .as_array()
                    .is_some_and(|items| items.len() == 5),
            format!("Fractional scale changed state: {state}"),
        )?;
        let controls = self.point("status-item-controls", 0.5, 0.5)?;
        self.click(controls)?;
        self.wait_for("scaled controls activation", || {
            Ok(self.state()?["activations"] == 2)
        })?;
        checks.push("Fractional output scale preserves saved order and usable click target");
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
        let result = json!({"passed":true,"checks":checks,"final":self.state()?,"scope":"Genuine virtual modifier/pointer/keyboard in disposable compositor; no live user input"});
        save_json(&self.out.join("result.json"), &result)?;
        println!("PASS native status strip modifier pointer and keyboard reorder");
        Ok(())
    }
}
fn run() -> Result<()> {
    let opt = options()?;
    if !opt.inside {
        return private_session(&opt);
    }
    setup(opt)?.check()
}
fn main() {
    if let Err(error) = run() {
        eprintln!("status-clock fixture: {error}");
        std::process::exit(1);
    }
}
