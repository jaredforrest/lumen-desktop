//! Native frame, layout, desktop reveal, and application frame regression.
use png::{ColorType, Decoder, Transformations};
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::os::unix::fs::PermissionsExt;
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
fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn nap(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

#[derive(Clone)]
struct Options {
    sway: PathBuf,
    launcher: Option<PathBuf>,
    output: PathBuf,
    pointer: PathBuf,
    assets: PathBuf,
    core_only: bool,
    inside: bool,
    effects: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut sway = None;
        let mut launcher = None;
        let mut output = None;
        let mut pointer = env::var_os("LUMEN_TEST_POINTER").map(PathBuf::from);
        let mut assets = env::current_dir()?.join("res/lumen-desktop/shell");
        let mut core_only = false;
        let mut inside = false;
        let mut effects = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--assets") => assets = PathBuf::from(args.next().ok_or("Missing assets")?),
                Some("--core-only") => core_only = true,
                Some("--inside") => inside = true,
                Some("--effects") => effects = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown window frame option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        let output = output.ok_or("--output required")?;
        Ok(Self {
            sway: sway.ok_or("--sway required")?,
            launcher,
            output: if output.is_absolute() {
                output
            } else {
                env::current_dir()?.join(output)
            },
            pointer: pointer.ok_or("--pointer-tool or LUMEN_TEST_POINTER required")?,
            assets,
            core_only,
            inside,
            effects,
        })
    }
}
fn outer(options: &Options) -> Result<()> {
    ensure(
        !options.output.exists(),
        format!("Fresh output required: {}", options.output.display()),
    )?;
    fs::create_dir_all(&options.output)?;
    let runtime = tempfile::Builder::new().prefix("sway-frame-").tempdir()?;
    fs::set_permissions(runtime.path(), fs::Permissions::from_mode(0o700))?;
    let config = options.output.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            runtime.path().display()
        ),
    )?;
    let mut cmd = Command::new("dbus-run-session");
    cmd.arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--sway")
        .arg(&options.sway)
        .arg("--output")
        .arg(&options.output)
        .arg("--pointer-tool")
        .arg(&options.pointer)
        .arg("--assets")
        .arg(&options.assets);
    if let Some(launcher) = &options.launcher {
        cmd.arg("--launcher").arg(launcher);
    }
    if options.core_only {
        cmd.arg("--core-only");
    }
    if options.effects {
        cmd.arg("--effects");
    }
    cmd.env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env_remove("AT_SPI_BUS_ADDRESS")
        .env("XDG_RUNTIME_DIR", runtime.path());
    for (key, value) in [
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        cmd.env(key, value);
    }
    let status = cmd.status()?;
    ensure(
        status.success(),
        format!("Private window frame fixture exited {status}"),
    )
}
fn env0(program: &str, env: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
    let bytes = output(program, &["--print-env0"], env)?;
    Ok(bytes
        .split(|v| *v == 0)
        .filter_map(|part| {
            part.iter().position(|v| *v == b'=').map(|at| {
                (
                    String::from_utf8_lossy(&part[..at]).into_owned(),
                    String::from_utf8_lossy(&part[at + 1..]).into_owned(),
                )
            })
        })
        .collect())
}
fn output(program: &str, args: &[&str], env: &BTreeMap<String, String>) -> Result<Vec<u8>> {
    let result = Command::new("timeout")
        .arg("10s")
        .arg(program)
        .args(args)
        .env_clear()
        .envs(env)
        .output()?;
    ensure(
        result.status.success(),
        format!(
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(result.stdout)
}
fn call(program: &str, args: &[&str], env: &BTreeMap<String, String>) -> Result<String> {
    Ok(String::from_utf8(output(program, args, env)?)?
        .trim()
        .to_owned())
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let until = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => {}
            Err(err) => last = err.to_string(),
        };
        if Instant::now() >= until {
            return Err(fail(format!("Timed out waiting for {label}: {last}")));
        }
        nap(100);
    }
}
fn matching(root: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    fs::read_dir(root)
        .ok()?
        .filter_map(|v| v.ok().map(|v| v.path()))
        .find(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with(prefix) && !n.ends_with(suffix)
            })
        })
}
fn descendants(node: &Value, all: &mut Vec<Value>) {
    if node.get("app_id").is_some_and(|v| !v.is_null())
        || node.get("window").is_some_and(|v| !v.is_null())
    {
        all.push(node.clone());
    }
    for field in ["nodes", "floating_nodes"] {
        if let Some(children) = node[field].as_array() {
            for child in children {
                descendants(child, all);
            }
        }
    }
}
struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        program: &str,
        args: &[&str],
        env: &BTreeMap<String, String>,
        log: &Path,
        interactive: bool,
    ) -> Result<usize> {
        let file = File::create(log)?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(env)
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
    fn write(&mut self, index: usize, text: &str) -> Result<()> {
        let stream = self.0[index].stdin.as_mut().ok_or("Process has no stdin")?;
        stream.write_all(text.as_bytes())?;
        stream.flush()?;
        Ok(())
    }
    fn terminate(&mut self, index: usize) -> Result<()> {
        let child = &mut self.0[index];
        if child.try_wait()?.is_none() {
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGTERM);
            }
            let _ = child.wait();
        }
        Ok(())
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
                nap(20);
            }
        }
    }
}
struct Fixture {
    out: PathBuf,
    env: BTreeMap<String, String>,
    children: Children,
    sway: PathBuf,
    assets: PathBuf,
    pointer: PathBuf,
    effects: bool,
}
impl Fixture {
    fn new(options: &Options) -> Result<Self> {
        let out = fs::canonicalize(&options.output)?;
        let mut env = env::vars().collect::<BTreeMap<_, _>>();
        for (key, value) in [
            ("GSETTINGS_BACKEND", "keyfile"),
            ("GTK_THEME", "WhiteSur-Light"),
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
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("GDK_BACKEND", "wayland"),
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
        ] {
            env.insert(key.into(), value.into());
        }
        for key in ["SWAYSOCK", "WAYLAND_DISPLAY", "DISPLAY"] {
            env.remove(key);
        }
        for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
            env.insert(
                format!("XDG_{key}_HOME"),
                s(&out.join(format!("test-{}", key.to_lowercase()))),
            );
        }
        if let Some(launcher) = &options.launcher {
            env.extend(env0(&s(launcher), &env)?);
            for (key, value) in [
                ("QT_ACCESSIBILITY", "0"),
                ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
                ("NO_AT_BRIDGE", "1"),
                ("GTK_A11Y", "none"),
                ("QT_QUICK_BACKEND", "software"),
                ("QT_QPA_PLATFORMTHEME", "generic"),
            ] {
                env.insert(key.into(), value.into());
            }
        }
        let appearance = which(
            "lumen-appearance",
            env.get("PATH").map(String::as_str).unwrap_or(""),
        )?;
        env.extend(env0(&s(&appearance), &env)?);
        let settings = PathBuf::from(&env["XDG_CONFIG_HOME"]).join("gtk-3.0/settings.ini");
        fs::create_dir_all(settings.parent().unwrap())?;
        fs::write(
            &settings,
            "[Settings]\ngtk-decoration-layout=:\ngtk-theme-name=WhiteSur-Light\n",
        )?;
        let profile = out.join("dconf-profile");
        fs::write(&profile, "user-db:frame-test\n")?;
        env.insert("DCONF_PROFILE".into(), s(&profile));
        let mut config = String::from(
            "title_align center\nfont Interlude Variable Medium 10\noutput * resolution 1280x900\ndefault_border normal 1\ndefault_floating_border normal 1\nclient.focused #b8b8bd #f5f5f7 #202024 #b8b8bd #b8b8bd\nfor_window [all] border normal 1\n",
        );
        if options.effects {
            config.push_str("corner_radius 12\nshadows enable\nshadow_blur_radius 24\nshadow_color #00000050\nanimation_duration_ms 0\n");
        }
        fs::write(out.join("test.conf"), config)?;
        Ok(Self {
            out,
            env,
            children: Children(Vec::new()),
            sway: options.sway.clone(),
            assets: options.assets.clone(),
            pointer: options.pointer.clone(),
            effects: options.effects,
        })
    }
    fn launch(
        &mut self,
        program: &str,
        args: &[&str],
        log: &str,
        interactive: bool,
    ) -> Result<usize> {
        self.children
            .launch(program, args, &self.env, &self.out.join(log), interactive)
    }
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        call(program, args, &self.env)
    }
    fn ipc(&self, command: &str) -> Result<()> {
        let reply: Value = serde_json::from_str(&self.call("swaymsg", &[command])?)?;
        ensure(
            reply
                .as_array()
                .is_some_and(|rows| rows.iter().all(|r| r["success"] == true)),
            format!("Sway rejected {command}: {reply}"),
        )?;
        nap(200);
        Ok(())
    }
    fn tree(&self) -> Result<Value> {
        serde_json::from_str(&self.call("swaymsg", &["-t", "get_tree", "-r"])?).map_err(Into::into)
    }
    fn leaves(&self) -> Result<Vec<Value>> {
        let mut all = Vec::new();
        descendants(&self.tree()?, &mut all);
        Ok(all)
    }
    fn named(&self, name: &str) -> Result<Value> {
        self.leaves()?
            .into_iter()
            .find(|n| n["name"] == name)
            .ok_or_else(|| fail(format!("Missing window {name}")))
    }
    fn wait_named(&self, name: &str) -> Result<Value> {
        wait(name, || {
            Ok(self.leaves()?.into_iter().find(|n| n["name"] == name))
        })
    }
    fn click(&self, x: i64, y: i64) -> Result<()> {
        self.ipc(&format!("seat seat0 cursor set {x} {y}"))?;
        self.ipc("seat seat0 cursor press button1")?;
        self.ipc("seat seat0 cursor release button1")
    }
    fn grim(&self, name: &str) -> Result<()> {
        self.call("grim", &[&s(&self.out.join(name))])?;
        Ok(())
    }
    fn buttons(&self, name: &str, node: &Value, scale: i64) -> Result<()> {
        let r = &node["deco_rect"];
        let y = (number(r, "y")? + 16) * scale;
        let image = RgbImage::read(&self.out.join(name))?;
        let colors = (0..3)
            .map(|i| image.pixel((number(r, "x").unwrap_or(0) + 20 + i * 20) * scale, y))
            .collect::<Vec<_>>();
        ensure(
            colors == vec![[255, 95, 87], [254, 188, 46], [40, 200, 64]],
            format!("Unexpected native button colors: {colors:?}"),
        )
    }
    fn menu_status(&self, fixture: &Path) -> Result<Value> {
        serde_json::from_str(&self.call(
            "quickshell",
            &["-p", &s(fixture), "ipc", "call", "fixture", "status"],
        )?)
        .map_err(Into::into)
    }
}
fn which(name: &str, path: &str) -> Result<PathBuf> {
    for folder in env::split_paths(path) {
        let candidate = folder.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("{name} missing from pinned PATH")))
}
fn number(node: &Value, key: &str) -> Result<i64> {
    node[key]
        .as_i64()
        .ok_or_else(|| fail(format!("Missing integer {key}: {node}")))
}
fn truth(node: &Value, key: &str) -> bool {
    node[key] == true
}
struct RgbImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}
impl RgbImage {
    fn read(path: &Path) -> Result<Self> {
        let mut decoder = Decoder::new(BufReader::new(File::open(path)?));
        decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut data = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut data)?;
        let channels = match info.color_type {
            ColorType::Rgb => 3,
            ColorType::Rgba => 4,
            ColorType::Grayscale => 1,
            ColorType::GrayscaleAlpha => 2,
            _ => return Err(fail("Unsupported PNG format")),
        };
        let mut pixels = Vec::new();
        for part in data[..info.buffer_size()].chunks_exact(channels) {
            if channels <= 2 {
                pixels.extend_from_slice(&[part[0], part[0], part[0]])
            } else {
                pixels.extend_from_slice(&part[..3])
            }
        }
        Ok(Self {
            width: info.width as usize,
            height: info.height as usize,
            pixels,
        })
    }
    fn pixel(&self, x: i64, y: i64) -> [u8; 3] {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return [0; 3];
        }
        let p = (y as usize * self.width + x as usize) * 3;
        [self.pixels[p], self.pixels[p + 1], self.pixels[p + 2]]
    }
}
fn main() {
    let result = (|| -> Result<()> {
        if env::args_os().any(|arg| arg == "--sleep") {
            nap(600_000);
            return Ok(());
        }
        let options = Options::parse()?;
        if options.inside {
            inner(&options)
        } else {
            outer(&options)
        }
    })();
    if let Err(e) = result {
        eprintln!("window frames: {e}");
        std::process::exit(1)
    }
}

