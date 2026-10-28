//! Exercise desktop widget visibility, focus, and paint on two private outputs.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
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
fn save_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn number(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| fail(format!("Missing {key}: {value}")))
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
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
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
        if path.is_dir() {
            copy_tree(&path, &to)?;
        } else if path.is_file() {
            fs::copy(&path, &to)?;
        } else {
            return Err(fail("Unsupported asset"));
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
    pointer_tool: PathBuf,
    calendar_runtime: PathBuf,
    output: PathBuf,
    assets: Option<PathBuf>,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut pointer_tool = None;
    let mut calendar_runtime = None;
    let mut output = None;
    let mut assets = None;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
            Some("--pointer-tool") => {
                pointer_tool = Some(PathBuf::from(args.next().ok_or("Missing pointer tool")?))
            }
            Some("--calendar-runtime") => {
                calendar_runtime = Some(PathBuf::from(
                    args.next().ok_or("Missing calendar runtime")?,
                ))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--assets") => assets = Some(PathBuf::from(args.next().ok_or("Missing assets")?)),
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown calendar-widget fixture option")),
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
        pointer_tool: pointer_tool.ok_or("--pointer-tool required")?,
        calendar_runtime: calendar_runtime.ok_or("--calendar-runtime required")?,
        output,
        assets,
        effects,
        inside,
    })
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = Path::new("/tmp").join(format!("linp-{}-{nonce}", std::process::id()));
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
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
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
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
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
        .envs(&vars)
        .status()?;
    require(
        status.success(),
        format!("Private widget-visibility fixture exited {status}"),
    )
}
struct Managed(Child);
impl Managed {
    fn launch(
        program: &Path,
        args: &[&str],
        vars: &BTreeMap<String, String>,
        log: &Path,
        pipe: bool,
    ) -> Result<Self> {
        let file = File::create(log)?;
        Ok(Self(
            Command::new(program)
                .args(args)
                .env_clear()
                .envs(vars)
                .stdin(if pipe { Stdio::piped() } else { Stdio::null() })
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
            let end = Instant::now() + Duration::from_secs(4);
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
struct Driver {
    opt: Options,
    vars: BTreeMap<String, String>,
    runtime: PathBuf,
    out: PathBuf,
    qml: PathBuf,
    processes: Vec<Managed>,
    pointer: Option<Managed>,
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
            self.qml.to_str().ok_or("Non-UTF8 QML path")?,
            "ipc",
            "call",
            "filesAudit",
            method,
        ];
        words.extend_from_slice(args);
        self.call("quickshell", &words)
    }
    fn ipc_json(&self, method: &str, args: &[&str]) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc(method, args)?)?)
    }
    fn state(&self) -> Result<Value> {
        self.ipc_json("state", &[])
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
            false,
        )?);
        Ok(())
    }
    fn point(&self, id: &str, output: &str, name: &str, rx: f64, ry: f64) -> Result<(f64, f64)> {
        let geometry = self.ipc_json("geometry", &[id, output, name])?;
        require(
            !geometry.is_null(),
            format!("Widget control absent: {id}/{name}"),
        )?;
        Ok((
            number(&geometry, "x")? + number(&geometry, "width")? * rx,
            number(&geometry, "y")? + number(&geometry, "height")?.min(25.) * ry,
        ))
    }
    fn command(&mut self, value: &str) -> Result<()> {
        let pointer = self.pointer.as_mut().ok_or("Pointer unavailable")?;
        let input = pointer
            .0
            .stdin
            .as_mut()
            .ok_or("Pointer stdin unavailable")?;
        input.write_all(value.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        pause(120);
        Ok(())
    }
    fn move_to(&mut self, x: f64, y: f64) -> Result<()> {
        self.call(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &x.round().to_string(),
                &y.round().to_string(),
            ],
        )?;
        self.command("m 0.5 0.5")
    }
    fn click(&mut self, position: (f64, f64)) -> Result<()> {
        self.move_to(position.0, position.1)?;
        self.command("p")?;
        self.command("r")
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.call("wtype", args)?;
        pause(150);
        Ok(())
    }
}
fn setup(opt: Options) -> Result<Driver> {
    fs::create_dir(&opt.output)?;
    let mut vars = launcher_environment(&opt.launcher)?;
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_HEADLESS_OUTPUTS", "2"),
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
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
    ] {
        vars.insert(key.into(), value.into());
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
    let runtime = PathBuf::from(
        vars.get("XDG_RUNTIME_DIR")
            .ok_or("Private runtime missing")?,
    );
    let source_assets = if let Some(path) = &opt.assets {
        fs::canonicalize(path)?
    } else {
        fs::canonicalize(
            vars.get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Packaged assets missing")?,
        )?
    };
    let assets = opt.output.join("assets");
    copy_tree(&source_assets, &assets)?;
    let palette = read_json(&assets.join("appearance/palette.json"))?;
    let appearance = opt.output.join("appearance.json");
    save_json(
        &appearance,
        &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}),
    )?;
    vars.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let package = env::current_exe()?
        .parent()
        .ok_or("Executable parent missing")?
        .parent()
        .ok_or("Executable root missing")?
        .join("share/lumen-widget-visibility-fixture");
    let fixture = package.join("settings_fixture.nu");
    let output = Command::new("nu")
        .arg(&fixture)
        .arg(&opt.output)
        .arg(vars.get("PATH").ok_or("Packaged PATH missing")?)
        .env_clear()
        .envs(&vars)
        .output()?;
    require(
        output.status.success(),
        format!(
            "Nushell settings fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    let overlay: Value = serde_json::from_slice(&output.stdout)?;
    for key in ["PATH", "LUMEN_NIX_SETTINGS_ACTIVE"] {
        vars.insert(
            key.into(),
            overlay[key]
                .as_str()
                .ok_or("Settings fixture export missing")?
                .into(),
        );
    }
    let mirror = opt.output.join("calendar");
    let fixture_events = package.join("fixture_calendar_events.nu");
    let events = Command::new("nu")
        .arg(&fixture_events)
        .arg(&mirror)
        .env_clear()
        .envs(&vars)
        .output()?;
    require(
        events.status.success(),
        format!(
            "Calendar mirror failed: {}",
            String::from_utf8_lossy(&events.stderr)
        ),
    )?;
    let real = fs::canonicalize(&opt.calendar_runtime)?;
    require(real.is_file(), "Rust calendar runtime unavailable")?;
    let bin = opt.output.join("bin");
    fs::create_dir_all(&bin)?;
    symlink(env::current_exe()?, bin.join("lumen-calendar"))?;
    vars.insert("LUMEN_CALENDAR_REAL".into(), real.display().to_string());
    vars.insert(
        "LUMEN_CALENDAR_FIXTURE".into(),
        opt.output.display().to_string(),
    );
    vars.insert("LUMEN_CALENDAR_TIMEZONE".into(), "Europe/London".into());
    vars.insert("TZ".into(), "Europe/London".into());
    vars.insert("CALENDAR_DESKTOP_PREFIX".into(), "lumen".into());
    vars.insert("CALENDAR_UID_SUFFIX".into(), "@lumen-desktop.local".into());
    vars.insert("GLOBAL_MENU_ENABLED".into(), "1".into());
    vars.insert(
        "LUMEN_COMPOSITOR_EFFECTS".into(),
        if opt.effects { "1" } else { "0" }.into(),
    );
    vars.insert(
        "LUMEN_MENU_BRIDGE".into(),
        vars.get("GLOBAL_MENU_BRIDGE")
            .ok_or("Menu bridge missing")?
            .clone(),
    );
    let desktop_items: Vec<Value> = ["small", "medium", "large"]
        .iter()
        .map(|size| {
            json!({
                "id":format!("calendar-{size}"),"type":"calendar","size":size,
                "style":"month","output":"HEADLESS-1","positions":{}
            })
        })
        .chain(["small", "medium", "large"].iter().map(|size| {
            json!({
                "id":format!("clock-{size}"),"type":"clock","size":size,
                "style":"month","hourCycle":"24","output":"HEADLESS-2","positions":{}
            })
        }))
        .collect();
    let preferences = json!({
        "desktopVisible":true,"desktopColor":"auto","reduceTransparency":false,
        "desktopItems":desktop_items
    });
    let state_home = PathBuf::from(vars.get("XDG_STATE_HOME").ok_or("State home missing")?);
    let widget_settings = state_home.join("lumen-desktop/session/notification-centre.json");
    fs::create_dir_all(
        widget_settings
            .parent()
            .ok_or("Widget settings parent missing")?,
    )?;
    save_json(&widget_settings, &preferences)?;
    vars.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            bin.display(),
            vars.get("PATH").ok_or("PATH missing")?
        ),
    );
    let qml = opt.output.join("shell.qml");
    fs::copy(package.join("fixture.qml"), &qml)?;
    let out = opt.output.clone();
    Ok(Driver {
        opt,
        vars,
        runtime,
        out,
        qml,
        processes: Vec::new(),
        pointer: None,
    })
}
struct Image {
    width: usize,
    height: usize,
    channels: usize,
    bytes: Vec<u8>,
}
impl Image {
    fn open(path: &Path) -> Result<Self> {
        let mut decoder = png::Decoder::new(File::open(path)?);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut bytes = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut bytes)?;
        let channels = match frame.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            other => return Err(fail(format!("Unsupported PNG type: {other:?}"))),
        };
        bytes.truncate(frame.buffer_size());
        Ok(Self {
            width: frame.width as usize,
            height: frame.height as usize,
            channels,
            bytes,
        })
    }
    fn pixel(&self, x: f64, y: f64) -> Result<[u8; 3]> {
        let x = x.round() as isize;
        let y = y.round() as isize;
        require(
            x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height,
            format!(
                "Pixel outside screenshot: {x},{y} / {}x{}",
                self.width, self.height
            ),
        )?;
        let offset = (y as usize * self.width + x as usize) * self.channels;
        Ok([
            self.bytes[offset],
            self.bytes[offset + 1],
            self.bytes[offset + 2],
        ])
    }
    fn variation(&self, rect: [f64; 4]) -> Result<f64> {
        let [x0, y0, x1, y1] = rect.map(|v| v.round() as isize);
        require(
            x0 >= 0
                && y0 >= 0
                && x1 > x0
                && y1 > y0
                && x1 as usize <= self.width
                && y1 as usize <= self.height,
            format!("Invalid clock crop: {rect:?}"),
        )?;
        let count = ((x1 - x0) * (y1 - y0)) as f64;
        let mut sums = [0.; 3];
        let mut squares = [0.; 3];
        for y in y0..y1 {
            for x in x0..x1 {
                let pixel = self.pixel(x as f64, y as f64)?;
                for channel in 0..3 {
                    let v = pixel[channel] as f64;
                    sums[channel] += v;
                    squares[channel] += v * v;
                }
            }
        }
        Ok((0..3)
            .map(|channel| {
                let mean = sums[channel] / count;
                (squares[channel] / count - mean * mean).max(0.).sqrt()
            })
            .fold(0., f64::max))
    }
}
fn rows(state: &Value) -> Result<&[Value]> {
    state["rows"]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| fail("Widget rows missing"))
}
fn row<'a>(state: &'a Value, id: &str) -> Result<&'a Value> {
    rows(state)?
        .iter()
        .find(|row| row["id"] == id)
        .ok_or_else(|| fail(format!("Widget row {id} missing: {state}")))
}
fn action_count(state: &Value) -> Result<usize> {
    Ok(state["actions"].as_array().ok_or("Actions missing")?.len())
}
fn all_rows(state: &Value, predicate: impl Fn(&Value) -> bool) -> Result<bool> {
    Ok(rows(state)?.iter().all(predicate))
}
fn contrast(style: &Value) -> Result<f64> {
    fn luminance(value: &str) -> Result<f64> {
        require(
            value.len() >= 7 && value.starts_with('#'),
            format!("Invalid colour: {value}"),
        )?;
        let mut rgb = [0.; 3];
        for (channel, start) in [1, 3, 5].into_iter().enumerate() {
            let v = u8::from_str_radix(&value[start..start + 2], 16)? as f64 / 255.;
            rgb[channel] = if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            };
        }
        Ok(rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722)
    }
    let foreground = luminance(style["foreground"].as_str().ok_or("Foreground missing")?)?;
    let background = luminance(style["background"].as_str().ok_or("Background missing")?)?;
    let (light, dark) = if foreground > background {
        (foreground, background)
    } else {
        (background, foreground)
    };
    Ok((light + 0.05) / (dark + 0.05))
}
impl Driver {
    fn geometry(&self, id: &str, output: &str, name: &str) -> Result<Value> {
        self.ipc_json("geometry", &[id, output, name])
    }
    fn clock_painted(&self, image: &Image) -> Result<Vec<Value>> {
        let rect = self.geometry("clock-large", "HEADLESS-2", "widget-clock-content")?;
        let variation = image.variation([
            number(&rect, "x")? + 12.,
            number(&rect, "y")? + 8.,
            number(&rect, "x")? + number(&rect, "width")? - 12.,
            number(&rect, "y")? + 65.,
        ])?;
        Ok(if variation > 10. {
            Vec::new()
        } else {
            vec![json!({"output":"HEADLESS-2","clockTextVariation":variation,"minimum":10})]
        })
    }
    fn capture_until(
        &self,
        name: &str,
        probe: impl Fn(&Image) -> Result<Vec<Value>>,
    ) -> Result<()> {
        let path = self.out.join(name);
        let mismatch_path = self
            .out
            .join(format!("{}-pixels.json", name.trim_end_matches(".png")));
        self.wait_for(&format!("painted {name}"), || {
            self.call("grim", &[path.to_str().ok_or("Non-UTF8 screenshot path")?])?;
            let image = Image::open(&path)?;
            let mismatches = probe(&image)?;
            save_json(&mismatch_path, &json!(mismatches))?;
            Ok(mismatches.is_empty())
        })
    }
    fn colour_capture(&self, name: &str, coloured: bool) -> Result<()> {
        let rect = self.geometry("calendar-medium", "HEADLESS-1", "calendar-day-2026-11-01")?;
        self.capture_until(name, |image| {
            let pixel = image.pixel(
                number(&rect, "x")? + number(&rect, "width")? / 2. - 7.,
                number(&rect, "y")? + (number(&rect, "height")? - 4.) / 2.,
            )?;
            let matches = if coloured {
                pixel[0] > pixel[1].saturating_add(40) && pixel[0] > pixel[2].saturating_add(40)
            } else {
                pixel.iter().max().unwrap() - pixel.iter().min().unwrap() < 30
            };
            let mut mismatches = if matches {
                Vec::new()
            } else {
                vec![json!({"output":"HEADLESS-1","calendarPixel":pixel,"coloured":coloured})]
            };
            mismatches.extend(self.clock_painted(image)?);
            Ok(mismatches)
        })
    }
    fn start(&mut self) -> Result<()> {
        let config = self.out.join("sway.conf");
        fs::write(
            &config,
            "output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0\nxwayland disable\nfor_window [app_id=\"widget-focus-fixture\"] floating enable, resize set 260 160, move position 1000 700\n",
        )?;
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
        self.pointer = Some(Managed::launch(
            &self.opt.pointer_tool,
            &["pointer", "move", "0", "0"],
            &self.vars,
            &self.out.join("pointer.log"),
            true,
        )?);
        self.command("m 0.5 0.5")?;
        let qml = self.qml.to_str().ok_or("Non-UTF8 QML path")?.to_owned();
        self.launch(Path::new("quickshell"), &["-p", &qml], "shell.log")?;
        self.wait_for("six loaded widgets", || {
            let state = self.state()?;
            Ok(rows(&state)?.len() == 6 && all_rows(&state, |row| row["loaded"] == true)?)
        })
    }
}
impl Driver {
    fn appearance(&self, palette: &str, preferences: Value) -> Result<()> {
        let palettes = read_json(&self.out.join("assets/appearance/palette.json"))?;
        save_json(
            &self.out.join("appearance.json"),
            &json!({"font":palettes["font"],"palette":palettes[palette],"preferences":preferences}),
        )
    }
    fn check(&mut self) -> Result<()> {
        self.start()?;
        let mut checks = Vec::new();
        let initial = self.state()?;
        require(
            all_rows(&initial, |row| row["monochrome"] == false)?
                && all_rows(&initial, |row| row["opacity"] == 1)?,
            format!("Initial widget visibility: {initial}"),
        )?;
        self.call("swaymsg", &["focus", "output", "HEADLESS-1"])?;
        self.launch(
            Path::new("foot"),
            &[
                "--app-id=widget-focus-fixture",
                "--title=Widget Focus Fixture",
                "sleep",
                "120",
            ],
            "app.log",
        )?;
        self.wait_for("external app focus", || {
            let state = self.state()?;
            Ok(number(&state, "windowId")? > 0.
                && all_rows(&state, |row| row["monochrome"] == true)?)
        })?;
        require(
            all_rows(&self.state()?, |row| {
                number(row, "opacity").is_ok_and(|v| v < 1.)
            })?,
            "Automatic app-focus widgets were not dimmed",
        )?;
        self.colour_capture("automatic-active-app.png", false)?;
        let before = action_count(&self.state()?)?;
        self.click(self.point(
            "calendar-large",
            "HEADLESS-1",
            "calendar-day-2026-11-02",
            0.5,
            0.5,
        )?)?;
        self.wait_for("desktop focus after calendar click", || {
            let state = self.state()?;
            Ok(state["windowId"] == 0 && all_rows(&state, |row| row["monochrome"] == false)?)
        })?;
        require(
            action_count(&self.state()?)? == before,
            "Calendar day opened an action",
        )?;
        self.call("swaymsg", &["[app_id=\"widget-focus-fixture\"]", "focus"])?;
        self.wait_for("app refocus", || {
            let state = self.state()?;
            Ok(number(&state, "windowId")? > 0.
                && all_rows(&state, |row| row["monochrome"] == true)?)
        })?;
        checks.push("Actual external app focus makes automatic widgets monochrome/dim in painted pixels on both displays; a dimmed calendar remains interactive and restores desktop focus");

        self.ipc("preference", &["desktopColor", "\"color\""])?;
        self.wait_for("full colour override", || {
            let state = self.state()?;
            Ok(all_rows(&state, |row| row["monochrome"] == false)?)
        })?;
        let state = self.state()?;
        require(
            number(&state, "windowId")? > 0. && all_rows(&state, |row| row["opacity"] == 1)?,
            format!("Full colour app focus: {state}"),
        )?;
        self.colour_capture("full-colour-active-app.png", true)?;
        self.ipc("preference", &["desktopColor", "\"monochrome\""])?;
        self.wait_for("monochrome override", || {
            Ok(all_rows(&self.state()?, |row| row["monochrome"] == true)?)
        })?;
        self.click((1200., 500.))?;
        self.wait_for("desktop focus", || Ok(self.state()?["windowId"] == 0))?;
        require(
            all_rows(&self.state()?, |row| row["monochrome"] == true)?,
            "Monochrome override lost on desktop focus",
        )?;
        self.colour_capture("monochrome-desktop.png", false)?;
        self.ipc("preference", &["desktopColor", "\"auto\""])?;
        self.wait_for("automatic desktop colour", || {
            Ok(all_rows(&self.state()?, |row| row["monochrome"] == false)?)
        })?;
        self.colour_capture("automatic-desktop.png", true)?;
        checks.push("Full colour and monochrome overrides remain explicit while app/desktop focus changes; automatic restores full colour on desktop focus");

        let before = action_count(&self.state()?)?;
        self.click(self.point(
            "calendar-large",
            "HEADLESS-1",
            "calendar-day-2026-11-02",
            0.5,
            0.5,
        )?)?;
        self.wait_for("manual calendar selection", || {
            Ok(row(&self.state()?, "calendar-large")?["selected"] == "2026-11-02")
        })?;
        require(
            action_count(&self.state()?)? == before,
            "Calendar day opened an action",
        )?;
        self.ipc("preference", &["desktopVisible", "false"])?;
        self.wait_for("hidden widgets", || {
            Ok(all_rows(&self.state()?, |row| row["visible"] == false)?)
        })?;
        let background_clicks = number(&self.state()?, "backgroundClicks")?;
        self.click(self.point("calendar-small", "HEADLESS-1", "", 0.5, 0.5)?)?;
        self.wait_for("background click through hidden widget", || {
            Ok(number(&self.state()?, "backgroundClicks")? == background_clicks + 1.)
        })?;
        require(
            action_count(&self.state()?)? == before,
            "Hidden widget accepted click",
        )?;
        self.ipc("preference", &["desktopVisible", "true"])?;
        self.wait_for("visible widgets", || {
            Ok(all_rows(&self.state()?, |row| row["visible"] == true)?)
        })?;
        require(
            row(&self.state()?, "calendar-large")?["selected"] == "2026-11-02",
            "Manual selection lost after hiding widgets",
        )?;
        checks.push("Hide releases widget input mask to background; showing preserves exact widget identity and manual calendar selection");

        self.ipc("update", &["calendar-small", "{\"output\":\"HEADLESS-2\"}"])?;
        self.wait_for("calendar moved to output two", || {
            let state = self.state()?;
            Ok(rows(&state)?
                .iter()
                .any(|r| r["id"] == "calendar-small" && r["output"] == "HEADLESS-2"))
        })?;
        require(
            !rows(&self.state()?)?
                .iter()
                .any(|r| r["id"] == "calendar-small" && r["output"] == "HEADLESS-1"),
            "Old output retained moved widget",
        )?;
        self.ipc("update", &["calendar-small", "{\"output\":\"HEADLESS-1\"}"])?;
        self.wait_for("calendar moved to output one", || {
            let state = self.state()?;
            Ok(rows(&state)?
                .iter()
                .any(|r| r["id"] == "calendar-small" && r["output"] == "HEADLESS-1"))
        })?;
        checks.push("Per-display preference routes its selected widget to the chosen display and removes the old target");

        self.ipc("preference", &["desktopColor", "\"monochrome\""])?;
        self.ipc("preference", &["reduceTransparency", "true"])?;
        self.wait_for("opaque widget materials", || {
            let state = self.state()?;
            all_rows(&state, |row| {
                row["opaque"] == true && row["alpha"] == 1 && row["opacity"] == 1
            })
        })?;
        self.capture_until("opaque-monochrome.png", |image| {
            let mut mismatches = self.clock_painted(image)?;
            let state = self.state()?;
            for id in ["calendar-large", "clock-large"] {
                let widget = row(&state, id)?;
                let pixel = image.pixel(
                    number(widget, "screenX")? + number(widget, "x")? + 130.,
                    number(widget, "screenY")? + number(widget, "y")? + 15.,
                )?;
                if pixel
                    .iter()
                    .zip([40, 40, 43])
                    .any(|(actual, expected)| (*actual as i32 - expected).abs() > 2)
                {
                    mismatches.push(json!({"output":widget["output"],"widget":id,
                        "pixel":pixel,"expected":[40,40,43],"tolerance":2}));
                }
            }
            Ok(mismatches)
        })?;
        checks.push("Reduce Transparency makes calendar and clock outer materials fully opaque without opacity-based dimming, confirmed by compositor pixels");

        self.ipc("preference", &["reduceTransparency", "false"])?;
        self.appearance("dark", json!({"textScale":1.5,"reduceTransparency":true}))?;
        self.wait_for("global opaque widgets", || {
            let state = self.state()?;
            all_rows(&state, |row| {
                row["opaque"] == true && row["alpha"] == 1 && row["opacity"] == 1
            })
        })?;
        let mut before = action_count(&self.state()?)?;
        for size in ["small", "medium", "large"] {
            let calendar = format!("calendar-{size}");
            let focus_name = if size == "small" {
                "widget-calendar-content"
            } else {
                "calendar-grid"
            };
            self.click(self.point(&calendar, "HEADLESS-1", focus_name, 0.1, 0.1)?)?;
            self.ipc("focusRelated", &[&calendar, "HEADLESS-1"])?;
            pause(200);
            let state = self.state()?;
            let widget = row(&state, &calendar)?;
            require(
                number(&widget["related"], "y")? >= number(widget, "y")?
                    && number(&widget["related"], "y")? + number(&widget["related"], "height")?
                        <= number(widget, "y")? + number(widget, "height")?,
                format!("Related link outside {size} widget: {widget}"),
            )?;
            self.key(&["-k", "space"])?;
            require(
                action_count(&self.state()?)? == before + 1,
                format!("Calendar {size} keyboard action missing"),
            )?;
            before += 1;
            let clock = format!("clock-{size}");
            self.click(self.point(&clock, "HEADLESS-2", "widget-clock-content", 0.5, 0.5)?)?;
            self.ipc("focusClock", &[&clock, "HEADLESS-2"])?;
            self.key(&["-k", "space"])?;
            require(
                action_count(&self.state()?)? == before + 2,
                format!("Clock {size} keyboard action missing"),
            )?;
            before += 2;
        }
        self.call(
            "grim",
            &[self
                .out
                .join("enlarged-opaque.png")
                .to_str()
                .ok_or("Non-UTF8 capture")?],
        )?;
        let style = self.ipc_json("dayStyle", &[])?;
        require(
            number(&style, "font")? >= 18. && contrast(&style)? >= 4.5,
            format!("Dark enlarged contrast: {style}"),
        )?;
        self.appearance("light", json!({"textScale":1.5,"reduceTransparency":true}))?;
        self.wait_for("light palette", || {
            Ok(self.ipc_json("dayStyle", &[])?["foreground"] == "#ffffff")
        })?;
        let style = self.ipc_json("dayStyle", &[])?;
        require(
            contrast(&style)? >= 4.5,
            format!("Light enlarged contrast: {style}"),
        )?;
        self.colour_capture("enlarged-light-monochrome.png", false)?;
        checks.push("At 150 percent text and global Reduce Transparency, all three supported calendar/clock sizes retain reachable distinct pointer/keyboard actions; selected-day contrast passes in both live palettes");

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
        let final_state = self.state()?;
        require(
            final_state["error"].as_str().is_some_and(str::is_empty),
            format!("Widget state error: {final_state}"),
        )?;
        save_json(
            &self.out.join("result.json"),
            &json!({
                "passed":true,"checks":checks,"final":final_state,
                "scope":"Two private native outputs, actual production focus bridge/external app, synthetic read-only calendars and disposable widget settings"
            }),
        )?;
        println!(
            "PASS native widget focus, colour, visibility, opaque material and enlarged controls"
        );
        Ok(())
    }
}
fn provider() -> Result<()> {
    let root = PathBuf::from(env::var("LUMEN_CALENDAR_FIXTURE")?);
    let real = PathBuf::from(env::var("LUMEN_CALENDAR_REAL")?);
    let mut line = String::new();
    BufReader::new(std::io::stdin()).read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line)?;
    require(
        request["action"] == "list" || request["action"] == "load",
        "Read-only widget must not mutate or sync",
    )?;
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("requests.jsonl"))?;
    writeln!(log, "{}", serde_json::to_string(&request)?)?;
    if root.join("delay").exists() {
        pause(1500);
    }
    if root.join("unavailable").exists() {
        println!(
            "{}",
            json!({"error":"Synthetic calendar provider unavailable"})
        );
        return Ok(());
    }
    let mut forwarded = request;
    forwarded["action"] = json!("list");
    let mut child = Command::new(real)
        .env("XDG_DATA_HOME", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    writeln!(
        child.stdin.as_mut().ok_or("Calendar stdin missing")?,
        "{}",
        serde_json::to_string(&forwarded)?
    )?;
    let output = child.wait_with_output()?;
    require(
        output.status.success(),
        format!(
            "Rust calendar failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    std::io::stdout().write_all(&output.stdout)?;
    Ok(())
}
fn run() -> Result<()> {
    if env::args_os()
        .next()
        .and_then(|path| PathBuf::from(path).file_name().map(|v| v.to_os_string()))
        .as_deref()
        == Some(std::ffi::OsStr::new("lumen-calendar"))
    {
        return provider();
    }
    let opt = options()?;
    if !opt.inside {
        return private_session();
    }
    setup(opt)?.check()
}
fn main() {
    if let Err(error) = run() {
        eprintln!("widget-visibility fixture: {error}");
        std::process::exit(1);
    }
}
