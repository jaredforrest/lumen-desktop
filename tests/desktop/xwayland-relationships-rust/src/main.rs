//! Actual X11 ancestry, modal transitions, XRes ownership, and key recipients.
mod projection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn i(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| fail(format!("Missing integer {key}: {value}")))
}
fn digest(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(data))
}
fn file_hash(path: &Path) -> Result<String> {
    Ok(digest(fs::read(path)?))
}

struct Options {
    sway: PathBuf,
    client: PathBuf,
    keyboard: PathBuf,
    xwayland: PathBuf,
    output: PathBuf,
    egl_vendor: Option<PathBuf>,
    effects: bool,
    without_xres: bool,
    projection: bool,
    projection_groups: bool,
    inside: bool,
    run: bool,
    parent_net: Option<String>,
    revision: Option<String>,
}
fn options() -> Result<Options> {
    let mut sway = None;
    let mut client = None;
    let mut keyboard = None;
    let mut xwayland = None;
    let mut output = None;
    let mut egl_vendor = None;
    let mut effects = false;
    let mut without_xres = false;
    let mut projection = false;
    let mut projection_groups = false;
    let mut inside = false;
    let mut run = false;
    let mut parent_net = None;
    let mut revision = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
            Some("--client") => {
                client = Some(PathBuf::from(args.next().ok_or("Missing X11 client")?))
            }
            Some("--keyboard") => {
                keyboard = Some(PathBuf::from(args.next().ok_or("Missing keyboard")?))
            }
            Some("--xwayland") => {
                xwayland = Some(PathBuf::from(args.next().ok_or("Missing Xwayland")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--egl-vendor") => {
                egl_vendor = Some(PathBuf::from(args.next().ok_or("Missing EGL vendor")?))
            }
            Some("--parent-net") => {
                parent_net = Some(
                    args.next()
                        .ok_or("Missing network identity")?
                        .into_string()
                        .map_err(|_| "Invalid network identity")?,
                )
            }
            Some("--revision") => {
                revision = Some(
                    args.next()
                        .ok_or("Missing source revision")?
                        .into_string()
                        .map_err(|_| "Invalid source revision")?,
                )
            }
            Some("--effects") => effects = true,
            Some("--without-xres") => without_xres = true,
            Some("--projection") => projection = true,
            Some("--projection-groups") => {
                projection = true;
                projection_groups = true;
            }
            Some("--inside") => inside = true,
            Some("--run") => run = true,
            _ => return Err(fail("Unknown Xwayland fixture option")),
        }
    }
    Ok(Options {
        sway: sway.ok_or("--sway required")?,
        client: client.ok_or("--client required")?,
        keyboard: keyboard.ok_or("--keyboard required")?,
        xwayland: xwayland.ok_or("--xwayland required")?,
        output: output.ok_or("--output required")?,
        egl_vendor,
        effects,
        without_xres,
        projection,
        projection_groups,
        inside,
        run,
        parent_net,
        revision,
    })
}
fn immutable(path: &Path) -> Result<PathBuf> {
    let value = fs::canonicalize(path)?;
    require(
        value.starts_with("/nix/store") && value.is_file(),
        "Existing immutable fixture dependency required",
    )?;
    Ok(value)
}
fn program(name: &str) -> Result<PathBuf> {
    for folder in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let path = folder.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(fail(format!("{name} unavailable in pinned PATH")))
}
fn args_for(options: &Options, inside: bool, run: bool) -> Vec<String> {
    let mut args = vec![
        "--sway".into(),
        options.sway.display().to_string(),
        "--client".into(),
        options.client.display().to_string(),
        "--keyboard".into(),
        options.keyboard.display().to_string(),
        "--xwayland".into(),
        options.xwayland.display().to_string(),
        "--output".into(),
        options.output.display().to_string(),
    ];
    if let Some(path) = &options.egl_vendor {
        args.extend(["--egl-vendor".into(), path.display().to_string()]);
    }
    if let Some(net) = &options.parent_net {
        args.extend(["--parent-net".into(), net.clone()]);
    }
    if let Some(revision) = &options.revision {
        args.extend(["--revision".into(), revision.clone()]);
    }
    if options.effects {
        args.push("--effects".into());
    }
    if options.without_xres {
        args.push("--without-xres".into());
    }
    if options.projection {
        args.push("--projection".into());
    }
    if options.projection_groups {
        args.push("--projection-groups".into());
    }
    if inside {
        args.push("--inside".into());
    }
    if run {
        args.push("--run".into());
    }
    args
}
fn outer(options: &mut Options) -> Result<()> {
    let repo = fs::canonicalize(env::current_dir()?)?;
    let output = if options.output.is_absolute() {
        options.output.clone()
    } else {
        repo.join(&options.output)
    };
    require(!output.exists(), "Fresh output directory required")?;
    let sway = immutable(&options.sway)?;
    let client = immutable(&options.client)?;
    let keyboard = immutable(&options.keyboard)?;
    let xwayland = immutable(&options.xwayland)?;
    let vendor = if options.effects {
        let path = options
            .egl_vendor
            .clone()
            .or_else(|| env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").map(PathBuf::from))
            .ok_or("Effects require pinned EGL vendor")?;
        Some(immutable(&path)?)
    } else {
        None
    };
    let revision = Command::new(program("git")?)
        .args([
            "-C",
            repo.to_str().ok_or("Repository path invalid")?,
            "rev-parse",
            "HEAD",
        ])
        .output()?;
    require(revision.status.success(), "Source revision unavailable")?;
    options.sway = sway;
    options.client = client;
    options.keyboard = keyboard;
    options.xwayland = xwayland;
    options.egl_vendor = vendor;
    options.output = output.clone();
    options.parent_net = Some(
        fs::read_link("/proc/self/ns/net")?
            .to_string_lossy()
            .into_owned(),
    );
    options.revision = Some(String::from_utf8(revision.stdout)?.trim().to_owned());
    fs::create_dir_all(&output)?;
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
    let mut command = Command::new(program("timeout")?);
    command
        .arg("120s")
        .arg(program("lumen-application-isolation")?)
        .arg("run")
        .arg("--source")
        .arg(&repo)
        .arg("--output")
        .arg(&output);
    if options.effects {
        command.arg("--software-graphics");
    }
    let status = command
        .arg("--")
        .arg(env::current_exe()?)
        .args(args_for(options, true, false))
        .status()?;
    require(
        status.success(),
        format!("Private Xwayland boundary exited {status}"),
    )
}
fn private_boundary(options: &Options) -> Result<()> {
    require(
        unsafe { libc::getuid() } != 0,
        "Private fixture must not run as root",
    )?;
    require(
        env::var("HOME")? == "/home/fixture",
        "Private home required",
    )?;
    require(
        fs::read_link("/proc/self/ns/net")?.to_string_lossy()
            != options
                .parent_net
                .as_deref()
                .ok_or("Parent network identity required")?,
        "Network namespace was not private",
    )?;
    for name in ["dri", "snd", "video0"] {
        require(
            !Path::new("/dev").join(name).exists(),
            format!("Host hardware entered fixture: {name}"),
        )?;
    }
    require(
        Path::new("/dev/udmabuf").exists() == options.effects,
        "Software allocator policy differs",
    )?;
    Ok(())
}
fn inner(options: &Options) -> Result<()> {
    private_boundary(options)?;
    let config = Path::new("/artifacts/bus.conf");
    fs::write(
        &config,
        "<busconfig><type>session</type><listen>unix:tmpdir=/run/fixture</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
    )?;
    let status = Command::new(program("timeout")?)
        .arg("110s")
        .arg(program("dbus-run-session")?)
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .args(args_for(options, true, true))
        .status()?;
    require(
        status.success(),
        format!("Private Xwayland session exited {status}"),
    )
}

fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_error = None;
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last_error = Some(error.to_string()),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!(
                "Xwayland fixture timed out waiting for {label}: {}",
                last_error.unwrap_or_default()
            )));
        }
        thread::sleep(Duration::from_millis(20));
    }
}
fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|row| row.ok().map(|row| row.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && (suffix.is_empty() || !name.ends_with(suffix))
            })
        }))
}
fn write_nu(path: &Path, nu: &Path, script: &str) -> Result<()> {
    fs::write(path, format!("#!{}\n{script}", nu.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
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
fn workspace_for(node: &Value, xid: i64, current: &str) -> Option<String> {
    let current = if node["type"] == "workspace" {
        node["name"].as_str().unwrap_or(current)
    } else {
        current
    };
    if node["window"].as_i64() == Some(xid) {
        return Some(current.to_owned());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            if let Some(found) = workspace_for(child, xid, current) {
                return Some(found);
            }
        }
    }
    None
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
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(values)
            .stdin(Stdio::null())
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file))
            .process_group(0)
            .spawn()?;
        Ok(Self(child))
    }
    fn terminate(&mut self) -> Result<()> {
        if self.0.try_wait()?.is_none() {
            unsafe {
                libc::kill(self.0.id() as i32, libc::SIGTERM);
            }
        }
        Ok(())
    }
    fn wait(&mut self, seconds: u64) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(fail("Managed child did not exit"));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
            let deadline = Instant::now() + Duration::from_secs(3);
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

