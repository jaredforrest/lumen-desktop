//! Native small-output system popover acceptance with Rust-only private providers.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zbus::blocking::{Connection, Proxy};

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
        .filter_map(|entry| entry.ok().map(|e| e.path()))
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
        let kind = fs::symlink_metadata(&path)?.file_type();
        if kind.is_symlink() {
            symlink(fs::read_link(&path)?, &to)?;
        } else if kind.is_dir() {
            copy_tree(&path, &to)?;
        } else if kind.is_file() {
            fs::copy(&path, &to)?;
            let mut permissions = fs::metadata(&to)?.permissions();
            permissions.set_mode(permissions.mode() | 0o200);
            fs::set_permissions(&to, permissions)?;
        } else {
            return Err(fail(format!("Unsupported asset {}", path.display())));
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
    provider: PathBuf,
    output: PathBuf,
    assets: Option<PathBuf>,
    theme: String,
    native_frame: bool,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut provider = None;
    let mut output = None;
    let mut assets = None;
    let mut theme = "dark".to_owned();
    let mut native_frame = false;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
            Some("--mpris-provider") => {
                provider = Some(PathBuf::from(args.next().ok_or("Missing MPRIS provider")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--assets") => assets = Some(PathBuf::from(args.next().ok_or("Missing assets")?)),
            Some("--theme") => {
                theme = args.next().ok_or("Missing theme")?.to_string_lossy().into();
                require(theme == "light" || theme == "dark", "Invalid theme")?;
            }
            Some("--native-frame") => {
                let value = args.next().ok_or("Missing native-frame mode")?;
                native_frame = match value.to_str() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err(fail("Invalid native-frame mode")),
                };
            }
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown system-popover fixture option")),
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
        provider: provider.ok_or("--mpris-provider required")?,
        output,
        assets,
        theme,
        native_frame,
        effects,
        inside,
    })
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = Path::new("/tmp").join(format!("lpop-{}-{nonce}", std::process::id()));
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
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
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
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
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
        .envs(vars)
        .status()?;
    require(
        status.success(),
        format!("Private system-popover fixture exited {status}"),
    )
}
struct Managed(Child);
impl Managed {
    fn launch(
        program: &Path,
        args: &[&str],
        vars: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<Self> {
        let file = File::create(log)?;
        Ok(Self(
            Command::new(program)
                .args(args)
                .env_clear()
                .envs(vars)
                .stdin(Stdio::null())
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
            let end = Instant::now() + Duration::from_secs(3);
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
fn calendar_helper() -> Result<()> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line)?;
    require(
        request["action"] == "list" || request["action"] == "load",
        "Synthetic calendar is read only",
    )?;
    println!(
        "{}",
        json!({
            "configured":false,"events":[],"monthEvents":[],"days":[],"total":0,
            "providers":[],"month":request["month"],"date":request["selectedDate"]
        })
    );
    Ok(())
}
fn notification_helper() -> Result<()> {
    let transport = env::args().last().unwrap_or_default();
    let row = json!({
        "id":1,"app":"Synthetic application","desktop":"fixture",
        "summary":"A long notification heading remains available with enlarged interface text",
        "body":"A private synthetic notification body with several lines of explanatory detail.",
        "timestamp":0,"actions":[{"id":"default","label":"Open synthetic notification"}],
        "icon":"","image":"","transient":false,"banner":false,"unread":false,"urgency":1
    });
    let response = json!({
        "transport":transport,"instance":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "revision":1,"ready":true,"rows":[row],"dnd":false,"previewsVisible":true,
        "focus":{"profiles":[],"active":"","name":"","until":0,"scheduled":false}
    });
    println!("{}", response);
    std::io::stdout().flush()?;
    for line in std::io::stdin().lock().lines() {
        line?;
    }
    Ok(())
}
fn name_owned(connection: &Connection, name: &str) -> Result<bool> {
    let bus = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    Ok(bus.call("NameHasOwner", &(name,))?)
}
fn add_media_player(connection: &Connection, name: &str, playing: bool) -> Result<()> {
    let control = Proxy::new(
        connection,
        "org.lumen.MediaFixture",
        "/org/lumen/MediaFixture",
        "org.lumen.MediaFixture",
    )?;
    let config = json!({
        "name":name,"identity":format!("Synthetic {name}"),
        "properties":{"PlaybackStatus":if playing {"Playing"} else {"Paused"},
            "Metadata":{"mpris:trackid":format!("/fixture/{name}"),"mpris:length":180000000,
                "xesam:title":"An extended synthetic track title for enlarged media controls",
                "xesam:artist":["An extended synthetic artist name"]}}
    });
    control.call::<_, _, ()>("Add", &(serde_json::to_string(&config)?,))?;
    Ok(())
}
fn screenshot_variation(path: &Path) -> Result<f64> {
    let mut decoder = png::Decoder::new(File::open(path)?);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut bytes)?;
    let channels = match frame.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(fail(format!("Unsupported screenshot type: {other:?}"))),
    };
    let count = (frame.width * frame.height) as f64;
    let mut sum = [0.; 3];
    let mut square = [0.; 3];
    for pixel in bytes[..frame.buffer_size()].chunks_exact(channels) {
        for channel in 0..3 {
            let value = pixel[channel] as f64;
            sum[channel] += value;
            square[channel] += value * value;
        }
    }
    Ok((0..3)
        .map(|channel| {
            let mean = sum[channel] / count;
            (square[channel] / count - mean * mean).max(0.).sqrt()
        })
        .fold(0., f64::max))
}
struct Driver {
    opt: Options,
    vars: BTreeMap<String, String>,
    out: PathBuf,
    runtime: PathBuf,
    fixture: PathBuf,
    processes: Vec<Managed>,
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
            self.fixture.to_str().ok_or("Non-UTF8 fixture path")?,
            "ipc",
            "call",
            "popoverAudit",
            method,
        ];
        words.extend_from_slice(args);
        self.call("quickshell", &words)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("state", &[])?)?)
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
        )?);
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
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "1"),
        (
            "LUMEN_UNIFIED_OWNED_HEADERS",
            if opt.native_frame { "0" } else { "1" },
        ),
        (
            "LUMEN_COMPOSITOR_EFFECTS",
            if opt.effects { "1" } else { "0" },
        ),
    ] {
        vars.insert(key.into(), value.into());
    }
    for key in [
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DISPLAY",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
    let runtime = PathBuf::from(vars.get("XDG_RUNTIME_DIR").ok_or("Runtime missing")?);
    let source = if let Some(assets) = &opt.assets {
        fs::canonicalize(assets)?
    } else {
        fs::canonicalize(vars.get("LUMEN_DESKTOP_ASSETS").ok_or("Assets missing")?)?
    };
    let copied_assets = opt.output.join("assets");
    copy_tree(&source, &copied_assets)?;
    let palette = read_json(&copied_assets.join("appearance/palette.json"))?;
    let appearance = opt.output.join("appearance.json");
    save_json(
        &appearance,
        &json!({
            "font":palette["font"],"palette":palette[&opt.theme],
            "preferences":{"mode":opt.theme,"reduceMotion":true,"textScale":1.5}
        }),
    )?;
    vars.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let bin = opt.output.join("bin");
    fs::create_dir(&bin)?;
    let exe = env::current_exe()?;
    symlink(&exe, bin.join("lumen-calendar"))?;
    let notifications = bin.join("lumen-notification-helper");
    symlink(&exe, &notifications)?;
    vars.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            bin.display(),
            vars.get("PATH").ok_or("PATH missing")?
        ),
    );
    vars.insert(
        "LUMEN_NOTIFICATION_HELPER".into(),
        notifications.display().to_string(),
    );
    vars.insert(
        "LUMEN_MPRIS_CALLS".into(),
        opt.output.join("media-calls.jsonl").display().to_string(),
    );
    let fixture = copied_assets.join("shell");
    let output = Command::new("lumen-fixture-providers")
        .arg("isolate")
        .arg(&fixture)
        .arg(&opt.output)
        .env_clear()
        .envs(&vars)
        .output()?;
    require(
        output.status.success(),
        format!(
            "Provider isolation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    let _: Value = serde_json::from_slice(&output.stdout)?;
    let menu = fixture.join("MenuBar.qml");
    let text = fs::read_to_string(&menu)?;
    require(
        text.contains("id: dismiss"),
        "Menu dismiss audit anchor missing",
    )?;
    fs::write(&menu,text.replacen("id: dismiss",
        "id: dismiss;Component.onCompleted:bar.shell.auditWindows=bar.shell.auditWindows.concat([dismiss])",1))?;
    let shell = fixture.join("shell.qml");
    let mut text = fs::read_to_string(&shell)?;
    let end = text.rfind('}').ok_or("Shell closing brace missing")?;
    text.truncate(end);
    text.push_str(include_str!("../audit.fragment.qml"));
    text.push_str("}\n");
    fs::write(&shell, text)?;
    let out = opt.output.clone();
    Ok(Driver {
        opt,
        vars,
        out,
        runtime,
        fixture,
        processes: Vec::new(),
    })
}
impl Driver {
    fn start(&mut self) -> Result<()> {
        let provider = fs::canonicalize(&self.opt.provider)?;
        self.launch(&provider, &[], "media-provider.log")?;
        let session = Connection::session()?;
        self.wait_for("MPRIS fixture", || {
            name_owned(&session, "org.lumen.MediaFixture")
        })?;
        add_media_player(&session, "alpha", true)?;
        add_media_player(&session, "beta", false)?;
        let config = self.out.join("sway.conf");
        let mut text = String::from(
            "output HEADLESS-1 resolution 640x480\nxwayland disable\ndefault_border none\nfor_window [app_id=\"org.quickshell\"] floating enable, resize set 420 300\n",
        );
        if self.opt.native_frame {
            text.push_str("default_border normal 1\ndefault_floating_border normal 1\nfor_window [app_id=\"org.quickshell\"] border normal 1\n");
        }
        fs::write(&config, text)?;
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
        let fixture = self
            .fixture
            .to_str()
            .ok_or("Non-UTF8 fixture path")?
            .to_owned();
        self.launch(Path::new("quickshell"), &["-p", &fixture], "shell.log")?;
        self.wait_for("popover audit", || Ok(self.ipc("ready", &[])? == "true"))
    }
    fn check(&mut self) -> Result<()> {
        self.start()?;
        let mut checks = Vec::new();
        for name in ["Controls", "Now Playing", "Date", "Calendar", "session"] {
            self.ipc("open", &[name])?;
            pause(600);
            if name == "Controls" {
                self.ipc("longLabels", &[])?;
            }
            let mut snapshots = Vec::new();
            for index in 0..35 {
                self.call("wtype", &["-k", "Tab"])?;
                pause(80);
                let state = self.state()?;
                if !state["focused"].is_null() {
                    require(
                        state["focused"]["reachable"] == true,
                        format!("{name} focus {index} clipped: {}", state["focused"]),
                    )?;
                }
                snapshots.push(state);
            }
            save_json(
                &self.out.join(format!("{name}-focus.json")),
                &json!(snapshots),
            )?;
            let screenshot = self.out.join(format!("{name}.png"));
            self.call("grim", &[screenshot.to_str().ok_or("Non-UTF8 screenshot")?])?;
            require(
                screenshot_variation(&screenshot)? > 10.,
                format!("Blank {name} popover"),
            )?;
            let commands: BTreeSet<String> = snapshots
                .iter()
                .filter_map(|row| row["focused"]["text"].as_str().map(str::to_owned))
                .collect();
            require(
                commands.len() >= 2,
                format!("{name} has too few focused commands: {commands:?}"),
            )?;
            if name == "Controls" {
                let labels: Vec<&Value> = snapshots
                    .iter()
                    .flat_map(|snap| snap["texts"].as_array().into_iter().flatten())
                    .filter(|text| {
                        text["text"] == "Display configuration and arrangement preferences"
                    })
                    .collect();
                require(
                    !labels.is_empty()
                        && labels
                            .iter()
                            .all(|row| row["truncated"] == false && row["size"] == 18),
                    format!("Enlarged Controls label clipped: {labels:?}"),
                )?;
                require(
                    commands.contains("System Settings…"),
                    format!("Settings command missing: {commands:?}"),
                )?;
            }
            if name == "Now Playing" {
                for required in [
                    "media-source",
                    "media-position",
                    "media-volume",
                    "media-play",
                    "media-next",
                    "media-previous",
                ] {
                    require(
                        commands.contains(required),
                        format!("Media command {required} missing: {commands:?}"),
                    )?;
                }
            }
            if name == "Date" {
                require(
                    commands.contains("Edit Widgets") && commands.contains("Open Calendar"),
                    format!("Date footer commands missing: {commands:?}"),
                )?;
            }
            if name == "session" {
                require(
                    commands.contains("Cancel") && commands.contains("Continue"),
                    format!("Session footer commands missing: {commands:?}"),
                )?;
            }
            checks.push(json!({"surface":name,"focusedCommands":commands}));
        }
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
                "passed":true,"checks":checks,"textScale":1.5,"output":[640,480],
                "providers":"Private synthetic notification/calendar and real private MPRIS services"
            }),
        )?;
        println!("PASS small-output system popover text/reachability");
        Ok(())
    }
}
fn run() -> Result<()> {
    let name = env::args_os()
        .next()
        .and_then(|path| PathBuf::from(path).file_name().map(|v| v.to_os_string()));
    if name.as_deref() == Some(std::ffi::OsStr::new("lumen-calendar")) {
        return calendar_helper();
    }
    if name.as_deref() == Some(std::ffi::OsStr::new("lumen-notification-helper")) {
        return notification_helper();
    }
    let opt = options()?;
    if !opt.inside {
        return private_session();
    }
    setup(opt)?.check()
}
fn main() {
    if let Err(error) = run() {
        eprintln!("system-popover fixture: {error}");
        std::process::exit(1);
    }
}
