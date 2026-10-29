//! Isolated native notification lock and recovery acceptance.
mod lock;
mod recovery;
mod focus_presentation;

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::UnixStream;
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
fn path(value: &Path) -> String {
    value.to_string_lossy().into_owned()
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!(
                "Notification fixture timed out waiting for {label}: {last}"
            )));
        }
        pause(40);
    }
}
fn live_socket(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let kind = fs::symlink_metadata(&from)?.file_type();
        if kind.is_symlink() {
            symlink(fs::read_link(&from)?, &to)?;
        } else if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to)?;
            let mut mode = fs::metadata(&to)?.permissions();
            mode.set_mode(mode.mode() | 0o200);
            fs::set_permissions(&to, mode)?;
        } else {
            return Err(fail(format!("Unsupported asset {}", from.display())));
        }
    }
    Ok(())
}
fn launch(
    command: &str,
    args: &[&str],
    envs: &BTreeMap<String, String>,
    log: &Path,
    piped: bool,
) -> Result<Child> {
    let file = File::create(log)?;
    let stderr = file.try_clone()?;
    let mut cmd = Command::new(command);
    cmd.args(args)
        .env_clear()
        .envs(envs)
        .stdout(file)
        .stderr(stderr)
        .stdin(if piped { Stdio::piped() } else { Stdio::null() });
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    Ok(cmd.spawn()?)
}
fn call(command: &str, args: &[&str], envs: &BTreeMap<String, String>) -> Result<String> {
    let result = Command::new(command)
        .args(args)
        .env_clear()
        .envs(envs)
        .output()?;
    require(
        result.status.success(),
        format!(
            "{command} {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}
fn call_input(
    command: &str,
    args: &[&str],
    envs: &BTreeMap<String, String>,
    input: &[u8],
) -> Result<String> {
    let mut process = Command::new(command)
        .args(args)
        .env_clear()
        .envs(envs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    process
        .stdin
        .take()
        .ok_or("Missing child stdin")?
        .write_all(input)?;
    let result = process.wait_with_output()?;
    require(
        result.status.success(),
        format!("{command}: {}", String::from_utf8_lossy(&result.stderr)),
    )?;
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}
struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        command: &str,
        args: &[&str],
        envs: &BTreeMap<String, String>,
        log: &Path,
        piped: bool,
    ) -> Result<usize> {
        self.0.push(launch(command, args, envs, log, piped)?);
        Ok(self.0.len() - 1)
    }
    fn stop(&mut self, index: usize) -> Result<()> {
        let child = &mut self.0[index];
        if child.try_wait()?.is_none() {
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGTERM);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait()?.is_none() {
            if Instant::now() > deadline {
                unsafe {
                    libc::killpg(child.id() as i32, libc::SIGKILL);
                }
                child.wait()?;
                break;
            }
            pause(25);
        }
        Ok(())
    }
    fn signal(&mut self, index: usize, signal: i32) {
        unsafe {
            libc::killpg(self.0[index].id() as i32, signal);
        }
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        for index in (0..self.0.len()).rev() {
            let _ = self.stop(index);
        }
    }
}
struct Options {
    scenario: String,
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    lock_client: Option<PathBuf>,
    frame_tool: Option<PathBuf>,
    effects: bool,
    inside: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut scenario = None;
        let mut output = None;
        let mut launcher = None;
        let mut sway = None;
        let mut lock_client = None;
        let mut frame_tool = None;
        let mut effects = false;
        let mut inside = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("lock" | "recovery" | "focus-presentation") => scenario = Some(arg.to_string_lossy().into_owned()),
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
                Some("--lock-client") => {
                    lock_client = Some(PathBuf::from(args.next().ok_or("Missing lock client")?))
                }
                Some("--frame-tool") => {
                    frame_tool = Some(PathBuf::from(args.next().ok_or("Missing frame tool")?))
                }
                Some("--effects") => effects = true,
                Some("--inside") => inside = true,
                _ => return Err(fail(format!("Unknown argument {}", arg.to_string_lossy()))),
            }
        }
        let output = output.ok_or("Missing output")?;
        let output = if output.is_absolute() {
            output
        } else {
            env::current_dir()?.join(output)
        };
        Ok(Self {
            scenario: scenario.ok_or("Missing scenario")?,
            output,
            launcher: launcher.ok_or("Missing launcher")?.canonicalize()?,
            sway: sway.ok_or("Missing sway")?.canonicalize()?,
            lock_client: lock_client.map(|p| p.canonicalize()).transpose()?,
            frame_tool: frame_tool.map(|p| p.canonicalize()).transpose()?,
            effects,
            inside,
        })
    }
    fn arguments(&self) -> Vec<String> {
        let mut result = vec![
            self.scenario.clone(),
            "--output".into(),
            path(&self.output),
            "--launcher".into(),
            path(&self.launcher),
            "--sway".into(),
            path(&self.sway),
        ];
        if let Some(value) = &self.lock_client {
            result.extend(["--lock-client".into(), path(value)]);
        }
        if let Some(value) = &self.frame_tool {
            result.extend(["--frame-tool".into(), path(value)]);
        }
        if self.effects {
            result.push("--effects".into());
        }
        result.push("--inside".into());
        result
    }
}
fn private_dir() -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let private = PathBuf::from(format!(
        "/tmp/nn-{}-{}",
        std::process::id(),
        stamp % 1_000_000_000
    ));
    fs::create_dir(&private)?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    Ok(private)
}
fn launcher_env(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher).arg("--print-env0").output()?;
    require(result.status.success(), "Launcher environment unavailable")?;
    let mut vars = BTreeMap::new();
    for part in result.stdout.split(|byte| *byte == 0) {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    Ok(vars)
}
struct Fixture {
    options: Options,
    private: PathBuf,
    vars: BTreeMap<String, String>,
    assets: PathBuf,
    children: Children,
    connection: Connection,
    compositor: usize,
    server: usize,
    qml: PathBuf,
}
impl Fixture {
    fn new(options: Options) -> Result<Self> {
        fs::create_dir_all(&options.output)?;
        let private = PathBuf::from(env::var("LUMEN_TEST_PRIVATE_ROOT")?);
        let mut vars = launcher_env(&options.launcher)?;
        for key in [
            "HOME",
            "XDG_RUNTIME_DIR",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
        ] {
            let value = if key == "XDG_RUNTIME_DIR" {
                private.join("r")
            } else {
                private.join(key)
            };
            fs::create_dir_all(&value)?;
            fs::set_permissions(&value, fs::Permissions::from_mode(0o700))?;
            vars.insert(key.into(), path(&value));
        }
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "AT_SPI_BUS_ADDRESS",
        ] {
            vars.remove(key);
        }
        for (key, value) in [
            ("WLR_BACKENDS", "headless"),
            (
                "WLR_RENDERER",
                if options.effects { "gles2" } else { "pixman" },
            ),
            (
                "WLR_RENDERER_FORCE_SOFTWARE",
                if options.effects { "1" } else { "0" },
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
            ("LUMEN_LAUNCH_SCOPES", "0"),
            ("LUMEN_LOCK_PREVIEWS", "0"),
        ] {
            vars.insert(key.into(), value.into());
        }
        vars.insert(
            "DBUS_SYSTEM_BUS_ADDRESS".into(),
            format!("unix:path={}/no-system-bus", private.display()),
        );
        let assets = PathBuf::from(
            vars.get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Missing packaged assets")?,
        );
        require(
            assets.join("shell/NotificationState.qml").is_file(),
            "Packaged notification assets missing",
        )?;
        vars.insert(
            "LUMEN_NOTIFICATION_STATE".into(),
            path(&private.join("history")),
        );
        vars.insert(
            "LUMEN_NIX_SETTINGS_ACTIVE".into(),
            path(&private.join("settings.json")),
        );
        fs::write(
            private.join("settings.json"),
            r#"{"notifications":{"preview":"when_unlocked","policies":{}}}"#,
        )?;
        let palette = read(&assets.join("appearance/palette.json"))?;
        save(
            &private.join("appearance.json"),
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}),
        )?;
        vars.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            path(&private.join("appearance.json")),
        );
        let connection = Connection::session()?;
        let mut fixture = Self {
            options,
            private,
            vars,
            assets,
            children: Children(Vec::new()),
            connection,
            compositor: 0,
            server: 0,
            qml: PathBuf::new(),
        };
        fs::write(
            fixture.private.join("sway.conf"),
            "output HEADLESS-1 resolution 1100x800\nxwayland disable\nseat seat0 fallback true\n",
        )?;
        fixture.vars.insert(
            "SWAYSOCK".into(),
            path(&fixture.private.join("r/compositor.sock")),
        );
        fixture.start_compositor("sway.log")?;
        fixture.server = fixture.start(
            "notifications.log",
            &path(Path::new(
                fixture
                    .vars
                    .get("LUMEN_NOTIFICATION_HELPER")
                    .ok_or("Missing notification helper")?,
            )),
            &["serve"],
            false,
        )?;
        wait("notification server", || {
            Ok((fixture
                .snapshot()
                .ok()
                .and_then(|v| v["lock"]["locked"].as_bool())
                == Some(false))
            .then_some(()))
        })?;
        copy_tree(&fixture.assets, &fixture.private.join("assets"))?;
        let scene = match fixture.options.scenario.as_str() {
            "lock" => include_str!("../lock.qml"),
            "recovery" => include_str!("../recovery.qml"),
            "focus-presentation" => include_str!("../focus_presentation.qml"),
            _ => return Err(fail("Unknown notification scenario")),
        };
        fixture.qml = fixture.private.join("fixture.qml");
        fs::write(&fixture.qml, scene)?;
        Ok(fixture)
    }
    fn start(&mut self, log: &str, command: &str, args: &[&str], piped: bool) -> Result<usize> {
        self.children.launch(
            command,
            args,
            &self.vars,
            &self.options.output.join(log),
            piped,
        )
    }
    fn start_compositor(&mut self, log: &str) -> Result<()> {
        let sock = PathBuf::from(self.vars.get("SWAYSOCK").ok_or("Missing socket")?);
        if sock.exists() {
            require(!live_socket(&sock), "Old compositor socket remains live")?;
            fs::remove_file(&sock)?;
        }
        let config = path(&self.private.join("sway.conf"));
        let sway = path(&self.options.sway);
        self.compositor = self.start(log, &sway, &["--unsupported-gpu", "-c", &config], false)?;
        wait("compositor IPC", || Ok(live_socket(&sock).then_some(())))?;
        let display = wait("Wayland display", || {
            Ok(fs::read_dir(self.private.join("r"))?
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .find(|p| {
                    p.file_name().is_some_and(|name| {
                        let name = name.to_string_lossy();
                        name.starts_with("wayland-") && !name.ends_with(".lock")
                    }) && live_socket(p)
                })
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())))
        })?;
        self.vars.insert("WAYLAND_DISPLAY".into(), display);
        Ok(())
    }
    fn snapshot(&self) -> Result<Value> {
        let proxy = Proxy::new(
            &self.connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.lumendesktop.Notifications",
        )?;
        let raw: String = proxy.call("Snapshot", &())?;
        Ok(serde_json::from_str(&raw)?)
    }
    fn notify(&self, label: &str, icon: &str, image: bool) -> Result<u32> {
        use std::collections::HashMap;
        use zbus::zvariant::Value as BusValue;
        let proxy = Proxy::new(
            &self.connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )?;
        let mut hints: HashMap<&str, BusValue<'_>> = HashMap::new();
        if image {
            hints.insert(
                "image-data",
                BusValue::new((1i32, 1i32, 4i32, true, 8i32, 4i32, vec![255u8, 0, 0, 255])),
            );
        }
        let action_label = format!("Private action {label}");
        let actions = vec!["default", action_label.as_str()];
        let id: u32 = proxy.call(
            "Notify",
            &(
                "Fixture",
                0u32,
                icon,
                format!("Private subject {label}"),
                format!("Private body {label}"),
                actions,
                hints,
                0i32,
            ),
        )?;
        Ok(id)
    }
    fn ipc(&self, target: &str, method: &str, args: &[&str]) -> Result<String> {
        let qml = path(&self.qml);
        let mut all = vec!["-p", qml.as_str(), "ipc", "call", target, method];
        all.extend_from_slice(args);
        call("quickshell", &all, &self.vars)
    }
    fn ui(&self, target: &str) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc(target, "status", &[])?)?)
    }
    fn preference(&self, value: &str) -> Result<()> {
        save(
            &self.private.join("settings.json"),
            &json!({"notifications":{"preview":value,"policies":{}}}),
        )
    }
}
fn main() -> Result<()> {
    let arguments: Vec<String> = env::args().collect();
    if arguments.get(1).is_some_and(|value| value == "producer") {
        return recovery::producer(arguments.get(2).ok_or("Missing producer label")?);
    }
    let options = Options::parse()?;
    if !options.inside {
        fs::create_dir(&options.output)?;
        let private = private_dir()?;
        let bus = private.join("bus.conf");
        fs::write(
            &bus,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
                private.display()
            ),
        )?;
        let mut command = Command::new("dbus-run-session");
        command
            .arg(format!("--config-file={}", bus.display()))
            .arg("--")
            .arg(env::current_exe()?)
            .args(options.arguments())
            .env("LUMEN_TEST_PRIVATE_ROOT", &private);
        let status = command.status();
        let cleanup = fs::remove_dir_all(&private);
        cleanup?;
        return require(status?.success(), "Notification native fixture failed");
    }
    let mut fixture = Fixture::new(options)?;
    match fixture.options.scenario.as_str() {
        "lock" => lock::run(&mut fixture),
        "recovery" => recovery::run(&mut fixture),
        "focus-presentation" => focus_presentation::run(&mut fixture),
        _ => Err(fail("Unknown scenario")),
    }
}