struct Keyboard {
    child: Child,
    stdout: BufReader<ChildStdout>,
    serial: u64,
}
impl Keyboard {
    fn new(program: &Path, values: &BTreeMap<String, String>, log: &Path) -> Result<Self> {
        let file = File::create(log)?;
        let mut child = Command::new(program)
            .env_clear()
            .envs(values)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(file))
            .process_group(0)
            .spawn()?;
        let stdout = BufReader::new(child.stdout.take().ok_or("Keyboard stdout missing")?);
        let mut keyboard = Self {
            child,
            stdout,
            serial: 0,
        };
        let mut ready = String::new();
        keyboard.stdout.read_line(&mut ready)?;
        require(
            ready.trim() == "ready",
            format!("Private keyboard did not become ready: {ready}"),
        )?;
        Ok(keyboard)
    }
    fn send(&mut self, operation: &str, value: i32, state: i32) -> Result<()> {
        self.serial += 1;
        let input = self.child.stdin.as_mut().ok_or("Keyboard stdin missing")?;
        writeln!(input, "{operation} {} {value} {state}", self.serial)?;
        input.flush()?;
        let mut reply = String::new();
        self.stdout.read_line(&mut reply)?;
        require(
            reply.trim() == format!("ack {}", self.serial),
            format!(
                "Keyboard command {} was not acknowledged: {reply}",
                self.serial
            ),
        )
    }
    fn type_a(&mut self) -> Result<()> {
        self.send("m", 0, 0)?;
        self.send("k", 30, 1)?;
        self.send("k", 30, 0)?;
        self.send("m", 0, 0)
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.child.id() as i32, libc::SIGTERM);
            }
            let _ = self.child.wait();
        }
    }
}

