//! Paint vivid content at declared owned content boundaries in a private session.
mod frame_boundary;

use frame_boundary::{outside_frame, raster_origin};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
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
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= end {
            return Err(fail(format!(
                "Bounded native condition failed: {label}: {last}"
            )));
        }
        pause(50);
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
fn all_nodes<'a>(node: &'a Value, result: &mut Vec<&'a Value>) {
    result.push(node);
    for field in ["nodes", "floating_nodes"] {
        if let Some(children) = node[field].as_array() {
            for child in children {
                all_nodes(child, result);
            }
        }
    }
}
fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| fail(format!("Missing number: {value}")))
}
fn integer(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| fail(format!("Missing integer: {value}")))
}
fn boolean(value: &Value) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| fail(format!("Missing boolean: {value}")))
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn asset_hashes(root: &Path) -> Result<Value> {
    fn visit(root: &Path, at: &Path, values: &mut serde_json::Map<String, Value>) -> Result<()> {
        for row in fs::read_dir(at)? {
            let row = row?;
            let path = row.path();
            if path.is_dir() {
                visit(root, &path, values)?;
            } else if path.is_file() {
                values.insert(
                    path.strip_prefix(root)?.to_string_lossy().into_owned(),
                    json!(hash(&fs::read(path)?)),
                );
            }
        }
        Ok(())
    }
    let mut values = serde_json::Map::new();
    visit(root, root, &mut values)?;
    Ok(Value::Object(values))
}

struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    output: PathBuf,
    source: PathBuf,
    pointer: PathBuf,
    effects: bool,
    native_frame: bool,
    backend: String,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut launcher = None;
        let mut sway = None;
        let mut output = None;
        let mut source = None;
        let mut pointer = None;
        let mut effects = false;
        let mut native_frame = None;
        let mut backend = None;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--source") => {
                    source = Some(PathBuf::from(args.next().ok_or("Missing source")?))
                }
                Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--effects") => effects = true,
                Some("--native-frame") => {
                    native_frame = Some(match args.next().ok_or("Missing native frame")?.to_str() {
                        Some("on") => true,
                        Some("off") => false,
                        _ => return Err(fail("Invalid native frame")),
                    })
                }
                Some("--content-backend") => {
                    backend = Some(
                        args.next()
                            .ok_or("Missing backend")?
                            .to_string_lossy()
                            .into_owned(),
                    )
                }
                _ => {
                    return Err(fail(format!(
                        "Unknown clipping option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        let backend = backend.ok_or("--content-backend required")?;
        require(
            backend == "software" || backend == "opengl",
            "Invalid content backend",
        )?;
        let current = env::current_dir()?;
        let output = output.ok_or("--output required")?;
        let output = if output.is_absolute() {
            output
        } else {
            current.join(output)
        };
        Ok(Self {
            launcher: launcher.ok_or("--launcher required")?.canonicalize()?,
            sway: sway.ok_or("--sway required")?.canonicalize()?,
            output,
            source: source.unwrap_or(current).canonicalize()?,
            pointer: pointer.unwrap_or(PathBuf::from("wlrctl")),
            effects,
            native_frame: native_frame.ok_or("--native-frame required")?,
            backend,
        })
    }
}
struct Managed {
    child: Child,
    argv: Vec<String>,
}
impl Managed {
    fn stop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.child.id() as i32, libc::SIGTERM);
            }
            let end = Instant::now() + Duration::from_secs(5);
            while self.child.try_wait().ok().flatten().is_none() {
                if Instant::now() >= end {
                    unsafe {
                        libc::killpg(self.child.id() as i32, libc::SIGKILL);
                    }
                    break;
                }
                pause(30);
            }
            let _ = self.child.wait();
        }
    }
}
struct Fixture {
    options: Options,
    env: BTreeMap<String, String>,
    runtime: PathBuf,
    qml: PathBuf,
    assets: PathBuf,
    processes: Vec<Managed>,
    pointer: Option<usize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let status=self.processes.iter_mut().map(|p|json!({"argv":p.argv,"returncode":p.child.try_wait().ok().flatten().and_then(|s|s.code())})).collect::<Vec<_>>();
        let _ = save(
            &self.options.output.join("process-status.json"),
            &json!(status),
        );
        for child in self.processes.iter_mut().rev() {
            child.stop();
        }
        let _ = fs::remove_dir_all(&self.runtime);
    }
}
impl Fixture {
    fn new(options: Options) -> Result<Self> {
        fs::create_dir(&options.output)?;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let runtime = env::temp_dir().join(format!("lclip-{}-{nonce}", std::process::id()));
        fs::create_dir(&runtime)?;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
        let mut values = env::vars().collect::<BTreeMap<_, _>>();
        values.insert("HOME".into(), runtime.display().to_string());
        values.insert("XDG_RUNTIME_DIR".into(), runtime.display().to_string());
        for (key, dir) in [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_DATA_HOME", "data"),
        ] {
            values.insert(key.into(), runtime.join(dir).display().to_string());
        }
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
        ] {
            values.remove(key);
        }
        let raw = Command::new(&options.launcher)
            .arg("--print-env0")
            .env_clear()
            .envs(&values)
            .output()?;
        require(raw.status.success(), "Launcher environment failed")?;
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
            ("QT_QUICK_BACKEND", "software"),
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
            ("LUMEN_NATIVE_WINDOW_CONTROLS", "1"),
            ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
            (
                "LUMEN_UNIFIED_OWNED_HEADERS",
                if options.native_frame { "0" } else { "1" },
            ),
        ] {
            values.insert(key.into(), value.into());
        }
        if options.backend == "opengl" {
            values.remove("QT_QUICK_BACKEND");
            values.insert("QSG_RHI_BACKEND".into(), "opengl".into());
        }
        let assets = options.output.join("assets");
        copy_tree(&options.source.join("res/lumen-desktop"), &assets)?;
        let appearance = options.output.join("appearance.json");
        save(
            &appearance,
            &json!({"font":"Interlude Variable","palette":{"background":"#f2f2f4","surface":"#ffffff","field":"#e5e5e8","text":"#202023","muted":"#626267","border":"#c8c8ce","hover":"#dedee3","accent":"#007aff","selectedText":"#000000"},"preferences":{"mode":"light","reduceMotion":true,"reduceTransparency":true}}),
        )?;
        values.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            appearance.display().to_string(),
        );
        let qml = options.output.join("shell.qml");
        fs::write(&qml, include_str!("../fixture.qml"))?;
        let config = options.output.join("sway.conf");
        let mut text = format!(
            "font sans 10\noutput * resolution 1920x1440\noutput HEADLESS-1 position 0 0 scale 1\noutput HEADLESS-2 resolution 1000x700 position 1920 0 scale 1.25\noutput * bg #29445b solid_color\ndefault_border pixel 0\ndefault_floating_border pixel 0\nfor_window [all] floating enable\nfor_window [title=\"^System Settings$\"] border {}\n",
            if options.native_frame {
                "normal 1"
            } else {
                "pixel 0"
            }
        );
        if options.effects {
            text.push_str("corner_radius 12\nshadows enable\nanimation_duration_ms 0\n");
        }
        fs::write(config, text)?;
        Ok(Self {
            options,
            env: values,
            runtime,
            qml,
            assets,
            processes: Vec::new(),
            pointer: None,
        })
    }
    fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut c = Command::new(program);
        c.args(args).env_clear().envs(&self.env);
        c
    }
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = self.command(program, args).output()?;
        require(
            output.status.success(),
            format!(
                "{program} {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn launch(&mut self, program: &str, args: &[&str], log: &str, pipe: bool) -> Result<usize> {
        let file = File::create(self.options.output.join(log))?;
        let mut c = self.command(program, args);
        c.stdout(file.try_clone()?).stderr(file);
        if pipe {
            c.stdin(Stdio::piped());
        }
        unsafe {
            c.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let argv = std::iter::once(program.to_owned())
            .chain(args.iter().map(|s| s.to_string()))
            .collect();
        self.processes.push(Managed {
            child: c.spawn()?,
            argv,
        });
        Ok(self.processes.len() - 1)
    }
    fn start_bus(&mut self) -> Result<()> {
        self.env.insert(
            "DBUS_SYSTEM_BUS_ADDRESS".into(),
            format!("unix:path={}", self.runtime.join("no-system-bus").display()),
        );
        let config = self.options.output.join("bus.conf");
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
                self.runtime.display()
            ),
        )?;
        let log = File::create(self.options.output.join("bus.log"))?;
        let mut command = self.command(
            "dbus-daemon",
            &[
                "--nofork",
                "--print-address=1",
                &format!("--config-file={}", config.display()),
            ],
        );
        command.stdout(Stdio::piped()).stderr(log);
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let mut child = command.spawn()?;
        let mut address = String::new();
        BufReader::new(child.stdout.as_mut().ok_or("Bus stdout")?).read_line(&mut address)?;
        require(
            address.starts_with("unix:"),
            "Private bus did not publish an address",
        )?;
        self.processes.push(Managed {
            child,
            argv: vec![
                "dbus-daemon".into(),
                "--nofork".into(),
                "--print-address=1".into(),
                format!("--config-file={}", config.display()),
            ],
        });
        self.env.insert(
            "DBUS_SESSION_BUS_ADDRESS".into(),
            address.trim().to_string(),
        );
        Ok(())
    }
    fn ipc(&self, command: &str) -> Result<()> {
        let answer: Value = serde_json::from_str(&self.call("swaymsg", &[command])?)?;
        require(
            answer
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
            format!("Sway IPC failed: {answer}"),
        )
    }
    fn shell(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut argv = vec![
            "-p",
            self.qml.to_str().ok_or("QML path")?,
            "ipc",
            "call",
            "clipping",
            method,
        ];
        argv.extend(args);
        self.call("quickshell", &argv)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.shell("state", &[])?)?)
    }
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-t", "get_tree", "-r"])?,
        )?)
    }
    fn window(&self, title: &str) -> Result<Option<Value>> {
        let tree = self.tree()?;
        let mut nodes = Vec::new();
        all_nodes(&tree, &mut nodes);
        Ok(nodes
            .into_iter()
            .find(|n| n["name"] == title && n["app_id"].as_str().is_some_and(|s| !s.is_empty()))
            .cloned())
    }
    fn matching(&self, title: &str) -> Result<Vec<Value>> {
        let tree = self.tree()?;
        let mut nodes = Vec::new();
        all_nodes(&tree, &mut nodes);
        Ok(nodes
            .into_iter()
            .filter(|n| n["app_id"] == "org.quickshell" && n["name"] == title)
            .cloned()
            .collect())
    }
    fn outputs(&self) -> Result<Value> {
        let value: Value =
            serde_json::from_str(&self.call("swaymsg", &["-t", "get_outputs", "-r"])?)?;
        Ok(value)
    }
    fn output(&self, name: &str) -> Result<Value> {
        self.outputs()?
            .as_array()
            .ok_or("Output list")?
            .iter()
            .find(|row| row["name"] == name && row["active"] == true)
            .cloned()
            .ok_or_else(|| fail(format!("Missing output {name}")))
    }
    fn pointer_command(&mut self, text: &str) -> Result<()> {
        let index = self.pointer.ok_or("Missing pointer")?;
        let stdin = self.processes[index]
            .child
            .stdin
            .as_mut()
            .ok_or("Pointer stdin")?;
        write!(stdin, "{text}")?;
        stdin.flush()?;
        Ok(())
    }
    fn click(&mut self, x: f64, y: f64) -> Result<()> {
        self.ipc(&format!(
            "seat seat0 cursor set {} {}",
            x.round() as i64,
            y.round() as i64
        ))?;
        self.pointer_command("m 0 0\np\nr\n")
    }
    fn start(&mut self) -> Result<()> {
        self.start_bus()?;
        let sway = self.options.sway.display().to_string();
        let config = self.options.output.join("sway.conf");
        self.launch(
            &sway,
            &[
                "-d",
                "--unsupported-gpu",
                "-c",
                config.to_str().ok_or("Config path")?,
            ],
            "compositor.log",
            false,
        )?;
        let socket = wait("Sway socket", || {
            Ok(fs::read_dir(&self.runtime)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .find(|p| {
                    p.file_name().is_some_and(|n| {
                        n.to_string_lossy().starts_with("sway-ipc.")
                            && n.to_string_lossy().ends_with(".sock")
                    })
                }))
        })?;
        self.env
            .insert("SWAYSOCK".into(), socket.display().to_string());
        wait("two outputs", || {
            Ok((self
                .outputs()?
                .as_array()
                .is_some_and(|rows| rows.iter().filter(|r| r["active"] == true).count() == 2))
            .then_some(()))
        })?;
        self.ipc("focus output HEADLESS-1")?;
        let display = wait("Wayland display", || {
            Ok(fs::read_dir(&self.runtime)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .find(|p| {
                    p.file_name().is_some_and(|n| {
                        n.to_string_lossy().starts_with("wayland-")
                            && !n.to_string_lossy().ends_with(".lock")
                    })
                }))
        })?;
        self.env.insert(
            "WAYLAND_DISPLAY".into(),
            display
                .file_name()
                .ok_or("Display filename")?
                .to_string_lossy()
                .into_owned(),
        );
        self.ipc("lumen_window margins disable")?;
        let qml = self.qml.display().to_string();
        self.launch("quickshell", &["-p", &qml], "shell.log", false)?;
        wait("clipping ready", || {
            Ok((self.shell("ready", &[])? == "true").then_some(()))
        })?;
        wait("graphics API", || {
            Ok((self.state()?["graphicsApi"]
                .as_i64()
                .is_some_and(|v| v != 0))
            .then_some(()))
        })?;
        let pointer = self.options.pointer.display().to_string();
        self.pointer = Some(self.launch(
            &pointer,
            &["pointer", "move", "0", "0"],
            "pointer.log",
            true,
        )?);
        self.pointer_command("m 0 0\n")?;
        self.launch("wtype", &["-s", "60000"], "keyboard.log", false)?;
        let native = self.options.native_frame;
        wait("settings window", || {
            Ok(self
                .window("System Settings")?
                .filter(|w| !native || w["deco_rect"]["height"].as_i64().is_some_and(|h| h > 0)))
        })?;
        Ok(())
    }
}

