//! Private native appearance controls and dialog regression.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn wait(label: &str, mut condition: impl FnMut() -> Result<bool>) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if condition().unwrap_or(false) {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(fail(format!("Timed out: {label}")));
        }
        pause(75);
    }
}
fn unique_root() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    PathBuf::from("/tmp").join(format!("lqa-{}-{stamp}", std::process::id()))
}
#[derive(Clone)]
struct Options {
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    assets: Option<PathBuf>,
    inside: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut output = None;
        let mut launcher = None;
        let mut sway = None;
        let mut assets = None;
        let mut inside = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
                Some("--assets") => {
                    assets = Some(PathBuf::from(args.next().ok_or("Missing assets")?))
                }
                Some("--inside") => inside = true,
                _ => return Err(fail("Unknown appearance fixture option")),
            }
        }
        Ok(Self {
            output: env::current_dir()?.join(output.ok_or("--output required")?),
            launcher: launcher.ok_or("--launcher required")?.canonicalize()?,
            sway: sway.ok_or("--sway required")?.canonicalize()?,
            assets,
            inside,
        })
    }
}
fn private_session() -> Result<()> {
    let root = unique_root();
    fs::create_dir_all(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let mut command = Command::new("dbus-run-session");
    command.arg("--").arg(env::current_exe()?);
    command.args(env::args_os().skip(1)).arg("--inside");
    command.env("HOME", &root).env("XDG_RUNTIME_DIR", &root);
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        command.env_remove(key);
    }
    let status = command.status();
    let _ = fs::remove_dir_all(&root);
    require(status?.success(), "Private appearance fixture failed")
}
struct Process(Child);
impl Process {
    fn alive(&mut self) -> Result<bool> {
        Ok(self.0.try_wait()?.is_none())
    }
    fn stop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let until = Instant::now() + Duration::from_secs(4);
        while self.0.try_wait().ok().flatten().is_none() {
            if Instant::now() >= until {
                unsafe {
                    libc::killpg(self.0.id() as i32, libc::SIGKILL);
                }
                break;
            }
            pause(30);
        }
        let _ = self.0.wait();
    }
}
struct Fixture {
    options: Options,
    env: BTreeMap<String, String>,
    qml: PathBuf,
    procs: Vec<Process>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for process in self.procs.iter_mut().rev() {
            process.stop();
        }
    }
}
impl Fixture {
    fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args).env_clear().envs(&self.env);
        command
    }
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = self.command(program, args).output()?;
        if !output.status.success() {
            return Err(fail(format!(
                "{program} {:?}: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn launch(&mut self, program: &str, args: &[&str], name: &str) -> Result<usize> {
        let log = File::create(self.options.output.join(format!("{name}.log")))?;
        let mut command = self.command(program, args);
        command.stdout(log.try_clone()?).stderr(log);
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        self.procs.push(Process(command.spawn()?));
        Ok(self.procs.len() - 1)
    }
    fn ipc(&self, args: &[&str]) -> Result<String> {
        let qml = self.qml.to_str().ok_or("Non-UTF8 QML path")?;
        let mut all = vec!["-p", qml, "ipc", "call", "test"];
        all.extend(args);
        self.call("quickshell", &all)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc(&["state"])?)?)
    }
    fn screenshot(&self, name: &str) -> Result<()> {
        self.call(
            "grim",
            &[self
                .options
                .output
                .join(name)
                .to_str()
                .ok_or("Non-UTF8 output path")?],
        )?;
        Ok(())
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.call("wtype", args)?;
        Ok(())
    }
    fn focus_key(&self, method: &str, keys: &[&str]) -> Result<()> {
        let mut args = vec!["-s", "400"];
        args.extend(keys);
        let mut process = self.command("wtype", &args).spawn()?;
        pause(150);
        self.ipc(&[method])?;
        require(process.wait()?.success(), "Physical keyboard input failed")?;
        pause(100);
        Ok(())
    }
}
fn make_png(path: &Path) -> Result<()> {
    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(file, 800, 500);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let mut pixels = vec![0u8; 800 * 500 * 3];
    for pixel in pixels.chunks_exact_mut(3) {
        pixel.copy_from_slice(&[0, 122, 255]);
    }
    writer.write_image_data(&pixels)?;
    Ok(())
}
fn run(options: Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    let mut env = env::vars().collect::<BTreeMap<_, _>>();
    let launcher_output = Command::new(&options.launcher)
        .arg("--print-env0")
        .env_clear()
        .envs(&env)
        .output()?;
    require(
        launcher_output.status.success(),
        "Launcher environment failed",
    )?;
    for part in launcher_output.stdout.split(|byte| *byte == 0) {
        if let Some(index) = part.iter().position(|byte| *byte == b'=') {
            let key = String::from_utf8(part[..index].to_vec())?;
            if [
                "PATH",
                "QT_PLUGIN_PATH",
                "QML_IMPORT_PATH",
                "GLOBAL_MENU_PYTHON",
                "LUMEN_FILES_BIN",
                "LUMEN_DESKTOP_ASSETS",
            ]
            .contains(&key.as_str())
            {
                env.insert(key, String::from_utf8(part[index + 1..].to_vec())?);
            }
        }
    }
    let assets = options.assets.clone().unwrap_or_else(|| {
        PathBuf::from(env.get("LUMEN_DESKTOP_ASSETS").cloned().unwrap_or_default()).join("shell")
    });
    require(
        assets.join("FolderStack.qml").exists(),
        "Packaged shell assets required",
    )?;
    let root = PathBuf::from(env.get("XDG_RUNTIME_DIR").ok_or("Missing runtime")?);
    for (key, path) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
    ] {
        let value = options.output.join(path);
        fs::create_dir_all(&value)?;
        env.insert(key.into(), value.display().to_string());
    }
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", "pixman"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("GDK_BACKEND", "wayland"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        env.insert(key.into(), value.into());
    }
    for key in [
        "AT_SPI_BUS_ADDRESS",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
    ] {
        env.remove(key);
    }
    let font = Command::new("fc-match")
        .args(["Interlude Variable", "-f", "%{file}"])
        .output()?;
    require(font.status.success(), "Font lookup failed")?;
    let font_path = PathBuf::from(String::from_utf8(font.stdout)?.trim());
    let font_conf = options.output.join("fonts.conf");
    fs::write(
        &font_conf,
        format!(
            "<fontconfig><include>/etc/fonts/fonts.conf</include><dir>{}</dir></fontconfig>",
            font_path.parent().ok_or("Font has no parent")?.display()
        ),
    )?;
    env.insert("FONTCONFIG_FILE".into(), font_conf.display().to_string());
    fs::create_dir_all(options.output.join("state/lumen-desktop/session"))?;
    let docs = options.output.join("home/Downloads");
    fs::create_dir_all(&docs)?;
    for index in 0..20 {
        fs::write(docs.join(format!("file-{index}.txt")), "synthetic")?;
    }
    let qml = options.output.join("shell.qml");
    fs::write(
        &qml,
        include_str!("../fixture.qml").replace("@ASSETS@", &assets.display().to_string()),
    )?;
    let sway_config = options.output.join("sway.conf");
    fs::write(
        &sway_config,
        "output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n",
    )?;
    let mut fixture = Fixture {
        options,
        env,
        qml,
        procs: Vec::new(),
    };
    let sway = fixture.options.sway.display().to_string();
    let config = sway_config.display().to_string();
    fixture.launch(&sway, &["--unsupported-gpu", "-c", &config], "sway")?;
    wait("private compositor", || {
        Ok(fs::read_dir(&root)?
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name().to_string_lossy().starts_with("sway-ipc")))
    })?;
    let socket = fs::read_dir(&root)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("sway-ipc"))
        })
        .ok_or("Missing Sway socket")?;
    let display = fs::read_dir(&root)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| name.starts_with("wayland-") && !name.ends_with(".lock"))
        .ok_or("Missing Wayland display")?;
    fixture
        .env
        .insert("SWAYSOCK".into(), socket.display().to_string());
    fixture.env.insert("WAYLAND_DISPLAY".into(), display);
    let qml_path = fixture.qml.display().to_string();
    let ui = fixture.launch("quickshell", &["-p", &qml_path], "ui")?;
    wait("appearance UI", || {
        fixture.state().map(|state| state.get("clicks").is_some())
    })?;
    require(fixture.procs[ui].alive()?, "Appearance UI exited")?;
    fixture.focus_key("focusButton", &["-k", "space"])?;
    require(
        fixture.state()?["clicks"] == 1,
        "Destructive button keyboard",
    )?;
    fixture.focus_key("focusCombo", &["-k", "Down", "-k", "Return"])?;
    require(fixture.state()?["index"] == 2, "Combo keyboard selection")?;
    require(
        fixture.ipc(&["colour"])?.starts_with("PASS"),
        "Colour alpha round trip",
    )?;
    fixture.ipc(&["sessionShow", "reboot"])?;
    pause(300);
    fixture.screenshot("session-confirmation.png")?;
    fixture.call("swaymsg", &["[title=\"Session Options\"]", "focus"])?;
    fixture.key(&["-s", "200", "-k", "Escape"])?;
    require(
        fixture.state()?["sessionVisible"] == false && fixture.state()?["confirmed"] == "",
        "Session Escape",
    )?;
    fixture.ipc(&["sessionShow", "reboot"])?;
    pause(300);
    fixture.call("swaymsg", &["[title=\"Session Options\"]", "focus"])?;
    fixture.key(&["-s", "200", "-k", "Return"])?;
    require(
        fixture.state()?["sessionVisible"] == false && fixture.state()?["confirmed"] == "",
        "Session Return",
    )?;
    fixture.ipc(&["sessionShow", "logout"])?;
    fixture.ipc(&["sessionConfirm"])?;
    require(
        fixture.state()?["confirmed"] == "" && fixture.state()?["sessionVisible"] == true,
        "Unavailable session action",
    )?;
    fixture.key(&["-s", "200", "-k", "Escape"])?;
    fixture.ipc(&["helpShow"])?;
    pause(300);
    fixture.call("swaymsg", &["[title=\"Desktop Help\"]", "focus"])?;
    fixture.key(&["-s", "400", "remote keyboard"])?;
    wait("Help search", || Ok(fixture.state()?["helpMatches"] == 1))?;
    fixture.screenshot("help-search.png")?;
    fixture.ipc(&["helpClose"])?;
    fixture.ipc(&["stackShow", docs.to_str().ok_or("Non-UTF8 docs")?])?;
    wait("Downloads stack", || {
        Ok(fixture.state()?["stackTotal"] == 20)
    })?;
    require(fixture.state()?["stackRows"] == 20, "Downloads rows")?;
    fixture.screenshot("downloads-stack.png")?;
    fixture.ipc(&["stackClose"])?;
    pause(1700);
    require(
        fixture.state()?["scrollOpacity"].as_f64() == Some(0.0),
        "Scrollbar idle hide",
    )?;
    fixture.screenshot("light.png")?;
    let example = docs.join("example.png");
    make_png(&example)?;
    require(
        fixture.ipc(&["mediaCheck"])?.starts_with("PASS"),
        "Media source/seek isolation",
    )?;
    fixture.screenshot("media-expanded.png")?;
    fixture.ipc(&["mediaClose"])?;
    fixture.ipc(&["previewShow", example.to_str().ok_or("Non-UTF8 example")?])?;
    pause(700);
    fixture.screenshot("quick-look.png")?;
    fixture.ipc(&["previewSave", example.to_str().ok_or("Non-UTF8 example")?])?;
    pause(600);
    fixture.screenshot("save-dialog.png")?;
    fixture.key(&["-k", "Escape"])?;
    fixture.ipc(&["previewClose"])?;
    fixture.ipc(&["remoteShow"])?;
    pause(300);
    fixture.call("swaymsg", &["[title=\"Remote Desktop\"]", "focus"])?;
    fixture.focus_key("remoteSelect", &["-k", "Right"])?;
    require(
        fixture.state()?["remoteSelection"] == "vnc",
        "Remote keyboard selection",
    )?;
    fixture.screenshot("remote-desktop.png")?;
    fixture.ipc(&["remoteClose"])?;
    fixture.ipc(&["infoShow"])?;
    pause(300);
    fixture.screenshot("file-info.png")?;
    fixture.ipc(&["infoClose"])?;
    for dark in [false, true] {
        fixture.ipc(&["theme", if dark { "true" } else { "false" }])?;
        for page in ["Fonts", "Colours", "Spelling", "Shortcuts"] {
            fixture.ipc(&["toolShow", page])?;
            pause(350);
            fixture.screenshot(&format!(
                "{}-{}.png",
                if dark { "dark" } else { "light" },
                page.to_lowercase()
            ))?;
            fixture.ipc(&["toolClose"])?;
        }
        fixture.ipc(&["displayShow"])?;
        pause(300);
        fixture.screenshot(&format!(
            "{}-displays.png",
            if dark { "dark" } else { "light" }
        ))?;
        fixture.ipc(&["displayClose"])?;
    }
    for scale in ["1.5", "2"] {
        fixture.call(
            "swaymsg",
            &[
                "output",
                "HEADLESS-1",
                "resolution",
                "1920x1080",
                "scale",
                scale,
            ],
        )?;
        fixture.ipc(&["toolShow", "Colours"])?;
        pause(400);
        fixture.screenshot(&format!("colours-scale-{scale}.png"))?;
        fixture.ipc(&["toolClose"])?;
    }
    let checks = [
        "media source selection, bounded seek and capability checks",
        "destructive button keyboard activation",
        "combo keyboard selection commits highlighted row",
        "colour RGB and alpha round trip",
        "automatic scrollbar idle hide",
        "session cancellation and unavailable observation blocks termination",
        "Help search via real keyboard",
        "Downloads preview reports all twenty files",
        "isolated preview, inspector, utilities and display fixtures in light/dark with 150/200 percent captures",
    ];
    fs::write(
        fixture.options.output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({"passed":true,"checks":checks}))?
        ),
    )?;
    let logs = fs::read_to_string(fixture.options.output.join("ui.log"))?;
    require(
        ![
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Failed to load configuration",
            "Unable to assign",
            "Cannot assign [undefined]",
        ]
        .iter()
        .any(|marker| logs.contains(marker)),
        "Appearance UI emitted QML errors",
    )?;
    Ok(())
}
fn main() -> Result<()> {
    let options = Options::parse()?;
    if options.inside {
        run(options)
    } else {
        private_session()
    }
}
