use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
pub fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
pub fn pause(milliseconds: u64) {
    thread::sleep(Duration::from_millis(milliseconds));
}
pub fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
pub fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Timed out: {label}: {last}")));
        }
        pause(50);
    }
}
pub fn wait_true(label: &str, mut probe: impl FnMut() -> Result<bool>) -> Result<()> {
    wait(label, || Ok(probe()?.then_some(())))
}
fn resolve(program: &Path, path: &str) -> Result<PathBuf> {
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
            let mut permissions = fs::metadata(&to)?.permissions();
            permissions.set_mode(permissions.mode() | 0o200);
            fs::set_permissions(&to, permissions)?;
        } else {
            return Err(fail(format!("Unsupported asset: {}", from.display())));
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
            vars.insert(key.to_owned(), value.to_owned());
        }
    }
    Ok(vars)
}
pub struct Options {
    pub scenario: String,
    pub output: PathBuf,
    pub launcher: PathBuf,
    pub sway: PathBuf,
    pub pointer_tool: PathBuf,
    pub atspi: PathBuf,
    pub exporter: Option<PathBuf>,
    pub effects: bool,
    pub inside: bool,
}
impl Options {
    pub fn parse() -> Result<Self> {
        let mut scenario = None;
        let mut output = None;
        let mut launcher = None;
        let mut sway = None;
        let mut pointer_tool = None;
        let mut atspi = None;
        let mut exporter = None;
        let mut effects = false;
        let mut inside = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("menubar" | "keyboard") => scenario = Some(arg.to_string_lossy().into_owned()),
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--pointer-tool") => {
                    pointer_tool = Some(PathBuf::from(args.next().ok_or("Missing pointer tool")?))
                }
                Some("--at-spi-prefix") => {
                    atspi = Some(PathBuf::from(args.next().ok_or("Missing AT-SPI prefix")?))
                }
                Some("--exporter") => {
                    exporter = Some(PathBuf::from(args.next().ok_or("Missing exporter")?))
                }
                Some("--effects") => effects = true,
                Some("--inside") => inside = true,
                _ => return Err(fail(format!("Unknown argument: {}", arg.to_string_lossy()))),
            }
        }
        let output = output.ok_or("--output is required")?;
        let output = if output.is_absolute() {
            output
        } else {
            env::current_dir()?.join(output)
        };
        Ok(Self {
            scenario: scenario.ok_or("Menu scenario is required")?,
            output,
            launcher: launcher.ok_or("--launcher is required")?.canonicalize()?,
            sway: sway.ok_or("--sway is required")?.canonicalize()?,
            pointer_tool: pointer_tool
                .ok_or("--pointer-tool is required")?
                .canonicalize()?,
            atspi: atspi.ok_or("--at-spi-prefix is required")?.canonicalize()?,
            exporter: exporter.map(|path| path.canonicalize()).transpose()?,
            effects,
            inside,
        })
    }
}
fn private_path() -> Result<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = PathBuf::from(format!("/tmp/lumen-menu-{}-{nonce}", std::process::id()));
    fs::create_dir(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}
