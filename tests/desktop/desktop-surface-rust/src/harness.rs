use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
pub fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
pub fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
pub fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
pub fn wait_true(label: &str, mut probe: impl FnMut() -> Result<bool>) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if probe().unwrap_or(false) {
            return Ok(());
        }
        if Instant::now() >= end {
            return Err(fail(format!("Timed out: {label}")));
        }
        pause(75);
    }
}
#[derive(Clone)]
pub struct Options {
    pub scenario: String,
    pub output: PathBuf,
    pub launcher: PathBuf,
    pub sway: PathBuf,
    pub pointer: PathBuf,
    pub inside: bool,
    pub layout: bool,
    pub multi_drag: bool,
    pub layer: String,
    pub skip_native_escape: bool,
    pub effects: bool,
}
impl Options {
    pub fn parse() -> Result<Self> {
        let mut args = env::args_os().skip(1);
        let scenario = args
            .next()
            .ok_or("Missing desktop surface scenario")?
            .to_string_lossy()
            .into_owned();
        let mut output = None;
        let mut launcher = None;
        let mut sway = None;
        let mut pointer = None;
        let mut inside = false;
        let mut layout = false;
        let mut multi_drag = false;
        let mut layer = "bottom".to_owned();
        let mut skip_native_escape = false;
        let mut effects = false;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--layout") => layout = true,
                Some("--multi-drag") => multi_drag = true,
                Some("--skip-native-escape") => skip_native_escape = true,
                Some("--inside") => inside = true,
                Some("--effects") => effects = true,
                Some("--layer") => {
                    layer = args
                        .next()
                        .ok_or("Missing layer")?
                        .to_string_lossy()
                        .into_owned()
                }
                _ => return Err(fail("Unknown desktop surface option")),
            }
        }
        require(layer == "bottom" || layer == "top", "Unknown layer")?;
        require(!multi_drag || layout, "Multi-drag requires layout")?;
        Ok(Self {
            effects,
            scenario,
            output: env::current_dir()?.join(output.ok_or("--output required")?),
            launcher: launcher.ok_or("--launcher required")?.canonicalize()?,
            sway: sway.ok_or("--sway required")?.canonicalize()?,
            pointer: pointer.ok_or("--pointer-tool required")?.canonicalize()?,
            inside,
            layout,
            multi_drag,
            layer,
            skip_native_escape,
        })
    }
}
pub fn private_session() -> Result<()> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = PathBuf::from(format!("/tmp/lds-{}-{nanos}", std::process::id()));
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let mut command = Command::new(env!("LUMEN_FIXTURE_DBUS_RUN_SESSION"));
    command.arg("--").arg(env::current_exe()?);
    command
        .args(env::args_os().skip(1))
        .arg("--inside")
        .env("HOME", &root)
        .env("XDG_RUNTIME_DIR", &root);
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        command.env_remove(name);
    }
    let status = command.status();
    let _ = fs::remove_dir_all(&root);
    require(status?.success(), "Private desktop surface fixture failed")
}
pub struct Managed(pub Child);
impl Managed {
    pub fn stop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let end = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait().ok().flatten().is_none() {
            if Instant::now() >= end {
                unsafe {
                    libc::killpg(self.0.id() as i32, libc::SIGKILL);
                }
                break;
            }
            pause(25);
        }
        let _ = self.0.wait();
    }
}
pub struct Fixture {
    pub options: Options,
    pub env: BTreeMap<String, String>,
    pub runtime: PathBuf,
    pub assets: PathBuf,
    pub desktop: PathBuf,
    pub processes: Vec<Managed>,
    pub pointer: Option<usize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for child in self.processes.iter_mut().rev() {
            child.stop();
        }
    }
}
impl Fixture {
    pub fn new(options: Options) -> Result<Self> {
        fs::create_dir(&options.output)?;
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let mut values = env::vars().collect::<BTreeMap<_, _>>();
        let output = Command::new(&options.launcher)
            .arg("--print-env0")
            .output()?;
        require(output.status.success(), "Launcher environment failed")?;
        for row in output.stdout.split(|byte| *byte == 0) {
            if let Some(index) = row.iter().position(|byte| *byte == b'=') {
                let key = String::from_utf8(row[..index].to_vec())?;
                if [
                    "PATH",
                    "GLOBAL_MENU_PYTHON",
                    "QT_PLUGIN_PATH",
                    "QML_IMPORT_PATH",
                    "LUMEN_FILE_JOBS_BIN",
                    "LUMEN_DESKTOP_ASSETS",
                ]
                .contains(&key.as_str())
                {
                    values.insert(key, String::from_utf8(row[index + 1..].to_vec())?);
                }
            }
        }
        let assets = PathBuf::from(
            values
                .get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Missing packaged assets")?,
        )
        .join("shell");
        require(
            assets.join("DesktopFilesSurface.qml").exists(),
            "Missing Desktop Files assets",
        )?;
        for key in ["CONFIG", "STATE", "DATA", "CACHE"] {
            let folder = options.output.join(key.to_lowercase());
            fs::create_dir_all(&folder)?;
            values.insert(format!("XDG_{key}_HOME"), folder.display().to_string());
        }
        for (key, value) in [
            ("WLR_BACKENDS", "headless"),
            ("WLR_RENDERER", "pixman"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
            ("LUMEN_FILE_SERVICE", "0"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
        ] {
            values.insert(key.into(), value.into());
        }
        for key in ["DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK"] {
            values.remove(key);
        }
        if options.effects {
            for (key, value) in [
                ("WLR_RENDERER", "gles2"),
                ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
                ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
                ("LIBGL_ALWAYS_SOFTWARE", "1"),
                ("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe"),
            ] {
                values.insert(key.into(), value.into());
            }
            let driver = Path::new(env!("LUMEN_FIXTURE_SOFTWARE_DRIVER"));
            values.insert(
                "LIBGL_DRIVERS_PATH".into(),
                driver.join("lib/dri").display().to_string(),
            );
            values.insert(
                "__EGL_VENDOR_LIBRARY_FILENAMES".into(),
                driver
                    .join("share/glvnd/egl_vendor.d/50_mesa.json")
                    .display()
                    .to_string(),
            );
        }
        if options.scenario.starts_with("files-drag") {
            values.insert(
                "FONTCONFIG_FILE".into(),
                env!("LUMEN_FIXTURE_FONTCONFIG").into(),
            );
        }
        let desktop = runtime.join("Desktop");
        fs::create_dir(&desktop)?;
        let config = PathBuf::from(&values["XDG_CONFIG_HOME"]);
        fs::write(
            config.join("user-dirs.dirs"),
            "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\n",
        )?;
        let palette: Value = serde_json::from_slice(&fs::read(
            assets
                .parent()
                .ok_or("Asset parent")?
                .join("appearance/palette.json"),
        )?)?;
        let appearance = options.output.join("appearance.json");
        save(
            &appearance,
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{},"desktopPreferences":{"showFiles":true,"fileSort":"name","fileDescending":false}}),
        )?;
        values.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            appearance.display().to_string(),
        );
        Ok(Self {
            options,
            env: values,
            runtime,
            assets,
            desktop,
            processes: Vec::new(),
            pointer: None,
        })
    }
    fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args).env_clear().envs(&self.env);
        command
    }
    pub fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = self.command(program, args).output()?;
        if !output.status.success() {
            return Err(fail(format!(
                "{program} {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    pub fn launch(
        &mut self,
        program: &str,
        args: &[&str],
        name: &str,
        pipe: bool,
    ) -> Result<usize> {
        let log = File::create(self.options.output.join(format!("{name}.log")))?;
        let mut command = self.command(program, args);
        command.stdout(log.try_clone()?).stderr(log);
        if pipe {
            command.stdin(Stdio::piped());
        }
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        self.processes.push(Managed(command.spawn()?));
        Ok(self.processes.len() - 1)
    }
    pub fn start(&mut self, config: &str, keyboard_delay: &str) -> Result<()> {
        let jobs = self
            .env
            .get("LUMEN_FILE_JOBS_BIN")
            .ok_or("Missing packaged file jobs")?
            .clone();
        self.launch(&jobs, &["serve"], "file-jobs", false)?;
        wait_true("private file jobs", || {
            Ok(self.runtime.join("lumen-file-operations.sock").exists())
        })?;
        let sway_config = self.options.output.join("sway.conf");
        fs::write(&sway_config, config)?;
        let sway = self.options.sway.display().to_string();
        self.launch(
            &sway,
            &[
                "--unsupported-gpu",
                "-c",
                sway_config.to_str().ok_or("Sway config path")?,
            ],
            "sway",
            false,
        )?;
        wait_true("private compositor", || {
            Ok(first_file(&self.runtime, "sway-ipc", false)?.is_some())
        })?;
        self.env.insert(
            "SWAYSOCK".into(),
            first_file(&self.runtime, "sway-ipc", false)?
                .ok_or("Missing compositor socket")?
                .display()
                .to_string(),
        );
        self.env.insert(
            "WAYLAND_DISPLAY".into(),
            first_file(&self.runtime, "wayland-", true)?
                .ok_or("Missing display")?
                .file_name()
                .ok_or("Missing display name")?
                .to_string_lossy()
                .into_owned(),
        );
        let pointer = self.options.pointer.display().to_string();
        let index = self.launch(&pointer, &["pointer", "move", "0", "0"], "pointer", true)?;
        self.pointer = Some(index);
        self.launch("wtype", &["-s", keyboard_delay], "keyboard", false)?;
        pause(300);
        Ok(())
    }
    pub fn pointer(&mut self, command: &str) -> Result<()> {
        let index = self.pointer.ok_or("Pointer missing")?;
        let pipe = self.processes[index]
            .0
            .stdin
            .as_mut()
            .ok_or("Pointer pipe missing")?;
        pipe.write_all(command.as_bytes())?;
        pipe.flush()?;
        Ok(())
    }
    pub fn ipc(&self, qml: &Path, target: &str, method: &str, args: &[&str]) -> Result<String> {
        let mut all = vec![
            "-p",
            qml.to_str().ok_or("QML path")?,
            "ipc",
            "call",
            target,
            method,
        ];
        all.extend(args);
        self.call("quickshell", &all)
    }
    pub fn state(&self, qml: &Path, target: &str) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc(
            qml,
            target,
            "status",
            &[],
        )?)?)
    }
    pub fn sway(&self, args: &[&str]) -> Result<String> {
        self.call("swaymsg", args)
    }
    pub fn cursor(&self, x: i64, y: i64) -> Result<()> {
        self.sway(&[
            "seat",
            "seat0",
            "cursor",
            "set",
            &x.to_string(),
            &y.to_string(),
        ])?;
        Ok(())
    }
    pub fn screenshot(&self, name: &str, include_cursor: bool) -> Result<()> {
        let path = self.options.output.join(name);
        let value = path.to_str().ok_or("Screenshot path")?;
        if include_cursor {
            self.call(env!("LUMEN_FIXTURE_GRIM"), &["-c", value])?;
        } else {
            self.call(env!("LUMEN_FIXTURE_GRIM"), &[value])?;
        }
        Ok(())
    }
    pub fn key(&self, name: &str) -> Result<()> {
        self.call("wtype", &["-k", name])?;
        Ok(())
    }
}
fn first_file(root: &Path, prefix: &str, exclude_lock: bool) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|row| row.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let text = name.to_string_lossy();
                text.starts_with(prefix) && (!exclude_lock || !text.ends_with(".lock"))
            })
        }))
}
