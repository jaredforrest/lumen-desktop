//! Private installed office windows with compositor focus and physical input.
use lumen_office_workflows::{
    Result, control::Control, corpus, ensure, error, formats, keyboard::PhysicalKeyboard, live,
    rasters,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn fail(message: impl Into<String>) -> Box<dyn std::error::Error> {
    error(message)
}
fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(30);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => {}
            Err(e) => last = e.to_string(),
        }
        if Instant::now() >= end {
            return Err(fail(format!(
                "Office readiness condition timed out: {label}: {last}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| fail(format!("Expected number: {value}")))
}
fn nodes<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    out.push(node);
    for field in ["nodes", "floating_nodes"] {
        if let Some(children) = node[field].as_array() {
            for child in children {
                nodes(child, out);
            }
        }
    }
}
fn executable(path: &Path) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    ensure(
        path.starts_with("/nix/store") && path.is_file(),
        "Exact installed application/test binaries required",
    )?;
    Ok(path)
}
fn sha(path: &Path) -> Result<String> {
    formats::digest(path)
}

#[derive(Clone)]
struct Options {
    writer: PathBuf,
    sheet: PathBuf,
    slides: PathBuf,
    software_driver: PathBuf,
    sway: PathBuf,
    keyboard_tool: PathBuf,
    pointer_tool: PathBuf,
    output: PathBuf,
    source: PathBuf,
    app: String,
    effects: bool,
    close_probe: bool,
    new_saveas: bool,
    dialog_tools: Option<PathBuf>,
    sheet_close_outcome: String,
    slides_close_outcome: String,
    writer_close_outcome: String,
    profiles_json: String,
    source_sha: String,
    inside: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut writer = None;
        let mut sheet = None;
        let mut slides = None;
        let mut software_driver = None;
        let mut sway = None;
        let mut keyboard_tool = None;
        let mut pointer_tool = None;
        let mut output = None;
        let mut source = None;
        let mut app = "writer".to_owned();
        let mut effects = false;
        let mut close_probe = false;
        let mut new_saveas = false;
        let mut dialog_tools = None;
        let mut sheet_close_outcome = String::new();
        let mut slides_close_outcome = String::new();
        let mut writer_close_outcome = "cancel".to_owned();
        let mut profiles_json = String::new();
        let mut source_sha = String::new();
        let mut inside = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--writer") => {
                    writer = Some(PathBuf::from(args.next().ok_or("Missing writer")?))
                }
                Some("--sheet") => sheet = Some(PathBuf::from(args.next().ok_or("Missing sheet")?)),
                Some("--slides") => {
                    slides = Some(PathBuf::from(args.next().ok_or("Missing slides")?))
                }
                Some("--software-driver") => {
                    software_driver = Some(PathBuf::from(args.next().ok_or("Missing driver")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
                Some("--keyboard-tool") => {
                    keyboard_tool = Some(PathBuf::from(args.next().ok_or("Missing keyboard")?))
                }
                Some("--pointer-tool") => {
                    pointer_tool = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
                }
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--source") => {
                    source = Some(PathBuf::from(args.next().ok_or("Missing source")?))
                }
                Some("--app") => {
                    app = args
                        .next()
                        .ok_or("Missing app")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--effects") => effects = true,
                Some("--close-probe") => close_probe = true,
                Some("--new-saveas") => new_saveas = true,
                Some("--dialog-tools") => {
                    dialog_tools = Some(PathBuf::from(args.next().ok_or("Missing dialog tools")?))
                }
                Some("--sheet-close-outcome") => {
                    sheet_close_outcome = args
                        .next()
                        .ok_or("Missing sheet outcome")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--slides-close-outcome") => {
                    slides_close_outcome = args
                        .next()
                        .ok_or("Missing slides outcome")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--writer-close-outcome") => {
                    writer_close_outcome = args
                        .next()
                        .ok_or("Missing writer outcome")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--profiles-json") => {
                    profiles_json = args
                        .next()
                        .ok_or("Missing profiles JSON")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--source-sha") => {
                    source_sha = args
                        .next()
                        .ok_or("Missing source SHA")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--inside") => inside = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown native office option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        ensure(
            ["writer", "sheet", "slides"].contains(&app.as_str()),
            "Invalid app",
        )?;
        ensure(
            ["cancel", "discard", "save"].contains(&writer_close_outcome.as_str()),
            "Invalid writer close outcome",
        )?;
        ensure(
            sheet_close_outcome.is_empty()
                || ["cancel", "discard", "save", "error"].contains(&sheet_close_outcome.as_str()),
            "Invalid sheet close outcome",
        )?;
        ensure(
            slides_close_outcome.is_empty()
                || ["cancel", "discard", "save"].contains(&slides_close_outcome.as_str()),
            "Invalid slides close outcome",
        )?;
        ensure(
            sheet_close_outcome.is_empty() || app == "sheet",
            "Spreadsheet close outcome requires the spreadsheet provider",
        )?;
        ensure(
            slides_close_outcome.is_empty() || app == "slides",
            "Presentation close outcome requires the presentation provider",
        )?;
        ensure(
            !new_saveas
                || (dialog_tools.is_some()
                    && !close_probe
                    && sheet_close_outcome.is_empty()
                    && slides_close_outcome.is_empty()
                    && writer_close_outcome == "cancel"),
            "New SaveAs requires private dialog tools and a separate lifecycle scenario",
        )?;
        let current = env::current_dir()?;
        let output = output.ok_or("--output required")?;
        Ok(Self {
            writer: writer.ok_or("--writer required")?,
            sheet: sheet.ok_or("--sheet required")?,
            slides: slides.ok_or("--slides required")?,
            software_driver: software_driver.ok_or("--software-driver required")?,
            sway: sway.ok_or("--sway required")?,
            keyboard_tool: keyboard_tool.ok_or("--keyboard-tool required")?,
            pointer_tool: pointer_tool.ok_or("--pointer-tool required")?,
            output: if output.is_absolute() {
                output
            } else {
                current.join(output)
            },
            source: source.unwrap_or(current),
            app,
            effects,
            close_probe,
            new_saveas,
            dialog_tools,
            sheet_close_outcome,
            slides_close_outcome,
            writer_close_outcome,
            profiles_json,
            source_sha,
            inside,
        })
    }
    fn app_binary(&self) -> &Path {
        match self.app.as_str() {
            "writer" => &self.writer,
            "sheet" => &self.sheet,
            _ => &self.slides,
        }
    }
    fn name(&self) -> &'static str {
        match self.app.as_str() {
            "writer" => "wordcraft",
            "sheet" => "gridcraft",
            _ => "deckcraft",
        }
    }
    fn extension(&self) -> &'static str {
        match self.app.as_str() {
            "writer" => "docx",
            "sheet" => "xlsx",
            _ => "pptx",
        }
    }
    fn cli(&self) -> PathBuf {
        self.app_binary()
            .with_file_name(format!("{}-cli", self.name()))
    }
}
fn outer(mut options: Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    fs::set_permissions(&options.output, fs::Permissions::from_mode(0o700))?;
    options.source = options.source.canonicalize()?;
    for binary in [
        &options.writer,
        &options.sheet,
        &options.slides,
        &options.sway,
        &options.keyboard_tool,
        &options.pointer_tool,
    ] {
        executable(binary)?;
    }
    let policies = Command::new("nix")
        .args(["eval", "--json", "--file"])
        .arg(options.source.join("config/application-profiles.nix"))
        .output()?;
    ensure(
        policies.status.success(),
        "Application profiles failed to evaluate",
    )?;
    let profiles_json = String::from_utf8(policies.stdout)?.trim().to_owned();
    let git = Command::new("git")
        .arg("-C")
        .arg(&options.source)
        .args(["rev-parse", "HEAD"])
        .output()?;
    ensure(git.status.success(), "Git source revision unavailable")?;
    let sha = String::from_utf8(git.stdout)?.trim().to_owned();
    let current = env::current_exe()?;
    let executable = if current.starts_with(&options.source) {
        Path::new("/source").join(current.strip_prefix(&options.source)?)
    } else {
        current
    };
    let boundary = env::var_os("PATH")
        .and_then(|value| {
            env::split_paths(&value)
                .map(|dir| dir.join("lumen-application-isolation"))
                .find(|path| path.is_file())
        })
        .ok_or("Pinned Rust application isolation launcher required")?;
    let mut cmd = Command::new(boundary);
    cmd.arg("run")
        .arg("--source")
        .arg(&options.source)
        .arg("--output")
        .arg(&options.output);
    if options.effects {
        cmd.arg("--software-graphics");
    }
    cmd.arg("--").arg(executable).arg("--inside");
    for (key, value) in [
        ("--writer", options.writer.display().to_string()),
        ("--sheet", options.sheet.display().to_string()),
        ("--slides", options.slides.display().to_string()),
        (
            "--software-driver",
            options.software_driver.display().to_string(),
        ),
        ("--sway", options.sway.display().to_string()),
        (
            "--keyboard-tool",
            options.keyboard_tool.display().to_string(),
        ),
        ("--pointer-tool", options.pointer_tool.display().to_string()),
        ("--app", options.app.clone()),
        (
            "--writer-close-outcome",
            options.writer_close_outcome.clone(),
        ),
        ("--profiles-json", profiles_json),
        ("--source-sha", sha),
        ("--output", "/artifacts".into()),
    ] {
        cmd.arg(key).arg(value);
    }
    if options.effects {
        cmd.arg("--effects");
    }
    if options.close_probe {
        cmd.arg("--close-probe");
    }
    if options.new_saveas {
        cmd.arg("--new-saveas");
    }
    if let Some(path) = &options.dialog_tools {
        cmd.arg("--dialog-tools").arg(path);
    }
    if !options.sheet_close_outcome.is_empty() {
        cmd.arg("--sheet-close-outcome")
            .arg(&options.sheet_close_outcome);
    }
    if !options.slides_close_outcome.is_empty() {
        cmd.arg("--slides-close-outcome")
            .arg(&options.slides_close_outcome);
    }
    let status = cmd.status()?;
    ensure(
        status.success(),
        format!("Native office workflow failed: {status}"),
    )
}
struct Managed(Child);
impl Managed {
    fn stop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
            let end = Instant::now() + Duration::from_secs(5);
            while self.0.try_wait().ok().flatten().is_none() {
                if Instant::now() >= end {
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
struct Native {
    options: Options,
    env: BTreeMap<String, String>,
    processes: Vec<Managed>,
    keyboard: Option<PhysicalKeyboard>,
    control: Option<Control>,
    pointer: Option<usize>,
    facts: Value,
    checks: Vec<String>,
    source: PathBuf,
    window: Value,
    pid: i64,
    port: u16,
    chooser: Option<lumen_office_workflows::dialogs::Chooser>,
}
impl Drop for Native {
    fn drop(&mut self) {
        self.keyboard.take();
        self.control.take();
        self.chooser.take();
        for child in self.processes.iter_mut().rev() {
            child.stop();
        }
    }
}
impl Native {
    fn new(options: Options) -> Result<Self> {
        let uid = unsafe { libc::getuid() };
        let hostname = fs::read_to_string("/proc/sys/kernel/hostname")?
            .trim()
            .to_owned();
        ensure(
            std::process::id() == 1
                && uid != 0
                && hostname == "fixture"
                && env::var("HOME")? == "/home/fixture",
            "Private application boundary required",
        )?;
        ensure(
            !["dri", "snd", "video0"]
                .iter()
                .any(|name| Path::new("/dev").join(name).exists())
                && Path::new("/dev/udmabuf").exists() == options.effects,
            "Unexpected hardware paths",
        )?;
        let out = &options.output;
        let mut facts = json!({});
        let mut test_sources = BTreeMap::new();
        for name in [
            "tests/desktop/office-workflows-rust/src/bin/office-native-workflows.rs",
            "tests/desktop/office-workflows-rust/src/control.rs",
            "tests/desktop/office-workflows-rust/src/corpus.rs",
            "tests/desktop/office-workflows-rust/src/formats.rs",
            "tests/desktop/office-workflows-rust/src/live.rs",
            "tests/desktop/office-workflows-rust/src/keyboard.rs",
        ] {
            let path = Path::new("/source").join(name);
            test_sources.insert(name.to_owned(), sha(&path)?);
        }
        facts["testSources"] = json!(test_sources);
        let mut installed = BTreeMap::new();
        for binary in [
            &options.writer,
            &options.sheet,
            &options.slides,
            &options.sway,
            &options.pointer_tool,
            &options.keyboard_tool,
        ] {
            installed.insert(binary.display().to_string(), sha(binary)?);
        }
        facts["installedBinaries"] = json!(installed);
        fs::create_dir("/etc/nix")?;
        save(
            Path::new("/etc/nix/desktop-settings.json"),
            &json!({"applicationPreferences":serde_json::from_str::<Value>(&options.profiles_json)?}),
        )?;
        let vulkan = options
            .software_driver
            .join("share/vulkan/icd.d/lvp_icd.x86_64.json");
        let egl = options
            .software_driver
            .join("share/glvnd/egl_vendor.d/50_mesa.json");
        ensure(
            vulkan.is_file() && egl.is_file(),
            "Software driver files missing",
        )?;
        let mut values = env::vars().collect::<BTreeMap<_, _>>();
        for (key, value) in [
            ("WLR_BACKENDS", "headless".into()),
            (
                "WLR_RENDERER",
                if options.effects { "gles2" } else { "pixman" }.into(),
            ),
            ("WLR_LIBINPUT_NO_DEVICES", "1".into()),
            ("LIBGL_ALWAYS_SOFTWARE", "1".into()),
            ("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe".into()),
            ("VK_DRIVER_FILES", vulkan.display().to_string()),
            ("VK_ICD_FILENAMES", vulkan.display().to_string()),
            ("__EGL_VENDOR_LIBRARY_FILENAMES", egl.display().to_string()),
            (
                "LIBGL_DRIVERS_PATH",
                options
                    .software_driver
                    .join("lib/dri")
                    .display()
                    .to_string(),
            ),
            ("WGPU_BACKEND", "vulkan".into()),
            ("SWAYSOCK", "/run/fixture/sway.sock".into()),
            ("DECKCRAFT_WAYLAND", "1".into()),
            ("DECKCRAFT_NO_AUDIO", "1".into()),
        ] {
            values.insert(key.into(), value);
        }
        if options.effects {
            for (key, value) in [
                ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
                ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
            ] {
                values.insert(key.into(), value.into());
            }
        }
        facts["boundary"] = json!({"uid":uid,"gid":unsafe{libc::getgid()},"pid":std::process::id(),"hostname":hostname,"home":env::var("HOME")?,"hardwarePathsAbsent":true,"softwareAllocatorExposed":options.effects,"renderer":values["WLR_RENDERER"]});
        let name = options.name();
        let port = 34217u16;
        values.insert(
            format!("{}_CONTROL_PORT", name.to_uppercase()),
            port.to_string(),
        );
        let source = out.join(format!("{}-working.{}", options.app, options.extension()));
        Ok(Self {
            options,
            env: values,
            processes: Vec::new(),
            keyboard: None,
            control: None,
            pointer: None,
            facts,
            checks: Vec::new(),
            source,
            window: Value::Null,
            pid: 0,
            port,
            chooser: None,
        })
    }
    fn out(&self, name: &str) -> PathBuf {
        self.options.output.join(name)
    }
    fn command(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.env)
            .output()?;
        ensure(
            output.status.success(),
            format!(
                "{program} {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn launch(&mut self, program: &str, args: &[&str], label: &str, pipe: bool) -> Result<usize> {
        let log = File::create(self.out(&format!("{label}.log")))?;
        let mut c = Command::new(program);
        c.args(args)
            .env_clear()
            .envs(&self.env)
            .stdout(log.try_clone()?)
            .stderr(log);
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
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.command("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn live_pid(&self) -> Result<Option<i64>> {
        let path =
            Path::new("/run/fixture").join(format!("lumen-profile-{}.json", self.options.name()));
        if !path.exists() {
            return Ok(None);
        }
        let value: Value = serde_json::from_slice(&fs::read(path)?)?;
        let rows = value["pids"].as_array().ok_or("Profile PID list missing")?;
        Ok(if rows.len() == 1 {
            rows[0].as_i64()
        } else {
            None
        })
    }
    fn native(&self) -> Result<Option<Value>> {
        let Some(pid) = self.live_pid()? else {
            return Ok(None);
        };
        let tree = self.tree()?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        let result = all
            .into_iter()
            .filter(|node| {
                node["pid"] == pid
                    && node["app_id"] == format!("ai.storyteller.{}", self.options.name())
                    && ["con", "floating_con"].contains(&node["type"].as_str().unwrap_or(""))
                    && node["rect"]["width"].as_i64().is_some_and(|w| w > 0)
            })
            .cloned()
            .collect::<Vec<_>>();
        Ok(if result.len() == 1 {
            Some(result[0].clone())
        } else {
            None
        })
    }
    fn listener(&self) -> bool {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
    }
    fn tool(&mut self, name: &str, args: Option<Value>) -> Result<Value> {
        self.control
            .as_mut()
            .ok_or("Control unavailable")?
            .tool(name, args)
    }
    fn control_command(&mut self, name: &str) -> Result<Value> {
        self.control
            .as_mut()
            .ok_or("Control unavailable")?
            .command(&self.options.app, name, None)
    }
    fn key(&mut self, args: &[&str]) -> Result<()> {
        let keyboard = self.keyboard.as_mut().ok_or("Keyboard unavailable")?;
        if let Some(index) = args.iter().position(|value| *value == "--") {
            for character in args.get(index + 1).ok_or("Text missing")?.chars() {
                keyboard.type_text(&character.to_string())?;
                thread::sleep(Duration::from_millis(40));
            }
            thread::sleep(Duration::from_millis(100));
            Ok(())
        } else {
            keyboard.key(args)
        }
    }
    fn pointer_events(&mut self, x: f64, y: f64, events: &[&str]) -> Result<()> {
        self.command(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &(x.round_ties_even() as i64).to_string(),
                &(y.round_ties_even() as i64).to_string(),
            ],
        )?;
        let index = self.pointer.ok_or("Pointer unavailable")?;
        for event in std::iter::once("m 0.5 0.5").chain(events.iter().copied()) {
            let stdin = self.processes[index]
                .0
                .stdin
                .as_mut()
                .ok_or("Pointer stdin")?;
            writeln!(stdin, "{event}")?;
            stdin.flush()?;
            thread::sleep(Duration::from_millis(120));
        }
        Ok(())
    }
    fn screenshot(&self, name: &str) -> Result<()> {
        self.command("grim", &[self.out(name).to_str().ok_or("Screenshot path")?])?;
        Ok(())
    }
    fn set_fact(&mut self, key: &str, value: Value) {
        self.facts[key] = value;
    }
    fn inspect(&self) -> Result<Value> {
        match self.options.app.as_str() {
            "writer" => formats::writer(&self.source),
            "sheet" => formats::spreadsheet(&self.source),
            _ => formats::presentation(&self.source),
        }
    }
    fn start(&mut self) -> Result<String> {
        let corpus = corpus::create()?;
        let actions = match self.options.app.as_str() {
            "writer" => &corpus.writer,
            "sheet" => &corpus.grid,
            _ => &corpus.deck,
        };
        let mut args = vec!["run".to_owned()];
        for (name, params) in actions {
            args.extend([
                "--cmd".into(),
                format!("{name}={}", serde_json::to_string(params)?),
            ]);
        }
        args.extend([
            if self.options.app == "sheet" {
                "--out".into()
            } else {
                "--save".into()
            },
            self.source.display().to_string(),
        ]);
        let log = File::create(self.out("create.log"))?;
        let status = Command::new(self.options.cli())
            .args(&args)
            .env_clear()
            .envs(&self.env)
            .stdout(log.try_clone()?)
            .stderr(log)
            .status()?;
        ensure(
            status.success(),
            "Installed provider corpus creation failed",
        )?;
        ensure(
            formats::package_kind(&self.source) == self.options.extension(),
            "Native source container mismatch",
        )?;
        let before = sha(&self.source)?;
        self.set_fact("originalSha256", json!(before));
        fs::write(
            self.out("sway.conf"),
            "output HEADLESS-1 resolution 1440x1000\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n",
        )?;
        let sway = self.options.sway.display().to_string();
        self.launch(
            &sway,
            &[
                "-d",
                "--unsupported-gpu",
                "-c",
                self.out("sway.conf").to_str().ok_or("Sway config path")?,
            ],
            "sway",
            false,
        )?;
        wait("private compositor IPC", || {
            Ok(Path::new("/run/fixture/sway.sock").exists().then_some(()))
        })?;
        let display = wait("private display", || {
            Ok(fs::read_dir("/run/fixture")?
                .filter_map(|row| row.ok().map(|row| row.path()))
                .find(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("wayland-"))
                        && path
                            .metadata()
                            .is_ok_and(|metadata| metadata.file_type().is_socket())
                }))
        })?;
        self.env.insert(
            "WAYLAND_DISPLAY".into(),
            display
                .file_name()
                .ok_or("Display name")?
                .to_string_lossy()
                .into_owned(),
        );
        if let Some(manifest) = &self.options.dialog_tools {
            let chooser = lumen_office_workflows::dialogs::start(
                &mut self.env,
                manifest,
                &self.options.output,
            )?;
            self.set_fact("privateChooser", chooser.facts.clone());
            self.chooser = Some(chooser);
        }
        self.keyboard = Some(PhysicalKeyboard::start(
            &self.options.keyboard_tool,
            &self.env,
            &self.out("keyboard.log"),
        )?);
        let app = self.options.app_binary().display().to_string();
        let name = self.options.name();
        let source = self.source.display().to_string();
        self.launch(&app, &[&source], name, false)?;
        self.window = wait("exact profile child maps a native window", || self.native())?;
        self.pid = self.live_pid()?.ok_or("Native PID missing")?;
        ensure(
            self.window["pid"] == self.pid,
            "Profile PID differs from native window",
        )?;
        let stat = fs::read_to_string(format!("/proc/{}/stat", self.pid))?;
        let process_start = stat
            .rsplit_once(") ")
            .ok_or("Process stat malformed")?
            .1
            .split_whitespace()
            .nth(19)
            .ok_or("Process start missing")?
            .to_owned();
        let executable = fs::read_link(format!("/proc/{}/exe", self.pid))?;
        self.set_fact("native",json!({"conId":self.window["id"],"pid":self.pid,"processStart":process_start,"executable":executable,"profileWrapperPid":self.processes.last().ok_or("Provider process")?.0.id()}));
        self.command(
            "swaymsg",
            &[&format!("[con_id={}]", self.window["id"]), "focus"],
        )?;
        wait("captured native focus", || {
            let tree = self.tree()?;
            let mut all = Vec::new();
            nodes(&tree, &mut all);
            Ok(all
                .iter()
                .any(|node| node["id"] == self.window["id"] && node["focused"] == true)
                .then_some(()))
        })?;
        wait("private live provider listener", || {
            Ok(self.listener().then_some(()))
        })?;
        let target = if self.options.app == "writer" {
            format!("127.0.0.1:{}", self.port)
        } else {
            self.port.to_string()
        };
        self.control = Some(Control::start(
            &[
                self.options.cli().display().to_string(),
                "mcp".into(),
                "--connect".into(),
                target,
            ],
            &self.out("control.log"),
            Some(&self.env),
        )?);
        let initialize = self
            .control
            .as_mut()
            .ok_or("Control unavailable")?
            .initialize()?;
        self.set_fact("initialize", initialize);
        let initial = self.control_command(if self.options.app == "writer" {
            "file.info"
        } else {
            "document.inspect"
        })?;
        self.set_fact("initialLiveDocument", initial.clone());
        let initial_text = serde_json::to_string(&initial)?;
        ensure(
            if self.options.app == "writer" {
                initial_text.contains(&source)
            } else if self.options.app == "sheet" {
                initial_text.contains("Summary")
            } else {
                initial_text.contains("Synthetic title")
            },
            "Live controller returned an unbound model",
        )?;
        self.checks.push(
            "Installed profile wrapper, exact native PID/conId and private live document agree"
                .into(),
        );
        Ok(before)
    }
    fn reopen_saved(&mut self) -> Result<()> {
        let saved_hash = sha(&self.source)?;
        let saved_facts = self.inspect()?;
        let before_live = live::snapshot(
            self.control.as_mut().ok_or("Control unavailable")?,
            &self.options.app,
        )?;
        self.set_fact("beforeReopen", before_live.clone());
        let new_tool = match self.options.app.as_str() {
            "writer" => "new_document",
            "sheet" => "new_workbook",
            _ => "new_presentation",
        };
        self.tool(new_tool, None)?;
        let inspect = if self.options.app == "writer" {
            "file.info"
        } else {
            "document.inspect"
        };
        let cleared = self.control_command(inspect)?;
        ensure(
            cleared["path"] != self.source.display().to_string(),
            "New document did not detach saved document",
        )?;
        let open_tool = match self.options.app.as_str() {
            "writer" => "open_document",
            "sheet" => "open_workbook",
            _ => "open_presentation",
        };
        self.tool(open_tool, Some(json!({"path":self.source})))?;
        wait("GUI reopens exact saved path cleanly", || {
            let value = self
                .control
                .as_mut()
                .ok_or("Control unavailable")?
                .command(&self.options.app, inspect, None)?;
            Ok(
                (value["path"] == self.source.display().to_string() && value["dirty"] != true)
                    .then_some(value),
            )
        })?;
        let after_live = live::snapshot(
            self.control.as_mut().ok_or("Control unavailable")?,
            &self.options.app,
        )?;
        self.set_fact("afterReopen", after_live.clone());
        let diff = formats::differences(&before_live, &after_live);
        self.set_fact("nativeReopenDifferences", json!(diff));
        if !diff.is_empty() {
            save(
                &self.out("reopen-differences.json"),
                &json!({"before":before_live,"after":after_live,"differences":diff}),
            )?;
        }
        ensure(
            diff.is_empty(),
            "Native reopened document differs from saved live content",
        )?;
        ensure(
            sha(&self.source)? == saved_hash && self.inspect()? == saved_facts,
            "Reopen modified saved file",
        )?;
        self.screenshot("reopened.png")?;
        self.checks.push("Native app clears then reopens exact saved file; full text/structured provider projection and independent file facts agree".into());
        Ok(())
    }
    fn pointer_start(&mut self) -> Result<()> {
        if self.pointer.is_none() {
            let tool = self.options.pointer_tool.display().to_string();
            self.pointer =
                Some(self.launch(&tool, &["pointer", "move", "0", "0"], "pointer", true)?);
        }
        Ok(())
    }
    fn writer_basic(&mut self, before: &str) -> Result<()> {
        let marker = " Native keyboard fixture edit";
        self.key(&["-M", "ctrl", "-k", "End", "-m", "ctrl"])?;
        self.key(&["--", marker])?;
        let live = wait(
            "complete physical text is acknowledged in live document",
            || {
                let value = self.tool("get_text", None)?;
                Ok(serde_json::to_string(&value)?
                    .contains(marker.trim())
                    .then_some(value))
            },
        )?;
        self.set_fact("typedLiveText", live);
        let dirty = wait("physical typing marks live document dirty", || {
            let value = self.control_command("file.info")?;
            Ok((value["dirty"] == true).then_some(value))
        })?;
        ensure(
            sha(&self.source)? == before,
            "Typing changed saved source before explicit Save",
        )?;
        self.set_fact("dirtyLiveDocument", dirty);
        self.set_fact("typedText", json!(marker));
        self.key(&["-M", "ctrl", "-k", "s", "-m", "ctrl"])?;
        wait("physical Save changes owned source", || {
            Ok((sha(&self.source)? != before).then_some(()))
        })?;
        let saved = self.inspect()?;
        ensure(
            serde_json::to_string(&saved)?.contains(marker.trim()),
            "Saved DOCX lacks physical keyboard edit",
        )?;
        self.set_fact("savedSha256", json!(sha(&self.source)?));
        self.set_fact("savedDocument", saved);
        self.checks.push("Physical keyboard edit stays dirty until explicit Save and appears in independently parsed DOCX".into());
        self.reopen_saved()?;
        if !self.options.new_saveas {
            self.writer_close()?;
        }
        Ok(())
    }
    fn sheet_basic(&mut self, before: &str) -> Result<()> {
        let marker = "Native sheet edit";
        self.pointer_start()?;
        self.pointer_events(156.0, 273.0, &["p", "r"])?;
        let cell = wait("physical pointer acknowledges exact C1 cell", || {
            let value = self.control_command("document.inspect")?;
            Ok((value["activeCell"] == "C1").then_some(value))
        })?;
        self.set_fact("cellBeforeEdit", cell);
        self.key(&["--", marker])?;
        self.key(&["-k", "Return"])?;
        let typed = wait("physical spreadsheet cell entry acknowledged", || {
            let value = self.tool("get_cell", Some(json!({"cell":"Summary!C1"})))?;
            Ok(serde_json::to_string(&value)?
                .contains(marker)
                .then_some(value))
        })?;
        self.set_fact("typedCell", typed);
        ensure(
            sha(&self.source)? == before,
            "Sheet source changed before Save",
        )?;
        self.key(&["-M", "ctrl", "-k", "s", "-m", "ctrl"])?;
        wait("physical spreadsheet Save changes source", || {
            Ok((sha(&self.source)? != before).then_some(()))
        })?;
        let saved = self.inspect()?;
        ensure(
            serde_json::to_string(&saved)?.contains(marker),
            "Saved sheet lacks physical edit",
        )?;
        self.set_fact("savedSha256", json!(sha(&self.source)?));
        self.set_fact("savedDocument", saved);
        self.checks.push("Physical cell entry stays unsaved until explicit Save and appears in independently parsed workbook".into());
        self.reopen_saved()
    }
    fn slides_basic(&mut self, before: &str) -> Result<()> {
        let marker = " Native slide edit";
        let first = self.tool("inspect_slide", Some(json!({"index":0})))?;
        self.set_fact("slideBeforeEdit", first);
        self.pointer_start()?;
        self.pointer_events(520.0, 320.0, &["p", "r", "p", "r"])?;
        self.key(&["-k", "End"])?;
        self.key(&["--", marker])?;
        let typed = wait("physical presentation text acknowledged", || {
            let value = self.tool("inspect_slide", Some(json!({"index":0})))?;
            Ok(serde_json::to_string(&value)?
                .contains(marker.trim())
                .then_some(value))
        })?;
        self.set_fact("typedSlide", typed);
        ensure(
            sha(&self.source)? == before,
            "Slides source changed before Save",
        )?;
        self.key(&["-M", "ctrl", "-k", "s", "-m", "ctrl"])?;
        wait("physical presentation Save changes source", || {
            Ok((sha(&self.source)? != before).then_some(()))
        })?;
        let saved = self.inspect()?;
        ensure(
            serde_json::to_string(&saved)?.contains(marker.trim()),
            "Saved slides lack physical edit",
        )?;
        self.set_fact("savedSha256", json!(sha(&self.source)?));
        self.set_fact("savedDocument", saved);
        self.checks.push("Physical presentation text edit is acknowledged live and independently parsed after explicit Save".into());
        self.reopen_saved()
    }
    fn kill_window(&self) -> Result<()> {
        self.command(
            "swaymsg",
            &[&format!("[con_id={}]", self.window["id"]), "kill"],
        )?;
        Ok(())
    }
    fn writer_close(&mut self) -> Result<()> {
        let ui = self.tool("ui_inspect", None)?;
        self.set_fact("uiBeforeClose", ui);
        self.key(&["--", " Unsaved close fixture"])?;
        wait("second edit acknowledged", || {
            let value = self.tool("get_text", None)?;
            Ok(serde_json::to_string(&value)?
                .contains("Unsaved close fixture")
                .then_some(()))
        })?;
        let saved_hash = sha(&self.source)?;
        self.kill_window()?;
        let dialog = wait("native close presents dirty-document decision", || {
            let value = self.tool("ui_inspect", None)?;
            Ok(value["dialog"]
                .as_object()
                .is_some_and(|dialog| !dialog.is_empty())
                .then_some(value))
        })?;
        self.set_fact("dirtyCloseDialog", dialog.clone());
        ensure(
            sha(&self.source)? == saved_hash,
            "Close prompt silently overwrote source",
        )?;
        self.screenshot("dirty-close.png")?;
        ensure(
            dialog["dialog"]["dialog"] == "saveChanges",
            "Writer dirty close dialog missing",
        )?;
        self.key(&["-k", "Escape"])?;
        wait("full-US Escape cancels dirty close", || {
            Ok((self.tool("ui_inspect", None)?["dialog"].is_null()).then_some(()))
        })?;
        ensure(
            self.native()?.is_some() && sha(&self.source)? == saved_hash,
            "Writer Escape lost document",
        )?;
        self.checks
            .push("Physical full-US Escape cancels dirty close without changing source".into());
        self.kill_window()?;
        wait("second captured native close prompts", || {
            Ok((!self.tool("ui_inspect", None)?["dialog"].is_null()).then_some(()))
        })?;
        self.pointer_start()?;
        let x = match self.options.writer_close_outcome.as_str() {
            "cancel" => 861.0,
            "discard" => 787.0,
            _ => 719.0,
        };
        self.pointer_events(x, 511.0, &["p", "r"])?;
        if self.options.writer_close_outcome == "cancel" {
            wait("physical Cancel button cancels dirty close", || {
                Ok((self.tool("ui_inspect", None)?["dialog"].is_null()).then_some(()))
            })?;
            ensure(
                self.native()?.is_some()
                    && serde_json::to_string(&self.tool("get_text", None)?)?
                        .contains("Unsaved close fixture")
                    && sha(&self.source)? == saved_hash,
                "Writer Cancel lost unsaved document",
            )?;
            self.checks.push("Dirty native close presents Save/Discard/Cancel and physical Cancel preserves document and saved source".into());
        } else {
            wait("captured native close releases its document", || {
                if self.native()?.is_none() {
                    return Ok(Some(()));
                }
                let info = self.control_command("file.info")?;
                Ok((info["path"] != self.source.display().to_string()).then_some(()))
            })?;
            let persisted = self.inspect()?;
            let persisted_text = serde_json::to_string(&persisted)?;
            if self.options.writer_close_outcome == "discard" {
                ensure(
                    sha(&self.source)? == saved_hash
                        && !persisted_text.contains("Unsaved close fixture"),
                    "Writer discard changed saved file",
                )?;
            } else {
                ensure(
                    sha(&self.source)? != saved_hash
                        && persisted_text.contains("Unsaved close fixture"),
                    "Writer save omitted dirty edit",
                )?;
            }
            self.set_fact("closeOutcome",json!({"choice":self.options.writer_close_outcome,"savedSha256":sha(&self.source)?,"tree":self.tree()?}));
            self.set_fact("nativeCloseCompleted", json!(true));
            self.checks.push(format!("Physical {} close releases document with independently validated saved-file outcome",self.options.writer_close_outcome));
        }
        Ok(())
    }
    fn app_ui(&mut self) -> Result<Value> {
        if self.options.app == "slides" {
            self.tool("call_app", Some(json!({"method":"ui.inspect"})))
        } else {
            Ok(serde_json::from_str(&self.command(
                self.options.cli().to_str().ok_or("CLI path")?,
                &["send", &self.port.to_string(), "ui.inspect", "{}"],
            )?)?)
        }
    }
    fn dirty_close_probe(&mut self) -> Result<()> {
        let expected = sha(&self.source)?;
        let marker = "Unsaved provider close fixture";
        ensure(
            self.native()?
                .as_ref()
                .is_some_and(|n| n["id"] == self.window["id"]),
            "Native close target changed",
        )?;
        if self.options.app == "sheet" {
            self.pointer_events(156.0, 289.0, &["p", "r"])?;
            wait("captured dirty-close cell selected", || {
                Ok((self.control_command("document.inspect")?["activeCell"] == "C2").then_some(()))
            })?;
            self.key(&["--", marker])?;
            self.key(&["-k", "Return"])?;
            let value = wait("dirty close cell acknowledged", || {
                let value = self.tool("get_cell", Some(json!({"cell":"Summary!C2"})))?;
                Ok(serde_json::to_string(&value)?
                    .contains(marker)
                    .then_some(value))
            })?;
            self.set_fact("closeProbeDirtyCell", value);
        } else {
            self.pointer_events(520.0, 320.0, &["p", "r", "p", "r"])?;
            self.key(&["-k", "End"])?;
            self.key(&["--", marker])?;
            let value = wait("dirty close slide acknowledged", || {
                let value = self.tool("inspect_slide", Some(json!({"index":0})))?;
                Ok(serde_json::to_string(&value)?
                    .contains(marker)
                    .then_some(value))
            })?;
            self.set_fact("closeProbeDirtySlide", value);
        }
        let before = self.control_command("document.inspect")?;
        ensure(
            before["dirty"] == true && sha(&self.source)? == expected,
            "Dirty close probe changed source",
        )?;
        self.set_fact("closeProbeBefore", before);
        self.kill_window()?;
        thread::sleep(Duration::from_millis(300));
        let state = json!({"wrapperExit": self.processes.last_mut().and_then(|p|p.0.try_wait().ok().flatten()).map(|s|s.code()), "capturedNativePidPresent": Path::new(&format!("/proc/{}", self.pid)).exists(), "capturedWindowPresent": self.native()?.is_some()});
        self.set_fact("closeProbeProcessState", state);
        match self.control_command("document.inspect") {
            Ok(value) => self.set_fact("closeProbeAfter", value),
            Err(_) => {
                self.set_fact(
                    "closeProbeDocumentInspection",
                    json!("refused_after_close_request_cause_unconfirmed"),
                );
                let frame = self
                    .control
                    .as_ref()
                    .and_then(|c| c.frames.last())
                    .cloned()
                    .unwrap_or(Value::Null);
                self.set_fact("closeProbeRefusal", frame);
            }
        }
        let ui = self.app_ui()?;
        self.set_fact("closeProbeUi", ui);
        self.set_fact("closeProbeTree", self.tree()?);
        self.set_fact("exploratoryCloseProbe", json!(true));
        ensure(
            sha(&self.source)? == expected,
            "Close probe changed saved source",
        )?;
        self.screenshot("close-probe.png")?;
        Ok(())
    }
    fn sheet_close(&mut self) -> Result<()> {
        let expected = sha(&self.source)?;
        let marker = "Unsaved provider close fixture";
        let first = self.facts["closeProbeUi"]["dialog"].clone();
        ensure(
            first["name"] == "saveChanges" && first["values"]["uid"].as_i64().is_some(),
            "Spreadsheet close prompt missing",
        )?;
        self.key(&["-k", "Escape"])?;
        wait("physical Escape cancels spreadsheet native close", || {
            Ok(self.app_ui()?["dialog"].is_null().then_some(()))
        })?;
        self.sheet_retained(&expected, marker)?;
        self.checks.push("Physical Escape preserves complete unsaved spreadsheet content and saved source after native close".into());
        self.key(&["-M", "ctrl", "-k", "f", "-m", "ctrl"])?;
        wait("physical Find opens unrelated spreadsheet dialog", || {
            Ok((self.app_ui()?["dialog"]["name"] == "find").then_some(()))
        })?;
        self.kill_window()?;
        thread::sleep(Duration::from_millis(200));
        ensure(
            self.native()?.is_some() && self.app_ui()?["dialog"]["name"] == "find",
            "Find dialog did not cancel close",
        )?;
        self.key(&["-k", "Escape"])?;
        wait("physical Escape dismisses unrelated dialog", || {
            Ok(self.app_ui()?["dialog"].is_null().then_some(()))
        })?;
        self.sheet_retained(&expected, marker)?;
        self.checks.push(
            "Native close during unrelated dialog is canceled without changing captured document"
                .into(),
        );
        self.kill_window()?;
        let dialog = wait("second captured spreadsheet native close prompts", || {
            let ui = self.app_ui()?;
            Ok(ui["dialog"].is_object().then_some(ui))
        })?;
        ensure(
            dialog["dialog"]["values"]["uid"] == first["values"]["uid"],
            "Spreadsheet close prompt changed owner",
        )?;
        let outcome = self.options.sheet_close_outcome.clone();
        if outcome == "error" {
            fs::set_permissions("/artifacts", fs::Permissions::from_mode(0o500))?;
        }
        let action = self.pointer_events(
            match outcome.as_str() {
                "cancel" => 642.0,
                "discard" => 574.0,
                _ => 693.0,
            },
            494.0,
            &["p", "r"],
        );
        let failed_message = if outcome == "error" && action.is_ok() {
            Some(wait(
                "failed physical spreadsheet Save reports error",
                || {
                    let ui = self.app_ui()?;
                    Ok((!ui["message"].is_null()).then_some(ui["message"].clone()))
                },
            ))
        } else {
            None
        };
        if outcome == "error" {
            fs::set_permissions("/artifacts", fs::Permissions::from_mode(0o700))?;
        }
        action?;
        if outcome == "cancel" || outcome == "error" {
            if outcome == "error" {
                let message = failed_message.ok_or("Failed save message absent")??;
                self.set_fact("failedCloseSaveMessage", message.clone());
                self.kill_window()?;
                thread::sleep(Duration::from_millis(200));
                ensure(
                    self.native()?.is_some() && self.app_ui()?["message"] == message,
                    "Failed save message lost",
                )?;
                self.key(&["-k", "Escape"])?;
                wait("physical Escape dismisses failed-save message", || {
                    Ok(self.app_ui()?["message"].is_null().then_some(()))
                })?;
            }
            wait("spreadsheet close confirmation retires", || {
                Ok(self.app_ui()?["dialog"].is_null().then_some(()))
            })?;
            self.sheet_retained(&expected, marker)?;
        } else {
            wait(
                "physical spreadsheet close releases captured document",
                || {
                    if self.native()?.is_none() {
                        return Ok(Some(()));
                    }
                    Ok((self.control_command("document.inspect")?["path"]
                        != self.source.display().to_string())
                    .then_some(()))
                },
            )?;
            let persisted = serde_json::to_string(&self.inspect()?)?;
            ensure(
                if outcome == "discard" {
                    sha(&self.source)? == expected && !persisted.contains(marker)
                } else {
                    sha(&self.source)? != expected && persisted.contains(marker)
                },
                "Spreadsheet close persisted wrong content",
            )?;
            self.set_fact("nativeCloseCompleted", json!(true));
        }
        self.set_fact(
            "spreadsheetCloseOutcome",
            json!({"choice":outcome,"savedSha256":sha(&self.source)?,"tree":self.tree()?}),
        );
        self.checks.push(format!("Physical spreadsheet {outcome} has independently validated saved-file and captured-document outcome"));
        Ok(())
    }
    fn sheet_retained(&mut self, expected: &str, marker: &str) -> Result<()> {
        let cell = self.tool("get_cell", Some(json!({"cell":"Summary!C2"})))?;
        ensure(
            self.native()?.is_some()
                && self.control_command("document.inspect")?["dirty"] == true
                && serde_json::to_string(&cell)?.contains(marker)
                && sha(&self.source)? == expected,
            "Spreadsheet dirty document not retained",
        )
    }
    fn slides_close(&mut self) -> Result<()> {
        let expected = sha(&self.source)?;
        let marker = "Unsaved provider close fixture";
        ensure(
            self.facts["closeProbeUi"]["dialog"] == "quit",
            "Presentation close prompt missing",
        )?;
        self.key(&["-k", "Escape"])?;
        wait("physical Escape cancels presentation dirty close", || {
            Ok(self.app_ui()?["dialog"].is_null().then_some(()))
        })?;
        ensure(
            self.native()?.is_some()
                && serde_json::to_string(&self.tool("inspect_slide", Some(json!({"index":0})))?)?
                    .contains(marker)
                && sha(&self.source)? == expected,
            "Presentation Escape lost document",
        )?;
        self.checks.push("Physical Escape cancels presentation native close and preserves complete unsaved text and saved file".into());
        self.kill_window()?;
        wait("repeated native presentation close prompts again", || {
            Ok((self.app_ui()?["dialog"] == "quit").then_some(()))
        })?;
        let outcome = self.options.slides_close_outcome.clone();
        self.pointer_events(
            match outcome.as_str() {
                "cancel" => 777.0,
                "discard" => 592.0,
                _ => 854.0,
            },
            510.0,
            &["p", "r"],
        )?;
        if outcome == "cancel" {
            wait(
                "physical presentation Cancel preserves its document",
                || Ok(self.app_ui()?["dialog"].is_null().then_some(())),
            )?;
            ensure(
                self.native()?.is_some()
                    && self.control_command("document.inspect")?["dirty"] == true
                    && serde_json::to_string(
                        &self.tool("inspect_slide", Some(json!({"index":0})))?,
                    )?
                    .contains(marker)
                    && sha(&self.source)? == expected,
                "Presentation Cancel lost document",
            )?;
        } else {
            wait(
                "physical presentation close releases captured document",
                || {
                    if self.native()?.is_none() {
                        return Ok(Some(()));
                    }
                    Ok((self.control_command("document.inspect")?["path"]
                        != self.source.display().to_string())
                    .then_some(()))
                },
            )?;
            let persisted = serde_json::to_string(&self.inspect()?)?;
            ensure(
                if outcome == "discard" {
                    sha(&self.source)? == expected && !persisted.contains(marker)
                } else {
                    sha(&self.source)? != expected && persisted.contains(marker)
                },
                "Presentation close persisted wrong content",
            )?;
            self.set_fact("nativeCloseCompleted", json!(true));
        }
        self.set_fact(
            "presentationCloseOutcome",
            json!({"choice":outcome,"savedSha256":sha(&self.source)?,"tree":self.tree()?}),
        );
        self.checks.push(format!("Physical presentation {outcome} has independently validated saved-file and native-document outcome"));
        Ok(())
    }
    fn owned_choosers(&self) -> Result<Vec<Value>> {
        let backend = self.facts["privateChooser"]["backendPid"]
            .as_i64()
            .ok_or("Chooser backend PID missing")?;
        let tree = self.tree()?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok(all
            .into_iter()
            .filter(|node| {
                node["pid"] == backend
                    && ["con", "floating_con"].contains(&node["type"].as_str().unwrap_or(""))
                    && node["rect"]["width"].as_i64().is_some_and(|w| w > 0)
            })
            .cloned()
            .collect())
    }
    fn chooser(&self) -> Result<Value> {
        wait("exact owned standard chooser maps and gains focus", || {
            Ok(self
                .owned_choosers()?
                .into_iter()
                .find(|n| n["focused"] == true))
        })
    }
    fn chooser_absent(&self, id: &Value) -> Result<()> {
        wait("captured chooser retires", || {
            Ok((!self.owned_choosers()?.iter().any(|n| n["id"] == *id)).then_some(()))
        })
    }
    fn new_text(&mut self) -> Result<Value> {
        match self.options.app.as_str() {
            "writer" => self.tool("get_text", None),
            "sheet" => self.tool("get_cell", Some(json!({"cell":"Sheet1!A1"}))),
            _ => self.tool("inspect_slide", Some(json!({"index":0}))),
        }
    }
    fn new_retained(&mut self, expected: &str, marker: &str) -> Result<()> {
        let info = self.control_command(if self.options.app == "writer" {
            "file.info"
        } else {
            "document.inspect"
        })?;
        ensure(
            info["dirty"] == true
                && (info["path"].is_null() || info["path"] == "")
                && serde_json::to_string(&self.new_text()?)?.contains(marker)
                && sha(&self.source)? == expected,
            "New unsaved document was not retained",
        )
    }
    fn new_saveas(&mut self) -> Result<()> {
        ensure(
            self.chooser.is_some(),
            "New SaveAs needs private dialog tools",
        )?;
        let expected = sha(&self.source)?;
        let marker = "Unsaved new document fixture";
        let new_tool = match self.options.app.as_str() {
            "writer" => "new_document",
            "sheet" => "new_workbook",
            _ => "new_presentation",
        };
        self.tool(new_tool, None)?;
        match self.options.app.as_str() {
            "writer" => self.key(&["--", marker])?,
            "sheet" => {
                self.key(&["--", marker])?;
                self.key(&["-k", "Return"])?
            }
            _ => {
                let slide = self.tool("inspect_slide", Some(json!({"index":0})))?;
                let ui = self.app_ui()?;
                let info = self.control_command("document.inspect")?;
                let title = slide["shapes"]
                    .as_array()
                    .ok_or("Slide shapes missing")?
                    .iter()
                    .filter(|shape| {
                        ["title", "ctrTitle"].contains(&shape["placeholder"].as_str().unwrap_or(""))
                    })
                    .collect::<Vec<_>>();
                ensure(title.len() == 1, "New slide title placeholder missing")?;
                let b = title[0]["box"].as_array().ok_or("Title box missing")?;
                let r = ui["slideRect"].as_array().ok_or("Slide rect missing")?;
                let owner = self.native()?.ok_or("Native window missing")?;
                let scale = number(&r[2])? / number(&info["slideSize"][0])?;
                let x = number(&owner["rect"]["x"])?
                    + number(&r[0])?
                    + (number(&b[0])? + number(&b[2])? / 2.0) * scale;
                let y = number(&owner["rect"]["y"])?
                    + number(&r[1])?
                    + (number(&b[1])? + number(&b[3])? / 2.0) * scale;
                self.set_fact(
                    "newPresentationTitleTarget",
                    json!({"shape":title[0],"point":[x,y],"slideRect":r,"scale":scale}),
                );
                self.pointer_events(x, y, &["p", "r", "p", "r"])?;
                self.key(&["-k", "End"])?;
                self.key(&["--", marker])?;
            }
        }
        let content = wait("new document physical text acknowledged", || {
            let v = self.new_text()?;
            Ok(serde_json::to_string(&v)?.contains(marker).then_some(v))
        })?;
        self.set_fact("newUnsavedContent", content);
        self.new_retained(&expected, marker)?;
        if self.options.app == "sheet" {
            self.kill_window()?;
            wait("new unsaved workbook native close decision", || {
                Ok((self.app_ui()?["dialog"]["name"] == "saveChanges").then_some(()))
            })?;
            self.pointer_events(693.0, 494.0, &["p", "r"])?;
        } else {
            self.key(&["-M", "ctrl", "-k", "s", "-m", "ctrl"])?;
        }
        let first = self.chooser()?;
        self.set_fact("newSaveAsChooser", first.clone());
        self.screenshot("unsaved-chooser.png")?;
        self.key(&["-k", "Escape"])?;
        self.chooser_absent(&first["id"])?;
        self.new_retained(&expected, marker)?;
        self.checks.push("New document actual standard SaveAs cancels physically while complete draft and original saved file remain intact".into());
        self.key(&["-M", "ctrl", "-k", "s", "-m", "ctrl"])?;
        let second = self.chooser()?;
        let before = self
            .owned_choosers()?
            .into_iter()
            .map(|n| n["id"].clone())
            .collect::<Vec<_>>();
        self.key(&["-M", "ctrl", "-k", "l", "-m", "ctrl"])?;
        self.key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        self.key(&[
            "--",
            &format!(
                "/source/missing-fixture-save-folder/book.{}",
                self.options.extension()
            ),
        ])?;
        self.key(&["-k", "Return"])?;
        let invalid = wait("standard chooser reports invalid location", || {
            Ok(self
                .owned_choosers()?
                .into_iter()
                .find(|n| !before.contains(&n["id"])))
        })?;
        self.set_fact("newSaveAsInvalidLocation", invalid.clone());
        self.screenshot("unsaved-chooser-error.png")?;
        self.key(&["-k", "Escape"])?;
        self.chooser_absent(&invalid["id"])?;
        self.key(&["-k", "Escape"])?;
        self.chooser_absent(&second["id"])?;
        self.new_retained(&expected, marker)?;
        self.checks.push("New document invalid-location chooser error and physical cancellation preserve complete unsaved content".into());
        self.key(&["-M", "ctrl", "-k", "s", "-m", "ctrl"])?;
        let third = self.chooser()?;
        let saved = self.out(&format!("new-unsaved.{}", self.options.extension()));
        self.key(&["-M", "ctrl", "-k", "l", "-m", "ctrl"])?;
        self.key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        self.key(&["--", saved.to_str().ok_or("SaveAs path")?])?;
        self.key(&["-k", "Return"])?;
        self.chooser_absent(&third["id"])?;
        wait(
            "explicit standard SaveAs writes generated native document",
            || Ok(saved.is_file().then_some(())),
        )?;
        let persisted = match self.options.app.as_str() {
            "writer" => formats::writer(&saved)?,
            "sheet" => formats::spreadsheet(&saved)?,
            _ => formats::presentation(&saved)?,
        };
        ensure(
            serde_json::to_string(&persisted)?.contains(marker) && sha(&self.source)? == expected,
            "SaveAs content or original source differs",
        )?;
        let info = wait("new SaveAs document acknowledged clean", || {
            let v = self.control_command(if self.options.app == "writer" {
                "file.info"
            } else {
                "document.inspect"
            })?;
            Ok((v["path"] == saved.display().to_string() && v["dirty"] == false).then_some(v))
        })?;
        self.set_fact(
            "newSaveAsResult",
            json!({"path":saved,"sha256":sha(&saved)?,"document":persisted,"live":info}),
        );
        self.screenshot("unsaved-saved.png")?;
        self.set_fact("completedPaint", json!("unsaved-saved.png"));
        self.kill_window()?;
        wait("explicit later clean native close succeeds", || {
            Ok(self.native()?.is_none().then_some(()))
        })?;
        self.set_fact("nativeCloseCompleted", json!(true));
        self.checks.push("Later explicit SaveAs persists complete new document, preserves original source and permits clean native close".into());
        Ok(())
    }
}

fn run(mut fixture: Native) -> Result<()> {
    let before = fixture.start()?;
    match fixture.options.app.as_str() {
        "writer" => fixture.writer_basic(&before)?,
        "sheet" => fixture.sheet_basic(&before)?,
        "slides" => fixture.slides_basic(&before)?,
        _ => unreachable!(),
    };
    if fixture.options.new_saveas {
        fixture.new_saveas()?;
    }
    if (fixture.options.close_probe
        || !fixture.options.sheet_close_outcome.is_empty()
        || !fixture.options.slides_close_outcome.is_empty())
        && fixture.options.app != "writer"
    {
        fixture.dirty_close_probe()?;
    }
    if !fixture.options.sheet_close_outcome.is_empty() {
        fixture.sheet_close()?;
    }
    if !fixture.options.slides_close_outcome.is_empty() {
        fixture.slides_close()?;
    }
    if fixture.options.effects {
        let log = fs::read_to_string(fixture.out("sway.log"))?;
        let required = [
            "Creating scenefx FX renderer",
            "Using OpenGL ES",
            "GL renderer: llvmpipe",
            "FX RENDERER: Shaders Initialized Successfully",
            "Trying udmabuf allocator",
            "Created GL FBO for buffer",
        ];
        ensure(
            required.iter().all(|marker| log.contains(marker)),
            "Software effects renderer proof missing",
        )?;
        fixture.set_fact("softwareEffectsProof", json!(required));
        fixture.checks.push("No-DRI software effects compositor initializes shaders, udmabuf and real rendered framebuffers".into());
    }
    fixture.screenshot("opened.png")?;
    let paint = if fixture.facts["nativeCloseCompleted"] == true {
        fixture.facts["completedPaint"]
            .as_str()
            .unwrap_or("reopened.png")
    } else {
        "opened.png"
    };
    let paint_path = fixture.out(paint);
    let (width, height, nonblack) = rasters::nonblack_pixels(&paint_path)?;
    ensure(
        nonblack > width * height / 4,
        "Native application paint is blank",
    )?;
    fixture.set_fact("paintEvidence", json!(paint));
    fixture.set_fact("paintedNonBlackPixels", json!(nonblack));
    save(&fixture.out("tree-opened.json"), &fixture.tree()?)?;
    save(
        &fixture.out("result.json"),
        &json!({"passed":true,"phase":if fixture.checks.len()>1{"native_partial"}else{"startup_only"},"app":fixture.options.app,"workflowComplete":false,"checks":fixture.checks,"facts":fixture.facts,"source":fixture.options.source_sha}),
    )?;
    Ok(())
}
fn main() -> Result<()> {
    let options = Options::parse()?;
    if options.inside {
        let fixture = Native::new(options)?;
        run(fixture)
    } else {
        outer(options)
    }
}
