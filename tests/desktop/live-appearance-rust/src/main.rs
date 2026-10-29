//! Assert painted palette replacement on six stable, mapped Quickshell windows.
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
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
fn number(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| fail(format!("Missing {key} in {value}")))
}
fn immutable(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    require(
        canonical.starts_with("/nix/store") && canonical.is_file(),
        "Pinned executable required",
    )?;
    Ok(path.to_path_buf())
}
fn program(name: &str, values: &BTreeMap<String, String>) -> Result<PathBuf> {
    for folder in env::split_paths(&OsString::from(
        values.get("PATH").ok_or("Packaged PATH missing")?,
    )) {
        let path = folder.join(name);
        if path.is_file() {
            return immutable(&path);
        }
    }
    Err(fail(format!("{name} unavailable in packaged PATH")))
}
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher).arg("--print-env0").output()?;
    require(
        result.status.success(),
        "Packaged shell environment unavailable",
    )?;
    let mut values = BTreeMap::new();
    for part in result
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
    {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            values.insert(key.into(), value.into());
        }
    }
    require(values.contains_key("PATH"), "Packaged PATH missing")?;
    Ok(values)
}
struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    appearance: PathBuf,
    output: PathBuf,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut appearance = None;
    let mut output = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
            Some("--appearance") => {
                appearance = Some(PathBuf::from(
                    args.next().ok_or("Missing appearance runtime")?,
                ))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            _ => return Err(fail("Unknown live appearance fixture option")),
        }
    }
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        appearance: appearance.ok_or("--appearance required")?,
        output: output.ok_or("--output required")?,
    })
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = Path::new("/tmp").join(format!("lla-{}-{nonce}", std::process::id()));
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
struct Managed(Child);
impl Managed {
    fn launch(
        program: &Path,
        args: &[&str],
        values: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<Self> {
        let file = File::create(log)?;
        Ok(Self(
            Command::new(program)
                .args(args)
                .env_clear()
                .envs(values)
                .stdin(Stdio::null())
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
            let deadline = Instant::now() + Duration::from_secs(4);
            while self.0.try_wait().ok().flatten().is_none() {
                if Instant::now() >= deadline {
                    unsafe {
                        libc::killpg(self.0.id() as i32, libc::SIGKILL);
                    }
                    let _ = self.0.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!(
                "Live palette update timed out waiting for {label}: {last}"
            )));
        }
        thread::sleep(Duration::from_millis(100));
    }
}
fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && (suffix.is_empty() || !name.ends_with(suffix))
            })
        }))
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else if path.is_file() {
            fs::copy(&path, &target)?;
        } else {
            return Err(fail(format!(
                "Unsupported appearance asset: {}",
                path.display()
            )));
        }
    }
    Ok(())
}
fn find_node(node: &Value, predicate: &impl Fn(&Value) -> bool) -> Option<Value> {
    if predicate(node) {
        return Some(node.clone());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            if let Some(found) = find_node(child, predicate) {
                return Some(found);
            }
        }
    }
    None
}
fn rgb(value: &Value) -> Result<[u8; 3]> {
    let text = value.as_str().ok_or("Palette color is not a string")?;
    require(
        text.len() == 7 && text.starts_with('#'),
        format!("Invalid palette color: {text}"),
    )?;
    Ok([
        u8::from_str_radix(&text[1..3], 16)?,
        u8::from_str_radix(&text[3..5], 16)?,
        u8::from_str_radix(&text[5..7], 16)?,
    ])
}
fn pixel_set(palette: &Value) -> Result<HashSet<[u8; 3]>> {
    ["background", "surface", "field", "hover"]
        .into_iter()
        .map(|key| rgb(&palette[key]))
        .collect()
}
fn screenshot_fractions(
    path: &Path,
    rect: &Value,
    expected: &HashSet<[u8; 3]>,
    old: &HashSet<[u8; 3]>,
) -> Result<(f64, f64)> {
    let decoder = png::Decoder::new(File::open(path)?);
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut bytes)?;
    require(
        frame.bit_depth == png::BitDepth::Eight,
        "Expected 8-bit PNG",
    )?;
    let channels = match frame.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(fail(format!("Unsupported PNG color type: {other:?}"))),
    };
    let x0 = number(rect, "x")? + 10;
    let y0 = number(rect, "y")? + 10;
    let x1 = number(rect, "x")? + number(rect, "width")? - 10;
    let y1 = number(rect, "y")? + number(rect, "height")? - 10;
    require(
        x0 >= 0
            && y0 >= 0
            && x1 <= frame.width as i64
            && y1 <= frame.height as i64
            && x1 > x0
            && y1 > y0,
        "Window crop outside screenshot",
    )?;
    let mut fresh = 0u64;
    let mut stale = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let index = (y as usize * frame.width as usize + x as usize) * channels;
            let color = [bytes[index], bytes[index + 1], bytes[index + 2]];
            if expected.contains(&color) {
                fresh += 1;
            }
            if old.contains(&color) {
                stale += 1;
            }
        }
    }
    let total = ((x1 - x0) * (y1 - y0)) as f64;
    Ok((fresh as f64 / total, stale as f64 / total))
}

