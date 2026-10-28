//! Private greeter IPC, session-lock authentication, and shared visual QA.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use walkdir::WalkDir;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
fn fail(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    message.into().into()
}
fn require(value: bool, message: impl Into<String>) -> Result<()> {
    if value { Ok(()) } else { Err(fail(message)) }
}
fn path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
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
            return Err(fail(format!("Timed out waiting for {label}: {last}")));
        }
        thread::sleep(Duration::from_millis(150));
    }
}
#[derive(Default)]
struct Options {
    source: Option<PathBuf>,
    assets: Option<PathBuf>,
    launcher: Option<PathBuf>,
    sway: Option<PathBuf>,
    font_config: Option<PathBuf>,
    lock_wrapper: Option<PathBuf>,
    output: Option<PathBuf>,
    inside: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut options = Self::default();
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--source") => {
                    options.source = Some(PathBuf::from(args.next().ok_or("Missing --source")?))
                }
                Some("--assets") => {
                    options.assets = Some(PathBuf::from(args.next().ok_or("Missing --assets")?))
                }
                Some("--launcher") => {
                    options.launcher = Some(PathBuf::from(args.next().ok_or("Missing --launcher")?))
                }
                Some("--sway") => {
                    options.sway = Some(PathBuf::from(args.next().ok_or("Missing --sway")?))
                }
                Some("--font-config") => {
                    options.font_config =
                        Some(PathBuf::from(args.next().ok_or("Missing --font-config")?))
                }
                Some("--lock-wrapper") => {
                    options.lock_wrapper =
                        Some(PathBuf::from(args.next().ok_or("Missing --lock-wrapper")?))
                }
                Some("--output") => {
                    options.output = Some(PathBuf::from(args.next().ok_or("Missing --output")?))
                }
                Some("--inside") => options.inside = true,
                _ => return Err(fail(format!("Unknown option: {}", arg.to_string_lossy()))),
            }
        }
        require(
            options.source.is_some()
                && options.output.is_some()
                && options.launcher.is_some()
                && options.sway.is_some(),
            "--source, --output, --launcher and --sway required",
        )?;
        Ok(options)
    }
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in WalkDir::new(source).min_depth(1) {
        let entry = entry?;
        let destination = target.join(entry.path().strip_prefix(source)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), &destination)?;
        }
    }
    Ok(())
}
fn parse_env0(bytes: &[u8]) -> BTreeMap<String, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|chunk| {
            let index = chunk.iter().position(|byte| *byte == b'=')?;
            Some((
                String::from_utf8_lossy(&chunk[..index]).into_owned(),
                String::from_utf8_lossy(&chunk[index + 1..]).into_owned(),
            ))
        })
        .collect()
}
struct Fixture {
    out: PathBuf,
    runtime: PathBuf,
    env: BTreeMap<String, String>,
    children: Vec<Child>,
}
impl Fixture {
    fn launch(&mut self, args: &[&str], name: &str) -> Result<usize> {
        let log = File::create(self.out.join(format!("{name}.log")))?;
        let mut command = Command::new(args[0]);
        command
            .args(&args[1..])
            .env_clear()
            .envs(&self.env)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let child = command.spawn()?;
        self.children.push(child);
        Ok(self.children.len() - 1)
    }
    fn call(&self, args: &[&str]) -> Result<String> {
        let result = Command::new("timeout")
            .arg("12s")
            .args(args)
            .env_clear()
            .envs(&self.env)
            .output()?;
        require(
            result.status.success(),
            format!(
                "Command {args:?} failed: {}",
                String::from_utf8_lossy(&result.stderr)
            ),
        )?;
        Ok(String::from_utf8(result.stdout)?.trim().to_owned())
    }
    fn screenshot(&self, name: &str) -> Result<()> {
        self.call(&["grim", &path(&self.out.join(name))])?;
        Ok(())
    }
    fn socket(&self, prefix: &str) -> Result<PathBuf> {
        Ok(fs::read_dir(&self.runtime)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|entry| {
                entry
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
            })
            .ok_or_else(|| fail(format!("Missing {prefix} socket")))?)
    }
    fn ipc(&self, qml: &Path, target: &str, method: &str) -> Result<String> {
        self.call(&[
            "quickshell",
            "-p",
            &path(qml),
            "ipc",
            "call",
            target,
            method,
        ])
    }
    fn stop(&mut self, index: usize) -> Result<()> {
        let child = &mut self.children[index];
        if child.try_wait()?.is_none() {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGTERM);
            }
            for _ in 0..30 {
                if child.try_wait()?.is_some() {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(100));
            }
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            child.wait()?;
        }
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for child in self.children.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGTERM);
                }
                if child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill();
                }
            }
            let _ = child.wait();
        }
    }
}
fn serve_greetd(listener: UnixListener, requests: Arc<Mutex<Vec<Value>>>) -> Result<()> {
    let (mut stream, _) = listener.accept()?;
    let mut count = 0;
    loop {
        let mut length = [0u8; 4];
        if let Err(error) = stream.read_exact(&mut length) {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                break;
            }
            return Err(error.into());
        }
        let size = u32::from_le_bytes(length) as usize;
        require(size < 1024 * 1024, "Greetd frame too large")?;
        let mut data = vec![0u8; size];
        stream.read_exact(&mut data)?;
        let request: Value = serde_json::from_slice(&data)?;
        let kind = request["type"]
            .as_str()
            .ok_or("Greetd request has no type")?;
        requests
            .lock()
            .unwrap()
            .push(json!({"type":kind,"cmd":request.get("cmd")}));
        let reply = match kind {
            "create_session" => {
                count += 1;
                json!({"type":"auth_message","auth_message_type":if count==1 {"visible"} else {"secret"},"auth_message":"Fixture challenge:"})
            }
            "post_auth_message_response" if request["response"] == "fixture-password" => {
                json!({"type":"success"})
            }
            "post_auth_message_response" => {
                json!({"type":"error","error_type":"auth_error","description":"Incorrect password"})
            }
            _ => json!({"type":"success"}),
        };
        let bytes = serde_json::to_vec(&reply)?;
        stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
        stream.write_all(&bytes)?;
    }
    Ok(())
}
fn rgb(path: &Path) -> Result<(usize, usize, Vec<[u8; 3]>)> {
    let mut decoder = png::Decoder::new(File::open(path)?);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes)?;
    let pixels = bytes[..info.buffer_size()]
        .chunks(match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            _ => return Err(fail("Unexpected PNG color type")),
        })
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    Ok((info.width as usize, info.height as usize, pixels))
}
fn same_layout(first: &Path, second: &Path) -> Result<usize> {
    let (width, height, a) = rgb(first)?;
    let (other_width, other_height, b) = rgb(second)?;
    require(
        width == other_width && height == other_height,
        "Login and lock preview sizes differ",
    )?;
    Ok(a.iter()
        .zip(b)
        .filter(|(a, b)| (0..3).any(|index| a[index].abs_diff(b[index]) > 8))
        .count())
}
fn outer(options: &Options) -> Result<()> {
    let out = fs::canonicalize(
        options
            .output
            .as_ref()
            .unwrap()
            .parent()
            .unwrap_or(Path::new(".")),
    )?
    .join(
        options
            .output
            .as_ref()
            .unwrap()
            .file_name()
            .ok_or("Output needs a name")?,
    );
    require(!out.exists(), "Fresh output directory required")?;
    fs::create_dir_all(&out)?;
    let runtime = TempDir::with_prefix("auth-qa-")?;
    fs::set_permissions(runtime.path(), fs::Permissions::from_mode(0o700))?;
    let bus_config = runtime.path().join("bus.conf");
    fs::write(
        &bus_config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            runtime.path().display()
        ),
    )?;
    let mut command = Command::new("dbus-run-session");
    command
        .arg(format!("--config-file={}", bus_config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--source")
        .arg(fs::canonicalize(options.source.as_ref().unwrap())?)
        .arg("--launcher")
        .arg(fs::canonicalize(options.launcher.as_ref().unwrap())?)
        .arg("--sway")
        .arg(fs::canonicalize(options.sway.as_ref().unwrap())?)
        .arg("--output")
        .arg(&out)
        .env("HOME", runtime.path())
        .env("XDG_RUNTIME_DIR", runtime.path());
    for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
        command.env(format!("XDG_{key}_HOME"), out.join(key.to_lowercase()));
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        command.env_remove(key);
    }
    command.env(
        "DBUS_SYSTEM_BUS_ADDRESS",
        format!("unix:path={}/no-system-bus", runtime.path().display()),
    );
    command
        .env("QT_ACCESSIBILITY", "0")
        .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_A11Y", "none");
    if let Some(value) = &options.assets {
        command.arg("--assets").arg(fs::canonicalize(value)?);
    }
    if let Some(value) = &options.font_config {
        command.arg("--font-config").arg(fs::canonicalize(value)?);
    }
    if let Some(value) = &options.lock_wrapper {
        command.arg("--lock-wrapper").arg(fs::canonicalize(value)?);
    }
    let status = command.status()?;
    require(
        status.success(),
        format!("Private auth test exited {status}"),
    )
}
fn pam_lib() -> Result<PathBuf> {
    for entry in fs::read_dir("/nix/store")? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().contains("-linux-pam-") {
            let path = entry.path().join("lib/security/pam_deny.so");
            if path.is_file() {
                return Ok(path.parent().unwrap().to_owned());
            }
        }
    }
    Err(fail("Pinned Linux PAM library unavailable"))
}
fn unique_pixels(path: &Path) -> Result<usize> {
    let (_, _, pixels) = rgb(path)?;
    Ok(pixels
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .len())
}
fn inner(options: &Options) -> Result<()> {
    let out = options.output.as_ref().unwrap();
    let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
    let assets = out.join("assets");
    copy_tree(
        options.assets.as_ref().map(|v| v.as_path()).unwrap_or(
            &options
                .source
                .as_ref()
                .unwrap()
                .join("res/lumen-desktop/auth"),
        ),
        &assets.join("auth"),
    )?;
    for entry in WalkDir::new(assets.join("auth")) {
        let entry = entry?;
        fs::set_permissions(
            entry.path(),
            fs::Permissions::from_mode(if entry.file_type().is_dir() {
                0o755
            } else {
                0o644
            }),
        )?;
    }
    fs::create_dir(assets.join("shell"))?;
    let wallpaper = options
        .source
        .as_ref()
        .unwrap()
        .join("res/lumen-desktop/shell/wallpaper.png");
    fs::copy(&wallpaper, assets.join("shell/wallpaper.png"))?;
    fs::copy(&wallpaper, assets.join("auth/wallpaper.png"))?;
    let mut base = env::vars().collect::<BTreeMap<_, _>>();
    let printed = Command::new(options.launcher.as_ref().unwrap())
        .arg("--print-env0")
        .output()?;
    require(printed.status.success(), "Launcher environment unavailable")?;
    base.extend(parse_env0(&printed.stdout));
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", "pixman"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("GDK_BACKEND", "wayland"),
        ("LUMEN_AUTH_SESSION", "/bin/true"),
        ("LUMEN_AUTH_USER", "test-user"),
        ("LUMEN_AUTH_NAME", "Test User"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        base.insert(key.into(), value.into());
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        base.remove(key);
    }
    for key in [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_DATA_HOME",
        "DBUS_SESSION_BUS_ADDRESS",
        "DBUS_SYSTEM_BUS_ADDRESS",
    ] {
        if let Ok(value) = env::var(key) {
            base.insert(key.into(), value);
        }
    }
    if let Some(value) = &options.font_config {
        base.insert("FONTCONFIG_FILE".into(), path(value));
    }
    let mut f = Fixture {
        out: out.to_owned(),
        runtime,
        env: base,
        children: Vec::new(),
    };
    let conf = out.join("sway.conf");
    fs::write(
        &conf,
        "output HEADLESS-1 resolution 1920x1080\nxwayland disable\ninput * xkb_layout gb\n",
    )?;
    f.launch(
        &[
            &path(options.sway.as_ref().unwrap()),
            "--unsupported-gpu",
            "-c",
            &path(&conf),
        ],
        "sway",
    )?;
    let sway_socket = wait("private compositor", || f.socket("sway-ipc").map(Some))?;
    let display = wait("private display", || {
        Ok(fs::read_dir(&f.runtime)?
            .filter_map(|v| v.ok())
            .map(|v| v.file_name().to_string_lossy().into_owned())
            .find(|v| v.starts_with("wayland-") && !v.ends_with(".lock")))
    })?;
    f.env.insert("SWAYSOCK".into(), path(&sway_socket));
    f.env.insert("WAYLAND_DISPLAY".into(), display);
    let listener = UnixListener::bind(f.runtime.join("greetd.sock"))?;
    let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let server_requests = Arc::clone(&requests);
    let server = thread::spawn(move || serve_greetd(listener, server_requests));
    f.env
        .insert("GREETD_SOCK".into(), path(&f.runtime.join("greetd.sock")));
    let greeterfile = assets.join("auth/greeter.qml");
    let original = fs::read_to_string(&greeterfile)?;
    let end = original.rfind('}').ok_or("Greeter root not found")?;
    fs::write(
        &greeterfile,
        format!(
            "{}IpcHandler {{target:\"fixture\";function passwordMasked():bool{{return !root.echoResponse && !root.responseReady;}}}}{}",
            &original[..end],
            &original[end..]
        ),
    )?;
    let greeter = f.launch(&["quickshell", "-p", &path(&greeterfile)], "greeter")?;
    thread::sleep(Duration::from_secs(2));
    require(
        f.children[greeter].try_wait()?.is_none(),
        fs::read_to_string(out.join("greeter.log"))?,
    )?;
    f.screenshot("login.png")?;
    f.call(&[
        "wtype",
        "-s",
        "300",
        "wrong-password",
        "-k",
        "Return",
        "-s",
        "100",
    ])?;
    wait("rejected greetd request", || {
        Ok((requests.lock().unwrap().len() >= 2).then_some(()))
    })?;
    thread::sleep(Duration::from_millis(500));
    require(
        !requests
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["type"] == "start_session"),
        "Wrong password started a session",
    )?;
    require(
        f.ipc(&greeterfile, "fixture", "passwordMasked")? == "true",
        "Password masking not reset",
    )?;
    let mut checks = vec![
        "visible challenge failure resets password masking",
        "incorrect greetd response rejected",
    ];
    f.screenshot("login-error.png")?;
    f.call(&[
        "wtype",
        "-s",
        "300",
        "fixture-password",
        "-k",
        "Return",
        "-s",
        "100",
    ])?;
    wait("session launch", || {
        Ok(requests
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["type"] == "start_session")
            .then_some(()))
    })?;
    wait("greeter exit", || {
        Ok(f.children[greeter].try_wait()?.map(|_| ()))
    })?;
    let start_commands: Vec<Value> = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|v| v["type"] == "start_session")
        .map(|v| v["cmd"].clone())
        .collect();
    require(
        start_commands == [json!(["/bin/true"])],
        format!("Unexpected session command: {start_commands:?}"),
    )?;
    checks.push("correct greetd response launches configured session");
    f.env.remove("GREETD_SOCK");
    let preview = assets.join("auth/preview.qml");
    fs::write(&preview, include_str!("../preview.qml"))?;
    let preview_child = f.launch(&["quickshell", "-p", &path(&preview)], "preview")?;
    thread::sleep(Duration::from_secs(1));
    require(
        f.children[preview_child].try_wait()?.is_none(),
        "Lock preview exited",
    )?;
    f.screenshot("lock-preview.png")?;
    let changed = same_layout(&out.join("login.png"), &out.join("lock-preview.png"))?;
    require(
        changed < 1500,
        format!("Login/lock layout changed at {changed} pixels"),
    )?;
    checks.push("shared login/lock layout pixel comparison");
    for (method, label) in [
        (
            "secretRetention",
            "authentication view clears responses across busy masking and visibility transitions",
        ),
        (
            "passwordReveal",
            "show-password control reveals and conceals input and resets after submission",
        ),
    ] {
        require(
            f.ipc(&preview, "preview", method)? == "true",
            format!("{method} failed"),
        )?;
        checks.push(label);
    }
    f.ipc(&preview, "preview", "power")?;
    thread::sleep(Duration::from_millis(300));
    f.screenshot("power-menu.png")?;
    f.call(&["wtype", "-s", "300", "-k", "Escape"])?;
    thread::sleep(Duration::from_millis(200));
    for (width, height, scale) in [
        (800, 600, 1),
        (1280, 720, 1),
        (2560, 1440, 2),
        (600, 400, 1),
    ] {
        f.call(&[
            "swaymsg",
            &format!("output HEADLESS-1 resolution {width}x{height} scale {scale}"),
        ])?;
        thread::sleep(Duration::from_millis(500));
        let bounds: Value = serde_json::from_str(&f.ipc(&preview, "preview", "bounds")?)?;
        let (x, y, w, h) = (
            bounds["x"].as_f64().ok_or("x")?,
            bounds["y"].as_f64().ok_or("y")?,
            bounds["width"].as_f64().ok_or("width")?,
            bounds["height"].as_f64().ok_or("height")?,
        );
        require(
            x >= 0.0
                && y >= 0.0
                && x + bounds["fieldWidth"].as_f64().ok_or("fieldWidth")? <= w
                && y + bounds["fieldHeight"].as_f64().ok_or("fieldHeight")? <= h,
            format!("Password outside display: {bounds}"),
        )?;
        f.screenshot(&format!("preview-{width}x{height}-{scale}.png"))?;
    }
    checks.push("password controls remain visible at four display sizes/scales");
    f.call(&["swaymsg", "output HEADLESS-1 resolution 1920x1080 scale 1"])?;
    f.stop(preview_child)?;
    f.call(&["swaymsg", "create_output"])?;
    checks.push("authentication lock tested with two outputs");
    let pamdir = out.join("pam");
    fs::create_dir(&pamdir)?;
    let pam = pam_lib()?;
    let lockfile = assets.join("auth/lock.qml");
    let lock_source = fs::read_to_string(&lockfile)?
        .replace(
            "config: \"swaylock\"",
            &format!(
                "config: \"fixture\"\n        configDirectory: \"{}\"",
                pamdir.display()
            ),
        )
        .replace(
            "onCompleted: result => {",
            "onCompleted: result => {\n            console.log(\"fixture-auth-result\", result);",
        );
    fs::write(&lockfile, lock_source)?;
    let secure = |fixture: &Fixture| {
        fixture
            .ipc(&lockfile, "auth", "secure")
            .map(|v| v == "true")
    };
    fs::write(
        pamdir.join("fixture"),
        format!(
            "auth required {}\naccount required {}\n",
            pam.join("pam_deny.so").display(),
            pam.join("pam_permit.so").display()
        ),
    )?;
    let lock = f.launch(&["quickshell", "-p", &path(&lockfile)], "lock")?;
    wait("secure lock", || Ok(secure(&f).ok().filter(|v| *v)))?;
    f.call(&[
        "wtype",
        "-s",
        "300",
        "fixture-password",
        "-k",
        "Return",
        "-s",
        "100",
    ])?;
    wait("failed PAM response", || {
        Ok(fs::read_to_string(out.join("lock.log"))?
            .contains("fixture-auth-result")
            .then_some(()))
    })?;
    require(secure(&f)?, "PAM denial released lock")?;
    checks.push("failed PAM authentication keeps compositor locked");
    fs::write(
        pamdir.join("fixture"),
        format!(
            "auth required {}\naccount required {}\n",
            pam.join("pam_permit.so").display(),
            pam.join("pam_permit.so").display()
        ),
    )?;
    f.call(&[
        "wtype",
        "-s",
        "300",
        "fixture-password",
        "-k",
        "Return",
        "-s",
        "100",
    ])?;
    wait("successful PAM release", || {
        Ok(f.children[lock].try_wait()?.map(|_| ()))
    })?;
    checks.push("successful PAM authentication releases lock");
    if let Some(wrapper) = &options.lock_wrapper {
        let source = fs::read_to_string(wrapper)?;
        let pattern = regex::Regex::new(r"/nix/store/[a-z0-9]+-lumen-auth-assets/lock\.qml")?;
        let edited = pattern.replace_all(&source, path(&lockfile)).into_owned();
        require(edited != source, "Packaged lock wrapper has no asset path")?;
        let target = out.join("lock-wrapper");
        fs::write(&target, edited)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700))?;
        f.call(&[&path(&target)])?;
        require(secure(&f)?, "Wrapper did not lock")?;
        let start = Instant::now();
        f.call(&[&path(&target)])?;
        require(
            secure(&f)? && start.elapsed() < Duration::from_secs(3),
            "Duplicate wrapper changed coverage",
        )?;
        checks.push("packaged wrapper waits for coverage and handles duplicate locking");
        f.call(&[
            "wtype",
            "-s",
            "300",
            "fixture-password",
            "-k",
            "Return",
            "-s",
            "100",
        ])?;
        wait("wrapper release", || {
            Ok((!secure(&f).unwrap_or(false)).then_some(()))
        })?;
    }
    let crash = f.launch(&["quickshell", "-p", &path(&lockfile)], "lock-crash")?;
    wait("crash lock", || Ok(secure(&f).ok().filter(|v| *v)))?;
    f.stop(crash)?;
    f.call(&[
        "grim",
        "-o",
        "HEADLESS-1",
        &path(&out.join("crash-closed.png")),
    ])?;
    f.call(&[
        "grim",
        "-o",
        "HEADLESS-2",
        &path(&out.join("crash-closed-secondary.png")),
    ])?;
    require(
        unique_pixels(&out.join("crash-closed.png"))? == 1
            && unique_pixels(&out.join("crash-closed-secondary.png"))? == 1,
        "Locker death exposed a desktop",
    )?;
    checks.push("locker process death leaves compositor covered");
    let logs = ["greeter", "preview", "lock", "lock-crash"]
        .iter()
        .map(|name| fs::read_to_string(out.join(format!("{name}.log"))))
        .collect::<std::io::Result<Vec<_>>>()?
        .join("\n");
    require(
        ![
            "Failed to load configuration",
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Cannot assign [undefined]",
        ]
        .iter()
        .any(|needle| logs.contains(needle)),
        "QML errors in auth log",
    )?;
    save(
        &out.join("result.json"),
        &json!({"passed":true,"checks":checks,"requests":*requests.lock().unwrap()}),
    )?;
    println!("{}", out.join("result.json").display());
    f.stop(greeter)?;
    drop(f);
    server
        .join()
        .map_err(|_| fail("Greetd server panicked"))??;
    Ok(())
}
fn main() -> Result<()> {
    let options = Options::parse()?;
    if options.inside {
        inner(&options)
    } else {
        outer(&options)
    }
}
