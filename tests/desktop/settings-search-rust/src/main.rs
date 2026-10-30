//! Physical settings search against the installed scene in a private compositor.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn ensure(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
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
            return Err(fail(format!("Timed out: {label}: {last}")));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn immutable_path(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .map_err(|error| fail(format!("Resolve {label} {}: {error}", path.display())))?;
    ensure(
        canonical.starts_with("/nix/store/"),
        format!("Unpinned {label}: {}", path.display()),
    )?;
    Ok(canonical)
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = fs::symlink_metadata(&source)?.file_type();
        if kind.is_symlink() {
            symlink(fs::read_link(&source)?, &target)?;
        } else if kind.is_dir() {
            copy_tree(&source, &target)?;
        } else if kind.is_file() {
            fs::copy(&source, &target)?;
            let mut mode = fs::metadata(&target)?.permissions();
            mode.set_mode(mode.mode() | 0o200);
            fs::set_permissions(&target, mode)?;
        } else {
            return Err(fail(format!("Unsupported asset: {}", source.display())));
        }
    }
    Ok(())
}
fn first(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && !name.ends_with(suffix)
            })
        }))
}
struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    output: PathBuf,
    effects: bool,
    inside: bool,
    revision: String,
    parent_net: String,
    egl: Option<PathBuf>,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut pointer = None;
    let mut output = None;
    let mut effects = false;
    let mut inside = false;
    let mut revision = String::new();
    let mut parent_net = String::new();
    let mut egl = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing compositor")?)),
            Some("--pointer-tool") => {
                pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer tool")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            Some("--source-revision") => {
                revision = args
                    .next()
                    .ok_or("Missing revision")?
                    .to_string_lossy()
                    .into()
            }
            Some("--parent-net") => {
                parent_net = args
                    .next()
                    .ok_or("Missing net namespace")?
                    .to_string_lossy()
                    .into()
            }
            Some("--egl-vendor") => {
                egl = Some(PathBuf::from(args.next().ok_or("Missing EGL vendor")?))
            }
            _ => return Err(fail(format!("Unknown option: {}", arg.to_string_lossy()))),
        }
    }
    let output = output.ok_or("--output required")?;
    let output = if output.is_absolute() {
        output
    } else {
        env::current_dir()?.join(output)
    };
    Ok(Options {
        launcher: immutable_path(&launcher.ok_or("--launcher required")?, "launcher")?,
        sway: immutable_path(&sway.ok_or("--sway required")?, "compositor")?,
        pointer: immutable_path(&pointer.ok_or("--pointer-tool required")?, "pointer")?,
        output,
        effects,
        inside,
        revision,
        parent_net,
        egl: egl
            .map(|path| immutable_path(&path, "EGL vendor"))
            .transpose()?,
    })
}
fn run_command(program: &str, args: &[&str], vars: &BTreeMap<String, String>) -> Result<String> {
    let result = Command::new(program)
        .args(args)
        .env_clear()
        .envs(vars)
        .output()
        .map_err(|error| fail(format!("Spawn {program} {args:?}: {error}")))?;
    ensure(
        result.status.success(),
        format!(
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}
fn launcher_env(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher)
        .arg("--print-env0")
        .output()
        .map_err(|error| {
            fail(format!(
                "Read launcher environment {}: {error}",
                launcher.display()
            ))
        })?;
    ensure(result.status.success(), "Launcher environment unavailable")?;
    let mut vars = BTreeMap::new();
    for part in result.stdout.split(|byte| *byte == 0) {
        if let Some((name, value)) = std::str::from_utf8(part)?.split_once('=') {
            vars.insert(name.into(), value.into());
        }
    }
    Ok(vars)
}
struct Managed(Child);
impl Managed {
    fn launch(
        program: &str,
        args: &[&str],
        vars: &BTreeMap<String, String>,
        log: &Path,
        stdin: bool,
    ) -> Result<Self> {
        let file = File::create(log)?;
        Ok(Self(
            Command::new(program)
                .args(args)
                .env_clear()
                .envs(vars)
                .stdin(if stdin { Stdio::piped() } else { Stdio::null() })
                .stdout(Stdio::from(file.try_clone()?))
                .stderr(Stdio::from(file))
                .process_group(0)
                .spawn()?,
        ))
    }
    fn write(&mut self, command: &str) -> Result<()> {
        self.0
            .stdin
            .as_mut()
            .ok_or("Missing child stdin")?
            .write_all(command.as_bytes())?;
        self.0
            .stdin
            .as_mut()
            .ok_or("Missing child stdin")?
            .flush()?;
        Ok(())
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
                if Instant::now() >= end {
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
fn private_bus(vars: &mut BTreeMap<String, String>, out: &Path, runtime: &Path) -> Result<Managed> {
    vars.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}", runtime.join("no-system-bus").display()),
    );
    let config = out.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            runtime.display()
        ),
    )?;
    let file = File::create(out.join("bus.log"))?;
    let mut process = Command::new(env!("LUMEN_SETTINGS_SEARCH_DBUS_DAEMON"))
        .arg("--nofork")
        .arg("--print-address=1")
        .arg(format!("--config-file={}", config.display()))
        .env_clear()
        .envs(&*vars)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(file))
        .process_group(0)
        .spawn()?;
    let line = BufReader::new(process.stdout.take().ok_or("Bus stdout missing")?)
        .lines()
        .next()
        .ok_or("Bus address missing")??;
    ensure(
        line.starts_with("unix:"),
        "Private bus did not publish an address",
    )?;
    vars.insert("DBUS_SESSION_BUS_ADDRESS".into(), line);
    Ok(Managed(process))
}
fn outer(opt: &Options) -> Result<()> {
    fs::create_dir(&opt.output)?;
    for path in [&opt.launcher, &opt.sway, &opt.pointer] {
        ensure(
            path.canonicalize()?.starts_with("/nix/store/"),
            format!("Unpinned tool: {}", path.display()),
        )?;
    }
    let repo = env::current_dir()?;
    let git = Command::new("git")
        .args(["-C", &path_string(&repo), "rev-parse", "HEAD"])
        .output()?;
    ensure(git.status.success(), "Source revision unavailable")?;
    let revision = String::from_utf8(git.stdout)?.trim().to_owned();
    let parent_net = path_string(&fs::read_link("/proc/self/ns/net")?);
    let boundary = env::var("PATH")?
        .split(':')
        .map(PathBuf::from)
        .map(|dir| dir.join("lumen-application-isolation"))
        .find(|candidate| candidate.is_file())
        .ok_or("Pinned application isolation launcher missing")?;
    let mut command = Command::new(boundary);
    command.args([
        "run",
        "--source",
        &path_string(&repo),
        "--output",
        &path_string(&opt.output),
    ]);
    if opt.effects {
        command.arg("--software-graphics");
    }
    command.arg("--").arg(env::current_exe()?).args([
        "--inside",
        "--launcher",
        &path_string(&opt.launcher),
        "--sway",
        &path_string(&opt.sway),
        "--pointer-tool",
        &path_string(&opt.pointer),
        "--output",
        "/artifacts",
        "--source-revision",
        &revision,
        "--parent-net",
        &parent_net,
    ]);
    if opt.effects {
        let path = opt
            .egl
            .clone()
            .or_else(|| env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").map(PathBuf::from))
            .ok_or("Mesa vendor missing")?;
        let path = immutable_path(&path, "Mesa vendor")?;
        command.args(["--effects", "--egl-vendor", &path_string(&path)]);
    }
    let status = command.status()?;
    ensure(
        status.success(),
        format!("Settings search fixture exited {status}"),
    )
}
fn nodes<'a>(node: &'a Value, output: &mut Vec<&'a Value>) {
    output.push(node);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                nodes(child, output);
            }
        }
    }
}
struct Driver {
    vars: BTreeMap<String, String>,
    out: PathBuf,
    qml: PathBuf,
    shell_pid: u32,
}
impl Driver {
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        run_command(program, args, &self.vars)
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut all = vec![
            "-p",
            self.qml.to_str().ok_or("QML path not UTF-8")?,
            "ipc",
            "call",
            "settingsAudit",
            method,
        ];
        all.extend(args);
        self.call("quickshell", &all)
    }
    fn state(&self) -> Result<Value> {
        let value: Value = serde_json::from_str(&self.ipc("state", &[])?)?;
        save(&self.out.join("last-state.json"), &value)?;
        Ok(value)
    }
    fn owner(&self) -> Result<Option<Value>> {
        let tree: Value = serde_json::from_str(&self.call("swaymsg", &["-r", "-t", "get_tree"])?)?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        let owners: Vec<_> = all
            .into_iter()
            .filter(|n| {
                n["pid"] == self.shell_pid
                    && n["app_id"] == "org.quickshell"
                    && n["name"] == "System Settings"
                    && n["window_rect"]["width"].as_i64().unwrap_or(0) > 0
                    && n["window_rect"]["height"].as_i64().unwrap_or(0) > 0
            })
            .collect();
        Ok(if owners.len() == 1 {
            Some(owners[0].clone())
        } else {
            None
        })
    }
    fn focused(&self) -> Result<bool> {
        Ok(self.owner()?.as_ref().is_some_and(|n| n["focused"] == true))
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.call("wtype", args)?;
        Ok(())
    }
    fn chord(&self, key: &str, modifier: &str) -> Result<()> {
        self.key(&["-M", modifier, "-k", key, "-m", modifier])
    }
    fn query(&self, text: &str) -> Result<()> {
        self.chord("f", "ctrl")?;
        wait("physical Ctrl+F search focus", || {
            Ok(self.state()?["searchGeometry"]["focus"]
                .as_bool()
                .filter(|v| *v))
        })?;
        self.chord("a", "ctrl")?;
        self.key(&[text])?;
        wait("actual typed query", || {
            Ok((self.state()?["search"] == text).then_some(()))
        })?;
        Ok(())
    }
    fn point_result(&self, pointer: &mut Managed, con_id: i64) -> Result<()> {
        let (owner, _) = wait("exact client configure/focus", || {
            let Some(owner) = self.owner()? else {
                return Ok(None);
            };
            let state = self.state()?;
            Ok((owner["id"] == con_id
                && owner["focused"] == true
                && owner["window_rect"]["width"] == state["windowWidth"]
                && owner["window_rect"]["height"] == state["windowHeight"])
                .then_some((owner, state)))
        })?;
        let rect: Value = serde_json::from_str(&self.ipc("result", &["dock-size"])?)?;
        ensure(
            rect["w"].as_f64().unwrap_or(0.) > 0. && rect["h"].as_f64().unwrap_or(0.) > 0.,
            "Dock result geometry invalid",
        )?;
        let number = |v: &Value| v.as_f64().unwrap_or(0.);
        let x = (number(&owner["rect"]["x"])
            + number(&owner["window_rect"]["x"])
            + number(&rect["x"])
            + number(&rect["w"]) / 2.)
            .round();
        let y = (number(&owner["rect"]["y"])
            + number(&owner["window_rect"]["y"])
            + number(&rect["y"])
            + number(&rect["h"]) / 2.)
            .round();
        self.call(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &format!("{x:.0}"),
                &format!("{y:.0}"),
            ],
        )?;
        pointer.write("m 0.5 0.5\n")?;
        wait("physical result hover", || {
            let rect: Value = serde_json::from_str(&self.ipc("result", &["dock-size"])?)?;
            Ok((rect["hovered"] == true).then_some(()))
        })?;
        Ok(())
    }
}
fn inner(opt: &Options) -> Result<()> {
    ensure(unsafe { libc::getuid() } != 0, "Fixture ran as root")?;
    ensure(
        path_string(&fs::read_link("/proc/self/ns/net")?) != opt.parent_net,
        "Host network namespace visible",
    )?;
    for name in ["dri", "snd", "video0"] {
        ensure(
            !Path::new("/dev").join(name).exists(),
            "Host device visible",
        )?;
    }
    ensure(
        Path::new("/dev/udmabuf").exists() == opt.effects,
        "Unexpected udmabuf mapping",
    )?;
    let out = PathBuf::from("/artifacts");
    let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    for (key, value) in launcher_env(&opt.launcher)? {
        vars.insert(key, value);
    }
    for (key, value) in [
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_NEARBY_SHARING_PROVIDER", ""),
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if opt.effects { "gles2" } else { "pixman" }),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
    ] {
        vars.insert(key.into(), value.into());
    }
    if opt.effects {
        vars.insert(
            "__EGL_VENDOR_LIBRARY_FILENAMES".into(),
            path_string(opt.egl.as_ref().ok_or("Mesa vendor missing")?),
        );
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
    let _bus = private_bus(&mut vars, &out, &runtime)?;
    let assets = out.join("assets");
    copy_tree(
        Path::new(
            vars.get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Installed assets missing")?,
        ),
        &assets,
    )?;
    for name in ["SettingsWindow.qml", "SettingsSearch.js"] {
        ensure(
            fs::read(assets.join("shell").join(name))?
                == fs::read(Path::new("/source/res/lumen-desktop/shell").join(name))?,
            format!("Installed source mismatch: {name}"),
        )?;
    }
    let template = Path::new("/source/tests/desktop/fixtures/settings_search_window.qml");
    let qml = out.join("shell.qml");
    fs::copy(template, &qml)?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = runtime.join("appearance.json");
    save(
        &appearance,
        &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{"reduceMotion":true}}),
    )?;
    vars.insert("LUMEN_APPEARANCE_STATE".into(), path_string(&appearance));
    let header_rule = fs::read_to_string("/source/res/lumen-desktop/sway.conf")?
        .lines()
        .find(|line| {
            line.starts_with("for_window [app_id=")
                && line.contains("System Settings")
                && line.ends_with("border pixel 0")
        })
        .ok_or("Production settings header rule missing")?
        .to_owned();
    let config = out.join("sway.conf");
    fs::write(
        &config,
        format!(
            "output HEADLESS-1 resolution 1280x900\nseat seat0 fallback true\nfocus_follows_mouse no\nfor_window [all] floating enable\n{header_rule}\n"
        ),
    )?;
    let _sway = Managed::launch(
        opt.sway.to_str().ok_or("Compositor path")?,
        &["-d", "--unsupported-gpu", "-c", &path_string(&config)],
        &vars,
        &out.join("sway.log"),
        false,
    )?;
    vars.insert(
        "SWAYSOCK".into(),
        path_string(&wait("private compositor", || {
            first(&runtime, "sway-ipc.", ".lock")
        })?),
    );
    vars.insert(
        "WAYLAND_DISPLAY".into(),
        wait("private display", || {
            Ok(first(&runtime, "wayland-", ".lock")?
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned()))
        })?,
    );
    let version: Value = serde_json::from_str(&run_command(
        "swaymsg",
        &["-r", "-t", "get_version"],
        &vars,
    )?)?;
    for (key, capability) in [
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "native_window_controls"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "native_window_actions"),
        ("LUMEN_COMPOSITOR_EFFECTS", "compositor_effects"),
    ] {
        vars.insert(
            key.into(),
            if version[capability] == true {
                "1"
            } else {
                "0"
            }
            .into(),
        );
    }
    let _keyboard = Managed::launch(
        "wtype",
        &["-s", "60000"],
        &vars,
        &out.join("keyboard.log"),
        false,
    )?;
    let mut pointer = Managed::launch(
        opt.pointer.to_str().ok_or("Pointer path")?,
        &["pointer", "move", "0", "0"],
        &vars,
        &out.join("pointer.log"),
        true,
    )?;
    pointer.write("m 0 0\n")?;
    let shell = Managed::launch(
        "quickshell",
        &["-p", &path_string(&qml)],
        &vars,
        &out.join("shell.log"),
        false,
    )?;
    let driver = Driver {
        vars,
        out: out.clone(),
        qml,
        shell_pid: shell.0.id(),
    };
    let mut checks = Vec::new();
    let owner = wait("captured settings owner map", || driver.owner())?;
    let mut con_id = owner["id"].as_i64().ok_or("Owner id missing")?;
    driver.call("swaymsg", &[&format!("[con_id={con_id}]"), "focus"])?;
    wait("captured owner focused", || {
        Ok(driver.focused()?.then_some(()))
    })?;
    wait("production settings active", || {
        Ok((driver.state()?["visible"] == true && driver.state()?["active"] == true).then_some(()))
    })?;
    ensure(
        driver.state()?["zoomChoices"] == json!(["Fill", "Zoom", "Minimize", "Do nothing"]),
        "Installed zoom choices differ",
    )?;
    checks.push(
        "Installed production settings retains Zoom choice and all existing titlebar alternatives",
    );
    driver.query("Title bar double-click action")?;
    driver.key(&["-k", "Return"])?;
    wait("exact titlebar result focus", || {
        let v = driver.state()?;
        Ok(
            (v["control"] == "title-bar-double-click-action" && v["selected"]["focus"] == true)
                .then_some(()),
        )
    })?;
    checks.push(
        "Physical Ctrl+F and typed exact visible label select, focus and reveal the exact control",
    );
    let selected = driver.state()?;
    ensure(
        selected["page"] == "Desktop & Dock"
            && selected["selected"]["y"].as_f64().unwrap_or(-1.) >= 0.
            && selected["selected"]["y"].as_f64().unwrap_or(0.)
                + selected["selected"]["h"].as_f64().unwrap_or(0.)
                <= selected["windowHeight"].as_f64().unwrap_or(0.),
        "Titlebar result not visible",
    )?;
    driver.call(
        env!("LUMEN_SETTINGS_SEARCH_GRIM"),
        &[&path_string(&out.join("titlebar-result.png"))],
    )?;
    driver.key(&["-k", "space", "-k", "Down", "-k", "Return"])?;
    wait("physical Zoom preference", || {
        Ok((driver.state()?["windowCalls"]
            == json!([{"key":"titlebarDoubleClick","value":"zoom"}]))
        .then_some(()))
    })?;
    checks.push(
        "Physical titlebar combo choice submits only canonical zoom to synthetic preference owner",
    );
    driver.query("Reduce")?;
    driver.key(&["-k", "Down"])?;
    wait("second result", || {
        Ok((driver.state()?["searchIndex"] == 1).then_some(()))
    })?;
    ensure(
        driver.state()?["results"][1]["control"] == "reduce-motion",
        "Wrong second result",
    )?;
    driver.key(&["-k", "Return"])?;
    wait("selected second control", || {
        let v = driver.state()?;
        Ok((v["control"] == "reduce-motion" && v["selected"]["focus"] == true).then_some(()))
    })?;
    checks
        .push("Down and Enter select the captured second control rather than first category match");
    driver.chord("Left", "alt")?;
    wait("Back exact control", || {
        let v = driver.state()?;
        Ok(
            (v["control"] == "title-bar-double-click-action" && v["selected"]["focus"] == true)
                .then_some(()),
        )
    })?;
    checks.push("Alt+Left restores exact control and focus across categories");
    driver.chord("w", "ctrl")?;
    wait("window close", || {
        Ok((driver.state()?["visible"] == false).then_some(()))
    })?;
    driver.ipc("reopen", &[])?;
    wait("reopen preserved control", || {
        let v = driver.state()?;
        Ok((v["visible"] == true
            && v["control"] == "title-bar-double-click-action"
            && v["selected"]["focus"] == true)
            .then_some(()))
    })?;
    con_id = wait("new mapped generation", || {
        Ok(driver
            .owner()?
            .filter(|n| n["focused"] == true)
            .and_then(|n| n["id"].as_i64()))
    })?;
    checks.push("Close/reopen preserves selected control and usable navigation history");
    driver.query("No such setting label")?;
    driver.key(&["-k", "Return"])?;
    ensure(
        driver.state()?["control"] == "title-bar-double-click-action",
        "Unmatched query retargeted control",
    )?;
    checks.push("Unmatched physical search cannot retarget the current page/control");
    driver.query("Dock size")?;
    driver.point_result(&mut pointer, con_id)?;
    pointer.write("p\n")?;
    driver.query("Reduce motion")?;
    pointer.write("r\n")?;
    ensure(
        driver.state()?["control"] == "title-bar-double-click-action",
        "Held press retargeted control",
    )?;
    checks.push("Held physical result press cannot activate a replacement query result on release");
    driver.query("Dock size")?;
    driver.point_result(&mut pointer, con_id)?;
    pointer.write("p\nr\n")?;
    wait("fresh physical click", || {
        let v = driver.state()?;
        Ok((v["control"] == "dock-size" && v["selected"]["focus"] == true).then_some(()))
    })?;
    checks.push("Fresh pointer press/release selects the intended indexed control");
    let final_state = driver.state()?;
    ensure(
        final_state["writes"] == 1
            && final_state["windowCalls"] == json!([{"key":"titlebarDoubleClick","value":"zoom"}]),
        "Search/navigation changed another preference",
    )?;
    driver.call(
        env!("LUMEN_SETTINGS_SEARCH_GRIM"),
        &[&path_string(&out.join("final.png"))],
    )?;
    if opt.effects {
        let log = fs::read_to_string(out.join("sway.log"))?;
        for marker in [
            "Creating scenefx FX renderer",
            "GL renderer: llvmpipe",
            "FX RENDERER: Shaders Initialized Successfully",
            "Trying udmabuf allocator",
            "Created GL FBO for buffer",
        ] {
            ensure(
                log.contains(marker),
                format!("Effects renderer marker missing: {marker}"),
            )?;
        }
    }
    save(
        &out.join("result.json"),
        &json!({"passed":true,"checks":checks,"effects":opt.effects,
        "sourceRevision":opt.revision,"fixtureSha256":format!("{:x}", Sha256::digest(include_bytes!("main.rs"))),"templateSha256":digest(template)?,
        "productionHeaderRule":header_rule,"packages":{"launcher":opt.launcher,"compositor":opt.sway,"pointer":opt.pointer},
        "scope":"Actual installed owned settings; physical Wayland keyboard/pointer; synthetic preference service; no host writes/provider rows/reference metrics claim"}),
    )?;
    println!("PASS settings search {}", checks.len());
    Ok(())
}
fn main() -> Result<()> {
    let opt = options()?;
    if opt.inside { inner(&opt) } else { outer(&opt) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artifact_symlink_is_replaced_by_namespace_visible_store_path() {
        let root = env::temp_dir().join(format!("lumen-settings-path-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let link = root.join("artifact-tool");
        let executable = Path::new(env!("LUMEN_SETTINGS_SEARCH_GRIM"));
        symlink(executable, &link).unwrap();
        let result = immutable_path(&link, "test tool").unwrap();
        assert_eq!(result, executable.canonicalize().unwrap());
        assert!(result.starts_with("/nix/store"));
        assert_ne!(result, link);
        let local = root.join("local-tool");
        fs::write(&local, "untrusted").unwrap();
        assert!(
            immutable_path(&local, "test tool")
                .unwrap_err()
                .to_string()
                .contains("Unpinned")
        );
        assert!(
            immutable_path(&root.join("absent"), "missing launcher")
                .unwrap_err()
                .to_string()
                .contains("Resolve missing launcher")
        );
        fs::remove_file(link).unwrap();
        fs::remove_file(local).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
