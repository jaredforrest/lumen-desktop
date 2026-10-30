use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
pub fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
pub fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
pub fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
pub fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= end {
            return Err(fail(format!("Timed out: {label}: {last}")));
        }
        pause(50);
    }
}
pub fn wait_true(label: &str, mut probe: impl FnMut() -> Result<bool>) -> Result<()> {
    wait(label, || Ok(probe()?.then_some(())))
}
pub struct Options {
    pub scenario: String,
    pub output: PathBuf,
    pub launcher: PathBuf,
    pub sway: PathBuf,
    pub effects: bool,
    pub inside: bool,
}
impl Options {
    pub fn parse() -> Result<Self> {
        let mut scenario = None;
        let mut output = None;
        let mut launcher = None;
        let mut sway = None;
        let mut effects = false;
        let mut inside = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some(
                    "lifecycle"
                    | "session"
                    | "recovery"
                    | "mru"
                    | "window-mru"
                    | "identity"
                    | "identity-absent"
                    | "window-mru-baseline",
                ) => scenario = Some(arg.to_string_lossy().into_owned()),
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--effects") => effects = true,
                Some("--inside") => inside = true,
                _ => return Err(fail(format!("Unknown argument: {}", arg.to_string_lossy()))),
            }
        }
        let output = output.ok_or("--output required")?;
        let output = if output.is_absolute() {
            output
        } else {
            env::current_dir()?.join(output)
        };
        Ok(Self {
            scenario: scenario.ok_or("Scenario required")?,
            output,
            launcher: launcher.ok_or("--launcher required")?.canonicalize()?,
            sway: sway.ok_or("--sway required")?.canonicalize()?,
            effects,
            inside,
        })
    }
}
fn private_path() -> Result<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = PathBuf::from(format!(
        "/tmp/lumen-lifecycle-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}
pub fn private_session() -> Result<()> {
    let private = private_path()?;
    let result = (|| -> Result<()> {
        let config = private.join("bus.conf");
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
                private.display()
            ),
        )?;
        let mut vars: BTreeMap<String, String> = env::vars().collect();
        vars.insert("HOME".into(), private.display().to_string());
        vars.insert("XDG_RUNTIME_DIR".into(), private.display().to_string());
        for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
            vars.insert(
                format!("XDG_{key}_HOME"),
                private.join(key.to_lowercase()).display().to_string(),
            );
        }
        for key in [
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "SWAYSOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
        ] {
            vars.remove(key);
        }
        vars.insert(
            "DBUS_SYSTEM_BUS_ADDRESS".into(),
            format!("unix:path={}/absent-system", private.display()),
        );
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
            format!("Private lifecycle fixture exited {status}"),
        )
    })();
    fs::remove_dir_all(&private)?;
    result
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let kind = fs::symlink_metadata(&from)?.file_type();
        if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_symlink() {
            symlink(fs::read_link(&from)?, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to)?;
            let mut mode = fs::metadata(&to)?.permissions();
            mode.set_mode(mode.mode() | 0o200);
            fs::set_permissions(&to, mode)?;
        } else {
            return Err(fail(format!("Unsupported asset: {}", from.display())));
        }
    }
    Ok(())
}
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    require(output.status.success(), "Launcher environment unavailable")?;
    let mut vars = BTreeMap::new();
    for part in output.stdout.split(|byte| *byte == 0) {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            vars.insert(key.to_owned(), value.to_owned());
        }
    }
    Ok(vars)
}
fn resolve(program: &Path, search: &str) -> Result<PathBuf> {
    if program.components().count() > 1 {
        return Ok(program.to_path_buf());
    }
    for directory in env::split_paths(&OsString::from(search)) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("Command unavailable: {}", program.display())))
}
pub struct Managed(pub Child);
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
    pub fn stop(&mut self) -> Result<()> {
        if self.0.try_wait()?.is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let end = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait()?.is_none() {
            if Instant::now() >= end {
                unsafe {
                    libc::killpg(self.0.id() as i32, libc::SIGKILL);
                }
                self.0.wait()?;
                break;
            }
            pause(25);
        }
        Ok(())
    }
    pub fn alive(&mut self) -> Result<bool> {
        Ok(self.0.try_wait()?.is_none())
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
pub struct Fixture {
    pub options: Options,
    pub vars: BTreeMap<String, String>,
    pub runtime: PathBuf,
    pub assets: PathBuf,
    pub qml: PathBuf,
    pub processes: Vec<Managed>,
}
impl Fixture {
    pub fn new(options: Options) -> Result<Self> {
        fs::create_dir(&options.output)?;
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let mut vars = launcher_environment(&options.launcher)?;
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
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
            ("GDK_BACKEND", "wayland"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
        ] {
            vars.insert(key.into(), value.into());
        }
        for key in [
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "SWAYSOCK",
            "AT_SPI_BUS_ADDRESS",
        ] {
            vars.remove(key);
        }
        let assets = PathBuf::from(
            vars.get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Packaged assets missing")?,
        );
        let qml = if options.scenario != "session" {
            options.output.join("assets/shell")
        } else {
            options.output.join("fixture.qml")
        };
        let mut fixture = Self {
            options,
            vars,
            runtime,
            assets,
            qml,
            processes: Vec::new(),
        };
        if fixture.options.scenario != "session" {
            fixture.prepare_lifecycle()?;
        } else {
            fixture.prepare_session()?;
        }
        fixture.setup_compositor()?;
        Ok(fixture)
    }
    pub fn sibling(&self, name: &str) -> Result<PathBuf> {
        Ok(env::current_exe()?
            .parent()
            .ok_or("Executable parent missing")?
            .join(name))
    }
    pub fn launch(&mut self, program: &Path, args: &[&str], log: &str) -> Result<usize> {
        let program = resolve(program, self.vars.get("PATH").ok_or("PATH missing")?)?;
        self.processes.push(Managed::launch(
            &program,
            args,
            &self.vars,
            &self.options.output.join(log),
        )?);
        Ok(self.processes.len() - 1)
    }
    pub fn stop(&mut self, index: usize) -> Result<()> {
        self.processes[index].stop()
    }
    pub fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let program = resolve(
            Path::new(program),
            self.vars.get("PATH").ok_or("PATH missing")?,
        )?;
        let output = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .output()?;
        require(
            output.status.success(),
            format!(
                "Command failed {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    pub fn ipc(&self, target: &str, method: &str, args: &[&str]) -> Result<String> {
        let mut words = vec![
            "-p",
            self.qml.to_str().ok_or("QML path UTF-8")?,
            "ipc",
            "call",
            target,
            method,
        ];
        words.extend_from_slice(args);
        self.call("quickshell", &words)
    }
    pub fn state(&self, target: &str, artifact: &str) -> Result<Value> {
        let value: Value = serde_json::from_str(&self.ipc(target, "status", &[])?)?;
        save(&self.options.output.join(artifact), &value)?;
        Ok(value)
    }
    pub fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    pub fn nodes(node: &Value, output: &mut Vec<Value>) {
        output.push(node.clone());
        for kind in ["nodes", "floating_nodes"] {
            if let Some(children) = node[kind].as_array() {
                for child in children {
                    Self::nodes(child, output);
                }
            }
        }
    }
    fn setup_compositor(&mut self) -> Result<()> {
        let config = self.options.output.join("sway.conf");
        let size = if self.options.scenario != "session" {
            "1440x1000"
        } else {
            "1280x800"
        };
        fs::write(
            &config,
            format!(
                "output HEADLESS-1 resolution {size}\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\n"
            ),
        )?;
        let sway = self.options.sway.clone();
        let mut arguments = vec!["--unsupported-gpu", "-c", config.to_str().unwrap()];
        if self.options.scenario == "identity" {
            arguments.push("-d");
        }
        self.launch(
            &sway,
            &arguments,
            "sway.log",
        )?;
        let socket = wait("private compositor", || {
            first_file(&self.runtime, "sway-ipc", ".lock")
        })?;
        let display = wait("private Wayland display", || {
            first_file(&self.runtime, "wayland-", ".lock")
        })?;
        self.vars
            .insert("SWAYSOCK".into(), socket.display().to_string());
        self.vars.insert(
            "WAYLAND_DISPLAY".into(),
            display.file_name().unwrap().to_string_lossy().into_owned(),
        );
        Ok(())
    }
    fn prepare_lifecycle(&mut self) -> Result<()> {
        let copied = self.options.output.join("assets");
        copy_tree(&self.assets, &copied)?;
        let shell = copied.join("shell");
        self.call(
            "lumen-fixture-providers",
            &[
                "isolate",
                shell.to_str().unwrap(),
                self.options.output.to_str().unwrap(),
            ],
        )?;
        let qml = shell.join("shell.qml");
        let source = fs::read_to_string(&qml)?;
        let split = source
            .rfind('\n')
            .and_then(|_| source.rfind('}'))
            .ok_or("Shell root missing brace")?;
        let mut transformed = source;
        transformed.insert_str(
            split,
            if self.options.scenario == "recovery" {
                include_str!("../recovery.qml.inc")
            } else if self.options.scenario == "mru" {
                include_str!("../mru.qml.inc")
            } else if self.options.scenario.starts_with("window-mru") {
                include_str!("../window-mru.qml.inc")
            } else {
                include_str!("../lifecycle.qml.inc")
            },
        );
        fs::write(qml, transformed)?;
        let palette = read(&copied.join("appearance/palette.json"))?;
        let appearance = self.options.output.join("appearance.json");
        save(
            &appearance,
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{"mode":"dark","reduceMotion":true}}),
        )?;
        self.vars.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            appearance.display().to_string(),
        );
        self.vars.insert(
            "GLOBAL_MENU_ENABLED".into(),
            if self.options.scenario == "recovery" {
                "1"
            } else {
                "0"
            }
            .into(),
        );
        self.vars.insert("LUMEN_FILE_SERVICE".into(), "0".into());
        let helper = self.options.output.join("helpers");
        fs::create_dir(&helper)?;
        symlink(
            self.sibling("lumen-calendar")?,
            helper.join("lumen-calendar"),
        )?;
        self.prepend_path(&helper)?;
        let applications = PathBuf::from(
            self.vars
                .get("XDG_DATA_HOME")
                .ok_or("Data directory missing")?,
        )
        .join("applications");
        fs::create_dir_all(&applications)?;
        let gapplication = resolve(Path::new("gapplication"), self.vars.get("PATH").unwrap())?;
        fs::write(
            applications.join("org.example.Lifecycle.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Lifecycle Fixture\nExec={} launch org.example.Lifecycle\nIcon=application-x-executable\nStartupWMClass=org.example.Lifecycle\n",
                gapplication.display()
            ),
        )?;
        if self.options.scenario == "mru" || self.options.scenario.starts_with("window-mru") {
            for suffix in ["A", "B", "C"] {
                let id = format!("org.example.Mru{suffix}");
                fs::write(
                    applications.join(format!("{id}.desktop")),
                    format!(
                        "[Desktop Entry]\nType=Application\nName=MRU Fixture {suffix}\nExec={} launch {id}\nIcon=application-x-executable\nStartupWMClass={id}\n",
                        gapplication.display()
                    ),
                )?;
            }
        }
        Ok(())
    }
    fn prepare_session(&mut self) -> Result<()> {
        let helper = self.runtime.join("bin");
        fs::create_dir(&helper)?;
        symlink(self.sibling("systemctl")?, helper.join("systemctl"))?;
        self.prepend_path(&helper)?;
        self.vars.insert(
            "LUMEN_TEST_ACTION_MARKER".into(),
            self.options
                .output
                .join("host-actions")
                .display()
                .to_string(),
        );
        let palette = read(&self.assets.join("appearance/palette.json"))?;
        let appearance = PathBuf::from(
            self.vars
                .get("XDG_STATE_HOME")
                .ok_or("State directory missing")?,
        )
        .join("lumen-desktop/appearance.json");
        fs::create_dir_all(appearance.parent().unwrap())?;
        save(
            &appearance,
            &json!({"font":palette["font"],"palette":palette["light"],"preferences":{}}),
        )?;
        let uri = format!("file://{}", self.assets.join("shell").display());
        fs::write(
            &self.qml,
            include_str!("../session.qml.template").replace("__SHELL_URI__", &uri),
        )?;
        Ok(())
    }
    fn prepend_path(&mut self, directory: &Path) -> Result<()> {
        let prior = self.vars.get("PATH").ok_or("PATH missing")?;
        self.vars
            .insert("PATH".into(), format!("{}:{prior}", directory.display()));
        Ok(())
    }
}
pub fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|item| item.ok().map(|item| item.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && !name.ends_with(suffix)
            })
        }))
}
