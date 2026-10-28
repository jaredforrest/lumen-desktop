//! Run the packaged layout menu against a private headless compositor.
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}

fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}

struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    output: PathBuf,
}

fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut output = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing compositor")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            _ => return Err(fail("Unknown layout-menu fixture option")),
        }
    }
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        output: output.ok_or("--output required")?,
    })
}

fn immutable(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    require(
        path.starts_with("/nix/store") && path.is_file(),
        "Pinned executable required",
    )?;
    Ok(path)
}

fn program(name: &str, values: &BTreeMap<String, String>) -> Result<PathBuf> {
    let path = values.get("PATH").ok_or("Packaged PATH missing")?;
    for directory in env::split_paths(&OsString::from(path)) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            immutable(&candidate)?;
            return Ok(candidate);
        }
    }
    Err(fail(format!("{name} unavailable in packaged PATH")))
}

fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher).arg("--print-env0").output()?;
    require(
        result.status.success(),
        "Packaged shell did not print environment",
    )?;
    let mut values = BTreeMap::new();
    for part in result
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
    {
        let text = std::str::from_utf8(part)?;
        if let Some((key, value)) = text.split_once('=') {
            values.insert(key.into(), value.into());
        }
    }
    require(values.contains_key("PATH"), "Packaged shell omitted PATH")?;
    Ok(values)
}

struct Managed(Child);
impl Managed {
    fn wait(&mut self, seconds: u64) -> Result<std::process::ExitStatus> {
        let until = Instant::now() + Duration::from_secs(seconds);
        loop {
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= until {
                return Err(fail("Owned process did not exit"));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = env::temp_dir().join(format!("lml-{}-{nonce}", std::process::id()));
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

fn wait_display(runtime: &Path, sway: &mut Managed, log: &Path) -> Result<String> {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(path) = fs::read_dir(runtime)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|path| {
                path.file_name().is_some_and(|name| {
                    let name = name.to_string_lossy();
                    name.starts_with("wayland-") && !name.ends_with(".lock")
                })
            })
        {
            return Ok(path
                .file_name()
                .ok_or("Wayland name missing")?
                .to_string_lossy()
                .into_owned());
        }
        if let Some(status) = sway.0.try_wait()? {
            return Err(fail(format!(
                "Headless compositor exited {status}: {}",
                fs::read_to_string(log).unwrap_or_default()
            )));
        }
        if Instant::now() >= until {
            return Err(fail(format!(
                "Headless compositor did not expose display: {}",
                fs::read_to_string(log).unwrap_or_default()
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn run(options: Options) -> Result<()> {
    let launcher = immutable(&options.launcher)?;
    let sway_binary = immutable(&options.sway)?;
    require(!options.output.exists(), "Fresh output directory required")?;
    fs::create_dir_all(&options.output)?;
    fs::set_permissions(&options.output, fs::Permissions::from_mode(0o700))?;
    let output = fs::canonicalize(&options.output)?;
    let private = PrivateRoot::new()?;
    let mut values = launcher_environment(&launcher)?;
    for key in [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
    ] {
        let directory = private.0.join(key.to_lowercase());
        fs::create_dir(&directory)?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        values.insert(key.into(), directory.display().to_string());
    }
    for key in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        values.remove(key);
    }
    for (key, value) in [
        ("QT_QPA_PLATFORM", "offscreen"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_IM_MODULE", ""),
        ("NO_AT_BRIDGE", "1"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("GTK_A11Y", "none"),
    ] {
        values.insert(key.into(), value.into());
    }
    let bus_config = output.join("bus.conf");
    fs::write(
        &bus_config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            values["XDG_RUNTIME_DIR"]
        ),
    )?;
    let bus_log = File::create(output.join("bus.log"))?;
    let mut bus = Managed(
        Command::new(program("dbus-daemon", &values)?)
            .arg("--nofork")
            .arg("--print-address=1")
            .arg(format!("--config-file={}", bus_config.display()))
            .env_clear()
            .envs(&values)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(bus_log))
            .spawn()?,
    );
    let stdout = bus
        .0
        .stdout
        .take()
        .ok_or("Private bus address pipe missing")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut address = String::new();
        let result = BufReader::new(stdout)
            .read_line(&mut address)
            .map(|_| address);
        let _ = sender.send(result);
    });
    let address = receiver.recv_timeout(Duration::from_secs(5))??;
    require(
        address.starts_with("unix:"),
        "Private bus did not return UNIX address",
    )?;
    values.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.trim().into());
    let sway_config = output.join("sway.conf");
    fs::write(&sway_config, "output * resolution 800x600\n")?;
    values.insert("WLR_BACKENDS".into(), "headless".into());
    values.insert("WLR_RENDERER".into(), "pixman".into());
    values.insert("WLR_LIBINPUT_NO_DEVICES".into(), "1".into());
    let compositor_log = output.join("compositor.log");
    let log = File::create(&compositor_log)?;
    let mut sway = Managed(
        Command::new(sway_binary)
            .args(["--unsupported-gpu", "-c"])
            .arg(&sway_config)
            .env_clear()
            .envs(&values)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?,
    );
    let display = wait_display(
        Path::new(&values["XDG_RUNTIME_DIR"]),
        &mut sway,
        &compositor_log,
    )?;
    values.insert("WAYLAND_DISPLAY".into(), display);
    values.insert("QT_QPA_PLATFORM".into(), "wayland".into());
    let assets = fs::canonicalize(env::current_dir()?.join("res/lumen-desktop/shell"))?;
    let scene_source = env::current_exe()?
        .parent()
        .ok_or("Executable directory missing")?
        .parent()
        .ok_or("Package root missing")?
        .join("share/lumen-window-layout-menu-fixture/fixture.qml");
    let scene = fs::read_to_string(scene_source)?
        .replace("@SHELL_ASSETS@", &format!("file:{}", assets.display()));
    let scene_file = output.join("shell.qml");
    fs::write(&scene_file, scene)?;
    let result = Command::new(program("timeout", &values)?)
        .arg("20s")
        .arg(program("quickshell", &values)?)
        .arg("-p")
        .arg(&scene_file)
        .env_clear()
        .envs(&values)
        .output()?;
    fs::write(output.join("quickshell.stdout.log"), &result.stdout)?;
    fs::write(output.join("quickshell.stderr.log"), &result.stderr)?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    require(
        result.status.success(),
        format!("Layout menu scene exited {}: {text}", result.status),
    )?;
    require(
        text.contains("LAYOUT_MENU_PASS"),
        format!("Layout menu did not complete checks: {text}"),
    )?;
    require(
        !text.contains("Failed to load"),
        format!("Layout menu load failed: {text}"),
    )?;
    let mut report = File::create(output.join("result.json"))?;
    report.write_all(b"{\"passed\":true,\"checks\":[\"Packaged layout menu validates targets, capabilities, popup ownership, and visible opening on a private Qt compositor\"]}\n")?;
    sway.0.kill()?;
    sway.wait(5)?;
    bus.0.kill()?;
    bus.wait(5)?;
    println!("PASS packaged window layout menu");
    Ok(())
}

fn main() {
    if let Err(error) = options().and_then(run) {
        eprintln!("Window layout menu fixture: {error}");
        std::process::exit(1);
    }
}
