//! Shared real compositor, D-Bus, keyboard and pointer harness for group fixtures.
mod desktop_card;
mod desktop_fullscreen;
mod desktop_transfer;
mod membership;
mod presentation;
mod selector;
mod window_groups;

use serde_json::{Value, json};
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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn ensure(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Group fixture timeout: {label}: {last}")));
        }
        pause(40);
    }
}
fn truth(value: &Value) -> bool {
    value == true
}
fn integer(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| fail(format!("Expected integer: {value}")))
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
            let mut p = fs::metadata(&target)?.permissions();
            p.set_mode(p.mode() | 0o200);
            fs::set_permissions(&target, p)?;
        } else {
            return Err(fail(format!("Unsupported asset {}", source.display())));
        }
    }
    Ok(())
}
fn launcher_env(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    ensure(
        output.status.success(),
        "Installed launcher environment unavailable",
    )?;
    let mut vars = BTreeMap::new();
    for part in output.stdout.split(|b| *b == 0) {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    Ok(vars)
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        // Quickshell's Unix IPC socket has a strict path-length limit.
        let path = PathBuf::from("/tmp").join(format!(
            "lng-{}-{}",
            std::process::id(),
            stamp % 1_000_000_000
        ));
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
                pause(30);
            }
        }
    }
}
struct Options {
    scenario: String,
    effects: bool,
    launcher: PathBuf,
    sway: PathBuf,
    client: PathBuf,
    pointer: PathBuf,
    locker: PathBuf,
    output: PathBuf,
}
fn options() -> Result<Options> {
    let mut scenario = None;
    let mut effects = false;
    let mut launcher = None;
    let mut sway = None;
    let mut client = None;
    let mut pointer = None;
    let mut locker = None;
    let mut output = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some(
                "selector"
                | "membership"
                | "window-groups"
                | "presentation"
                | "desktop-transfer"
                | "desktop-transfer-unsupported",
            ) => scenario = Some(arg.to_string_lossy().into_owned()),
            Some("--effects") => effects = true,
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing compositor")?)),
            Some("--client") => client = Some(PathBuf::from(args.next().ok_or("Missing client")?)),
            Some("--keyboard") => {
                let _ = args.next().ok_or("Missing keyboard")?;
            }
            Some("--pointer-tool") => {
                pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
            }
            Some("--locker") => locker = Some(PathBuf::from(args.next().ok_or("Missing locker")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            _ => return Err(fail(format!("Unknown option {}", arg.to_string_lossy()))),
        }
    }
    let output = output.ok_or("Missing output")?;
    let output = if output.is_absolute() {
        output
    } else {
        env::current_dir()?.join(output)
    };
    Ok(Options {
        scenario: scenario.ok_or("Select selector, membership or window-groups")?,
        effects,
        launcher: launcher.ok_or("Missing launcher")?,
        sway: sway.ok_or("Missing compositor")?,
        client: client.ok_or("Missing client")?,
        pointer: pointer.ok_or("Missing pointer")?,
        locker: locker.ok_or("Missing locker")?,
        output,
    })
}
struct Context {
    opt: Options,
    root: PrivateRoot,
    vars: BTreeMap<String, String>,
    swaymsg: PathBuf,
    qml: PathBuf,
    procs: Vec<Managed>,
    captured: Vec<Value>,
    shell_pid: u32,
    pointer: Option<usize>,
    held: bool,
    position: (f64, f64),
}
impl Drop for Context {
    fn drop(&mut self) {
        // Retire clients before compositor/bus, and every child before its files.
        while let Some(child) = self.procs.pop() {
            drop(child);
        }
    }
}
impl Context {
    fn call(&self, program: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .output()?;
        ensure(
            output.status.success(),
            format!(
                "{} {args:?} failed: {}",
                program.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }
    fn named(&self, program: &str, args: &[&str]) -> Result<String> {
        self.call(Path::new(program), args)
    }
    fn launch(&mut self, program: &Path, args: &[&str], log: &str, stdin: bool) -> Result<usize> {
        let file = File::create(self.opt.output.join(log))?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .process_group(0)
            .stdin(if stdin { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file))
            .spawn()?;
        self.procs.push(Managed(child));
        Ok(self.procs.len() - 1)
    }
    fn write(&mut self, index: usize, data: &str) -> Result<()> {
        let input = self.procs[index]
            .0
            .stdin
            .as_mut()
            .ok_or("Process stdin unavailable")?;
        input.write_all(data.as_bytes())?;
        input.flush()?;
        Ok(())
    }
    fn sway(&self, command: &str, success: bool) -> Result<()> {
        let output = Command::new(&self.swaymsg)
            .args(["-r", command])
            .env_clear()
            .envs(&self.vars)
            .output()?;
        if success {
            ensure(
                output.status.success(),
                format!("Sway IPC transport failed: {command}"),
            )?;
        }
        let rows: Value = serde_json::from_slice(&output.stdout)?;
        ensure(
            rows.as_array().is_some_and(|list| {
                !list.is_empty() && list.iter().all(|r| truth(&r["success"]) == success)
            }),
            format!("Sway IPC result for {command}: {rows}"),
        )
    }
    fn native(&self, kind: &str) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call(&self.swaymsg, &["-r", "-t", kind])?,
        )?)
    }
    fn groups(&self) -> Result<Value> {
        Ok(self.native("get_tree")?["window_groups"].clone())
    }
    fn group_action(&self, action: &str) -> Result<()> {
        self.sway(
            &format!(
                "lumen_window groups {} {action}",
                self.groups()?["revision"]
            ),
            true,
        )
    }
    fn window(&self, n: usize) -> Result<Option<Value>> {
        let tree = self.native("get_tree")?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok(all
            .into_iter()
            .find(|row| row["name"] == format!("Relationship {n}"))
            .cloned())
    }
    fn group(&self, n: usize) -> Result<Value> {
        let id = self.captured[n]["id"].clone();
        self.groups()?["groups"]
            .as_array()
            .ok_or("Groups absent")?
            .iter()
            .find(|group| {
                group["members"]
                    .as_array()
                    .is_some_and(|members| members.iter().any(|member| member["id"] == id))
            })
            .cloned()
            .ok_or_else(|| fail(format!("Window {n} has no group")))
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut all = vec![
            "-p",
            self.qml.to_str().ok_or("QML path not UTF-8")?,
            "ipc",
            "call",
            "groupAudit",
            method,
        ];
        all.extend(args);
        self.named("quickshell", &all)
    }
    fn ui(&self) -> Result<Value> {
        let value: Value = serde_json::from_str(&self.ipc("status", &[])?)?;
        save(&self.opt.output.join("last-ui.json"), &value)?;
        Ok(value)
    }
    fn surface(&self, output: &str) -> Result<Option<Value>> {
        Ok(self.ui()?["surfaces"]
            .as_array()
            .ok_or("Surfaces missing")?
            .iter()
            .find(|s| s["output"] == output)
            .cloned())
    }
    fn pointer_start(&mut self) -> Result<()> {
        let tool = self.opt.pointer.clone();
        let index = self.launch(&tool, &["pointer", "move", "0", "0"], "pointer.log", true)?;
        self.pointer = Some(index);
        Ok(())
    }
    fn pointer_event(&mut self, command: &str) -> Result<()> {
        self.write(
            self.pointer.ok_or("Pointer not started")?,
            &format!("{command}\n"),
        )?;
        pause(70);
        Ok(())
    }
    fn move_pointer(&mut self, x: f64, y: f64) -> Result<()> {
        if self.held {
            self.pointer_event(&format!(
                "m {} {}",
                x - self.position.0,
                y - self.position.1
            ))?;
        } else {
            self.sway(
                &format!("seat seat0 cursor set {} {}", x as i64, y as i64),
                true,
            )?;
            self.pointer_event("m 0.5 0.5")?;
        }
        self.position = (x, y);
        Ok(())
    }
    fn press(&mut self) -> Result<()> {
        self.pointer_event("p")?;
        self.held = true;
        Ok(())
    }
    fn release(&mut self) -> Result<()> {
        self.pointer_event("r")?;
        self.held = false;
        Ok(())
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.named("wtype", args)?;
        Ok(())
    }
    fn picture(&self, name: &str) -> Result<()> {
        self.named("grim", &[&text(&self.opt.output.join(name))])?;
        Ok(())
    }
    fn result(&self, checks: &[&str]) -> Result<()> {
        save(
            &self.opt.output.join("result.json"),
            &json!({"passed":true,"checks":checks}),
        )
    }
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
fn setup(opt: Options) -> Result<Context> {
    fs::create_dir(&opt.output)?;
    let repo = env::current_dir()?;
    let root = PrivateRoot::new()?;
    let runtime = root.0.join("runtime");
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let mut vars = launcher_env(&opt.launcher)?;
    for name in [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_STATE_HOME",
        "XDG_DATA_HOME",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
    ] {
        let path = if name == "HOME" {
            root.0.join("home")
        } else if name == "XDG_RUNTIME_DIR" {
            runtime.clone()
        } else {
            root.0.join(name)
        };
        fs::create_dir_all(&path)?;
        vars.insert(name.into(), text(&path));
    }
    for name in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(name);
    }
    for (name, value) in [
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
    ] {
        vars.insert(name.into(), value.into());
    }
    let swaymsg = opt
        .sway
        .parent()
        .ok_or("Compositor bin directory missing")?
        .join("swaymsg");
    let qml = root.0.join("shell.qml");
    let mut ctx = Context {
        opt,
        root,
        vars,
        swaymsg,
        qml,
        procs: Vec::new(),
        captured: Vec::new(),
        shell_pid: 0,
        pointer: None,
        held: false,
        position: (0., 0.),
    };
    let busconf = ctx.root.0.join("bus.conf");
    fs::write(
        &busconf,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            runtime.display()
        ),
    )?;
    let log = File::create(ctx.opt.output.join("bus.log"))?;
    let mut bus = Command::new("dbus-daemon")
        .args([
            "--nofork",
            "--print-address=1",
            &format!("--config-file={}", busconf.display()),
        ])
        .env_clear()
        .envs(&ctx.vars)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log))
        .process_group(0)
        .spawn()?;
    let address = BufReader::new(bus.stdout.take().ok_or("Private bus stdout missing")?)
        .lines()
        .next()
        .ok_or("Private bus address missing")??;
    ensure(
        address.starts_with("unix:"),
        "Private bus did not publish an address",
    )?;
    ctx.procs.push(Managed(bus));
    ctx.vars.insert("DBUS_SESSION_BUS_ADDRESS".into(), address);
    let shortcut = format!(
        "quickshell -p {} ipc call groupAudit beginFocused",
        ctx.qml.display()
    );
    let config = ctx.root.0.join("sway.conf");
    fs::write(
        &config,
        format!(
            "output HEADLESS-1 resolution 1200x800 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1200 0\nseat seat0 fallback true\nfocus_follows_mouse no\nxwayland disable\nfor_window [all] floating enable, border pixel 0\nbindsym Mod4+Control+g exec {shortcut}\n"
        ),
    )?;
    let sway = ctx.opt.sway.clone();
    let config_path = text(&config);
    let mut compositor_args = vec!["--unsupported-gpu", "-c", &config_path];
    if matches!(
        ctx.opt.scenario.as_str(),
        "presentation" | "desktop-transfer" | "desktop-transfer-unsupported"
    ) {
        compositor_args.push("-d");
    }
    ctx.launch(&sway, &compositor_args, "sway.log", false)?;
    ctx.vars.insert(
        "SWAYSOCK".into(),
        text(&wait("private compositor socket", || {
            first(&runtime, "sway-ipc", ".lock")
        })?),
    );
    ctx.vars.insert(
        "WAYLAND_DISPLAY".into(),
        wait("private Wayland display", || {
            Ok(first(&runtime, "wayland-", ".lock")?
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned()))
        })?,
    );
    ctx.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log", false)?;
    ctx.sway(
        "focus output HEADLESS-1; workspace 1; move workspace to output HEADLESS-1",
        true,
    )?;
    let client = ctx.opt.client.clone();
    ctx.launch(&client, &[], "client.log", true)?;
    ctx.captured = wait("four native relationship windows", || {
        let windows = (0..4).map(|n| ctx.window(n)).collect::<Result<Vec<_>>>()?;
        Ok(windows.into_iter().collect::<Option<Vec<_>>>())
    })?;
    ctx.sway(
        &format!(
            "[con_id={}] move container to workspace 3",
            ctx.captured[3]["id"]
        ),
        true,
    )?;
    ctx.sway("workspace 3; move workspace to output HEADLESS-2", true)?;
    ctx.sway("focus output HEADLESS-1; workspace 1", true)?;
    let appearance = Path::new(
        ctx.vars
            .get("XDG_STATE_HOME")
            .ok_or("Private state missing")?,
    )
    .join("lumen-desktop");
    fs::create_dir(&appearance)?;
    save(
        &appearance.join("appearance.json"),
        &json!({"font":"sans-serif","palette":{"background":"#242426","surface":"#303033","field":"#363639","text":"#eeeeef","muted":"#aaaab0","border":"#505054","hover":"#414145","accent":"#007aff","selectedText":"#ffffff"},"preferences":{"mode":"dark","reduceMotion":true}}),
    )?;
    let assets = if matches!(
        ctx.opt.scenario.as_str(),
        "presentation" | "desktop-transfer" | "desktop-transfer-unsupported"
    ) {
        PathBuf::from(
            ctx.vars
                .get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Installed desktop assets required")?,
        )
    } else {
        repo.join("res/lumen-desktop")
    };
    copy_tree(&assets, &ctx.root.0.join("assets"))?;
    let qml = if ctx.opt.scenario == "selector" {
        include_str!("../selector.qml")
    } else if ctx.opt.scenario.starts_with("desktop-transfer") {
        include_str!("../desktop-transfer.qml")
    } else if ctx.opt.scenario == "presentation" {
        include_str!("../presentation.qml")
    } else {
        include_str!("../membership.qml")
    };
    fs::write(&ctx.qml, qml)?;
    let qml_path = ctx.qml.clone();
    let shell = ctx.launch(
        Path::new("quickshell"),
        &["-p", &text(&qml_path)],
        if ctx.opt.scenario == "selector" {
            "selector.log"
        } else {
            "membership.log"
        },
        false,
    )?;
    ctx.shell_pid = ctx.procs[shell].0.id();
    Ok(ctx)
}
fn main() -> Result<()> {
    let opt = options()?;
    let scenario = opt.scenario.clone();
    let mut ctx = if scenario == "window-groups" {
        window_groups::setup(opt)?
    } else {
        setup(opt)?
    };
    if scenario == "desktop-transfer-unsupported" {
        desktop_transfer::unsupported(&mut ctx)
    } else if scenario == "desktop-transfer" {
        desktop_transfer::run(&mut ctx)
    } else if scenario == "selector" {
        selector::run(&mut ctx)
    } else if scenario == "presentation" {
        presentation::run(&mut ctx)
    } else if scenario == "window-groups" {
        window_groups::run(&mut ctx)
    } else {
        membership::run(&mut ctx)
    }
}
