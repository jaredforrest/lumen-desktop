//! Private native icon catalogue: four output scales and six appearance profiles.
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::BufRead;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn error(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(error(message)) }
}
fn text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn number() -> Result<u128> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos())
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn wait<T>(name: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(25);
    loop {
        if let Ok(Some(value)) = probe() {
            return Ok(value);
        }
        if Instant::now() >= end {
            return Err(error(format!("UI readiness timed out: {name}")));
        }
        pause(100);
    }
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let dest = target.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &dest)?;
        } else if path.is_file() {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}
fn read_env0(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for row in bytes.split(|v| *v == 0) {
        if let Some(i) = row.iter().position(|v| *v == b'=') {
            map.insert(
                String::from_utf8(row[..i].to_vec())?,
                String::from_utf8(row[i + 1..].to_vec())?,
            );
        }
    }
    Ok(map)
}
#[derive(Clone)]
struct Opt {
    assets: PathBuf,
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    inside: bool,
}
impl Opt {
    fn parse() -> Result<Self> {
        let mut assets = None;
        let mut output = None;
        let mut launcher = None;
        let mut sway = PathBuf::from("sway");
        let mut inside = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--assets") => {
                    assets = Some(PathBuf::from(args.next().ok_or("--assets value")?))
                }
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("--output value")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("--launcher value")?))
                }
                Some("--sway") => sway = PathBuf::from(args.next().ok_or("--sway value")?),
                Some("--inside") => inside = true,
                _ => return Err(error(format!("unknown option: {}", arg.to_string_lossy()))),
            }
        }
        let cwd = env::current_dir()?;
        let absolute = |p: PathBuf| if p.is_absolute() { p } else { cwd.join(p) };
        Ok(Self {
            assets: absolute(assets.ok_or("--assets required")?).canonicalize()?,
            output: absolute(output.ok_or("--output required")?),
            launcher: absolute(launcher.ok_or("--launcher required")?).canonicalize()?,
            sway: if sway == Path::new("sway") {
                sway
            } else {
                absolute(sway)
            },
            inside,
        })
    }
}
fn private_entry(opt: &Opt) -> Result<()> {
    fs::create_dir(&opt.output)?;
    let runtime = env::temp_dir().join(format!("lumen-icons-{}-{}", std::process::id(), number()?));
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let result = (|| -> Result<()> {
        let config = opt.output.join("bus.conf");
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
                runtime.display()
            ),
        )?;
        write_json(
            &opt.output.join("bootstrap.json"),
            &json!({"privateHome":true,"privateXdg":true,"ambientSessionRemoved":true,"activationDirectories":false,"systemBus":"inert","scope":"Disposable nested session only"}),
        )?;
        let mut child = Command::new("dbus-run-session");
        child
            .arg(format!("--config-file={}", config.display()))
            .arg("--")
            .arg(env::current_exe()?)
            .args(env::args_os().skip(1))
            .arg("--inside");
        child
            .env("HOME", runtime.join("home"))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("QT_ACCESSIBILITY", "0")
            .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0")
            .env("NO_AT_BRIDGE", "1")
            .env("GTK_A11Y", "none")
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}/no-system-bus", runtime.display()),
            )
            .env(
                "PULSE_SERVER",
                format!("unix:{}/no-pulse", runtime.display()),
            )
            .env("PIPEWIRE_REMOTE", "no-pipewire");
        fs::create_dir(runtime.join("home"))?;
        for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
            let dir = runtime.join(key.to_lowercase());
            fs::create_dir(&dir)?;
            child.env(format!("XDG_{key}_HOME"), dir);
        }
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
        ] {
            child.env_remove(key);
        }
        require(child.status()?.success(), "private icon fixture failed")
    })();
    let _ = fs::remove_dir_all(&runtime);
    result
}
fn bootstrap_check() -> Result<()> {
    let root = env::temp_dir().join(format!(
        "lumen-icon-bootstrap-{}-{}",
        std::process::id(),
        number()?
    ));
    fs::create_dir(&root)?;
    let result = (|| -> Result<()> {
        let bin = root.join("bin");
        fs::create_dir(&bin)?;
        symlink(env::current_exe()?, bin.join("dbus-run-session"))?;
        let output = root.join("output");
        let capture = root.join("capture.json");
        let host = root.join("host-home");
        let mut child = Command::new(env::current_exe()?);
        child
            .arg("--assets")
            .arg(&root)
            .arg("--output")
            .arg(&output)
            .arg("--launcher")
            .arg(env::current_exe()?)
            .env("PATH", format!("{}:{}", bin.display(), env::var("PATH")?))
            .env("ICON_BOOTSTRAP_CAPTURE", &capture)
            .env("HOME", &host)
            .env("DISPLAY", ":host")
            .env("WAYLAND_DISPLAY", "host-wayland")
            .env("SWAYSOCK", "host-sway")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/host-session")
            .env("AT_SPI_BUS_ADDRESS", "unix:path=/host-accessibility");
        let outcome = child.output()?;
        require(
            outcome.status.success(),
            format!("outer entry: {}", String::from_utf8_lossy(&outcome.stderr)),
        )?;
        let data: Value = serde_json::from_slice(&fs::read(capture)?)?;
        let values = &data["environment"];
        let runtime = PathBuf::from(values["XDG_RUNTIME_DIR"].as_str().ok_or("runtime")?);
        require(
            data["args"]
                .as_array()
                .ok_or("args")?
                .iter()
                .any(|v| v == "--inside"),
            "missing child marker",
        )?;
        require(
            values["HOME"] != host.to_string_lossy().as_ref(),
            "host home leaked",
        )?;
        for key in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_CACHE_HOME",
            "XDG_DATA_HOME",
        ] {
            require(
                Path::new(values[key].as_str().ok_or("private path")?).starts_with(&runtime),
                format!("{key} escaped runtime"),
            )?;
        }
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
        ] {
            require(values[key].is_null(), format!("{key} leaked"))?;
        }
        require(
            values["DBUS_SYSTEM_BUS_ADDRESS"]
                == format!("unix:path={}/no-system-bus", runtime.display()),
            "system bus not inert",
        )?;
        let config = data["configuration"].as_str().ok_or("configuration")?;
        require(
            config.contains("<auth>EXTERNAL</auth>")
                && !["<include", "<servicedir", "<standard_session_servicedirs"]
                    .iter()
                    .any(|v| config.contains(v)),
            "activation paths present",
        )?;
        require(!runtime.exists(), "private runtime survived")?;
        println!("PASS icon render bootstrap");
        Ok(())
    })();
    let _ = fs::remove_dir_all(root);
    result
}
fn fake_bus() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let config = args
        .iter()
        .find_map(|v| v.strip_prefix("--config-file="))
        .ok_or("config file")?;
    let keys = [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_DATA_HOME",
        "DBUS_SYSTEM_BUS_ADDRESS",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ];
    let mut values = serde_json::Map::new();
    for key in keys {
        values.insert(
            key.into(),
            env::var(key).ok().map_or(Value::Null, Value::String),
        );
    }
    write_json(
        Path::new(&env::var("ICON_BOOTSTRAP_CAPTURE")?),
        &json!({"args":args,"configuration":text(Path::new(config))?,"environment":values}),
    )
}
fn fake_calendar() -> Result<()> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line)?;
    require(
        request["action"] == "list" || request["action"] == "load",
        "unexpected calendar request",
    )?;
    println!(
        "{}",
        json!({"configured":false,"events":[],"monthEvents":[],"days":[],"providers":[],"total":0})
    );
    Ok(())
}
struct Managed(Child);
impl Managed {
    fn stop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let end = Instant::now() + Duration::from_secs(5);
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
    opt: Opt,
    env: BTreeMap<String, String>,
    runtime: PathBuf,
    qml: PathBuf,
    shell: PathBuf,
    processes: Vec<Managed>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for p in self.processes.iter_mut().rev() {
            p.stop();
        }
    }
}
impl Fixture {
    fn new(opt: Opt) -> Result<(Self, Value, Vec<PathBuf>, Vec<String>)> {
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let out = Command::new(&opt.launcher).arg("--print-env0").output()?;
        require(out.status.success(), "launcher environment")?;
        let mut vars = read_env0(&out.stdout)?;
        for (key, value) in [
            ("WLR_BACKENDS", "headless"),
            ("WLR_RENDERER", "pixman"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_QUICK_BACKEND", "software"),
            ("LUMEN_FILE_SERVICE", "0"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
            ("GLOBAL_MENU_ENABLED", "0"),
            ("LUMEN_SEARCH_ROOTS", "[]"),
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
        ] {
            vars.insert(key.into(), value.into());
        }
        vars.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            opt.output.join("appearance.json").display().to_string(),
        );
        vars.insert(
            "XDG_DATA_DIRS".into(),
            vars.get("XDG_DATA_HOME").ok_or("XDG_DATA_HOME")?.clone(),
        );
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "AT_SPI_BUS_ADDRESS",
        ] {
            vars.remove(key);
        }
        let shell = opt.output.join("assets/shell");
        copy_tree(
            opt.assets.parent().ok_or("assets parent")?,
            shell.parent().ok_or("shell parent")?,
        )?;
        let providers = Command::new("lumen-fixture-providers")
            .arg("isolate")
            .arg(&shell)
            .arg(&opt.output)
            .output()?;
        require(
            providers.status.success(),
            format!(
                "provider isolation: {}",
                String::from_utf8_lossy(&providers.stderr)
            ),
        )?;
        let provider_records: Value = serde_json::from_slice(&providers.stdout)?;
        let bin = opt.output.join("bin");
        fs::create_dir(&bin)?;
        symlink(env::current_exe()?, bin.join("lumen-calendar"))?;
        vars.insert(
            "PATH".into(),
            format!("{}:{}", bin.display(), vars.get("PATH").ok_or("PATH")?),
        );
        let apps =
            Path::new(vars.get("XDG_DATA_HOME").ok_or("XDG_DATA_HOME")?).join("applications");
        fs::create_dir_all(&apps)?;
        for (name, icon) in [
            ("Calendar", "calendar"),
            ("Notes", "notes"),
            ("Reminders", "reminders"),
        ] {
            fs::write(
                apps.join(format!("org.example.{name}.desktop")),
                format!(
                    "[Desktop Entry]\nType=Application\nName={name}\nExec=false\nIcon={}\n",
                    shell
                        .join("reference-icons")
                        .join(format!("{icon}.svg"))
                        .display()
                ),
            )?;
        }
        let mut catalogue = Vec::new();
        for directory in ["icons", "reference-icons"] {
            for e in fs::read_dir(shell.join(directory))? {
                let p = e?.path();
                if p.extension().is_some_and(|s| s == "svg") {
                    catalogue.push(p);
                }
            }
        }
        catalogue.sort();
        require(!catalogue.is_empty(), "empty icon catalogue")?;
        let glyph_source = text(&shell.join("SystemGlyph.qml"))?;
        let re = Regex::new(r#"name==="([^\"]+)""#)?;
        let mut symbols: Vec<String> = re
            .captures_iter(&glyph_source)
            .map(|c| c[1].to_string())
            .collect();
        symbols.sort();
        symbols.dedup();
        require(!symbols.is_empty(), "empty glyph catalogue")?;
        let qml = opt.output.join("grid.qml");
        let source = include_str!("../grid.qml")
            .replace("__SYMBOLS__", &serde_json::to_string(&symbols)?)
            .replace(
                "__ICONS__",
                &serde_json::to_string(
                    &catalogue
                        .iter()
                        .map(|v| v.display().to_string())
                        .collect::<Vec<_>>(),
                )?,
            );
        fs::write(&qml, source)?;
        Ok((
            Self {
                opt,
                env: vars,
                runtime,
                qml,
                shell,
                processes: Vec::new(),
            },
            provider_records,
            catalogue,
            symbols,
        ))
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut c = Command::new(program);
        c.env_clear().envs(&self.env);
        c
    }
    fn launch(
        &mut self,
        program: impl AsRef<std::ffi::OsStr>,
        args: &[&str],
        log: &str,
    ) -> Result<()> {
        let mut c = self.command(program);
        let logfile = File::create(self.opt.output.join(log))?;
        c.args(args).stdout(Stdio::from(logfile.try_clone()?));
        c.stderr(Stdio::from(logfile));
        unsafe {
            c.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let p = c.spawn()?;
        self.processes.push(Managed(p));
        Ok(())
    }
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let out = self.command(program).args(args).output()?;
        require(
            out.status.success(),
            format!(
                "{program} {:?}: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            ),
        )?;
        Ok(String::from_utf8(out.stdout)?.trim().to_string())
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut a = vec![
            "-p",
            self.shell.to_str().ok_or("shell path")?,
            "ipc",
            "call",
            "desktop",
            method,
        ];
        a.extend(args);
        self.call("quickshell", &a)
    }
    fn grid(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut a = vec![
            "-p",
            self.qml.to_str().ok_or("qml path")?,
            "ipc",
            "call",
            "icons",
            method,
        ];
        a.extend(args);
        self.call("quickshell", &a)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.grid("state", &[])?)?)
    }
    fn appearance(&self, dark: bool, style: &str) -> Result<()> {
        let palette: Value = serde_json::from_str(&text(
            &self
                .shell
                .parent()
                .ok_or("asset parent")?
                .join("appearance/palette.json"),
        )?)?;
        let mode = if dark { "dark" } else { "light" };
        let target = self.opt.output.join("appearance.json");
        let tmp = self.opt.output.join("appearance.tmp");
        write_json(
            &tmp,
            &json!({"font":palette["font"],"palette":palette[mode],"preferences":{"mode":mode,"iconAppearance":style,"dockMagnification":true,"reduceMotion":false}}),
        )?;
        fs::rename(tmp, target)?;
        Ok(())
    }
    fn start(&mut self) -> Result<()> {
        self.appearance(false, "auto")?;
        let config = self.opt.output.join("sway.conf");
        fs::write(
            &config,
            "output HEADLESS-1 resolution 1440x1000\noutput * bg #29445b solid_color\nxwayland disable\ndefault_border none\nfor_window [title=\"^Icon catalogue$\"] floating enable\n",
        )?;
        self.launch(
            &self.opt.sway.clone(),
            &[
                "--unsupported-gpu",
                "-c",
                config.to_str().ok_or("config path")?,
            ],
            "sway.log",
        )?;
        let sock = wait("Sway socket", || {
            Ok(fs::read_dir(&self.runtime)?
                .filter_map(|v| v.ok())
                .map(|v| v.file_name().to_string_lossy().to_string())
                .find(|v| v.starts_with("sway-ipc") && v.ends_with(".sock")))
        })?;
        let wayland = wait("Wayland socket", || {
            Ok(fs::read_dir(&self.runtime)?
                .filter_map(|v| v.ok())
                .map(|v| v.file_name().to_string_lossy().to_string())
                .find(|v| v.starts_with("wayland-") && !v.ends_with(".lock")))
        })?;
        self.env.insert(
            "SWAYSOCK".into(),
            self.runtime.join(sock).display().to_string(),
        );
        self.env.insert("WAYLAND_DISPLAY".into(), wayland);
        let shell_path = self.shell.to_string_lossy().into_owned();
        self.launch("quickshell", &["-p", &shell_path].as_slice(), "shell.log")?;
        wait("shell status", || {
            Ok(serde_json::from_str::<Value>(&self.ipc("status", &[])?).ok())
        })?;
        let qml_path = self.qml.to_string_lossy().into_owned();
        self.launch("quickshell", &["-p", &qml_path].as_slice(), "grid.log")?;
        wait("grid items", || {
            Ok((self.state()?["items"]
                .as_array()
                .is_some_and(|v| !v.is_empty()))
            .then_some(()))
        })?;
        Ok(())
    }
}
fn find_window(node: &Value) -> Option<Value> {
    if node["name"] == "Icon catalogue" && node["visible"] == true {
        return Some(node.clone());
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(rows) = node[key].as_array() {
            for child in rows {
                if let Some(found) = find_window(child) {
                    return Some(found);
                }
            }
        }
    }
    None
}
fn rgb(s: &str) -> Result<[i32; 3]> {
    let s = s.strip_prefix('#').ok_or("color")?;
    require(s.len() >= 6, "short color")?;
    Ok([0, 2, 4].map(|i| i32::from_str_radix(&s[i..i + 2], 16).unwrap_or(0)))
}
struct Image {
    width: u32,
    height: u32,
    channels: usize,
    data: Vec<u8>,
}
impl Image {
    fn load(path: &Path) -> Result<Self> {
        let mut decoder = png::Decoder::new(File::open(path)?);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut bytes = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut bytes)?;
        bytes.truncate(info.buffer_size());
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            _ => {
                return Err(error(format!(
                    "unsupported screenshot color {:?}",
                    info.color_type
                )));
            }
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            channels,
            data: bytes,
        })
    }
    fn pixel(&self, x: i32, y: i32) -> Result<[i32; 3]> {
        require(
            x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32,
            format!("pixel out of bounds {x},{y}"),
        )?;
        let i = (y as usize * self.width as usize + x as usize) * self.channels;
        Ok([
            self.data[i] as i32,
            self.data[i + 1] as i32,
            self.data[i + 2] as i32,
        ])
    }
    fn crop(&self, box_: [i32; 4]) -> Result<Vec<[i32; 3]>> {
        let [x0, y0, x1, y1] = box_;
        require(
            x0 >= 0
                && y0 >= 0
                && x1 <= self.width as i32
                && y1 <= self.height as i32
                && x0 < x1
                && y0 < y1,
            "invalid image crop",
        )?;
        let mut v = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                v.push(self.pixel(x, y)?);
            }
        }
        Ok(v)
    }
}
fn painted(f: &Fixture, path: &Path, box_: Option<[i32; 4]>) -> Result<bool> {
    f.call("grim", &[path.to_str().ok_or("capture path")?])?;
    let im = Image::load(path)?;
    let pixels = im.crop(box_.unwrap_or([0, 0, im.width as i32, im.height as i32]))?;
    let n = pixels.len() as f64;
    let mut mean = [0.0; 3];
    let mut square = [0.0; 3];
    for p in pixels {
        for i in 0..3 {
            mean[i] += p[i] as f64;
            square[i] += (p[i] * p[i]) as f64;
        }
    }
    Ok(
        (0..3).any(|i| (square[i] / n - (mean[i] / n).powi(2)).max(0.0).sqrt() > 2.0)
            && (0..3).any(|i| mean[i] / n > 10.0),
    )
}
fn variant_sample(
    desired: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    image: &Image,
) -> Result<Option<([i32; 3], [i32; 3])>> {
    let path = Path::new(desired.strip_prefix("file://").unwrap_or(desired));
    let svg = text(path)?;
    let rect = Regex::new(r#"<rect\b[^>]*>"#)?
        .find(&svg)
        .ok_or("SVG rect")?
        .as_str()
        .to_string();
    let attr = |source: &str, key: &str| -> Result<String> {
        let re = Regex::new(&format!(r#"\b{}="([^"]+)""#, regex::escape(key)))?;
        Ok(re.captures(source).ok_or("SVG attribute")?[1].to_string())
    };
    let view = attr(&svg, "viewBox")?
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let fill = rgb(&attr(&rect, "fill")?)?;
    let rx = attr(&rect, "x")?.parse::<f64>()?;
    let ry = attr(&rect, "y")?.parse::<f64>()?;
    let rw = attr(&rect, "width")?.parse::<f64>()?;
    let rh = attr(&rect, "height")?.parse::<f64>()?;
    let sx = (rx + rw * 0.1) / view[2];
    let sy = (ry + rh * 0.5) / view[3];
    Ok(Some((
        image.pixel((x + w * sx).round() as i32, (y + h * sy).round() as i32)?,
        fill,
    )))
}
fn grid_painted(
    f: &Fixture,
    path: &Path,
    items: &[Value],
    box_: &Value,
    client: &Value,
    scale: f64,
    style: &str,
    dark: bool,
) -> Result<Option<Vec<Value>>> {
    f.call("grim", &[path.to_str().ok_or("capture path")?])?;
    let image = Image::load(path)?;
    let state = f.state()?;
    let background = rgb(state["background"].as_str().ok_or("background")?)?;
    let px = |v: &Value, key: &str| -> Result<f64> { Ok(v[key].as_f64().ok_or("geometry")?) };
    let cx = (px(box_, "x")? + px(client, "x")? + px(client, "width")? - 2.0) * scale;
    let cy = (px(box_, "y")? + px(client, "y")? + px(client, "height")? - 2.0) * scale;
    let corner = image.pixel(cx.round() as i32, cy.round() as i32)?;
    if (0..3).any(|i| (corner[i] - background[i]).abs() > 3) {
        write_json(
            &f.opt.output.join("last-paint-mismatch.json"),
            &json!({"reason":"profile background not presented","capture":path.file_name().unwrap().to_string_lossy(),"observed":corner,"expected":background}),
        )?;
        return Ok(None);
    }
    let mut records = Vec::new();
    for item in items {
        let kind = item["kind"].as_str().ok_or("kind")?;
        let source = item["source"].as_str().ok_or("source")?;
        let desired = item["desired"].as_str().ok_or("desired")?;
        let ix = px(item, "x")?;
        let iy = px(item, "y")?;
        let iw = px(item, "width")?;
        let ih = px(item, "height")?;
        require(
            ix >= 0.0
                && iy >= 0.0
                && ix + iw <= px(client, "width")?
                && iy + ih <= px(client, "height")?,
            format!("item out of window: {item}"),
        )?;
        if kind == "asset"
            && ["calendar", "notes", "reminders"]
                .iter()
                .any(|name| source.ends_with(&format!("/reference-icons/{name}.svg")))
        {
            let expected = style == "dark" || style == "auto" && dark;
            require(
                desired.contains("/reference-icons/dark/") == expected,
                format!("wrong source variant: {item}"),
            )?;
        }
        let bg = rgb(item["background"].as_str().ok_or("item background")?)?;
        let x = (px(box_, "x")? + px(client, "x")? + ix) * scale;
        let y = (px(box_, "y")? + px(client, "y")? + iy) * scale;
        let w = iw * scale;
        let h = ih * scale;
        let pixels = image.crop([
            x.round() as i32,
            y.round() as i32,
            (x + w).round() as i32,
            (y + h).round() as i32,
        ])?;
        let mut count = pixels
            .iter()
            .filter(|p| (0..3).any(|i| (p[i] - bg[i]).abs() > 15))
            .count();
        if kind == "symbol" {
            let ink = rgb(item["ink"].as_str().ok_or("ink")?)?;
            let d = [ink[0] - bg[0], ink[1] - bg[1], ink[2] - bg[2]];
            let norm = d.iter().map(|v| v * v).sum::<i32>() as f64;
            count = pixels
                .iter()
                .filter(|p| {
                    let delta = [p[0] - bg[0], p[1] - bg[1], p[2] - bg[2]];
                    let coverage = (0..3).map(|i| delta[i] * d[i]).sum::<i32>() as f64 / norm;
                    coverage > 0.12
                        && coverage <= 1.1
                        && (0..3)
                            .all(|i| ((delta[i] as f64) - coverage * (d[i] as f64)).abs() < 20.0)
                })
                .count();
        } else if iw == 64.0
            && ["calendar", "notes", "reminders"]
                .iter()
                .any(|name| desired.ends_with(&format!("/{name}.svg")))
            && desired.contains("/reference-icons/")
        {
            if let Some((sample, expected)) = variant_sample(desired, x, y, w, h, &image)? {
                if (0..3).any(|i| (sample[i] - expected[i]).abs() > 8) {
                    write_json(
                        &f.opt.output.join("last-paint-mismatch.json"),
                        &json!({"reason":"source variant not presented","capture":path.file_name().unwrap().to_string_lossy(),"item":item,"observed":sample,"expected":expected}),
                    )?;
                    return Ok(None);
                }
            }
        }
        if count < 4 {
            write_json(
                &f.opt.output.join("last-paint-mismatch.json"),
                &json!({"reason":"sample not painted with current role","capture":path.file_name().unwrap().to_string_lossy(),"item":item,"paintedPixels":count}),
            )?;
            return Ok(None);
        }
        if kind == "asset" {
            require(
                px(item, "rasterWidth")? >= w && px(item, "rasterHeight")? >= h,
                format!("undersized raster: {item}"),
            )?;
        }
        records.push(json!({"source":source,"desired":desired,"size":iw,"physicalBox":[x,y,w,h],"paintedPixels":count}));
    }
    Ok(Some(records))
}
fn run(opt: Opt) -> Result<()> {
    let (mut f, provider_records, catalogue, symbols) = Fixture::new(opt)?;
    f.start()?;
    wait("catalogue count", || {
        Ok((f.state()?["items"]
            .as_array()
            .is_some_and(|v| v.len() == catalogue.len() * 3))
        .then_some(()))
    })?;
    let mut records = Vec::new();
    for scale in [1.0_f64, 1.25, 1.5, 2.0] {
        let resolution = format!(
            "output HEADLESS-1 resolution {}x{} scale {}",
            (1440.0 * scale) as i32,
            (1000.0 * scale) as i32,
            scale
        );
        let reply: Value = serde_json::from_str(&f.call("swaymsg", &[&resolution])?)?;
        require(
            reply
                .as_array()
                .ok_or("Sway response")?
                .iter()
                .all(|v| v["success"] == true),
            "output scale failed",
        )?;
        for dark in [false, true] {
            for style in ["auto", "default", "dark"] {
                let name = format!(
                    "{}-{}-{}",
                    scale,
                    if dark { "dark" } else { "light" },
                    style
                );
                f.appearance(dark, style)?;
                wait("appearance and icon readiness", || {
                    let s = f.state()?;
                    let ready = s["dark"] == dark
                        && s["style"] == style
                        && s["items"].as_array().is_some_and(|v| {
                            v.iter().all(|i| i["ready"] == true && i["failed"] == false)
                        });
                    Ok(ready.then_some(()))
                })?;
                f.grid("showGrid", &["false"])?;
                f.ipc("menu", &[""])?;
                let dock = f.opt.output.join(format!("{name}-dock.png"));
                wait("Dock painting", || {
                    Ok(painted(
                        &f,
                        &dock,
                        Some([
                            0,
                            (820.0 * scale) as i32,
                            (1440.0 * scale) as i32,
                            (1000.0 * scale) as i32,
                        ]),
                    )?
                    .then_some(()))
                })?;
                f.ipc("launcher", &[])?;
                pause(200);
                f.call("wtype", &["-M", "ctrl", "-k", "1", "-m", "ctrl"])?;
                let launcher = f.opt.output.join(format!("{name}-launcher.png"));
                wait("Launcher painting", || {
                    Ok(painted(
                        &f,
                        &launcher,
                        Some([
                            (100.0 * scale) as i32,
                            (80.0 * scale) as i32,
                            (1340.0 * scale) as i32,
                            (880.0 * scale) as i32,
                        ]),
                    )?
                    .then_some(()))
                })?;
                f.ipc("menu", &[""])?;
                f.grid("showGrid", &["true"])?;
                wait("Grid visibility", || {
                    Ok((f.state()?["visible"] == true).then_some(()))
                })?;
                let window = wait("catalogue window", || {
                    let tree: Value =
                        serde_json::from_str(&f.call("swaymsg", &["-r", "-t", "get_tree"])?)?;
                    Ok(find_window(&tree))
                })?;
                let box_ = window["rect"].clone();
                let client = window["window_rect"].clone();
                require(
                    client["width"] == 1260 && client["height"] == 840,
                    format!("wrong client size {window}"),
                )?;
                let items = f.state()?["items"].as_array().ok_or("items")?.clone();
                let grid = f.opt.output.join(format!("{name}-grid.png"));
                let icons = wait("icon paint", || {
                    grid_painted(&f, &grid, &items, &box_, &client, scale, style, dark)
                })?;
                f.grid("showSymbols", &[])?;
                wait("symbol readiness", || {
                    let s = f.state()?;
                    Ok((s["items"].as_array().is_some_and(|v| {
                        v.len() == symbols.len() * 3 && v.iter().all(|i| i["ready"] == true)
                    }))
                    .then_some(()))
                })?;
                let items = f.state()?["items"].as_array().ok_or("symbols")?.clone();
                let capture = f.opt.output.join(format!("{name}-symbols.png"));
                let glyphs = wait("glyph paint", || {
                    grid_painted(&f, &capture, &items, &box_, &client, scale, style, dark)
                })?;
                records.push(json!({"scale":scale,"dark":dark,"style":style,"icons":icons,"symbols":glyphs,"window":box_,"captures":[grid.file_name().unwrap().to_string_lossy(),capture.file_name().unwrap().to_string_lossy()]}));
                f.grid("showGrid", &["false"])?;
            }
        }
    }
    for log in ["shell.log", "grid.log"] {
        let contents = text(&f.opt.output.join(log))?;
        for bad in [
            "Failed to load configuration",
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Cannot assign",
            "Error decoding",
            "Cannot open:",
        ] {
            require(!contents.contains(bad), format!("{log}: {bad}"))?;
        }
    }
    let mut hashes = serde_json::Map::new();
    for path in &catalogue {
        hashes.insert(
            path.strip_prefix(&f.shell)?.display().to_string(),
            Value::String(sha(&fs::read(path)?)),
        );
    }
    let glyph_hash = sha(&fs::read(f.shell.join("SystemGlyph.qml"))?);
    let result = json!({"passed":true,"assets":f.opt.assets,"launcher":f.opt.launcher,"compositor":f.opt.sway,"scales":[1,1.25,1.5,2],"appearances":["light-auto","light-default","light-dark","dark-auto","dark-default","dark-dark"],"cases":records,"catalogueCount":catalogue.len(),"symbolCount":symbols.len(),"symbolNames":symbols,"symbolSourceSha256":glyph_hash,"symbolSamples":records.iter().map(|v|v["symbols"].as_array().map_or(0,Vec::len)).sum::<usize>(),"iconSamples":records.iter().map(|v|v["icons"].as_array().map_or(0,Vec::len)).sum::<usize>(),"providerIsolation":provider_records,"fixtureSha256":sha(include_bytes!("main.rs")),"catalogueSha256":hashes});
    let path = f.opt.output.join("result.json");
    write_json(&path, &result)?;
    println!("{}", path.display());
    Ok(())
}
fn main() -> Result<()> {
    let argv0 = env::args_os().next().unwrap_or_default();
    let name = Path::new(&argv0)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    if name == "dbus-run-session" {
        return fake_bus();
    }
    if name == "lumen-calendar" {
        return fake_calendar();
    }
    if env::args().any(|v| v == "--bootstrap-check") {
        return bootstrap_check();
    }
    let opt = Opt::parse()?;
    if !opt.inside {
        private_entry(&opt)
    } else {
        run(opt)
    }
}
