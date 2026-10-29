//! Native two-output widget catalogue and crash recovery acceptance.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
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
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn wait_true(label: &str, mut probe: impl FnMut() -> Result<bool>) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if probe().unwrap_or(false) {
            return Ok(());
        }
        if Instant::now() >= end {
            return Err(fail(format!("Timed out: {label}")));
        }
        pause(50);
    }
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for row in fs::read_dir(source)? {
        let entry = row?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &destination)?;
        } else if path.is_file() {
            fs::copy(path, destination)?;
        }
    }
    Ok(())
}
struct Options {
    output: PathBuf,
    source: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    theme: String,
    effects: bool,
    inside: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut args = env::args_os().skip(1);
        let mut output = None;
        let mut source = None;
        let mut launcher = None;
        let mut sway = None;
        let mut pointer = None;
        let mut theme = "dark".to_owned();
        let mut effects = false;
        let mut inside = false;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--source") => {
                    source = Some(PathBuf::from(args.next().ok_or("Missing source")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--theme") => {
                    theme = args
                        .next()
                        .ok_or("Missing theme")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--effects") => effects = true,
                Some("--inside") => inside = true,
                _ => return Err(fail("Unknown widget catalogue option")),
            }
        }
        require(theme == "light" || theme == "dark", "Unknown widget theme")?;
        Ok(Self {
            output: env::current_dir()?.join(output.ok_or("--output required")?),
            source: source.unwrap_or(env::current_dir()?).canonicalize()?,
            launcher: launcher.ok_or("--launcher required")?.canonicalize()?,
            sway: sway.ok_or("--sway required")?.canonicalize()?,
            pointer: pointer.ok_or("--pointer-tool required")?.canonicalize()?,
            theme,
            effects,
            inside,
        })
    }
}
fn private_session() -> Result<()> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = PathBuf::from(format!("/tmp/lwc-{}-{nanos}", std::process::id()));
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let bus_config = root.join("bus.conf");
    fs::write(
        &bus_config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            root.display()
        ),
    )?;
    let mut cmd = Command::new("dbus-run-session");
    cmd.arg(format!("--config-file={}", bus_config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .args(env::args_os().skip(1))
        .arg("--inside")
        .env("HOME", &root)
        .env("XDG_RUNTIME_DIR", &root)
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}/absent-system", root.display()),
        );
    for (key, dir) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
    ] {
        let path = root.join(dir);
        fs::create_dir(&path)?;
        cmd.env(key, path);
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        cmd.env_remove(key);
    }
    let status = cmd.status();
    let _ = fs::remove_dir_all(&root);
    require(status?.success(), "Private widget fixture failed")
}
struct Managed(Child);
impl Managed {
    fn stop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let end = Instant::now() + Duration::from_secs(4);
        while self.0.try_wait().ok().flatten().is_none() {
            if Instant::now() >= end {
                unsafe {
                    libc::killpg(self.0.id() as i32, libc::SIGKILL);
                }
                break;
            }
            pause(25);
        }
        let _ = self.0.wait();
    }
    fn kill(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGKILL);
            }
        }
        let _ = self.0.wait();
    }
}
struct Fixture {
    options: Options,
    env: BTreeMap<String, String>,
    runtime: PathBuf,
    qml: PathBuf,
    processes: Vec<Managed>,
    pointer: Option<usize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for child in self.processes.iter_mut().rev() {
            child.stop();
        }
    }
}
impl Fixture {
    fn new(options: Options) -> Result<Self> {
        fs::create_dir(&options.output)?;
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let raw = Command::new(&options.launcher)
            .arg("--print-env0")
            .output()?;
        require(raw.status.success(), "Launcher environment failed")?;
        let mut values = env::vars().collect::<BTreeMap<_, _>>();
        for row in raw.stdout.split(|byte| *byte == 0) {
            if let Some(index) = row.iter().position(|byte| *byte == b'=') {
                values.insert(
                    String::from_utf8(row[..index].to_vec())?,
                    String::from_utf8(row[index + 1..].to_vec())?,
                );
            }
        }
        for (key, value) in [
            ("WLR_BACKENDS", "headless"),
            ("WLR_HEADLESS_OUTPUTS", "2"),
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
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
            ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
            ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
            ("LUMEN_FILE_SERVICE", "0"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
        ] {
            values.insert(key.into(), value.into());
        }
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "AT_SPI_BUS_ADDRESS",
        ] {
            values.remove(key);
        }
        let assets = options.output.join("assets");
        copy_tree(&options.source.join("res/lumen-desktop"), &assets)?;
        let palette: Value =
            serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
        let appearance = options.output.join("appearance.json");
        save(
            &appearance,
            &json!({"font":palette["font"],"palette":palette[&options.theme],"preferences":{}}),
        )?;
        values.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            appearance.display().to_string(),
        );
        let qml = options.output.join("shell.qml");
        fs::write(&qml, include_str!("../fixture.qml"))?;
        Ok(Self {
            options,
            env: values,
            runtime,
            qml,
            processes: Vec::new(),
            pointer: None,
        })
    }
    fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args).env_clear().envs(&self.env);
        command
    }
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = self.command(program, args).output()?;
        if !output.status.success() {
            return Err(fail(format!(
                "{program} {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn launch(&mut self, program: &str, args: &[&str], log: &str, pipe: bool) -> Result<usize> {
        let file = File::create(self.options.output.join(log))?;
        let mut cmd = self.command(program, args);
        cmd.stdout(file.try_clone()?).stderr(file);
        if pipe {
            cmd.stdin(Stdio::piped());
        }
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        self.processes.push(Managed(cmd.spawn()?));
        Ok(self.processes.len() - 1)
    }
    fn start(&mut self) -> Result<()> {
        let config = self.options.output.join("sway.conf");
        fs::write(
            &config,
            "output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1280x900 position 1280 0\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border pixel 0\n",
        )?;
        let sway = self.options.sway.display().to_string();
        self.launch(
            &sway,
            &[
                "--unsupported-gpu",
                "-c",
                config.to_str().ok_or("Config path")?,
            ],
            "sway.log",
            false,
        )?;
        wait_true("private compositor", || {
            Ok(first_file(&self.runtime, "sway-ipc", false)?.is_some())
        })?;
        self.env.insert(
            "SWAYSOCK".into(),
            first_file(&self.runtime, "sway-ipc", false)?
                .ok_or("Missing Sway socket")?
                .display()
                .to_string(),
        );
        self.env.insert(
            "WAYLAND_DISPLAY".into(),
            first_file(&self.runtime, "wayland-", true)?
                .ok_or("Missing display")?
                .file_name()
                .ok_or("Display name")?
                .to_string_lossy()
                .into_owned(),
        );
        Ok(())
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut argv = vec![
            "-p",
            self.qml.to_str().ok_or("QML path")?,
            "ipc",
            "call",
            "widgetAudit",
            method,
        ];
        argv.extend(args);
        self.call("quickshell", &argv)
    }
    fn state(&self) -> Result<Value> {
        let value: Value = serde_json::from_str(&self.ipc("state", &[])?)?;
        save(&self.options.output.join("last-state.json"), &value)?;
        Ok(value)
    }
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn pointer(&mut self, value: &str) -> Result<()> {
        let index = self.pointer.ok_or("Missing pointer")?;
        let pipe = self.processes[index]
            .0
            .stdin
            .as_mut()
            .ok_or("Missing pointer pipe")?;
        pipe.write_all(value.as_bytes())?;
        pipe.flush()?;
        pause(120);
        Ok(())
    }
    fn cursor(&mut self, x: i64, y: i64) -> Result<()> {
        self.call(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &x.to_string(),
                &y.to_string(),
            ],
        )?;
        self.pointer("m 0.5 0.5\n")
    }
    fn click(&mut self, x: i64, y: i64) -> Result<()> {
        self.cursor(x, y)?;
        self.pointer("p\n")?;
        self.pointer("r\n")
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.call("wtype", args)?;
        pause(120);
        Ok(())
    }
    fn screenshot(&self, name: &str) -> Result<()> {
        self.call(
            "grim",
            &[self
                .options
                .output
                .join(name)
                .to_str()
                .ok_or("Screenshot path")?],
        )?;
        Ok(())
    }
    fn stop(&mut self, index: usize) {
        self.processes[index].stop();
    }
    fn kill(&mut self, index: usize) {
        self.processes[index].kill();
    }
}
fn first_file(root: &Path, prefix: &str, exclude_lock: bool) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|row| row.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let text = name.to_string_lossy();
                text.starts_with(prefix) && (!exclude_lock || !text.ends_with(".lock"))
            })
        }))
}
fn node<'a>(value: &'a Value, key: &str, value_to_find: &str) -> Option<&'a Value> {
    if value[key] == value_to_find {
        return Some(value);
    }
    for field in ["nodes", "floating_nodes"] {
        if let Some(children) = value[field].as_array() {
            for child in children {
                if let Some(found) = node(child, key, value_to_find) {
                    return Some(found);
                }
            }
        }
    }
    None
}
fn item(value: &Value) -> Result<&Value> {
    value["preferences"]["desktopItems"]
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or_else(|| fail("Missing widget item"))
}
fn card(value: &Value) -> Result<&Value> {
    value["outputs"]
        .as_array()
        .and_then(|outputs| outputs.iter().find(|output| output["name"] == "HEADLESS-1"))
        .and_then(|output| output["items"].as_array())
        .and_then(|items| items.first())
        .ok_or_else(|| fail("Missing widget card"))
}
fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| fail("Missing numeric widget value"))
}
fn local_point(f: &Fixture, name: &str) -> Result<(i64, i64)> {
    let local: Value = serde_json::from_str(&f.ipc("point", &[name])?)?;
    let tree = f.tree()?;
    let window = node(&tree, "name", "Widget Catalogue").ok_or("Missing catalogue window")?;
    Ok((
        (number(&local["x"])? + number(&window["rect"]["x"])?).round() as i64,
        (number(&local["y"])? + number(&window["rect"]["y"])?).round() as i64,
    ))
}
fn widget_point(f: &Fixture, name: &str) -> Result<(i64, i64)> {
    let value: Value = serde_json::from_str(&f.ipc("widgetPoint", &[name])?)?;
    Ok((
        number(&value["x"])?.round() as i64,
        number(&value["y"])?.round() as i64,
    ))
}
fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    f.start()?;
    let qml = f.qml.display().to_string();
    let mut shell = f.launch("quickshell", &["-p", &qml], "shell.log", false)?;
    wait_true("two widget outputs", || {
        Ok(f.state()?["outputs"]
            .as_array()
            .is_some_and(|values| values.len() == 2))
    })?;
    let document = f.options.output.join("synthetic-document.txt");
    let doc_bin = env::current_exe()?
        .parent()
        .ok_or("Binary folder")?
        .join("widget-document-client");
    let document_process = f.launch(
        "foot",
        &[
            "--app-id=lumen-recovery-document",
            "--title=Recovery document",
            "-e",
            doc_bin.to_str().ok_or("Document client")?,
            document.to_str().ok_or("Document path")?,
        ],
        "document.log",
        false,
    )?;
    wait_true("editable document", || {
        Ok(node(&f.tree()?, "app_id", "lumen-recovery-document").is_some())
    })?;
    let tree = f.tree()?;
    let original = node(&tree, "app_id", "lumen-recovery-document").ok_or("Missing document")?;
    let identity = (original["id"].clone(), original["pid"].clone());
    f.call(
        "swaymsg",
        &[
            &format!("[con_id={}]", identity.0),
            "move",
            "container",
            "to",
            "workspace",
            "recovery-document",
        ],
    )?;
    f.call("swaymsg", &["workspace", "1"])?;
    f.launch("wtype", &["-s", "60000"], "keyboard.log", false)?;
    let pointer = f.options.pointer.display().to_string();
    let pointer_index = f.launch(
        &pointer,
        &["pointer", "move", "0", "0"],
        "pointer.log",
        true,
    )?;
    f.pointer = Some(pointer_index);
    pause(200);
    let (x, y) = local_point(&f, "widget-add-clock")?;
    f.click(x, y)?;
    wait_true("clock added to two outputs", || {
        let value = f.state()?;
        Ok(item(&value).is_ok()
            && value["outputs"].as_array().is_some_and(|outputs| {
                outputs.iter().all(|output| {
                    output["items"]
                        .as_array()
                        .is_some_and(|items| items.len() == 1)
                })
            }))
    })?;
    let mut checks = vec!["Actual catalogue click creates one stable instance on both outputs"];
    f.ipc("capture", &[])?;
    let value = f.state()?;
    let clock = card(&value)?;
    f.click(
        number(&clock["x"])?.round() as i64 + 90,
        number(&clock["y"])?.round() as i64 + 70,
    )?;
    wait_true("clock activation", || Ok(f.state()?["activations"] == 1))?;
    checks.push("Clock activation emits exactly one related-content action");
    f.click(20, 850)?;
    wait_true(
        "background mask click",
        || Ok(f.state()?["background"] == 1),
    )?;
    checks.push("Widget union mask lets desktop background clicks through");
    let value = f.state()?;
    let clock = card(&value)?;
    let start = (
        number(&clock["x"])? + number(&clock["width"])? - 25.0,
        number(&clock["y"])? + 14.0,
    );
    f.cursor(start.0.round() as i64, start.1.round() as i64)?;
    f.pointer("p\n")?;
    f.cursor(
        (start.0 + 100.0).round() as i64,
        (start.1 + 60.0).round() as i64,
    )?;
    wait_true("held widget movement", || {
        Ok(card(&f.state()?)?["moving"] == true)
    })?;
    f.key(&["-k", "Escape"])?;
    f.pointer("r\n")?;
    wait_true("cancelled widget movement", || {
        Ok(card(&f.state()?)?["moving"] == false)
    })?;
    require(
        item(&f.state()?)?["positions"]
            .as_object()
            .is_some_and(|positions| positions.is_empty()),
        "Cancelled move persisted",
    )?;
    checks.push("Pointer preview and Escape do not persist a cancelled placement");
    let value = f.state()?;
    let clock = card(&value)?;
    let start = (
        number(&clock["x"])? + number(&clock["width"])? - 25.0,
        number(&clock["y"])? + 14.0,
    );
    f.cursor(start.0.round() as i64, start.1.round() as i64)?;
    f.pointer("p\n")?;
    f.cursor(
        (start.0 + 100.0).round() as i64,
        (start.1 + 60.0).round() as i64,
    )?;
    f.pointer("r\n")?;
    wait_true("committed widget position", || {
        Ok(item(&f.state()?)?["positions"].get("HEADLESS-1").is_some())
    })?;
    let value = f.state()?;
    require(
        item(&value)?["positions"].get("HEADLESS-2").is_none() && value["same"] == true,
        "Widget movement changed other output or identity",
    )?;
    checks.push("Pointer release saves only the touched display without recreating the widget");
    let original = item(&f.state()?)?["positions"]["HEADLESS-1"].clone();
    f.key(&["-k", "space", "-k", "Right", "-k", "Escape"])?;
    require(
        item(&f.state()?)?["positions"]["HEADLESS-1"] == original,
        "Keyboard Escape moved widget",
    )?;
    f.key(&["-k", "space", "-k", "Right", "-k", "Return"])?;
    wait_true("keyboard widget commit", || {
        Ok(number(&item(&f.state()?)?["positions"]["HEADLESS-1"]["x"])? > number(&original["x"])?)
    })?;
    checks.push("Keyboard move previews cancel or commit against the same captured instance");
    let saved_before_crash = item(&f.state()?)?.clone();
    let value = f.state()?;
    let clock = card(&value)?;
    let start = (
        number(&clock["x"])? + number(&clock["width"])? - 25.0,
        number(&clock["y"])? + 14.0,
    );
    f.cursor(start.0.round() as i64, start.1.round() as i64)?;
    f.pointer("p\n")?;
    f.cursor(
        (start.0 + 80.0).round() as i64,
        (start.1 + 40.0).round() as i64,
    )?;
    wait_true("move before shell crash", || {
        Ok(card(&f.state()?)?["moving"] == true)
    })?;
    f.kill(shell);
    f.pointer("r\n")?;
    shell = f.launch(
        "quickshell",
        &["-p", &qml],
        "shell-crash-restart.log",
        false,
    )?;
    wait_true("widget recovered", || {
        Ok(item(&f.state()?)?["id"] == saved_before_crash["id"])
    })?;
    require(
        item(&f.state()?)? == &saved_before_crash && card(&f.state()?)?["moving"] == false,
        "Crash persisted preview or changed widget",
    )?;
    let tree = f.tree()?;
    let current = node(&tree, "app_id", "lumen-recovery-document")
        .ok_or("Document lost during shell crash")?;
    require(
        current["id"] == identity.0 && current["pid"] == identity.1,
        "Document native identity changed",
    )?;
    f.call("swaymsg", &[&format!("[con_id={}]", identity.0), "focus"])?;
    f.key(&["recovery-document-still-editable", "-k", "Return"])?;
    wait_true("document editable after crash", || {
        Ok(document.exists()
            && fs::read_to_string(&document)?.contains("recovery-document-still-editable"))
    })?;
    f.call("swaymsg", &["workspace", "1"])?;
    f.click(20, 850)?;
    wait_true("recovered mask", || Ok(f.state()?["background"] == 1))?;
    let value = f.state()?;
    let clock = card(&value)?;
    f.click(
        number(&clock["x"])?.round() as i64 + 90,
        number(&clock["y"])?.round() as i64 + 70,
    )?;
    wait_true(
        "recovered activation",
        || Ok(f.state()?["activations"] == 1),
    )?;
    pause(200);
    require(
        f.state()?["activations"] == 1,
        "Recovery duplicated activation",
    )?;
    f.ipc("capture", &[])?;
    checks.push("SIGKILL during held native movement preserves committed layout, surviving editable document identity, click-through and one fresh activation");
    let identifier = item(&f.state()?)?["id"]
        .as_str()
        .ok_or("Widget id")?
        .to_owned();
    let (x, y) = local_point(&f, &format!("widget-size-{identifier}"))?;
    f.click(x, y)?;
    f.key(&["-k", "Home", "-k", "Return"])?;
    wait_true("widget size", || {
        let value = f.state()?;
        Ok(item(&value)?["size"] == "small" && card(&value)?["width"] == 214)
    })?;
    let value = f.state()?;
    require(
        value["same"] == true && value["sameEditor"] == true,
        "Resize recreated widget",
    )?;
    checks.push("Size control updates both surfaces while preserving component identity");
    f.screenshot(&format!("widgets-{}.png", f.options.theme))?;
    let saved = item(&f.state()?)?.clone();
    f.stop(shell);
    shell = f.launch("quickshell", &["-p", &qml], "shell-restart.log", false)?;
    wait_true("widget graceful restart", || {
        Ok(item(&f.state()?)?["id"] == saved["id"])
    })?;
    require(item(&f.state()?)? == &saved, "Restart changed widget")?;
    checks.push("Restart restores the exact instance, size and independent positions");
    let (x, y) = local_point(&f, &format!("widget-remove-{identifier}"))?;
    f.click(x, y)?;
    wait_true("last widget removed", || {
        let value = f.state()?;
        Ok(value["preferences"]["desktopItems"] == json!([])
            && value["outputs"]
                .as_array()
                .is_some_and(|outputs| outputs.iter().all(|output| output["items"] == json!([]))))
    })?;
    checks.push("Removing the last instance preserves an explicitly empty layout");
    let (x, y) = local_point(&f, "widget-add-calendar")?;
    f.click(x, y)?;
    wait_true("calendar added", || {
        let value = f.state()?;
        Ok(item(&value)?["type"] == "calendar"
            && value["outputs"].as_array().is_some_and(|outputs| {
                outputs.iter().all(|output| {
                    output["items"]
                        .as_array()
                        .is_some_and(|items| items.len() == 1)
                })
            }))
    })?;
    require(
        item(&f.state()?)?["id"] != identifier,
        "Calendar reused identity",
    )?;
    checks.push("Calendar can be added after an explicitly empty layout with a fresh identity");
    f.screenshot(&format!("calendar-{}.png", f.options.theme))?;
    let (x, y) = widget_point(&f, "calendar-open-related")?;
    f.click(x, y)?;
    wait_true("calendar pointer activation", || {
        Ok(f.state()?["activations"] == 1)
    })?;
    f.key(&["-k", "space"])?;
    wait_true("calendar keyboard activation", || {
        Ok(f.state()?["activations"] == 2)
    })?;
    pause(200);
    require(
        f.state()?["activations"] == 2,
        "Calendar activation duplicated",
    )?;
    checks.push(
        "Calendar related-content pointer and keyboard activation each dispatch exactly once",
    );
    for _ in 0..3 {
        let (x, y) = local_point(&f, "widget-add-clock")?;
        f.click(x, y)?;
    }
    let mut maximum = 0.0f64;
    for _ in 0..24 {
        f.key(&["-k", "Tab"])?;
        maximum = maximum.max(number(&f.state()?["scrollY"])?);
    }
    require(
        maximum > 0.0,
        "Keyboard traversal did not reveal editor controls",
    )?;
    checks.push("Keyboard traversal reveals controls below the widget editor viewport");
    require(f.state()?["error"] == "", "Widget state error")?;
    for name in ["shell.log", "shell-restart.log", "shell-crash-restart.log"] {
        let log = fs::read_to_string(f.options.output.join(name))?;
        require(
            ![
                "TypeError:",
                "ReferenceError:",
                "Binding loop",
                "Failed to load configuration",
                "Cannot assign",
            ]
            .iter()
            .any(|marker| log.contains(marker)),
            format!("{name} QML error"),
        )?;
    }
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":checks,"final":f.state()?,"scope":"Two private native outputs, synthetic date, real pointer and keyboard; no live accounts/settings"}),
    )?;
    let _ = (shell, document_process);
    Ok(())
}
fn main() -> Result<()> {
    let options = Options::parse()?;
    if options.inside {
        run(options)
    } else {
        private_session()
    }
}
