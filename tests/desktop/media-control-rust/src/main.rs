//! Exercise native media controls against private MPRIS and AT-SPI services.
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
use zbus::blocking::{Connection as BusConnection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

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
    provider: PathBuf,
    atspi: PathBuf,
    output: PathBuf,
    assets: Option<PathBuf>,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut pointer_tool = None;
    let mut provider = None;
    let mut atspi = None;
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
            Some("--provider") => {
                provider = Some(PathBuf::from(args.next().ok_or("Missing provider")?))
            }
            Some("--atspi") => {
                atspi = Some(PathBuf::from(args.next().ok_or("Missing AT-SPI runtime")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--assets") => assets = Some(PathBuf::from(args.next().ok_or("Missing assets")?)),
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown media-control fixture option")),
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
        provider: provider.ok_or("--provider required")?,
        atspi: atspi.ok_or("--atspi required")?,
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
        format!("Private media-control fixture exited {status}"),
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
            "mediaAudit",
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
    fn point(&self, name: &str, rx: f64) -> Result<(f64, f64)> {
        let geometry = self.ipc_json("geometry", &[name])?;
        require(!geometry.is_null(), format!("Media control absent: {name}"))?;
        Ok((
            number(&geometry, "x")? + number(&geometry, "width")? * rx,
            number(&geometry, "y")? + number(&geometry, "height")? / 2.,
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
    fn click_name(&mut self, name: &str, rx: f64) -> Result<()> {
        self.click(self.point(name, rx)?)
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.call("wtype", args)?;
        pause(120);
        Ok(())
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
        ("QT_ACCESSIBILITY", "1"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1"),
        ("GSETTINGS_BACKEND", "memory"),
        (
            "LUMEN_COMPOSITOR_EFFECTS",
            if opt.effects { "1" } else { "0" },
        ),
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
        "NO_AT_BRIDGE",
        "GTK_A11Y",
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
        .join("share/lumen-media-control-fixture");
    let qml = opt.output.join("shell.qml");
    fs::copy(package.join("fixture.qml"), &qml)?;
    vars.insert(
        "LUMEN_MPRIS_CALLS".into(),
        opt.output.join("calls.jsonl").display().to_string(),
    );
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
const CONTROL_NAME: &str = "org.lumen.MediaFixture";
const CONTROL_PATH: &str = "/org/lumen/MediaFixture";
fn name_owned(connection: &BusConnection, name: &str) -> Result<bool> {
    let bus = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    Ok(bus.call("NameHasOwner", &(name,))?)
}
fn control_add(connection: &BusConnection, name: &str, properties: Value) -> Result<()> {
    let control = Proxy::new(connection, CONTROL_NAME, CONTROL_PATH, CONTROL_NAME)?;
    let data = json!({"name":name,"identity":format!("Player {name}"),"properties":properties});
    control.call::<_, _, ()>("Add", &(serde_json::to_string(&data)?,))?;
    Ok(())
}
fn control_configure(connection: &BusConnection, name: &str, properties: Value) -> Result<()> {
    let control = Proxy::new(connection, CONTROL_NAME, CONTROL_PATH, CONTROL_NAME)?;
    control.call::<_, _, ()>("Configure", &(name, serde_json::to_string(&properties)?))?;
    Ok(())
}
fn control_remove(connection: &BusConnection, name: &str) -> Result<()> {
    let control = Proxy::new(connection, CONTROL_NAME, CONTROL_PATH, CONTROL_NAME)?;
    control.call::<_, _, ()>("Remove", &(name,))?;
    Ok(())
}
fn control_calls(connection: &BusConnection) -> Result<Vec<Value>> {
    let control = Proxy::new(connection, CONTROL_NAME, CONTROL_PATH, CONTROL_NAME)?;
    let data: String = control.call("Calls", &())?;
    Ok(serde_json::from_str(&data)?)
}
fn accessible(
    connection: &BusConnection,
    name: &str,
    path: &Path,
) -> Result<Option<(String, OwnedObjectPath)>> {
    use std::collections::{HashSet, VecDeque};
    let mut pending = VecDeque::from([(
        "org.a11y.atspi.Registry".to_owned(),
        OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root")?,
    )]);
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    let mut found = None;
    while let Some((destination, object)) = pending.pop_front() {
        if rows.len() >= 150 {
            break;
        }
        if object.as_str() == "/org/a11y/atspi/null" {
            continue;
        }
        if !seen.insert((destination.clone(), object.clone())) {
            continue;
        }
        let properties = Proxy::new(
            connection,
            destination.as_str(),
            object.as_str(),
            "org.freedesktop.DBus.Properties",
        )?;
        let label: OwnedValue = properties.call("Get", &("org.a11y.atspi.Accessible", "Name"))?;
        let label = String::try_from(label)?;
        let item = Proxy::new(
            connection,
            destination.as_str(),
            object.as_str(),
            "org.a11y.atspi.Accessible",
        )?;
        let role: String = item.call("GetRoleName", &())?;
        rows.push(json!({"name":label,"role":role}));
        if label == name {
            found = Some((destination.clone(), object.clone()));
        }
        let children: Vec<(String, OwnedObjectPath)> = item.call("GetChildren", &())?;
        pending.extend(children);
    }
    save_json(path, &json!(rows))?;
    Ok(found)
}
impl Driver {
    fn selected(&self) -> Result<String> {
        Ok(self.state()?["selected"].as_str().unwrap_or("").to_owned())
    }
    fn start(&mut self) -> Result<(BusConnection, BusConnection)> {
        let atspi = fs::canonicalize(&self.opt.atspi)?;
        self.launch(
            &atspi.join("libexec/at-spi-bus-launcher"),
            &["--launch-immediately", "--a11y=1", "--screen-reader=1"],
            "a11y-bus.log",
        )?;
        let session = BusConnection::session()?;
        self.wait_for("AT-SPI bus", || name_owned(&session, "org.a11y.Bus"))?;
        let a11y_service = Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus")?;
        let address: String = a11y_service.call("GetAddress", &())?;
        self.vars
            .insert("AT_SPI_BUS_ADDRESS".into(), address.clone());
        let a11y = zbus::blocking::connection::Builder::address(address.as_str())?.build()?;
        self.launch(
            &atspi.join("libexec/at-spi2-registryd"),
            &[],
            "registry.log",
        )?;
        self.wait_for("AT-SPI registry", || {
            name_owned(&a11y, "org.a11y.atspi.Registry")
        })?;
        let provider = fs::canonicalize(&self.opt.provider)?;
        self.launch(&provider, &[], "provider.log")?;
        self.wait_for("MPRIS mock control", || name_owned(&session, CONTROL_NAME))?;

        let config = self.out.join("sway.conf");
        fs::write(
            &config,
            "output HEADLESS-1 resolution 800x600\nxwayland disable\n",
        )?;
        let sway = resolve_command(&self.opt.sway, self.vars.get("PATH").ok_or("PATH missing")?)?;
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
        self.wait_for("no-service media state", || {
            Ok(self.state()?["title"] == "Not Playing")
        })?;
        Ok((session, a11y))
    }
}
impl Driver {
    fn check(&mut self) -> Result<()> {
        let (session, a11y) = self.start()?;
        let mut checks = Vec::new();
        require(
            self.state()?["playEnabled"] == false,
            "No-service playback was enabled",
        )?;
        checks.push("No service leaves playback unavailable");
        control_add(&session, "alpha", json!({"PlaybackStatus":"Playing"}))?;
        self.wait_for("alpha playback", || {
            let state = self.state()?;
            Ok(state["playing"] == true && state["title"] == "Synthetic title")
        })?;
        let state = self.state()?;
        require(
            state["artist"] == "Synthetic artist"
                && state["volumeSupported"] == true
                && state["seek"] == true,
            format!("Alpha metadata or capabilities: {state}"),
        )?;
        control_add(
            &session,
            "beta",
            json!({
                "PlaybackStatus":"Paused",
                "Metadata":{"mpris:trackid":"/fixture/beta","mpris:length":240000000,
                    "xesam:title":"Second title","xesam:artist":["Second artist"]}
            }),
        )?;
        self.wait_for("two media players", || {
            Ok(self.state()?["players"]
                .as_array()
                .is_some_and(|v| v.len() == 2))
        })?;
        self.ipc("focus", &["media-source"])?;
        self.key(&["-k", "End"])?;
        self.wait_for("beta selected", || Ok(self.selected()?.ends_with(".beta")))?;
        require(
            self.state()?["title"] == "Second title",
            "Beta title unavailable",
        )?;
        let before = control_calls(&session)?.len();
        self.ipc("focus", &["media-play"])?;
        self.key(&["-k", "space"])?;
        self.wait_for("beta keyboard play", || {
            Ok(control_calls(&session)?.len() == before + 1)
        })?;
        let calls = control_calls(&session)?;
        require(
            calls.last().is_some_and(|row| {
                row["name"].as_str().is_some_and(|v| v.ends_with(".beta"))
                    && (row["method"] == "Play" || row["method"] == "PlayPause")
            }),
            format!("Keyboard targeted wrong player: {calls:?}"),
        )?;
        let supported = self.out.join("supported.png");
        self.wait_for("painted supported media", || {
            self.call("grim", &[supported.to_str().ok_or("Non-UTF8 screenshot")?])?;
            let bottom = (30. + number(&self.state()?, "height")?).round() as usize;
            let (_, deviation) = screenshot_moments(&supported, [30, 30, 350, bottom])?;
            Ok(deviation.into_iter().fold(0., f64::max) > 10.)
        })?;
        checks.push("Actual MPRIS properties supply title/artist/status and keyboard source selection/play target the second player");

        control_add(&session, "gamma", json!({}))?;
        self.wait_for("three media players", || {
            Ok(self.state()?["players"]
                .as_array()
                .is_some_and(|v| v.len() == 3))
        })?;
        self.ipc("focus", &["media-source"])?;
        self.key(&["-k", "space"])?;
        self.wait_for("source chooser open", || {
            Ok(self.state()?["sourceOpen"] == true)
        })?;
        control_remove(&session, "gamma")?;
        control_add(&session, "delta", json!({}))?;
        self.wait_for("replacement source list", || {
            Ok(self.state()?["players"]
                .as_array()
                .is_some_and(|v| v.len() == 3))
        })?;
        let choices = self.state()?["choices"]
            .as_array()
            .ok_or("Source choices missing")?
            .clone();
        require(
            choices.iter().any(|row| row == "Player gamma")
                && !choices.iter().any(|row| row == "Player delta"),
            format!("Open chooser identity changed: {choices:?}"),
        )?;
        self.key(&["-k", "End", "-k", "Return"])?;
        self.wait_for("source chooser closed", || {
            Ok(self.state()?["sourceOpen"] == false)
        })?;
        require(
            self.selected()?.ends_with(".beta"),
            format!("Stale chooser selected {}", self.selected()?),
        )?;
        control_remove(&session, "delta")?;
        self.wait_for("two players after chooser", || {
            Ok(self.state()?["players"]
                .as_array()
                .is_some_and(|v| v.len() == 2))
        })?;
        checks.push("An open source chooser retains its labels and identities; a removed choice cannot select a replacement row");

        let before = control_calls(&session)?.len();
        let point = self.point("media-next", 0.5)?;
        self.move_to(point.0, point.1)?;
        self.command("p")?;
        self.ipc("select", &["org.mpris.MediaPlayer2.alpha"])?;
        self.command("r")?;
        require(
            control_calls(&session)?.len() == before,
            format!(
                "Held pointer action retargeted: {:?}",
                control_calls(&session)?
            ),
        )?;
        checks.push(
            "Pointer press/release cannot redirect a pending action to a newly selected player",
        );

        self.ipc("select", &["org.mpris.MediaPlayer2.beta"])?;
        self.wait_for("beta reselected", || {
            Ok(self.selected()?.ends_with(".beta"))
        })?;
        let point = self.point("media-next", 0.5)?;
        self.move_to(point.0, point.1)?;
        self.command("p")?;
        control_remove(&session, "beta")?;
        self.wait_for("one player after beta removal", || {
            Ok(self.state()?["players"]
                .as_array()
                .is_some_and(|v| v.len() == 1))
        })?;
        control_add(
            &session,
            "beta",
            json!({
                "Metadata":{"mpris:trackid":"/fixture/new","mpris:length":100000000,
                    "xesam:title":"Replacement title"}
            }),
        )?;
        self.wait_for("replacement beta title", || {
            Ok(self.state()?["title"] == "Replacement title")
        })?;
        self.command("r")?;
        require(
            control_calls(&session)?.len() == before,
            format!(
                "Held pointer action reached replacement: {:?}",
                control_calls(&session)?
            ),
        )?;
        checks.push("Replacement owner with the same service name receives no action held on its predecessor");

        let before = control_calls(&session)?.len();
        self.ipc("focus", &["media-next"])?;
        self.key(&["-k", "space"])?;
        self.wait_for("keyboard Next", || {
            Ok(control_calls(&session)?.len() == before + 1)
        })?;
        let calls = control_calls(&session)?;
        require(
            calls
                .last()
                .is_some_and(|row| row["method"] == "Next" && row["generation"] == 5),
            format!("Keyboard Next owner: {calls:?}"),
        )?;
        let before = calls.len();
        let target = wait("AT-SPI Next track", || {
            accessible(
                &a11y,
                "Next track",
                &self.out.join("accessibility-tree.json"),
            )
        })?;
        let action = Proxy::new(
            &a11y,
            target.0.as_str(),
            target.1.as_str(),
            "org.a11y.atspi.Action",
        )?;
        let done: bool = action.call("DoAction", &(0i32,))?;
        require(done, "AT-SPI Next action declined")?;
        self.wait_for("AT-SPI Next call", || {
            Ok(control_calls(&session)?.len() == before + 1)
        })?;
        let calls = control_calls(&session)?;
        require(
            calls
                .last()
                .is_some_and(|row| row["method"] == "Next" && row["generation"] == 5),
            format!("AT-SPI Next owner: {calls:?}"),
        )?;
        for label in ["Media source", "Player volume", "Now Playing"] {
            require(
                accessible(&a11y, label, &self.out.join("accessibility-tree.json"))?.is_some(),
                format!("AT-SPI label missing: {label}"),
            )?;
        }
        checks.push("Actual private AT-SPI exposes media context/source/volume and dispatches the enabled Next action to its exact owner");

        control_configure(&session, "beta", json!({"PlaybackStatus":"Paused"}))?;
        self.wait_for("paused beta", || Ok(self.state()?["playing"] == false))?;
        let before = control_calls(&session)?.len();
        let point = self.point("media-play", 0.5)?;
        self.move_to(point.0, point.1)?;
        self.command("p")?;
        save_json(&self.out.join("held-play-before.json"), &self.state()?)?;
        control_configure(&session, "beta", json!({"PlaybackStatus":"Playing"}))?;
        self.wait_for("concurrent playback", || {
            Ok(self.state()?["playing"] == true)
        })?;
        save_json(&self.out.join("held-play-after.json"), &self.state()?)?;
        self.command("r")?;
        self.wait_for("play intent settled", || {
            Ok(self.state()?["pending"].is_null())
        })?;
        require(
            self.state()?["playing"] == true
                && control_calls(&session)?[before..]
                    .iter()
                    .all(|row| row["method"] == "Play"),
            format!("Held play intent changed: {:?}", control_calls(&session)?),
        )?;
        checks.push("A held Play intent stays Play when another client starts the same player before release");

        control_configure(
            &session,
            "beta",
            json!({
                "CanGoNext":false,"CanSeek":false,"CanControl":false
            }),
        )?;
        self.wait_for("capabilities disabled", || {
            let state = self.state()?;
            Ok(state["nextEnabled"] == false
                && state["playEnabled"] == false
                && state["seek"] == false
                && state["volumeEnabled"] == false)
        })?;
        let before = control_calls(&session)?.len();
        self.click_name("media-next", 0.5)?;
        require(
            control_calls(&session)?.len() == before,
            "Disabled Next sent a call",
        )?;
        checks.push("Live capability loss disables transport, position and player-volume controls without sending commands");

        control_configure(
            &session,
            "beta",
            json!({
                "CanControl":true,"CanSeek":true,"CanGoNext":true
            }),
        )?;
        self.wait_for("capabilities restored", || {
            let state = self.state()?;
            Ok(state["volumeEnabled"] == true && state["seek"] == true)
        })?;
        self.ipc("focus", &["media-volume"])?;
        self.key(&["-k", "Right"])?;
        self.wait_for("volume Set call", || {
            Ok(control_calls(&session)?.len() > before)
        })?;
        let calls = control_calls(&session)?;
        require(
            calls
                .last()
                .is_some_and(|row| row["method"] == "Set" && row["args"][0] == "Volume")
                && number(&self.state()?, "volume")? > 0.5,
            format!("Volume action: {calls:?}"),
        )?;
        let before = calls.len();
        self.click_name("media-position", 0.7)?;
        self.wait_for("SetPosition call", || {
            Ok(control_calls(&session)?.len() > before)
        })?;
        require(
            control_calls(&session)?
                .last()
                .is_some_and(|row| row["method"] == "SetPosition"),
            "Position did not invoke SetPosition",
        )?;
        checks.push("Real keyboard player-volume and pointer position controls write typed MPRIS properties/methods");

        control_remove(&session, "beta")?;
        self.wait_for("one remaining player", || {
            Ok(self.state()?["players"]
                .as_array()
                .is_some_and(|v| v.len() == 1))
        })?;
        control_remove(&session, "alpha")?;
        self.wait_for("no active player", || {
            Ok(self.state()?["title"] == "Not Playing")
        })?;
        control_add(
            &session,
            "unsupported",
            json!({"Volume":null,"CanSeek":false}),
        )?;
        self.wait_for("unsupported player", || {
            Ok(self.state()?["players"]
                .as_array()
                .is_some_and(|v| v.len() == 1))
        })?;
        let state = self.state()?;
        require(
            state["volumeSupported"] == false
                && state["volumeEnabled"] == false
                && state["seek"] == false,
            format!("Unsupported capabilities: {state}"),
        )?;
        self.call(
            "grim",
            &[self
                .out
                .join("unsupported.png")
                .to_str()
                .ok_or("Non-UTF8 capture")?],
        )?;
        control_remove(&session, "unsupported")?;
        self.wait_for("all players gone", || {
            Ok(self.state()?["players"]
                .as_array()
                .is_some_and(Vec::is_empty))
        })?;
        self.call(
            "grim",
            &[self
                .out
                .join("unavailable.png")
                .to_str()
                .ok_or("Non-UTF8 capture")?],
        )?;
        checks.push("Missing optional volume/seek capabilities and disappearing services remain honest and unavailable");

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
        save_json(
            &self.out.join("result.json"),
            &json!({
                "passed":true,"checks":checks,"calls":control_calls(&session)?,
                "scope":"Private real D-Bus MPRIS services, native compositor and actual pointer/keyboard"
            }),
        )?;
        println!("PASS native media transport and captured player identity");
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
        eprintln!("media-control fixture: {error}");
        std::process::exit(1);
    }
}