struct Capture {
    width: usize,
    height: usize,
    channels: usize,
    data: Vec<u8>,
}
impl Capture {
    fn read(path: &Path) -> Result<Self> {
        let mut decoder = png::Decoder::new(File::open(path)?);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut data = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut data)?;
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
        data.truncate(info.buffer_size());
        Ok(Self {
            width: info.width as usize,
            height: info.height as usize,
            channels,
            data,
        })
    }
    fn pixel(&self, x: i64, y: i64) -> Result<[u8; 3]> {
        require(
            x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height,
            "Pixel outside capture",
        )?;
        let index = (y as usize * self.width + x as usize) * self.channels;
        Ok([self.data[index], self.data[index + 1], self.data[index + 2]])
    }
}
fn vivid(pixel: [u8; 3]) -> bool {
    [[255, 0, 255], [0, 255, 0], [0, 255, 255]]
        .into_iter()
        .any(|color| {
            pixel
                .iter()
                .zip(color)
                .all(|(actual, expected)| (*actual as i16 - expected as i16).abs() <= 3)
        })
}
fn pixel_match(actual: [u8; 3], expected: [u8; 3]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, b)| (*a as i16 - b as i16).abs() <= 3)
}
impl Fixture {
    fn record_case(
        &mut self,
        scale: f64,
        width: i64,
        height: i64,
        bottom: bool,
        identity: i64,
        frame_radius: f64,
    ) -> Result<Value> {
        self.shell("scroll", &[if bottom { "true" } else { "false" }])?;
        let current = self
            .window("System Settings")?
            .ok_or("Settings window missing")?;
        require(
            integer(&current["id"])? == identity,
            "Settings identity changed",
        )?;
        let status = self.state()?;
        require(
            boolean(&status["software"])? == (self.options.backend == "software"),
            format!("Graphics backend mismatch: {status}"),
        )?;
        require(
            boolean(&status["maskActive"])?
                == (!self.options.native_frame && self.options.backend == "opengl"),
            format!("Mask activation mismatch: {status}"),
        )?;
        let display = self.output("HEADLESS-1")?;
        let box_rect = &current["rect"];
        let screen = &display["rect"];
        let client = &current["window_rect"];
        let surface = &status["surface"];
        let image = self.options.output.join(format!(
            "edge-{scale}-{width}-{}.png",
            if bottom { 1 } else { 0 }
        ));
        let image_text = image.to_str().ok_or("Image path")?;
        let origin_x = number(&box_rect["x"])? + number(&client["x"])? + number(&surface["x"])?
            - number(&screen["x"])?;
        let origin_y = number(&box_rect["y"])? + number(&client["y"])? + number(&surface["y"])?
            - number(&screen["y"])?;
        let surface_width = number(&surface["width"])?;
        let surface_height = number(&surface["height"])?;
        let painted_samples = wait("painted edges", || {
            self.call(
                "grim",
                &["-o", "HEADLESS-1", "-s", &scale.to_string(), image_text],
            )?;
            let capture = Capture::read(&image)?;
            let mut samples = Vec::new();
            for (x, y) in [
                (4.0, surface_height / 2.0),
                (surface_width - 4.0, surface_height / 2.0),
                (surface_width / 2.0, 4.0),
                (surface_width / 2.0, surface_height - 4.0),
            ] {
                let col = ((x + number(&status["scroll"]["x"])?) / 128.0).floor() as i64;
                let row = ((y + number(&status["scroll"]["y"])?) / 128.0).floor() as i64;
                let expected = match (col + row).rem_euclid(3) {
                    0 => [255, 0, 255],
                    1 => [0, 255, 0],
                    _ => [0, 255, 255],
                };
                let px = ((origin_x + x) * scale).round_ties_even() as i64;
                let py = ((origin_y + y) * scale).round_ties_even() as i64;
                let actual = capture.pixel(px, py)?;
                if !pixel_match(actual, expected) {
                    return Ok(None);
                }
                samples.push(json!({"pixel":[px,py],"rgb":actual}));
            }
            Ok(Some(samples))
        })?;
        let capture = Capture::read(&image)?;
        require(
            ((capture.width as i64) - integer(&display["current_mode"]["width"])?).abs() <= 1
                && ((capture.height as i64) - integer(&display["current_mode"]["height"])?).abs()
                    <= 1,
            "Capture dimensions differ from output",
        )?;
        let radius = if boolean(&status["maskActive"])? {
            number(&status["maskRadius"])?
        } else {
            0.0
        };
        let left = raster_origin(origin_x, scale)?;
        let right = raster_origin(origin_x + surface_width, scale)?;
        let top = raster_origin(origin_y, scale)?;
        let bottom_pixel = raster_origin(origin_y + surface_height, scale)?;
        let band = (radius.max(frame_radius) * scale).ceil() as i64 + 4;
        let mut coordinates = HashSet::<(i64, i64)>::new();
        for py in (top - 4)..(top + band) {
            for px in (left - 4)..(right + 4) {
                coordinates.insert((px, py));
            }
        }
        for py in (bottom_pixel - band)..(bottom_pixel + 4) {
            for px in (left - 4)..(right + 4) {
                coordinates.insert((px, py));
            }
        }
        for px in (left - 4)..(left + band) {
            for py in (top - 4)..(bottom_pixel + 4) {
                coordinates.insert((px, py));
            }
        }
        for px in (right - band)..(right + 4) {
            for py in (top - 4)..(bottom_pixel + 4) {
                coordinates.insert((px, py));
            }
        }
        let mut painted = 0usize;
        let mut violations = Vec::<Value>::new();
        for (px, py) in coordinates {
            if px < 0 || py < 0 || px >= capture.width as i64 || py >= capture.height as i64 {
                continue;
            }
            if !vivid(capture.pixel(px, py)?) {
                continue;
            }
            painted += 1;
            let x = (px as f64 + 0.5 - left as f64) / scale;
            let y = (py as f64 + 0.5 - top as f64) / scale;
            let mut outside =
                outside_frame(x, y, surface_width, surface_height, radius, 1.0 / scale)
                    .map_err(|e| fail(format!("Frame oracle: {e:?}")))?;
            if self.options.effects {
                let outer_x = number(&box_rect["x"])? - number(&screen["x"])?;
                let outer_y = number(&box_rect["y"])?
                    - number(&current["deco_rect"]["height"])?
                    - number(&screen["y"])?;
                let outer_width = number(&box_rect["width"])?;
                let outer_height =
                    number(&box_rect["height"])? + number(&current["deco_rect"]["height"])?;
                let fx = (px as f64 + 0.5 - raster_origin(outer_x, scale)? as f64) / scale;
                let fy = (py as f64 + 0.5 - raster_origin(outer_y, scale)? as f64) / scale;
                outside |=
                    outside_frame(fx, fy, outer_width, outer_height, frame_radius, 1.0 / scale)
                        .map_err(|e| fail(format!("Compositor frame oracle: {e:?}")))?;
            }
            if outside && violations.len() < 30 {
                violations.push(json!([px, py]));
            }
        }
        let mut record = json!({"scale":scale,"size":[width,height],"bottom":bottom,"status":status,"physicalOrigin":[left,top],"compositorRadius":frame_radius,"window":current,"output":display,"capture":image.display().to_string(),"painted":painted,"paintedSamples":painted_samples,"violations":violations});
        save(&self.options.output.join("last-state.json"), &record)?;
        require(
            painted as f64 > 100.0 * scale * scale,
            format!(
                "Insufficient vivid pixels: {}",
                serde_json::to_string(&record)?
            ),
        )?;
        require(
            violations.is_empty(),
            format!(
                "Vivid content outside frame: {}",
                serde_json::to_string(&record)?
            ),
        )?;
        let target = &status["search"];
        require(!target.is_null(), "Search field unavailable")?;
        self.click(
            number(&box_rect["x"])?
                + number(&client["x"])?
                + number(&target["x"])?
                + number(&target["width"])? / 2.0,
            number(&box_rect["y"])?
                + number(&client["y"])?
                + number(&target["y"])?
                + number(&target["height"])? / 2.0,
        )?;
        wait("search focus", || {
            Ok((self.state()?["searchFocused"] == true).then_some(()))
        })?;
        self.call("wtype", &["-M", "ctrl", "-k", "a", "-m", "ctrl", "edge"])?;
        wait("search typing", || {
            Ok((self.state()?["query"] == "edge").then_some(()))
        })?;
        self.call(
            "wtype",
            &["-M", "ctrl", "-k", "a", "-m", "ctrl", "-k", "BackSpace"],
        )?;
        wait("search clearing", || {
            Ok((self.state()?["query"] == "").then_some(()))
        })?;
        if !bottom {
            self.shell("resetViewport", &[])?;
            let viewport = self.state()?["viewport"].clone();
            let scroll_needed =
                number(&viewport["contentHeight"])? > number(&viewport["rect"]["height"])?;
            let target_x = number(&viewport["rect"]["x"])? + 10.0;
            let target_y =
                number(&viewport["rect"]["y"])? + number(&viewport["rect"]["height"])? / 2.0;
            self.ipc(&format!(
                "seat seat0 cursor set {} {}",
                (origin_x + target_x).round_ties_even() as i64,
                (origin_y + target_y).round_ties_even() as i64
            ))?;
            self.pointer_command("m 0 0\n")?;
            wait("viewport hover", || {
                let state = self.state()?;
                let v = &state["viewport"];
                Ok((v["hovered"] == true
                    && (number(&v["pointerX"])? - 10.0).abs() <= 1.0
                    && (number(&v["pointerY"])? - number(&viewport["rect"]["height"])? / 2.0)
                        .abs()
                        <= 1.0)
                    .then_some(()))
            })?;
            self.pointer_command("w 1\n")?;
            if scroll_needed {
                wait("viewport scroll", || {
                    Ok((number(&self.state()?["viewport"]["y"])? > 0.0).then_some(()))
                })?;
            } else {
                require(
                    number(&self.state()?["viewport"]["y"])? == 0.0,
                    "Viewport scrolled without overflow",
                )?;
            }
            record["productionScrollInput"] = json!({"overflow":scroll_needed,"before":viewport,"after":self.state()?["viewport"]});
        }
        Ok(record)
    }
}

