//! Native two-output fullscreen menu and input fixture.
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
        pause(100);
    }
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for row in fs::read_dir(source)? {
        let row = row?;
        let path = row.path();
        let destination = target.join(row.file_name());
        if path.is_dir() {
            copy_tree(&path, &destination)?;
        } else if path.is_file() {
            fs::copy(path, destination)?;
        }
    }
    Ok(())
}
fn first_file(root: &Path, prefix: &str, exclude_lock: bool) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && (!exclude_lock || !name.ends_with(".lock"))
            })
        }))
}
fn nodes<'a>(node: &'a Value, output: &mut Vec<&'a Value>) {
    output.push(node);
    for field in ["nodes", "floating_nodes"] {
        if let Some(children) = node[field].as_array() {
            for child in children {
                nodes(child, output);
            }
        }
    }
}
struct Options {
    output: PathBuf,
    source: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
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
                Some("--effects") => effects = true,
                Some("--inside") => inside = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown fullscreen menu option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        Ok(Self {
            output: env::current_dir()?.join(output.ok_or("--output required")?),
            source: source.unwrap_or(env::current_dir()?).canonicalize()?,
            launcher: launcher.ok_or("--launcher required")?.canonicalize()?,
            sway: sway.ok_or("--sway required")?.canonicalize()?,
            pointer: pointer.ok_or("--pointer-tool required")?.canonicalize()?,
            effects,
            inside,
        })
    }
}
fn private_session() -> Result<()> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = PathBuf::from(format!("/tmp/lfm-{}-{nanos}", std::process::id()));
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let config = root.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            root.display()
        ),
    )?;
    let mut cmd = Command::new("dbus-run-session");
    cmd.arg(format!("--config-file={}", config.display()))
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
    require(status?.success(), "Private fullscreen menu fixture failed")
}
struct Managed(Child);
impl Managed {
    fn stop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let end = Instant::now() + Duration::from_secs(3);
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
}
struct Fixture {
    options: Options,
    env: BTreeMap<String, String>,
    runtime: PathBuf,
    qml: PathBuf,
    client: PathBuf,
    appearance: PathBuf,
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
            (
                "WLR_RENDERER",
                if options.effects { "gles2" } else { "pixman" },
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
            (
                "LUMEN_COMPOSITOR_EFFECTS",
                if options.effects { "1" } else { "0" },
            ),
            ("GLOBAL_MENU_ENABLED", "0"),
            ("WLR_HEADLESS_OUTPUTS", "2"),
        ] {
            values.insert(key.into(), value.into());
        }
        if options.effects {
            values.insert("WLR_RENDERER_FORCE_SOFTWARE".into(), "1".into());
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
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{},"menuBarPreferences":{"fullscreenVisibility":"automatic"}}),
        )?;
        values.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            appearance.display().to_string(),
        );
        let qml = assets.join("shell/shell.qml");
        let original = fs::read_to_string(&qml)?;
        let index = original.rfind('}').ok_or("Shell QML missing root brace")?;
        fs::write(
            &qml,
            format!(
                "{}{}{}",
                &original[..index],
                include_str!("../shell.qml.inc"),
                "}\n"
            ),
        )?;
        let menu_file = assets.join("shell/MenuBar.qml");
        let menu = fs::read_to_string(&menu_file)?;
        fs::write(menu_file, menu.replace("    id: bar", "    id: bar\n    function pointerAudit(){return {hovered:barHover.hovered,brand:brandButton.pointerAudit()};}"))?;
        let button_file = assets.join("shell/MenuButton.qml");
        let button = fs::read_to_string(&button_file)?;
        fs::write(button_file, button.replace("    id: button", "    id: button\n    function pointerAudit(){return {inside:mouse.containsMouse,x:mouse.mouseX,y:mouse.mouseY};}"))?;
        let client = options.output.join("client.qml");
        fs::write(&client, include_str!("../client.qml"))?;
        let isolation: Value = serde_json::from_str(&String::from_utf8(
            Command::new("lumen-fixture-providers")
                .args([
                    "isolate",
                    assets.join("shell").to_str().ok_or("Shell path")?,
                    options.output.to_str().ok_or("Output path")?,
                ])
                .env_clear()
                .envs(&values)
                .output()?
                .stdout,
        )?)?;
        save(&options.output.join("provider-isolation.json"), &isolation)?;
        Ok(Self {
            options,
            env: values,
            runtime,
            qml,
            client,
            appearance,
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
        let config_text = format!(
            "output HEADLESS-1 resolution 800x600 position 0 0\noutput HEADLESS-2 resolution 640x480 position 800 0\nxwayland disable\nfor_window [title=\"Fullscreen Input Probe\"] floating enable, move container to output HEADLESS-1\nbindsym Mod4+m exec quickshell -p '{}' ipc call fullscreenMenuAudit keyboardMenu\n",
            self.qml.display().to_string().replace('\'', "'\\''")
        );
        fs::write(&config, config_text)?;
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
        wait_true("compositor socket", || {
            Ok(first_file(&self.runtime, "sway-ipc", false)?.is_some())
        })?;
        self.env.insert(
            "SWAYSOCK".into(),
            first_file(&self.runtime, "sway-ipc", false)?
                .ok_or("Missing Sway socket")?
                .display()
                .to_string(),
        );
        wait_true("Wayland display", || {
            Ok(first_file(&self.runtime, "wayland-", true)?.is_some())
        })?;
        self.env.insert(
            "WAYLAND_DISPLAY".into(),
            first_file(&self.runtime, "wayland-", true)?
                .ok_or("Missing display")?
                .file_name()
                .ok_or("Display name")?
                .to_string_lossy()
                .into_owned(),
        );
        self.launch("wtype", &["-s", "120000"], "keyboard.log", false)?;
        let pointer_tool = self.options.pointer.display().to_string();
        self.pointer = Some(self.launch(
            &pointer_tool,
            &["pointer", "move", "0", "0"],
            "pointer.log",
            true,
        )?);
        self.pointer_command("m 0.5 0.5")?;
        let qml = self.qml.display().to_string();
        self.launch("quickshell", &["-p", &qml], "shell.log", false)?;
        wait_true("two menu bars", || {
            Ok(self.state()?["available"] == true
                && self.state()?["bars"]
                    .as_array()
                    .is_some_and(|bars| bars.len() == 2))
        })?;
        let client = self.client.display().to_string();
        self.launch("quickshell", &["-p", &client], "client.log", false)?;
        wait_true("native client", || Ok(self.native().is_ok()))?;
        Ok(())
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut argv = vec![
            "-p",
            self.qml.to_str().ok_or("QML path")?,
            "ipc",
            "call",
            "fullscreenMenuAudit",
            method,
        ];
        argv.extend(args);
        self.call("quickshell", &argv)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("state", &[])?)?)
    }
    fn row(&self, name: &str) -> Result<Value> {
        self.state()?["bars"]
            .as_array()
            .ok_or_else(|| fail("No bars"))?
            .iter()
            .find(|row| row["output"] == name)
            .cloned()
            .ok_or_else(|| fail(format!("Missing output {name}")))
    }
    fn observed(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.call(
            "quickshell",
            &[
                "-p",
                self.client.to_str().ok_or("Client path")?,
                "ipc",
                "call",
                "client",
                "state",
            ],
        )?)?)
    }
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn outputs(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_outputs"])?,
        )?)
    }
    fn native(&self) -> Result<Value> {
        let tree = self.tree()?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        all.into_iter()
            .find(|node| node["name"] == "Fullscreen Input Probe")
            .cloned()
            .ok_or_else(|| fail("Missing native client"))
    }
    fn pointer_command(&mut self, value: &str) -> Result<()> {
        let index = self.pointer.ok_or("Missing pointer")?;
        let pipe = self.processes[index]
            .0
            .stdin
            .as_mut()
            .ok_or("Missing pointer pipe")?;
        writeln!(pipe, "{value}")?;
        pipe.flush()?;
        pause(120);
        Ok(())
    }
    fn move_to(&mut self, x: f64, y: f64) -> Result<()> {
        let outputs = self.outputs()?;
        let active = outputs
            .as_array()
            .ok_or("Output list")?
            .iter()
            .filter(|row| row["active"] == true)
            .collect::<Vec<_>>();
        let min_x = active
            .iter()
            .filter_map(|row| row["rect"]["x"].as_i64())
            .min()
            .ok_or("No active output")?;
        let min_y = active
            .iter()
            .filter_map(|row| row["rect"]["y"].as_i64())
            .min()
            .ok_or("No active output")?;
        self.call(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &((x - min_x as f64).round() as i64).to_string(),
                &((y - min_y as f64).round() as i64).to_string(),
            ],
        )?;
        self.pointer_command("m 0.5 0.5")
    }
    fn click(&mut self, x: f64, y: f64) -> Result<()> {
        self.move_to(x, y)?;
        self.pointer_command("p")?;
        self.pointer_command("r")
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.call("wtype", args)?;
        pause(120);
        Ok(())
    }
    fn preference(&self, value: &str, legacy: bool) -> Result<()> {
        let mut data: Value = serde_json::from_slice(&fs::read(&self.appearance)?)?;
        let mut pref = json!({"order":["battery","sound","focus","search","controls"],"hourCycle":"24","showDate":true});
        if value == "automatic" || legacy {
            pref["fullscreenVisibility"] = json!(value);
        } else {
            pref["visibility"] = json!(value);
        }
        data["menuBarPreferences"] = pref;
        let pending = self.appearance.with_extension("new");
        save(&pending, &data)?;
        fs::rename(pending, &self.appearance)?;
        Ok(())
    }
    fn screenshot(&self, name: &str, output: Option<&str>) -> Result<()> {
        let path = self.options.output.join(name);
        let path = path.to_str().ok_or("Screenshot path")?;
        if let Some(output) = output {
            self.call("grim", &["-o", output, path])?;
        } else {
            self.call("grim", &[path])?;
        }
        Ok(())
    }
    fn pixel(&self, name: &str, x: usize, y: usize) -> Result<[u8; 3]> {
        let file = File::open(self.options.output.join(name))?;
        let mut decoder = png::Decoder::new(file);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer)?;
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            _ => {
                return Err(fail(format!(
                    "Unsupported PNG color: {:?}",
                    info.color_type
                )));
            }
        };
        require(
            x < info.width as usize && y < info.height as usize,
            "Pixel outside screenshot",
        )?;
        let index = (y * info.width as usize + x) * channels;
        Ok([buffer[index], buffer[index + 1], buffer[index + 2]])
    }
    fn painted(&self, name: &str, first: bool, second: bool) -> Result<()> {
        wait_true(name, || {
            self.screenshot(name, None)?;
            let mut mismatches = Vec::new();
            for (x, shown) in [(300, first), (1100, second)] {
                let pixel = self.pixel(name, x, 10)?;
                let low = *pixel.iter().min().unwrap();
                let high = *pixel.iter().max().unwrap();
                let matches = if shown {
                    low > 20 && high < 120 && high - low < 25
                } else {
                    pixel[2] > pixel[0] + 40
                };
                if !matches {
                    mismatches.push(json!({"x":x,"shown":shown,"pixel":pixel}));
                }
            }
            save(
                &self.options.output.join(format!("{name}-pixels.json")),
                &json!(mismatches),
            )?;
            Ok(mismatches.is_empty())
        })
    }
    fn desktop_painted(&self, name: &str, first: bool, second: bool) -> Result<()> {
        wait_true(name, || {
            self.screenshot(name, None)?;
            Ok([(300, first), (1100, second)]
                .into_iter()
                .all(|(x, shown)| {
                    self.pixel(name, x, 10).is_ok_and(|pixel| {
                        if shown {
                            *pixel.iter().min().unwrap() > 20
                        } else {
                            *pixel.iter().max().unwrap() < 10
                        }
                    })
                }))
        })
    }
    fn id(&self) -> Result<i64> {
        self.native()?["id"]
            .as_i64()
            .ok_or_else(|| fail("Missing client id"))
    }
    fn focus(&self, id: i64) -> Result<()> {
        self.call("swaymsg", &[&format!("[con_id={id}]"), "focus"])?;
        self.call(
            "quickshell",
            &[
                "-p",
                self.client.to_str().ok_or("Client path")?,
                "ipc",
                "call",
                "client",
                "focus",
            ],
        )?;
        Ok(())
    }
    fn fullscreen(&self, id: i64, args: &[&str]) -> Result<()> {
        let mut argv = vec![format!("[con_id={id}]"), "fullscreen".into()];
        argv.extend(args.iter().map(|arg| arg.to_string()));
        self.call(
            "swaymsg",
            &argv.iter().map(String::as_str).collect::<Vec<_>>(),
        )?;
        Ok(())
    }
}
fn all_bars(state: &Value, mut predicate: impl FnMut(&Value) -> bool) -> bool {
    state["bars"]
        .as_array()
        .is_some_and(|bars| !bars.is_empty() && bars.iter().all(&mut predicate))
}
fn visible_zone(row: &Value, visible: bool, zone: i64) -> bool {
    row["visible"] == visible && row["zone"] == zone
}
fn counts_plus(before: &Value, after: &Value, field: &str) -> bool {
    after[field].as_i64() == before[field].as_i64().map(|v| v + 1)
}
fn run(mut f: Fixture) -> Result<()> {
    f.start()?;
    f.move_to(500.0, 300.0)?;
    save(
        &f.options.output.join("desktop-state.json"),
        &json!({"state":f.state()?,"tree":f.tree()?,"outputs":f.outputs()?}),
    )?;
    require(
        all_bars(&f.state()?, |r| visible_zone(r, true, 28)),
        "Desktop menu did not reserve both outputs",
    )?;
    f.painted("desktop.png", true, true)?;
    let mut checks = vec!["Desktop retains its menu and reserved work area on both outputs"];
    f.preference("always", true)?;
    wait_true("legacy always", || {
        Ok(all_bars(&f.state()?, |r| visible_zone(r, true, 28)))
    })?;
    checks.push("Legacy automatic startup and always live snapshots migrate without warnings or competing fields");
    f.preference("fullscreen", false)?;
    f.move_to(500.0, 300.0)?;
    wait_true("desktop hidden", || {
        Ok(all_bars(&f.state()?, |r| visible_zone(r, false, 0)))
    })?;
    f.desktop_painted("desktop-hidden.png", false, false)?;
    f.move_to(300.0, 0.0)?;
    wait_true("desktop local reveal", || {
        Ok(f.row("HEADLESS-1")?["visible"] == true && f.row("HEADLESS-2")?["visible"] == false)
    })?;
    f.desktop_painted("desktop-local-reveal.png", true, false)?;
    f.click(34.0, 14.0)?;
    wait_true("desktop pointer menu", || {
        Ok(f.row("HEADLESS-1")?["menu"] == "Desktop")
    })?;
    f.key(&["-k", "Escape"])?;
    f.move_to(500.0, 300.0)?;
    wait_true("desktop leave", || {
        Ok(all_bars(&f.state()?, |r| r["visible"] == false))
    })?;
    f.key(&["-M", "logo", "-k", "m", "-m", "logo"])?;
    wait_true("desktop keyboard menu", || {
        Ok(f.row("HEADLESS-1")?["menu"] == "Desktop")
    })?;
    f.key(&["-k", "Escape"])?;
    wait_true("desktop keyboard close", || {
        Ok(f.row("HEADLESS-1")?["visible"] == false)
    })?;
    f.focus(f.id()?)?;
    let rect = f.native()?["rect"].clone();
    let before = f.observed()?;
    f.click(
        rect["x"].as_f64().ok_or("Client x")? + rect["width"].as_f64().ok_or("Client width")? / 2.0,
        rect["y"].as_f64().ok_or("Client y")?
            + rect["height"].as_f64().ok_or("Client height")? / 2.0,
    )?;
    f.key(&["-k", "k"])?;
    wait_true("desktop client key", || {
        Ok(counts_plus(&before, &f.observed()?, "keys"))
    })?;
    require(
        counts_plus(&before, &f.observed()?, "clicks"),
        "Desktop client click lost",
    )?;
    checks.push("Desktop autohide removes reserved space; output-local pointer and keyboard menus return exact client input");
    f.preference("auto_hide", false)?;
    f.move_to(500.0, 300.0)?;
    wait_true("auto hide", || {
        Ok(all_bars(&f.state()?, |r| {
            r["autoHide"] == true && r["visible"] == false
        }))
    })?;
    f.preference("always", false)?;
    wait_true("live always", || {
        Ok(all_bars(&f.state()?, |r| visible_zone(r, true, 28)))
    })?;
    f.desktop_painted("desktop-live-always.png", true, true)?;
    f.preference("desktop", false)?;
    wait_true("live desktop", || {
        Ok(all_bars(&f.state()?, |r| visible_zone(r, true, 28)))
    })?;
    checks.push("Live canonical desktop/always/auto_hide choices have one visibility authority on both outputs");
    let id = f.id()?;
    f.focus(id)?;
    f.fullscreen(id, &["enable"])?;
    wait_true("automatic fullscreen", || {
        Ok(f.row("HEADLESS-1")?["automatic"] == true && f.row("HEADLESS-1")?["visible"] == false)
    })?;
    require(
        f.row("HEADLESS-2")?["visible"] == true && f.row("HEADLESS-2")?["fullscreen"] == false,
        "Secondary menu lost",
    )?;
    require(
        f.native()?["fullscreen_mode"] == 1,
        "Client did not enter local fullscreen",
    )?;
    f.painted("automatic-hidden.png", false, true)?;
    let before = f.observed()?;
    f.key(&["-k", "k"])?;
    wait_true("fullscreen client key", || {
        Ok(counts_plus(&before, &f.observed()?, "keys"))
    })?;
    f.click(400.0, 200.0)?;
    require(
        counts_plus(&before, &f.observed()?, "clicks"),
        "Fullscreen client click lost",
    )?;
    f.move_to(300.0, 0.0)?;
    wait_true("automatic reveal", || {
        Ok(f.row("HEADLESS-1")?["visible"] == true)
    })?;
    f.painted("automatic-revealed.png", true, true)?;
    require(
        f.native()?["fullscreen_mode"] == 1,
        "Fullscreen changed after reveal",
    )?;
    f.click(34.0, 14.0)?;
    wait_true("automatic pointer menu", || {
        Ok(f.row("HEADLESS-1")?["menu"] == "Desktop")
    })?;
    f.key(&["-k", "Escape"])?;
    wait_true("automatic menu close", || {
        Ok(f.row("HEADLESS-1")?["menu"] == "")
    })?;
    f.move_to(400.0, 200.0)?;
    wait_true("automatic leave", || {
        Ok(f.row("HEADLESS-1")?["visible"] == false)
    })?;
    f.painted("automatic-left.png", false, true)?;
    require(
        f.native()?["fullscreen_mode"] == 1,
        "Fullscreen changed after leave",
    )?;
    let before = f.observed()?;
    f.click(400.0, 200.0)?;
    f.key(&["-k", "k"])?;
    wait_true("return client input", || {
        Ok(counts_plus(&before, &f.observed()?, "keys"))
    })?;
    require(
        counts_plus(&before, &f.observed()?, "clicks"),
        "Client click not returned",
    )?;
    checks.push("Automatic top-edge reveal, menu click, Escape and leave preserve fullscreen and restore exact app input");
    f.key(&["-M", "logo", "-k", "m", "-m", "logo"])?;
    wait_true("fullscreen keyboard menu", || {
        Ok(f.row("HEADLESS-1")?["menu"] == "Desktop" && f.row("HEADLESS-1")?["visible"] == true)
    })?;
    f.key(&["-k", "Escape"])?;
    wait_true("fullscreen keyboard close", || {
        Ok(f.row("HEADLESS-1")?["visible"] == false)
    })?;
    require(
        f.native()?["fullscreen_mode"] == 1,
        "Keyboard menu left fullscreen",
    )?;
    checks.push(
        "Physical keyboard route opens a hidden menu and Escape returns to the fullscreen client",
    );
    f.preference("fullscreen", false)?;
    f.move_to(400.0, 200.0)?;
    wait_true("fullscreen-only", || {
        Ok(f.row("HEADLESS-1")?["visible"] == true
            && f.row("HEADLESS-1")?["autoHide"] == false
            && f.row("HEADLESS-2")?["visible"] == false)
    })?;
    f.desktop_painted("fullscreen-only-visible.png", true, false)?;
    require(
        f.native()?["fullscreen_mode"] == 1,
        "Fullscreen-only changed client",
    )?;
    f.preference("auto_hide", false)?;
    wait_true("everywhere autohide", || {
        Ok(all_bars(&f.state()?, |r| {
            r["autoHide"] == true && r["visible"] == false
        }))
    })?;
    require(
        f.native()?["fullscreen_mode"] == 1,
        "Autohide changed client",
    )?;
    f.preference("desktop", false)?;
    wait_true("desktop from fullscreen", || {
        Ok(f.row("HEADLESS-1")?["automatic"] == true
            && f.row("HEADLESS-1")?["visible"] == false
            && f.row("HEADLESS-2")?["visible"] == true)
    })?;
    checks.push("Fullscreen-only and everywhere-autohide modes independently respect the other desktop output");
    f.preference("always", false)?;
    wait_true("always fullscreen", || {
        Ok(f.row("HEADLESS-1")?["automatic"] == false && f.row("HEADLESS-1")?["visible"] == true)
    })?;
    require(
        f.row("HEADLESS-1")?["zone"] == 0,
        "Fullscreen reserved desktop work area",
    )?;
    f.painted("always.png", true, true)?;
    f.move_to(400.0, 200.0)?;
    pause(700);
    require(
        f.row("HEADLESS-1")?["visible"] == true && f.native()?["fullscreen_mode"] == 1,
        "Always policy changed fullscreen",
    )?;
    f.preference("automatic", false)?;
    wait_true("live automatic", || {
        Ok(f.row("HEADLESS-1")?["automatic"] == true && f.row("HEADLESS-1")?["visible"] == false)
    })?;
    f.painted("live-automatic.png", false, true)?;
    checks.push("Live always/automatic presentation changes preserve fullscreen; fullscreen reserves no desktop work area");
    f.fullscreen(id, &["enable", "global"])?;
    wait_true("global fullscreen", || {
        Ok(f.native()?["fullscreen_mode"] == 2
            && f.state()?["outputs"]
                .as_object()
                .is_some_and(|outputs| outputs.values().all(|v| v == true)))
    })?;
    f.move_to(500.0, 200.0)?;
    wait_true("global hidden", || {
        Ok(all_bars(&f.state()?, |r| r["visible"] == false))
    })?;
    f.painted("global-hidden.png", false, false)?;
    f.move_to(1100.0, 0.0)?;
    wait_true("global secondary reveal", || {
        Ok(f.row("HEADLESS-2")?["visible"] == true)
    })?;
    f.painted("global-secondary-revealed.png", false, true)?;
    f.click(834.0, 14.0)?;
    wait_true("global secondary menu", || {
        Ok(f.row("HEADLESS-2")?["menu"] == "Desktop")
    })?;
    f.key(&["-k", "Escape"])?;
    f.move_to(1100.0, 200.0)?;
    wait_true("global secondary leave", || {
        Ok(f.row("HEADLESS-2")?["visible"] == false)
    })?;
    require(f.native()?["fullscreen_mode"] == 2, "Global mode changed")?;
    checks.push("Actual global fullscreen covers both outputs while secondary menu reveal/click/Escape preserves mode2");
    f.fullscreen(id, &["disable"])?;
    f.fullscreen(id, &["enable"])?;
    wait_true("restore local mode", || {
        Ok(f.row("HEADLESS-1")?["fullscreen"] == true
            && f.row("HEADLESS-2")?["fullscreen"] == false)
    })?;
    f.move_to(400.0, 200.0)?;
    f.call("swaymsg", &["output", "HEADLESS-1", "disable"])?;
    wait_true("migrate output", || {
        Ok(f.state()?["bars"].as_array().is_some_and(|v| v.len() == 1)
            && f.state()?["outputs"].get("HEADLESS-1").is_none()
            && f.row("HEADLESS-2")?["fullscreen"] == true)
    })?;
    require(
        f.native()?["fullscreen_mode"] == 1 && f.native()?["id"] == id,
        "Client identity changed on output removal",
    )?;
    let outputs = f.outputs()?;
    let remaining = outputs
        .as_array()
        .ok_or("Outputs list")?
        .iter()
        .find(|output| output["name"] == "HEADLESS-2")
        .ok_or("Missing remaining output")?["rect"]
        .clone();
    let rx = remaining["x"].as_f64().ok_or("Remaining x")?;
    let ry = remaining["y"].as_f64().ok_or("Remaining y")?;
    f.move_to(rx + 300.0, ry)?;
    wait_true("migrated reveal", || {
        Ok(f.row("HEADLESS-2")?["visible"] == true)
    })?;
    wait_true("migrated menu paint", || {
        f.screenshot("migrated-menu.png", Some("HEADLESS-2"))?;
        let pixel = f.pixel("migrated-menu.png", 300, 10)?;
        let lo = *pixel.iter().min().unwrap();
        let hi = *pixel.iter().max().unwrap();
        Ok(lo > 20 && hi < 120 && hi - lo < 25)
    })?;
    f.click(rx + 34.0, ry + 14.0)?;
    save(
        &f.options.output.join("removal-click.json"),
        &json!({"state":f.state()?,"client":f.observed()?,"outputs":f.outputs()?,"seats":serde_json::from_str::<Value>(&f.call("swaymsg",&["-r","-t","get_seats"])?)?}),
    )?;
    wait_true("migrated pointer menu", || {
        Ok(f.row("HEADLESS-2")?["menu"] == "Desktop")
    })?;
    f.key(&["-k", "Escape"])?;
    f.move_to(rx + 300.0, ry + 200.0)?;
    wait_true("migrated menu leave", || {
        Ok(f.row("HEADLESS-2")?["visible"] == false)
    })?;
    checks.push("Removing the fullscreen output migrates the same client and its local reveal/menu behavior without orphaned chrome");
    f.fullscreen(id, &["disable"])?;
    wait_true("leave fullscreen", || {
        Ok(f.row("HEADLESS-2")?["fullscreen"] == false && f.row("HEADLESS-2")?["visible"] == true)
    })?;
    require(
        f.row("HEADLESS-2")?["zone"] == 28,
        "Remaining desktop work area missing",
    )?;
    let log = fs::read_to_string(f.options.output.join("shell.log"))?;
    for marker in [
        "WARN scene:",
        "ReferenceError:",
        "TypeError:",
        "Binding loop",
        "Failed to load configuration",
        "Cannot assign",
    ] {
        require(!log.contains(marker), format!("Shell diagnostic {marker}"))?;
    }
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":checks,"final":f.state()?,"client":f.observed()?,"scope":"Actual private fullscreen native client, production menu/provider, two outputs and physical pointer/keyboard"}),
    )?;
    println!("PASS native fullscreen menu policy");
    Ok(())
}
fn main() -> Result<()> {
    let options = Options::parse()?;
    if !options.inside {
        private_session()
    } else {
        run(Fixture::new(options)?)
    }
}
