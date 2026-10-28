//! Private Calendar/Files screen-identity fit and state preservation.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, Write};
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

struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    output: PathBuf,
    native_frame: bool,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut output = None;
    let mut native_frame = None;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--native-frame") => {
                native_frame = Some(match args.next().ok_or("Missing frame mode")?.to_str() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err(fail("Invalid frame mode")),
                })
            }
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown consumer-fit fixture option")),
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
        output,
        native_frame: native_frame.ok_or("--native-frame required")?,
        effects,
        inside,
    })
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = env::temp_dir().join(format!("lfit-{}-{nonce}", std::process::id()));
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
    vars.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}", root.0.join("no-system-bus").display()),
    );
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
        .envs(vars)
        .status()?;
    require(
        status.success(),
        format!("Private consumer-fit fixture exited {status}"),
    )
}
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher).arg("--print-env0").output()?;
    require(result.status.success(), "Launcher environment unavailable")?;
    let mut vars = BTreeMap::new();
    for part in result.stdout.split(|byte| *byte == 0) {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    Ok(vars)
}
fn resolve(program: &str, vars: &BTreeMap<String, String>) -> Result<PathBuf> {
    let path = vars.get("PATH").ok_or("Packaged PATH missing")?;
    for dir in env::split_paths(&OsString::from(path)) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("Missing program {program}")))
}
fn command(program: &Path, args: &[&str], vars: &BTreeMap<String, String>) -> Result<String> {
    let result = Command::new(program)
        .args(args)
        .env_clear()
        .envs(vars)
        .output()?;
    require(
        result.status.success(),
        format!(
            "{} {:?} exited {}: {}",
            program.display(),
            args,
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
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
            let end = Instant::now() + Duration::from_secs(3);
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
        pause(60);
    }
}
fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|item| item.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && (suffix.is_empty() || !name.ends_with(suffix))
            })
        }))
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let kind = fs::symlink_metadata(&from)?.file_type();
        if kind.is_symlink() {
            symlink(fs::read_link(&from)?, &to)?;
        } else if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to)?;
            let mut p = fs::metadata(&to)?.permissions();
            p.set_mode(p.mode() | 0o200);
            fs::set_permissions(&to, p)?;
        } else {
            return Err(fail(format!("Unsupported asset {}", from.display())));
        }
    }
    Ok(())
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn calendar_helper() -> Result<()> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line)?;
    require(
        request["action"] == "list" || request["action"] == "load",
        "Calendar helper only accepts list/load",
    )?;
    println!(
        "{}",
        json!({"configured":false,"providers":[],"events":[],"days":[],"total":0})
    );
    std::io::stdout().flush()?;
    Ok(())
}
fn package_share() -> Result<PathBuf> {
    Ok(env::current_exe()?
        .parent()
        .ok_or("Fixture bin missing")?
        .parent()
        .ok_or("Fixture prefix missing")?
        .join("share/lumen-consumer-fit-fixture"))
}
fn sway_ipc(vars: &BTreeMap<String, String>, cmd: &str) -> Result<()> {
    let output = command(&resolve("swaymsg", vars)?, &[cmd], vars)?;
    let rows: Value = serde_json::from_str(&output)?;
    require(
        rows.as_array()
            .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
        format!("Sway rejected {cmd}: {rows}"),
    )
}
fn invoke(
    qml: &Path,
    vars: &BTreeMap<String, String>,
    method: &str,
    args: &[&str],
) -> Result<String> {
    let mut command_args = vec![
        "-p",
        qml.to_str().ok_or("QML path UTF-8")?,
        "ipc",
        "call",
        "consumerFit",
        method,
    ];
    command_args.extend_from_slice(args);
    command(&resolve("quickshell", vars)?, &command_args, vars)
}
fn state(qml: &Path, vars: &BTreeMap<String, String>, name: &str) -> Result<Value> {
    Ok(serde_json::from_str(&invoke(qml, vars, "state", &[name])?)?)
}
fn fit_complete(value: &Value, minimum: i64) -> bool {
    value["window"]["fit"]["completed"]
        .as_i64()
        .is_some_and(|count| count > minimum)
        && value["window"]["fit"]["busy"] == false
}
fn all_nodes<'a>(node: &'a Value, output: &mut Vec<&'a Value>) {
    output.push(node);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                all_nodes(child, output);
            }
        }
    }
}
fn window(vars: &BTreeMap<String, String>, title: &str) -> Result<Option<Value>> {
    let tree: Value = serde_json::from_str(&command(
        &resolve("swaymsg", vars)?,
        &["-r", "-t", "get_tree"],
        vars,
    )?)?;
    let mut nodes = Vec::new();
    all_nodes(&tree, &mut nodes);
    Ok(nodes
        .into_iter()
        .find(|node| node["app_id"] == "org.quickshell" && node["name"] == title)
        .cloned())
}
fn asset_hashes(root: &Path) -> Result<Value> {
    fn scan(base: &Path, dir: &Path, map: &mut serde_json::Map<String, Value>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = fs::symlink_metadata(&path)?.file_type();
            if kind.is_dir() {
                scan(base, &path, map)?;
            } else if path.is_file() {
                let relative = path.strip_prefix(base)?.to_string_lossy().to_string();
                map.insert(relative, json!(sha256(&fs::read(&path)?)));
            }
        }
        Ok(())
    }
    let mut map = serde_json::Map::new();
    scan(root, root, &mut map)?;
    Ok(Value::Object(map))
}
fn run(options: Options) -> Result<()> {
    require(!options.output.exists(), "Fresh output directory required")?;
    fs::create_dir_all(&options.output)?;
    let out = &options.output;
    let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
    let mut vars = launcher_environment(&options.launcher)?;
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
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "1"),
        (
            "LUMEN_UNIFIED_OWNED_HEADERS",
            if options.native_frame { "0" } else { "1" },
        ),
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
    let assets = out.join("assets");
    copy_tree(
        Path::new(
            vars.get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Packaged assets missing")?,
        ),
        &assets,
    )?;
    let palettes: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = out.join("appearance.json");
    save_json(
        &appearance,
        &json!({"font":palettes["font"],"palette":palettes["dark"],"preferences":{"reduceMotion":true,"reduceTransparency":true}}),
    )?;
    vars.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let folder = runtime.join("Synthetic");
    fs::create_dir(&folder)?;
    fs::write(
        folder.join("selected.txt"),
        "Synthetic preview preserved across display changes.\n",
    )?;
    let bin = out.join("bin");
    fs::create_dir(&bin)?;
    let provider = bin.join("lumen-calendar");
    symlink(env::current_exe()?, &provider)?;
    let path = vars.get("PATH").ok_or("Packaged PATH missing")?.clone();
    vars.insert("PATH".into(), format!("{}:{path}", bin.display()));
    let qml = out.join("shell.qml");
    let source = fs::read_to_string(package_share()?.join("fixture.qml"))?;
    fs::write(
        &qml,
        source.replace(
            "@FOLDER@",
            &serde_json::to_string(&folder.display().to_string())?,
        ),
    )?;
    let config = out.join("sway.conf");
    fs::write(
        &config,
        format!(
            "font sans 10\noutput HEADLESS-1 resolution 1920x1440 position 0 0 scale 1\noutput HEADLESS-2 resolution 1000x700 position 1920 0 scale 1.25\noutput * bg #29445b solid_color\nfor_window [all] floating enable\ndefault_floating_border {}\n",
            if options.native_frame {
                "normal 1"
            } else {
                "pixel 0"
            }
        ),
    )?;
    let _sway = Managed::launch(
        &options.sway,
        &[
            "-d",
            "--unsupported-gpu",
            "-c",
            &config.display().to_string(),
        ],
        &vars,
        &out.join("compositor.log"),
    )?;
    let socket = wait("private compositor", || {
        first_file(&runtime, "sway-ipc.", "")
    })?;
    vars.insert("SWAYSOCK".into(), socket.display().to_string());
    let display = wait("private display", || {
        first_file(&runtime, "wayland-", ".lock")
    })?;
    vars.insert(
        "WAYLAND_DISPLAY".into(),
        display
            .file_name()
            .ok_or("Display name")?
            .to_string_lossy()
            .into(),
    );
    sway_ipc(&vars, "lumen_window margins disable")?;
    sway_ipc(&vars, "focus output HEADLESS-1")?;
    let _shell = Managed::launch(
        &resolve("quickshell", &vars)?,
        &["-p", &qml.display().to_string()],
        &vars,
        &out.join("shell.log"),
    )?;
    wait("Files window", || {
        Ok(state(&qml, &vars, "Files")
            .ok()
            .and_then(|value| value["window"].is_object().then_some(())))
    })?;
    let mut records = Vec::new();
    for name in ["Files", "Calendar"] {
        invoke(&qml, &vars, "showConsumer", &[name])?;
        wait("consumer visible", || {
            Ok((state(&qml, &vars, name)?["window"]["visible"] == true).then_some(()))
        })?;
        let title = state(&qml, &vars, name)?["window"]["title"]
            .as_str()
            .ok_or("Window title missing")?
            .to_owned();
        let initial_node = wait("consumer tree", || window(&vars, &title))?;
        let identity = initial_node["id"]
            .as_i64()
            .ok_or("Container identity missing")?;
        wait("first screen fit", || {
            Ok(fit_complete(&state(&qml, &vars, name)?, 0).then_some(()))
        })?;
        if name == "Files" {
            let mut last_retry = Instant::now() - Duration::from_secs(3);
            wait("Files ready", || {
                let snapshot = state(&qml, &vars, name)?;
                save_json(&out.join("files-wait.json"), &snapshot)?;
                if snapshot["ready"] != true
                    && snapshot["error"]
                        .as_str()
                        .is_some_and(|error| !error.is_empty())
                    && last_retry.elapsed() > Duration::from_secs(2)
                {
                    invoke(&qml, &vars, "showConsumer", &[name])?;
                    last_retry = Instant::now();
                }
                Ok((snapshot["ready"] == true).then_some(()))
            })?;
            invoke(&qml, &vars, "select", &[])?;
            let selected = folder.join("selected.txt").display().to_string();
            wait("Files preview", || {
                let value = state(&qml, &vars, name)?;
                Ok((value["selection"] == json!([selected])
                    && value["previewBusy"] == false
                    && value["preview"].as_object().is_some_and(|v| !v.is_empty()))
                .then_some(()))
            })?;
        }
        let before = state(&qml, &vars, name)?;
        let completed = before["window"]["fit"]["completed"]
            .as_i64()
            .ok_or("First fit count")?;
        sway_ipc(
            &vars,
            &format!("[con_id={identity}] move container to output HEADLESS-2"),
        )?;
        wait("fit on smaller output", || {
            let value = state(&qml, &vars, name)?;
            Ok(
                (value["window"]["screen"] == "HEADLESS-2" && fit_complete(&value, completed))
                    .then_some(()),
            )
        })?;
        let small = state(&qml, &vars, name)?;
        let outcome = &small["window"]["fit"]["result"];
        require(
            small["window"]["fit"]["error"] == "",
            "Small output fit error",
        )?;
        wait("client size acknowledgement", || {
            let value = state(&qml, &vars, name)?;
            Ok((value["window"]["width"] == outcome["clientWidth"]
                && value["window"]["height"] == outcome["clientHeight"])
                .then_some(()))
        })?;
        let node = window(&vars, &title)?.ok_or("Consumer vanished")?;
        require(node["id"] == identity, "Container identity changed")?;
        require(
            outcome["outerWidth"].as_f64().is_some_and(|x| x <= 768.0)
                && outcome["outerHeight"]
                    .as_f64()
                    .is_some_and(|x| x <= if name == "Calendar" { 460.0 } else { 410.0 }),
            "Small output outer fit exceeded bounds",
        )?;
        let small = state(&qml, &vars, name)?;
        let rect = node["rect"].clone();
        let completed = small["window"]["fit"]["completed"]
            .as_i64()
            .ok_or("Small fit count")?;
        invoke(&qml, &vars, "repeat", &[name])?;
        wait("coalesced repeated fit", || {
            Ok(fit_complete(&state(&qml, &vars, name)?, completed).then_some(()))
        })?;
        let repeated = state(&qml, &vars, name)?;
        require(
            repeated["window"]["fit"]["completed"] == completed + 1
                && repeated["window"]["fit"]["result"]["changed"] == false
                && window(&vars, &title)?.ok_or("Consumer vanished after repeat")?["rect"] == rect,
            "Repeated fit changed stable window",
        )?;
        command(
            &resolve("grim", &vars)?,
            &[
                "-o",
                "HEADLESS-2",
                "-s",
                "1.25",
                &out.join(format!("{}-small.png", name.to_lowercase()))
                    .display()
                    .to_string(),
            ],
            &vars,
        )?;
        let completed = repeated["window"]["fit"]["completed"]
            .as_i64()
            .ok_or("Repeated fit count")?;
        sway_ipc(
            &vars,
            &format!("[con_id={identity}] move container to output HEADLESS-1"),
        )?;
        wait("return to large output", || {
            let value = state(&qml, &vars, name)?;
            Ok(
                (value["window"]["screen"] == "HEADLESS-1" && fit_complete(&value, completed))
                    .then_some(()),
            )
        })?;
        let returned = state(&qml, &vars, name)?;
        require(
            returned["window"]["fit"]["error"] == ""
                && returned["window"]["fit"]["result"]["changed"] == false,
            "Return fit changed/error",
        )?;
        require(
            returned["window"]["width"] == small["window"]["width"]
                && returned["window"]["height"] == small["window"]["height"],
            "Return dimensions changed",
        )?;
        if name == "Files" {
            require(
                returned["selection"] == before["selection"]
                    && returned["previewOpen"] == before["previewOpen"]
                    && returned["preview"] == before["preview"],
                "Files selection/preview lost",
            )?;
        }
        records.push(json!({"consumer":name,"before":before,"small":small,"repeat":repeated,"returned":returned,"tree":node}));
        sway_ipc(&vars, "focus output HEADLESS-1")?;
    }
    let source_revision = command(
        &resolve("git", &vars)?,
        &[
            "-C",
            &env::current_dir()?.display().to_string(),
            "rev-parse",
            "HEAD",
        ],
        &vars,
    )?;
    let fixture_hash = sha256(include_bytes!("main.rs"));
    let provider_hash = sha256(&fs::read(env::current_exe()?)?);
    let result = json!({"passed":true,"cases":records,"source":source_revision,"launcher":options.launcher,"compositor":options.sway,
        "effects":options.effects,"nativeFrame":if options.native_frame{"on"}else{"off"},"fixtureSha256":fixture_hash,"providerSha256":provider_hash,
        "assetsSha256":asset_hashes(&assets)?});
    save_json(&out.join("result.json"), &result)?;
    println!("PASS consumer fit");
    Ok(())
}
fn main() -> Result<()> {
    let name = env::args_os().next().unwrap_or_default();
    if Path::new(&name)
        .file_name()
        .is_some_and(|name| name == "lumen-calendar")
    {
        return calendar_helper();
    }
    let options = options()?;
    if options.inside {
        run(options)
    } else {
        private_session()
    }
}
