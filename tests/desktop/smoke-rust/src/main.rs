//! Exercise the installed desktop shell in a disposable native compositor.
use serde_json::{Value, json};
mod keyboard;
use std::cell::RefCell;
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

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn write(path: &Path, value: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path, value)?;
    Ok(())
}
fn save_json(path: &Path, value: &Value) -> Result<()> {
    write(path, format!("{}\n", serde_json::to_string_pretty(value)?))
}
fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn number(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| fail(format!("{key} missing in {value}")))
}
fn integer(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| fail(format!("{key} missing in {value}")))
}
fn boolean(value: &Value, key: &str) -> Result<bool> {
    value[key]
        .as_bool()
        .ok_or_else(|| fail(format!("{key} missing in {value}")))
}

#[derive(Default)]
struct Options {
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    assets: Option<PathBuf>,
    click_pointer: Option<PathBuf>,
    drag_pointer: Option<PathBuf>,
    effects: bool,
    desktop_widgets: bool,
    menu_only: bool,
    theme: String,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut result = Options {
        theme: "light".into(),
        launcher: "lumen-desktop-shell".into(),
        sway: "sway".into(),
        ..Default::default()
    };
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let text = arg.to_str().ok_or("Non-UTF8 argument")?;
        match text {
            "--output" => result.output = PathBuf::from(args.next().ok_or("--output needs path")?),
            "--launcher" => {
                result.launcher = PathBuf::from(args.next().ok_or("--launcher needs path")?)
            }
            "--sway" => result.sway = PathBuf::from(args.next().ok_or("--sway needs path")?),
            "--assets" => {
                result.assets = Some(PathBuf::from(args.next().ok_or("--assets needs path")?))
            }
            "--click-pointer" => {
                result.click_pointer = Some(PathBuf::from(
                    args.next().ok_or("--click-pointer needs path")?,
                ))
            }
            "--drag-pointer" => {
                result.drag_pointer = Some(PathBuf::from(
                    args.next().ok_or("--drag-pointer needs path")?,
                ))
            }
            "--effects" => result.effects = true,
            "--desktop-widgets" => result.desktop_widgets = true,
            "--menu-only" => result.menu_only = true,
            "--theme" => {
                result.theme = args
                    .next()
                    .ok_or("--theme needs value")?
                    .to_string_lossy()
                    .into()
            }
            "--inside" => result.inside = true,
            _ => return Err(fail(format!("Unknown option: {text}"))),
        }
    }
    require(!result.output.as_os_str().is_empty(), "--output required")?;
    require(
        ["light", "dark"].contains(&result.theme.as_str()),
        "--theme must be light or dark",
    )?;
    result.output = if result.output.is_absolute() {
        result.output
    } else {
        env::current_dir()?.join(result.output)
    };
    Ok(result)
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = Path::new("/tmp").join(format!("luis-{}-{nonce}", std::process::id()));
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
fn private_session(opt: &Options) -> Result<()> {
    let root = PrivateRoot::new()?;
    let bus = root.0.join("bus.conf");
    write(
        &bus,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            root.0.display()
        ),
    )?;
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    vars.insert("HOME".into(), root.0.display().to_string());
    vars.insert("XDG_RUNTIME_DIR".into(), root.0.display().to_string());
    vars.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/absent-system", root.0.display()),
    );
    for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
        vars.insert(
            format!("XDG_{key}_HOME"),
            opt.output.join(key.to_lowercase()).display().to_string(),
        );
    }
    for (key, value) in [
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        vars.insert(key.into(), value.into());
    }
    for key in [
        "AT_SPI_BUS_ADDRESS",
        "DBUS_SESSION_BUS_ADDRESS",
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
    ] {
        vars.remove(key);
    }
    let executable = env::current_exe()?;
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let status = Command::new("dbus-run-session")
        .arg(format!("--config-file={}", bus.display()))
        .arg("--")
        .arg(executable)
        .args(args)
        .arg("--inside")
        .env_clear()
        .envs(&vars)
        .status()?;
    require(
        status.success(),
        format!("Private smoke session exited {status}"),
    )
}
fn resolve_command(name: &Path, path: &str) -> Result<PathBuf> {
    if name.components().count() > 1 {
        return Ok(name.to_path_buf());
    }
    for directory in env::split_paths(&OsString::from(path)) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("Command unavailable: {}", name.display())))
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
            return Err(fail(format!("Unsupported asset {}", path.display())));
        }
    }
    Ok(())
}
struct Managed(Child);
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
}
impl Drop for Managed {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
            let end = Instant::now() + Duration::from_secs(5);
            while self.0.try_wait().ok().flatten().is_none() {
                if Instant::now() > end {
                    unsafe {
                        libc::killpg(self.0.id() as i32, libc::SIGKILL);
                    }
                    let _ = self.0.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(30));
            }
        }
    }
}
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    require(
        output.status.success(),
        "Packaged launcher did not export environment",
    )?;
    let mut vars = BTreeMap::new();
    for entry in output.stdout.split(|byte| *byte == 0) {
        if let Some((key, value)) = std::str::from_utf8(entry)?.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    Ok(vars)
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
        thread::sleep(Duration::from_millis(150));
    }
}
fn truthy(value: bool) -> Option<()> {
    value.then_some(())
}

struct Driver {
    opt: Options,
    run: PathBuf,
    runtime: PathBuf,
    assets: PathBuf,
    vars: BTreeMap<String, String>,
    processes: Vec<Managed>,
    keyboard: RefCell<Option<keyboard::PhysicalKeyboard>>,
}
impl Driver {
    fn run_command(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .output()?;
        require(
            output.status.success(),
            format!(
                "{program} {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut words = vec![
            "-p",
            self.assets.to_str().ok_or("Non-UTF8 assets")?,
            "ipc",
            "call",
            "desktop",
            method,
        ];
        words.extend_from_slice(args);
        self.run_command("quickshell", &words)
    }
    fn ipc_json(&self, method: &str, args: &[&str]) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc(method, args)?)?)
    }
    fn state(&self) -> Result<Value> {
        self.ipc_json("filesStatus", &[])
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.keyboard.borrow_mut().as_mut().ok_or("Persistent keyboard unavailable")?.key(args)
    }
    fn capture(&self, name: &str) -> Result<()> {
        self.run_command(
            "grim",
            &[self.run.join(name).to_str().ok_or("Non-UTF8 output")?],
        )?;
        Ok(())
    }
    fn launch(&mut self, program: &Path, args: &[&str], name: &str) -> Result<()> {
        self.processes.push(Managed::launch(
            program,
            args,
            &self.vars,
            &self.run.join(format!("{name}.log")),
        )?);
        Ok(())
    }
    fn wait_for(&self, label: &str, mut predicate: impl FnMut() -> Result<bool>) -> Result<()> {
        if let Err(error) = wait(label, || predicate().map(truthy)) {
            self.save_failure();
            let _ = self.capture("failure.png");
            return Err(error);
        }
        Ok(())
    }
}