fn inner(options: &Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    let config = f.out.join("test.conf");
    let sway = s(&f.sway);
    f.launch(
        &sway,
        &["--unsupported-gpu", "-c", &s(&config)],
        "nested-sway.log",
        false,
    )?;
    let runtime = PathBuf::from(&f.env["XDG_RUNTIME_DIR"]);
    let sock = wait("Sway socket", || {
        Ok(matching(&runtime, "sway-ipc.", ".lock"))
    })?;
    f.env.insert("SWAYSOCK".into(), s(&sock));
    wait("native compositor", || {
        Ok((serde_json::from_str::<Value>(&f.call("swaymsg",&["-t","get_version","-r"])?)?["native_window_controls"]==true).then_some(()))
    })?;
    f.ipc("lumen_window margins disable")?;
    let wayland = wait("Wayland socket", || {
        Ok(matching(&runtime, "wayland-", ".lock"))
    })?;
    f.env.insert(
        "WAYLAND_DISPLAY".into(),
        wayland.file_name().unwrap().to_string_lossy().into_owned(),
    );
    let sleeper = s(&env::current_exe()?);
    f.launch(
        "foot",
        &["--title=Frame Test", &sleeper, "--sleep"],
        "foot.log",
        false,
    )?;
    let first = f.wait_named("Frame Test")?;
    ensure(
        number(&first["deco_rect"], "height")? == 32,
        format!("Native frame height: {first}"),
    )?;
    f.ipc("seat seat0 cursor set 110 16")?;
    f.grim("native-frame.png")?;
    f.buttons("native-frame.png", &first, 1)?;
    f.ipc("seat seat0 cursor set 20 16")?;
    f.ipc("seat seat0 cursor press button1")?;
    f.ipc("seat seat0 cursor set 110 16")?;
    f.ipc("seat seat0 cursor release button1")?;
    f.named("Frame Test")?;
    f.ipc("output HEADLESS-1 resolution 2560x1800 scale 2")?;
    f.grim("native-frame-2x.png")?;
    f.buttons("native-frame-2x.png", &f.named("Frame Test")?, 2)?;
    f.ipc("output HEADLESS-1 resolution 1280x900 scale 1")?;
    let original = f.named("Frame Test")?;
    for (action, expected) in [
        ("left", [0, 0, 640, 900]),
        ("right", [640, 0, 640, 900]),
        ("top", [0, 0, 1280, 450]),
        ("bottom", [0, 450, 1280, 450]),
        ("top-left", [0, 0, 640, 450]),
        ("top-right", [640, 0, 640, 450]),
        ("bottom-left", [0, 450, 640, 450]),
        ("bottom-right", [640, 450, 640, 450]),
    ] {
        f.ipc(&format!("[title=\"^Frame Test$\"] lumen_window {action}"))?;
        let placed = f.named("Frame Test")?;
        let r = &placed["rect"];
        let dh = number(&placed["deco_rect"], "height")?;
        let actual = [
            number(r, "x")?,
            number(r, "y")? - dh,
            number(r, "width")?,
            number(r, "height")? + dh,
        ];
        ensure(
            actual == expected,
            format!("{action} placed {actual:?}, expected {expected:?}"),
        )?;
        f.ipc(&format!("[title=\"^Frame Test$\"] lumen_window {action}"))?;
        ensure(
            f.named("Frame Test")?["rect"] == placed["rect"],
            format!("{action} not idempotent"),
        )?;
    }
    for _ in 0..2 {
        f.ipc("[title=\"^Frame Test$\"] lumen_window restore")?;
        ensure(
            f.named("Frame Test")?["rect"] == original["rect"],
            "Original placement not restored",
        )?;
    }
    let ticks = f.out.join("layout-events.jsonl");
    f.launch(
        "swaymsg",
        &["-r", "-m", "-t", "subscribe", "[\"tick\"]"],
        "layout-events.jsonl",
        false,
    )?;
    nap(200);
    let pointer_path = s(&f.pointer);
    let pointer = f.launch(
        &pointer_path,
        &["pointer", "move", "0", "0"],
        "pointer.log",
        true,
    )?;
    nap(200);
    f.ipc("seat seat0 cursor set 200 100")?;
    f.children.write(pointer, "m -140 -84\n")?;
    nap(300);
    let mut payloads = Vec::new();
    for line in fs::read_to_string(&ticks)?
        .lines()
        .filter(|line| line.starts_with('{'))
    {
        if let Ok(event) = serde_json::from_str::<Value>(line) {
            if let Some(payload) = event["payload"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
            {
                payloads.push(payload)
            }
        }
    }
    ensure(
        payloads
            .iter()
            .any(|p| p["lumen_layout"] == 1 && p["id"] == original["id"]),
        format!("Green hover lacked scoped layout event: {payloads:?}"),
    )?;
    f.ipc("seat seat0 cursor set 200 100")?;
    println!(
        "PASS: native half/quarter layouts are idempotent, original placement restores, green hover emits scoped event."
    );
    f.ipc("lumen_window margins enable")?;
    f.ipc("[title=\"^Frame Test$\"] lumen_window left")?;
    let margined = f.named("Frame Test")?;
    ensure(
        number(&margined["rect"], "x")? == 8 && number(&margined["rect"], "width")? == 628,
        format!("Margins: {}", margined["rect"]),
    )?;
    f.ipc("[title=\"^Frame Test$\"] lumen_window restore")?;
    f.ipc("lumen_window margins disable")?;
    f.ipc("seat seat0 cursor set 110 16")?;
    f.children.write(pointer, "m 0 0\np\nr\np\nr\n")?;
    nap(300);
    ensure(
        truth(&f.named("Frame Test")?["lumen_state"], "filled"),
        "Double click failed to fill",
    )?;
    f.ipc("[title=\"^Frame Test$\"] lumen_window restore")?;
    println!("PASS: optional layout margins and native titlebar double-click Fill work.");
    let menu = f.out.join("layout-menu-fixture.qml");
    let assets = fs::canonicalize(&f.assets)?;
    fs::write(
        &menu,
        format!(
            "import QtQuick\nimport Quickshell\nimport Quickshell.Io\nimport \"file:{}\" as D\nShellRoot {{\n QtObject {{id:stub;property string popupOwner:\"\";signal dismissPopups(string owner);function claimPopup(owner){{popupOwner=owner;dismissPopups(owner);}}}}\n D.WindowLayoutMenu {{id:menu;shell:stub}}\n IpcHandler {{target:\"fixture\";function status():string{{return JSON.stringify({{visible:menu.visible,target:menu.target,supported:menu.supported}});}}}}\n}}\n",
            assets.display()
        ),
    )?;
    let menu_id = f.launch("quickshell", &["-p", &s(&menu)], "layout-menu.log", false)?;
    wait("layout menu supported", || {
        Ok(truth(&f.menu_status(&menu)?, "supported").then_some(()))
    })?;
    f.ipc("seat seat0 cursor set 200 100")?;
    f.children.write(pointer, "m -140 -84\n")?;
    wait("green hover menu", || {
        Ok(truth(&f.menu_status(&menu)?, "visible").then_some(()))
    })?;
    nap(700);
    ensure(
        truth(&f.menu_status(&menu)?, "visible"),
        "Hover menu closed on green",
    )?;
    f.children.write(pointer, "m 30 54\n")?;
    nap(500);
    ensure(
        truth(&f.menu_status(&menu)?, "visible"),
        "Hover menu closed across gap",
    )?;
    f.grim("layout-menu.png")?;
    f.children.write(pointer, "p\nr\n")?;
    wait("menu Fill", || {
        Ok(truth(&f.named("Frame Test")?["lumen_state"], "filled").then_some(()))
    })?;
    ensure(
        !truth(&f.menu_status(&menu)?, "visible"),
        "Layout menu remained open",
    )?;
    f.ipc("[title=\"^Frame Test$\"] lumen_window restore")?;
    f.children.terminate(menu_id)?;
    println!(
        "PASS: real green menu stays open at trigger and across pointer gap; captured-window Fill commits."
    );
    f.click(60, 16)?;
    ensure(
        number(&f.named("Frame Test")?, "fullscreen_mode")? != 0,
        "Green button did not fullscreen",
    )?;
    f.ipc("[title=\"^Frame Test$\"] lumen_window fullscreen")?;
    f.ipc("[title=\"^Frame Test$\"] lumen_window fill")?;
    let filled = f.named("Frame Test")?;
    ensure(
        number(&filled, "fullscreen_mode")? == 0
            && number(&filled["deco_rect"], "height")? == 32
            && filled["type"] == "floating_con",
        format!("Fill state: {filled}"),
    )?;
    f.ipc("[title=\"^Frame Test$\"] lumen_window fill")?;
    let restored = f.named("Frame Test")?;
    ensure(
        restored["type"] == original["type"] && restored["rect"] == original["rect"],
        "Fill did not restore original placement",
    )?;
    f.click(40, 16)?;
    ensure(
        f.named("Frame Test")?["scratchpad_state"] != "none",
        "Yellow button did not minimize",
    )?;
    f.ipc("[title=\"^Frame Test$\"] scratchpad show")?;
    let restored = f.named("Frame Test")?;
    ensure(
        restored["type"] == original["type"] && restored["rect"] == original["rect"],
        "Minimize did not restore placement",
    )?;
    let n = f.named("Frame Test")?;
    fs::write(
        f.out.join("restored.json"),
        serde_json::to_string_pretty(&n)?,
    )?;
    f.grim("restored.png")?;
    let r = &n["deco_rect"];
    f.click(number(r, "x")? + 20, number(r, "y")? + 16)?;
    wait("frame close", || {
        Ok(f.leaves()?
            .iter()
            .all(|n| n["name"] != "Frame Test")
            .then_some(()))
    })?;
    println!(
        "PASS: native title bar is 32px; close, minimise/restore placement fullscreen and reversible fill work."
    );
    f.launch(
        "foot",
        &["--title=Floating Frame Test", &sleeper, "--sleep"],
        "floating-foot.log",
        false,
    )?;
    f.wait_named("Floating Frame Test")?;
    f.ipc("[title=\"^Floating Frame Test$\"] floating enable, resize set width 500 px height 320 px, move position 150 px 170 px")?;
    wait("floating placement", || {
        Ok(f.leaves()?.into_iter().find(|n| {
            n["name"] == "Floating Frame Test"
                && n["type"] == "floating_con"
                && number(&n["rect"], "width").unwrap_or(1000) < 600
                && number(&n["rect"], "height").unwrap_or(1000) < 450
        }))
    })?;
    let before = f.named("Floating Frame Test")?;
    f.ipc("[title=\"^Floating Frame Test$\"] lumen_window minimize")?;
    f.ipc("workspace 2")?;
    f.ipc("[title=\"^Floating Frame Test$\"] scratchpad show")?;
    let after = f.named("Floating Frame Test")?;
    ensure(
        after["type"] == before["type"] && after["rect"] == before["rect"],
        "Floating minimize did not restore geometry",
    )?;
    f.ipc("[title=\"^Floating Frame Test$\"] lumen_window fill")?;
    ensure(
        number(&f.named("Floating Frame Test")?["deco_rect"], "height")? == 32,
        "Floating Fill lost native frame",
    )?;
    f.ipc("[title=\"^Floating Frame Test$\"] lumen_window fill")?;
    ensure(
        f.named("Floating Frame Test")?["rect"] == before["rect"],
        "Floating Fill did not restore geometry",
    )?;
    f.ipc("[title=\"^Floating Frame Test$\"] lumen_window minimize")?;
    f.ipc("output HEADLESS-1 resolution 640x480 scale 1")?;
    f.ipc("[title=\"^Floating Frame Test$\"] scratchpad show")?;
    let limited = f.named("Floating Frame Test")?["rect"].clone();
    let x = number(&limited, "x")?;
    let y = number(&limited, "y")?;
    let w = number(&limited, "width")?;
    let h = number(&limited, "height")?;
    ensure(
        x >= 0 && y >= 0 && x + w <= 640 && y + h <= 480,
        format!("Floating restore outside small work area: {limited}"),
    )?;
    f.ipc("output HEADLESS-1 resolution 1280x900 scale 1")?;
    println!("PASS: restore constrains floating placement to a resized surviving work area.");
    f.ipc("[title=\"^Floating Frame Test$\"] lumen_window close")?;
    wait("floating close", || {
        Ok(f.leaves()?.is_empty().then_some(()))
    })?;
    println!("PASS: floating minimize/fill restore geometry after a workspace change.");
    f.launch(
        "foot",
        &["--title=Tiled A", &sleeper, "--sleep"],
        "tile-a.log",
        false,
    )?;
    f.wait_named("Tiled A")?;
    f.launch(
        "foot",
        &["--title=Tiled B", &sleeper, "--sleep"],
        "tile-b.log",
        false,
    )?;
    f.wait_named("Tiled B")?;
    let before = rects(&f.leaves()?);
    f.ipc("[title=\"^Tiled A$\"] lumen_window minimize")?;
    f.ipc("[title=\"^Tiled A$\"] scratchpad show")?;
    ensure(
        rects(&f.leaves()?) == before,
        "Tiled sibling placements changed after minimize/restore",
    )?;
    f.ipc("[title=\"^Tiled A$\"] lumen_window minimize")?;
    f.ipc("[title=\"^Tiled [AB]$\"] lumen_window hide")?;
    let hidden = named_map(&f.leaves()?);
    ensure(
        truth(&hidden["Tiled A"]["lumen_state"], "minimised")
            && truth(&hidden["Tiled A"]["lumen_state"], "hidden"),
        "Tiled A hide lost minimization",
    )?;
    ensure(
        !truth(&hidden["Tiled B"]["lumen_state"], "minimised")
            && truth(&hidden["Tiled B"]["lumen_state"], "hidden"),
        "Tiled B hide state incorrect",
    )?;
    f.ipc("[title=\"^Tiled [AB]$\"] lumen_window unhide")?;
    let shown = named_map(&f.leaves()?);
    ensure(
        shown["Tiled A"]["scratchpad_state"] != "none"
            && truth(&shown["Tiled A"]["lumen_state"], "minimised"),
        "Tiled A unhide state incorrect",
    )?;
    ensure(
        shown["Tiled B"]["scratchpad_state"] == "none"
            && !truth(&shown["Tiled B"]["lumen_state"], "hidden"),
        "Tiled B unhide state incorrect",
    )?;
    let desktop_before = shown["Tiled B"]["rect"].clone();
    f.ipc("lumen_window show-desktop-on")?;
    let desktop = named_map(&f.leaves()?);
    ensure(
        truth(&desktop["Tiled B"]["lumen_state"], "desktop_hidden")
            && !truth(&desktop["Tiled B"]["lumen_state"], "minimised"),
        "Desktop reveal lost Tiled B state",
    )?;
    ensure(
        truth(&desktop["Tiled A"]["lumen_state"], "minimised")
            && !truth(&desktop["Tiled A"]["lumen_state"], "desktop_hidden"),
        "Desktop reveal lost Tiled A state",
    )?;
    f.ipc("lumen_window show-desktop-on")?;
    f.ipc("lumen_window show-desktop-off")?;
    f.ipc("lumen_window show-desktop-off")?;
    let desktop = named_map(&f.leaves()?);
    ensure(
        desktop["Tiled B"]["rect"] == desktop_before
            && !truth(&desktop["Tiled B"]["lumen_state"], "desktop_hidden")
            && truth(&desktop["Tiled A"]["lumen_state"], "minimised"),
        "Desktop reveal idempotence failed",
    )?;
    f.ipc("lumen_window show-desktop-on")?;
    f.ipc("[title=\"^Tiled B$\"] scratchpad show")?;
    ensure(
        !truth(&f.named("Tiled B")?["lumen_state"], "desktop_hidden"),
        "Tiled B activation did not reveal desktop",
    )?;
    f.ipc("[title=\"^Tiled B$\"] lumen_window fullscreen")?;
    f.ipc("lumen_window show-desktop-on")?;
    f.ipc("lumen_window show-desktop-off")?;
    ensure(
        number(&f.named("Tiled B")?, "fullscreen_mode")? != 0,
        "Fullscreen lost during desktop reveal",
    )?;
    f.ipc("[title=\"^Tiled B$\"] lumen_window fullscreen")?;
    f.ipc("lumen_window show-desktop-on")?;
    f.launch(
        "foot",
        &["--title=Desktop Reveal New", &sleeper, "--sleep"],
        "desktop-new.log",
        false,
    )?;
    f.wait_named("Desktop Reveal New")?;
    ensure(
        !truth(&f.named("Tiled B")?["lumen_state"], "desktop_hidden"),
        "New app failed to reveal desktop",
    )?;
    f.ipc("[title=\"^Desktop Reveal New$\"] lumen_window close")?;
    println!(
        "PASS: reversible desktop reveal preserves minimisation/fullscreen; activation and new app windows restore the desktop."
    );
    f.ipc("create_output")?;
    let outputs: Value = serde_json::from_str(&f.call("swaymsg", &["-t", "get_outputs", "-r"])?)?;
    let second = outputs
        .as_array()
        .and_then(|all| all.iter().find(|v| v["name"] != "HEADLESS-1"))
        .and_then(|v| v["name"].as_str())
        .ok_or("Second output missing")?
        .to_owned();
    f.ipc(&format!("output {second} position 1280 0"))?;
    f.ipc(&format!("focus output {second}"))?;
    f.launch(
        "foot",
        &["--title=Desktop Second Display", &sleeper, "--sleep"],
        "desktop-second.log",
        false,
    )?;
    f.wait_named("Desktop Second Display")?;
    f.ipc("focus output HEADLESS-1")?;
    let visible_before = f
        .leaves()?
        .into_iter()
        .filter(|n| n["name"] == "Tiled B" || n["name"] == "Desktop Second Display")
        .map(|n| (n["name"].as_str().unwrap().to_owned(), n["rect"].clone()))
        .collect::<BTreeMap<_, _>>();
    f.ipc("lumen_window show-desktop-on")?;
    let visible = named_map(&f.leaves()?);
    ensure(
        visible_before
            .keys()
            .all(|name| truth(&visible[name]["lumen_state"], "desktop_hidden")),
        "Two-output reveal failed",
    )?;
    f.ipc("lumen_window show-desktop-off")?;
    let visible = named_map(&f.leaves()?);
    ensure(
        visible_before.iter().all(|(name, rect)| {
            !truth(&visible[name]["lumen_state"], "desktop_hidden")
                && visible[name]["rect"] == *rect
        }),
        "Two-output restore failed",
    )?;
    f.ipc("[title=\"^Desktop Second Display$\"] lumen_window close")?;
    f.ipc(&format!("output {second} disable"))?;
    f.ipc("focus output HEADLESS-1")?;
    println!("PASS: desktop reveal restores visible windows independently on two outputs.");
    f.ipc("lumen_window show-desktop-on")?;
    f.ipc("[title=\"^Tiled B$\"] lumen_window hide")?;
    f.ipc("lumen_window show-desktop-off")?;
    let changed = f.named("Tiled B")?;
    ensure(
        truth(&changed["lumen_state"], "hidden")
            && !truth(&changed["lumen_state"], "desktop_hidden")
            && changed["scratchpad_state"] != "none",
        "Hide during desktop reveal failed",
    )?;
    f.ipc("[title=\"^Tiled B$\"] lumen_window unhide")?;
    f.ipc("lumen_window show-desktop-on")?;
    f.ipc("[title=\"^Tiled B$\"] lumen_window minimize")?;
    f.ipc("lumen_window show-desktop-off")?;
    let changed = f.named("Tiled B")?;
    ensure(
        truth(&changed["lumen_state"], "minimised")
            && !truth(&changed["lumen_state"], "desktop_hidden")
            && changed["scratchpad_state"] != "none",
        "Minimize during desktop reveal failed",
    )?;
    f.ipc("[title=\"^Tiled B$\"] scratchpad show")?;
    println!(
        "PASS: hide/minimise requests during desktop reveal persist after reveal restoration."
    );
    f.ipc("[title=\"^Tiled A$\"] scratchpad show")?;
    ensure(
        rects(&f.leaves()?) == before,
        "Original tiled placements not restored",
    )?;
    f.ipc("[title=\"^Tiled [AB]$\"] lumen_window close")?;
    wait("tiled close", || Ok(f.leaves()?.is_empty().then_some(())))?;
    println!("PASS: hide/unhide keeps independent minimisation and original placements.");
    println!("PASS: minimizing one tiled sibling restores both placements.");
    if options.core_only {
        return Ok(());
    }
    full_variant(&mut f, options)
}
fn named_map(nodes: &[Value]) -> BTreeMap<String, Value> {
    nodes
        .iter()
        .filter_map(|n| n["name"].as_str().map(|name| (name.to_owned(), n.clone())))
        .collect()
}
fn rects(nodes: &[Value]) -> BTreeMap<String, Value> {
    nodes
        .iter()
        .filter_map(|n| {
            n["name"]
                .as_str()
                .map(|name| (name.to_owned(), n["rect"].clone()))
        })
        .collect()
}
fn full_variant(f: &mut Fixture, _options: &Options) -> Result<()> {
    f.call(
        "gsettings",
        &[
            "set",
            "org.gnome.desktop.wm.preferences",
            "button-layout",
            ":",
        ],
    )?;
    f.launch("evince", &["--new-window"], "gtk.log", false)?;
    wait("GTK application", || {
        Ok(f.leaves()?
            .iter()
            .any(|n| n["app_id"] == "org.gnome.Evince")
            .then_some(()))
    })?;
    f.grim("gtk-frame.png")?;
    fs::write(
        f.out.join("gtk-tree.json"),
        serde_json::to_string_pretty(&f.leaves()?)?,
    )?;
    println!("PASS: GTK header-bar application loads under the common frame.");
    f.ipc("[app_id=\"org.gnome.Evince\"] kill")?;
    wait("GTK close", || Ok(f.leaves()?.is_empty().then_some(())))?;

    let qt = f.out.join("qt.qml");
    fs::write(
        &qt,
        "import QtQuick\nimport Quickshell\nimport Quickshell.Wayland\nShellRoot { PanelWindow { anchors {top:true;bottom:true;left:true;right:true} exclusionMode:ExclusionMode.Ignore; WlrLayershell.layer:WlrLayer.Background;color:\"#808080\" } FloatingWindow { title:\"Qt Frame Test\"; visible:true; implicitWidth:600; implicitHeight:400; Rectangle {anchors.fill:parent;color:\"#f2f2f4\"} } }",
    )?;
    use std::fs::OpenOptions;
    OpenOptions::new().append(true).open(f.out.join("test.conf"))?.write_all(b"for_window [title=\"^Qt Frame Test$\"] floating enable, resize set width 600 px height 400 px, move position center\n")?;
    f.ipc("reload")?;
    f.launch("quickshell", &["-p", &s(&qt)], "qt.log", false)?;
    f.wait_named("Qt Frame Test")?;
    nap(400);
    f.grim("qt-frame.png")?;
    if f.effects {
        let node = f.named("Qt Frame Test")?;
        let rect = &node["rect"];
        let x = number(rect, "x")?;
        let y = number(rect, "y")? - number(&node["deco_rect"], "height")?;
        let image = RgbImage::read(&f.out.join("qt-frame.png"))?;
        let corner = image.pixel(x, y)[0] as f64 / 255.0;
        let shadow = image.pixel(x - 6, number(rect, "y")? + 100)[0] as f64 / 255.0;
        ensure(
            (corner - 128.0 / 255.0).abs() < 0.04,
            format!("Rounded corner failed: {corner} {node}"),
        )?;
        ensure(
            shadow > 0.1 && shadow < 0.49,
            format!("Shadow failed: {shadow} {node}"),
        )?;
        println!("PASS: rounded corner reveals the background and floating shadow darkens it.");
    }
    f.ipc("[title=\"^Qt Frame Test$\"] kill")?;
    wait("Qt close", || Ok(f.leaves()?.is_empty().then_some(())))?;

    let utility = f.out.join("utility.qml");
    fs::write(
        &utility,
        "import QtQuick\nimport Quickshell\nShellRoot { FloatingWindow {title:\"Window Overview\";visible:true;implicitWidth:400;implicitHeight:300;Rectangle{anchors.fill:parent;color:\"#f2f2f4\"}}}",
    )?;
    f.launch("quickshell", &["-p", &s(&utility)], "utility.log", false)?;
    f.wait_named("Window Overview")?;
    let before = wait("utility placement", || {
        Ok(f.leaves()?.into_iter().find(|n| {
            n["name"] == "Window Overview"
                && n["rect"]["width"] == 1280
                && n["rect"]["height"] == 868
        }))
    })?;
    f.ipc("[title=\"^Window Overview$\"] lumen_window minimize")?;
    f.ipc("[title=\"^Window Overview$\"] lumen_window fill")?;
    let after = f.named("Window Overview")?;
    ensure(
        after["scratchpad_state"] == "none"
            && after["rect"] == before["rect"]
            && after["fullscreen_mode"] == 0,
        format!("Utility accepted unsupported action: {after}"),
    )?;
    f.ipc("[title=\"^Window Overview$\"] lumen_window close")?;
    wait("utility close", || Ok(f.leaves()?.is_empty().then_some(())))?;
    println!("PASS: utility windows reject unsupported minimize/fill actions.");

    let home = f.out.join("test-home");
    fs::create_dir_all(&home)?;
    f.env.insert("HOME".into(), s(&home));
    let obsidian = PathBuf::from(&f.env["XDG_CONFIG_HOME"]).join("obsidian/obsidian.json");
    fs::create_dir_all(obsidian.parent().ok_or("Obsidian config has no parent")?)?;
    fs::write(&obsidian, "{\"frame\":\"native\",\"updateDisabled\":true}")?;
    f.launch(
        "obsidian",
        &["--ozone-platform=wayland", "--disable-gpu"],
        "electron.log",
        false,
    )?;
    let leaves = wait("Electron application", || {
        Ok((!f.leaves()?.is_empty()).then_some(f.leaves()?))
    })?;
    f.grim("electron-frame.png")?;
    fs::write(
        f.out.join("electron-tree.json"),
        serde_json::to_string_pretty(&leaves)?,
    )?;
    f.buttons("electron-frame.png", &leaves[0], 1)?;
    println!(
        "PASS: Qt and Electron native-frame applications load; all three traffic lights render."
    );
    Ok(())
}