struct Fixture {
    output: PathBuf,
    _private: PrivateRoot,
    values: BTreeMap<String, String>,
    appearance: PathBuf,
    palettes: Value,
    scene: PathBuf,
    processes: Vec<Managed>,
}
impl Fixture {
    fn new(options: Options) -> Result<Self> {
        let launcher = immutable(&options.launcher)?;
        let sway = immutable(&options.sway)?;
        let appearance = immutable(&options.appearance)?;
        require(!options.output.exists(), "Fresh output directory required")?;
        fs::create_dir_all(&options.output)?;
        fs::set_permissions(&options.output, fs::Permissions::from_mode(0o700))?;
        let output = fs::canonicalize(options.output)?;
        let private = PrivateRoot::new()?;
        let mut values = launcher_environment(&launcher)?;
        values.insert("HOME".into(), private.0.display().to_string());
        values.insert("XDG_RUNTIME_DIR".into(), private.0.display().to_string());
        values.insert(
            "DBUS_SYSTEM_BUS_ADDRESS".into(),
            format!("unix:path={}/no-system-bus", private.0.display()),
        );
        for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
            let path = private.0.join(key.to_lowercase());
            fs::create_dir(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            values.insert(format!("XDG_{key}_HOME"), path.display().to_string());
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
        for (key, value) in [
            ("WLR_BACKENDS", "headless"),
            ("WLR_RENDERER", "pixman"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
            ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
            ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
            ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
        ] {
            values.insert(key.into(), value.into());
        }
        let config = private.0.join("bus.conf");
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
                private.0.display()
            ),
        )?;
        let bus_log = File::create(output.join("bus.log"))?;
        let mut bus = Command::new(program("dbus-daemon", &values)?)
            .arg("--nofork")
            .arg("--print-address=1")
            .arg(format!("--config-file={}", config.display()))
            .env_clear()
            .envs(&values)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(bus_log))
            .process_group(0)
            .spawn()?;
        let stdout = bus.stdout.take().ok_or("Bus address pipe missing")?;
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
            format!(
                "Private bus did not start: {}",
                fs::read_to_string(output.join("bus.log")).unwrap_or_default()
            ),
        )?;
        values.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.trim().into());
        let palettes: Value = serde_json::from_slice(&fs::read(
            env::current_dir()?.join("res/lumen-desktop/appearance/palette.json"),
        )?)?;
        values.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            output.join("appearance.json").display().to_string(),
        );
        let source = env::current_exe()?
            .parent()
            .ok_or("Executable parent missing")?
            .parent()
            .ok_or("Package root missing")?
            .join("share/lumen-live-appearance-fixture/fixture.qml");
        let scene = output.join("shell.qml");
        fs::copy(source, &scene)?;
        copy_tree(
            &env::current_dir()?.join("res/lumen-desktop"),
            &output.join("assets"),
        )?;
        let mut fixture = Self {
            output,
            _private: private,
            values,
            appearance,
            palettes,
            scene,
            processes: vec![Managed(bus)],
        };
        fixture.theme("light")?;
        let sway_config = fixture.output.join("sway.conf");
        fs::write(
            &sway_config,
            "output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n",
        )?;
        fixture.launch(
            &sway,
            &[
                "--unsupported-gpu",
                "-c",
                sway_config.to_str().ok_or("Sway config path invalid")?,
            ],
            "sway.log",
        )?;
        let runtime = PathBuf::from(&fixture.values["XDG_RUNTIME_DIR"]);
        let sock = wait("compositor IPC", || first_file(&runtime, "sway-ipc", ""))?;
        fixture
            .values
            .insert("SWAYSOCK".into(), sock.display().to_string());
        let display = wait("private display", || {
            first_file(&runtime, "wayland-", ".lock")
        })?;
        fixture.values.insert(
            "WAYLAND_DISPLAY".into(),
            display
                .file_name()
                .ok_or("Wayland name missing")?
                .to_string_lossy()
                .into_owned(),
        );
        let quickshell = program("quickshell", &fixture.values)?;
        let scene = fixture.scene.display().to_string();
        fixture.launch(&quickshell, &["-p", &scene], "shell.log")?;
        wait("initial appearance state", || {
            Ok(fixture
                .state()
                .ok()
                .is_some_and(|state| {
                    state["background"]
                        .as_str()
                        .is_some_and(|value| !value.is_empty())
                })
                .then_some(()))
        })?;
        Ok(fixture)
    }
    fn launch(&mut self, program: &Path, args: &[&str], label: &str) -> Result<()> {
        self.processes.push(Managed::launch(
            program,
            args,
            &self.values,
            &self.output.join(label),
        )?);
        Ok(())
    }
    fn call(&self, name: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program("timeout", &self.values)?)
            .arg("10s")
            .arg(program(name, &self.values)?)
            .args(args)
            .env_clear()
            .envs(&self.values)
            .output()?;
        require(
            output.status.success(),
            format!(
                "{name} failed {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn theme(&self, mode: &str) -> Result<Value> {
        let output = Command::new(program("timeout", &self.values)?)
            .arg("5s")
            .arg(&self.appearance)
            .args(["--resolve-palette", mode, "blue", "false"])
            .env_clear()
            .envs(&self.values)
            .output()?;
        require(
            output.status.success(),
            format!(
                "Rust appearance resolver failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        let colors: Value = serde_json::from_slice(&output.stdout)?;
        let payload = json!({"font":self.palettes["font"],"palette":colors,"preferences":{"mode":mode,"reduceMotion":true}});
        let temp = self.output.join("appearance.tmp");
        fs::write(&temp, serde_json::to_vec(&payload)?)?;
        fs::rename(temp, self.output.join("appearance.json"))?;
        Ok(colors)
    }
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn window(&self, title: &str) -> Result<Option<Value>> {
        Ok(find_node(&self.tree()?, &|node| {
            node["app_id"] == "org.quickshell" && node["name"] == title && node["visible"] == true
        }))
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut command = vec![
            "-p",
            self.scene.to_str().ok_or("Scene path invalid")?,
            "ipc",
            "call",
            "audit",
            method,
        ];
        command.extend(args);
        self.call("quickshell", &command)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("state", &[])?)?)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        while let Some(process) = self.processes.pop() {
            drop(process);
        }
    }
}

