//! Private AT-SPI acceptance. QML assets exercise installed controls through the wire protocol.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::{Builder, Runtime};
use zbus::{Connection, Proxy};
mod reader;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn fail(s: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    s.into().into()
}
fn ensure(ok: bool, label: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(label)) }
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn wait<T>(label: &str, mut predicate: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(Some(value)) = predicate() {
            return Ok(value);
        }
        if Instant::now() > end {
            return Err(fail(format!("Timed out {label}")));
        }
        thread::sleep(Duration::from_millis(25));
    }
}
#[derive(Clone)]
struct Opt {
    mode: String,
    atspi: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    output: PathBuf,
    inside: bool,
    extras: BTreeMap<String, String>,
    effects: bool,
}
impl Opt {
    fn parse() -> Result<Self> {
        let mut iter = env::args().skip(1);
        let mode = iter.next().ok_or("mode required")?;
        let mut fields = BTreeMap::new();
        let mut inside = false;
        let mut effects = false;
        while let Some(key) = iter.next() {
            if key == "--inside" {
                inside = true
            } else if key == "--effects" {
                effects = true
            } else {
                fields.insert(key, iter.next().ok_or("missing argument value")?);
            }
        }
        let take = |key: &str| -> Result<PathBuf> {
            Ok(PathBuf::from(
                fields.get(key).ok_or(format!("{key} required"))?,
            ))
        };
        Ok(Self {
            mode,
            atspi: take("--atspi")?,
            launcher: take("--launcher")?,
            sway: take("--sway")?,
            output: take("--output")?,
            inside,
            extras: fields,
            effects,
        })
    }
    fn args(&self) -> Vec<String> {
        let mut args = vec![
            self.mode.clone(),
            "--atspi".into(),
            self.atspi.display().to_string(),
            "--launcher".into(),
            self.launcher.display().to_string(),
            "--sway".into(),
            self.sway.display().to_string(),
            "--output".into(),
            self.output.display().to_string(),
            "--inside".into(),
        ];
        for (key, value) in &self.extras {
            args.extend([key.clone(), value.clone()]);
        }
        if self.effects {
            args.push("--effects".into());
        }
        args
    }
    fn extra(&self, key: &str) -> Result<PathBuf> {
        Ok(PathBuf::from(
            self.extras.get(key).ok_or(format!("{key} required"))?,
        ))
    }
}
struct ChildProcess(Child);
impl Drop for ChildProcess {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let _ = self.0.wait();
    }
}
struct Harness {
    opt: Opt,
    env: BTreeMap<String, String>,
    runtime: PathBuf,
    processes: Vec<ChildProcess>,
    a11y: Option<Connection>,
    rt: Runtime,
}
impl Harness {
    fn new(opt: Opt) -> Result<Self> {
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let env = env::vars().collect();
        Ok(Self {
            opt,
            env,
            runtime,
            processes: Vec::new(),
            a11y: None,
            rt: Builder::new_current_thread().enable_all().build()?,
        })
    }
    fn launcher_env(&mut self) -> Result<()> {
        let output = Command::new(&self.opt.launcher)
            .arg("--print-env0")
            .env_clear()
            .envs(&self.env)
            .output()?;
        ensure(output.status.success(), "Launcher environment")?;
        for part in output.stdout.split(|byte| *byte == 0) {
            if let Some(i) = part.iter().position(|byte| *byte == b'=') {
                self.env.insert(
                    String::from_utf8(part[..i].to_vec())?,
                    String::from_utf8(part[i + 1..].to_vec())?,
                );
            }
        }
        Ok(())
    }
    fn launch(&mut self, args: &[String], name: &str) -> Result<u32> {
        let log = File::create(self.opt.output.join(name))?;
        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..])
            .env_clear()
            .envs(&self.env)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log));
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let child = cmd.spawn()?;
        let pid = child.id();
        self.processes.push(ChildProcess(child));
        Ok(pid)
    }
    fn call(&self, args: &[String]) -> Result<String> {
        let output = Command::new(&args[0])
            .args(&args[1..])
            .env_clear()
            .envs(&self.env)
            .output()?;
        ensure(
            output.status.success(),
            &format!(
                "Command {}: {}",
                args[0],
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }
    fn connection(&self, address: &str) -> Result<Connection> {
        let builder = zbus::connection::Builder::address(address)?;
        Ok(self.rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), builder.build()).await
        })??)
    }
    fn name_owner(&self, connection: &Connection, name: &str) -> Result<bool> {
        Ok(self.rt.block_on(async {
            let proxy = Proxy::new(
                connection,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
            )
            .await?;
            proxy.call("NameHasOwner", &(name,)).await
        })?)
    }
    fn start_a11y(&mut self) -> Result<()> {
        let session = self
            .env
            .get("DBUS_SESSION_BUS_ADDRESS")
            .ok_or("session bus")?
            .clone();
        let bus = self.connection(&session)?;
        let atspi = self.opt.atspi.clone();
        self.launch(
            &[
                atspi
                    .join("libexec/at-spi-bus-launcher")
                    .display()
                    .to_string(),
                "--launch-immediately".into(),
                "--a11y=1".into(),
                "--screen-reader=1".into(),
            ],
            "a11y-bus.log",
        )?;
        wait("private accessibility bus", || {
            Ok(self.name_owner(&bus, "org.a11y.Bus")?.then_some(()))
        })?;
        let address: String = self.rt.block_on(async {
            let proxy = Proxy::new(&bus, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus").await?;
            proxy.call("GetAddress", &()).await
        })?;
        ensure(
            address.starts_with(&format!("unix:path={}/", self.runtime.display())),
            "AT-SPI bus escaped private runtime",
        )?;
        self.env
            .insert("AT_SPI_BUS_ADDRESS".into(), address.clone());
        self.launch(
            &[atspi
                .join("libexec/at-spi2-registryd")
                .display()
                .to_string()],
            "registry.log",
        )?;
        let a11y = self.connection(&address)?;
        wait("private accessibility registry", || {
            Ok(self
                .name_owner(&a11y, "org.a11y.atspi.Registry")?
                .then_some(()))
        })?;
        self.a11y = Some(a11y);
        Ok(())
    }
    fn start_sway(&mut self, offscreen: bool) -> Result<()> {
        fs::write(
            self.opt.output.join("sway.conf"),
            "output HEADLESS-1 resolution 800x600\nseat seat0 fallback true\n",
        )?;
        for (key, val) in [
            (
                "QT_QPA_PLATFORM",
                if offscreen { "offscreen" } else { "wayland" },
            ),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_ACCESSIBILITY", "1"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1"),
            ("WLR_BACKENDS", "headless"),
            ("WLR_RENDERER", "pixman"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ] {
            self.env.insert(key.into(), val.into());
        }
        for key in [
            "NO_AT_BRIDGE",
            "GTK_A11Y",
            "SWAYSOCK",
            "WAYLAND_DISPLAY",
            "DISPLAY",
        ] {
            self.env.remove(key);
        }
        self.launch(
            &[
                self.opt.sway.display().to_string(),
                "--unsupported-gpu".into(),
                "-c".into(),
                self.opt.output.join("sway.conf").display().to_string(),
            ],
            "sway.log",
        )?;
        let runtime = self.runtime.clone();
        let socket = wait("private compositor", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name()
                        .is_some_and(|v| v.to_string_lossy().starts_with("sway-ipc."))
                }))
        })?;
        self.env
            .insert("SWAYSOCK".into(), socket.display().to_string());
        let display = wait("private display", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .find(|name| name.starts_with("wayland-") && !name.ends_with(".lock")))
        })?;
        self.env.insert("WAYLAND_DISPLAY".into(), display);
        self.env.insert("QT_QPA_PLATFORM".into(), "wayland".into());
        Ok(())
    }
    fn qml(&self, template: &str, source: &Path) -> Result<PathBuf> {
        let path = self.opt.output.join("shell.qml");
        fs::write(
            &path,
            template.replace("@@SOURCE@@", &source.display().to_string()),
        )?;
        Ok(path)
    }
    fn ipc(&self, fixture: &Path, target: &str, method: &str) -> Result<String> {
        self.call(&[
            "quickshell".into(),
            "-p".into(),
            fixture.display().to_string(),
            "ipc".into(),
            "call".into(),
            target.into(),
            method.into(),
        ])
    }
    fn action_metadata(&mut self) -> Result<()> {
        self.launcher_env()?;
        self.start_a11y()?;
        let source = PathBuf::from(
            self.env
                .get("LUMEN_DESKTOP_ASSETS")
                .ok_or("desktop assets")?,
        )
        .join("shell");
        let palette: Value = serde_json::from_slice(&fs::read(
            source
                .parent()
                .ok_or("assets")?
                .join("appearance/palette.json"),
        )?)?;
        let appearance = Path::new(self.env.get("XDG_STATE_HOME").ok_or("state home")?)
            .join("lumen-desktop/appearance.json");
        fs::create_dir_all(appearance.parent().ok_or("appearance parent")?)?;
        save(
            &appearance,
            &json!({"font":palette["font"],"palette":palette["light"],"preferences":{}}),
        )?;
        self.start_sway(false)?;
        let fixture = self.qml(include_str!("../action.qml"), &source)?;
        self.launch(
            &[
                "quickshell".into(),
                "-p".into(),
                fixture.display().to_string(),
            ],
            "shell.log",
        )?;
        let a11y = self.a11y.as_ref().ok_or("AT-SPI")?.clone();
        let target = wait("private production action", || {
            self.find_named(&a11y, "Action sentinel")
        })?;
        let name: String = self.rt.block_on(async {
            let proxy = Proxy::new(
                &a11y,
                target.0.as_str(),
                target.1.as_str(),
                "org.a11y.atspi.Action",
            )
            .await?;
            proxy.call("GetName", &(0_i32,)).await
        })?;
        let description: String = self.rt.block_on(async {
            let proxy = Proxy::new(
                &a11y,
                target.0.as_str(),
                target.1.as_str(),
                "org.a11y.atspi.Action",
            )
            .await?;
            proxy.call("GetDescription", &(0_i32,)).await
        })?;
        ensure(
            ["press", "click"].contains(&name.to_lowercase().as_str()),
            "Canonical Action name",
        )?;
        let activated: bool = self.rt.block_on(async {
            let proxy = Proxy::new(
                &a11y,
                target.0.as_str(),
                target.1.as_str(),
                "org.a11y.atspi.Action",
            )
            .await?;
            proxy.call("DoAction", &(0_i32,)).await
        })?;
        ensure(activated, "Canonical Action refused")?;
        wait("exact private action once", || {
            Ok((self.ipc(&fixture, "metadata", "count")? == "1").then_some(()))
        })?;
        let proof: Value = serde_json::from_slice(&fs::read(
            self.opt.atspi.join("share/action-alias-proof.json"),
        )?)?;
        save(
            &self.opt.output.join("result.json"),
            &json!({"passed":true,"name":name,"description":description,"proof":proof,"checks":["canonical actual Action name and description","actual production button action once"]}),
        )?;
        Ok(())
    }
    fn find_named(&self, a11y: &Connection, name: &str) -> Result<Option<(String, String)>> {
        let mut pending = std::collections::VecDeque::from([(
            "org.a11y.atspi.Registry".to_string(),
            "/org/a11y/atspi/accessible/root".to_string(),
        )]);
        let mut seen = std::collections::HashSet::new();
        while let Some(reference) = pending.pop_front() {
            if !seen.insert(reference.clone()) || reference.1 == "/org/a11y/atspi/null" {
                continue;
            }
            if seen.len() > 1000 {
                return Err(fail("AT-SPI tree bound"));
            }
            let found = self.rt.block_on(async {
                let proxy = Proxy::new(
                    a11y,
                    reference.0.as_str(),
                    reference.1.as_str(),
                    "org.freedesktop.DBus.Properties",
                )
                .await?;
                let value: zbus::zvariant::OwnedValue = proxy
                    .call("Get", &("org.a11y.atspi.Accessible", "Name"))
                    .await?;
                zbus::Result::Ok(
                    String::try_from(value)
                        .map_err(|error| zbus::Error::Failure(error.to_string()))?,
                )
            });
            if found.ok().as_deref() == Some(name) {
                return Ok(Some(reference));
            }
            let children: Result<Vec<(String, zbus::zvariant::OwnedObjectPath)>> =
                self.rt.block_on(async {
                    let proxy = Proxy::new(
                        a11y,
                        reference.0.as_str(),
                        reference.1.as_str(),
                        "org.a11y.atspi.Accessible",
                    )
                    .await?;
                    Ok(proxy.call("GetChildren", &()).await?)
                });
            if let Ok(children) = children {
                pending.extend(
                    children
                        .into_iter()
                        .map(|(owner, path)| (owner, path.to_string())),
                );
            }
        }
        Ok(None)
    }
}
fn outer(opt: Opt) -> Result<i32> {
    let live = Path::new(&format!("/run/user/{}/at-spi/bus_0", unsafe {
        libc::getuid()
    }))
    .to_path_buf();
    let live_before = fs::metadata(&live)
        .ok()
        .map(|value| std::os::unix::fs::MetadataExt::ino(&value));
    fs::create_dir(&opt.output)?;
    let private = tempfile::tempdir()?;
    let root = private.path();
    let bus = root.join("bus.conf");
    fs::write(
        &bus,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            root.display()
        ),
    )?;
    let mut cmd = if opt.mode == "screen-reader-input" {
        let mut command = Command::new(opt.extra("--isolation-tool")?);
        command.args([
            "--user",
            "--map-current-user",
            "--pid",
            "--fork",
            "--mount-proc",
            "--kill-child",
            "--net",
            "dbus-run-session",
        ]);
        command
    } else {
        Command::new("dbus-run-session")
    };
    cmd.arg(format!("--config-file={}", bus.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .args(opt.args());
    let mut environment = env::vars().collect::<BTreeMap<_, _>>();
    environment.insert("HOME".into(), root.display().to_string());
    environment.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    environment.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/absent-system", root.display()),
    );
    for key in ["CONFIG", "STATE", "DATA", "CACHE"] {
        let value = root.join(key.to_lowercase());
        fs::create_dir_all(&value)?;
        environment.insert(format!("XDG_{key}_HOME"), value.display().to_string());
    }
    for key in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "AT_SPI_BUS_ADDRESS",
        "NO_AT_BRIDGE",
        "GTK_A11Y",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        environment.remove(key);
    }
    let status = cmd.env_clear().envs(environment).status()?;
    let live_after = fs::metadata(&live)
        .ok()
        .map(|value| std::os::unix::fs::MetadataExt::ino(&value));
    ensure(
        live_before == live_after,
        "Live accessibility socket changed",
    )?;
    Ok(status.code().unwrap_or(1))
}
fn run() -> Result<i32> {
    let opt = Opt::parse()?;
    if !opt.inside {
        return outer(opt);
    }
    let mut h = Harness::new(opt)?;
    match h.opt.mode.as_str() {
        "action-metadata" => h.action_metadata()?,
        "screen-reader-input" => h.screen_reader_input()?,
        other => return Err(fail(format!("Unimplemented accessibility mode: {other}"))),
    }
    Ok(0)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
