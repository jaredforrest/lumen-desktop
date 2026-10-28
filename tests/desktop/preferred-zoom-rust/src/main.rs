//! Private native preferred-client Zoom, placement restore and titlebar gestures.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(s: impl Into<String>) -> Box<dyn Error> {
    s.into().into()
}
fn ensure(ok: bool, s: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(s)) }
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(Some(v)) = probe() {
            return Ok(v);
        }
        if Instant::now() >= end {
            return Err(fail(format!("Timed out: {label}")));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
fn nodes<'a>(n: &'a Value, out: &mut Vec<&'a Value>) {
    out.push(n);
    for field in ["nodes", "floating_nodes"] {
        if let Some(children) = n[field].as_array() {
            for child in children {
                nodes(child, out)
            }
        }
    }
}
fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn monotonic_ns() -> u128 {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut value);
    }
    value.tv_sec as u128 * 1_000_000_000 + value.tv_nsec as u128
}
fn integer(v: &Value) -> Result<i64> {
    v.as_i64()
        .ok_or_else(|| fail(format!("Expected integer: {v}")))
}
fn first_file(root: &Path, prefix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
        }))
}
#[derive(Clone)]
struct Opt {
    source: PathBuf,
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    egl_vendor: Option<PathBuf>,
    effects: bool,
    rapid: bool,
    inside: bool,
    revision: String,
    parent_net: String,
}
impl Opt {
    fn parse() -> Result<Self> {
        let mut args = env::args_os().skip(1);
        let (mut source, mut output, mut launcher, mut sway, mut pointer, mut egl_vendor) =
            (None, None, None, None, None, None);
        let (mut effects, mut rapid, mut inside) = (false, false, false);
        let (mut revision, mut parent_net) = (String::new(), String::new());
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--source") => {
                    source = Some(PathBuf::from(args.next().ok_or("Missing source")?))
                }
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--pointer") | Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--egl-vendor") => {
                    egl_vendor = Some(PathBuf::from(args.next().ok_or("Missing EGL vendor")?))
                }
                Some("--source-revision") => {
                    revision = args
                        .next()
                        .ok_or("Missing revision")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--parent-net") => {
                    parent_net = args
                        .next()
                        .ok_or("Missing parent net")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--effects") => effects = true,
                Some("--rapid") => rapid = true,
                Some("--inside") => inside = true,
                _ => return Err(fail(format!("Unknown option: {}", arg.to_string_lossy()))),
            }
        }
        Ok(Self {
            source: if inside {
                PathBuf::from("/source")
            } else {
                source.unwrap_or(env::current_dir()?).canonicalize()?
            },
            output: output.ok_or("--output required")?,
            launcher: launcher.ok_or("--launcher required")?,
            sway: sway.ok_or("--sway required")?,
            pointer: pointer.ok_or("--pointer required")?,
            egl_vendor: egl_vendor
                .or_else(|| env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").map(PathBuf::from)),
            effects,
            rapid,
            inside,
            revision,
            parent_net,
        })
    }
}
fn installed(p: &Path) -> Result<PathBuf> {
    let p = p.canonicalize()?;
    ensure(
        p.starts_with("/nix/store") && p.is_file(),
        "Pinned immutable fixture tool required",
    )?;
    Ok(p)
}
fn outer(mut o: Opt) -> Result<()> {
    let output = if o.output.is_absolute() {
        o.output.clone()
    } else {
        env::current_dir()?.join(&o.output)
    };
    fs::create_dir(&output)?;
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
    o.launcher = installed(&o.launcher)?;
    o.sway = installed(&o.sway)?;
    o.pointer = installed(&o.pointer)?;
    if o.effects {
        o.egl_vendor = Some(installed(
            o.egl_vendor
                .as_ref()
                .ok_or("Pinned software EGL vendor required")?,
        )?)
    }
    let rev = Command::new("git")
        .args(["-C", string_path(&o.source)?, "rev-parse", "HEAD"])
        .output()?;
    ensure(rev.status.success(), "Git revision unavailable")?;
    let revision = String::from_utf8(rev.stdout)?.trim().to_owned();
    let exe = env::current_exe()?;
    let child_exe = if exe.starts_with(&o.source) {
        Path::new("/source").join(exe.strip_prefix(&o.source)?)
    } else {
        exe
    };
    let boundary = env::split_paths(&env::var_os("PATH").ok_or("PATH missing")?)
        .map(|p| p.join("lumen-application-isolation"))
        .find(|p| p.is_file())
        .ok_or("Isolation launcher unavailable")?;
    let mut cmd = Command::new(boundary);
    cmd.args(["run", "--source"])
        .arg(&o.source)
        .arg("--output")
        .arg(&output);
    if o.effects {
        cmd.arg("--software-graphics");
    }
    cmd.arg("--private-pty")
        .arg("--")
        .arg(child_exe)
        .arg("--inside");
    for (k, v) in [
        ("--source", o.source.display().to_string()),
        ("--launcher", o.launcher.display().to_string()),
        ("--sway", o.sway.display().to_string()),
        ("--pointer", o.pointer.display().to_string()),
        ("--output", "/artifacts/result".into()),
        ("--source-revision", revision),
        (
            "--parent-net",
            fs::read_link("/proc/self/ns/net")?.display().to_string(),
        ),
    ] {
        cmd.arg(k).arg(v);
    }
    if o.rapid {
        cmd.arg("--rapid");
    }
    if o.effects {
        cmd.arg("--effects")
            .arg("--egl-vendor")
            .arg(o.egl_vendor.ok_or("EGL vendor absent")?);
    }
    let status = cmd.status()?;
    ensure(
        status.success(),
        format!("Preferred Zoom fixture failed: {status}"),
    )
}
fn string_path(p: &Path) -> Result<&str> {
    p.to_str().ok_or_else(|| fail("Non-UTF8 path"))
}
struct Managed(Child);
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
                    break;
                }
                thread::sleep(Duration::from_millis(30));
            }
            let _ = self.0.wait();
        }
    }
}
struct Fixture {
    opt: Opt,
    env: BTreeMap<String, String>,
    processes: Vec<Managed>,
    records: Vec<Value>,
    qml: PathBuf,
    pointer_index: Option<usize>,
    owner_index: Option<usize>,
    cid: i64,
    owner_pid: i64,
    identity: String,
    before: Value,
}
impl Fixture {
    fn new(o: Opt) -> Result<Self> {
        ensure(unsafe { libc::getuid() } != 0, "Root isolation identity")?;
        ensure(
            fs::read_link("/proc/self/ns/net")?.display().to_string() != o.parent_net,
            "Network namespace not private",
        )?;
        ensure(!Path::new("/.old-root").exists(), "Old root exposed")?;
        for dev in ["dri", "snd", "video0"] {
            ensure(
                !Path::new("/dev").join(dev).exists(),
                format!("Hardware device exposed: {dev}"),
            )?;
        }
        ensure(
            fs::metadata("/dev/pts/ptmx")?.file_type().is_char_device(),
            "Private PTY missing",
        )?;
        ensure(
            Path::new("/dev/udmabuf").exists() == o.effects,
            "Software allocator mismatch",
        )?;
        let mut ifnames = Vec::new();
        unsafe {
            let ptr = libc::if_nameindex();
            ensure(!ptr.is_null(), "Interface inventory failed")?;
            let mut p = ptr;
            while (*p).if_index != 0 {
                ifnames.push(
                    std::ffi::CStr::from_ptr((*p).if_name)
                        .to_string_lossy()
                        .into_owned(),
                );
                p = p.add(1);
            }
            libc::if_freenameindex(ptr);
        }
        ensure(
            ifnames == ["lo"],
            format!("Network interfaces: {ifnames:?}"),
        )?;
        let out = &o.output;
        fs::create_dir(out)?;
        let home = PathBuf::from("/run/fixture/preferred-zoom-home");
        fs::create_dir_all(&home)?;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        let mut env = env::vars().collect::<BTreeMap<_, _>>();
        env.insert("HOME".into(), home.display().to_string());
        env.insert("XDG_RUNTIME_DIR".into(), home.display().to_string());
        for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
            let path = home.join(key.to_lowercase());
            fs::create_dir_all(&path)?;
            env.insert(format!("XDG_{key}_HOME"), path.display().to_string());
        }
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
        ] {
            env.remove(key);
        }
        let raw = Command::new(&o.launcher)
            .arg("--print-env0")
            .env_clear()
            .envs(&env)
            .output()?;
        ensure(raw.status.success(), "Launcher environment failed")?;
        for row in raw.stdout.split(|b| *b == 0) {
            if let Some(i) = row.iter().position(|b| *b == b'=') {
                env.insert(
                    String::from_utf8(row[..i].to_vec())?,
                    String::from_utf8(row[i + 1..].to_vec())?,
                );
            }
        }
        for (k, v) in [
            ("WLR_BACKENDS", "headless"),
            ("WLR_HEADLESS_OUTPUTS", "2"),
            ("WLR_RENDERER", if o.effects { "gles2" } else { "pixman" }),
            (
                "WLR_RENDERER_FORCE_SOFTWARE",
                if o.effects { "1" } else { "0" },
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
        ] {
            env.insert(k.into(), v.into());
        }
        if o.effects {
            env.insert(
                "__EGL_VENDOR_LIBRARY_FILENAMES".into(),
                o.egl_vendor
                    .as_ref()
                    .ok_or("EGL vendor missing")?
                    .display()
                    .to_string(),
            );
        }
        let qml = out.join("subject.qml");
        fs::write(&qml, include_str!("../subject.qml"))?;
        let config = out.join("sway.conf");
        fs::write(
            &config,
            "font sans 10\noutput HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 800x600 position 1280 0\noutput * bg #29445b solid_color\nfor_window [app_id=\"org.quickshell\"] floating enable\ndefault_floating_border normal 2\n",
        )?;
        Ok(Self {
            opt: o,
            env,
            processes: Vec::new(),
            records: Vec::new(),
            qml,
            pointer_index: None,
            owner_index: None,
            cid: 0,
            owner_pid: 0,
            identity: String::new(),
            before: Value::Null,
        })
    }
    fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut c = Command::new(program);
        c.args(args).env_clear().envs(&self.env);
        c
    }
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let out = self.command(program, args).output()?;
        ensure(
            out.status.success(),
            format!(
                "{program} {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            ),
        )?;
        Ok(String::from_utf8(out.stdout)?.trim().to_owned())
    }
    fn launch(&mut self, program: &str, args: &[&str], log: &str, pipe: bool) -> Result<usize> {
        let file = File::create(self.opt.output.join(log))?;
        let mut c = self.command(program, args);
        c.stdout(file.try_clone()?).stderr(file);
        if pipe {
            c.stdin(Stdio::piped());
        }
        unsafe {
            c.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        self.processes.push(Managed(c.spawn()?));
        Ok(self.processes.len() - 1)
    }
    fn raw(&self, command: &str) -> Result<Value> {
        let out = self.command("swaymsg", &["-r", command]).output()?;
        ensure(
            [Some(0), Some(2)].contains(&out.status.code()),
            format!("Sway IPC failed: {}", String::from_utf8_lossy(&out.stderr)),
        )?;
        Ok(serde_json::from_slice(&out.stdout)?)
    }
    fn ipc(&self, command: &str) -> Result<Value> {
        let result = self.raw(command)?;
        ensure(
            result
                .as_array()
                .is_some_and(|rows| rows.iter().all(|r| r["success"] == true)),
            format!("Sway IPC rejected {command}: {result}"),
        )?;
        Ok(result)
    }
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn find(&self, pred: impl Fn(&Value) -> bool) -> Result<Option<Value>> {
        let tree = self.tree()?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok(all.into_iter().find(|n| pred(n)).cloned())
    }
    fn subject(&self) -> Result<Option<Value>> {
        self.find(|n| n["app_id"] == "org.quickshell" && n["name"] == "Preferred Zoom Fixture")
    }
    fn node(&self) -> Result<Value> {
        self.subject()?.ok_or_else(|| fail("Subject missing"))
    }
    fn invoke(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut cmd = vec![
            "-p",
            string_path(&self.qml)?,
            "ipc",
            "call",
            "zoomFixture",
            method,
        ];
        cmd.extend(args);
        self.call("quickshell", &cmd)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.invoke("state", &[])?)?)
    }
    fn saved(node: &Value) -> Value {
        json!({"rect":node["rect"],"window_rect":node["window_rect"],"deco_rect":node["deco_rect"]})
    }
    fn client(node: &Value) -> (i64, i64) {
        (
            node["window_rect"]["width"].as_i64().unwrap_or(0),
            node["window_rect"]["height"].as_i64().unwrap_or(0),
        )
    }
    fn agreement(&self) -> Result<bool> {
        let state = self.state()?;
        Ok((integer(&state["width"])?, integer(&state["height"])?) == Self::client(&self.node()?))
    }
    fn action(&self, name: &str) -> Result<Value> {
        let result = self.ipc(&format!("{}lumen_window {name}", self.identity))?;
        if name != "minimize" {
            wait(
                &format!("client/native size agreement after {name}"),
                || Ok(self.agreement()?.then_some(())),
            )?;
        }
        Ok(result)
    }
    fn reset(&self) -> Result<Value> {
        if self.node()?["lumen_state"]["filled"] == true {
            self.action("restore")?;
        }
        self.ipc(&format!(
            "{}resize set 460 px 340 px, move absolute position 180 px 180 px",
            self.identity
        ))?;
        wait("reset placement", || {
            let n = self.node()?;
            Ok((n["rect"]["x"] == 180 && n["rect"]["width"] == 460).then_some(()))
        })?;
        Ok(Self::saved(&self.node()?))
    }
    fn record(&mut self, v: Value) {
        self.records.push(v)
    }
    fn start(&mut self) -> Result<()> {
        let bus = self.opt.output.join("bus.conf");
        let address = "unix:path=/run/fixture/preferred-zoom-bus";
        fs::write(
            &bus,
            format!(
                "<busconfig><type>session</type><listen>{address}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>"
            ),
        )?;
        self.env.insert(
            "DBUS_SYSTEM_BUS_ADDRESS".into(),
            "unix:path=/run/fixture/no-system-bus".into(),
        );
        self.launch(
            "dbus-daemon",
            &["--nofork", &format!("--config-file={}", bus.display())],
            "bus.log",
            false,
        )?;
        wait("private bus", || {
            Ok(Path::new("/run/fixture/preferred-zoom-bus")
                .exists()
                .then_some(()))
        })?;
        self.env
            .insert("DBUS_SESSION_BUS_ADDRESS".into(), address.into());
        let sway = self.opt.sway.display().to_string();
        let config = self.opt.output.join("sway.conf");
        self.launch(
            &sway,
            &["-d", "--unsupported-gpu", "-c", string_path(&config)?],
            "compositor.log",
            false,
        )?;
        let runtime = PathBuf::from(self.env.get("XDG_RUNTIME_DIR").ok_or("Runtime absent")?);
        let sock = wait("compositor socket", || first_file(&runtime, "sway-ipc"))?;
        self.env
            .insert("SWAYSOCK".into(), sock.display().to_string());
        let display = wait("Wayland display", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|e| e.ok())
                .find(|e| {
                    e.file_name().to_string_lossy().starts_with("wayland-")
                        && !e.file_name().to_string_lossy().ends_with(".lock")
                })
                .map(|e| e.file_name().to_string_lossy().into_owned()))
        })?;
        self.env.insert("WAYLAND_DISPLAY".into(), display);
        ensure(
            serde_json::from_str::<Value>(&self.call("swaymsg", &["-r", "-t", "get_version"])?)?["native_preferred_zoom"]
                == true,
            "Compositor lacks native preferred Zoom",
        )?;
        self.ipc("lumen_window margins disable")?;
        self.ipc("focus output HEADLESS-1")?;
        let qml = self.qml.display().to_string();
        let owner = self.launch("quickshell", &["-p", &qml], "subject.log", false)?;
        self.owner_index = Some(owner);
        let node = wait("owned subject", || self.subject())?;
        self.cid = integer(&node["id"])?;
        self.owner_pid = self.processes[owner].0.id() as i64;
        ensure(node["pid"] == self.owner_pid, "Subject PID differs")?;
        self.identity = format!(
            "[con_id={} pid={} app_id=\"^org[.]quickshell$\" title=\"^Preferred Zoom Fixture$\"] ",
            self.cid, self.owner_pid
        );
        Ok(())
    }
}
fn main() -> Result<()> {
    let o = Opt::parse()?;
    if !o.inside {
        outer(o)
    } else {
        let mut f = Fixture::new(o)?;
        f.start()?;
        if let Err(error) = run(&mut f) {
            let _ = save(&f.opt.output.join("failed-records.json"), &json!(f.records));
            if let Ok(tree) = f.tree() {
                let _ = save(&f.opt.output.join("failed-tree.json"), &tree);
            }
            return Err(error);
        }
        Ok(())
    }
}
impl Fixture {
    fn register(&self, title: &str) -> Result<Value> {
        self.invoke("register", &[&self.cid.to_string(), title])?;
        wait("captured owner helper response", || {
            let state = self.state()?;
            let response = state["response"].as_str().unwrap_or("");
            Ok((!response.is_empty() && state["busy"] == false)
                .then(|| serde_json::from_str(response))
                .transpose()?)
        })
    }
    fn foreign(&self) -> Result<Option<Value>> {
        self.find(|n| n["app_id"] == "foreign-zoom-fixture")
    }
    fn resize_subject(&self) -> Result<Option<Value>> {
        let pid = self.owner_pid;
        self.find(|n| n["name"] == "Client Resize Fixture" && n["pid"] == pid)
    }
    fn pointer_events(&mut self, events: &[&str]) -> Result<()> {
        let index = self.pointer_index.ok_or("Pointer not started")?;
        let input = self.processes[index]
            .0
            .stdin
            .as_mut()
            .ok_or("Pointer stdin closed")?;
        for event in events {
            writeln!(input, "{event}")?;
        }
        input.flush()?;
        Ok(())
    }
}
fn run(f: &mut Fixture) -> Result<()> {
    let before = f.reset()?;
    f.before = before.clone();
    let failure = f.raw(&format!("{}lumen_window zoom", f.identity))?;
    ensure(
        !failure
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["success"] == true))
            && Fixture::saved(&f.node()?) == before,
        "Unregistered Zoom changed placement",
    )?;
    f.record(
        json!({"case":"unregistered rejects without resize","before":before,"result":failure}),
    );
    f.launch(
        "foot",
        &[
            "-o",
            "resize-by-cells=no",
            "--app-id=foreign-zoom-fixture",
            "sleep",
            "60",
        ],
        "foreign.log",
        false,
    )?;
    let foreign = wait("actual unregistered foreign application", || f.foreign())?;
    let foreign_before = Fixture::saved(&foreign);
    let foreign_identity = format!("[con_id={} pid={}] ", foreign["id"], foreign["pid"]);
    let failure = f.raw(&format!("{foreign_identity}lumen_window zoom"))?;
    let after = f.foreign()?.ok_or("Foreign vanished")?;
    ensure(
        !failure
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["success"] == true))
            && Fixture::saved(&after) == foreign_before
            && after["lumen_state"]["preferred_zoom"] != true,
        "Foreign preferred size inferred",
    )?;
    f.record(json!({"case":"actual foreign app has no inferred preferred size","window":after,"response":failure}));
    f.ipc(&format!("{foreign_identity}lumen_window close"))?;
    wait("foreign disposable app closes", || {
        Ok(f.foreign()?.is_none().then_some(()))
    })?;
    f.ipc(&format!("{}focus", f.identity))?;
    let registration = f.register("Preferred Zoom Fixture")?;
    ensure(
        registration["preferredSize"] == json!({"width":640,"height":440})
            && registration["container"] == f.cid
            && registration["pid"] == f.owner_pid,
        format!("Preferred registration: {registration}"),
    )?;
    f.record(
        json!({"case":"actual same-owner helper preferred registration","response":registration}),
    );
    wait("preferred capability", || {
        Ok((f.node()?["lumen_state"]["preferred_zoom"] == true).then_some(()))
    })?;
    f.action("zoom")?;
    wait("preferred client dimensions", || {
        let n = f.node()?;
        Ok((Fixture::client(&n) == (640, 440) && n["lumen_state"]["zoomed"] == true).then_some(()))
    })?;
    let zoom = Fixture::saved(&f.node()?);
    ensure(zoom != before, "Zoom did not change placement")?;
    f.call(
        "grim",
        &[
            "-o",
            "HEADLESS-1",
            string_path(&f.opt.output.join("preferred-zoom.png"))?,
        ],
    )?;
    f.action("zoom")?;
    wait("second Zoom restores exact original", || {
        Ok((Fixture::saved(&f.node()?) == before).then_some(()))
    })?;
    f.record(
        json!({"case":"registered client size and toggle restore","original":before,"zoom":zoom}),
    );
    f.action("fill")?;
    wait("Fill", || {
        Ok((f.node()?["lumen_state"]["filled"] == true).then_some(()))
    })?;
    let filled = Fixture::saved(&f.node()?);
    ensure(
        filled != zoom && f.node()?["lumen_state"]["zoomed"] != true,
        "Fill retained Zoom",
    )?;
    f.action("restore")?;
    wait("Fill restore", || {
        Ok((Fixture::saved(&f.node()?) == before).then_some(()))
    })?;
    for placement in ["fill", "left"] {
        f.action("zoom")?;
        wait("Zoom before placement", || {
            Ok((f.node()?["lumen_state"]["zoomed"] == true).then_some(()))
        })?;
        f.action(placement)?;
        wait("placement clears Zoom", || {
            Ok((f.node()?["lumen_state"]["zoomed"] != true).then_some(()))
        })?;
        f.action("restore")?;
        wait("placement retains original restore", || {
            Ok((Fixture::saved(&f.node()?) == before).then_some(()))
        })?;
        f.record(json!({"case":format!("Zoom then {placement} retains original restore")}));
    }
    if f.opt.rapid {
        let mut trace = Vec::new();
        for command in ["zoom", "fill", "restore"] {
            f.ipc(&format!("{}lumen_window {command}", f.identity))?;
            trace.push(json!({"command":command,"timeNs":monotonic_ns(),"native":f.node()?}));
            save(
                &f.opt.output.join("rapid-transcript.json"),
                &json!({"original":before,"steps":trace}),
            )?;
        }
        fs::write(
            f.opt.output.join("rapid-client-after.json"),
            f.invoke("state", &[])?,
        )?;
        wait(
            "rapid placement preserves original restore after client commits",
            || Ok((Fixture::saved(&f.node()?) == before && f.agreement()?).then_some(())),
        )?;
        f.record(json!({"case":"rapid Zoom Fill Restore preserves original after commits","steps":trace}));
        f.action("fill")?;
        ensure(
            f.node()?["pid"] == f.owner_pid
                && f.processes[f.owner_index.ok_or("Owner missing")?]
                    .0
                    .try_wait()?
                    .is_none(),
            "Owner exited",
        )?;
        unsafe {
            libc::kill(f.owner_pid as i32, libc::SIGSTOP);
        }
        let target = (|| -> Result<(Value, Value)> {
            f.ipc(&format!("{}lumen_window restore", f.identity))?;
            let restore = f.node()?;
            f.ipc(&format!(
                "{}resize set width 540 px height 400 px",
                f.identity
            ))?;
            Ok((restore, Fixture::saved(&f.node()?)))
        })();
        unsafe {
            libc::kill(f.owner_pid as i32, libc::SIGCONT);
        }
        let (restore, latest) = target?;
        wait(
            "latest native target survives older restore acknowledgement",
            || Ok((Fixture::saved(&f.node()?) == latest && f.agreement()?).then_some(())),
        )?;
        f.record(json!({"case":"new native resize survives stale restore acknowledgement","restore":restore,"latest":latest,"received":f.state()?}));
        f.invoke("showClient", &["true"])?;
        let resizable = wait("ordinary Qt client", || f.resize_subject())?;
        let resize_id = integer(&resizable["id"])?;
        let resize_identity = format!("[con_id={resize_id} pid={}] ", f.owner_pid);
        let original_client = Fixture::client(&resizable);
        f.ipc(&format!("{resize_identity}lumen_window fill"))?;
        wait("ordinary client Fill receipt", || {
            let state = f.state()?;
            let current = f.resize_subject()?.ok_or("Resize subject missing")?;
            Ok(((
                integer(&state["clientWidth"])?,
                integer(&state["clientHeight"])?,
            ) == Fixture::client(&current))
            .then_some(()))
        })?;
        f.ipc(&format!("{resize_identity}lumen_window restore"))?;
        wait("ordinary client restore receipt", || {
            let state = f.state()?;
            Ok((f
                .resize_subject()?
                .as_ref()
                .is_some_and(|n| Fixture::client(n) == original_client)
                && (
                    integer(&state["clientWidth"])?,
                    integer(&state["clientHeight"])?,
                ) == original_client)
                .then_some(()))
        })?;
        f.invoke("resizeClient", &["520", "360"])?;
        wait(
            "ordinary client self-resize after restore acknowledgement",
            || {
                Ok((f
                    .resize_subject()?
                    .as_ref()
                    .is_some_and(|n| Fixture::client(n) == (520, 360)))
                .then_some(()))
            },
        )?;
        f.record(json!({"case":"client self-resize remains supported after restore acknowledgement","window":f.resize_subject()?}));
        f.invoke("showClient", &["false"])?;
        wait("ordinary client unmaps", || {
            Ok(f.resize_subject()?.is_none().then_some(()))
        })?;
        f.before = f.reset()?;
    }
    let pointer = f.opt.pointer.display().to_string();
    f.pointer_index = Some(f.launch(
        &pointer,
        &["pointer", "move", "0", "0"],
        "pointer.log",
        true,
    )?);
    for mode in ["none", "zoom", "fill", "minimize"] {
        let original = f.reset()?;
        f.ipc(&format!("lumen_window titlebar-action {mode}"))?;
        let box_ = f.node()?;
        let deco = integer(&box_["deco_rect"]["height"])?;
        ensure(deco > 0, "Missing titlebar decoration")?;
        f.ipc(&format!(
            "seat seat0 cursor set {} {}",
            integer(&box_["rect"]["x"])? + 180,
            integer(&box_["rect"]["y"])? - deco / 2
        ))?;
        f.pointer_events(&["m 0 0", "p", "r", "p", "r"])?;
        match mode {
            "none" => {
                thread::sleep(Duration::from_millis(300));
                ensure(
                    Fixture::saved(&f.node()?) == original,
                    "None titlebar action changed placement",
                )?
            }
            "zoom" => {
                wait("titlebar Zoom", || {
                    let n = f.node()?;
                    Ok(
                        (n["lumen_state"]["zoomed"] == true && Fixture::client(&n) == (640, 440))
                            .then_some(()),
                    )
                })?;
            }
            "fill" => {
                wait("titlebar Fill", || {
                    let n = f.node()?;
                    Ok(
                        (n["lumen_state"]["filled"] == true && n["lumen_state"]["zoomed"] != true)
                            .then_some(()),
                    )
                })?;
            }
            _ => {
                wait("titlebar minimize", || {
                    Ok((f.node()?["lumen_state"]["minimised"] == true).then_some(()))
                })?;
                f.ipc(&format!("{}scratchpad show", f.identity))?;
                wait("restore minimized subject", || {
                    Ok((f.node()?["visible"] == true).then_some(()))
                })?;
            }
        }
        f.record(
            json!({"case":format!("physical titlebar double-click {mode}"),"after":f.node()?}),
        );
        thread::sleep(Duration::from_millis(350));
    }
    f.reset()?;
    f.action("preferred-size 2000 1600")?;
    f.ipc(&format!(
        "{}move container to output HEADLESS-2",
        f.identity
    ))?;
    f.action("zoom")?;
    let small = wait("small output Zoom", || {
        let n = f.node()?;
        Ok((n["lumen_state"]["zoomed"] == true).then_some(n))
    })?;
    ensure(
        Fixture::client(&small).0 <= 800 && Fixture::client(&small).1 < 600,
        "Preferred size exceeds smaller output",
    )?;
    f.record(json!({"case":"oversized preferred client fits smaller output","window":small}));
    f.action("restore")?;
    f.ipc("output HEADLESS-2 resolution 650x480")?;
    wait("actual output shrink", || {
        let outputs: Value =
            serde_json::from_str(&f.call("swaymsg", &["-r", "-t", "get_outputs"])?)?;
        Ok(outputs
            .as_array()
            .and_then(|rows| rows.iter().find(|n| n["name"] == "HEADLESS-2"))
            .is_some_and(|n| n["rect"]["width"] == 650)
            .then_some(()))
    })?;
    f.action("zoom")?;
    let shrunk = wait("Zoom respects changed output size", || {
        let n = f.node()?;
        Ok((n["lumen_state"]["zoomed"] == true
            && Fixture::client(&n).0 <= 650
            && Fixture::client(&n).1 < 480)
            .then_some(n))
    })?;
    f.record(json!({"case":"actual output shrink before Zoom","window":shrunk}));
    f.action("restore")?;
    f.ipc(&format!(
        "{}move container to output HEADLESS-1",
        f.identity
    ))?;
    f.reset()?;
    f.action("preferred-size 640 440")?;
    f.invoke("minSize", &["700", "500"])?;
    thread::sleep(Duration::from_millis(150));
    f.action("zoom")?;
    wait("protocol minimum clamps preferred client", || {
        Ok((Fixture::client(&f.node()?) == (700, 500)).then_some(()))
    })?;
    f.record(json!({"case":"actual protocol minimum constraints","window":f.node()?}));
    f.action("restore")?;
    f.invoke("minSize", &["1", "1"])?;
    f.invoke("title", &["Renamed Zoom Fixture"])?;
    wait("owner title change", || {
        Ok(f.find(|n| n["name"] == "Renamed Zoom Fixture")?
            .is_some()
            .then_some(()))
    })?;
    let stale = f.register("Preferred Zoom Fixture")?;
    ensure(
        stale.get("error").is_some(),
        "Stale helper registration accepted",
    )?;
    let renamed = f
        .find(|n| n["id"] == f.cid)?
        .ok_or("Renamed subject absent")?;
    ensure(
        renamed["name"] == "Renamed Zoom Fixture",
        "Title did not change",
    )?;
    f.record(
        json!({"case":"captured helper rejects changed title","response":stale,"window":renamed}),
    );
    f.invoke("title", &["Preferred Zoom Fixture"])?;
    wait("restored title", || f.subject())?;
    for value in ["0 440", "640 0", "32769 440", "640x 440"] {
        let failure = f.raw(&format!(
            "{}lumen_window preferred-size {value}",
            f.identity
        ))?;
        ensure(
            !failure
                .as_array()
                .is_some_and(|rows| rows.iter().all(|r| r["success"] == true)),
            format!("Invalid preferred dimensions accepted: {value}"),
        )?;
    }
    f.record(json!({"case":"invalid preferred dimensions reject"}));
    f.invoke("minSize", &["1500", "1000"])?;
    wait("minimum exceeds usable workspace capability", || {
        Ok((f.node()?["lumen_state"]["preferred_zoom"] != true).then_some(()))
    })?;
    let failure = f.raw(&format!("{}lumen_window zoom", f.identity))?;
    ensure(
        !failure
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["success"] == true)),
        "Oversized minimum Zoom accepted",
    )?;
    f.record(
        json!({"case":"minimum beyond workspace truthfully disables Zoom","response":failure}),
    );
    f.invoke("minSize", &["1", "1"])?;
    if f.opt.effects {
        let log = fs::read_to_string(f.opt.output.join("compositor.log"))?;
        ensure(
            [
                "Creating scenefx FX renderer",
                "GL renderer: llvmpipe",
                "FX RENDERER: Shaders Initialized Successfully",
                "Trying udmabuf allocator",
                "Created GL FBO for buffer",
            ]
            .iter()
            .all(|s| log.contains(s)),
            "Software effects renderer proof missing",
        )?;
    }
    let helper = env::split_paths(&f.env.get("PATH").ok_or("Fixture PATH absent")?)
        .map(|p| p.join("lumen-window-geometry"))
        .find(|p| p.is_file())
        .ok_or("Geometry helper absent")?
        .canonicalize()?;
    let exe = Path::new("/source/tests/desktop/preferred-zoom-rust/src/main.rs");
    save(
        &f.opt.output.join("result.json"),
        &json!({"passed":true,"source":f.opt.revision,"isolation":{"nonRoot":true,"privateNetwork":true,"privateTerminal":true,"hardwareAbsent":true,"oldRootDetached":true},"helper":helper,"checks":f.records,"launcher":f.opt.launcher,"compositor":f.opt.sway,"effects":f.opt.effects,"fixtureSha256":digest(exe)?,"limitations":["Same-user preferred-size registration is a presentation contract, not a security boundary","Physical titlebar gestures cover each preference once; no third-party preferred-size inference"]}),
    )?;
    Ok(())
}