struct RgbImage {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}
impl RgbImage {
    fn open(path: &Path) -> Result<Self> {
        let mut decoder = png::Decoder::new(File::open(path)?);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buffer)?;
        let channels = match frame.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            other => return Err(fail(format!("Unsupported screenshot colour: {other:?}"))),
        };
        let bytes = buffer[..frame.buffer_size()]
            .chunks_exact(channels)
            .flat_map(|pixel| pixel[..3].to_vec())
            .collect();
        Ok(Self {
            width: frame.width as usize,
            height: frame.height as usize,
            bytes,
        })
    }
    fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let start = (y * self.width + x) * 3;
        [
            self.bytes[start],
            self.bytes[start + 1],
            self.bytes[start + 2],
        ]
    }
    fn moments(&self, rect: [usize; 4]) -> Result<([f64; 3], [f64; 3])> {
        require(
            rect[0] < rect[2]
                && rect[1] < rect[3]
                && rect[2] <= self.width
                && rect[3] <= self.height,
            "Invalid screenshot crop",
        )?;
        let count = ((rect[2] - rect[0]) * (rect[3] - rect[1])) as f64;
        let mut sums = [0.; 3];
        let mut squares = [0.; 3];
        for y in rect[1]..rect[3] {
            for x in rect[0]..rect[2] {
                let pixel = self.pixel(x, y);
                for channel in 0..3 {
                    let value = pixel[channel] as f64;
                    sums[channel] += value;
                    squares[channel] += value * value;
                }
            }
        }
        let mut means = [0.; 3];
        let mut deviations = [0.; 3];
        for channel in 0..3 {
            means[channel] = sums[channel] / count;
            deviations[channel] = (squares[channel] / count - means[channel] * means[channel])
                .max(0.)
                .sqrt();
        }
        Ok((means, deviations))
    }
    fn equal_crop(&self, other: &Self, rect: [usize; 4]) -> Result<bool> {
        require(
            self.width == other.width
                && self.height == other.height
                && rect[2] <= self.width
                && rect[3] <= self.height,
            "Incompatible screenshot crops",
        )?;
        for y in rect[1]..rect[3] {
            for x in rect[0]..rect[2] {
                if self.pixel(x, y) != other.pixel(x, y) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
fn find_node<'a>(value: &'a Value, predicate: &impl Fn(&Value) -> bool) -> Option<&'a Value> {
    if predicate(value) {
        return Some(value);
    }
    for key in ["nodes", "floating_nodes"] {
        for child in value[key].as_array().into_iter().flatten() {
            if let Some(node) = find_node(child, predicate) {
                return Some(node);
            }
        }
    }
    None
}
impl Driver {
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.run_command("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn window(&self, name: &str) -> Result<Value> {
        let tree = self.tree()?;
        find_node(&tree, &|node| {
            node["app_id"] == "org.quickshell" && node["name"] == name
        })
        .cloned()
        .ok_or_else(|| fail(format!("Window {name} unavailable")))
    }
    fn window_capture(&self, title: &str, filename: &str) -> Result<()> {
        self.review_window(title)?;
        self.wait_for(&format!("painted {title}"), || {
            let node = self.window(title)?;
            if !boolean(&node, "visible")?
                || (node["scratchpad_state"] != "none"
                    && !self.run.join("review-qhd.json").is_file())
            {
                return Ok(false);
            }
            let rect = &node["rect"];
            self.capture(filename)?;
            let image = RgbImage::open(&self.run.join(filename))?;
            let x1 = (integer(rect, "x")? + 20).max(0) as usize;
            let y1 = (integer(rect, "y")? + 50).max(0) as usize;
            let x2 = (integer(rect, "x")? + integer(rect, "width")? - 20)
                .min(image.width as i64)
                .max(0) as usize;
            let y2 = (integer(rect, "y")? + integer(rect, "height")? - 20)
                .min(image.height as i64)
                .max(0) as usize;
            if x2 <= x1 || y2 <= y1 {
                return Ok(false);
            }
            let (mean, deviation) = image.moments([x1, y1, x2, y2])?;
            Ok(mean.iter().copied().fold(0., f64::max) > 10.
                && deviation.iter().copied().fold(0., f64::max) > 2.)
        })
    }
    fn screenshot_mean(&self, name: &str, rect: [usize; 4]) -> Result<f64> {
        let (means, _) = RgbImage::open(&self.run.join(name))?.moments(rect)?;
        Ok(means.iter().sum::<f64>() / 3.)
    }
    fn save_failure(&self) {
        let mut value = serde_json::Map::new();
        for key in ["status", "filesStatus", "textToolsStatus", "layout"] {
            match self.ipc_json(key, &[]) {
                Ok(row) => {
                    value.insert(key.into(), row);
                }
                Err(error) => {
                    value.insert(format!("{key}Error"), json!(error.to_string()));
                }
            }
        }
        match self.tree() {
            Ok(tree) => {
                value.insert("tree".into(), tree);
            }
            Err(error) => {
                value.insert("treeError".into(), json!(error.to_string()));
            }
        }
        let _ = save_json(
            &self.run.join("readiness-failure.json"),
            &Value::Object(value),
        );
    }
}
fn setup(opt: Options) -> Result<Driver> {
    let exported = launcher_environment(&opt.launcher)?;
    let source_assets = if let Some(path) = &opt.assets {
        fs::canonicalize(path)?
    } else {
        fs::canonicalize(
            Path::new(
                exported
                    .get("LUMEN_DESKTOP_ASSETS")
                    .ok_or("Launcher assets missing")?,
            )
            .join("shell"),
        )?
    };
    let run = opt.output.clone();
    fs::create_dir(&run)?;
    let copied_assets = run.join("assets");
    copy_tree(
        source_assets.parent().ok_or("Assets have no parent")?,
        &copied_assets,
    )?;
    let assets = copied_assets.join(source_assets.file_name().ok_or("Assets have no name")?);
    if let Some(wallpaper) = env::var_os("LUMEN_RELEASE_WALLPAPER") { fs::set_permissions(assets.join("wallpaper.png"), fs::Permissions::from_mode(0o644))?; fs::copy(wallpaper, assets.join("wallpaper.png"))?; }
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    for key in [
        "PATH",
        "GLOBAL_MENU_PYTHON",
        "GLOBAL_MENU_BRIDGE",
        "QT_PLUGIN_PATH",
        "QML_IMPORT_PATH",
        "QT_QPA_PLATFORMTHEME",
        "QT_LINUX_ACCESSIBILITY_ALWAYS_ON",
        "LUMEN_FILE_JOBS_BIN",
        "LUMEN_NOTIFICATION_HELPER",
        "LUMEN_NEARBY_SHARING_LABEL",
        "LUMEN_SEARCH_BIN",
    ] {
        if let Some(value) = exported.get(key) {
            vars.insert(key.into(), value.clone());
        }
    }
    let runtime = PathBuf::from(vars.get("XDG_RUNTIME_DIR").ok_or("No private runtime")?);
    for (key, value) in [
        ("XDG_RUNTIME_DIR", runtime.display().to_string()),
        ("XDG_CONFIG_HOME", run.join("config").display().to_string()),
        ("XDG_STATE_HOME", run.join("state").display().to_string()),
        ("XDG_CACHE_HOME", run.join("cache").display().to_string()),
        ("XDG_DATA_HOME", run.join("data").display().to_string()),
        ("WLR_BACKENDS", "headless".into()),
        (
            "WLR_RENDERER",
            if opt.effects { "gles2" } else { "pixman" }.into(),
        ),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if opt.effects { "1" } else { "0" }.into(),
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1".into()),
        ("LIBGL_ALWAYS_SOFTWARE", "1".into()),
        ("WLR_LIBINPUT_NO_DEVICES", "1".into()),
        ("QT_QPA_PLATFORM", "wayland".into()),
        ("GDK_BACKEND", "wayland".into()),
        ("LUMEN_FILE_SERVICE", "0".into()),
        ("LUMEN_LAUNCH_SCOPES", "0".into()),
        ("LUMEN_NEARBY_SHARING_PROVIDER", "".into()),
        ("GLOBAL_MENU_ENABLED", "0".into()),
        ("QT_ACCESSIBILITY", "0".into()),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0".into()),
        ("NO_AT_BRIDGE", "1".into()),
        ("GTK_A11Y", "none".into()),
    ] {
        vars.insert(key.into(), value);
    }
    if !opt.effects {
        vars.insert("QT_QUICK_BACKEND".into(), "software".into());
    }
    for key in [
        "AT_SPI_BUS_ADDRESS",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
    ] {
        vars.remove(key);
    }
    let docs = run.join("Documents");
    fs::create_dir(&docs)?;
    write(&docs.join("example.txt"), "US English preview fixture")?;
    vars.insert(
        "LUMEN_SEARCH_ROOTS".into(),
        json!([docs.display().to_string()]).to_string(),
    );
    let fixture_bin = run.join("bin");
    fs::create_dir(&fixture_bin)?;
    let provider = resolve_command(
        Path::new("lumen-fixture-providers"),
        vars.get("PATH").ok_or("PATH missing")?,
    )?;
    let isolated = Command::new(&provider)
        .args([
            "isolate",
            assets.to_str().ok_or("Bad assets path")?,
            run.to_str().ok_or("Bad output path")?,
        ])
        .env_clear()
        .envs(&vars)
        .output()?;
    require(
        isolated.status.success(),
        format!(
            "Provider isolation failed: {}",
            String::from_utf8_lossy(&isolated.stderr)
        ),
    )?;
    let _: Value = serde_json::from_slice(&isolated.stdout)?;
    let calendar = fs::canonicalize(resolve_command(
        Path::new("lumen-fixture-calendar"),
        vars.get("PATH").ok_or("PATH missing")?,
    )?)?;
    require(
        calendar.starts_with("/nix/store") && calendar.is_file(),
        "Pinned Rust calendar fixture required",
    )?;
    symlink(calendar, fixture_bin.join("lumen-calendar"))?;
    vars.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            fixture_bin.display(),
            vars.get("PATH").ok_or("PATH missing")?
        ),
    );
    if opt.desktop_widgets {
        let widget = run.join("state/lumen-desktop/session/notification-centre.json");
        fs::create_dir_all(widget.parent().ok_or("Widget state parent")?)?;
        save_json(
            &widget,
            &json!({"desktopVisible":true,"desktopStyle":"agenda","desktopColor":"monochrome","positions":{"HEADLESS-1":{"x":0.1,"y":0.2}}}),
        )?;
    }
    Ok(Driver {
        opt,
        run,
        runtime,
        assets,
        vars,
        processes: Vec::new(),
        keyboard: RefCell::new(None),
    })
}
impl Driver {
    fn review_providers(&mut self) -> Result<()> {
        let mut defaults:Value=serde_json::from_str(&self.run_command("lumen-nix-settings", &["read"])?)?;
        defaults["notifications"]["preview"]=json!("always");
        defaults["appearance"]["mode"]=json!(self.opt.theme);
        defaults["menuBar"]["hourCycle"]=json!("12");
        let settings=self.run.join("review-settings.json");save_json(&settings,&defaults)?;
        self.vars.insert("LUMEN_REVIEW_SETTINGS".into(),settings.display().to_string());
        let helper=resolve_command(Path::new("lumen-fixture-settings"),self.vars.get("PATH").ok_or("PATH missing")?)?;
        symlink(&helper,self.run.join("bin/lumen-nix-settings"))?;
        symlink(&helper,self.run.join("bin/lumen-system-controls"))?;
        symlink(&helper,self.run.join("bin/lumen-display-settings"))?;
        symlink(&helper,self.run.join("bin/lumen-nearby-sharing"))?;
        symlink(&helper,self.run.join("bin/desktop-network-diagnose"))?;
        symlink(&helper,self.run.join("bin/lumen-privacy-status"))?;
        symlink(&helper,self.run.join("bin/lumen-portal-permissions"))?;
        self.vars.insert("LUMEN_NEARBY_SHARING_PROVIDER".into(),helper.display().to_string());
        self.vars.insert("LUMEN_NIX_SETTINGS_BIN".into(),helper.display().to_string());
        self.vars.insert("LUMEN_REVIEW_POPULATED".into(),"1".into());
        self.vars.insert("LUMEN_NEARBY_SHARING_ICON".into(),env::var("LUMEN_REVIEW_SHARING_ICON").unwrap_or_default());
        self.vars.insert("LUMEN_SEARCH_ROOTS".into(),json!([self.run.join("Documents").display().to_string(),self.run.join("Studio").display().to_string()]).to_string());
        if let Some(binary)=env::var_os("LUMEN_REVIEW_GSETTINGS_BIN") {
            let binary=PathBuf::from(binary);require(binary.starts_with("/nix/store") && binary.is_file(),"Immutable settings backend required")?;
            symlink(binary,self.run.join("bin/gsettings"))?;
        }
        let appearance=PathBuf::from(env::var_os("LUMEN_REVIEW_APPEARANCE_BIN").ok_or("Pinned review appearance runtime missing")?);
        require(appearance.starts_with("/nix/store") && appearance.is_file(),"Immutable appearance runtime required")?;
        symlink(appearance,self.run.join("bin/lumen-appearance"))?;
        if let Some(bundle)=env::var_os("LUMEN_REVIEW_SHORTCUT_BUNDLE") {
            let bundle=read_json(Path::new(&bundle))?;let config=self.run.join("config/lumen-desktop");fs::create_dir_all(&config)?;
            for (key,name) in [("registry","shortcuts.json"),("editor","shortcut-editor.json")] {fs::copy(bundle[key].as_str().ok_or("Missing shortcut fixture")?,config.join(name))?;}
        }
        self.vars.insert("PIPEWIRE_REMOTE".into(),"lumen-review-audio".into());
        self.vars.insert("PIPEWIRE_RUNTIME_DIR".into(),self.runtime.display().to_string());
        self.vars.insert("PULSE_SERVER".into(),format!("unix:{}/absent-pulse",self.runtime.display()));
        let audio=self.run.join("review-audio.conf");
        write(&audio,r#"
context.properties = { core.daemon = true core.name = lumen-review-audio }
context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert support.* = support/libspa-support }
context.modules = [
 { name = libpipewire-module-protocol-native }
 { name = libpipewire-module-spa-node-factory }
 { name = libpipewire-module-client-node }
 { name = libpipewire-module-adapter }
 { name = libpipewire-module-metadata }
 { name = libpipewire-module-access }
]
context.objects = [
 { factory = metadata args = { metadata.name = default metadata.values = [
   { key = default.audio.sink type = "Spa:String:JSON" value = { name = review-speakers } }
   { key = default.audio.source type = "Spa:String:JSON" value = { name = review-microphone } }
 ] } }
 { factory = adapter args = { factory.name = support.null-audio-sink node.name = review-speakers node.description = "Studio Speakers" media.class = Audio/Sink audio.position = [ FL FR ] } }
 { factory = adapter args = { factory.name = support.null-audio-sink node.name = review-headphones node.description = "Studio Headphones" media.class = Audio/Sink audio.position = [ FL FR ] } }
 { factory = adapter args = { factory.name = support.null-audio-sink node.name = review-microphone node.description = "Studio Microphone" media.class = Audio/Source audio.position = [ MONO ] } }
]
"#)?;
        self.launch(Path::new("pipewire"), &["-c",audio.to_str().ok_or("Audio config path")?],"review-audio")?;
        self.wait_for("Private audio socket",||Ok(self.runtime.join("lumen-review-audio").exists()))?;
        if let Some(player)=env::var_os("LUMEN_REVIEW_MPRIS_BIN") {
            self.vars.insert("DBUS_SYSTEM_BUS_ADDRESS".into(),self.vars.get("DBUS_SESSION_BUS_ADDRESS").ok_or("Private review bus missing")?.clone());
            self.vars.insert("LUMEN_REVIEW_BLUETOOTH".into(),"1".into());
            let player=PathBuf::from(player);require(player.starts_with("/nix/store") && player.is_file(),"Immutable media fixture required")?;
            self.vars.insert("LUMEN_MPRIS_CALLS".into(),self.run.join("review-media-actions.jsonl").display().to_string());
            self.launch(&player,&[],"review-media")?;
            self.wait_for("Private media source",||Ok(self.run_command("gdbus", &["call","--session","--dest","org.lumen.MediaFixture","--object-path","/org/lumen/MediaFixture","--method","org.lumen.MediaFixture.Add",r#"{"name":"studio","identity":"Studio Music"}"#]).is_ok()))?;
            self.run_command("gdbus", &["call","--session","--dest","org.lumen.MediaFixture","--object-path","/org/lumen/MediaFixture","--method","org.lumen.MediaFixture.Configure","studio",r#"{"PlaybackStatus":"Playing","Metadata":{"mpris:trackid":"/review/track","mpris:length":240000000,"xesam:title":"Morning Light","xesam:artist":["Studio Sessions"]}}"#])?;
        }
        Ok(())
    }

    fn review_notify(&self, app:&str, title:&str, body:&str) -> Result<()> {
        self.run_command("gdbus", &["call","--session","--dest","org.freedesktop.Notifications",
            "--object-path","/org/freedesktop/Notifications","--method","org.freedesktop.Notifications.Notify",
            app,"0","",title,body,"[]","{}","0"])?;
        Ok(())
    }

    fn compositor(&mut self) -> Result<Value> {
        let mut config = String::from(
            "output HEADLESS-1 resolution 1440x1000\nxwayland disable\ndefault_border none\nfor_window [app_id=\"org.quickshell\"] floating enable\n",
        );
        config.push_str(&format!("output HEADLESS-1 bg {} fill\n", serde_json::to_string(&self.assets.join("wallpaper.png").display().to_string())?));
        if self.opt.effects {
            config.push_str("corner_radius 12\nshadows enable\nshadow_blur_radius 28\nanimation_duration_ms 0\nblur enable\nblur_xray disable\nlayer_effects \"lumen-dock\" \"blur enable; blur_ignore_transparent enable; corner_radius 0\"\nlayer_effects \"lumen-menu-popup\" \"blur enable; blur_ignore_transparent enable\"\n");
        }
        let conf = self.run.join("sway.conf");
        write(&conf, config)?;
        let sway = resolve_command(&self.opt.sway, self.vars.get("PATH").ok_or("PATH missing")?)?;
        let conf_text = conf.to_str().ok_or("Non-UTF8 sway config")?;
        let mut args = vec!["--unsupported-gpu"];
        if self.opt.effects {
            args.push("-d");
        }
        args.extend_from_slice(&["-c", conf_text]);
        self.launch(&sway, &args, "sway")?;
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
        let capabilities: Value =
            serde_json::from_str(&self.run_command("swaymsg", &["-t", "get_version", "-r"])?)?;
        for (key, capability) in [
            ("LUMEN_NATIVE_WINDOW_CONTROLS", "native_window_controls"),
            ("LUMEN_NATIVE_WINDOW_ACTIONS", "native_window_actions"),
            ("LUMEN_UNIFIED_OWNED_HEADERS", "native_window_actions"),
            ("LUMEN_COMPOSITOR_EFFECTS", "compositor_effects"),
        ] {
            self.vars.insert(
                key.into(),
                if capabilities[capability] == true {
                    "1"
                } else {
                    "0"
                }
                .into(),
            );
        }
        let palettes = read_json(
            &self
                .assets
                .parent()
                .ok_or("Assets parent missing")?
                .join("appearance/palette.json"),
        )?;
        let appearance = if env::var_os("LUMEN_RELEASE_CAPTURE").is_some() {self.run.join("state/lumen-desktop/appearance.json")} else {self.run.join("appearance.json")};
        fs::create_dir_all(appearance.parent().ok_or("Appearance parent missing")?)?;
        save_json(
            &appearance,
            &json!({"font":palettes["font"],"palette":palettes[&self.opt.theme],"preferences":{"mode":self.opt.theme,"reduceMotion":true},"menuBarPreferences":if env::var_os("LUMEN_RELEASE_CAPTURE").is_some(){read_json(&self.run.join("review-settings.json"))?["menuBar"].clone()}else{Value::Null}}),
        )?;
        self.vars.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            appearance.display().to_string(),
        );
        let jobs = PathBuf::from(
            self.vars
                .get("LUMEN_FILE_JOBS_BIN")
                .ok_or("Pinned File Jobs binary missing")?,
        );
        require(jobs.is_file(), "Pinned File Jobs binary unavailable")?;
        self.launch(&jobs, &["serve"], "file-jobs")?;
        self.wait_for("File Jobs service", || {
            Ok(self.runtime.join("lumen-file-operations.sock").exists())
        })?;
        let assets = self
            .assets
            .to_str()
            .ok_or("Non-UTF8 shell path")?
            .to_owned();
        self.launch(Path::new("quickshell"), &["-p", &assets], "shell")?;
        self.keyboard.replace(Some(keyboard::PhysicalKeyboard::start(
            Path::new(env!("LUMEN_SMOKE_KEYBOARD")), &self.vars, &self.run.join("keyboard.log"),
        )?));
        if let Err(error) =
            self.wait_for("shell status", || Ok(self.ipc_json("status", &[]).is_ok()))
        {
            self.save_failure();
            return Err(error);
        }
        Ok(capabilities)
    }
    fn menu_only(&self) -> Result<()> {
        let mut layouts = Vec::new();
        for width in [1080, 720, 480] {
            self.run_command(
                "swaymsg",
                &[&format!("output HEADLESS-1 resolution {width}x1000")],
            )?;
            self.wait_for("menu width", || {
                Ok(self.ipc_json("menuLayoutStatus", &[])?[0]["width"] == width)
            })?;
            let layout = self.ipc_json("menuLayoutStatus", &[])?[0].clone();
            require(
                number(&layout, "menuEnd")? <= number(&layout, "statusStart")? - 4.,
                format!("Menu and status collided: {layout}"),
            )?;
            if width == 480 {
                require(
                    integer(&layout, "overflow")? > 0,
                    format!("Expected 480px overflow: {layout}"),
                )?;
            }
            if integer(&layout, "overflow")? > 0 {
                self.ipc("menu", &["Overflow"])?;
                self.wait_for("overflow menu", || {
                    Ok(self.ipc_json("layout", &[])?[0]["menu"] == "Overflow")
                })?;
                self.key(&["-k", "Down"])?;
                self.wait_for("overflow selection", || {
                    Ok(self.ipc_json("layout", &[])?[0]["selectedIndex"]
                        .as_i64()
                        .unwrap_or(-1)
                        >= 0)
                })?;
                thread::sleep(Duration::from_millis(300));
                self.capture(&format!("menu-overflow-{width}.png"))?;
                self.key(&["-k", "Return"])?;
                self.wait_for("overflow activation", || {
                    Ok(self.ipc_json("layout", &[])?[0]["menu"] != "Overflow")
                })?;
                self.key(&["-k", "Escape"])?;
            }
            layouts.push(layout);
        }
        self.check_shell_log(&[
            "Failed to load configuration",
            "ReferenceError:",
            "TypeError:",
            "polish() loop",
            "Binding loop detected",
            "Unable to assign",
        ])?;
        let result = json!({"passed":true,"checks":["1080/720/480 no menu-status collision","hidden menus and status actions in overflow","real keyboard selection, activation and dismissal"],"layouts":layouts});
        save_json(&self.run.join("result.json"), &result)?;
        println!("{}", self.run.join("result.json").display());
        Ok(())
    }
    fn check_shell_log(&self, markers: &[&str]) -> Result<()> {
        let log = fs::read_to_string(self.run.join("shell.log"))?;
        for marker in markers {
            require(
                !log.contains(marker),
                format!("Shell log contains {marker}: {log}"),
            )?;
        }
        Ok(())
    }
    fn widgets(&self) -> Result<()> {
        self.wait_for("desktop widget", || {
            Ok(self.ipc_json("desktopWidgetStatus", &[])?["outputs"]
                .as_array()
                .is_some_and(|items| !items.is_empty()))
        })?;
        let state = self.ipc_json("desktopWidgetStatus", &[])?;
        require(
            state["error"].as_str().unwrap_or("").is_empty(),
            format!("Widget error: {state}"),
        )?;
        let valid = state["outputs"].as_array().is_some_and(|items| {
            items.iter().any(|widget| {
                widget["visible"] == true
                    && widget["widgetWidth"].as_f64().unwrap_or(0.) > 100.
                    && widget["widgetHeight"].as_f64().unwrap_or(0.) > 100.
                    && widget["monochrome"] == true
            })
        });
        require(valid, format!("Desktop widget failed: {state}"))?;
        save_json(&self.run.join("desktop-widgets.json"), &state)?;
        thread::sleep(Duration::from_millis(500));
        self.capture("desktop-widgets.png")?;
        Ok(())
    }
    fn settings_node(&self) -> Result<Value> {
        self.window("System Settings")
    }
    fn press_control(&self, index: i64, tool: &Path) -> Result<()> {
        let node = self.settings_node()?;
        let rect = &node["rect"];
        self.run_command(
            "swaymsg",
            &[&format!(
                "seat seat0 cursor set {} {}",
                integer(rect, "x")? + 20 + 20 * index,
                integer(rect, "y")? + 24
            )],
        )?;
        self.run_command(
            tool.to_str().ok_or("Non-UTF8 click tool")?,
            &["pointer", "click", "left"],
        )?;
        Ok(())
    }
    fn check_header_buttons(&self, tool: &Path) -> Result<()> {
        let before = self.settings_node()?["rect"].clone();
        self.press_control(2, tool)?;
        self.wait_for("Settings maximize", || {
            Ok(self.settings_node()?["rect"] != before)
        })?;
        require(
            self.settings_node()?["fullscreen_mode"] == 1,
            "Full-screen header action did not enter full screen",
        )?;
        self.press_control(2, tool)?;
        self.wait_for("Settings restore", || {
            let node = self.settings_node()?;
            let rect = &node["rect"];
            Ok(node["fullscreen_mode"] == 0
                && rect["width"] == before["width"]
                && rect["height"] == before["height"]
                && (integer(rect, "x")? - integer(&before, "x")?).abs() <= 20
                && (integer(rect, "y")? - integer(&before, "y")?).abs() <= 20)
        })?;
        self.press_control(1, tool)?;
        self.wait_for("Settings minimize", || {
            Ok(self.settings_node()?["scratchpad_state"] != "none")
        })?;
        self.ipc("activate", &["lumen-system-settings"])?;
        self.wait_for("Settings activate", || {
            Ok(self.settings_node()?["scratchpad_state"] == "none")
        })?;
        let restored = self.settings_node()?["rect"].clone();
        require(
            restored["width"] == before["width"] && restored["height"] == before["height"],
            "Settings size changed after activate",
        )?;
        self.press_control(0, tool)?;
        self.wait_for("Settings close", || {
            Ok(self.ipc_json("settingsStatus", &[])?["visible"] == false)
        })?;
        println!("PASS owned Settings header buttons and Dock activation restore placement");
        Ok(())
    }
    fn basic_checks(&self) -> Result<Value> {
        let docs = self.run.join("Documents");
        self.ipc("files", &[docs.to_str().ok_or("Non-UTF8 Documents")?])?;
        self.wait_for("Files rows", || {
            let state = self.state()?;
            Ok(state["count"] == 1 && state["busy"] == false)
        })?;
        for (number, view) in [
            ("1", "grid"),
            ("2", "list"),
            ("3", "columns"),
            ("4", "gallery"),
        ] {
            self.key(&["-M", "ctrl", "-k", number, "-m", "ctrl"])?;
            self.wait_for("Files view", || Ok(self.state()?["view"] == view))?;
            thread::sleep(Duration::from_millis(300));
            self.capture(&format!("files-{view}.png"))?;
        }
        self.key(&["-k", "Down", "-k", "space"])?;
        self.wait_for("Quick Look text", || {
            Ok(self.ipc_json("quickLookStatus", &[])?["type"] == "text")
        })?;
        self.key(&["-k", "Escape"])?;
        self.ipc("textTools", &[])?;
        self.wait_for("Text Tools visible", || {
            Ok(self.ipc_json("textToolsStatus", &[])?["visible"] == true)
        })?;
        self.wait_for("Text Tools native keyboard focus", || {
            Ok(self.ipc_json("status", &[])?["windows"].as_array().is_some_and(|rows| rows.iter().any(|row| row["title"] == "Text Tools" && row["active"] == true)))
        })?;
        for page in [
            "Speech",
            "Replacements",
            "Shortcuts",
            "Spelling",
            "Dictation",
            "Services",
            "Fonts",
            "Colours",
        ] {
            self.key(&["-M", "ctrl", "-k", "Tab", "-m", "ctrl"])?;
            self.wait_for("Text Tools page", || {
                Ok(self.ipc_json("textToolsStatus", &[])?["page"] == page)
            })?;
        }
        self.key(&["-k", "Escape"])?;
        self.ipc("focusDock", &[])?;
        thread::sleep(Duration::from_millis(300));
        self.key(&["-k", "End", "-k", "Return"])?;
        self.wait_for("Dock Trash", || Ok(self.state()?["path"] == "trash:"))?;
        self.window_capture("Files — Trash", "files.png")?;
        self.ipc("settings", &["Appearance"])?;
        self.wait_for("Settings appearance", || {
            Ok(self.ipc_json("settingsStatus", &[])?["busy"] == false)
        })?;
        self.window_capture("System Settings", "settings.png")?;
        self.ipc("settings", &["VPN"])?;
        self.wait_for("Settings VPN", || {
            Ok(self.ipc_json("settingsStatus", &[])?["page"] == "VPN")
        })?;
        self.window_capture("System Settings", "vpn-settings.png")?;
        Ok(json!([
            "Files four views",
            "Quick Look text",
            "Text Tools keyboard pages",
            "Dock keyboard Trash",
            "100/125/150/200 percent scales",
            "Calendar Accounts UI",
            "Notification Centre idempotent open/close and unscaled viewport at 100/200 percent"
        ]))
    }
    fn drag_checks(&self, tool: &Path, checks: &mut Vec<String>) -> Result<()> {
        self.run_command("swaymsg", &["output HEADLESS-1 scale 1"])?;
        thread::sleep(Duration::from_millis(400));
        for (number, view, start, delta) in [
            ("1", "grid", (366, 150), (-112, 0)),
            ("2", "list", (300, 177), (0, -31)),
        ] {
            let folder = self.run.join(format!("drag-{view}"));
            fs::create_dir(&folder)?;
            fs::create_dir(folder.join("Target"))?;
            let payload = folder.join("payload.txt");
            write(&payload, "stationary spring-drop fixture")?;
            self.ipc("files", &[folder.to_str().ok_or("Non-UTF8 drag folder")?])?;
            self.wait_for("drag fixture folder", || {
                let state = self.state()?;
                Ok(state["path"] == folder.display().to_string()
                    && state["count"] == 2
                    && state["busy"] == false)
            })?;
            self.key(&["-M", "ctrl", "-k", number, "-m", "ctrl"])?;
            self.wait_for("drag fixture view", || Ok(self.state()?["view"] == view))?;
            thread::sleep(Duration::from_millis(400));
            let tree = self.tree()?;
            let window = find_node(&tree, &|node| {
                node["focused"] == true && node["app_id"] == "org.quickshell"
            })
            .ok_or("Focused Quickshell drag window unavailable")?;
            let rect = &window["rect"];
            self.run_command(
                "swaymsg",
                &[&format!(
                    "seat seat0 cursor set {} {}",
                    integer(rect, "x")? + start.0,
                    integer(rect, "y")? + start.1
                )],
            )?;
            self.run_command(
                tool.to_str().ok_or("Non-UTF8 drag pointer tool")?,
                &[
                    "pointer",
                    "move",
                    &delta.0.to_string(),
                    &delta.1.to_string(),
                ],
            )?;
            self.wait_for("spring open drop", || {
                Ok(folder.join("Target/payload.txt").exists() && self.state()?["working"] == false)
            })?;
            require(
                self.state()?["path"] == folder.join("Target").display().to_string(),
                "Spring target path incorrect",
            )?;
            require(
                fs::read(&payload)? == fs::read(folder.join("Target/payload.txt"))?,
                "Spring drop changed payload",
            )?;
            checks.push(format!(
                "{view} spring-open/drop without further pointer movement; source retained"
            ));
        }
        Ok(())
    }

    fn review_window(&self,title:&str) -> Result<()> {
        if !self.run.join("review-qhd.json").is_file(){return Ok(());}
        let Ok(scale)=env::var("LUMEN_RELEASE_SCALE") else{return Ok(());};let scale:f64=scale.parse()?;
        let node=self.window(title)?;let desired:(f64,f64)=match title {"Calendar"=>(1500.0,980.0),"System Settings"=>(1100.0,950.0),"Files — Studio"=>(1400.0,900.0),_=>(1000.0,740.0)};
        let width=desired.0.min(2560.0/scale-100.0) as i64;let height=desired.1.min(1440.0/scale-180.0) as i64;
        for _ in 0..20 {
            let tree=self.tree()?;
            let other=find_node(&tree,&|n|n["app_id"]=="org.quickshell" && n["id"]!=node["id"] && n["visible"]==true);
            let Some(other)=other else{break;};
            self.run_command("swaymsg",&[&format!("[con_id={}] move scratchpad",other["id"])])?;
        }
        self.run_command("swaymsg",&[&format!("[con_id={}] move container to workspace number 1, floating enable, resize set width {width} px height {height} px, move position center, focus",node["id"])])?;
        thread::sleep(Duration::from_millis(350));
        if let Some(pointer)=env::var_os("LUMEN_REVIEW_POINTER_BIN") {
            let pointer=PathBuf::from(pointer);require(pointer.starts_with("/nix/store") && pointer.is_file(),"Immutable pointer required")?;
            let rect=self.window(title)?["rect"].clone();
            self.run_command("swaymsg",&[&format!("seat seat0 cursor set {} {}",integer(&rect,"x")?+180,integer(&rect,"y")?+18)])?;
            self.run_command(pointer.to_str().ok_or("Pointer path")?,&["pointer","click","left"])?;
            thread::sleep(Duration::from_millis(200));
        }
        Ok(())
    }
    fn review_application_capture(&mut self,kind:&str,variable:&str) -> Result<()> {
        let Some(binary)=env::var_os(variable) else{return Ok(());};
        let binary=PathBuf::from(binary);require(binary.starts_with("/nix/store") && binary.is_file(),"Immutable review application required")?;
        self.run_command("swaymsg",&["[app_id=\"org.quickshell\"] move scratchpad"])?;
        let before=self.tree()?;let profile=self.run.join("data").join(format!("review-{kind}"));fs::create_dir_all(&profile)?;
        let previous_vars=self.vars.clone();self.vars.retain(|key,_|!key.starts_with("CODEX_") && !key.starts_with("OPENAI_"));
        self.vars.insert("CODEX_HOME".into(),profile.join("codex-home").display().to_string());
        self.vars.insert("CODEX_MULTI_LAUNCH".into(),"1".into());
        self.vars.insert("CODEX_LINUX_DISABLE_USAGE_REPORTING".into(),"1".into());
        let process_count=self.processes.len();let profile_arg=format!("--user-data-dir={}",profile.display());let class_arg=format!("--class=org.lumen.review-{kind}");
        let mut args=vec!["--ozone-platform=wayland","--no-first-run","--no-default-browser-check",profile_arg.as_str(),class_arg.as_str()];
        if kind=="emoji" {args=vec!["--standalone"];}else if self.opt.theme=="dark" {args.push("--force-dark-mode");}
        if kind=="chatgpt" {args.push("--app=https://chatgpt.com");}
        self.launch(&binary,&args,&format!("review-{kind}"))?;
        let mut observed=Value::Null;
        self.wait_for("Disposable application window",||{let tree=self.tree()?;if let Some(node)=find_node(&tree,&|n|n["app_id"].as_str().is_some_and(|id|!id.is_empty() && id!="org.quickshell") && find_node(&before,&|old|old["id"]==n["id"]).is_none()){observed=node.clone();Ok(true)}else{Ok(false)}})?;
        let scale=env::var("LUMEN_RELEASE_SCALE").unwrap_or("1".into()).parse::<f64>()?;
        let width=(2560.0/scale-100.0).min(1500.0) as i64;let height=(1440.0/scale-160.0).min(1000.0) as i64;
        let placement=if kind=="emoji" {"floating enable, move position center, focus".to_owned()} else {format!("floating enable, resize set width {width} px height {height} px, move position center, focus")};
        self.run_command("swaymsg",&[&format!("[con_id={}] {placement}",observed["id"])])?;
        thread::sleep(Duration::from_secs(5));
        let settled=self.tree()?;
        if let Some(node)=find_node(&settled,&|n|n["focused"]==true && n["app_id"].as_str().is_some_and(|id|!id.is_empty() && id!="org.quickshell")) {observed=node.clone();}
        self.run_command("swaymsg",&[&format!("[con_id={}] {placement}",observed["id"])])?;
        thread::sleep(Duration::from_millis(700));self.capture(&format!("{kind}-app.png"))?;
        save_json(&self.run.join(format!("{kind}-tree.json")),&self.tree()?)?;
        save_json(&self.run.join(format!("{kind}-capture.json")),&json!({"window":observed["name"],"profile":"disposable","credentialsProvided":false}))?;
        self.processes.truncate(process_count);self.vars=previous_vars;Ok(())
    }
    fn release_capture(&mut self, checks: Vec<String>) -> Result<()> {
        if let Ok(scale)=env::var("LUMEN_RELEASE_SCALE") {
            let scale_number: f64=scale.parse()?;require([1.0,1.25,1.5].contains(&scale_number),"Unsupported review scale")?;
            self.run_command("swaymsg",&[&format!("output HEADLESS-1 resolution 2560x1440 scale {scale}")])?;
            let expected=(2560.0/scale_number) as i64;
            self.wait_for("QHD review output geometry",||Ok(self.ipc_json("status",&[])?["screens"].as_array().is_some_and(|rows|rows.iter().any(|r|r["name"]=="HEADLESS-1" && r["width"].as_i64()==Some(expected)))))?;
            save_json(&self.run.join("review-qhd.json"),&json!({"width":2560,"height":1440,"scale":scale_number}))?;
        }
        let studio = self.run.join("Documents/Review/Studio");
        fs::create_dir_all(&studio)?;
        for name in ["Projects", "Documents", "Images", "Notes"] { fs::create_dir_all(studio.join(name))?; }
        write(&studio.join("Release Notes.txt"), "Lumen Desktop\n\nA shared desktop experience.\n\nReview the shell, Files, Calendar and Settings.\n")?;
        write(&studio.join("Project Plan.csv"), "Milestone,Status\nDesign review,In progress\nVisual baseline,Review\nRelease,Planned\n")?;
        fs::copy(self.assets.join("wallpaper.png"), studio.join("Landscape.png"))?;
        for (name,contents) in [("README.md","# Studio\n\nA synthetic review workspace.\n"),("Project.json",r#"{"name":"Lumen Review","status":"in progress"}"#),("Welcome.html","<!doctype html><title>Studio</title><h1>Welcome to the studio</h1>"),("Poster.svg",r##"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480"><rect width="640" height="480" fill="#336699"/><text x="40" y="240" fill="white" font-size="48">Studio</text></svg>"##)] {write(&studio.join(name),contents)?;}
        // Valid minimal media/archive files and a long name exercise varied list geometry.
        let mut wave=Vec::from(&b"RIFF"[..]);wave.extend(40u32.to_le_bytes());wave.extend(b"WAVEfmt ");wave.extend(16u32.to_le_bytes());wave.extend(1u16.to_le_bytes());wave.extend(1u16.to_le_bytes());wave.extend(8000u32.to_le_bytes());wave.extend(16000u32.to_le_bytes());wave.extend(2u16.to_le_bytes());wave.extend(16u16.to_le_bytes());wave.extend(b"data");wave.extend(4u32.to_le_bytes());wave.extend([0u8;4]);fs::write(studio.join("Studio ambience.wav"),wave)?;
        fs::write(studio.join("Studio archive.zip"),[80u8,75,5,6,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0])?;
        write(&studio.join("Studio presentation with a deliberately long descriptive name for the upcoming release review.txt"),"Synthetic long-name sample.\n")?;
        self.ipc("files", &[studio.to_str().ok_or("Studio path")?])?;
        self.wait_for("Studio content", || Ok(self.state()?["busy"] == false && self.state()?["count"] == 14))?;
        self.wait_for("Studio focus", || Ok(self.ipc_json("status", &[])?["windows"].as_array().is_some_and(|rows| rows.iter().any(|r| r["title"].as_str().unwrap_or("").contains("Studio") && r["active"] == true))))?;
        self.review_window("Files — Studio")?;
        for (view_key, view) in [("1","grid"),("2","list"),("3","columns"),("4","gallery")] {
            self.key(&["-M","ctrl","-k",view_key,"-m","ctrl"])?;
            self.wait_for("Studio view", || Ok(self.state()?["view"] == view))?;
            thread::sleep(Duration::from_millis(450));
            self.capture(&format!("files-{view}.png"))?;
            self.key(&["-M","ctrl","-k","a","-m","ctrl"])?;
            self.wait_for("All sample files selected",||Ok(self.state()?["selection"].as_array().is_some_and(|rows|rows.len()==14)))?;
            thread::sleep(Duration::from_millis(200));
            self.capture(&format!("files-{view}-selected.png"))?;
            self.key(&["-M","shift","-k","F10","-m","shift"])?;
            self.wait_for("Selected file actions",||Ok(self.state()?["contextMenu"]==true))?;
            thread::sleep(Duration::from_millis(250));
            self.capture(&format!("files-{view}-actions.png"))?;
            let placement=self.state()?["contextMenuRect"].clone();
            require(number(&placement,"x")?>=0. && number(&placement,"x")?<number(&placement,"windowWidth")? && number(&placement,"y")?>=0. && number(&placement,"y")?<number(&placement,"windowHeight")?,"Keyboard context menu starts outside its owner")?;
            save_json(&self.run.join(format!("files-{view}-actions-placement.json")),&placement)?;
            self.key(&["-k","Escape"])?;
            self.wait_for("File actions dismissed",||Ok(self.state()?["contextMenu"]==false))?;
            if view=="list" {
                if let Some(pointer)=env::var_os("LUMEN_REVIEW_POINTER_BIN") {
                    let node=self.window("Files — Studio")?;let rect=&node["rect"];
                    self.run_command("swaymsg",&[&format!("seat seat0 cursor set {} {}",integer(rect,"x")?+integer(rect,"width")?-24,integer(rect,"y")?+number(&placement,"y")? as i64)])?;
                    self.run_command(Path::new(&pointer).to_str().ok_or("Pointer path")?,&["pointer","click","right"])?;
                    self.wait_for("Pointer context menu",||Ok(self.state()?["contextMenu"]==true))?;
                    thread::sleep(Duration::from_millis(250));
                    let edge=self.state()?["contextMenuRect"].clone();
                    if env::var("LUMEN_RELEASE_SCALE").unwrap_or("1".into())=="1" {require(number(&edge,"x")?+number(&edge,"width")?>number(&edge,"windowWidth")?,"Pointer context menu remained constrained to its owner")?;}
                    save_json(&self.run.join("files-context-overflow-placement.json"),&edge)?;
                    self.capture("files-context-overflow.png")?;
                    self.key(&["-k","Escape"])?;
                    self.wait_for("Pointer actions dismissed",||Ok(self.state()?["contextMenu"]==false))?;
                }
            }
            self.key(&["-k","Escape"])?;
        }
        self.key(&["-M","ctrl","-k","2","-m","ctrl"])?;
        self.key(&["-k","Down","-k","Down","-k","Down","-k","Down","-k","Down"])?;
        self.wait_for("Image selection",||Ok(self.state()?["selected"]=="Landscape.png"))?;
        self.key(&["-M","ctrl","-k","4","-m","ctrl"])?;
        self.wait_for("Selected image in Gallery",||Ok(self.state()?["view"]=="gallery" && self.state()?["selected"]=="Landscape.png"))?;
        thread::sleep(Duration::from_millis(350));self.capture("files-gallery-image.png")?;
        self.key(&["-k","space"])?;
        self.wait_for("Image Quick Look",||Ok(self.ipc_json("quickLookStatus",&[])?["type"]=="image"))?;
        thread::sleep(Duration::from_millis(400));self.capture("files-image-preview.png")?;
        self.key(&["-k","Escape"])?;
        thread::sleep(Duration::from_millis(400));
        self.capture("desktop.png")?;
        for (method, title, filename) in [("calendar","Calendar","calendar-app.png"),("remoteDesktop","Remote Desktop","remote-app.png"),("textTools","Text Tools","text-tools-app.png")] {
            self.ipc(method, &[])?;
            thread::sleep(Duration::from_millis(600));
            if method=="calendar" {self.review_window(title)?;self.key(&["-M","ctrl","-k","3","-m","ctrl"])?;thread::sleep(Duration::from_millis(350));}
            self.window_capture(title, filename)?;
            if method=="textTools" {
                for page in ["Dictionary","Speech","Replacements","Shortcuts","Spelling","Dictation","Services","Fonts","Colours"] {
                    self.key(&["-M","ctrl","-k","Tab","-m","ctrl"])?;
                    self.wait_for("Review utility page",||Ok(self.ipc_json("textToolsStatus",&[])?["page"]==page))?;
                    thread::sleep(Duration::from_millis(200));self.window_capture("Text Tools",&format!("text-tools-{}.png",page.to_lowercase()))?;
                }
            }
            if method=="calendar" {
                self.key(&["-M","ctrl","-k","1","-m","ctrl"])?;
                thread::sleep(Duration::from_millis(350));self.window_capture("Calendar","calendar-day.png")?;
                self.key(&["-M","ctrl","-k","3","-m","ctrl"])?;
            }
        }
        for (page, filename) in [("Appearance","appearance.png"),("Desktop & Dock","desktop-settings.png"),("Displays","displays.png"),("Sound","sound.png"),("Privacy & Security","privacy.png"),("Accessibility","accessibility.png"),("Wallpaper","wallpaper.png"),("Internet Accounts","accounts.png")] {
            self.ipc("settings", &[page])?;
            self.wait_for("release settings page", || Ok(self.ipc_json("settingsStatus", &[])?["page"] == page))?;
            thread::sleep(Duration::from_millis(400));
            self.window_capture("System Settings", filename)?;
        }
        if env::var_os("LUMEN_RELEASE_SCALE").is_some(){self.review_window("Calendar")?;}
        self.ipc("menu", &["Controls"])?;
        thread::sleep(Duration::from_millis(500));
        self.capture("controls.png")?;
        self.ipc("menu", &["Launcher"])?;
        thread::sleep(Duration::from_millis(500));
        self.capture("search.png")?;
        self.key(&["Calendar"])?;
        self.wait_for("Typed launcher search", || Ok(self.ipc_json("searchStatus",&[])?["views"].as_array().is_some_and(|rows|rows.iter().any(|r| r.to_string().contains("Calendar")))))?;
        thread::sleep(Duration::from_millis(500));self.capture("search-calendar.png")?;
        for (query,filename,has_results) in [("Release","search-files.png",true),("zzzz-no-such-review-item","search-empty.png",false),("Open Control Centre","search-actions.png",true)] {
            self.key(&["-M","ctrl","-k","a","-m","ctrl"])?;self.key(&[query])?;
            self.wait_for("Typed search result state",||{let state=self.ipc_json("searchStatus",&[])?;Ok(state["available"]==true && state["searching"]==false && state["views"].as_array().is_some_and(|rows|rows.iter().any(|r|r["query"]==query && (r["count"].as_u64().unwrap_or(0)>0)==has_results)))})?;
            thread::sleep(Duration::from_millis(250));self.capture(filename)?;
        }
        self.key(&["-k","Return"])?;
        self.wait_for("Search action activation",||Ok(self.ipc_json("layout",&[])?[0]["menu"]=="Controls"))?;
        self.capture("search-action-open.png")?;
        self.ipc("menu", &[""])?;
        self.ipc("notificationCentre", &["open"])?;
        thread::sleep(Duration::from_millis(500));
        self.capture("calendar-panel.png")?;
        self.wait_for("Notification transport", || Ok(self.ipc_json("notificationStatus",&[])?["available"]==true))?;
        self.review_notify("Calendar","Design review in 10 minutes","Studio - bring the latest layout sketches.")?;
        self.wait_for("One notification", || Ok(self.ipc_json("notificationStatus",&[])?["count"]==1))?;
        thread::sleep(Duration::from_millis(300));self.capture("notifications-single.png")?;
        if env::var_os("LUMEN_REVIEW_CORNER_DIAGNOSTICS").is_some(){
            self.run_command("swaymsg",&["layer_effects \"lumen-menu-popup\" \"blur disable\""])?;
            thread::sleep(Duration::from_millis(250));self.capture("notification-no-blur.png")?;
            self.run_command("swaymsg",&["layer_effects \"lumen-menu-popup\" \"blur enable; blur_ignore_transparent enable\""])?;
            thread::sleep(Duration::from_millis(250));
        }
        self.review_notify("Calendar","Lunch with Sam","Today at 12:30.")?;
        self.review_notify("Calendar","Release planning","Today at 14:00.")?;
        self.wait_for("Calendar stack", || Ok(self.ipc_json("notificationStatus",&[])?["count"]==3))?;
        thread::sleep(Duration::from_millis(300));self.capture("notifications-stack.png")?;
        if let Some(pointer)=env::var_os("LUMEN_REVIEW_POINTER_BIN") {
            let pointer=PathBuf::from(pointer);require(pointer.starts_with("/nix/store") && pointer.is_file(),"Immutable pointer fixture required")?;
            let stack=self.ipc_json("layout",&[])?[0]["calendar"]["notificationGroups"][0].clone();
            self.run_command("swaymsg",&[&format!("seat seat0 cursor set {} {}",number(&stack,"x")?+number(&stack,"width")?/2.,number(&stack,"y")?+number(&stack,"height")?/2.)])?;
            self.run_command(pointer.to_str().ok_or("Pointer path")?,&["pointer","click","left"])?;
            self.wait_for("Notification stack expanded",||Ok(self.ipc_json("layout",&[])?[0]["calendar"]["notificationGroups"][0]["expanded"]==true))?;
            thread::sleep(Duration::from_millis(250));self.capture("notifications-expanded.png")?;
            let stack=self.ipc_json("layout",&[])?[0]["calendar"]["notificationGroups"][0].clone();
            self.run_command("swaymsg",&[&format!("seat seat0 cursor set {} {}",number(&stack,"x")?+50.,number(&stack,"y")?+13.)])?;
            self.run_command(pointer.to_str().ok_or("Pointer path")?,&["pointer","click","left"])?;
            self.wait_for("Notification stack collapsed",||Ok(self.ipc_json("layout",&[])?[0]["calendar"]["notificationGroups"][0]["expanded"]==false))?;
        }
        self.review_notify("Files","Copy complete","Project assets are ready in Studio.")?;
        self.review_notify("Files","Export complete","The preview has been saved.")?;
        self.wait_for("Multiple notification groups", || Ok(self.ipc_json("notificationStatus",&[])?["count"]==5))?;
        thread::sleep(Duration::from_millis(300));self.capture("notifications-groups.png")?;
        self.ipc("notificationCentre", &["close"])?;
        self.ipc("settings", &["Accessibility"])?;
        let mut preference_checks=Vec::new();
        for (key,label,control_id) in [("reduceTransparency","Reduce transparency","reduce-transparency"),("reduceMotion","Reduce motion","reduce-motion"),("increaseContrast","Increase contrast","increase-contrast")] {
            self.ipc("settings", &["Accessibility"])?;
            self.review_window("System Settings")?;
            self.key(&["-M","ctrl","-k","f","-m","ctrl"])?;
            self.key(&["-M","ctrl","-k","a","-m","ctrl"])?;
            self.key(&[label])?;self.key(&["-k","Return"])?;
            self.wait_for("Accessible setting receives keyboard focus",||{let state=self.ipc_json("settingsStatus",&[])?;Ok(state["controlId"]==control_id && state["controlFocused"]==true && state["busy"]==false)})?;
            for enabled in [true,false] {
                self.key(&["-k","space"])?;
                self.wait_for("Saved accessibility preference",||Ok(read_json(&self.run.join("review-settings.json"))?["appearance"][key]==enabled && read_json(Path::new(self.vars.get("LUMEN_APPEARANCE_STATE").ok_or("Appearance path")?))?["preferences"][key]==enabled && self.ipc_json("settingsStatus",&[])?["busy"]==false))?;
                preference_checks.push(json!({"key":key,"enabled":enabled,"persisted":true,"appearancePublished":true}));
                if enabled {thread::sleep(Duration::from_millis(250));self.capture(&format!("accessibility-{key}.png"))?;}
            }
        }
        save_json(&self.run.join("accessibility-preference-checks.json"),&json!({"scope":"Physical keyboard through Settings search, actual switch activation, private persistence and published appearance state", "checks":preference_checks}))?;
        self.ipc("settings", &["Displays"])?;
        for scale in if env::var_os("LUMEN_RELEASE_SCALE").is_some(){vec![]}else{vec!["1.25","1.5"]} {
            self.run_command("swaymsg",&[&format!("output HEADLESS-1 scale {scale}")])?;
            let expected=(1440.0/scale.parse::<f64>()?).round() as i64;
            self.wait_for("Output scale observed by the shell",||Ok(self.ipc_json("status",&[])?["screens"].as_array().is_some_and(|rows|rows.iter().any(|row|row["name"]=="HEADLESS-1" && row["width"].as_i64()==Some(expected)))))?;
            thread::sleep(Duration::from_millis(600));self.capture(&format!("displays-scale-{scale}.png"))?;
        }
        if env::var_os("LUMEN_RELEASE_SCALE").is_none(){self.run_command("swaymsg",&["output HEADLESS-1 scale 1"])?;}
        self.review_application_capture("codex","LUMEN_REVIEW_CODEX_BIN")?;
        self.review_application_capture("chatgpt","LUMEN_REVIEW_BROWSER_BIN")?;
        self.review_application_capture("emoji","LUMEN_REVIEW_EMOJI_BIN")?;
        save_json(&self.run.join("release-capture.json"), &json!({"purpose":"Release presentation screenshots of prepared desktop in an isolated session", "priorWorkflowChecks":checks,"accountData":"Synthetic; no personal accounts or devices available"}))?;
        Ok(())
    }
    fn finish(&self, mut checks: Vec<String>) -> Result<()> {
        self.key(&["-k", "Escape"])?;
        self.ipc("menu", &["Controls"])?;
        self.wait_for("Controls menu", || {
            Ok(self.ipc_json("layout", &[])?[0]["menu"] == "Controls")
        })?;
        require(
            self.ipc_json("layout", &[])?[0]["scrollY"] == 0,
            "Control Centre retained panel scroll",
        )?;
        thread::sleep(Duration::from_millis(500));
        self.capture("controls.png")?;
        self.key(&["-k", "Escape"])?;
        self.ipc("menu", &["Launcher"])?;
        thread::sleep(Duration::from_millis(500));
        self.capture("search.png")?;
        self.key(&["-k", "Escape"])?;
        self.ipc("settings", &["Internet Accounts"])?;
        self.wait_for("Calendar Accounts", || {
            Ok(self.ipc_json("settingsStatus", &[])?["page"] == "Internet Accounts")
        })?;
        self.capture("calendar-accounts.png")?;
        self.key(&["-k", "Escape"])?;
        self.ipc("notificationCentre", &["open"])?;
        thread::sleep(Duration::from_millis(500));
        self.capture("calendar-wallpaper.png")?;
        self.ipc("notificationCentre", &["close"])?;
        let mut ppm = b"P6\n64 64\n255\n".to_vec();
        for y in 0..64 {
            for x in 0..64 {
                ppm.extend_from_slice(if (x / 4 + y / 4) % 2 == 1 {
                    &[40, 90, 130]
                } else {
                    &[220, 180, 140]
                });
            }
        }
        let pattern = self.run.join("checker.ppm");
        write(&pattern, ppm)?;
        self.run_command(
            "swaymsg",
            &[&format!("output HEADLESS-1 bg {} tile", pattern.display())],
        )?;
        thread::sleep(Duration::from_millis(300));
        self.capture("calendar-before.png")?;
        self.ipc("notificationCentre", &["open"])?;
        self.ipc("notificationCentre", &["open"])?;
        thread::sleep(Duration::from_millis(500));
        require(
            self.ipc_json("layout", &[])?[0]["menu"] == "Date",
            "Double notification open lost Date",
        )?;
        self.ipc("notificationCentre", &["close"])?;
        self.wait_for("Notification Centre close", || {
            Ok(self.ipc_json("layout", &[])?[0]["menu"] == "")
        })?;
        self.ipc("notificationCentre", &["open"])?;
        thread::sleep(Duration::from_millis(300));
        for scale in ["1", "2"] {
            self.run_command("swaymsg", &[&format!("output HEADLESS-1 scale {scale}")])?;
            thread::sleep(Duration::from_millis(300));
            let layout = self.ipc_json("layout", &[])?[0].clone();
            require(
                number(&layout, "scale")? == 1.,
                format!("Popover scale changed: {layout}"),
            )?;
            require(
                number(&layout, "panelHeight")? <= number(&layout, "height")?
                    && number(&layout, "panelWidth")? <= number(&layout, "width")?,
                format!("Popover outside output: {layout}"),
            )?;
            let capture = format!("calendar-popover-scale-{scale}.png");
            self.capture(&capture)?;
            if self.opt.effects && scale == "1" {
                let before = RgbImage::open(&self.run.join("calendar-before.png"))?;
                let after = RgbImage::open(&self.run.join(&capture))?;
                require(
                    before.equal_crop(&after, [16, 800, 96, 880])?,
                    "Popup blur changed desktop pixels outside content",
                )?;
            }
        }
        self.key(&["-k", "Escape"])?;
        for scale in ["1", "1.25", "1.5", "2"] {
            self.run_command("swaymsg", &[&format!("output HEADLESS-1 scale {scale}")])?;
            thread::sleep(Duration::from_millis(500));
            require(
                self.state()?["visible"] == true,
                "Files vanished after output scale",
            )?;
            self.capture(&format!("scale-{scale}.png"))?;
        }
        if self.opt.effects {
            checks.push("Transparent popup pixels do not blur the desktop outside content".into());
        }
        if let Some(tool) = &self.opt.drag_pointer {
            self.drag_checks(tool, &mut checks)?;
        }
        if self.opt.desktop_widgets {
            self.run_command("swaymsg", &["[app_id=\"org.quickshell\"] move scratchpad"])?;
            thread::sleep(Duration::from_millis(500));
            self.capture("desktop-widgets-painted.png")?;
        }
        self.check_shell_log(&[
            "Failed to load configuration",
            "ReferenceError:",
            "TypeError:",
        ])?;
        let sway_log = fs::read_to_string(self.run.join("sway.log"))?;
        require(
            !sway_log.contains("Unknown/invalid command"),
            "Sway rejected a fixture command",
        )?;
        if self.opt.effects {
            for proof in [
                "Using software rendering",
                "Creating scenefx FX renderer",
                "FX RENDERER: Shaders Initialized Successfully",
                "Created GL FBO for buffer",
            ] {
                require(
                    sway_log.contains(proof),
                    format!("Effects renderer proof missing: {proof}"),
                )?;
            }
            checks.push(
                "Explicit software Scenefx renderer initialized FX shaders and effect framebuffers"
                    .into(),
            );
            for (image, area) in [
                ("files.png", [470, 260, 1000, 700]),
                ("controls.png", [1130, 100, 1420, 450]),
            ] {
                require(
                    self.screenshot_mean(image, area)? > if self.opt.theme == "dark" { 25. } else { 60. },
                    format!("Incomplete effects interior: {image}"),
                )?;
            }
            checks.push("Effects captures contain rendered window and popup interiors".into());
        }
        save_json(
            &self.run.join("result.json"),
            &json!({"passed":true,"assets":self.assets.display().to_string(),"checks":checks}),
        )?;
        println!("{}", self.run.join("result.json").display());
        Ok(())
    }
    fn execute(mut self) -> Result<()> {
        if env::var_os("LUMEN_RELEASE_CAPTURE").is_some() {self.review_providers()?;}
        let capabilities = self.compositor()?;
        if env::var_os("LUMEN_RELEASE_CAPTURE").is_some() {
            let notification=PathBuf::from(self.vars.get("LUMEN_NOTIFICATION_HELPER").ok_or("Pinned notification service missing")?);
            self.launch(&notification,&["serve"],"review-notifications")?;
        }
        if self.opt.menu_only {
            return self.menu_only();
        }
        if self.opt.desktop_widgets {
            self.widgets()?;
        }
        let checks = self.basic_checks()?;
        if capabilities["native_window_actions"] == true {
            if let Some(tool) = &self.opt.click_pointer {
                self.check_header_buttons(tool)?;
            }
        }
        let checks = checks
            .as_array()
            .ok_or("Basic checks are not array")?
            .iter()
            .map(|value| value.as_str().unwrap_or("").into())
            .collect();
        if env::var_os("LUMEN_RELEASE_CAPTURE").is_some() { self.release_capture(checks) } else { self.finish(checks) }
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("installed-shell smoke: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let opt = options()?;
    if !opt.inside {
        return private_session(&opt);
    }
    let driver = setup(opt)?;
    driver.execute()
}