fn socket_inode() -> Option<u64> {
    let path = format!("/run/user/{}/at-spi/bus_0", unsafe { libc::getuid() });
    fs::metadata(path).ok().map(|data| data.ino())
}
pub fn private_session(options: &Options) -> Result<()> {
    let before = socket_inode();
    let private = private_path()?;
    let result = (|| -> Result<()> {
        fs::write(
            private.join("bus.conf"),
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
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
            "AT_SPI_BUS_ADDRESS",
            "NO_AT_BRIDGE",
            "GTK_A11Y",
            "DBUS_SESSION_BUS_ADDRESS",
            "SWAYSOCK",
            "WAYLAND_DISPLAY",
            "DISPLAY",
        ] {
            vars.remove(key);
        }
        vars.insert("GSETTINGS_BACKEND".into(), "memory".into());
        vars.insert("QT_ACCESSIBILITY".into(), "1".into());
        vars.insert("QT_LINUX_ACCESSIBILITY_ALWAYS_ON".into(), "1".into());
        vars.insert(
            "DBUS_SYSTEM_BUS_ADDRESS".into(),
            format!("unix:path={}/no-system-bus", private.display()),
        );
        let status = Command::new("dbus-run-session")
            .arg(format!(
                "--config-file={}",
                private.join("bus.conf").display()
            ))
            .arg("--")
            .arg(env::current_exe()?)
            .args(env::args_os().skip(1))
            .arg("--inside")
            .env_clear()
            .envs(&vars)
            .status()?;
        require(
            status.success(),
            format!("Private menu fixture exited {status}"),
        )?;
        require(
            before == socket_inode(),
            "Live accessibility socket changed",
        )?;
        let result = options.output.join("result.json");
        let mut value = read(&result)?;
        value["liveSocketUnchanged"] = json!(true);
        save(&result, &value)
    })();
    let cleanup = fs::remove_dir_all(&private);
    cleanup?;
    result
}
pub fn write_result(output: &Path, checks: Vec<&str>, mut extras: Value) -> Result<()> {
    extras["passed"] = json!(true);
    extras["checks"] = json!(checks);
    save(&output.join("result.json"), &extras)
}