fn run(options: Options) -> Result<()> {
    let fixture = Fixture::new(options)?;
    let mut checks = Vec::new();
    for (name, title) in [
        ("help", "Desktop Help"),
        ("session", "Session Options"),
        ("remote", "Remote Desktop"),
        ("text", "Text Tools"),
        ("info", "File Info"),
        ("quick", "Quick Look"),
    ] {
        fixture.ipc("open", &[name])?;
        let original = wait("mapped owned window", || fixture.window(title))?;
        let identity = number(&original, "id")?;
        let rectangle = original["rect"].clone();
        for (step, mode) in ["light", "dark", "light"].into_iter().enumerate() {
            let colors = fixture.theme(mode)?;
            wait("QML theme consumed atomic replacement", || {
                Ok((fixture.state()?["dark"] == (mode == "dark")).then_some(()))
            })?;
            let expected = pixel_set(&colors)?;
            let old = pixel_set(&fixture.palettes[if mode == "light" { "dark" } else { "light" }])?
                .difference(&expected)
                .copied()
                .collect::<HashSet<_>>();
            let check = wait("fresh painted window palette", || {
                let current = fixture.window(title)?.ok_or("Owned window disappeared")?;
                require(
                    number(&current, "id")? == identity && current["rect"] == rectangle,
                    "Theme update recreated or moved the window",
                )?;
                let path = fixture.output.join(format!("{name}-{step}-{mode}.png"));
                fixture.call("grim", &[path.to_str().ok_or("Screenshot path invalid")?])?;
                let (fresh, stale) = screenshot_fractions(&path, &rectangle, &expected, &old)?;
                if fresh < 0.35 || stale > 0.02 {
                    return Ok(None);
                }
                Ok(Some(
                    json!({"surface":name,"mode":mode,"step":step,"windowId":identity,"rect":rectangle,
                    "palettePixelFraction":fresh,"stalePalettePixelFraction":stale}),
                ))
            })?;
            checks.push(check);
        }
    }
    let log = fs::read_to_string(fixture.output.join("shell.log"))?;
    for marker in [
        "ReferenceError:",
        "TypeError:",
        "Binding loop",
        "Failed to load configuration",
        "Cannot assign",
    ] {
        require(!log.contains(marker), format!("QML log contains {marker}"))?;
    }
    let result = json!({"passed":true,
        "scope":"Six owned windows kept mapped during light/dark/light atomic file updates; unchanged native identity and geometry, painted palette freshness",
        "checks":checks});
    fs::write(
        fixture.output.join("result.json"),
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )?;
    println!("PASS live appearance switching on six mapped owned windows");
    Ok(())
}
fn main() {
    if let Err(error) = options().and_then(run) {
        eprintln!("Live appearance fixture: {error}");
        std::process::exit(1);
    }
}