struct Client {
    child: Child,
    stdout: ChildStdout,
    buffer: Vec<u8>,
    latest: Value,
    serial: u64,
    output: PathBuf,
}
impl Client {
    fn new(
        label: &str,
        program: &Path,
        values: &BTreeMap<String, String>,
        output: &Path,
    ) -> Result<Self> {
        let file = File::create(output.join(format!("{label}.log")))?;
        let mut child = Command::new(program)
            .arg(label)
            .env_clear()
            .envs(values)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(file))
            .process_group(0)
            .spawn()?;
        let stdout = child.stdout.take().ok_or("X11 client stdout missing")?;
        let mut client = Self {
            child,
            stdout,
            buffer: Vec::new(),
            latest: Value::Null,
            serial: 0,
            output: output.to_path_buf(),
        };
        let pid = client.child.id() as i64;
        client.wait_state("initial X11 client state", |state| {
            state["pid"].as_i64() == Some(pid)
        })?;
        Ok(client)
    }
    fn pump(&mut self, timeout: Duration) -> Result<bool> {
        let milliseconds = timeout.as_millis().min(i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd {
            fd: self.stdout.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut descriptor, 1, milliseconds) };
        if ready < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if ready == 0 {
            return Ok(false);
        }
        let mut chunk = [0u8; 65_536];
        let size = self.stdout.read(&mut chunk)?;
        require(size > 0, "Owned X11 client exited before acknowledgement")?;
        self.buffer.extend_from_slice(&chunk[..size]);
        while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line = self.buffer.drain(..=index).collect::<Vec<_>>();
            self.latest = serde_json::from_slice(&line[..line.len() - 1])?;
            fs::write(
                self.output
                    .join(format!("{}-client-state.json", self.child.id())),
                serde_json::to_vec_pretty(&self.latest)?,
            )?;
        }
        Ok(true)
    }
    fn drain(&mut self) -> Result<()> {
        while self.pump(Duration::ZERO)? {}
        Ok(())
    }
    fn wait_state(&mut self, label: &str, predicate: impl Fn(&Value) -> bool) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if predicate(&self.latest) {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(fail(format!("X11 client timed out waiting for {label}")));
            }
            self.pump(remaining)?;
        }
    }
    fn state(&mut self) -> Result<Value> {
        self.drain()?;
        require(!self.latest.is_null(), "X11 client has no state")?;
        Ok(self.latest.clone())
    }
    fn command(&mut self, action: &str, index: usize, value: i64) -> Result<()> {
        self.serial += 1;
        let input = self
            .child
            .stdin
            .as_mut()
            .ok_or("X11 client stdin missing")?;
        writeln!(input, "{action} {index} {value}")?;
        input.flush()?;
        let serial = self.serial;
        self.wait_state(&format!("{action} serial {serial}"), |state| {
            state["serial"]
                .as_u64()
                .is_some_and(|value| value >= serial)
        })
    }
    fn xid(&mut self, index: usize) -> Result<i64> {
        i(
            self.state()?["windows"]
                .get(index)
                .ok_or("X11 window state missing")?,
            "id",
        )
    }
    fn key_count(&mut self, index: usize) -> Result<i64> {
        i(
            self.state()?["windows"]
                .get(index)
                .ok_or("X11 window state missing")?,
            "keys",
        )
    }
    fn wait_exit(&mut self, seconds: u64) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(fail("X11 client did not exit"));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.child.id() as i32, libc::SIGTERM);
            }
            let _ = self.child.wait();
        }
    }
}