impl Fixture {
    fn fit_proof(&mut self, identity: i64) -> Result<Value> {
        let title = "System Settings";
        let before = self.window(title)?.ok_or("Settings window vanished")?;
        let pad_width =
            integer(&before["rect"]["width"])? - integer(&before["window_rect"]["width"])?;
        let pad_height = integer(&before["rect"]["height"])?
            + integer(&before["deco_rect"]["height"])?
            - integer(&before["window_rect"]["height"])?;
        self.ipc(&format!(
            "[con_id={identity}] resize set width {} px height {} px",
            1200 + pad_width,
            900 + pad_height
        ))?;
        wait("oversized client", || {
            let state = self.state()?;
            let output = self.output("HEADLESS-1")?;
            Ok(
                ((integer(&state["width"])? + pad_width > integer(&output["rect"]["width"])? - 32)
                    && (integer(&state["height"])? + pad_height
                        > integer(&output["rect"]["height"])? - 150))
                    .then_some(()),
            )
        })?;
        let completed = integer(&self.state()?["fit"]["completed"])?;
        self.shell("fitRepeated", &[])?;
        wait("first fit", || {
            let state = self.state()?;
            Ok(
                (integer(&state["fit"]["completed"])? > completed && state["fit"]["busy"] == false)
                    .then_some(()),
            )
        })?;
        let fitted = self.state()?;
        require(
            fitted["fit"]["error"] == "",
            format!("Fit failed: {fitted}"),
        )?;
        let outcome = &fitted["fit"]["result"];
        require(
            outcome["supported"] == true && outcome["changed"] == true,
            "First fit did not change oversized client",
        )?;
        wait("fit size", || {
            let state = self.state()?;
            Ok((state["width"] == outcome["clientWidth"]
                && state["height"] == outcome["clientHeight"])
                .then_some(()))
        })?;
        let fitted = self.state()?;
        require(
            number(&outcome["outerWidth"])? <= number(&outcome["budgetWidth"])?
                && number(&outcome["outerHeight"])? <= number(&outcome["budgetHeight"])?,
            "Fit exceeds outer budget",
        )?;
        let after_fit = self.window(title)?.ok_or("Fitted window missing")?;
        let completed = integer(&fitted["fit"]["completed"])?;
        self.shell("fitRepeated", &[])?;
        wait("repeat fit", || {
            let state = self.state()?;
            Ok(
                (integer(&state["fit"]["completed"])? > completed && state["fit"]["busy"] == false)
                    .then_some(()),
            )
        })?;
        let stable = self.state()?;
        require(
            stable["fit"]["error"] == "" && stable["fit"]["result"]["changed"] == false,
            "Repeat fit was not stable",
        )?;
        require(
            integer(&stable["fit"]["completed"])? == completed + 1,
            "Repeated fit count changed unexpectedly",
        )?;
        require(
            self.window(title)?.ok_or("Stable window missing")?["rect"] == after_fit["rect"],
            "Repeated fit moved window",
        )?;
        let mut proof = json!({"first":fitted,"repeat":stable,"window":after_fit});
        let before_move = self.state()?;
        let completed = integer(&before_move["fit"]["completed"])?;
        self.ipc(&format!(
            "[con_id={identity}] move container to output HEADLESS-2"
        ))?;
        wait("fit smaller output", || {
            let state = self.state()?;
            Ok((state["screen"] == "HEADLESS-2"
                && integer(&state["fit"]["completed"])? > completed
                && state["fit"]["busy"] == false)
                .then_some(()))
        })?;
        let small = self.state()?;
        require(small["fit"]["error"] == "", "Smaller output fit failed")?;
        let small_outcome = &small["fit"]["result"];
        require(
            integer(&small["width"])? < integer(&before_move["width"])?
                || integer(&small["height"])? < integer(&before_move["height"])?,
            "Smaller output did not shrink client",
        )?;
        wait("smaller fit size", || {
            let state = self.state()?;
            Ok((state["width"] == small_outcome["clientWidth"]
                && state["height"] == small_outcome["clientHeight"])
                .then_some(()))
        })?;
        let output = self.output("HEADLESS-2")?;
        require(
            number(&small_outcome["outerWidth"])? <= number(&output["rect"]["width"])? - 32.0
                && number(&small_outcome["outerHeight"])?
                    <= number(&output["rect"]["height"])? - 150.0,
            "Smaller output exceeds budget",
        )?;
        let small = self.state()?;
        let small_window = self.window(title)?.ok_or("Smaller output window missing")?;
        require(
            integer(&small_window["id"])? == identity,
            "Window identity changed on smaller output",
        )?;
        self.call(
            "grim",
            &[
                "-o",
                "HEADLESS-2",
                "-s",
                "1.25",
                self.options
                    .output
                    .join("fit-small-output.png")
                    .to_str()
                    .ok_or("Screenshot path")?,
            ],
        )?;
        let completed = integer(&small["fit"]["completed"])?;
        self.ipc(&format!(
            "[con_id={identity}] move container to output HEADLESS-1"
        ))?;
        wait("return output fit", || {
            let state = self.state()?;
            Ok((state["screen"] == "HEADLESS-1"
                && integer(&state["fit"]["completed"])? > completed
                && state["fit"]["busy"] == false)
                .then_some(()))
        })?;
        let returned = self.state()?;
        require(returned["fit"]["error"] == "", "Return output fit failed")?;
        require(
            returned["width"] == small["width"] && returned["height"] == small["height"],
            "Return output changed client size",
        )?;
        proof["smallerOutput"] = json!({"state":small,"window":small_window});
        proof["returnOutput"] = returned;
        self.ipc("focus output HEADLESS-1")?;
        self.shell("auxiliary", &[title, "0"])?;
        wait("ambiguous duplicate window", || {
            let rows = self.matching(title)?;
            let state = self.state()?;
            Ok((rows.len() == 2
                && rows.iter().filter(|row| row["id"] != identity).all(|row| {
                    row["window_rect"]["width"] == state["auxiliary"]["width"]
                        && row["window_rect"]["height"] == state["auxiliary"]["height"]
                }))
            .then_some(()))
        })?;
        let matching = self.matching(title)?;
        let duplicate_id = integer(
            &matching
                .iter()
                .find(|row| row["id"] != identity)
                .ok_or("Duplicate ID missing")?["id"],
        )?;
        let before_ambiguous = matching
            .iter()
            .map(|row| (integer(&row["id"]).unwrap_or_default(), row["rect"].clone()))
            .collect::<BTreeMap<_, _>>();
        let completed = integer(&self.state()?["fit"]["completed"])?;
        self.shell("fitRepeated", &[])?;
        wait("ambiguous fit rejection", || {
            let state = self.state()?;
            Ok(
                (integer(&state["fit"]["completed"])? > completed && state["fit"]["busy"] == false)
                    .then_some(()),
            )
        })?;
        let after_ambiguous = self
            .matching(title)?
            .iter()
            .map(|row| (integer(&row["id"]).unwrap_or_default(), row["rect"].clone()))
            .collect::<BTreeMap<_, _>>();
        let ambiguous_state = self.state()?;
        fs::write(
            self.options.output.join("ambiguous-fit-decision.json"),
            serde_json::to_vec_pretty(&json!({"state":ambiguous_state,"before":before_ambiguous,"after":after_ambiguous,"original":identity,"duplicate":duplicate_id}))?,
        )?;
        require(
            ambiguous_state["fit"]["error"]
                .as_str()
                .is_some_and(|e| !e.is_empty())
                && after_ambiguous == before_ambiguous,
            "Ambiguous duplicate was not rejected",
        )?;
        self.shell("hideAuxiliary", &[])?;
        wait("hide duplicate", || {
            Ok((self.matching(title)?.len() == 1).then_some(()))
        })?;
        let literal_title = "Literal \"quote\"; [x] \\ café — Ω\nline";
        self.shell("auxiliary", &[literal_title, &duplicate_id.to_string()])?;
        let literal = wait("literal title window", || {
            Ok(self.window(literal_title)?.filter(|window| {
                let state = self.state().ok();
                state.is_some_and(|state| {
                    window["window_rect"]["width"] == state["auxiliary"]["width"]
                        && window["window_rect"]["height"] == state["auxiliary"]["height"]
                })
            }))
        })?;
        require(
            integer(&literal["id"])? != duplicate_id,
            "Retired ID reused unexpectedly",
        )?;
        let before_literal = literal["rect"].clone();
        let completed = integer(&self.state()?["auxiliary"]["completed"])?;
        self.shell("fitAuxiliary", &[])?;
        wait("retired ID fit rejection", || {
            let state = self.state()?;
            Ok((integer(&state["auxiliary"]["completed"])? > completed
                && state["auxiliary"]["busy"] == false)
                .then_some(()))
        })?;
        require(
            self.state()?["auxiliary"]["error"]
                .as_str()
                .is_some_and(|e| !e.is_empty())
                && self
                    .window(literal_title)?
                    .ok_or("Literal window missing")?["rect"]
                    == before_literal,
            "Retired container was not rejected",
        )?;
        self.shell("auxiliary", &[literal_title, "0"])?;
        let completed = integer(&self.state()?["auxiliary"]["completed"])?;
        self.shell("fitAuxiliary", &[])?;
        wait("literal title fit", || {
            let state = self.state()?;
            Ok((integer(&state["auxiliary"]["completed"])? > completed
                && state["auxiliary"]["busy"] == false)
                .then_some(()))
        })?;
        let literal_proof = self.state()?["auxiliary"].clone();
        require(
            literal_proof["error"] == "" && literal_proof["result"]["changed"] == true,
            "Literal title fit failed",
        )?;
        wait("literal title fit dimensions", || {
            let state = self.state()?;
            Ok(
                (state["auxiliary"]["width"] == literal_proof["result"]["clientWidth"]
                    && state["auxiliary"]["height"] == literal_proof["result"]["clientHeight"])
                    .then_some(()),
            )
        })?;
        require(
            literal_proof["result"]["pid"]
                == self
                    .window(literal_title)?
                    .ok_or("Literal window missing")?["pid"],
            "Literal title PID mismatch",
        )?;
        self.shell("hideAuxiliary", &[])?;
        wait("hide literal title", || {
            Ok(self.window(literal_title)?.is_none().then_some(()))
        })?;
        proof["ambiguousRejected"] = json!(true);
        proof["retiredContainerRejected"] = json!(true);
        proof["literalTitle"] = literal_proof;
        Ok(proof)
    }
}

