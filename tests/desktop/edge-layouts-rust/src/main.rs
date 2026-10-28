//! Private native edge-tiling previews, commits, and pointer-constraint acceptance.
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(fail(message))
    }
}
fn i(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| fail(format!("Missing integer {key}: {value}")))
}
fn f(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| fail(format!("Missing number {key}: {value}")))
}
fn id(value: &Value) -> Result<i64> {
    i(value, "id")
}
fn frame_rect(node: &Value) -> Result<Value> {
    let mut rect = node["rect"].clone();
    let deco = i(&node["deco_rect"], "height")?;
    rect["y"] = json!(i(&rect, "y")? - deco);
    rect["height"] = json!(i(&rect, "height")? + deco);
    Ok(rect)
}
fn expected_rect(event: &Value) -> Value {
    json!({"x":event["x"],"y":event["y"],"width":event["width"],"height":event["height"]})
}
fn flag(event: &Value, key: &str) -> bool {
    event[key] == true || event[key].as_i64() == Some(1)
}

struct Options {
    sway: PathBuf,
    pointer: PathBuf,
    keyboard: PathBuf,
    lock: PathBuf,
    quickshell: Option<PathBuf>,
    constraint: Option<PathBuf>,
    output: PathBuf,
    private: Option<PathBuf>,
    effects: bool,
    constraints_only: bool,
}
fn options() -> Result<Options> {
    let mut sway = None;
    let mut pointer = None;
    let mut keyboard = None;
    let mut lock = None;
    let mut quickshell = None;
    let mut constraint = None;
    let mut output = None;
    let mut private = None;
    let mut effects = false;
    let mut constraints_only = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
            Some("--pointer") => {
                pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
            }
            Some("--keyboard") => {
                keyboard = Some(PathBuf::from(args.next().ok_or("Missing keyboard")?))
            }
            Some("--lock") => lock = Some(PathBuf::from(args.next().ok_or("Missing lock client")?)),
            Some("--quickshell") => {
                quickshell = Some(PathBuf::from(args.next().ok_or("Missing Quickshell")?))
            }
            Some("--constraint-client") => {
                constraint = Some(PathBuf::from(
                    args.next().ok_or("Missing constraint client")?,
                ))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--private") => {
                private = Some(PathBuf::from(args.next().ok_or("Missing private runtime")?))
            }
            Some("--effects") => effects = true,
            Some("--constraints-only") => constraints_only = true,
            _ => return Err(fail("Unknown edge-layouts fixture option")),
        }
    }
    Ok(Options {
        sway: sway.ok_or("--sway required")?,
        pointer: pointer.ok_or("--pointer required")?,
        keyboard: keyboard.ok_or("--keyboard required")?,
        lock: lock.ok_or("--lock required")?,
        quickshell,
        constraint,
        output: output.ok_or("--output required")?,
        private,
        effects,
        constraints_only,
    })
}
fn program(name: &str, values: &BTreeMap<String, String>) -> Result<PathBuf> {
    let path = values.get("PATH").ok_or("Pinned PATH missing")?;
    for folder in env::split_paths(path) {
        let candidate = folder.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("{name} unavailable in pinned PATH")))
}
fn environment(root: &Path, effects: bool) -> BTreeMap<String, String> {
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        values.remove(key);
    }
    values.insert("HOME".into(), root.display().to_string());
    values.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    for key in ["CONFIG", "STATE", "DATA", "CACHE"] {
        values.insert(
            format!("XDG_{key}_HOME"),
            root.join(key.to_lowercase()).display().to_string(),
        );
    }
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_HEADLESS_OUTPUTS", "2"),
        ("WLR_RENDERER", if effects { "gles2" } else { "pixman" }),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_QPA_PLATFORMTHEME", ""),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        values.insert(key.into(), value.into());
    }
    values.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/absent-system", root.display()),
    );
    values
}
fn outer(options: &Options) -> Result<()> {
    require(!options.output.exists(), "Fresh output directory required")?;
    fs::create_dir_all(&options.output)?;
    let output = fs::canonicalize(&options.output)?;
    let private = tempfile::Builder::new().prefix("lumen-edge-").tempdir()?;
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o700))?;
    for key in ["config", "state", "data", "cache"] {
        fs::create_dir(private.path().join(key))?;
    }
    let config = private.path().join("bus.conf");
    fs::write(&config, format!("<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>", private.path().display()))?;
    let values = environment(private.path(), options.effects);
    let keyboard = if options.keyboard == Path::new("wtype") {
        program("wtype", &values)?
    } else {
        fs::canonicalize(&options.keyboard)?
    };
    let mut command = Command::new(program("timeout", &values)?);
    command
        .arg("130s")
        .arg(program("dbus-run-session", &values)?)
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--sway")
        .arg(fs::canonicalize(&options.sway)?)
        .arg("--pointer")
        .arg(fs::canonicalize(&options.pointer)?)
        .arg("--keyboard")
        .arg(keyboard)
        .arg("--lock")
        .arg(fs::canonicalize(&options.lock)?)
        .arg("--output")
        .arg(output)
        .arg("--private")
        .arg(private.path())
        .env_clear()
        .envs(values);
    if let Some(path) = &options.quickshell {
        command.arg("--quickshell").arg(fs::canonicalize(path)?);
    }
    if let Some(path) = &options.constraint {
        command
            .arg("--constraint-client")
            .arg(fs::canonicalize(path)?);
    }
    if options.effects {
        command.arg("--effects");
    }
    if options.constraints_only {
        command.arg("--constraints-only");
    }
    let status = command.status()?;
    require(
        status.success(),
        format!("Private edge-layouts fixture exited {status}"),
    )
}
struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        program: &Path,
        args: &[&str],
        values: &BTreeMap<String, String>,
        log: &Path,
        interactive: bool,
    ) -> Result<usize> {
        let file = File::create(log)?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(values)
            .stdin(if interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file))
            .process_group(0)
            .spawn()?;
        self.0.push(child);
        Ok(self.0.len() - 1)
    }
    fn send(&mut self, index: usize, command: &str) -> Result<()> {
        let input = self.0[index]
            .stdin
            .as_mut()
            .ok_or("Interactive child input unavailable")?;
        input.write_all(command.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        Ok(())
    }
    fn wait_exit(&mut self, index: usize) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.0[index].try_wait()? {
                return require(status.success(), format!("Child {index} exited {status}"));
            }
            if Instant::now() >= deadline {
                return Err(fail(format!("Child {index} did not exit")));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                unsafe {
                    libc::killpg(child.id() as i32, libc::SIGTERM);
                }
            }
        }
        for child in self.0.iter_mut().rev() {
            let deadline = Instant::now() + Duration::from_secs(3);
            while child.try_wait().ok().flatten().is_none() {
                if Instant::now() >= deadline {
                    unsafe {
                        libc::killpg(child.id() as i32, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
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
                "Edge layouts timed out waiting for {label}: {}",
                last_error.unwrap_or_default()
            )));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|row| row.ok().map(|row| row.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && !name.ends_with(suffix)
            })
        }))
}
fn call(program: &Path, args: &[&str], values: &BTreeMap<String, String>) -> Result<String> {
    let output = Command::new("timeout")
        .arg("5s")
        .arg(program)
        .args(args)
        .env_clear()
        .envs(values)
        .output()?;
    require(
        output.status.success(),
        format!(
            "{} {:?} failed: {}",
            program.display(),
            args,
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
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

struct Subscriber {
    stream: UnixStream,
    buffer: Vec<u8>,
    events: Vec<Value>,
}
impl Subscriber {
    fn new(socket: &Path) -> Result<Self> {
        let mut stream = UnixStream::connect(socket)?;
        let payload = br#"["tick"]"#;
        let mut frame = b"i3-ipc".to_vec();
        frame.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
        frame.extend_from_slice(&2u32.to_ne_bytes());
        frame.extend_from_slice(payload);
        stream.write_all(&frame)?;
        Ok(Self {
            stream,
            buffer: Vec::new(),
            events: Vec::new(),
        })
    }
    fn collect(&mut self, duration: Duration) -> Result<()> {
        let deadline = Instant::now() + duration;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.stream.set_read_timeout(Some(remaining))?;
            let mut chunk = [0u8; 65_536];
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err(fail("Sway event subscriber disconnected")),
                Ok(size) => self.buffer.extend_from_slice(&chunk[..size]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    break
                }
                Err(error) => return Err(error.into()),
            }
            while self.buffer.len() >= 14 {
                require(&self.buffer[..6] == b"i3-ipc", "Invalid Sway event framing")?;
                let size = u32::from_ne_bytes(self.buffer[6..10].try_into()?) as usize;
                let kind = u32::from_ne_bytes(self.buffer[10..14].try_into()?);
                require(size <= 1_048_576, "Oversized Sway event")?;
                if self.buffer.len() < 14 + size {
                    break;
                }
                let body = self.buffer[14..14 + size].to_vec();
                self.buffer.drain(..14 + size);
                if kind == ((1u32 << 31) | 7) {
                    let wrapper: Value = serde_json::from_slice(&body)?;
                    if let Some(payload) = wrapper["payload"].as_str() {
                        if let Ok(event) = serde_json::from_str::<Value>(payload) {
                            if event.get("lumen_drag_layout").is_some()
                                || event.get("lumen_drag_overview").is_some()
                            {
                                self.events.push(event);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn phases(&self, from: usize) -> Vec<&str> {
        self.events[from..]
            .iter()
            .filter_map(|event| event["phase"].as_str())
            .collect()
    }
    fn has(&self, from: usize, phase: &str) -> bool {
        self.phases(from).contains(&phase)
    }
    fn preview(&self, from: usize) -> Result<Value> {
        self.events[from..]
            .iter()
            .find(|event| event["phase"] == "preview")
            .cloned()
            .ok_or_else(|| {
                fail(format!(
                    "Preview event missing from {}",
                    json!(self.events[from..])
                ))
            })
    }
}

struct Fixture<'a> {
    options: &'a Options,
    values: BTreeMap<String, String>,
    swaymsg: PathBuf,
    foot: PathBuf,
    grim: PathBuf,
    children: Children,
    subscriber: Subscriber,
    pointer: usize,
    subject_app: String,
    cid: i64,
}
impl<'a> Fixture<'a> {
    fn new(options: &'a Options) -> Result<Self> {
        let private = options.private.as_ref().ok_or("Private runtime required")?;
        let mut values = env::vars().collect::<BTreeMap<_, _>>();
        require(
            values
                .get("DBUS_SESSION_BUS_ADDRESS")
                .is_some_and(|address| address.starts_with("unix:")),
            "Private session bus required",
        )?;
        let swaymsg = program("swaymsg", &values)?;
        let foot = program("foot", &values)?;
        let grim = program("grim", &values)?;
        let mut children = Children(Vec::new());
        let config = options.output.join("sway.conf");
        fs::write(&config, "output HEADLESS-1 resolution 1280x720 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0\nseat seat0 fallback true\nfloating_modifier Mod4 normal\ndefault_border normal 2\n")?;
        children.launch(
            &options.sway,
            &[
                "--unsupported-gpu",
                "-c",
                config.to_str().ok_or("Sway config path invalid")?,
            ],
            &values,
            &options.output.join("sway.log"),
            false,
        )?;
        let socket = wait("Sway socket", || first_file(private, "sway-ipc.", ".lock"))?;
        values.insert("SWAYSOCK".into(), socket.display().to_string());
        let display = wait("Wayland display", || {
            first_file(private, "wayland-", ".lock")
        })?;
        values.insert(
            "WAYLAND_DISPLAY".into(),
            display
                .file_name()
                .ok_or("Display name invalid")?
                .to_string_lossy()
                .into_owned(),
        );
        let version: Value =
            serde_json::from_str(&call(&swaymsg, &["-r", "-t", "get_version"], &values)?)?;
        require(
            version["native_edge_tiling"] == true,
            "Native edge tiling capability missing",
        )?;
        if let Some(qs) = &options.quickshell {
            let bar = options.output.join("bar.qml");
            fs::write(&bar, include_str!("../bar.qml"))?;
            children.launch(
                qs,
                &["-p", bar.to_str().ok_or("Bar path invalid")?],
                &values,
                &options.output.join("bar.log"),
                false,
            )?;
            thread::sleep(Duration::from_millis(500));
        }
        let subscriber = Subscriber::new(&socket)?;
        let pointer = children.launch(
            &options.pointer,
            &["pointer", "move", "0", "0"],
            &values,
            &options.output.join("pointer.log"),
            true,
        )?;
        thread::sleep(Duration::from_millis(150));
        let mut fixture = Self {
            options,
            values,
            swaymsg,
            foot,
            grim,
            children,
            subscriber,
            pointer,
            subject_app: "edge-test".into(),
            cid: 0,
        };
        fixture.children.launch(
            &fixture.foot,
            &[
                "-o",
                "resize-by-cells=no",
                "--app-id=edge-test",
                "sleep",
                "180",
            ],
            &fixture.values,
            &options.output.join("foot.log"),
            false,
        )?;
        fixture.cid = id(&wait("edge subject", || fixture.window())?)?;
        Ok(fixture)
    }
    fn sway(&self, args: &[&str]) -> Result<Value> {
        let mut parts = vec!["-r"];
        parts.extend_from_slice(args);
        Ok(serde_json::from_str(&call(
            &self.swaymsg,
            &parts,
            &self.values,
        )?)?)
    }
    fn ipc(&self, command: &str) -> Result<Value> {
        let reply = self.sway(&[command])?;
        require(
            reply
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
            format!("Sway command {command} failed: {reply}"),
        )?;
        Ok(reply)
    }
    fn tree(&self) -> Result<Value> {
        self.sway(&["-t", "get_tree"])
    }
    fn app(&self, name: &str) -> Result<Option<Value>> {
        Ok(find_node(&self.tree()?, &|node| node["app_id"] == name))
    }
    fn window(&self) -> Result<Option<Value>> {
        self.app(&self.subject_app)
    }
    fn required(&self) -> Result<Value> {
        self.window()?
            .ok_or_else(|| fail(format!("Subject {} missing", self.subject_app)))
    }
    fn outer_rect(&self) -> Result<Value> {
        frame_rect(&self.required()?)
    }
    fn bystander_rect(&self, cid: i64) -> Result<Value> {
        let node = find_node(&self.tree()?, &|node| node["id"].as_i64() == Some(cid))
            .ok_or("Bystander disappeared")?;
        Ok(node["rect"].clone())
    }
    fn reset(&self) -> Result<Value> {
        self.ipc(&format!("[con_id={}] floating enable, resize set 400 px 300 px, move absolute position 200 px 200 px", self.cid))?;
        thread::sleep(Duration::from_millis(150));
        self.outer_rect()
    }
    fn screenshot(&self, name: &str) -> Result<()> {
        let path = self.options.output.join(name);
        call(
            &self.grim,
            &[
                "-o",
                "HEADLESS-1",
                path.to_str().ok_or("Image path invalid")?,
            ],
            &self.values,
        )?;
        Ok(())
    }
    fn at(&self, x: f64, y: f64) -> Result<()> {
        self.ipc(&format!("seat seat0 cursor set {x} {y}"))?;
        Ok(())
    }
    fn motion(&mut self, dx: f64, dy: f64) -> Result<()> {
        self.children.send(self.pointer, &format!("m {dx} {dy}"))?;
        self.subscriber.collect(Duration::from_millis(120))
    }
    fn button(&mut self, value: &str) -> Result<()> {
        self.children.send(self.pointer, value)?;
        self.subscriber.collect(Duration::from_millis(160))
    }
    fn collect(&mut self, millis: u64) -> Result<()> {
        self.subscriber.collect(Duration::from_millis(millis))
    }
    fn start(&mut self) -> Result<Value> {
        let rect = self.reset()?;
        fs::write(
            self.options.output.join("window-before.json"),
            serde_json::to_vec_pretty(&self.required()?)?,
        )?;
        self.screenshot("before.png")?;
        self.at(f(&rect, "x")? + 150.0, f(&rect, "y")? + 10.0)?;
        self.button("p")?;
        self.motion(1.0, 1.0)?;
        Ok(rect)
    }
    fn to(&mut self, x: f64, y: f64) -> Result<()> {
        self.at(x + 0.2, y + 0.2)?;
        self.motion(-0.2, -0.2)
    }
    fn release(&mut self) -> Result<()> {
        self.button("r")?;
        thread::sleep(Duration::from_millis(150));
        Ok(())
    }
    fn phase(&self, begin: usize, phase: &str) -> bool {
        self.subscriber.has(begin, phase)
    }
    fn preview(&self, begin: usize) -> Result<Value> {
        self.subscriber.preview(begin)
    }
    fn event_count(&self) -> usize {
        self.subscriber.events.len()
    }
}

fn full_drag_suite(fixture: &mut Fixture<'_>) -> Result<(Option<Value>, Option<Value>)> {
    if fixture.options.constraints_only {
        fixture.ipc(&format!("[con_id={}] kill", fixture.cid))?;
        wait("original subject closure", || {
            Ok(fixture.window()?.is_none().then_some(()))
        })?;
        return Ok((None, None));
    }
    let original = fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(0.0, 360.0)?;
    fixture.collect(320)?;
    require(
        fixture.phase(begin, "preview"),
        "Left-edge preview event missing",
    )?;
    let preview = fixture.preview(begin)?;
    require(
        preview["id"].as_i64() == Some(fixture.cid) && preview["target"] == 1,
        "Preview targeted wrong window or side",
    )?;
    if fixture.options.quickshell.is_some() {
        require(
            i(&preview, "y")? >= 40 && i(&preview, "height")? <= 680,
            "Bar margin absent from preview",
        )?;
    }
    require(
        fixture.outer_rect()?["width"] == original["width"],
        "Preview resized client before release",
    )?;
    fixture.screenshot("preview.png")?;
    fixture.to(100.0, 360.0)?;
    require(
        fixture.phase(begin, "leave"),
        "Leaving edge did not clear preview",
    )?;
    fixture.release()?;
    require(
        !fixture.phase(begin, "commit"),
        "Released outside edge committed layout",
    )?;

    let original = fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(0.0, 360.0)?;
    fixture.collect(300)?;
    fixture.release()?;
    require(
        fixture.phase(begin, "commit"),
        "Valid edge release did not commit",
    )?;
    require(
        fixture.outer_rect()? == expected_rect(&preview),
        "Committed geometry differs from preview",
    )?;
    fixture.ipc(&format!("[con_id={}] lumen_window restore", fixture.cid))?;
    thread::sleep(Duration::from_millis(150));
    require(
        fixture.outer_rect()? == original,
        "Restore lost pre-drag geometry",
    )?;

    fixture.children.launch(
        &fixture.foot,
        &[
            "-o",
            "resize-by-cells=no",
            "--app-id=edge-bystander",
            "sleep",
            "180",
        ],
        &fixture.values,
        &fixture.options.output.join("bystander.log"),
        false,
    )?;
    let bystander = wait("bystander window", || fixture.app("edge-bystander"))?;
    let other_id = id(&bystander)?;
    fixture.ipc(&format!("[con_id={other_id}] floating enable, resize set 240 px 180 px, move absolute position 800 px 400 px"))?;
    thread::sleep(Duration::from_millis(200));
    let other_box = fixture.bystander_rect(other_id)?;
    for (x, y, target) in [
        (1279.5, 360.0, 2),
        (0.0, 0.0, 3),
        (1279.5, 0.0, 4),
        (0.0, 719.5, 5),
        (1279.5, 719.5, 6),
    ] {
        let original = fixture.start()?;
        let begin = fixture.event_count();
        fixture.to(x, y)?;
        fixture.collect(320)?;
        let candidate = fixture.preview(begin)?;
        require(
            candidate["id"].as_i64() == Some(fixture.cid) && candidate["target"] == target,
            format!("Corner or side selected wrong target: {candidate}"),
        )?;
        fixture.to(600.0, 360.0)?;
        require(fixture.phase(begin, "leave"), "Corner leave event missing")?;
        fixture.release()?;
        require(
            !fixture.phase(begin, "commit"),
            "Corner leave committed stale preview",
        )?;
        fixture.start()?;
        let begin = fixture.event_count();
        fixture.to(x, y)?;
        fixture.collect(320)?;
        fixture.release()?;
        require(
            fixture.phase(begin, "commit") && fixture.outer_rect()? == expected_rect(&candidate),
            "Corner commit geometry differs from preview",
        )?;
        require(
            fixture.bystander_rect(other_id)? == other_box,
            "Bystander moved during edge commit",
        )?;
        fixture.ipc(&format!("[con_id={}] lumen_window restore", fixture.cid))?;
        thread::sleep(Duration::from_millis(200));
        require(
            fixture.outer_rect()? == original,
            "Corner restore lost original geometry",
        )?;
    }

    let original = fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(0.0, 360.0)?;
    fixture.collect(300)?;
    call(
        &fixture.options.keyboard,
        &["-k", "Escape"],
        &fixture.values,
    )?;
    fixture.collect(120)?;
    fixture.release()?;
    require(
        fixture.phase(begin, "cancel")
            && !fixture.phase(begin, "commit")
            && fixture.outer_rect()? == original,
        "Escape did not cancel edge drag",
    )?;
    fixture.ipc("lumen_window edge-tiling disable")?;
    fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(0.0, 360.0)?;
    fixture.collect(350)?;
    fixture.release()?;
    require(
        !fixture.phase(begin, "preview") && !fixture.phase(begin, "commit"),
        "Disabled edge tiling still previewed",
    )?;
    fixture.ipc("lumen_window edge-tiling enable")?;

    // Held Alt broadens edge activation; disabling the option removes that path.
    fixture.start()?;
    let keyboard = fixture.children.launch(
        &fixture.options.keyboard,
        &["-M", "alt", "-s", "1200", "-m", "alt"],
        &fixture.values,
        &fixture.options.output.join("option.log"),
        false,
    )?;
    thread::sleep(Duration::from_millis(200));
    let begin = fixture.event_count();
    fixture.to(20.0, 360.0)?;
    require(
        fixture.phase(begin, "preview"),
        "Held Alt did not broaden edge preview",
    )?;
    fixture.release()?;
    require(
        fixture.phase(begin, "commit"),
        "Held Alt did not commit layout",
    )?;
    fixture.children.wait_exit(keyboard)?;
    fixture.ipc("lumen_window option-tiling disable")?;
    fixture.start()?;
    let keyboard = fixture.children.launch(
        &fixture.options.keyboard,
        &["-M", "alt", "-s", "1200", "-m", "alt"],
        &fixture.values,
        &fixture.options.output.join("option-off.log"),
        false,
    )?;
    thread::sleep(Duration::from_millis(200));
    let begin = fixture.event_count();
    fixture.to(20.0, 360.0)?;
    fixture.collect(350)?;
    fixture.release()?;
    require(
        !fixture.phase(begin, "preview"),
        "Disabled Alt tiling still previewed",
    )?;
    fixture.children.wait_exit(keyboard)?;
    fixture.ipc("lumen_window option-tiling enable")?;

    fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(2279.5, 400.0)?;
    fixture.collect(320)?;
    let second = fixture.preview(begin)?;
    fixture.release()?;
    require(
        fixture.outer_rect()? == expected_rect(&second)
            && i(&second, "x")? >= 1280
            && i(&second, "height")? <= 800,
        "Second-output layout ignored its workarea",
    )?;
    fixture.ipc("lumen_window tile-gap 16")?;
    fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(0.0, 360.0)?;
    fixture.collect(320)?;
    let gap = fixture.preview(begin)?;
    require(i(&gap, "x")? == 16, "Configurable gap missing from preview")?;
    fixture.release()?;
    fixture.ipc("lumen_window tile-gap 8")?;

    fixture.ipc("lumen_window top-edge-overview disable")?;
    fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(600.0, 0.0)?;
    fixture.collect(320)?;
    fixture.release()?;
    require(
        fixture.phase(begin, "commit"),
        "Top-edge fill did not commit with overview disabled",
    )?;
    fixture.ipc("lumen_window top-edge-overview enable")?;
    fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(600.0, 0.0)?;
    fixture.collect(1000)?;
    fixture.release()?;
    require(
        fixture.subscriber.events[begin..].iter().any(|event| {
            flag(event, "lumen_drag_overview") && event["id"].as_i64() == Some(fixture.cid)
        }) && !fixture.phase(begin, "commit"),
        "Top-edge overview did not win dwell arbitration",
    )?;
    fixture.ipc("lumen_window top-edge-fill disable")?;
    fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(600.0, 0.0)?;
    fixture.collect(1000)?;
    fixture.release()?;
    require(
        fixture.subscriber.events[begin..]
            .iter()
            .any(|event| flag(event, "lumen_drag_overview"))
            && !fixture.phase(begin, "preview")
            && !fixture.phase(begin, "commit"),
        "Overview-only top edge was not exclusive",
    )?;
    fixture.ipc("lumen_window top-edge-overview disable")?;
    fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(600.0, 0.0)?;
    fixture.collect(1000)?;
    fixture.release()?;
    require(
        !fixture.subscriber.events[begin..]
            .iter()
            .any(|event| flag(event, "lumen_drag_overview"))
            && !fixture.phase(begin, "preview")
            && !fixture.phase(begin, "commit"),
        "Disabled top-edge actions still fired",
    )?;
    fixture.ipc("lumen_window top-edge-fill enable")?;
    fixture.ipc("lumen_window top-edge-overview enable")?;

    let original = fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(0.0, 360.0)?;
    fixture.collect(320)?;
    let locker = fixture.children.launch(
        &fixture.options.lock,
        &[],
        &fixture.values,
        &fixture.options.output.join("lock.log"),
        true,
    )?;
    fixture.collect(300)?;
    require(
        fixture.phase(begin, "cancel"),
        "Session lock did not cancel pending edge drag",
    )?;
    fixture.children.send(locker, "u")?;
    fixture.children.wait_exit(locker)?;
    fixture.release()?;
    require(
        !fixture.phase(begin, "commit") && fixture.outer_rect()? == original,
        "Late release committed after session lock",
    )?;

    fixture.start()?;
    let begin = fixture.event_count();
    fixture.to(0.0, 360.0)?;
    fixture.collect(320)?;
    fixture.ipc(&format!("[con_id={}] kill", fixture.cid))?;
    wait("captured subject closure", || {
        Ok(fixture.window()?.is_none().then_some(()))
    })?;
    fixture.release()?;
    require(
        !fixture.phase(begin, "commit") && fixture.bystander_rect(other_id)? == other_box,
        "Closed target retargeted commit onto bystander",
    )?;
    require(
        fixture.sway(&["-t", "get_version"])?["native_edge_tiling"] == true,
        "Native edge tiling capability disappeared",
    )?;
    Ok((Some(preview), Some(second)))
}

fn constraint_suite(fixture: &mut Fixture<'_>) -> Result<Vec<String>> {
    let mut checked = Vec::new();
    let Some(client_program) = &fixture.options.constraint else {
        return Ok(checked);
    };
    let client_program = client_program.clone();
    fixture.motion(0.0, 0.0)?;
    fixture.subject_app = "constraint-test".into();
    for kind in ["fixed", "bounded", "free"] {
        let client = fixture.children.launch(
            &client_program,
            &[kind],
            &fixture.values,
            &fixture
                .options
                .output
                .join(format!("constraint-{kind}.log")),
            true,
        )?;
        fixture.cid = id(&wait(&format!("{kind} client window"), || {
            fixture.window()
        })?)?;
        fixture.ipc(&format!("[con_id={}] border normal 2", fixture.cid))?;
        let original = fixture.reset()?;
        require(
            i(&fixture.required()?["deco_rect"], "height")? > 0,
            "Constraint client titlebar missing",
        )?;
        if kind != "free" {
            if kind == "fixed" {
                fixture.at(f(&original, "x")? + 60.0, f(&original, "y")? + 16.0)?;
                fixture.button("p")?;
                fixture.release()?;
                require(
                    fixture.outer_rect()? == original
                        && fixture.required()?["lumen_state"]["filled"] == false,
                    "Fixed client changed on titlebar click",
                )?;
            }
            fixture.ipc(&format!("[con_id={}] lumen_window left", fixture.cid))?;
            thread::sleep(Duration::from_millis(200));
            require(
                fixture.outer_rect()? == original
                    && fixture.required()?["lumen_state"]["filled"] == false,
                format!("{kind} client accepted invalid left tile"),
            )?;
            let original = fixture.start()?;
            let begin = fixture.event_count();
            fixture.to(0.0, 360.0)?;
            fixture.collect(350)?;
            require(
                !fixture.phase(begin, "preview"),
                format!("{kind} client previewed invalid tile"),
            )?;
            call(
                &fixture.options.keyboard,
                &["-k", "Escape"],
                &fixture.values,
            )?;
            fixture.release()?;
            require(
                fixture.outer_rect()? == original,
                format!("{kind} client moved after Escape"),
            )?;
            if kind == "bounded" {
                fixture.start()?;
                let begin = fixture.event_count();
                fixture.to(2279.5, 400.0)?;
                fixture.collect(320)?;
                fixture.release()?;
                require(
                    fixture.phase(begin, "commit"),
                    "Bounded client rejected valid second-output tile",
                )?;
                fixture.ipc(&format!("[con_id={}] lumen_window restore", fixture.cid))?;
                thread::sleep(Duration::from_millis(200));
                require(
                    fixture.outer_rect()? == original,
                    "Bounded client restore lost original geometry",
                )?;
            }
        } else {
            for (command, active) in [("l", "locked"), ("c", "confined")] {
                let original = fixture.reset()?;
                fixture.at(f(&original, "x")? + 150.0, f(&original, "y")? + 100.0)?;
                fixture.motion(1.0, 1.0)?;
                fixture.children.send(client, command)?;
                let log = fixture
                    .options
                    .output
                    .join(format!("constraint-{kind}.log"));
                wait(active, || {
                    Ok(fs::read_to_string(&log)?
                        .lines()
                        .any(|line| line == active)
                        .then_some(()))
                })?;
                let begin = fixture.event_count();
                fixture.button("p")?;
                fixture.motion(-2000.0, -2000.0)?;
                fixture.collect(350)?;
                fixture.release()?;
                require(
                    !fixture.subscriber.events[begin..]
                        .iter()
                        .any(|event| event["phase"] == "preview" || event["phase"] == "commit"),
                    format!("{active} pointer generated edge tiling"),
                )?;
                require(
                    fixture.outer_rect()? == original,
                    format!("{active} pointer moved client"),
                )?;
                let log_text = fs::read_to_string(&log)?;
                let reports = log_text
                    .lines()
                    .filter(|line| line.starts_with("motion "))
                    .map(|line| {
                        let values = line.split_whitespace().skip(1).collect::<Vec<_>>();
                        if values.len() != 2 {
                            return Err(fail("Malformed pointer motion record"));
                        }
                        Ok((values[0].parse::<f64>()?, values[1].parse::<f64>()?))
                    })
                    .collect::<Result<Vec<_>>>()?;
                require(
                    !reports.is_empty()
                        && reports
                            .iter()
                            .all(|(x, y)| (0.0..=400.0).contains(x) && (0.0..=300.0).contains(y)),
                    format!("{active} pointer escaped client: {reports:?}"),
                )?;
                fixture.children.send(client, "u")?;
                fixture.collect(200)?;
            }
            fixture.start()?;
            let begin = fixture.event_count();
            fixture.to(0.0, 360.0)?;
            fixture.collect(320)?;
            fixture.release()?;
            require(
                fixture.phase(begin, "commit"),
                "Released pointer constraint blocked valid drag",
            )?;
        }
        checked.push(kind.to_owned());
        fixture.children.send(client, "q")?;
        fixture.children.wait_exit(client)?;
        wait("constraint client closure", || {
            Ok(fixture.window()?.is_none().then_some(()))
        })?;
    }
    Ok(checked)
}

fn run(options: &Options) -> Result<()> {
    let mut fixture = Fixture::new(options)?;
    let (preview, second) = full_drag_suite(&mut fixture)?;
    let checked = constraint_suite(&mut fixture)?;
    let report = json!({
        "passed":true,
        "fullDragSuite":!options.constraints_only,
        "constraints":checked,
        "events":fixture.subscriber.events,
        "capturedID":fixture.cid,
        "preview":preview,
        "secondOutput":second,
        "effects":options.effects,
    });
    fs::write(
        options.output.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if options.constraints_only {
        println!("PASS native fixed/bounded sizes, pointer lock/confinement and released-constraint drag");
    } else {
        println!("PASS native drag preview/leave/cancel/commit/restore/modifiers/top arbitration/lock and client constraints");
    }
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let options = options()?;
        if options.private.is_some() {
            run(&options)
        } else {
            outer(&options)
        }
    })();
    if let Err(error) = result {
        eprintln!("Edge layouts fixture failed: {error}");
        std::process::exit(1);
    }
}
