//! Native search-history routing and concurrency fixture.
use regex::Regex;
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
    app_launcher: PathBuf,
    sway: PathBuf,
    pointer_tool: PathBuf,
    output: PathBuf,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut app_launcher = None;
    let mut sway = None;
    let mut pointer_tool = None;
    let mut output = None;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--app-launcher") => {
                app_launcher = Some(PathBuf::from(args.next().ok_or("Missing app launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
            Some("--pointer-tool") => {
                pointer_tool = Some(PathBuf::from(args.next().ok_or("Missing pointer tool")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown search-history fixture option")),
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
        app_launcher: app_launcher.ok_or("--app-launcher required")?,
        sway: sway.ok_or("--sway required")?,
        pointer_tool: pointer_tool.ok_or("--pointer-tool required")?,
        output,
        effects,
        inside,
    })
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = env::temp_dir().join(format!("lhist-{}-{nonce}", std::process::id()));
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
        format!("Private search-history fixture exited {status}"),
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
fn transform(path: &Path, old: &str, new: &str, reason: &str) -> Result<Value> {
    let original = fs::read_to_string(path)?;
    let count = original.matches(old).count();
    require(
        count == 1,
        format!("Expected exactly one {old:?} in {}", path.display()),
    )?;
    let modified = original.replacen(old, new, 1);
    require(
        modified.replacen(new, old, 1) == original,
        "Fixture transformation not reversible",
    )?;
    fs::write(path, &modified)?;
    Ok(
        json!({"path":path,"reason":reason,"originalSha256":sha256(original.as_bytes()),"modifiedSha256":sha256(modified.as_bytes()),"originalMatchCount":1,"roundtrip":true}),
    )
}
fn package_share() -> Result<PathBuf> {
    Ok(env::current_exe()?
        .parent()
        .ok_or("Fixture bin missing")?
        .parent()
        .ok_or("Fixture prefix missing")?
        .join("share/lumen-search-history-fixture"))
}
fn helper_output() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var("LUMEN_HISTORY_FIXTURE_OUTPUT")?))
}
fn locator() -> Result<()> {
    let out = helper_output()?;
    let args: Vec<String> = env::args().skip(1).collect();
    File::options()
        .append(true)
        .create(true)
        .open(out.join("locator.log"))?
        .write_all(format!("{}\n", args.join(" ")).as_bytes())?;
    print!("{}", fs::read_to_string(out.join("service.pid"))?);
    Ok(())
}
fn fallback() -> Result<()> {
    let out = helper_output()?;
    let args: Vec<String> = env::args().skip(1).collect();
    File::options()
        .append(true)
        .create(true)
        .open(out.join("fallback.jsonl"))?
        .write_all(format!("{}\n", serde_json::to_string(&args)?).as_bytes())?;
    Ok(())
}
fn transport() -> Result<()> {
    let out = helper_output()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line)?;
    if request["action"] == "record" && out.join("hold-record").exists() {
        fs::write(out.join("held-record"), &line)?;
        let until = Instant::now() + Duration::from_secs(10);
        while !out.join("release-record").exists() {
            require(Instant::now() < until, "Private held request expired")?;
            pause(20);
        }
    }
    let backend = PathBuf::from(env::var("LUMEN_SEARCH_HISTORY_BACKEND")?);
    let mut child = Command::new(backend)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("History child stdin missing")?
        .write_all(line.as_bytes())?;
    let result = child.wait_with_output()?;
    std::io::stdout().write_all(&result.stdout)?;
    require(
        result.status.success(),
        format!("History backend exited {}", result.status),
    )
}
impl Managed {
    fn launch_stdin(
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
                .stdin(Stdio::piped())
                .stdout(Stdio::from(file.try_clone()?))
                .stderr(Stdio::from(file))
                .process_group(0)
                .spawn()?,
        ))
    }
    fn send(&mut self, line: &str) -> Result<()> {
        self.0
            .stdin
            .as_mut()
            .ok_or("Pointer stdin unavailable")?
            .write_all(line.as_bytes())?;
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        if self.0.try_wait()?.is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                if self.0.try_wait()?.is_some() {
                    break;
                }
                if Instant::now() > until {
                    unsafe {
                        libc::killpg(self.0.id() as i32, libc::SIGKILL);
                    }
                    self.0.wait()?;
                    break;
                }
                pause(30);
            }
        }
        Ok(())
    }
}
fn all_nodes<'a>(node: &'a Value, output: &mut Vec<&'a Value>) {
    if node.get("app_id").is_some() {
        output.push(node);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                all_nodes(child, output);
            }
        }
    }
}
fn audit(
    shell: &Managed,
    vars: &BTreeMap<String, String>,
    method: &str,
    args: &[&str],
) -> Result<String> {
    let quickshell = resolve("quickshell", vars)?;
    let pid = shell.0.id().to_string();
    let mut command_args = vec!["ipc", "--pid", pid.as_str(), "call", "historyAudit", method];
    command_args.extend_from_slice(args);
    command(&quickshell, &command_args, vars)
}
fn state(shell: &Managed, vars: &BTreeMap<String, String>) -> Result<Value> {
    Ok(serde_json::from_str(&audit(shell, vars, "status", &[])?)?)
}
fn keys(vars: &BTreeMap<String, String>, args: &[&str]) -> Result<()> {
    command(&resolve("wtype", vars)?, args, vars).map(|_| ())
}
fn toggle(vars: &BTreeMap<String, String>) -> Result<()> {
    keys(
        vars,
        &[
            "-M", "logo", "-P", "Super_L", "-k", "space", "-p", "Super_L", "-m", "logo",
        ],
    )
}
fn recall(vars: &BTreeMap<String, String>, delta: i8) -> Result<()> {
    keys(
        vars,
        &[
            "-M",
            "alt",
            "-P",
            "Alt_L",
            "-k",
            if delta > 0 { "Up" } else { "Down" },
            "-p",
            "Alt_L",
            "-m",
            "alt",
        ],
    )
}
fn type_query(shell: &Managed, vars: &BTreeMap<String, String>, text: &str) -> Result<()> {
    keys(vars, &["-M", "ctrl", "a", "-m", "ctrl"])?;
    keys(vars, &[text])?;
    wait("typed query", || {
        Ok((state(shell, vars)?["query"] == text).then_some(()))
    })
}
fn saved(path: &Path) -> Result<Value> {
    if path.exists() {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    } else {
        Ok(json!([]))
    }
}
fn start_shell(
    index: u8,
    source: &Path,
    out: &Path,
    vars: &BTreeMap<String, String>,
) -> Result<Managed> {
    Managed::launch(
        &resolve("quickshell", vars)?,
        &["-p", &source.display().to_string()],
        vars,
        &out.join(format!("shell-{index}.log")),
    )
}
fn state_wait(
    shell: &Managed,
    vars: &BTreeMap<String, String>,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value> {
    wait(label, || {
        let current = state(shell, vars)?;
        Ok(predicate(&current).then_some(current))
    })
}
fn pointer_button(
    text: &str,
    shell: &Managed,
    pointer: &mut Managed,
    vars: &BTreeMap<String, String>,
) -> Result<()> {
    let point: Value = wait("enabled settings button", || {
        let value: Value = serde_json::from_str(&audit(shell, vars, "button", &[text])?)?;
        Ok((value["enabled"] == true).then_some(value))
    })?;
    let tree: Value = serde_json::from_str(&command(
        &resolve("swaymsg", vars)?,
        &["-r", "-t", "get_tree"],
        vars,
    )?)?;
    let mut nodes = Vec::new();
    all_nodes(&tree, &mut nodes);
    let node = nodes
        .into_iter()
        .find(|node| node["name"] == "System Settings")
        .ok_or("Settings window missing")?;
    let x = node["rect"]["x"].as_f64().ok_or("Window x")?
        + node["window_rect"]["x"].as_f64().ok_or("Client x")?
        + point["x"].as_f64().ok_or("Button x")?;
    let y = node["rect"]["y"].as_f64().ok_or("Window y")?
        + node["window_rect"]["y"].as_f64().ok_or("Client y")?
        + point["y"].as_f64().ok_or("Button y")?;
    command(
        &resolve("swaymsg", vars)?,
        &[
            "seat seat0 cursor set",
            &x.round().to_string(),
            &y.round().to_string(),
        ],
        vars,
    )?;
    pointer.send("m 0.5 0.5\n")?;
    pause(120);
    pointer.send("p\n")?;
    pause(120);
    pointer.send("r\n")?;
    Ok(())
}
fn setup(
    options: &Options,
) -> Result<(BTreeMap<String, String>, PathBuf, PathBuf, PathBuf, Value)> {
    require(!options.output.exists(), "Fresh output directory required")?;
    fs::create_dir_all(&options.output)?;
    let out = &options.output;
    let mut vars = launcher_environment(&options.launcher)?;
    for (key, value) in [
        ("GSETTINGS_BACKEND", "memory"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
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
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
    ] {
        vars.insert(key.into(), value.into());
    }
    for key in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
    let home = PathBuf::from(vars.get("HOME").ok_or("Private HOME missing")?);
    let docs = home.join("Documents");
    fs::create_dir(&docs)?;
    vars.insert(
        "LUMEN_SEARCH_ROOTS".into(),
        serde_json::to_string(&[docs.display().to_string()])?,
    );
    vars.insert(
        "LUMEN_HISTORY_FIXTURE_OUTPUT".into(),
        out.display().to_string(),
    );
    let assets_source = PathBuf::from(
        vars.get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Packaged assets missing")?,
    );
    let assets = out.join("assets");
    copy_tree(&assets_source, &assets)?;
    let source = assets.join("shell");
    let provider = resolve("lumen-fixture-providers", &vars)?;
    let isolated = command(
        &provider,
        &[
            "isolate",
            &source.display().to_string(),
            &out.display().to_string(),
        ],
        &vars,
    )?;
    let _: Value = serde_json::from_str(&isolated)?;
    let defaults = json!({"textScale":1,"mode":"light","accent":"blue","iconAppearance":"auto","switchToAppSpace":true,
        "dockMagnification":true,"reduceMotion":true,"reduceTransparency":false,"scrollbars":"auto","increaseContrast":false,
        "sidebarIconSize":"medium","dockPins":null,"dockRecentApps":false,"dockIndicators":true,"dockFolders":[],
        "dockPosition":"bottom","dockAutoHide":false,"dockSize":48,"dockFiles":[]});
    let search_policy =
        json!({"queryHistory":true,"disabledProviders":["applications","files","actions"]});
    let policy = home.join("settings.json");
    save_json(
        &policy,
        &json!({"appearance":defaults,"search":search_policy}),
    )?;
    vars.insert(
        "LUMEN_NIX_SETTINGS_ACTIVE".into(),
        policy.display().to_string(),
    );
    let palettes: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = home.join("state/lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("Appearance parent")?)?;
    save_json(
        &appearance,
        &json!({"font":palettes["font"],"palette":palettes["light"],"preferences":defaults,"searchPreferences":search_policy}),
    )?;
    let mut transforms = Vec::new();
    transforms.push(transform(&source.join("MenuBar.qml"),"function launcherStatus() {return launcherPanel.status();}","function launcherStatus() {return launcherPanel.status();}\n    function historyAuditLauncher() {return launcherPanel;}","Read-only accessor to the actual nested popup launcher")?);
    let shell_path = source.join("shell.qml");
    let original = fs::read_to_string(&shell_path)?;
    let (head, tail) = original.rsplit_once('}').ok_or("Shell root missing")?;
    let addition = fs::read_to_string(package_share()?.join("audit.fragment.qml"))?;
    let modified = format!("{head}{addition}}}{tail}");
    require(
        modified.replacen(&addition, "", 1) == original,
        "Shell audit not reversible",
    )?;
    fs::write(&shell_path, &modified)?;
    transforms.push(json!({"path":shell_path,"reason":"Append read-only launcher/history/settings introspection","originalSha256":sha256(original.as_bytes()),"modifiedSha256":sha256(modified.as_bytes()),"roundtrip":true}));
    let invocation = fs::read_to_string(&options.app_launcher)?;
    let router_re = Regex::new(r"(/nix/store/[^\s]+/bin/lumen-shell-ipc)")?;
    let router_path = router_re
        .captures(&invocation)
        .ok_or("Installed launcher router missing")?
        .get(1)
        .ok_or("Router capture")?
        .as_str()
        .to_owned();
    let router_text = fs::read_to_string(&router_path)?;
    let systemctl_re = Regex::new(r"--systemctl (/nix/store/[^ ]+/bin/systemctl)")?;
    let systemctl_path = systemctl_re
        .captures(&router_text)
        .ok_or("Installed systemctl locator missing")?
        .get(1)
        .ok_or("Locator capture")?
        .as_str()
        .to_owned();
    require(
        router_text.matches("--systemctl ").count() == 1,
        "Installed router changed locator count",
    )?;
    let locator = out.join("systemctl-private");
    symlink(env::current_exe()?, &locator)?;
    let router = out.join("shell-ipc");
    fs::write(&router, &router_text)?;
    transforms.push(transform(
        &router,
        &systemctl_path,
        &locator.display().to_string(),
        "Exact installed router systemctl locator only",
    )?);
    fs::set_permissions(&router, fs::Permissions::from_mode(0o700))?;
    let fallback = out.join("fallback-observer");
    symlink(env::current_exe()?, &fallback)?;
    let entry = out.join("app-launcher");
    fs::write(&entry, &invocation)?;
    transforms.push(transform(
        &entry,
        &router_path,
        &router.display().to_string(),
        "Installed launcher dispatch path points at isolated exact router",
    )?);
    let fallback_path = Regex::new(r"(/nix/store/[^\s]+/bin/rofi)")?
        .captures(&invocation)
        .ok_or("Installed fallback missing")?
        .get(1)
        .ok_or("Fallback capture")?
        .as_str()
        .to_owned();
    transforms.push(transform(
        &entry,
        &fallback_path,
        &fallback.display().to_string(),
        "Only fallback application replaced by argument observer",
    )?);
    fs::set_permissions(&entry, fs::Permissions::from_mode(0o700))?;
    let mut history_hashes = serde_json::Map::new();
    for name in ["AppLauncher.qml", "SearchHistoryState.qml"] {
        let copied = fs::read(source.join(name))?;
        let installed = fs::read(assets_source.join("shell").join(name))?;
        require(
            copied == installed,
            format!("Installed {name} changed before test"),
        )?;
        history_hashes.insert(name.into(), json!(sha256(&copied)));
    }
    let history_bin = resolve("lumen-search-history", &vars)?;
    vars.insert(
        "LUMEN_SEARCH_HISTORY_BACKEND".into(),
        history_bin.display().to_string(),
    );
    let replacement = format!(
        "{},\"transport\"",
        serde_json::to_string(&env::current_exe()?.display().to_string())?
    );
    transforms.push(transform(&source.join("SearchHistoryState.qml"),"\"lumen-search-history\"",&replacement,"Exact history transport gate delays only an original captured record; Rust backend unmodified")?);
    save_json(
        &out.join("manifest.json"),
        &json!({"historySourceHashes":history_hashes,"launcher":options.launcher,"appLauncher":options.app_launcher,"transforms":transforms,"historyBackendUnmodified":true,"noClipboardOffers":true}),
    )?;
    Ok((vars, source, entry, appearance, search_policy))
}
fn run(options: Options) -> Result<()> {
    let (mut vars, source, entry, appearance, _search_policy) = setup(&options)?;
    let out = &options.output;
    let home = PathBuf::from(vars.get("HOME").ok_or("Private HOME missing")?);
    let history_path = home.join("state/lumen-desktop/search/queries.json");
    let config = out.join("sway.conf");
    fs::write(
        &config,
        format!(
            "output HEADLESS-1 resolution 1280x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_border none\nfor_window [all] floating enable\nbindsym Mod4+space exec {} >{} 2>&1\n",
            entry.display(),
            out.join("invocation.log").display()
        ),
    )?;
    let _sway = Managed::launch(
        &options.sway,
        &["--unsupported-gpu", "-c", &config.display().to_string()],
        &vars,
        &out.join("sway.log"),
    )?;
    let socket = wait("compositor", || first_file(&home, "sway-ipc", ""))?;
    vars.insert("SWAYSOCK".into(), socket.display().to_string());
    let display = wait("display", || first_file(&home, "wayland-", ".lock"))?;
    vars.insert(
        "WAYLAND_DISPLAY".into(),
        display
            .file_name()
            .ok_or("Display name")?
            .to_string_lossy()
            .into(),
    );
    let _keyboard = Managed::launch(
        &resolve("wtype", &vars)?,
        &["-s", "600000"],
        &vars,
        &out.join("keyboard.log"),
    )?;
    let mut pointer = Managed::launch_stdin(
        &options.pointer_tool,
        &["pointer", "move", "0", "0"],
        &vars,
        &out.join("pointer.log"),
    )?;
    let pidfile = out.join("service.pid");
    let mut shell = start_shell(1, &source, out, &vars)?;
    let first_pid = shell.0.id();
    fs::write(&pidfile, first_pid.to_string())?;
    state_wait(&shell, &vars, "shell", |v| {
        v["menus"].as_array().is_some_and(|m| m.len() == 1)
    })?;
    require(
        state(&shell, &vars)?["visible"] == false && saved(&history_path)? == json!([]),
        "Initial history not empty",
    )?;
    toggle(&vars)?;
    state_wait(&shell, &vars, "installed invocation", |v| {
        v["visible"] == true && v["enabled"] == true && v["busy"] == false
    })?;
    type_query(&shell, &vars, "Synthetic first query")?;
    keys(&vars, &["-k", "Escape"])?;
    wait("first deliberate record", || {
        Ok((state(&shell, &vars)?["visible"] == false
            && saved(&history_path)? == json!(["Synthetic first query"]))
        .then_some(()))
    })?;
    toggle(&vars)?;
    state_wait(&shell, &vars, "reopen", |v| {
        v["visible"] == true && v["busy"] == false
    })?;
    type_query(&shell, &vars, "Synthetic second query")?;
    keys(&vars, &["-k", "Escape"])?;
    wait("second deliberate record", || {
        Ok(
            (saved(&history_path)? == json!(["Synthetic second query", "Synthetic first query"]))
                .then_some(()),
        )
    })?;
    let mut checks =
        vec!["native installed invocation records ordinary dismissed queries with explicit policy"];
    toggle(&vars)?;
    state_wait(&shell, &vars, "history loaded", |v| {
        v["visible"] == true && v["busy"] == false
    })?;
    type_query(&shell, &vars, "Synthetic draft")?;
    recall(&vars, 1)?;
    state_wait(&shell, &vars, "latest history", |v| {
        v["query"] == "Synthetic second query"
    })?;
    recall(&vars, 1)?;
    state_wait(&shell, &vars, "older history", |v| {
        v["query"] == "Synthetic first query"
    })?;
    recall(&vars, 1)?;
    require(
        state(&shell, &vars)?["query"] == "Synthetic first query",
        "Recall exceeded oldest",
    )?;
    recall(&vars, -1)?;
    state_wait(&shell, &vars, "newer history", |v| {
        v["query"] == "Synthetic second query"
    })?;
    recall(&vars, -1)?;
    state_wait(&shell, &vars, "restored draft", |v| {
        v["query"] == "Synthetic draft"
    })?;
    recall(&vars, -1)?;
    require(
        state(&shell, &vars)?["query"] == "Synthetic draft",
        "Recall exceeded draft",
    )?;
    checks.push("actual Alt Up Down bounds retain and restore typed draft");
    keys(&vars, &["-k", "Escape"])?;
    wait("record restored draft", || {
        Ok((saved(&history_path)?[0] == "Synthetic draft").then_some(()))
    })?;
    let expected = saved(&history_path)?;
    toggle(&vars)?;
    state_wait(&shell, &vars, "clipboard category", |v| {
        v["visible"] == true && v["busy"] == false
    })?;
    keys(&vars, &["-M", "ctrl", "4", "-m", "ctrl"])?;
    state_wait(&shell, &vars, "clipboard category selected", |v| {
        v["provider"] == "Clipboard"
    })?;
    type_query(&shell, &vars, "Synthetic excluded clipboard query")?;
    keys(&vars, &["-k", "Escape"])?;
    state_wait(&shell, &vars, "clipboard dismissed", |v| {
        v["visible"] == false
    })?;
    require(
        saved(&history_path)? == expected,
        "Clipboard query was recorded",
    )?;
    checks.push("clipboard-category query excluded without reading any clipboard offer");
    let mode = fs::metadata(&history_path)?.permissions().mode() & 0o777;
    require(
        mode == 0o600
            && fs::metadata(history_path.parent().ok_or("History parent")?)?
                .permissions()
                .mode()
                & 0o777
                == 0o700,
        "Private history mode incorrect",
    )?;
    shell.stop()?;
    shell = start_shell(2, &source, out, &vars)?;
    let second_pid = shell.0.id();
    state_wait(&shell, &vars, "replacement shell", |v| {
        v["menus"].as_array().is_some_and(|m| m.len() == 1)
    })?;
    toggle(&vars)?;
    wait("stale PID fallback", || {
        Ok(out.join("fallback.jsonl").exists().then_some(()))
    })?;
    require(
        state(&shell, &vars)?["visible"] == false,
        "Stale PID retargeted new shell",
    )?;
    let fallback_text = fs::read_to_string(out.join("fallback.jsonl"))?;
    let last = fallback_text
        .lines()
        .last()
        .ok_or("No fallback arguments")?;
    require(
        serde_json::from_str::<Value>(last)? == json!(["-show", "drun"]),
        "Fallback arguments changed",
    )?;
    checks.push(
        "stale captured service PID uses observed fallback without retargeting replacement shell",
    );
    fs::write(&pidfile, second_pid.to_string())?;
    toggle(&vars)?;
    state_wait(&shell, &vars, "replacement invocation", |v| {
        v["visible"] == true && v["busy"] == false
    })?;
    require(
        state(&shell, &vars)?["history"] == expected,
        "History missing after restart",
    )?;
    recall(&vars, 1)?;
    state_wait(&shell, &vars, "persisted latest query", |v| {
        v["query"] == expected[0]
    })?;
    checks.push("new service PID invocation restores persisted history after actual shell restart");
    for _ in 0..expected.as_array().ok_or("Expected history array")?.len() {
        recall(&vars, -1)?;
    }
    state_wait(&shell, &vars, "restore empty new draft", |v| {
        v["query"] == ""
    })?;
    toggle(&vars)?;
    state_wait(&shell, &vars, "toggle close", |v| v["visible"] == false)?;
    toggle(&vars)?;
    let toggled = state_wait(&shell, &vars, "toggle open", |v| v["visible"] == true)?;
    require(
        toggled["menus"]
            .as_array()
            .is_some_and(|menus| menus.iter().filter(|m| *m == "Launcher").count() == 1),
        "Multiple search surfaces",
    )?;
    checks.push("repeated installed native invocation toggles a single search surface");
    type_query(&shell, &vars, "Synthetic pre-disable pending intent")?;
    fs::write(out.join("hold-record"), "")?;
    command(
        &out.join("shell-ipc"),
        &["call", "desktop", "settings", "Search"],
        &vars,
    )?;
    wait("held before disable", || {
        Ok(out.join("held-record").exists().then_some(()))
    })?;
    let mut prefs: Value = serde_json::from_slice(&fs::read(&appearance)?)?;
    prefs["searchPreferences"]["queryHistory"] = json!(false);
    save_json(&appearance, &prefs)?;
    state_wait(&shell, &vars, "observed disable", |v| v["enabled"] == false)?;
    prefs["searchPreferences"]["queryHistory"] = json!(true);
    save_json(&appearance, &prefs)?;
    state_wait(&shell, &vars, "observed reenable", |v| v["enabled"] == true)?;
    fs::write(out.join("release-record"), "")?;
    wait("captured disable Clear completes", || {
        Ok(
            (state(&shell, &vars)?["busy"] == false && saved(&history_path)? == json!([]))
                .then_some(()),
        )
    })?;
    require(!history_path.exists(), "Disabled history file survived")?;
    checks.push("observed rapid disable reenable preserves queued Clear intent while original worker is held");
    for path in ["hold-record", "held-record", "release-record"] {
        fs::remove_file(out.join(path))?;
    }
    toggle(&vars)?;
    state_wait(&shell, &vars, "new post-disable invocation", |v| {
        v["visible"] == true && v["busy"] == false
    })?;
    type_query(&shell, &vars, "Synthetic pre-clear pending intent")?;
    fs::write(out.join("hold-record"), "")?;
    command(
        &out.join("shell-ipc"),
        &["call", "desktop", "settings", "Search"],
        &vars,
    )?;
    wait("held original record", || {
        Ok(out.join("held-record").exists().then_some(()))
    })?;
    state_wait(&shell, &vars, "Search settings", |v| {
        v["settingsVisible"] == true && v["settingsPage"] == "Search" && v["settingsBusy"] == false
    })?;
    pointer_button("Clear Search History", &shell, &mut pointer, &vars)?;
    wait("actual clear", || {
        Ok(
            (state(&shell, &vars)?["settingsBusy"] == false && !history_path.exists())
                .then_some(()),
        )
    })?;
    fs::write(out.join("release-record"), "")?;
    state_wait(&shell, &vars, "retired record completed", |v| {
        v["busy"] == false
    })?;
    require(!history_path.exists(), "Retired record recreated history")?;
    let held: Value = serde_json::from_slice(&fs::read(out.join("held-record"))?)?;
    require(
        held["generation"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "Held record generation missing",
    )?;
    checks
        .push("actual settings Clear Search History rejects an in-flight earlier captured record");
    shell.stop()?;
    shell = start_shell(3, &source, out, &vars)?;
    fs::write(&pidfile, shell.0.id().to_string())?;
    state_wait(&shell, &vars, "post-clear replacement", |v| {
        v["menus"].as_array().is_some_and(|m| m.len() == 1)
    })?;
    toggle(&vars)?;
    let restored = state_wait(&shell, &vars, "post-clear invocation", |v| {
        v["visible"] == true && v["busy"] == false
    })?;
    require(
        restored["history"] == json!([]) && saved(&history_path)? == json!([]),
        "Cleared history returned after restart",
    )?;
    recall(&vars, 1)?;
    require(
        state(&shell, &vars)?["query"] == "",
        "Retired query recalled",
    )?;
    checks.push("clear survives actual shell replacement and cannot recall retired queries");
    let result = json!({"passed":true,"checks":checks,"launcher":options.launcher,"appLauncher":options.app_launcher,"effects":options.effects,
        "shellPids":[first_pid,second_pid,shell.0.id()],"privateHistoryMode":mode,"clipboardOffersRead":false,"hostServicesUsed":false});
    save_json(&out.join("result.json"), &result)?;
    println!("{}", result);
    Ok(())
}
fn main() -> Result<()> {
    let name = env::args_os().next().unwrap_or_default();
    let name = Path::new(&name)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    match name.as_ref() {
        "systemctl-private" => locator(),
        "fallback-observer" => fallback(),
        _ => {
            if env::args().nth(1).as_deref() == Some("transport") {
                transport()
            } else {
                let options = options()?;
                if options.inside {
                    run(options)
                } else {
                    private_session()
                }
            }
        }
    }
}