fn run(options: Options) -> Result<()> {
    let mut fixture = Fixture::new(options)?;
    fixture.start()?;
    let title = "System Settings";
    let initial = fixture.window(title)?.ok_or("Settings window missing")?;
    let identity = integer(&initial["id"])?;
    let frame_radius = if fixture.options.effects { 12.0 } else { 0.0 };
    let mut records = Vec::<Value>::new();
    for scale in [1.0, 1.25, 1.5, 2.0] {
        fixture.ipc(&format!("output HEADLESS-1 scale {scale}"))?;
        for (width, height) in [(790, 520), (520, 360)] {
            let before = fixture.window(title)?.ok_or("Settings window missing")?;
            let padding_width =
                integer(&before["rect"]["width"])? - integer(&before["window_rect"]["width"])?;
            let padding_height = integer(&before["rect"]["height"])?
                + integer(&before["deco_rect"]["height"])?
                - integer(&before["window_rect"]["height"])?;
            fixture.ipc(&format!(
                "[con_id={identity}] resize set width {} px height {} px",
                width + padding_width,
                height + padding_height
            ))?;
            fixture.ipc(&format!("[con_id={identity}] move position center"))?;
            wait("window client size", || {
                let state = fixture.state()?;
                Ok((state["width"] == width && state["height"] == height).then_some(()))
            })?;
            for bottom in [false, true] {
                records.push(fixture.record_case(
                    scale,
                    width,
                    height,
                    bottom,
                    identity,
                    frame_radius,
                )?);
            }
        }
    }
    let fit_proof = fixture.fit_proof(identity)?;
    let current = fixture
        .window(title)?
        .ok_or("Settings window missing before close")?;
    let status = fixture.state()?;
    let box_rect = &current["rect"];
    let client = &current["window_rect"];
    let close_point = if fixture.options.native_frame {
        vec![
            number(&box_rect["x"])? + 20.0,
            number(&box_rect["y"])? - number(&current["deco_rect"]["height"])? / 2.0,
        ]
    } else {
        let button = &status["closeControl"];
        require(!button.is_null(), "Close control missing")?;
        vec![
            number(&box_rect["x"])?
                + number(&client["x"])?
                + number(&button["x"])?
                + number(&button["width"])? / 2.0,
            number(&box_rect["y"])?
                + number(&client["y"])?
                + number(&button["y"])?
                + number(&button["height"])? / 2.0,
        ]
    };
    fixture.click(close_point[0], close_point[1])?;
    wait("close settings window", || {
        Ok(fixture.window(title)?.is_none().then_some(()))
    })?;
    let source = fixture.call(
        "git",
        &[
            "-C",
            fixture.options.source.to_str().ok_or("Source path")?,
            "rev-parse",
            "HEAD",
        ],
    )?;
    let result = json!({"passed":true,"cases":records,"source":source,"fixtureSha256":hash(include_bytes!("main.rs")),"closePoint":close_point,"closePassed":true,"fitProof":fit_proof,
        "launcher":fixture.options.launcher.display().to_string(),"compositor":fixture.options.sway.display().to_string(),"pointer":fixture.options.pointer.display().to_string(),
        "backend":fixture.options.backend,"nativeFrame":if fixture.options.native_frame{"on"}else{"off"},"effects":fixture.options.effects,"assetsSha256":asset_hashes(&fixture.assets)?});
    save(&fixture.options.output.join("result.json"), &result)?;
    println!(
        "PASS native content clipping {} {} {}",
        if fixture.options.effects {
            "effects"
        } else {
            "basic"
        },
        fixture.options.backend,
        if fixture.options.native_frame {
            "on"
        } else {
            "off"
        }
    );
    Ok(())
}
fn main() -> Result<()> {
    run(Options::parse()?)
}
