//! Native pointer, keyboard, output, fullscreen, and lock corner-event acceptance.
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
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
fn require(value: bool, message: &str) -> Result<()> {
    if value { Ok(()) } else { Err(fail(message)) }
}

struct Options {
    sway: PathBuf,
    pointer: PathBuf,
    keyboard: PathBuf,
    lock: PathBuf,
    output: PathBuf,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut sway = None;
    let mut pointer = None;
    let mut keyboard = None;
    let mut lock = None;
    let mut output = None;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway path")?)),
            Some("--pointer") => {
                pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer path")?))
            }
            Some("--keyboard") => {
                keyboard = Some(PathBuf::from(args.next().ok_or("Missing keyboard path")?))
            }
            Some("--lock") => lock = Some(PathBuf::from(args.next().ok_or("Missing lock path")?)),
            Some("--output") => {
                output = Some(PathBuf::from(args.next().ok_or("Missing output path")?))
            }
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown hot-corners fixture option")),
        }
    }
    Ok(Options {
        sway: sway.ok_or("--sway required")?,
        pointer: pointer.ok_or("--pointer required")?,
        keyboard: keyboard.ok_or("--keyboard required")?,
        lock: lock.ok_or("--lock required")?,
        output: output.ok_or("--output required")?,
        effects,
        inside,
    })
}

