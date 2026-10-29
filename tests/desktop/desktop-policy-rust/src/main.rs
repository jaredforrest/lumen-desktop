//! Shared native compositor, Overview, pointer, keyboard, and backend harness.
mod application;
mod display;
mod display_recovery;
mod lifecycle;
mod owned_geometry;
mod owned_inactive;
mod screen_saver_queue;
mod typography;
mod desktop_typography;
mod dock_offer;
mod semantic_state;
mod live_overlay;
mod utility_typography;
mod calendar_detail_typography;
mod overview_typography;
mod folder_stack;
mod launcher_typography;
mod physical_keyboard;
mod resize_hit_targets;
mod shared_control_typography;
mod dock_motion;
mod reduced_motion;
mod pointer_retirement;
mod window_controls;
mod minimized_dock;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
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
fn need(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn path(value: &Path) -> String {
    value.to_string_lossy().into_owned()
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn load(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn hash(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        };
        if Instant::now() >= deadline {
            return Err(fail(format!(
                "Desktop policy timeout waiting for {label}: {last}"
            )));
        }
        pause(40)
    }
}
fn first(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && !name.ends_with(suffix)
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
            symlink(fs::read_link(&from)?, &to)?
        } else if kind.is_dir() {
            copy_tree(&from, &to)?
        } else if kind.is_file() {
            fs::copy(&from, &to)?;
            let mut permissions = fs::metadata(&to)?.permissions();
            permissions.set_mode(permissions.mode() | 0o200);
            fs::set_permissions(&to, permissions)?
        } else {
            return Err(fail(format!("Unsupported asset {}", from.display())));
        }
    }
    Ok(())
}
fn launcher_env(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher).arg("--print-env0").output()?;
    need(result.status.success(), "Launcher environment unavailable")?;
    let mut vars = BTreeMap::new();
    for bytes in result.stdout.split(|byte| *byte == 0) {
        if let Some((key, value)) = std::str::from_utf8(bytes)?.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    Ok(vars)
}
fn nodes<'a>(root: &'a Value, all: &mut Vec<&'a Value>) {
    all.push(root);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = root[key].as_array() {
            for child in children {
                nodes(child, all)
            }
        }
    }
}
fn find_node(tree: &Value, predicate: impl Fn(&Value) -> bool) -> Option<Value> {
    let mut all = Vec::new();
    nodes(tree, &mut all);
    all.into_iter().find(|node| predicate(node)).cloned()
}
fn members(tree: &Value, predicate: impl Fn(&Value) -> bool) -> Vec<Value> {
    let mut all = Vec::new();
    nodes(tree, &mut all);
    all.into_iter()
        .filter(|node| predicate(node))
        .cloned()
        .collect()
}
fn number(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| fail(format!("Expected integer: {value}")))
}
struct Private(PathBuf);
impl Private {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = PathBuf::from(format!(
            "/tmp/ldp-{}-{}",
            std::process::id(),
            stamp % 1_000_000_000
        ));
        fs::create_dir(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        Ok(Self(root))
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct ChildProcess(Child);
impl ChildProcess {
    fn stop(&mut self, signal: i32) -> Result<()> {
        if self.0.try_wait()?.is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, signal);
            }
        }
        let end = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait()?.is_none() {
            if Instant::now() > end {
                unsafe {
                    libc::killpg(self.0.id() as i32, libc::SIGKILL);
                }
                self.0.wait()?;
                break;
            }
            pause(25)
        }
        Ok(())
    }
}
impl Drop for ChildProcess {
    fn drop(&mut self) {
        let _ = self.stop(libc::SIGTERM);
    }
}
struct Options {
    scenario: String,
    launcher: PathBuf,
    sway: PathBuf,
    client: Option<PathBuf>,
    pointer: Option<PathBuf>,
    keyboard: Option<PathBuf>,
    font_config: Option<PathBuf>,
    software_driver: Option<PathBuf>,
    effects: bool,
    output: PathBuf,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut scenario = None;
        let mut launcher = None;
        let mut sway = None;
        let mut client = None;
        let mut pointer = None;
        let mut keyboard = None;
        let mut font_config = None;
        let mut software_driver = None;
        let mut effects = false;
        let mut output = None;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("application" | "lifecycle" | "display" | "display-recovery" | "display-recovery-legacy" | "owned-geometry" | "owned-geometry-legacy" | "owned-components" | "owned-components-legacy" | "owned-inactive" | "owned-inactive-native" | "typography" | "desktop-typography" | "utility-typography" | "calendar-detail-typography" | "overview-typography" | "folder-stack" | "launcher-typography" | "shared-control-typography" | "resize-hit-targets" | "resize-hit-targets-rapid" | "dock-motion" | "reduced-motion" | "pointer-retirement" | "window-controls" | "minimized-dock" | "dock-offer" | "dock-offer-busy" | "semantic-state" | "live-overlay-appearance") => {
                    scenario = Some(arg.to_string_lossy().into_owned())
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
                Some("--client") => {
                    client = Some(PathBuf::from(args.next().ok_or("Missing client")?))
                }
                Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--keyboard") => keyboard = Some(PathBuf::from(args.next().ok_or("Missing keyboard")?)),
                Some("--font-config") => font_config = Some(PathBuf::from(args.next().ok_or("Missing font config")?)),
                Some("--software-driver") => software_driver = Some(PathBuf::from(args.next().ok_or("Missing software driver")?)),
                Some("--effects") => effects = true,
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                _ => return Err(fail(format!("Unknown argument {}", arg.to_string_lossy()))),
            }
        }
        let output = output.ok_or("Missing output")?;
        let output = if output.is_absolute() {
            output
        } else {
            env::current_dir()?.join(output)
        };
        Ok(Self {
            scenario: scenario.ok_or("Missing scenario")?,
            launcher: launcher.ok_or("Missing launcher")?.canonicalize()?,
            sway: sway.ok_or("Missing sway")?.canonicalize()?,
            client: client.map(|p| p.canonicalize()).transpose()?,
            pointer: pointer.map(|p| p.canonicalize()).transpose()?,
            keyboard: keyboard.map(|p| p.canonicalize()).transpose()?,
            font_config: font_config.map(|p| p.canonicalize()).transpose()?,
            software_driver: software_driver.map(|p| p.canonicalize()).transpose()?,
            effects,
            output,
        })
    }
}
struct Context {
    opt: Options,
    root: Private,
    repo: PathBuf,
    vars: BTreeMap<String, String>,
    swaymsg: PathBuf,
    qml: PathBuf,
    children: Vec<ChildProcess>,
    pointer: Option<usize>,
    held: bool,
    position: (f64, f64),
    original_socket: Option<String>,
}
impl Drop for Context {
    fn drop(&mut self) {
        self.children.clear();
    }
}
impl Context {
    fn launch(&mut self, command: &str, args: &[&str], log: &str, piped: bool) -> Result<usize> {
        let file = File::create(self.opt.output.join(log))?;
        let mut process = Command::new(command);
        process
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .stdin(if piped { Stdio::piped() } else { Stdio::null() })
            .stdout(file.try_clone()?)
            .stderr(file)
            .process_group(0);
        self.children.push(ChildProcess(process.spawn()?));
        Ok(self.children.len() - 1)
    }
    fn call(&self, command: &str, args: &[&str]) -> Result<String> {
        let result = Command::new(command)
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .output()?;
        need(
            result.status.success(),
            format!(
                "{command} {args:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            ),
        )?;
        Ok(String::from_utf8(result.stdout)?.trim().into())
    }
    fn input(&self, command: &str, input: &Value) -> Result<Value> {
        let mut process = Command::new(command)
            .env_clear()
            .envs(&self.vars)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        process
            .stdin
            .take()
            .ok_or("Missing stdin")?
            .write_all(serde_json::to_string(input)?.as_bytes())?;
        let result = process.wait_with_output()?;
        need(
            result.status.success(),
            format!("{command}: {}", String::from_utf8_lossy(&result.stderr)),
        )?;
        Ok(serde_json::from_slice(&result.stdout)?)
    }
    fn sway(&self, args: &[&str]) -> Result<Value> {
        let mut all = vec!["-r"];
        all.extend_from_slice(args);
        Ok(serde_json::from_str(
            &self.call(&path(&self.swaymsg), &all)?,
        )?)
    }
    fn checked(&self, command: &str) -> Result<()> {
        let result = self.sway(&[command])?;
        need(
            result
                .as_array()
                .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|r| r["success"] == true)),
            format!("Sway command failed: {command}: {result}"),
        )
    }
    fn tree(&self) -> Result<Value> {
        self.sway(&["-t", "get_tree"])
    }
    fn native(&self, app: &str) -> Result<Option<Value>> {
        Ok(find_node(&self.tree()?, |node| node["app_id"] == app))
    }
    fn app(&self, app: &str) -> Result<Vec<Value>> {
        Ok(members(&self.tree()?, |node| node["app_id"] == app))
    }
    fn find(&self, id: &Value) -> Result<Option<Value>> {
        Ok(find_node(&self.tree()?, |node| node["id"] == *id))
    }
    fn request(&self, action: &str, values: Value) -> Result<Value> {
        let mut command = json!({"action":action});
        let map = command.as_object_mut().ok_or("Request object")?;
        for (key, value) in values.as_object().ok_or("Request values")? {
            map.insert(key.clone(), value.clone());
        }
        self.input("lumen-window-overview", &command)
    }
    fn status(&self) -> Result<Value> {
        let result = self.request("status", json!({}))?;
        need(
            result.get("error").is_none(),
            format!("Overview status error: {result}"),
        )?;
        Ok(result)
    }
    fn mutate(&self, action: &str, mut values: Value) -> Result<Value> {
        let state = self.status()?;
        values["revision"] = state["revision"].clone();
        values["sessionEpoch"] = state["sessionEpoch"].clone();
        let result = self.request(action, values)?;
        need(
            result.get("error").is_none(),
            format!("Overview mutation error: {result}"),
        )?;
        Ok(result)
    }
    fn row(&self, id: &Value) -> Result<Value> {
        self.status()?["windows"]
            .as_array()
            .ok_or("Missing windows")?
            .iter()
            .find(|row| row["id"] == *id)
            .cloned()
            .ok_or_else(|| fail(format!("Missing window {id}")))
    }
    fn foot(&mut self, app: &str, log: &str) -> Result<Value> {
        let before: BTreeSet<i64> = self
            .app(app)?
            .iter()
            .filter_map(|n| n["id"].as_i64())
            .collect();
        self.launch(
            "foot",
            &[&format!("--app-id={app}"), "sleep", "180"],
            &format!("{log}.log"),
            false,
        )?;
        wait("new native foot window", || {
            Ok(self
                .app(app)?
                .into_iter()
                .find(|n| n["id"].as_i64().is_some_and(|id| !before.contains(&id))))
        })
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let target = if self.opt.scenario == "application" { "membershipAudit" }
            else if self.opt.scenario.starts_with("display-recovery") { "displaySpaceRecovery" }
            else if self.opt.scenario.starts_with("owned-geometry") { "ownedGeometryAudit" }
            else if self.opt.scenario.starts_with("owned-components") { "ownedComponentsAudit" }
            else if self.opt.scenario == "typography" { "ownedText" }
            else if self.opt.scenario == "desktop-typography" { "desktopText" }
            else if self.opt.scenario == "semantic-state" { "semantic" }
            else if self.opt.scenario == "live-overlay-appearance" { "liveOverlay" }
            else if self.opt.scenario == "utility-typography" { "utilityTypography" }
            else if self.opt.scenario == "calendar-detail-typography" { "calendarDetails" }
            else if self.opt.scenario == "overview-typography" { "overviewText" }
            else if self.opt.scenario == "folder-stack" { "stackAudit" }
            else if self.opt.scenario == "launcher-typography" { "launcherText" }
            else if self.opt.scenario == "shared-control-typography" { "sharedTypography" }
            else if self.opt.scenario == "dock-motion" { "dockAudit" }
            else if self.opt.scenario == "reduced-motion" { "motionAudit" }
            else { "desktopAudit" };
        let qml = path(&self.qml);
        let mut all = vec!["-p", qml.as_str(), "ipc", "call", target, method];
        all.extend_from_slice(args);
        self.call("quickshell", &all)
    }
    fn ui(&self) -> Result<Value> {
        let state: Value = serde_json::from_str(&self.ipc("status", &[])?)?;
        save(&self.opt.output.join("last-ui.json"), &state)?;
        Ok(state)
    }
    fn picture(&self, name: &str) -> Result<()> {
        self.call(option_env!("LUMEN_GRIM").unwrap_or("grim"), &[&path(&self.opt.output.join(name))])?;
        Ok(())
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.call("wtype", args)?;
        Ok(())
    }
    fn write(&mut self, index: usize, text: &str) -> Result<()> {
        let stdin = self.children[index]
            .0
            .stdin
            .as_mut()
            .ok_or("Process stdin unavailable")?;
        stdin.write_all(text.as_bytes())?;
        stdin.flush()?;
        Ok(())
    }
    fn pointer_start(&mut self) -> Result<()> {
        let tool = path(self.opt.pointer.as_ref().ok_or("Missing pointer tool")?);
        self.pointer =
            Some(self.launch(&tool, &["pointer", "move", "0", "0"], "pointer.log", true)?);
        Ok(())
    }
    fn pointer_command(&mut self, command: &str) -> Result<()> {
        self.write(
            self.pointer.ok_or("Pointer not started")?,
            &format!("{command}\n"),
        )?;
        pause(70);
        Ok(())
    }
    fn move_pointer(&mut self, point: (f64, f64)) -> Result<()> {
        if self.held {
            self.pointer_command(&format!(
                "m {} {}",
                point.0 - self.position.0,
                point.1 - self.position.1
            ))?
        } else {
            self.checked(&format!(
                "seat seat0 cursor set {} {}",
                point.0 as i64, point.1 as i64
            ))?;
            self.pointer_command("m 0.5 0.5")?
        }
        self.position = point;
        Ok(())
    }
    fn press(&mut self) -> Result<()> {
        self.pointer_command("p")?;
        self.held = true;
        Ok(())
    }
    fn release(&mut self) -> Result<()> {
        self.pointer_command("r")?;
        self.held = false;
        Ok(())
    }
}
fn setup(opt: Options) -> Result<Context> {
    if !opt.output.exists() { fs::create_dir(&opt.output)?; }
    let repo = env::current_dir()?;
    let original_socket = env::var("SWAYSOCK").ok();
    let root = Private::new()?;
    let mut vars = launcher_env(&opt.launcher)?;
    for key in [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_STATE_HOME",
        "XDG_DATA_HOME",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
    ] {
        let value = root.0.join(key);
        fs::create_dir_all(&value)?;
        fs::set_permissions(&value, fs::Permissions::from_mode(0o700))?;
        vars.insert(key.into(), path(&value));
    }
    for key in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
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
        ("QT_QUICK_BACKEND", "software"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        vars.insert(key.into(), value.into());
    }
    if opt.scenario == "desktop-typography" || opt.scenario == "semantic-state" || opt.scenario == "live-overlay-appearance" || opt.scenario == "utility-typography" || opt.scenario == "calendar-detail-typography" || opt.scenario == "overview-typography" || opt.scenario == "launcher-typography" || opt.scenario == "shared-control-typography" || opt.scenario == "dock-motion" || opt.scenario == "reduced-motion" {
        vars.insert("WLR_HEADLESS_OUTPUTS".into(), "1".into());
    }
    if let Some(font_config) = &opt.font_config {
        need(font_config.starts_with("/nix/store") && font_config.is_file(), "Pinned font configuration required")?;
        vars.insert("FONTCONFIG_FILE".into(), path(font_config));
    }
    if let Some(driver) = &opt.software_driver {
        vars.insert("LIBGL_DRIVERS_PATH".into(), path(&driver.join("lib/dri")));
        vars.insert("__EGL_VENDOR_LIBRARY_FILENAMES".into(), path(&driver.join("share/glvnd/egl_vendor.d/50_mesa.json")));
        vars.insert("MESA_LOADER_DRIVER_OVERRIDE".into(), "llvmpipe".into());
    }
    let swaymsg = opt
        .sway
        .parent()
        .ok_or("Missing sway bin directory")?
        .join("swaymsg");
    let qml = root.0.join("shell.qml");
    let mut ctx = Context {
        opt,
        root,
        repo,
        vars,
        swaymsg,
        qml,
        children: Vec::new(),
        pointer: None,
        held: false,
        position: (0., 0.),
        original_socket,
    };
    let runtime = ctx.root.0.join("XDG_RUNTIME_DIR");
    let busconf = ctx.root.0.join("bus.conf");
    fs::write(
        &busconf,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            runtime.display()
        ),
    )?;
    let log = File::create(ctx.opt.output.join("bus.log"))?;
    let mut bus = Command::new(option_env!("LUMEN_DBUS_DAEMON").unwrap_or("dbus-daemon"))
        .args([
            "--nofork",
            "--print-address=1",
            &format!("--config-file={}", busconf.display()),
        ])
        .env_clear()
        .envs(&ctx.vars)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log)
        .process_group(0)
        .spawn()?;
    let address = BufReader::new(bus.stdout.take().ok_or("No private bus output")?)
        .lines()
        .next()
        .ok_or("No private bus address")??;
    need(
        address.starts_with("unix:"),
        "Private D-Bus address invalid",
    )?;
    ctx.children.push(ChildProcess(bus));
    ctx.vars.insert("DBUS_SESSION_BUS_ADDRESS".into(), address);
    let config = ctx.root.0.join("sway.conf");
    let source = if ctx.opt.scenario == "reduced-motion" {
        "output HEADLESS-1 resolution 1100x800\noutput * bg #29445b solid_color\nfocus_follows_mouse no\nbindsym Mod4+1 workspace number 1\nbindsym Mod4+2 workspace number 2\n"
    } else if ctx.opt.scenario == "shared-control-typography" {
        "output HEADLESS-1 resolution 900x700 position 0 0\noutput * bg #29445b solid_color\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\nfor_window [all] floating enable, border normal 1\n"
    } else if ctx.opt.scenario.starts_with("resize-hit-targets") {
        "output HEADLESS-1 resolution 1600x1200 position 0 0 scale 1\noutput HEADLESS-2 resolution 1600x1200 position 1600 0 scale 1.5\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_floating_border normal 2\nfor_window [all] floating enable\n"
    } else if ctx.opt.scenario.starts_with("owned-") {
        "output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfloating_modifier Mod4 normal\nfor_window [all] floating enable, border normal 1\n"
    } else if ctx.opt.scenario == "application" || ctx.opt.scenario.starts_with("display") {
        "output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1024x768 position 1280 0\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border pixel 0\n"
    } else {
        "output HEADLESS-1 resolution 1440x1000 position 0 0\noutput HEADLESS-2 resolution 1280x900 position 1440 0\nseat seat0 fallback true\nfocus_follows_mouse no\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n"
    };
    fs::write(&config, source)?;
    let sway = path(&ctx.opt.sway);
    ctx.launch(
        &sway,
        &["--unsupported-gpu", "-c", &path(&config)],
        "sway.log",
        false,
    )?;
    ctx.vars.insert(
        "SWAYSOCK".into(),
        path(&wait("private compositor", || {
            first(&runtime, "sway-ipc", ".lock")
        })?),
    );
    ctx.vars.insert(
        "WAYLAND_DISPLAY".into(),
        wait("private display", || {
            Ok(first(&runtime, "wayland-", ".lock")?
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())))
        })?,
    );
    ctx.checked("focus output HEADLESS-1; workspace \"1\"; move workspace to output HEADLESS-1")?;
    let palette = load(&ctx.repo.join("res/lumen-desktop/appearance/palette.json"))?;
    save(
        &ctx.root.0.join("appearance.json"),
        &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}),
    )?;
    ctx.vars.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        path(&ctx.root.0.join("appearance.json")),
    );
    copy_tree(
        &ctx.repo.join("res/lumen-desktop"),
        &ctx.root.0.join("assets"),
    )?;
    if ctx.opt.scenario == "typography" {
        let documents = ctx.root.0.join("Documents");
        fs::create_dir(&documents)?;
        fs::write(documents.join("An ordinary document.txt"), "Private fixture unchanged\n")?;
        fs::write(documents.join("Another document.txt"), "Second private document\n")?;
        ctx.vars.insert("LUMEN_FIXTURE_DOCUMENTS".into(), path(&documents));
        save(&ctx.root.0.join("appearance.json"), &json!({"font":palette["font"],"palette":palette["light"],"preferences":{"textScale":1.5,"reduceMotion":true,"reduceTransparency":true}}))?;
    }
    if ctx.opt.scenario == "desktop-typography" {
        desktop_typography::prepare(&mut ctx, &palette)?;
    }
    let scene = if ctx.opt.scenario == "application" { include_str!("../application.qml") }
        else if ctx.opt.scenario.starts_with("display-recovery") { include_str!("../display_recovery.qml") }
        else if ctx.opt.scenario.starts_with("owned-geometry") { include_str!("../owned_geometry.qml") }
        else if ctx.opt.scenario.starts_with("owned-components") { include_str!("../owned_components.qml") }
        else if ctx.opt.scenario.starts_with("owned-inactive") { include_str!("../owned_inactive.qml") }
        else if ctx.opt.scenario == "typography" { include_str!("../typography.qml") }
        else if ctx.opt.scenario == "desktop-typography" { include_str!("../desktop_typography.qml") }
        else if ctx.opt.scenario == "semantic-state" { include_str!("../semantic_state.qml") }
        else if ctx.opt.scenario == "live-overlay-appearance" { include_str!("../live_overlay.qml") }
        else if ctx.opt.scenario == "utility-typography" { include_str!("../utility_typography.qml") }
        else if ctx.opt.scenario == "calendar-detail-typography" { include_str!("../calendar_detail_typography.qml") }
        else if ctx.opt.scenario == "overview-typography" { include_str!("../overview_typography.qml") }
        else if ctx.opt.scenario == "folder-stack" { include_str!("../folder_stack.qml") }
        else if ctx.opt.scenario == "launcher-typography" { include_str!("../launcher_typography.qml") }
        else if ctx.opt.scenario == "shared-control-typography" { include_str!("../shared_control_typography.qml") }
        else if ctx.opt.scenario == "dock-motion" { include_str!("../dock_motion.qml") }
        else if ctx.opt.scenario == "reduced-motion" { include_str!("../reduced_motion.qml") }
        else if ctx.opt.scenario == "pointer-retirement" { include_str!("../pointer_retirement.qml") }
        else { include_str!("../lifecycle.qml") };
    fs::write(&ctx.qml, scene)?;
    Ok(ctx)
}
fn main() -> Result<()> {
    if env::args().nth(1).as_deref() == Some("captured-directory-facts") {
        use std::os::unix::fs::MetadataExt;
        let target = PathBuf::from(env::args_os().nth(2).ok_or("Missing captured directory")?);
        let facts = fs::metadata(target)?;
        println!("{}", json!([facts.dev().to_string(),facts.ino().to_string(),facts.mode().to_string(),
            facts.size().to_string(),(facts.mtime() as i128*1_000_000_000+facts.mtime_nsec() as i128).to_string(),
            (facts.ctime() as i128*1_000_000_000+facts.ctime_nsec() as i128).to_string()]));
        return Ok(());
    }
    if env::args_os().next().and_then(|path| PathBuf::from(path).file_name().map(|name| name.to_owned()))
        .is_some_and(|name| name == "lumen-screen-saver") {
        return screen_saver_queue::provider();
    }
    if env::args().nth(1).as_deref() == Some("screen-saver-release") {
        return screen_saver_queue::release();
    }
    if env::args().nth(1).as_deref()==Some("set") {
        let args=env::args().skip(1).collect::<Vec<_>>();
        need(args.len()==3 && args[1]=="spaces","Unexpected display settings capture")?;
        let value:Value=serde_json::from_str(&args[2])?;
        let target=env::var("LUMEN_DISPLAY_SETTINGS_CAPTURE")?;
        let mut file=OpenOptions::new().create(true).append(true).open(target)?;
        writeln!(file,"{}",json!({"section":"spaces","value":value}))?;
        println!("{}",json!({"ok":true}));
        return Ok(());
    }
    let opt = Options::parse()?;
    let scenario = opt.scenario.clone();
    let mut ctx = setup(opt)?;
    if scenario == "application" {
        application::run(&mut ctx)
    } else if scenario == "display" {
        display::run(&mut ctx)
    } else if scenario.starts_with("display-recovery") {
        display_recovery::run(&mut ctx,scenario.ends_with("legacy"))
    } else if scenario.starts_with("owned-geometry") {
        owned_geometry::run(&mut ctx,scenario.ends_with("legacy"))
    } else if scenario.starts_with("owned-components") {
        owned_geometry::components(&mut ctx,scenario.ends_with("legacy"))
    } else if scenario.starts_with("owned-inactive") {
        owned_inactive::run(&mut ctx,scenario.ends_with("native"))
    } else if scenario == "typography" {
        typography::run(&mut ctx)
    } else if scenario == "desktop-typography" {
        desktop_typography::run(&mut ctx)
    } else if scenario == "semantic-state" {
        semantic_state::run(&mut ctx)
    } else if scenario == "live-overlay-appearance" {
        live_overlay::run(&mut ctx)
    } else if scenario == "utility-typography" {
        utility_typography::run(&mut ctx)
    } else if scenario == "calendar-detail-typography" {
        calendar_detail_typography::run(&mut ctx)
    } else if scenario == "overview-typography" {
        overview_typography::run(&mut ctx)
    } else if scenario == "folder-stack" {
        folder_stack::run(&mut ctx)
    } else if scenario == "launcher-typography" {
        launcher_typography::run(&mut ctx)
    } else if scenario == "shared-control-typography" {
        shared_control_typography::run(&mut ctx)
    } else if scenario.starts_with("resize-hit-targets") {
        resize_hit_targets::run(&mut ctx, scenario.ends_with("rapid"))
    } else if scenario == "dock-motion" {
        dock_motion::run(&mut ctx)
    } else if scenario == "reduced-motion" {
        reduced_motion::run(&mut ctx)
    } else if scenario == "pointer-retirement" {
        pointer_retirement::run(&mut ctx)
    } else if scenario == "window-controls" {
        window_controls::run(&mut ctx)
    } else if scenario == "minimized-dock" {
        minimized_dock::run(&mut ctx)
    } else if scenario.starts_with("dock-offer") {
        dock_offer::run(&mut ctx)
    } else {
        lifecycle::run(&mut ctx)
    }
}