struct Fixture {
    env: BTreeMap<String, String>,
    sway: Managed,
    keyboard: Keyboard,
    clients: Vec<Client>,
    checks: Vec<&'static str>,
}
impl Fixture {
    fn new(options: &Options) -> Result<Self> {
        let output = Path::new("/artifacts");
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let mut values: BTreeMap<String, String> = env::vars().collect();
        for (key, value) in [
            ("WLR_BACKENDS", "headless"),
            (
                "WLR_RENDERER",
                if options.effects { "gles2" } else { "pixman" },
            ),
            ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
            ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ] {
            values.insert(key.into(), value.into());
        }
        if options.effects {
            values.insert(
                "__EGL_VENDOR_LIBRARY_FILENAMES".into(),
                options
                    .egl_vendor
                    .as_ref()
                    .ok_or("Missing EGL vendor")?
                    .display()
                    .to_string(),
            );
        }
        let nu = immutable(&program("nu")?)?;
        let source = Path::new("/source/tests/desktop/xwayland-relationships-rust");
        write_nu(
            &output.join("xwayland-server"),
            &nu,
            &fs::read_to_string(source.join("xwayland-wrapper.nu"))?,
        )?;
        write_nu(
            &output.join("display-helper"),
            &nu,
            &fs::read_to_string(source.join("display-helper.nu"))?,
        )?;
        values.insert("WLR_XWAYLAND".into(), "/artifacts/xwayland-server".into());
        values.insert(
            "LUMEN_XWAYLAND_REAL".into(),
            options.xwayland.display().to_string(),
        );
        values.insert(
            "LUMEN_XWAYLAND_NO_XRES".into(),
            if options.without_xres { "1" } else { "0" }.into(),
        );
        fs::write(
            output.join("sway.conf"),
            "output HEADLESS-1 resolution 1280x800\nseat seat0 fallback true\nfocus_follows_mouse no\nxwayland force\nfor_window [all] floating enable\nexec /artifacts/display-helper\n",
        )?;
        let sway = Managed::launch(
            &options.sway,
            &["-d", "--unsupported-gpu", "-c", "/artifacts/sway.conf"],
            &values,
            &output.join("sway.log"),
        )?;
        let sock = wait("private compositor", || {
            first_file(&runtime, "sway-ipc.", "")
        })?;
        values.insert("SWAYSOCK".into(), sock.display().to_string());
        let wayland = wait("private Wayland socket", || {
            first_file(&runtime, "wayland-", ".lock")
        })?;
        values.insert(
            "WAYLAND_DISPLAY".into(),
            wayland
                .file_name()
                .ok_or("Wayland socket name missing")?
                .to_string_lossy()
                .into_owned(),
        );
        let display = wait("private Xwayland DISPLAY", || {
            let path = output.join("display");
            if path.exists() {
                Ok(Some(fs::read_to_string(path)?))
            } else {
                Ok(None)
            }
        })?;
        require(
            display.starts_with(':'),
            "Private Xwayland display required",
        )?;
        values.insert("DISPLAY".into(), display);
        let keyboard = Keyboard::new(&options.keyboard, &values, &output.join("keyboard.log"))?;
        let first = Client::new("first", &options.client, &values, output)?;
        let second = Client::new("second", &options.client, &values, output)?;
        let mut fixture = Self {
            env: values,
            sway,
            keyboard,
            clients: vec![first, second],
            checks: Vec::new(),
        };
        fixture.map(0, 0)?;
        fixture.map(1, 0)?;
        Ok(fixture)
    }
    fn call(&self, args: &[&str]) -> Result<(std::process::ExitStatus, String)> {
        let result = Command::new(program("timeout")?)
            .arg("5s")
            .arg(program("swaymsg")?)
            .args(args)
            .env_clear()
            .envs(&self.env)
            .output()?;
        Ok((result.status, String::from_utf8(result.stdout)?))
    }
    fn tree(&self) -> Result<Value> {
        let (status, output) = self.call(&["-r", "-t", "get_tree"])?;
        require(
            status.success(),
            format!("Compositor tree unavailable: {status}"),
        )?;
        Ok(serde_json::from_str(&output)?)
    }
    fn ipc(&self, request: &str) -> Result<Value> {
        let (status, output) = self.call(&["-r", request])?;
        require(
            status.success(),
            format!("Compositor command failed ({status}): {request}: {output}"),
        )?;
        Ok(serde_json::from_str(&output)?)
    }
    fn command(&mut self, who: usize, action: &str, index: usize, value: i64) -> Result<()> {
        self.clients[who].command(action, index, value)
    }
    fn xid(&mut self, who: usize, index: usize) -> Result<i64> {
        self.clients[who].xid(index)
    }
    fn count(&mut self, who: usize, index: usize) -> Result<i64> {
        self.clients[who].key_count(index)
    }
    fn node(&mut self, who: usize, index: usize) -> Result<Option<Value>> {
        let xid = self.xid(who, index)?;
        Ok(find_node(&self.tree()?, &|node| {
            node["window"].as_i64() == Some(xid)
        }))
    }
    fn mapped(&mut self, who: usize, index: usize) -> Result<Value> {
        wait("actual mapped X11 surface", || self.node(who, index))
    }
    fn relation(&mut self, who: usize, index: usize) -> Result<Value> {
        Ok(self
            .node(who, index)?
            .map(|node| node["lumen_relationships"].clone())
            .unwrap_or(Value::Null))
    }
    fn map(&mut self, who: usize, index: usize) -> Result<Value> {
        self.command(who, "map", index, 0)?;
        self.mapped(who, index)
    }
    fn location(&mut self, who: usize, index: usize) -> Result<Option<String>> {
        let xid = self.xid(who, index)?;
        Ok(workspace_for(&self.tree()?, xid, ""))
    }
    fn focused(&mut self, who: usize, index: usize) -> Result<bool> {
        Ok(self
            .node(who, index)?
            .is_some_and(|node| node["focused"] == true))
    }
    fn focus(&mut self, who: usize, index: usize, target: Option<(usize, usize)>) -> Result<()> {
        let node = self.mapped(who, index)?;
        self.ipc(&format!("[con_id={}] focus", i(&node, "id")?))?;
        let (target_who, target_index) = target.unwrap_or((who, index));
        wait("exact native focus", || {
            Ok(self.focused(target_who, target_index)?.then_some(()))
        })
    }
    fn type_only(&mut self, who: usize, index: usize, others: &[(usize, usize)]) -> Result<()> {
        let target_before = self.count(who, index)?;
        let mut bystanders = Vec::new();
        for &(other, slot) in others {
            bystanders.push((other, slot, self.count(other, slot)?));
        }
        self.keyboard.type_a()?;
        thread::sleep(Duration::from_millis(120));
        wait("actual X11 key event", || {
            Ok((self.count(who, index)? == target_before + 1).then_some(()))
        })?;
        for (other, slot, before) in bystanders {
            require(
                self.count(other, slot)? == before,
                "Keyboard reached bystander",
            )?;
        }
        Ok(())
    }
    fn relation_wait(
        &mut self,
        label: &str,
        predicate: impl Fn(&Value) -> bool,
        who: usize,
        index: usize,
    ) -> Result<()> {
        wait(label, || {
            Ok(predicate(&self.relation(who, index)?).then_some(()))
        })
    }
    fn node_wait(
        &mut self,
        label: &str,
        predicate: impl Fn(Option<&Value>) -> bool,
        who: usize,
        index: usize,
    ) -> Result<()> {
        wait(label, || {
            let node = self.node(who, index)?;
            Ok(predicate(node.as_ref()).then_some(()))
        })
    }
}

