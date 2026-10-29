//! Real keyboard switcher acceptance against isolated and installed shells.
mod installed;
mod model;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
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
fn path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => {}
            Err(e) => last = e.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Timed out: {label}: {last}")));
        }
        pause(50);
    }
}
fn first(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|e| e.ok().map(|x| x.path()))
        .find(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with(prefix) && !n.ends_with(suffix)
            })
        }))
}
fn nodes<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    out.push(node);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for c in children {
                nodes(c, out);
            }
        }
    }
}
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let e = entry?;
        let src = e.path();
        let dst = to.join(e.file_name());
        let kind = fs::symlink_metadata(&src)?.file_type();
        if kind.is_symlink() {
            symlink(fs::read_link(&src)?, &dst)?;
        } else if kind.is_dir() {
            copy_tree(&src, &dst)?;
        } else if kind.is_file() {
            fs::copy(&src, &dst)?;
            let mut p = fs::metadata(&dst)?.permissions();
            p.set_mode(p.mode() | 0o200);
            fs::set_permissions(&dst, p)?;
        } else {
            return Err(fail(format!("Unsupported asset {}", src.display())));
        }
    }
    Ok(())
}
fn launcher_env(
    launcher: &Path,
    vars: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher)
        .arg("--print-env0")
        .env_clear()
        .envs(vars)
        .output()?;
    ensure(
        result.status.success(),
        "Installed launcher environment unavailable",
    )?;
    let mut output = BTreeMap::new();
    for part in result.stdout.split(|b| *b == 0) {
        if let Some((k, v)) = std::str::from_utf8(part)?.split_once('=') {
            output.insert(k.into(), v.into());
        }
    }
    Ok(output)
}
fn which(name: &str, vars: &BTreeMap<String, String>) -> Result<PathBuf> {
    let path = vars.get("PATH").ok_or("PATH missing")?;
    for dir in env::split_paths(path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("Program unavailable: {name}")))
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let dir = PathBuf::from("/tmp").join(format!(
            "lsw-{}-{}",
            std::process::id(),
            stamp % 1_000_000_000
        ));
        fs::create_dir(&dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        Ok(Self(dir))
    }
}
impl Drop for PrivateRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Managed(Child);
impl Managed {
    fn stop(&mut self) -> Result<()> {
        if self.0.try_wait()?.is_none() {
            unsafe { libc::killpg(self.0.id() as i32, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(4);
            while self.0.try_wait()?.is_none() {
                if Instant::now() >= deadline {
                    unsafe { libc::killpg(self.0.id() as i32, libc::SIGKILL) };
                    self.0.wait()?;
                    break;
                }
                pause(30);
            }
        }
        Ok(())
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
struct Options {
    scenario: String,
    launcher: Option<PathBuf>,
    action: Option<PathBuf>,
    sway: PathBuf,
    wtype: PathBuf,
    output: PathBuf,
    effects: bool,
}
fn options() -> Result<Options> {
    let mut scenario = None;
    let mut launcher = None;
    let mut action = None;
    let mut sway = None;
    let mut wtype = PathBuf::from("wtype");
    let mut output = None;
    let mut effects = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("model" | "installed") => scenario = Some(arg.to_string_lossy().into_owned()),
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--action") => action = Some(PathBuf::from(args.next().ok_or("Missing action")?)),
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing compositor")?)),
            Some("--wtype") => wtype = PathBuf::from(args.next().ok_or("Missing wtype")?),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--effects") => effects = true,
            _ => {
                return Err(fail(format!(
                    "Unknown switcher fixture option {}",
                    arg.to_string_lossy()
                )));
            }
        }
    }
    let output = output.ok_or("Output required")?;
    let output = if output.is_absolute() {
        output
    } else {
        env::current_dir()?.join(output)
    };
    Ok(Options {
        scenario: scenario.ok_or("Select model or installed")?,
        launcher,
        action,
        sway: sway.ok_or("Compositor required")?,
        wtype,
        output,
        effects,
    })
}
struct Context {
    opt: Options,
    root: PrivateRoot,
    vars: BTreeMap<String, String>,
    swaymsg: PathBuf,
    procs: Vec<Managed>,
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
                "{} {args:?}: {}",
                program.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }
    fn named(&self, name: &str, args: &[&str]) -> Result<String> {
        self.call(Path::new(name), args)
    }
    fn launch(&mut self, program: &Path, args: &[&str], log: &str) -> Result<usize> {
        let file = File::create(self.opt.output.join(log))?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .stdin(Stdio::null())
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file))
            .process_group(0)
            .spawn()?;
        self.procs.push(Managed(child));
        Ok(self.procs.len() - 1)
    }
    fn sway(&self, args: &[&str]) -> Result<Value> {
        Ok(serde_json::from_str(&self.call(&self.swaymsg, args)?)?)
    }
    fn tree(&self) -> Result<Value> {
        self.sway(&["-r", "-t", "get_tree"])
    }
    fn windows(&self) -> Result<Vec<Value>> {
        let tree = self.tree()?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok(all
            .into_iter()
            .filter(|n| n["app_id"].as_str().is_some())
            .cloned()
            .collect())
    }
    fn focus(&self, title: &str) -> Result<()> {
        let node = self
            .find(title)?
            .ok_or_else(|| fail(format!("Window {title} missing")))?;
        self.named("swaymsg", &[&format!("[con_id={}]", node["id"]), "focus"])?;
        Ok(())
    }
    fn find(&self, title: &str) -> Result<Option<Value>> {
        Ok(self.windows()?.into_iter().find(|n| n["name"] == title))
    }
    fn keys(&self, args: &[&str]) -> Result<()> {
        self.call(&self.opt.wtype, args)?;
        Ok(())
    }
}
fn setup(opt: Options) -> Result<Context> {
    fs::create_dir(&opt.output)?;
    let root = PrivateRoot::new()?;
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    vars.insert("HOME".into(), path(&root.0));
    vars.insert("XDG_RUNTIME_DIR".into(), path(&root.0));
    for name in ["CONFIG", "STATE", "CACHE", "DATA"] {
        let dir = root.0.join(name.to_lowercase());
        fs::create_dir(&dir)?;
        vars.insert(format!("XDG_{name}_HOME"), path(&dir));
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
    vars.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}", root.0.join("no-system-bus").display()),
    );
    if opt.scenario == "installed" {
        let launcher = opt.launcher.as_ref().ok_or("Installed launcher required")?;
        for (k, v) in launcher_env(launcher, &vars)? {
            vars.insert(k, v);
        }
    }
    let swaymsg = opt
        .sway
        .parent()
        .ok_or("Compositor bin missing")?
        .join("swaymsg");
    let mut ctx = Context {
        opt,
        root,
        vars,
        swaymsg,
        procs: Vec::new(),
    };
    let conf = ctx.root.0.join("bus.conf");
    fs::write(
        &conf,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            ctx.root.0.display()
        ),
    )?;
    let log = File::create(ctx.opt.output.join("bus.log"))?;
    let mut bus = Command::new("dbus-daemon")
        .args([
            "--nofork",
            "--print-address=1",
            &format!("--config-file={}", conf.display()),
        ])
        .env_clear()
        .envs(&ctx.vars)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log))
        .process_group(0)
        .spawn()?;
    let address = BufReader::new(bus.stdout.take().ok_or("Bus stdout missing")?)
        .lines()
        .next()
        .ok_or("Bus address missing")??;
    ensure(address.starts_with("unix:"), "Private bus address invalid")?;
    ctx.procs.push(Managed(bus));
    ctx.vars.insert("DBUS_SESSION_BUS_ADDRESS".into(), address);
    Ok(ctx)
}
fn main() -> Result<()> {
    let opt = options()?;
    let scenario = opt.scenario.clone();
    let mut ctx = setup(opt)?;
    if scenario == "model" {
        model::run(&mut ctx)
    } else {
        installed::run(&mut ctx)
    }
}
