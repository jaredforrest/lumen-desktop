//! Exercise the real calendar widget with a private calendar mirror and compositor.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
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
    calendar_runtime: PathBuf,
    output: PathBuf,
    assets: Option<PathBuf>,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut pointer_tool = None;
    let mut calendar_runtime = None;
    let mut output = None;
    let mut assets = None;
    let mut effects = false;
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
            Some("--calendar-runtime") => {
                calendar_runtime = Some(PathBuf::from(
                    args.next().ok_or("Missing calendar runtime")?,
                ))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--assets") => assets = Some(PathBuf::from(args.next().ok_or("Missing assets")?)),
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown calendar-widget fixture option")),
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
        calendar_runtime: calendar_runtime.ok_or("--calendar-runtime required")?,
        output,
        assets,
        effects,
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
        format!("Private calendar-widget fixture exited {status}"),
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
            node["app_id"] == "org.quickshell" && node["name"] == "Calendar widget fixture"
        })
        .cloned()
        .ok_or_else(|| fail("Calendar widget window unavailable"))
    }
    fn point(&self, name: &str, rx: f64, ry: f64) -> Result<(f64, f64)> {
        let geometry = self.ipc_json("geometry", &[name])?;
        require(
            !geometry.is_null(),
            format!("Calendar widget control absent: {name}"),
        )?;
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
    fn key(&self, args: &[&str]) -> Result<()> {
        self.call("wtype", args)?;
        pause(150);
        Ok(())
    }
    fn focus_until(&self, name: &str) -> Result<()> {
        for _ in 0..12 {
            self.key(&["-k", "Tab"])?;
            if self.state()?["focus"] == name {
                return Ok(());
            }
        }
        Err(fail(format!(
            "Keyboard focus did not reach {name}: {}",
            self.state()?
        )))
    }
}
fn setup(opt: Options) -> Result<Driver> {
    fs::create_dir(&opt.output)?;
    let mut vars = launcher_environment(&opt.launcher)?;
    for (key, value) in [
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
        .join("share/lumen-calendar-widget-fixture");
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
    let mirror = opt.output.join("calendar");
    let fixture_events = package.join("fixture_calendar_events.nu");
    let events = Command::new("nu")
        .arg(&fixture_events)
        .arg(&mirror)
        .env_clear()
        .envs(&vars)
        .output()?;
    require(
        events.status.success(),
        format!(
            "Calendar mirror failed: {}",
            String::from_utf8_lossy(&events.stderr)
        ),
    )?;
    let real = fs::canonicalize(&opt.calendar_runtime)?;
    require(real.is_file(), "Rust calendar runtime unavailable")?;
    let bin = opt.output.join("bin");
    fs::create_dir_all(&bin)?;
    symlink(env::current_exe()?, bin.join("lumen-calendar"))?;
    vars.insert("LUMEN_CALENDAR_REAL".into(), real.display().to_string());
    vars.insert(
        "LUMEN_CALENDAR_FIXTURE".into(),
        opt.output.display().to_string(),
    );
    vars.insert("LUMEN_CALENDAR_TIMEZONE".into(), "Europe/London".into());
    vars.insert("TZ".into(), "Europe/London".into());
    vars.insert("CALENDAR_DESKTOP_PREFIX".into(), "lumen".into());
    vars.insert("CALENDAR_UID_SUFFIX".into(), "@lumen-desktop.local".into());
    vars.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            bin.display(),
            vars.get("PATH").ok_or("PATH missing")?
        ),
    );
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
        self.launch(Path::new("wtype"), &["-s", "120000"], "keyboard.log")?;
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
        self.wait_for("calendar widget window", || Ok(self.window().is_ok()))?;
        self.wait_for("calendar widget loaded", || {
            let state = self.state()?;
            Ok(state["ready"] == true && state["loaded"] == true)
        })
    }
    fn check(&mut self) -> Result<()> {
        self.start()?;
        let mut checks = Vec::new();
        let wall_time = self.ipc("wallTime", &[])?;
        require(
            wall_time.contains("1:30"),
            format!("Calendar wall time: {wall_time}"),
        )?;
        checks.push("Calendar-zone wall-time label is not normalised through the host DST gap");
        let initial = self.state()?;
        require(
            initial["selected"] == "2026-10-31"
                && initial["events"].as_array().is_some_and(|v| v.len() == 2),
            format!("Initial calendar state: {initial}"),
        )?;
        File::create(self.out.join("delay"))?;
        self.ipc("clock", &["2026-11-01"])?;
        let immediate = self.state()?;
        require(
            immediate["selected"] == "2026-11-01"
                && immediate["month"] == "2026-11-01"
                && immediate["events"].as_array().is_some_and(Vec::is_empty),
            format!("Stale rollover state: {immediate}"),
        )?;
        self.wait_for("mixed calendar events", || {
            let state = self.state()?;
            Ok(state["ready"] == true && state["events"].as_array().is_some_and(|v| v.len() == 4))
        })?;
        fs::remove_file(self.out.join("delay"))?;
        let mixed = self.state()?;
        let times: Vec<_> = mixed["events"]
            .as_array()
            .ok_or("Events missing")?
            .iter()
            .map(|event| event["startTime"].as_str().unwrap_or("").to_owned())
            .collect();
        require(
            times == ["00:00", "01:30", "02:00", "03:00"],
            format!("Mixed order: {mixed}"),
        )?;
        require(
            mixed["events"][0]["allDay"] == true && mixed["events"][1]["calendar"] == "Travel",
            format!("Mixed data: {mixed}"),
        )?;
        checks.push("Following-today month rollover clears stale prior-day rows while delayed read-only provider returns mixed all-day, overlapping and cross-timezone data");

        let before = self.state()?["actions"]
            .as_array()
            .ok_or("Actions missing")?
            .len();
        let body = self.point("widget-calendar-content", 0.25, 0.4)?;
        self.click(body)?;
        let state = self.state()?;
        require(
            state["actions"]
                .as_array()
                .is_some_and(|v| v.len() == before)
                && state["selected"] == "2026-11-01",
            format!("Background click mutated calendar: {state}"),
        )?;
        let day = self.point("calendar-day-2026-11-02", 0.5, 0.5)?;
        self.click(day)?;
        self.wait_for("day selection", || {
            let state = self.state()?;
            Ok(state["selected"] == "2026-11-02" && state["ready"] == true)
        })?;
        let state = self.state()?;
        require(
            state["events"].as_array().is_some_and(Vec::is_empty)
                && state["actions"]
                    .as_array()
                    .is_some_and(|v| v.len() == before),
            format!("Day selection invoked an action: {state}"),
        )?;
        self.ipc("clock", &["2026-12-01"])?;
        let state = self.state()?;
        require(
            state["selected"] == "2026-11-02"
                && state["month"] == "2026-11-01"
                && state["following"] == false,
            format!("Explicit selection lost: {state}"),
        )?;
        checks.push("Actual day pointer activation selects once without opening the app; explicit selection survives later clock/month rollover");

        self.ipc("clock", &["2026-11-01"])?;
        self.ipc("today", &[])?;
        self.wait_for("today events", || {
            let state = self.state()?;
            Ok(state["ready"] == true && state["events"].as_array().is_some_and(|v| v.len() == 4))
        })?;
        let wanted = self.state()?["events"][1].clone();
        let event_id = wanted["id"].as_str().ok_or("Event ID missing")?;
        let event_name = self.ipc("eventName", &[])?;
        require(
            ["iCloud", "Travel", "Europe/London"]
                .iter()
                .all(|part| event_name.contains(part)),
            format!("Event accessible name: {event_name}"),
        )?;
        self.click(self.point(&format!("calendar-event-{event_id}"), 0.5, 0.5)?)?;
        let state = self.state()?;
        require(
            state["actions"]
                .as_array()
                .is_some_and(|v| v.len() == before + 1)
                && state["actions"][before]["id"] == event_id
                && state["actions"][before]["day"] == "2026-11-01",
            format!("Pointer event action: {state}"),
        )?;
        self.ipc("focusGrid", &[])?;
        self.focus_until(&format!("calendar-event-{event_id}"))?;
        self.key(&["-k", "space"])?;
        let state = self.state()?;
        require(
            state["actions"]
                .as_array()
                .is_some_and(|v| v.len() == before + 2)
                && state["actions"][before + 1]["id"] == event_id,
            format!("Keyboard event action: {state}"),
        )?;
        checks.push("Actual event pointer and keyboard activations each open exactly the same provider/calendar/event identity once");

        self.focus_until("calendar-open-related")?;
        pause(200);
        let state = self.state()?;
        let related = &state["related"];
        require(
            number(related, "y")? >= 0.
                && number(related, "y")? + number(related, "height")? < 760.,
            format!("Related app link offscreen: {state}"),
        )?;
        self.key(&["-k", "space"])?;
        let state = self.state()?;
        require(
            state["actions"]
                .as_array()
                .is_some_and(|v| v.len() == before + 3)
                && state["actions"][before + 2]["kind"] == "related"
                && state["actions"][before + 2]["day"] == "2026-11-01",
            format!("Keyboard related action: {state}"),
        )?;
        self.click(self.point("calendar-open-related", 0.5, 0.5)?)?;
        let state = self.state()?;
        require(
            state["actions"]
                .as_array()
                .is_some_and(|v| v.len() == before + 4)
                && state["actions"][before + 3]["kind"] == "related"
                && state["actions"][before + 3]["day"] == "2026-11-01",
            format!("Pointer related action: {state}"),
        )?;
        checks.push("Related-app link is distinct, reachable after focus scrolling, and each pointer/keyboard activation opens exactly once");

        self.call(
            "grim",
            &[self
                .out
                .join("mixed-widget-dark.png")
                .to_str()
                .ok_or("Non-UTF8 capture")?],
        )?;
        self.ipc("focusGrid", &[])?;
        self.key(&["-k", "Right"])?;
        let state = self.state()?;
        require(
            state["selected"] == "2026-11-02"
                && state["events"].as_array().is_some_and(Vec::is_empty)
                && state["actions"]
                    .as_array()
                    .is_some_and(|v| v.len() == before + 4),
            format!("Arrow navigation: {state}"),
        )?;
        checks.push("Actual calendar-grid arrow navigation changes the date once without invoking the associated app");

        self.ipc("clock", &["2026-04-30"])?;
        self.ipc("today", &[])?;
        self.wait_for("April calendar", || Ok(self.state()?["ready"] == true))?;
        self.ipc("pause", &["true"])?;
        let position = self.point("calendar-day-2026-04-10", 0.5, 0.5)?;
        self.move_to(position.0, position.1)?;
        self.command("p")?;
        self.ipc("clock", &["2026-05-01"])?;
        self.command("r")?;
        self.wait_for("held day identity", || {
            let state = self.state()?;
            Ok(state["selected"] == "2026-04-10" && state["ready"] == true)
        })?;
        checks.push("Month rollover during a held day press preserves the pressed date identity instead of activating the replacement month cell");

        self.ipc("clock", &["2026-04-10"])?;
        self.ipc("today", &[])?;
        self.wait_for("April related link", || Ok(self.state()?["ready"] == true))?;
        let position = self.point("calendar-open-related", 0.5, 0.5)?;
        self.move_to(position.0, position.1)?;
        self.command("p")?;
        let count = self.state()?["actions"]
            .as_array()
            .ok_or("Actions missing")?
            .len();
        self.ipc("clock", &["2026-04-11"])?;
        self.command("r")?;
        let state = self.state()?;
        require(
            state["actions"]
                .as_array()
                .is_some_and(|v| v.len() == count + 1)
                && state["actions"][count]["day"] == "2026-04-10"
                && state["selected"] == "2026-04-11",
            format!("Related link press date changed: {state}"),
        )?;
        checks.push("Related-app press captures its date across a clock tick rather than opening a different release-time date");

        self.ipc("pause", &["false"])?;
        self.wait_for("resumed calendar", || Ok(self.state()?["ready"] == true))?;
        File::create(self.out.join("unavailable"))?;
        self.ipc("refresh", &[])?;
        self.wait_for("provider error", || {
            let state = self.state()?;
            Ok(state["ready"] == true
                && state["error"]
                    .as_str()
                    .is_some_and(|v| v.contains("unavailable")))
        })?;
        require(
            self.state()?["selected"] == "2026-04-11",
            "Provider failure changed selection",
        )?;
        fs::remove_file(self.out.join("unavailable"))?;
        checks.push("Provider failure is explicit and cannot silently select a different date or invoke an app");

        let log = fs::read_to_string(self.out.join("shell.log"))?;
        for marker in [
            "WARN scene:",
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Failed to load configuration",
            "Cannot assign",
        ] {
            require(!log.contains(marker), format!("QML error {marker}: {log}"))?;
        }
        let requests: Vec<Value> = fs::read_to_string(self.out.join("requests.jsonl"))?
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
        require(
            requests
                .iter()
                .all(|row| row["action"] == "list" || row["action"] == "load"),
            format!("Mutating calendar request: {requests:?}"),
        )?;
        save_json(
            &self.out.join("result.json"),
            &json!({
                "passed":true,"checks":checks,"final":self.state()?,
                "requests":requests,
                "scope":"Private compositor, actual desktop calendar widget and pointer/keyboard; synthetic local ICS mirror only"
            }),
        )?;
        println!(
            "PASS native calendar widget mixed data, rollover and exact single-action identity"
        );
        Ok(())
    }
}
fn provider() -> Result<()> {
    let root = PathBuf::from(env::var("LUMEN_CALENDAR_FIXTURE")?);
    let real = PathBuf::from(env::var("LUMEN_CALENDAR_REAL")?);
    let mut line = String::new();
    BufReader::new(std::io::stdin()).read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line)?;
    require(
        request["action"] == "list" || request["action"] == "load",
        "Read-only widget must not mutate or sync",
    )?;
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("requests.jsonl"))?;
    writeln!(log, "{}", serde_json::to_string(&request)?)?;
    if root.join("delay").exists() {
        pause(1500);
    }
    if root.join("unavailable").exists() {
        println!(
            "{}",
            json!({"error":"Synthetic calendar provider unavailable"})
        );
        return Ok(());
    }
    let mut forwarded = request;
    forwarded["action"] = json!("list");
    let mut child = Command::new(real)
        .env("XDG_DATA_HOME", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    {
        let input = child.stdin.as_mut().ok_or("Calendar stdin missing")?;
        writeln!(input, "{}", serde_json::to_string(&forwarded)?)?;
    }
    let output = child.wait_with_output()?;
    require(
        output.status.success(),
        format!(
            "Rust calendar failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    std::io::stdout().write_all(&output.stdout)?;
    Ok(())
}
fn run() -> Result<()> {
    if env::args_os()
        .next()
        .and_then(|path| PathBuf::from(path).file_name().map(|v| v.to_os_string()))
        .as_deref()
        == Some(std::ffi::OsStr::new("lumen-calendar"))
    {
        return provider();
    }
    let opt = options()?;
    if !opt.inside {
        return private_session();
    }
    setup(opt)?.check()
}
fn main() {
    if let Err(error) = run() {
        eprintln!("calendar-widget fixture: {error}");
        std::process::exit(1);
    }
}