fn private_environment(root: &Path, effects: bool) -> BTreeMap<String, String> {
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        values.remove(name);
    }
    values.insert("HOME".into(), root.display().to_string());
    values.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    values.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/no-system-bus", root.display()),
    );
    for name in ["CONFIG", "STATE", "DATA", "CACHE"] {
        values.insert(
            format!("XDG_{name}_HOME"),
            root.join(name.to_lowercase()).display().to_string(),
        );
    }
    for (name, value) in [
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
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        values.insert(name.into(), value.into());
    }
    values
}
fn outer(options: &Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    let output = fs::canonicalize(&options.output)?;
    let private = tempfile::tempdir()?;
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o700))?;
    let config = private.path().join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            private.path().display()
        ),
    )?;
    let mut command = Command::new("timeout");
    command
        .arg("85s")
        .arg("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--sway")
        .arg(&options.sway)
        .arg("--pointer")
        .arg(&options.pointer)
        .arg("--keyboard")
        .arg(&options.keyboard)
        .arg("--lock")
        .arg(&options.lock)
        .arg("--output")
        .arg(output)
        .env_clear()
        .envs(private_environment(private.path(), options.effects));
    if options.effects {
        command.arg("--effects");
    }
    let status = command.status()?;
    require(
        status.success(),
        &format!("Private hot-corners fixture exited {status}"),
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
        let stream = File::create(log)?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(values)
            .stdin(if interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::from(stream.try_clone()?))
            .stderr(Stdio::from(stream))
            .process_group(0)
            .spawn()?;
        self.0.push(child);
        Ok(self.0.len() - 1)
    }
    fn wait(&mut self, index: usize, seconds: u64) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            if self.0[index].try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(fail("Fixture child did not exit"));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                unsafe { libc::killpg(child.id() as i32, libc::SIGTERM) };
            }
        }
        for child in self.0.iter_mut().rev() {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                if Instant::now() >= deadline {
                    unsafe { libc::killpg(child.id() as i32, libc::SIGKILL) };
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
}
fn wait<T>(description: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = probe()? {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Timed out waiting for {description}")));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
fn first_match(folder: &Path, prefix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
        }))
}
fn first_wayland(folder: &Path) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with("wayland-") && !name.ends_with(".lock")
            })
        }))
}
fn call(program: &str, args: &[&str], values: &BTreeMap<String, String>) -> Result<Value> {
    let output = Command::new("timeout")
        .arg("5s")
        .arg(program)
        .args(args)
        .env_clear()
        .envs(values)
        .output()?;
    require(
        output.status.success(),
        &format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn ipc(command: &str, values: &BTreeMap<String, String>) -> Result<()> {
    let reply = call("swaymsg", &["-r", command], values)?;
    require(
        reply
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
        "Sway IPC command failed",
    )
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
            let mut chunk = [0u8; 65536];
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err(fail("Sway event subscriber disconnected")),
                Ok(length) => self.buffer.extend_from_slice(&chunk[..length]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    break;
                }
                Err(error) => return Err(error.into()),
            }
            while self.buffer.len() >= 14 {
                require(&self.buffer[..6] == b"i3-ipc", "Invalid Sway event framing")?;
                let length = u32::from_ne_bytes(self.buffer[6..10].try_into()?) as usize;
                let kind = u32::from_ne_bytes(self.buffer[10..14].try_into()?);
                require(length <= 1_048_576, "Oversized Sway event")?;
                if self.buffer.len() < 14 + length {
                    break;
                }
                let body = self.buffer[14..14 + length].to_vec();
                self.buffer.drain(..14 + length);
                if kind == ((1u32 << 31) | 7) {
                    let wrapper: Value = serde_json::from_slice(&body)?;
                    if let Some(payload) = wrapper["payload"].as_str() {
                        if let Ok(event) = serde_json::from_str::<Value>(payload) {
                            if event["kind"] == "lumen-corner" {
                                self.events.push(event);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn latest(&self) -> Result<&Value> {
        self.events.last().ok_or_else(|| fail("No corner event"))
    }
}

struct Driver<'a> {
    pointer: &'a mut Child,
    subscriber: &'a mut Subscriber,
    envs: &'a BTreeMap<String, String>,
}
impl Driver<'_> {
    fn send(&mut self, command: &str) -> Result<()> {
        let stream = self
            .pointer
            .stdin
            .as_mut()
            .ok_or("Pointer stdin unavailable")?;
        stream.write_all(command.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        Ok(())
    }
    fn motion(&mut self, x: f64, y: f64) -> Result<()> {
        self.send(&format!("m {x} {y}"))?;
        self.subscriber.collect(Duration::from_millis(150))
    }
    fn button(&mut self, value: &str) -> Result<()> {
        self.send(value)?;
        thread::sleep(Duration::from_millis(150));
        Ok(())
    }
    fn warp(&mut self, x: f64, y: f64) -> Result<()> {
        ipc(&format!("seat seat0 cursor set {x} {y}"), self.envs)?;
        self.subscriber.collect(Duration::from_millis(150))
    }
    fn enter(
        &mut self,
        x: f64,
        y: f64,
        corner: Option<&str>,
        expected: &[(&str, Value)],
    ) -> Result<()> {
        let before = self.subscriber.events.len();
        self.warp(x + 0.25, y + 0.25)?;
        require(
            self.subscriber.events.len() == before,
            "Warp activated a corner",
        )?;
        self.motion(-0.25, -0.25)?;
        require(
            self.subscriber.events.len() == before + 1,
            "Pointer entry did not produce exactly one corner event",
        )?;
        let event = self.subscriber.latest()?;
        require(
            event["corner"] == json!(corner),
            &format!("Wrong corner event: {event}"),
        )?;
        for (key, value) in expected {
            require(
                event[*key] == *value,
                &format!("Wrong {key} in corner event: {event}"),
            )?;
        }
        Ok(())
    }
}

fn inner(options: &Options) -> Result<()> {
    let root = PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR missing")?);
    let mut envs = env::vars().collect::<BTreeMap<_, _>>();
    let config = options.output.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1280x720 position 0 0\noutput HEADLESS-2 resolution 1280x720 position 1280 0\nseat seat0 fallback true\n",
    )?;
    let mut children = Children(Vec::new());
    children.launch(
        &options.sway,
        &[
            "--unsupported-gpu",
            "-c",
            config.to_str().ok_or("Invalid Sway config")?,
        ],
        &envs,
        &options.output.join("sway.log"),
        false,
    )?;
    let socket = wait("Sway IPC socket", || first_match(&root, "sway-ipc."))?;
    let wayland = wait("Wayland display", || first_wayland(&root))?;
    envs.insert("SWAYSOCK".into(), socket.display().to_string());
    envs.insert(
        "WAYLAND_DISPLAY".into(),
        wayland
            .file_name()
            .ok_or("No Wayland filename")?
            .to_string_lossy()
            .into_owned(),
    );
    let version = call("swaymsg", &["-r", "-t", "get_version"], &envs)?;
    require(
        version["native_hot_corners"] == true,
        "Native hot corners unavailable",
    )?;
    let mut subscriber = Subscriber::new(&socket)?;
    subscriber.collect(Duration::from_millis(150))?;
    let pointer = children.launch(
        &options.pointer,
        &["pointer", "move", "0", "0"],
        &envs,
        &options.output.join("pointer.log"),
        true,
    )?;
    thread::sleep(Duration::from_millis(200));
    {
        let mut driver = Driver {
            pointer: &mut children.0[pointer],
            subscriber: &mut subscriber,
            envs: &envs,
        };
        driver.warp(100.0, 100.0)?;
        require(
            driver.subscriber.events.is_empty(),
            "Warp generated a corner event",
        )?;
        driver.enter(
            0.0,
            0.0,
            Some("top-left"),
            &[
                ("locked", json!(false)),
                ("fullscreen", json!(false)),
                ("adjacent", json!(false)),
            ],
        )?;
        let before = driver.subscriber.events.len();
        driver.motion(5.0, 5.0)?;
        driver.motion(-5.0, -5.0)?;
        require(
            driver.subscriber.events.len() == before,
            "Corner hysteresis did not latch",
        )?;
        driver.motion(10.0, 10.0)?;
        require(
            driver.subscriber.latest()?["corner"].is_null(),
            "Corner exit missing",
        )?;
        driver.enter(1279.5, 0.0, Some("top-right"), &[("adjacent", json!(true))])?;
        driver.motion(-20.0, 20.0)?;
        require(
            driver.subscriber.latest()?["corner"].is_null(),
            "Adjacent corner exit missing",
        )?;
        driver.enter(
            0.0,
            719.5,
            Some("bottom-left"),
            &[("adjacent", json!(false)), ("output", json!("HEADLESS-1"))],
        )?;
        driver.motion(20.0, -20.0)?;
        driver.enter(
            1279.5,
            719.5,
            Some("bottom-right"),
            &[("adjacent", json!(true)), ("output", json!("HEADLESS-1"))],
        )?;
        driver.motion(-20.0, -20.0)?;
        driver.enter(
            2559.5,
            0.0,
            Some("top-right"),
            &[("adjacent", json!(false)), ("output", json!("HEADLESS-2"))],
        )?;
        driver.motion(-20.0, 20.0)?;
        driver.warp(20.0, 20.0)?;
        driver.button("p")?;
        let before = driver.subscriber.events.len();
        driver.warp(0.25, 0.25)?;
        driver.motion(-0.25, -0.25)?;
        require(
            driver.subscriber.events.len() == before + 1
                && driver.subscriber.latest()?["corner"].is_null(),
            "Drag activated a corner",
        )?;
        driver.button("r")?;
        let before = driver.subscriber.events.len();
        driver.motion(0.1, 0.1)?;
        require(
            driver.subscriber.events.len() == before,
            "Release reactivated occupied corner",
        )?;
        driver.motion(20.0, 20.0)?;
    }
    let keyboard = children.launch(
        &options.keyboard,
        &["-M", "shift", "-s", "1200", "-m", "shift"],
        &envs,
        &options.output.join("keyboard.log"),
        false,
    )?;
    thread::sleep(Duration::from_millis(250));
    let before;
    {
        let mut driver = Driver {
            pointer: &mut children.0[pointer],
            subscriber: &mut subscriber,
            envs: &envs,
        };
        driver.enter(
            0.0,
            0.0,
            Some("top-left"),
            &[("modifiers", json!(["shift"]))],
        )?;
        before = driver.subscriber.events.len();
    }
    children.wait(keyboard, 3)?;
    subscriber.collect(Duration::from_millis(150))?;
    require(
        subscriber.events.len() == before,
        "Keyboard exit generated a corner action",
    )?;
    Driver {
        pointer: &mut children.0[pointer],
        subscriber: &mut subscriber,
        envs: &envs,
    }
    .motion(20.0, 20.0)?;
    let _foot = children.launch(
        Path::new("foot"),
        &["--app-id=corner-test", "sleep", "120"],
        &envs,
        &options.output.join("foot.log"),
        false,
    )?;
    thread::sleep(Duration::from_millis(500));
    ipc("[app_id=\"corner-test\"] fullscreen enable", &envs)?;
    thread::sleep(Duration::from_millis(200));
    {
        let mut driver = Driver {
            pointer: &mut children.0[pointer],
            subscriber: &mut subscriber,
            envs: &envs,
        };
        driver.enter(0.0, 0.0, Some("top-left"), &[("fullscreen", json!(true))])?;
        driver.motion(20.0, 20.0)?;
    }
    ipc("[app_id=\"corner-test\"] fullscreen disable", &envs)?;
    {
        let mut driver = Driver {
            pointer: &mut children.0[pointer],
            subscriber: &mut subscriber,
            envs: &envs,
        };
        driver.enter(0.0, 0.0, Some("top-left"), &[("fullscreen", json!(false))])?;
    }
    let locker = children.launch(
        &options.lock,
        &[],
        &envs,
        &options.output.join("lock.log"),
        true,
    )?;
    thread::sleep(Duration::from_millis(300));
    subscriber.collect(Duration::from_millis(150))?;
    require(
        subscriber.latest()?["corner"].is_null() && subscriber.latest()?["locked"] == true,
        "Session lock did not inhibit occupied corner",
    )?;
    children.0[locker]
        .stdin
        .as_mut()
        .ok_or("Locker input missing")?
        .write_all(b"u\n")?;
    children.wait(locker, 3)?;
    subscriber.collect(Duration::from_millis(150))?;
    require(
        subscriber.latest()?["corner"].is_null() && subscriber.latest()?["locked"] == false,
        "Unlock did not clear lock fact",
    )?;
    {
        let mut driver = Driver {
            pointer: &mut children.0[pointer],
            subscriber: &mut subscriber,
            envs: &envs,
        };
        let before = driver.subscriber.events.len();
        driver.motion(0.1, 0.1)?;
        require(
            driver.subscriber.events.len() == before,
            "Unlock reactivated occupied corner",
        )?;
        driver.motion(20.0, 20.0)?;
        driver.enter(0.0, 0.0, Some("top-left"), &[("locked", json!(false))])?;
        driver.motion(20.0, 20.0)?;
    }
    let locker = children.launch(
        &options.lock,
        &[],
        &envs,
        &options.output.join("locked-entry.log"),
        true,
    )?;
    thread::sleep(Duration::from_millis(300));
    subscriber.collect(Duration::from_millis(150))?;
    {
        let mut driver = Driver {
            pointer: &mut children.0[pointer],
            subscriber: &mut subscriber,
            envs: &envs,
        };
        driver.enter(0.0, 0.0, None, &[("locked", json!(true))])?;
    }
    children.0[locker]
        .stdin
        .as_mut()
        .ok_or("Locker input missing")?
        .write_all(b"u\n")?;
    children.wait(locker, 3)?;
    subscriber.collect(Duration::from_millis(150))?;
    {
        let mut driver = Driver {
            pointer: &mut children.0[pointer],
            subscriber: &mut subscriber,
            envs: &envs,
        };
        let before = driver.subscriber.events.len();
        driver.motion(0.1, 0.1)?;
        require(
            driver.subscriber.events.len() == before,
            "Locked entry reactivated after unlock",
        )?;
        driver.motion(20.0, 20.0)?;
        driver.enter(0.0, 0.0, Some("top-left"), &[("locked", json!(false))])?;
    }
    let fields = HashSet::from([
        "kind",
        "version",
        "corner",
        "output",
        "modifiers",
        "locked",
        "fullscreen",
        "adjacent",
    ]);
    for event in &subscriber.events {
        require(
            event.as_object().is_some_and(|object| {
                object.keys().map(String::as_str).collect::<HashSet<_>>() == fields
            }),
            "Corner event fields changed",
        )?;
        require(
            event["version"] == 1
                && event["output"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
                && ["locked", "fullscreen", "adjacent"]
                    .iter()
                    .all(|key| event[*key].is_boolean()),
            "Corner event types changed",
        )?;
    }
    fs::write(
        options.output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"passed":true,"events":subscriber.events,"version":version})
            )?
        ),
    )?;
    println!("PASS native corner pointer/modifier/drag/fullscreen/adjacency/lock lifecycle");
    Ok(())
}

fn main() {
    let result = options().and_then(|options| {
        if options.inside {
            inner(&options)
        } else {
            outer(&options)
        }
    });
    if let Err(error) = result {
        eprintln!("hot corners: {error}");
        std::process::exit(1);
    }
}