pub struct Managed(pub Child);
impl Managed {
    fn launch(
        program: &Path,
        args: &[&str],
        vars: &BTreeMap<String, String>,
        log: &Path,
        pipe: bool,
    ) -> Result<Self> {
        let file = File::create(log)?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(vars)
            .stdin(if pipe { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file))
            .process_group(0)
            .spawn()?;
        Ok(Self(child))
    }
    pub fn stop(&mut self) -> Result<()> {
        if self.0.try_wait()?.is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait()?.is_none() {
            if Instant::now() >= deadline {
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
    pub fn pid(&self) -> u32 {
        self.0.id()
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Reference(pub String, pub OwnedObjectPath);
#[derive(Clone)]
pub struct Row {
    pub reference: Reference,
    pub name: String,
    pub role: String,
    pub showing: bool,
    pub enabled: bool,
    pub focused: bool,
    pub checked: bool,
}
impl Row {
    pub fn json(&self) -> Value {
        json!({"name": self.name, "role": self.role, "showing":self.showing,"enabled":self.enabled,"focused":self.focused,"checked":self.checked})
    }
}
pub struct Fixture {
    pub options: Options,
    pub vars: BTreeMap<String, String>,
    pub runtime: PathBuf,
    pub shell: PathBuf,
    pub session: Connection,
    pub a11y: Connection,
    pub processes: Vec<Managed>,
    pub pointer: usize,
}
impl Fixture {
    pub fn new(options: Options) -> Result<Self> {
        fs::create_dir(&options.output)?;
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let mut vars = launcher_environment(&options.launcher)?;
        for (key, value) in [
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_ACCESSIBILITY", "1"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1"),
            ("GSETTINGS_BACKEND", "memory"),
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
            ("GLOBAL_MENU_ENABLED", "1"),
            ("LUMEN_FILE_SERVICE", "0"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
        ] {
            vars.insert(key.into(), value.into());
        }
        for key in [
            "NO_AT_BRIDGE",
            "GTK_A11Y",
            "AT_SPI_BUS_ADDRESS",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "DISPLAY",
        ] {
            vars.remove(key);
        }
        let source = PathBuf::from(
            vars.get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Packaged assets missing")?,
        );
        let assets = options.output.join("assets");
        copy_tree(&source, &assets)?;
        let shell = assets.join("shell");
        let mut fixture = Self {
            options,
            vars,
            runtime,
            shell,
            session: Connection::session()?,
            a11y: Connection::session()?,
            processes: Vec::new(),
            pointer: 0,
        };
        fixture.call(
            "lumen-fixture-providers",
            &[
                "isolate",
                &fixture.shell.display().to_string(),
                &fixture.options.output.display().to_string(),
            ],
        )?;
        fixture.instrument()?;
        let palette = read(&assets.join("appearance/palette.json"))?;
        let appearance = PathBuf::from(
            fixture
                .vars
                .get("XDG_STATE_HOME")
                .ok_or("State directory missing")?,
        )
        .join("lumen-desktop/appearance.json");
        fs::create_dir_all(appearance.parent().ok_or("Appearance parent missing")?)?;
        save(
            &appearance,
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}),
        )?;
        let atspi = fixture.options.atspi.clone();
        fixture.launch(
            &atspi.join("libexec/at-spi-bus-launcher"),
            &["--launch-immediately", "--a11y=1", "--screen-reader=1"],
            "bus.log",
            false,
        )?;
        wait_true("private accessibility bus", || {
            name_owned(&fixture.session, "org.a11y.Bus")
        })?;
        let service = Proxy::new(
            &fixture.session,
            "org.a11y.Bus",
            "/org/a11y/bus",
            "org.a11y.Bus",
        )?;
        let address: String = service.call("GetAddress", &())?;
        require(
            address.starts_with(&format!("unix:path={}/", fixture.runtime.display())),
            "Accessibility bus escaped private runtime",
        )?;
        fixture
            .vars
            .insert("AT_SPI_BUS_ADDRESS".into(), address.clone());
        fixture.a11y = zbus::blocking::connection::Builder::address(address.as_str())?.build()?;
        fixture.launch(
            &atspi.join("libexec/at-spi2-registryd"),
            &[],
            "registry.log",
            false,
        )?;
        wait_true("private AT-SPI registry", || {
            name_owned(&fixture.a11y, "org.a11y.atspi.Registry")
        })?;
        let config = fixture.options.output.join("sway.conf");
        fs::write(
            &config,
            format!(
                "output HEADLESS-1 resolution 1280x1000\nseat seat0 fallback true\nxwayland {}\nfocus_follows_mouse no\nfor_window [all] floating enable\n",
                if fixture.options.scenario == "keyboard" {
                    "enable"
                } else {
                    "disable"
                }
            ),
        )?;
        let sway = fixture.options.sway.clone();
        fixture.launch(
            &sway,
            &["--unsupported-gpu", "-c", &config.display().to_string()],
            "sway.log",
            false,
        )?;
        let socket = wait("private compositor", || {
            first_file(&fixture.runtime, "sway-ipc", ".lock")
        })?;
        let display = wait("private display", || {
            first_file(&fixture.runtime, "wayland-", ".lock")
        })?;
        fixture
            .vars
            .insert("SWAYSOCK".into(), socket.display().to_string());
        fixture.vars.insert(
            "WAYLAND_DISPLAY".into(),
            display.file_name().unwrap().to_string_lossy().into_owned(),
        );
        fixture.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log", false)?;
        let pointer = fixture.options.pointer_tool.clone();
        fixture.pointer = fixture.launch(
            &pointer,
            &["pointer", "move", "0", "0"],
            "pointer.log",
            true,
        )?;
        fixture.launch(
            Path::new("quickshell"),
            &["-p", &fixture.shell.display().to_string()],
            "shell.log",
            false,
        )?;
        wait_true("owned MenuBar", || Ok(fixture.state()?["ready"] == true))?;
        Ok(fixture)
    }
    pub fn call(&self, command: &str, args: &[&str]) -> Result<String> {
        let program = resolve(
            Path::new(command),
            self.vars.get("PATH").ok_or("PATH missing")?,
        )?;
        let result = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.vars)
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
    pub fn launch(
        &mut self,
        program: &Path,
        args: &[&str],
        log: &str,
        pipe: bool,
    ) -> Result<usize> {
        let program = resolve(program, self.vars.get("PATH").ok_or("PATH missing")?)?;
        self.processes.push(Managed::launch(
            &program,
            args,
            &self.vars,
            &self.options.output.join(log),
            pipe,
        )?);
        Ok(self.processes.len() - 1)
    }
    pub fn stop(&mut self, index: usize) -> Result<()> {
        self.processes[index].stop()
    }
    pub fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut words = vec![
            "-p",
            self.shell.to_str().ok_or("Shell path UTF-8")?,
            "ipc",
            "call",
            "menuAudit",
            method,
        ];
        words.extend_from_slice(args);
        self.call("quickshell", &words)
    }
    pub fn state(&self) -> Result<Value> {
        let value: Value = serde_json::from_str(&self.ipc("status", &[])?)?;
        save(&self.options.output.join("last-state.json"), &value)?;
        Ok(value)
    }
    pub fn files_state(&self) -> Result<Value> {
        let value: Value = serde_json::from_str(&self.ipc("filesStatus", &[])?)?;
        save(&self.options.output.join("last-files-state.json"), &value)?;
        Ok(value)
    }
    pub fn key(&self, keys: &[&str]) -> Result<()> {
        self.call("wtype", keys)?;
        pause(120);
        Ok(())
    }
    pub fn pointer_command(&mut self, value: &str) -> Result<()> {
        let stdin = self.processes[self.pointer]
            .0
            .stdin
            .as_mut()
            .ok_or("Pointer stdin missing")?;
        writeln!(stdin, "{value}")?;
        stdin.flush()?;
        pause(120);
        Ok(())
    }
    pub fn point(&mut self, reference: &Reference) -> Result<()> {
        let component = Proxy::new(
            &self.a11y,
            reference.0.as_str(),
            reference.1.as_str(),
            "org.a11y.atspi.Component",
        )?;
        let (x, y, width, height): (i32, i32, i32, i32) =
            component.call("GetExtents", &(0_i32,))?;
        require(width > 0 && height > 0, "AT-SPI row has no extent")?;
        self.call(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &(x + width / 2).to_string(),
                &(y + height / 2).to_string(),
            ],
        )?;
        self.pointer_command("m 0.5 0.5")
    }
    pub fn named(&self, name: &str) -> Result<Option<Row>> {
        let mut pending = VecDeque::from([Reference(
            "org.a11y.atspi.Registry".into(),
            OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root")?,
        )]);
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        let mut found = None;
        while let Some(reference) = pending.pop_front() {
            if rows.len() >= 1500 {
                break;
            }
            if reference.1.as_str() == "/org/a11y/atspi/null" || !seen.insert(reference.clone()) {
                continue;
            }
            let item = match Proxy::new(
                &self.a11y,
                reference.0.as_str(),
                reference.1.as_str(),
                "org.a11y.atspi.Accessible",
            ) {
                Ok(item) => item,
                Err(_) => continue,
            };
            let result = (|| -> Result<(Row, Vec<(String, OwnedObjectPath)>)> {
                let properties = Proxy::new(
                    &self.a11y,
                    reference.0.as_str(),
                    reference.1.as_str(),
                    "org.freedesktop.DBus.Properties",
                )?;
                let label: OwnedValue =
                    properties.call("Get", &("org.a11y.atspi.Accessible", "Name"))?;
                let label = String::try_from(label)?;
                let role: String = item.call("GetRoleName", &())?;
                let states: Vec<u32> = item.call("GetState", &())?;
                let has = |index: usize| {
                    states
                        .get(index / 32)
                        .is_some_and(|word| *word & (1 << (index % 32)) != 0)
                };
                let row = Row {
                    reference: reference.clone(),
                    name: label,
                    role,
                    showing: has(25),
                    enabled: has(8),
                    focused: has(12),
                    checked: has(4),
                };
                let children = item.call("GetChildren", &())?;
                Ok((row, children))
            })();
            if let Ok((row, children)) = result {
                if row.name == name && row.showing {
                    found = Some(row.clone());
                    rows.push(row.json());
                    break;
                }
                rows.push(row.json());
                pending.extend(
                    children
                        .into_iter()
                        .map(|(name, path)| Reference(name, path)),
                );
            }
        }
        save(&self.options.output.join("last-tree.json"), &json!(rows))?;
        Ok(found)
    }
    pub fn action(&self, reference: &Reference) -> Result<bool> {
        let proxy = Proxy::new(
            &self.a11y,
            reference.0.as_str(),
            reference.1.as_str(),
            "org.a11y.atspi.Action",
        )?;
        Ok(proxy.call("DoAction", &(0_i32,))?)
    }
    pub fn focus(&self, reference: &Reference) -> Result<bool> {
        let proxy = Proxy::new(
            &self.a11y,
            reference.0.as_str(),
            reference.1.as_str(),
            "org.a11y.atspi.Component",
        )?;
        Ok(proxy.call("GrabFocus", &())?)
    }
    pub fn open_menu(&self, name: &str, app: &str) -> Result<()> {
        self.call("swaymsg", &[&format!("[app_id=\"{app}\"]"), "focus"])?;
        wait_true("native focus and installed bridge watcher", || {
            let state = self.state()?;
            Ok(state["active"] == app
                && state["filesContext"] == false
                && state["bridge"]["appName"] == app
                && state["bridge"]["windowId"].as_u64().unwrap_or(0) > 0)
        })?;
        self.ipc("open", &[name])?;
        wait_true("menu open", || Ok(self.state()?["menu"] == name))?;
        self.ipc("clear", &[])?;
        Ok(())
    }
    pub fn client(&mut self, app: &str) -> Result<usize> {
        let executable = env::current_exe()?;
        let capture = self.options.output.join(format!("{app}-input.txt"));
        let index = self.launch(
            Path::new("foot"),
            &[
                &format!("--app-id={app}"),
                &format!("--title={app}"),
                "-e",
                executable.to_str().unwrap(),
                "capture-input",
                capture.to_str().unwrap(),
            ],
            &format!("{app}.log"),
            false,
        )?;
        wait_true(app, || {
            Ok(self.state()?["windows"]
                .as_array()
                .is_some_and(|windows| windows.iter().any(|window| window["app"] == app)))
        })?;
        Ok(index)
    }
    fn instrument(&self) -> Result<()> {
        let bridge = self.shell.join("GlobalMenu.qml");
        replace_once(
            &bridge,
            "id: bridge",
            "id: bridge\n        stderr:SplitParser {onRead:line=>console.warn(\"Bridge fixture: \",line)}",
        )?;
        let scene = self.shell.join("shell.qml");
        let mut content = fs::read_to_string(&scene)?;
        content = replace_text(
            &content,
            "SettingsState {id:settingsState;active:settingsWindow.visible}",
            "SettingsState {id:settingsState;active:false}",
        )?;
        if self.options.scenario == "menubar" {
            content = replace_text(
                &content,
                "function sessionAction(action){claimPopup(\"\");sessionOptions.show(action);focusOwnedWindow(\"Session Options\");}",
                "function sessionAction(action){menuFixtureSessions=menuFixtureSessions.concat([action]);}",
            )?;
            content = replace_text(
                &content,
                "function filesAction(action){",
                "function filesAction(action){menuFixtureFileActions=menuFixtureFileActions.concat([{action:action,target:root.filesWindow===root.menuFixtureFirstFiles?\"first\":\"second\"}]);",
            )?;
            content = replace_text(
                &content,
                "function focusWindow(window) {",
                "function focusWindow(window) {\n        if(window)menuFixtureFocus=menuFixtureFocus.concat([window.appId]);",
            )?;
        } else {
            content = replace_text(
                &content,
                "function sessionAction(action){claimPopup(\"\");sessionOptions.show(action);focusOwnedWindow(\"Session Options\");}",
                "function sessionAction(action){throw Error(\"Unexpected session command in keyboard fixture\");}",
            )?;
        }
        let split = content
            .rfind('}')
            .ok_or("Shell root missing closing brace")?;
        let fragment = if self.options.scenario == "menubar" {
            include_str!("../menubar.qml.inc")
        } else {
            include_str!("../keyboard.qml.inc")
        };
        content.insert_str(split, fragment);
        fs::write(scene, content)?;
        Ok(())
    }
}
pub fn name_owned(connection: &Connection, name: &str) -> Result<bool> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    Ok(proxy.call("NameHasOwner", &(name,))?)
}
pub fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && !name.ends_with(suffix)
            })
        }))
}
fn replace_text(source: &str, original: &str, replacement: &str) -> Result<String> {
    require(
        source.matches(original).count() == 1,
        format!("Fixture instrumentation boundary changed: {original}"),
    )?;
    Ok(source.replacen(original, replacement, 1))
}
fn replace_once(path: &Path, original: &str, replacement: &str) -> Result<()> {
    let source = fs::read_to_string(path)?;
    fs::write(path, replace_text(&source, original, replacement)?)?;
    Ok(())
}