fn run(options: &Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    if options.projection {
        projection::run(&mut f, options)?;
    } else if options.without_xres {
        require(
            f.clients[0].state()?["xres"] == false && f.clients[1].state()?["xres"] == false,
            "XRes unexpectedly available",
        )?;
        require(
            f.node(0, 0)?.ok_or("First surface missing")?["pid"] == 0
                && f.node(1, 0)?.ok_or("Second surface missing")?["pid"] == 0,
            "Unknown XRes PID did not remain zero",
        )?;
        let parent = f.xid(0, 0)?;
        f.command(0, "parent", 1, parent)?;
        f.command(0, "property", 1, 1)?;
        f.map(0, 1)?;
        require(
            f.relation(0, 1)?
                == json!({"version":1,"parent_supported":false,"parent":null,"modal":null}),
            "Unknown-PID relationship unexpectedly supported",
        )?;
        f.focus(0, 0, None)?;
        f.type_only(0, 0, &[(0, 1), (1, 0)])?;
        f.checks.push("Actual XRes-disabled server leaves unknown-PID ancestry/modal unavailable and preserves explicit key recipient");
    } else {
        normal_checks(&mut f)?;
    }
    fs::write(
        "/artifacts/final-tree.json",
        serde_json::to_vec_pretty(&f.tree()?)?,
    )?;
    let parent = f.xid(0, 0)?;
    f.command(0, "parent", 1, parent)?;
    f.command(0, "property", 1, 1)?;
    f.sway.terminate()?;
    let sway = f.sway.wait(5)?;
    let first = f.clients[0].wait_exit(5)?;
    let second = f.clients[1].wait_exit(5)?;
    require(
        sway.success() && first.success() && second.success(),
        "Sway or X11 client failed clean shutdown",
    )?;
    require(
        !Path::new(f.env.get("SWAYSOCK").ok_or("Missing compositor socket")?).exists(),
        "Private Sway socket leaked",
    )?;
    f.checks.push("Actual compositor SIGTERM exit with live X11 parent/modal clients detaches listeners cleanly and removes private socket");
    if options.effects {
        let log = fs::read_to_string("/artifacts/sway.log")?;
        for expected in [
            "Creating scenefx FX renderer",
            "GL renderer: llvmpipe",
            "FX RENDERER: Shaders Initialized Successfully",
            "Trying udmabuf allocator",
            "Created GL FBO for buffer",
        ] {
            require(
                log.contains(expected),
                format!("Effects rendering evidence missing: {expected}"),
            )?;
        }
    }
    let packages = [
        ("compositor", &options.sway),
        ("client", &options.client),
        ("keyboard", &options.keyboard),
        ("xwayland", &options.xwayland),
    ]
    .into_iter()
    .map(|(name, path)| {
        Ok((
            name.to_owned(),
            json!({"path":path,"sha256":file_hash(path)?}),
        ))
    })
    .collect::<Result<serde_json::Map<String, Value>>>()?;
    let report = json!({
        "passed":true, "checks":f.checks, "sourceRevision":options.revision,
        "effects":options.effects, "withoutXRes":options.without_xres, "projection":options.projection,
        "projectionFixtureSha256":if options.projection {Some(file_hash(Path::new("/source/tests/desktop/xwayland-relationships-rust/src/projection.rs"))?)}else{None},
        "fixtureSha256":file_hash(Path::new("/source/tests/desktop/xwayland-relationships-rust/src/main.rs"))?,
        "packages":packages,
        "scope":"Private actual Xwayland server/XRes/WM hints and physical key recipients; hints are not authenticated application or document identity",
    });
    fs::write(
        "/artifacts/result.json",
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    println!(
        "PASS actual Xwayland relationships {} checks",
        report["checks"].as_array().unwrap().len()
    );
    Ok(())
}

fn normal_checks(f: &mut Fixture) -> Result<()> {
    require(
        f.clients[0].state()?["xres"] == true && f.clients[1].state()?["xres"] == true,
        "XRes ownership unavailable",
    )?;
    require(
        i(&f.node(0, 0)?.ok_or("First surface missing")?, "pid")? == f.clients[0].child.id() as i64,
        "First XRes PID differs",
    )?;
    require(
        i(&f.node(1, 0)?.ok_or("Second surface missing")?, "pid")?
            == f.clients[1].child.id() as i64,
        "Second XRes PID differs",
    )?;
    let parent_xid = f.xid(0, 0)?;
    f.command(0, "parent", 1, parent_xid)?;
    f.map(0, 1)?;
    let parent_id = i(&f.mapped(0, 0)?, "id")?;
    f.relation_wait(
        "live same-XRes-process ancestry",
        |r| r["parent"] == parent_id,
        0,
        1,
    )?;
    let relation = f.relation(0, 1)?;
    require(
        relation["parent_supported"] == true && relation["modal"] == false,
        "Initial ancestry metadata differs",
    )?;
    f.checks
        .push("Actual XRes process identities and live WM_TRANSIENT_FOR map to exact parent conId");

    f.focus(0, 1, None)?;
    f.type_only(0, 1, &[(0, 0), (1, 0)])?;
    f.focus(0, 0, None)?;
    f.type_only(0, 0, &[(0, 1), (1, 0)])?;
    f.focus(1, 0, None)?;
    f.type_only(1, 0, &[(0, 0), (0, 1)])?;
    f.checks.push("Utility/document/other-process focus sequence delivers each physical key only to the captured native recipient");

    f.command(0, "parent", 2, parent_xid)?;
    f.command(0, "property", 2, 1)?;
    f.map(0, 2)?;
    f.focus(0, 0, Some((0, 2)))?;
    f.type_only(0, 2, &[(0, 0), (0, 1), (1, 0)])?;
    f.checks.push("Initially modal child receives parent-targeted focus and typing without redirecting unrelated app");

    f.command(0, "modal", 2, 0)?;
    f.relation_wait(
        "client-message modal removal",
        |r| r["modal"] == false,
        0,
        2,
    )?;
    f.focus(0, 0, None)?;
    f.command(0, "modal", 2, 1)?;
    wait("live modal add immediate focus reconciliation", || {
        Ok((f.focused(0, 2)? && f.relation(0, 2)?["modal"] == true).then_some(()))
    })?;
    f.type_only(0, 2, &[(0, 0), (1, 0)])?;
    f.command(0, "modal", 2, 0)?;
    f.relation_wait("live modal remove", |r| r["modal"] == false, 0, 2)?;
    f.focus(0, 0, None)?;
    f.command(0, "property", 2, 1)?;
    f.node_wait(
        "property modal add",
        |n| n.is_some_and(|n| n["focused"] == true),
        0,
        2,
    )?;
    f.command(0, "property", 2, 0)?;
    f.relation_wait(
        "property deletion resets modal",
        |r| r["modal"] == false,
        0,
        2,
    )?;
    f.focus(0, 0, None)?;
    f.checks.push("Actual EWMH client-message and property add/remove/delete reconcile modality live with no sticky retired state");

    let utility_xid = f.xid(0, 1)?;
    f.command(0, "parent", 2, utility_xid)?;
    let utility_id = i(&f.mapped(0, 1)?, "id")?;
    f.relation_wait("actual reparent", |r| r["parent"] == utility_id, 0, 2)?;
    f.command(0, "parent", 2, 0)?;
    f.relation_wait(
        "actual detach",
        |r| r["parent"].is_null() && r["parent_supported"] == true,
        0,
        2,
    )?;
    f.checks
        .push("Live reparent/detach updates exact parent metadata");

    f.command(0, "parent", 1, parent_xid)?;
    f.command(0, "parent", 0, utility_xid)?;
    wait("upstream rejects cyclic parent hint", || {
        Ok((f.relation(0, 0)?["parent"].is_null()
            && f.relation(0, 1)?["parent"] == i(&f.mapped(0, 0)?, "id")?)
        .then_some(()))
    })?;
    f.focus(0, 0, None)?;
    f.type_only(0, 0, &[(0, 1), (0, 2), (1, 0)])?;
    f.command(0, "parent", 0, 0)?;
    f.checks.push("Actual cycle attempt is rejected by pinned XWM and preserves acyclic ancestry without hangs or arbitrary focus redirection");

    f.command(0, "parent", 2, parent_xid)?;
    f.command(0, "modal", 1, 1)?;
    f.command(0, "modal", 2, 1)?;
    wait("two actual modal siblings", || {
        Ok(
            (f.relation(0, 1)?["modal"] == true && f.relation(0, 2)?["modal"] == true)
                .then_some(()),
        )
    })?;
    f.focus(1, 0, None)?;
    let parent_id = i(&f.mapped(0, 0)?, "id")?;
    f.ipc(&format!("[con_id={parent_id}] focus"))?;
    require(
        f.focused(1, 0)?,
        "Ambiguous modal siblings stole unrelated focus",
    )?;
    f.type_only(1, 0, &[(0, 0), (0, 1), (0, 2)])?;
    f.command(0, "modal", 1, 0)?;
    f.command(0, "modal", 2, 0)?;
    f.checks.push(
        "Ambiguous live modal siblings reject parent activation and never select arbitrary sibling",
    );

    f.command(0, "parent", 2, 0)?;
    f.command(0, "modal", 2, 1)?;
    f.relation_wait(
        "parentless modal",
        |r| r["modal"] == true && r["parent"].is_null(),
        0,
        2,
    )?;
    require(
        f.mapped(0, 2)?["lumen_pair"]["eligible"] == false,
        "Parentless modal became pair eligible",
    )?;
    let first_id = i(&f.mapped(0, 0)?, "id")?;
    let modal_id = i(&f.mapped(0, 2)?, "id")?;
    let (status, text) = f.call(&["-r", &format!("lumen_pair create {first_id} {modal_id}")])?;
    let result: Value = serde_json::from_str(&text)?;
    require(
        status.code() == Some(2)
            && result[0]["success"] == false
            && f.mapped(0, 0)?["lumen_pair"]["id"].is_null(),
        "Parentless modal pairing was not rejected",
    )?;
    f.command(0, "modal", 2, 0)?;
    f.checks.push("Actual parentless modal is ineligible for paired primary membership and create fails without changing partner");

    let second_xid = f.xid(1, 0)?;
    f.command(0, "parent", 1, second_xid)?;
    f.command(0, "pid", 1, f.clients[1].child.id() as i64)?;
    f.relation_wait(
        "crossprocess hint unknown",
        |r| r["parent_supported"] == false,
        0,
        1,
    )?;
    let node = f.mapped(0, 1)?;
    let relation = f.relation(0, 1)?;
    require(
        i(&node, "pid")? == f.clients[0].child.id() as i64
            && relation["parent"].is_null()
            && relation["modal"].is_null(),
        "Cross-process or forged PID changed ownership",
    )?;
    f.command(0, "property", 1, 1)?;
    f.focus(1, 0, None)?;
    f.type_only(1, 0, &[(0, 0), (0, 1), (0, 2)])?;
    f.command(0, "property", 1, 0)?;
    f.checks.push("Cross-process parent and forged _NET_WM_PID cannot change XRes PID, confer ancestry or steal unrelated keyboard input");

    f.command(0, "parent", 1, parent_xid)?;
    f.command(0, "unmap", 0, 0)?;
    f.node_wait("parent actual unmap", |n| n.is_none(), 0, 0)?;
    require(
        f.relation(0, 1)?["parent"].is_null(),
        "Unmapped parent left stale link",
    )?;
    f.map(0, 0)?;
    let parent_id = i(&f.mapped(0, 0)?, "id")?;
    f.relation_wait("parent actual remap", |r| r["parent"] == parent_id, 0, 1)?;
    f.command(0, "destroy", 0, 0)?;
    f.node_wait("parent actual destroy", |n| n.is_none(), 0, 0)?;
    f.relation_wait("destroy clears child link", |r| r["parent"].is_null(), 0, 1)?;
    f.command(0, "new", 0, 0)?;
    f.map(0, 0)?;
    require(
        i(&f.mapped(0, 0)?, "id")? != parent_id && f.relation(0, 1)?["parent"].is_null(),
        "New parent inherited stale conId",
    )?;
    f.checks.push(
        "Parent unmap/remap/destroy and replacement retire stale conIds without retargeting child",
    );

    f.command(0, "override", 3, 1)?;
    f.command(0, "map", 3, 0)?;
    wait("actual mapped override-redirect surface", || {
        let state = f.clients[0].state()?;
        Ok((state["windows"][3]["mapState"] == 2
            && state["windows"][3]["overrideRedirect"] == 1
            && f.node(0, 3)?.is_none())
        .then_some(()))
    })?;
    let override_xid = f.xid(0, 3)?;
    f.command(0, "parent", 1, override_xid)?;
    f.relation_wait(
        "managed child rejects unmanaged parent",
        |r| r["parent_supported"] == false && r["parent"].is_null(),
        0,
        1,
    )?;
    f.focus(0, 1, None)?;
    f.type_only(0, 1, &[(0, 0), (1, 0)])?;
    f.command(0, "override", 3, 0)?;
    let managed = i(&f.mapped(0, 3)?, "id")?;
    f.relation_wait(
        "unmanaged parent becomes managed",
        |r| r["parent"] == managed,
        0,
        1,
    )?;
    f.command(0, "override", 3, 1)?;
    wait("managed parent becomes unmanaged", || {
        Ok(
            (f.node(0, 3)?.is_none() && f.relation(0, 1)?["parent_supported"] == false)
                .then_some(()),
        )
    })?;
    f.focus(0, 1, None)?;
    f.type_only(0, 1, &[(0, 0), (1, 0)])?;
    f.command(0, "parent", 1, 0)?;
    f.command(0, "unmap", 3, 0)?;
    f.command(0, "override", 3, 0)?;
    f.checks.push("Actual managed child rejects same-PID override-redirect parent safely through managed/unmanaged transitions without wrong casts or key targets");

    let owner = i(&f.mapped(0, 0)?, "id")?;
    let partner = i(&f.mapped(1, 0)?, "id")?;
    f.ipc(&format!("lumen_pair create {owner} {partner}"))?;
    f.node_wait(
        "actual X11 primary pair",
        |n| n.is_some_and(|n| n["lumen_pair"]["partner"] == partner),
        0,
        0,
    )?;
    let owner_xid = f.xid(0, 0)?;
    f.command(0, "parent", 3, owner_xid)?;
    f.command(0, "property", 3, 1)?;
    f.map(0, 3)?;
    wait("actual mapped modal follows paired owner", || {
        Ok(
            (f.relation(0, 3)?["parent"] == owner && f.location(0, 3)? == f.location(0, 0)?)
                .then_some(()),
        )
    })?;
    f.focus(0, 0, Some((0, 3)))?;
    f.type_only(0, 3, &[(0, 0), (1, 0)])?;
    require(
        f.mapped(0, 3)?["lumen_pair"]["eligible"] == false,
        "Modal became pair eligible",
    )?;
    f.ipc(&format!("lumen_pair leave {owner}"))?;
    wait("paired owner exit restores modal association", || {
        Ok((f.mapped(0, 0)?["lumen_pair"]["id"].is_null()
            && f.location(0, 3)? == f.location(0, 0)?)
        .then_some(()))
    })?;
    f.ipc(&format!("lumen_pair leave {partner}"))?;
    f.command(0, "parent", 3, 0)?;
    f.command(0, "property", 3, 0)?;
    wait(
        "actual dialog detaches without stale pair ownership",
        || {
            Ok((f.relation(0, 3)?["parent"].is_null()
                && f.mapped(0, 3)?["lumen_pair"]["eligible"] == true)
                .then_some(()))
        },
    )?;
    f.checks.push("Actual X11 primary pair admits a new same-process modal, routes exact input and restores/detaches its owner without stranding it");
    Ok(())
}

fn main() {
    let result = options().and_then(|mut options| {
        if options.run {
            run(&options)
        } else if options.inside {
            inner(&options)
        } else {
            outer(&mut options)
        }
    });
    if let Err(error) = result {
        eprintln!("Xwayland relationship fixture: {error}");
        std::process::exit(1);
    }
}
