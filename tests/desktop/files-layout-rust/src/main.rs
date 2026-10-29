//! Native Files tab and sidebar pointer/keyboard regression.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}

struct Options {
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    lock_client: Option<PathBuf>,
    effects: bool,
    history: bool,
    selection: bool,
    split_panes: bool,
    views: Vec<String>,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut output = None;
    let mut launcher = None;
    let mut sway = None;
    let mut pointer = None;
    let mut lock_client = None;
    let mut effects = false;
    let mut history = false;
    let mut selection = false;
    let mut split_panes = false;
    let mut views = Vec::new();
    let mut inside = false;
    let mut args = env::args_os().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
            Some("--pointer-tool") => {
                pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
            }
            Some("--lock-client") => {
                lock_client = Some(PathBuf::from(args.next().ok_or("Missing lock client")?))
            }
            Some("--effects") => effects = true,
            Some("--history") => history = true,
            Some("--selection") => selection = true,
            Some("--split-panes") => split_panes = true,
            Some("--views") => {
                while args
                    .peek()
                    .is_some_and(|value| !value.to_string_lossy().starts_with("--"))
                {
                    let mode = args
                        .next()
                        .ok_or("Missing view")?
                        .to_string_lossy()
                        .into_owned();
                    require(
                        ["list", "grid", "gallery", "columns"].contains(&mode.as_str()),
                        "Unknown Files view",
                    )?;
                    views.push(mode);
                }
                require(!views.is_empty(), "--views requires at least one view")?;
            }
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown Files layout fixture option")),
        }
    }
    Ok(Options {
        output: output.ok_or("--output required")?,
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        pointer: pointer.ok_or("--pointer-tool required")?,
        lock_client,
        effects,
        history,
        selection,
        split_panes,
        views,
        inside,
    })
}
fn private_environment(root: &Path) -> BTreeMap<String, String> {
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    values.insert("HOME".into(), root.display().to_string());
    values.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    for name in ["CONFIG", "STATE", "CACHE", "DATA"] {
        values.insert(
            format!("XDG_{name}_HOME"),
            root.join(name.to_lowercase()).display().to_string(),
        );
    }
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        values.remove(name);
    }
    values
}
fn outer(options: &Options) -> Result<()> {
    let private = tempfile::tempdir()?;
    let config = private.path().join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy user=\"{}\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            private.path().display(),
            unsafe { libc::getuid() }
        ),
    )?;
    let mut command = Command::new("timeout");
    command
        .arg(if options.history || options.selection || options.split_panes {
            "300s"
        } else {
            "120s"
        })
        .arg("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--output")
        .arg(&options.output)
        .arg("--launcher")
        .arg(&options.launcher)
        .arg("--sway")
        .arg(&options.sway)
        .arg("--pointer-tool")
        .arg(&options.pointer)
        .env_clear()
        .envs(private_environment(private.path()));
    if options.effects {
        command.arg("--effects");
    }
    if options.history {
        command.arg("--history");
    }
    if options.split_panes {
        command.arg("--split-panes");
    }
    if options.selection {
        command.arg("--selection").arg("--lock-client").arg(
            options
                .lock_client
                .as_ref()
                .ok_or("Selection lock client missing")?,
        );
        if !options.views.is_empty() {
            command.arg("--views").args(&options.views);
        }
    }
    let status = command.status()?;
    require(
        status.success(),
        &format!("Private Files layout fixture exited {status}"),
    )
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(from, to)?;
        }
    }
    Ok(())
}
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    require(output.status.success(), "Launcher environment failed")?;
    let mut values = BTreeMap::new();
    for row in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|row| !row.is_empty())
    {
        if let Some((name, value)) = std::str::from_utf8(row)?.split_once('=') {
            values.insert(name.to_owned(), value.to_owned());
        }
    }
    Ok(values)
}
fn call(program: &str, args: &[&str], envs: &BTreeMap<String, String>) -> Result<String> {
    let output = Command::new("timeout")
        .arg("8s")
        .arg(program)
        .args(args)
        .env_clear()
        .envs(envs)
        .output()?;
    require(
        output.status.success(),
        &format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        program: &str,
        args: &[&str],
        envs: &BTreeMap<String, String>,
        log: &Path,
        interactive: bool,
    ) -> Result<usize> {
        let stream = File::options().append(true).create(true).open(log)?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(envs)
            .process_group(0)
            .stdin(if interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::from(stream.try_clone()?))
            .stderr(Stdio::from(stream))
            .spawn()?;
        self.0.push(child);
        Ok(self.0.len() - 1)
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
            let deadline = Instant::now() + Duration::from_secs(5);
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
            return Err(fail(format!(
                "Private Files layout acceptance timed out ({label}): {last}"
            )));
        }
        thread::sleep(Duration::from_millis(80));
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
fn nodes<'a>(node: &'a Value, found: &mut Vec<&'a Value>) {
    found.push(node);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                nodes(child, found);
            }
        }
    }
}
struct Fixture {
    envs: BTreeMap<String, String>,
    qml: PathBuf,
    out: PathBuf,
    folders: BTreeMap<String, String>,
}
impl Fixture {
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut values = vec![
            "-p",
            self.qml.to_str().ok_or("Invalid QML path")?,
            "ipc",
            "call",
            "filesAudit",
            method,
        ];
        values.extend_from_slice(args);
        call("quickshell", &values, &self.envs)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("state", &[])?)?)
    }
    fn window(&self) -> Result<Value> {
        let tree: Value =
            serde_json::from_str(&call("swaymsg", &["-r", "-t", "get_tree"], &self.envs)?)?;
        let mut found = Vec::new();
        nodes(&tree, &mut found);
        found
            .into_iter()
            .find(|v| {
                v["app_id"] == "org.quickshell"
                    && v["name"].as_str().is_some_and(|s| s.starts_with("Files"))
            })
            .cloned()
            .ok_or_else(|| fail("Files window missing"))
    }
    fn point(&self, name: &str, rx: f64, ry: f64) -> Result<(f64, f64)> {
        let g: Value = serde_json::from_str(&self.ipc("geometry", &[name])?)?;
        require(!g.is_null(), &format!("Control {name} missing"))?;
        let rect = self.window()?["rect"].clone();
        let number = |v: &Value, k: &str| -> Result<f64> {
            v[k].as_f64().ok_or_else(|| fail(format!("Missing {k}")))
        };
        Ok((
            number(&rect, "x")? + number(&g, "x")? + number(&g, "width")? * rx,
            number(&rect, "y")? + number(&g, "y")? + number(&g, "height")?.min(25.0) * ry,
        ))
    }
    fn move_to(&self, children: &mut Children, pointer: usize, x: f64, y: f64) -> Result<()> {
        call(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &format!("{:.0}", x),
                &format!("{:.0}", y),
            ],
            &self.envs,
        )?;
        self.pointer(children, pointer, "m 0.5 0.5")
    }
    fn pointer(&self, children: &mut Children, pointer: usize, action: &str) -> Result<()> {
        let stream = children.0[pointer]
            .stdin
            .as_mut()
            .ok_or("Pointer stdin missing")?;
        writeln!(stream, "{action}")?;
        stream.flush()?;
        thread::sleep(Duration::from_millis(120));
        Ok(())
    }
    fn click(&self, children: &mut Children, pointer: usize, pos: (f64, f64)) -> Result<()> {
        self.move_to(children, pointer, pos.0, pos.1)?;
        self.pointer(children, pointer, "p")?;
        self.pointer(children, pointer, "r")
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        call("wtype", args, &self.envs)?;
        thread::sleep(Duration::from_millis(150));
        Ok(())
    }
    fn drag(
        &self,
        children: &mut Children,
        pointer: usize,
        pos: (f64, f64),
        dx: f64,
        dy: f64,
        cancel: bool,
    ) -> Result<()> {
        self.move_to(children, pointer, pos.0, pos.1)?;
        self.pointer(children, pointer, "p")?;
        for _ in 0..8 {
            self.pointer(children, pointer, &format!("m {} {}", dx / 8.0, dy / 8.0))?;
        }
        if cancel {
            let mut snapshots = vec![self.state()?];
            self.key(&["-k", "Escape"])?;
            snapshots.push(self.state()?);
            self.pointer(children, pointer, "m 8 0")?;
            snapshots.push(self.state()?);
            fs::write(
                self.out.join("cancellation-state.json"),
                serde_json::to_vec_pretty(&snapshots)?,
            )?;
        }
        self.pointer(children, pointer, "r")
    }
    fn invariant(&self) -> Result<Value> {
        let state = self.state()?;
        let first = &self.folders["First"];
        require(
            state["path"] == *first
                && state["selection"] == json!([format!("{first}/selected.txt")])
                && state["activeId"] == "first",
            &format!("Files identity changed: {state}"),
        )?;
        Ok(state)
    }
    fn screenshot(&self, name: &str) -> Result<()> {
        call(
            "grim",
            &[self
                .out
                .join(name)
                .to_str()
                .ok_or("Screenshot path invalid")?],
            &self.envs,
        )?;
        Ok(())
    }
    fn resize(&self, id: &Value, width: i64, height: i64) -> Result<()> {
        call(
            "swaymsg",
            &[
                &format!("[con_id={}]", id.as_i64().ok_or("Window ID missing")?),
                "resize",
                "set",
                "width",
                &width.to_string(),
                "px",
                "height",
                &height.to_string(),
                "px",
            ],
            &self.envs,
        )?;
        Ok(())
    }
}
fn tab_ids(state: &Value) -> Result<Vec<String>> {
    Ok(state["tabs"]
        .as_array()
        .ok_or("Tabs missing")?
        .iter()
        .map(|v| v["id"].as_str().unwrap_or("").to_owned())
        .collect())
}
fn screenshot_has_content(path: &Path, rect: &Value) -> Result<bool> {
    let decoder = png::Decoder::new(File::open(path)?);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let bpp = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(fail("Unexpected screenshot colour format")),
    };
    let x0 = (rect["x"].as_i64().ok_or("Window x missing")? + 10).max(0) as usize;
    let y0 = (rect["y"].as_i64().ok_or("Window y missing")? + 70).max(0) as usize;
    let x1 = (rect["x"].as_i64().ok_or("Window x missing")?
        + rect["width"].as_i64().ok_or("Window width missing")?
        - 10)
        .max(0) as usize;
    let y1 = (rect["y"].as_i64().ok_or("Window y missing")?
        + rect["height"].as_i64().ok_or("Window height missing")?
        - 10)
        .max(0) as usize;
    let mut sum = [0u64; 3];
    let mut squares = [0u64; 3];
    let mut count = 0u64;
    for y in y0..y1.min(info.height as usize) {
        for x in x0..x1.min(info.width as usize) {
            let i = (y * info.width as usize + x) * bpp;
            for c in 0..3 {
                let pixel = buf[i + c] as u64;
                sum[c] += pixel;
                squares[c] += pixel * pixel;
            }
            count += 1;
        }
    }
    let means = (0..3)
        .map(|c| sum[c] as f64 / count as f64)
        .collect::<Vec<_>>();
    Ok(count > 0
        && means.iter().copied().fold(0.0, f64::max) > 10.0
        && (0..3).any(|c| {
            let mean = means[c];
            let deviation = (squares[c] as f64 / count as f64 - mean * mean)
                .max(0.0)
                .sqrt();
            deviation > 2.0
        }))
}
fn inner(options: &Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    let mut envs = launcher_environment(&options.launcher)?;
    for (k, v) in [
        ("WLR_BACKENDS", "headless"),
        (
            "WLR_RENDERER",
            if options.effects { "gles2" } else { "pixman" },
        ),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
    ] {
        envs.insert(k.into(), v.into());
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        envs.remove(key);
    }
    if options.effects {
        for (k, v) in [
            ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
            ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ] {
            envs.insert(k.into(), v.into());
        }
    }
    let runtime = PathBuf::from(envs.get("XDG_RUNTIME_DIR").ok_or("Runtime missing")?);
    let home = PathBuf::from(envs.get("HOME").ok_or("Home missing")?);
    let mut folders = BTreeMap::new();
    for name in ["First", "Second", "Third"] {
        let folder = home.join(name);
        fs::create_dir(&folder)?;
        fs::write(
            folder.join("selected.txt"),
            format!("unchanged fixture {name}"),
        )?;
        folders.insert(name.into(), folder.display().to_string());
    }
    let repo = env::current_dir()?;
    let palette: Value = serde_json::from_slice(&fs::read(
        repo.join("res/lumen-desktop/appearance/palette.json"),
    )?)?;
    let appearance = options.output.join("appearance.json");
    fs::write(
        &appearance,
        json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}).to_string(),
    )?;
    envs.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    copy_tree(
        &repo.join("res/lumen-desktop"),
        &options.output.join("assets"),
    )?;
    let qml = options.output.join("shell.qml");
    fs::write(
        &qml,
        include_str!("../fixture.qml")
            .replace("__PATHS__", &serde_json::to_string(&folders)?)
            .replace("persistViewPreferences:false", if options.split_panes { "persistViewPreferences:true" } else { "persistViewPreferences:false" })
            .replace("service:service;visible:true}", if options.split_panes { "service:service;visible:true;implicitWidth:1200}" } else { "service:service;visible:true}" }),
    )?;
    let mut fixture = Fixture {
        envs,
        qml,
        out: options.output.clone(),
        folders,
    };
    let mut children = Children(Vec::new());
    let mut checks = Vec::new();
    let config = fixture.out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n",
    )?;
    children.launch(
        options.sway.to_str().ok_or("Sway path invalid")?,
        &[
            "--unsupported-gpu",
            "-c",
            config.to_str().ok_or("Config path invalid")?,
        ],
        &fixture.envs,
        &fixture.out.join("sway.log"),
        false,
    )?;
    let socket = wait("Sway socket", || first_match(&runtime, "sway-ipc"))?;
    let display = wait("Wayland display", || first_wayland(&runtime))?;
    fixture
        .envs
        .insert("SWAYSOCK".into(), socket.display().to_string());
    fixture.envs.insert(
        "WAYLAND_DISPLAY".into(),
        display
            .file_name()
            .ok_or("Display missing")?
            .to_string_lossy()
            .into_owned(),
    );
    children.launch(
        "quickshell",
        &["-p", fixture.qml.to_str().ok_or("QML path invalid")?],
        &fixture.envs,
        &fixture.out.join("shell.log"),
        false,
    )?;
    wait("Files ready", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| v["ready"] == true)
            .map(|_| ()))
    })?;
    fixture.ipc("select", &[])?;
    fixture.invariant()?;
    fixture.screenshot("files-selected.png")?;
    children.launch(
        "wtype",
        &["-s", "60000"],
        &fixture.envs,
        &fixture.out.join("keyboard.log"),
        false,
    )?;
    let pointer = children.launch(
        options.pointer.to_str().ok_or("Pointer path invalid")?,
        &["pointer", "move", "0", "0"],
        &fixture.envs,
        &fixture.out.join("pointer.log"),
        true,
    )?;
    thread::sleep(Duration::from_millis(200));
    if options.split_panes {
        return split_panes_inner(&fixture, &mut children, pointer);
    }
    let pos = fixture.point("files-sidebar-favourites", 0.4, 0.5)?;
    fixture.click(&mut children, pointer, pos)?;
    wait("sidebar collapse", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| {
                v["collapsed"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|x| x == "favourites"))
            })
            .map(|_| ()))
    })?;
    fixture.invariant()?;
    fixture.key(&["-k", "space"])?;
    wait("sidebar expand", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| {
                v["collapsed"]
                    .as_array()
                    .is_some_and(|a| !a.iter().any(|x| x == "favourites"))
            })
            .map(|_| ()))
    })?;
    fixture.invariant()?;
    checks
        .push("Pointer disclosure and keyboard expansion preserve current route and selected file");
    let width = fixture.state()?["sidebarWidth"]
        .as_f64()
        .ok_or("Sidebar width missing")?;
    let pos = fixture.point("files-sidebar-resize", 0.5, 0.5)?;
    fixture.drag(&mut children, pointer, pos, 90.0, 0.0, false)?;
    wait("pointer resize", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| v["savedWidth"].as_f64().is_some_and(|n| n >= width + 85.0))
            .map(|_| ()))
    })?;
    fixture.invariant()?;
    let saved = fixture.state()?["savedWidth"]
        .as_f64()
        .ok_or("Saved width missing")?;
    fixture.key(&["-k", "Right"])?;
    wait("keyboard resize", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| {
                v["savedWidth"]
                    .as_f64()
                    .is_some_and(|n| (n - saved - 10.0).abs() < 0.1)
            })
            .map(|_| ()))
    })?;
    fixture.invariant()?;
    checks.push("Pointer and keyboard resize are distinct from file/sidebar selection");
    let saved = fixture.state()?["savedWidth"]
        .as_f64()
        .ok_or("Saved width missing")?;
    let pos = fixture.point("files-sidebar-resize", 0.5, 0.5)?;
    fixture.drag(&mut children, pointer, pos, 40.0, 0.0, true)?;
    require(
        fixture.state()?["savedWidth"].as_f64() == Some(saved),
        "Escape committed sidebar resize",
    )?;
    fixture.invariant()?;
    checks.push("Escape cancels a held sidebar resize without committing");
    let first = fixture.point("document-tab-first", 0.5, 0.5)?;
    let third = fixture.point("document-tab-third", 0.5, 0.5)?;
    fixture.drag(&mut children, pointer, third, first.0 - third.0, 0.0, false)?;
    wait("tab reorder", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| {
                tab_ids(v).ok() == Some(vec!["third".into(), "first".into(), "second".into()])
            })
            .map(|_| ()))
    })?;
    fixture.invariant()?;
    checks.push("Real tab drag reorders stable identity without changing active content");
    let before = tab_ids(&fixture.state()?)?;
    let pos = fixture.point("document-tab-third", 0.5, 0.5)?;
    fixture.drag(&mut children, pointer, pos, 90.0, 0.0, true)?;
    require(
        tab_ids(&fixture.state()?)? == before,
        "Escape committed tab drag",
    )?;
    fixture.invariant()?;
    checks.push("Escape cancels tab drag even with further motion before release");
    let pos = fixture.point("document-tab-third", 0.5, 0.5)?;
    fixture.drag(&mut children, pointer, pos, 0.0, 80.0, false)?;
    require(
        tab_ids(&fixture.state()?)? == before,
        "Outside release reordered tabs",
    )?;
    fixture.invariant()?;
    checks.push("Release outside tab drop region cancels without navigation");
    let pos = fixture.point("document-tab-first", 0.4, 0.5)?;
    fixture.click(&mut children, pointer, pos)?;
    fixture.key(&[
        "-M", "ctrl", "-M", "shift", "-k", "Right", "-m", "shift", "-m", "ctrl",
    ])?;
    wait("keyboard tab reorder", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| {
                tab_ids(v).ok() == Some(vec!["third".into(), "second".into(), "first".into()])
            })
            .map(|_| ()))
    })?;
    fixture.invariant()?;
    checks.push("Keyboard tab reorder retains active identity, history and selection");
    let node = fixture.window()?;
    fixture.resize(&node["id"], 480, 600)?;
    wait("narrow window", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| v["width"] == 480)
            .map(|_| ()))
    })?;
    let close: Value =
        serde_json::from_str(&fixture.ipc("geometry", &["document-tab-close-first"])?)?;
    require(
        close["x"].as_f64().unwrap_or(-1.0)
            >= fixture.state()?["sidebarWidth"].as_f64().unwrap_or(9999.0)
            && close["x"].as_f64().unwrap_or(9999.0) + close["width"].as_f64().unwrap_or(9999.0)
                <= 450.0,
        "Narrow tab close target clipped",
    )?;
    fixture.screenshot("files-narrow-tabs.png")?;
    fixture.invariant()?;
    checks.push("Narrow tab strip keeps active tab close target visible without shrinking targets");
    fixture.resize(&node["id"], 900, 600)?;
    wait("wide window", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| v["width"] == 900)
            .map(|_| ()))
    })?;
    let pos = fixture.point("document-tab-close-second", 0.5, 0.5)?;
    fixture.click(&mut children, pointer, pos)?;
    wait("close inactive tab", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| v["tabs"].as_array().is_some_and(|a| a.len() == 2))
            .map(|_| ()))
    })?;
    fixture.invariant()?;
    checks.push("Closing an inactive tab does not navigate or lose active selection");
    fixture.key(&["-M", "ctrl", "-k", "w", "-m", "ctrl"])?;
    wait("close active tab", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| v["activeId"] == "third" && v["ready"] == true)
            .map(|_| ()))
    })?;
    let mut value = fixture.state()?;
    let first = &fixture.folders["First"];
    let third = &fixture.folders["Third"];
    let history = value["history"]
        .as_array()
        .ok_or("History missing")?
        .iter()
        .map(|v| v.as_str().or_else(|| v["path"].as_str()).unwrap_or(""))
        .collect::<Vec<_>>();
    require(
        value["path"] == *third
            && history == vec![first.as_str(), third.as_str()]
            && value["historyIndex"] == 1
            && value["selection"] == json!([format!("{third}/selected.txt")]),
        &format!("Tab history changed: {value}"),
    )?;
    checks.push("Closing active tab restores exact destination history and selected identity");
    let node = fixture.window()?;
    fixture.resize(&node["id"], 480, 600)?;
    wait("final narrow window", || {
        Ok(fixture
            .state()
            .ok()
            .filter(|v| v["width"] == 480)
            .map(|_| ()))
    })?;
    value = fixture.state()?;
    require(
        value["sidebarWidth"].as_f64().is_some_and(|n| n <= 180.0)
            && value["width"].as_f64().unwrap_or(0.0)
                - value["sidebarWidth"].as_f64().unwrap_or(9999.0)
                >= 300.0
            && value["savedWidth"].as_f64() == Some(saved),
        "Narrow sidebar changed saved preference",
    )?;
    checks.push("Actual 480px window bounds sidebar without overwriting wider saved preference");
    fixture.screenshot("files-narrow.png")?;
    require(
        screenshot_has_content(&fixture.out.join("files-narrow.png"), &node["rect"])?,
        "Blank file interior",
    )?;
    let log = fs::read_to_string(fixture.out.join("shell.log"))?;
    require(
        ![
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Failed to load configuration",
            "Cannot assign",
        ]
        .iter()
        .any(|m| log.contains(m)),
        &format!("QML errors: {log}"),
    )?;
    for (name, path) in &fixture.folders {
        require(
            fs::read_to_string(Path::new(path).join("selected.txt"))?
                == format!("unchanged fixture {name}"),
            "Fixture file changed",
        )?;
    }
    fs::write(
        fixture.out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"checks":checks,"final":value,"scope":"Genuine virtual pointer/keyboard in isolated compositor; real disposable Files backend; no live user input or files"}),
        )?,
    )?;
    println!("PASS native Files tab/sidebar pointer and keyboard regression");
    Ok(())
}
fn split_state(fixture: &Fixture) -> Result<Value> {
    Ok(serde_json::from_str(&fixture.ipc("splitState", &[])?)?)
}
fn split_panes_inner(fixture: &Fixture, children: &mut Children, pointer: usize) -> Result<()> {
    let layout: Value = serde_json::from_str(&fixture.ipc("layoutCases", &[])?)?;
    require(layout["count"] == 144 && layout["errors"] == json!([]), &format!("File layout model cases failed: {layout}"))?;
    let tabs: Value = serde_json::from_str(&fixture.ipc("tabCases", &[])?)?;
    require(tabs["errors"] == json!([]), &format!("Files tab model cases failed: {tabs}"))?;
    let pref = PathBuf::from(fixture.envs.get("XDG_STATE_HOME").ok_or("State home missing")?)
        .join("lumen-desktop/session/files.json");
    let mut checks = Vec::new();
    fixture.ipc("splitSetup", &["list", "true"])?;
    wait("preview visible", || Ok(split_state(fixture).ok().filter(|v| v["previewVisible"] == true).map(|_| ())))?;
    let initial = split_state(fixture)?["preferredPreview"].as_i64().ok_or("Preview preference missing")?;
    let point = fixture.point("files-preview-resize", 0.5, 0.5)?;
    fixture.drag(children, pointer, point, -70.0, 0.0, false)?;
    wait("pointer preview width", || Ok(split_state(fixture).ok().filter(|v| v["preferredPreview"].as_i64().is_some_and(|n| n >= initial + 65)).map(|_| ())))?;
    let saved_preview = split_state(fixture)?["preferredPreview"].as_i64().ok_or("Preview width missing")?;
    fixture.key(&["-k", "Right"])?;
    wait("keyboard preview width", || Ok(split_state(fixture).ok().filter(|v| v["preferredPreview"] == saved_preview - 10).map(|_| ())))?;
    let saved_preview = saved_preview - 10;
    wait("persisted preview", || Ok(fs::read(&pref).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).filter(|v| v["previewWidth"] == saved_preview).map(|_| ())))?;
    fixture.invariant()?;
    checks.push("Actual preview pointer and keyboard resizing persists without changing selection");
    let original = fs::read(&pref)?;
    let point = fixture.point("files-preview-resize", 0.5, 0.5)?;
    fixture.drag(children, pointer, point, -40.0, 0.0, true)?;
    require(split_state(fixture)?["preferredPreview"] == saved_preview && fs::read(&pref)? == original, "Canceled preview resize changed saved preference")?;
    let node = fixture.window()?;
    fixture.resize(&node["id"], 480, 600)?;
    wait("narrow preview", || Ok(split_state(fixture).ok().filter(|v| v["previewVisible"] == false && v["previewOpen"] == true && v["previewMessage"].as_str().is_some_and(|s| !s.is_empty())).map(|_| ())))?;
    fixture.resize(&node["id"], 1200, 650)?;
    wait("restored preview", || Ok(split_state(fixture).ok().filter(|v| v["previewVisible"] == true && v["previewWidth"] == saved_preview).map(|_| ())))?;
    checks.push("Narrow window hides preview and restores persisted preferred width when widened");
    fixture.ipc("scale", &["1.5"])?;
    wait("text scale", || Ok(split_state(fixture).ok().filter(|v| v["scale"] == 1.5 && v["previewWidth"].as_i64().is_some_and(|n| n >= 240)).map(|_| ())))?;
    fixture.ipc("splitSetup", &["columns", "false"])?;
    wait("columns loaded", || Ok(split_state(fixture).ok().filter(|v| v["columnsLoaded"] == true).map(|_| ())))?;
    fixture.resize(&node["id"], 720, 650)?;
    wait("column window", || Ok(fixture.state().ok().filter(|v| v["width"] == 720).map(|_| ())))?;
    fixture.ipc("columnStart", &[])?;
    let point = fixture.point("files-column-resize-0", 0.5, 0.5)?;
    fixture.drag(children, pointer, point, 50.0, 0.0, false)?;
    wait("pointer column width", || Ok(split_state(fixture).ok().filter(|v| v["preferredColumns"][0].as_i64().is_some_and(|n| n >= 330) && v["columnX"].as_f64().is_some_and(|n| n < 1.0)).map(|_| ())))?;
    let saved_column = split_state(fixture)?["preferredColumns"][0].as_i64().ok_or("Column width missing")?;
    fixture.key(&["-k", "Right"])?;
    wait("keyboard column width", || Ok(split_state(fixture).ok().filter(|v| v["preferredColumns"][0] == saved_column + 10).map(|_| ())))?;
    let saved_column = saved_column + 10;
    wait("persisted column", || Ok(fs::read(&pref).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).filter(|v| v["columnWidths"][0] == saved_column).map(|_| ())))?;
    checks.push("First column pointer and keyboard resizing persists at 150 percent text scale");
    let idle_original = fs::read(&pref)?;
    fixture.ipc("columnOffset", &["60"])?;
    wait("column offset", || Ok(split_state(fixture).ok().filter(|v| v["columnX"].as_f64().is_some_and(|n| (n - 60.0).abs() < 1.0)).map(|_| ())))?;
    fixture.key(&["-k", "Escape"])?;
    require(split_state(fixture)?["columnX"].as_f64().is_some_and(|n| (n - 60.0).abs() < 1.0) && fs::read(&pref)? == idle_original, "Idle Escape changed viewport or saved preference")?;
    fixture.ipc("columnStart", &[])?;
    checks.push("Idle Escape preserves column viewport and preference bytes");
    let original = fs::read(&pref)?;
    let point = fixture.point("files-column-resize-0", 0.5, 0.5)?;
    fixture.drag(children, pointer, point, -45.0, 0.0, true)?;
    require(split_state(fixture)?["preferredColumns"][0] == saved_column && fs::read(&pref)? == original, "Canceled column resize changed saved preference")?;
    fixture.key(&["-k", "Home"])?;
    wait("column Home", || Ok(split_state(fixture).ok().filter(|v| v["preferredColumns"][0] == 300).map(|_| ())))?;
    fixture.key(&["-k", "End"])?;
    wait("column End", || Ok(split_state(fixture).ok().filter(|v| v["preferredColumns"][0].as_i64().is_some_and(|n| n >= 400)).map(|_| ())))?;
    let saved_column = split_state(fixture)?["preferredColumns"][0].as_i64().ok_or("Column width missing")?;
    wait("persisted End", || Ok(fs::read(&pref).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).filter(|v| v["columnWidths"][0] == saved_column).map(|_| ())))?;
    checks.push("Escape, Home and End preserve column constraints and preference bytes");
    let original = fs::read(&pref)?;
    call("swaymsg", &["output", "HEADLESS-1", "scale", "1.5"], &fixture.envs)?;
    wait("fractional output scale", || {
        let outputs: Value = serde_json::from_str(&call("swaymsg", &["-r", "-t", "get_outputs"], &fixture.envs)?)?;
        Ok(outputs.as_array().and_then(|items| items.iter().find(|v| v["name"] == "HEADLESS-1" && v["scale"] == 1.5)).map(|_| ()))
    })?;
    fixture.resize(&node["id"], 600, 600)?;
    wait("scaled narrow window", || Ok(fixture.state().ok().filter(|v| v["width"] == 600).map(|_| ())))?;
    require(split_state(fixture)?["preferredColumns"][0] == saved_column && fs::read(&pref)? == original, "Output scale changed saved column width")?;
    let handle: Value = serde_json::from_str(&fixture.ipc("geometry", &["files-column-resize-0"])?)?;
    require(handle["x"].as_f64().unwrap_or(9999.0) + handle["width"].as_f64().unwrap_or(9999.0) <= fixture.state()?["width"].as_f64().unwrap_or(0.0) - 8.0, "Column divider left visible window")?;
    call("swaymsg", &["output", "HEADLESS-1", "scale", "1"], &fixture.envs)?;
    fixture.resize(&node["id"], 720, 650)?;
    wait("unscaled window", || Ok(fixture.state().ok().filter(|v| v["width"] == 720).map(|_| ())))?;
    require(split_state(fixture)?["preferredColumns"][0] == saved_column && fs::read(&pref)? == original, "Restoring output scale changed preference")?;
    checks.push("Fractional output scale and resize retain preferred column width");
    fixture.screenshot("files-split-enlarged.png")?;
    fixture.invariant()?;
    let shell = &mut children.0[1];
    unsafe { libc::killpg(shell.id() as i32, libc::SIGTERM) };
    let _ = shell.wait()?;
    children.launch("quickshell", &["-p", fixture.qml.to_str().ok_or("QML path invalid")?], &fixture.envs, &fixture.out.join("shell-reopened.log"), false)?;
    wait("reopened split preferences", || Ok(split_state(fixture).ok().filter(|v| v["ready"] == true && v["preferredPreview"] == saved_preview && v["preferredColumns"][0] == saved_column).map(|_| ())))?;
    checks.push("New Files process restores both split preferences from Rust backend");
    let log = format!("{}\n{}", fs::read_to_string(fixture.out.join("shell.log"))?, fs::read_to_string(fixture.out.join("shell-reopened.log"))?);
    require(!["ReferenceError:", "TypeError:", "Binding loop", "Failed to load configuration", "Cannot assign"].iter().any(|marker| log.contains(marker)), &format!("QML errors: {log}"))?;
    fs::write(fixture.out.join("result.json"), serde_json::to_vec_pretty(&json!({"passed":true,"checks":checks,"final":split_state(fixture)?,"scope":"Actual pointer and keyboard in isolated compositor with Rust Files preferences"}))?)?;
    println!("PASS native Files split pane layout regression");
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let options = options()?;
        if options.inside {
            if options.history {
                history_inner(&options)
            } else if options.selection {
                selection_inner(&options)
            } else {
                inner(&options)
            }
        } else {
            outer(&options)
        }
    })();
    if let Err(error) = result {
        eprintln!("Files layout fixture failed: {error}");
        std::process::exit(1)
    }
}

fn history_wait(
    fixture: &Fixture,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value> {
    let result = wait(label, || {
        Ok(fixture.state().ok().filter(|state| predicate(state)))
    });
    if result.is_err() {
        if let Ok(state) = fixture.state() {
            let _ = fs::write(
                fixture.out.join("failure-state.json"),
                serde_json::to_vec_pretty(&state)?,
            );
        }
    }
    result
}
fn history_path(fixture: &Fixture, path: &str) -> Result<Value> {
    history_wait(fixture, "Files history route", |state| {
        state["ready"] == true && state["path"] == path
    })
}
fn history_offset(state: &Value, mode: &str, first: &str) -> Result<f64> {
    let scroll = &state["presentation"]["scroll"];
    if mode == "columns" {
        Ok(scroll["columns"]["columns"]
            .as_array()
            .ok_or("Columns missing")?
            .iter()
            .find(|column| column["path"] == first)
            .and_then(|column| column["offset"].as_f64())
            .unwrap_or(0.0))
    } else {
        scroll[mode]["offset"]
            .as_f64()
            .ok_or_else(|| fail("History viewport offset missing"))
    }
}
fn history_backend_pid(fixture: &Fixture, shell: u32) -> Result<i32> {
    let children = fs::read_to_string(format!("/proc/{shell}/task/{shell}/children"))?;
    let backend = fixture
        .envs
        .get("LUMEN_FILES_BIN")
        .ok_or("Files backend binary missing")?
        .as_bytes();
    for pid in children.split_whitespace() {
        let command = fs::read(format!("/proc/{pid}/cmdline"))?;
        if command.windows(backend.len()).any(|slice| slice == backend) {
            return Ok(pid.parse()?);
        }
    }
    Err(fail("Quickshell Files backend child missing"))
}
fn history_select(fixture: &Fixture) -> Result<()> {
    fixture.ipc("select", &[])?;
    Ok(())
}
fn history_navigate(fixture: &Fixture, path: &str) -> Result<()> {
    fixture.ipc("navigate", &[path])?;
    Ok(())
}
fn history_inner(options: &Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    let mut envs = launcher_environment(&options.launcher)?;
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        (
            "WLR_RENDERER",
            if options.effects { "gles2" } else { "pixman" },
        ),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
    ] {
        envs.insert(key.into(), value.into());
    }
    if options.effects {
        for (key, value) in [
            ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
            ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ] {
            envs.insert(key.into(), value.into());
        }
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        envs.remove(key);
    }
    let runtime = PathBuf::from(envs.get("XDG_RUNTIME_DIR").ok_or("Runtime missing")?);
    let home = PathBuf::from(envs.get("HOME").ok_or("Home missing")?);
    let mut folders = BTreeMap::new();
    for name in ["First", "Second", "Third"] {
        let folder = home.join(name);
        fs::create_dir(&folder)?;
        fs::write(
            folder.join("selected.txt"),
            format!("unchanged fixture {name}"),
        )?;
        for index in 0..600 {
            fs::write(folder.join(format!("document-{index:03}.txt")), "synthetic")?;
        }
        folders.insert(name.to_owned(), folder.display().to_string());
    }
    let repo = env::current_dir()?;
    let palette: Value = serde_json::from_slice(&fs::read(
        repo.join("res/lumen-desktop/appearance/palette.json"),
    )?)?;
    let appearance = options.output.join("appearance.json");
    fs::write(
        &appearance,
        json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}).to_string(),
    )?;
    envs.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    copy_tree(
        &repo.join("res/lumen-desktop"),
        &options.output.join("assets"),
    )?;
    let qml = options.output.join("shell.qml");
    fs::write(
        &qml,
        include_str!("../history.qml").replace("__PATHS__", &serde_json::to_string(&folders)?),
    )?;
    let mut fixture = Fixture {
        envs,
        qml,
        out: options.output.clone(),
        folders,
    };
    let mut children = Children(Vec::new());
    let mut checks = Vec::new();
    let config = fixture.out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n",
    )?;
    children.launch(
        options.sway.to_str().ok_or("Sway path invalid")?,
        &[
            "--unsupported-gpu",
            "-c",
            config.to_str().ok_or("Config path invalid")?,
        ],
        &fixture.envs,
        &fixture.out.join("sway.log"),
        false,
    )?;
    let socket = wait("Sway socket", || first_match(&runtime, "sway-ipc"))?;
    let display = wait("Wayland display", || first_wayland(&runtime))?;
    fixture
        .envs
        .insert("SWAYSOCK".into(), socket.display().to_string());
    fixture.envs.insert(
        "WAYLAND_DISPLAY".into(),
        display
            .file_name()
            .ok_or("Display missing")?
            .to_string_lossy()
            .into_owned(),
    );
    let shell = children.launch(
        "quickshell",
        &["-p", fixture.qml.to_str().ok_or("QML path invalid")?],
        &fixture.envs,
        &fixture.out.join("shell.log"),
        false,
    )?;
    history_wait(&fixture, "Files history initial rows", |state| {
        state["ready"] == true
    })?;
    history_select(&fixture)?;
    fixture.screenshot("files-selected.png")?;
    children.launch(
        "wtype",
        &["-s", "60000"],
        &fixture.envs,
        &fixture.out.join("keyboard.log"),
        false,
    )?;
    let pointer = children.launch(
        options.pointer.to_str().ok_or("Pointer path invalid")?,
        &["pointer", "move", "0", "0"],
        &fixture.envs,
        &fixture.out.join("pointer.log"),
        true,
    )?;
    thread::sleep(Duration::from_millis(200));
    let first = fixture.folders["First"].clone();
    let second = fixture.folders["Second"].clone();
    let third = fixture.folders["Third"].clone();
    for (mode, number) in [
        ("list", "2"),
        ("grid", "1"),
        ("gallery", "4"),
        ("columns", "3"),
    ] {
        history_navigate(&fixture, &first)?;
        history_path(&fixture, &first)?;
        history_select(&fixture)?;
        fixture.key(&["-M", "ctrl", "-k", number, "-m", "ctrl"])?;
        thread::sleep(Duration::from_millis(350));
        fixture.key(&["-k", "Up"])?;
        let row = wait("first visible Files row", || {
            Ok(
                serde_json::from_str::<Value>(&fixture.ipc("rowGeometry", &["0"])?)
                    .ok()
                    .filter(|value| !value.is_null()),
            )
        })?;
        let rect = fixture.window()?["rect"].clone();
        let coordinate = |axis: &str, dimension: &str| -> Result<f64> {
            Ok(rect[axis].as_f64().ok_or("Window coordinate missing")?
                + row[axis].as_f64().ok_or("Row coordinate missing")?
                + row[dimension].as_f64().ok_or("Row dimension missing")? / 2.0)
        };
        fixture.click(
            &mut children,
            pointer,
            (coordinate("x", "width")?, coordinate("y", "height")?),
        )?;
        for _ in 0..45 {
            fixture.key(&["-k", "Down"])?;
        }
        let before = history_wait(&fixture, "scrolled Files view", |state| {
            state["view"] == mode
                && state["selection"]
                    .as_array()
                    .is_some_and(|items| !items.is_empty())
                && history_offset(state, mode, &first).is_ok_and(|offset| offset > 0.0)
        })?;
        require(
            before["selection"]
                .as_array()
                .is_some_and(|array| !array.is_empty())
                && before["view"] == mode,
            &format!("{mode} selection missing: {before}"),
        )?;
        history_navigate(&fixture, &second)?;
        history_path(&fixture, &second)?;
        fixture.key(&["-M", "ctrl", "-k", "2", "-m", "ctrl"])?;
        let preference = Path::new(
            fixture
                .envs
                .get("XDG_STATE_HOME")
                .ok_or("State home missing")?,
        )
        .join("lumen-desktop/session/files.json");
        wait("global list preference", || {
            let data = fs::read(&preference)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            Ok(data.filter(|v| v["view"] == "list").map(|_| ()))
        })?;
        let preference_time = fs::metadata(&preference)?.modified()?;
        fixture.key(&["-M", "alt", "-k", "Left", "-m", "alt"])?;
        history_path(&fixture, &first)?;
        let initial = history_offset(&before, mode, &first)?;
        let after = history_wait(&fixture, "restored Files viewport", |state| {
            state["ready"] == true
                && state["path"] == first
                && state["view"] == mode
                && state["selection"] == before["selection"]
                && state["columnFacts"]
                    .as_array()
                    .is_some_and(|columns| columns.iter().all(|column| column["loaded"] == true))
                && history_offset(state, mode, &first)
                    .is_ok_and(|offset| (initial - offset).abs() < 3.0)
        })?;
        fs::write(
            fixture.out.join(format!("history-{mode}.json")),
            serde_json::to_vec_pretty(&json!({"before":before,"after":after}))?,
        )?;
        require(
            after["selection"] == before["selection"] && after["view"] == mode,
            &format!("{mode} history presentation changed: {after}"),
        )?;
        let saved_preference: Value = serde_json::from_slice(&fs::read(&preference)?)?;
        require(
            saved_preference["view"] == "list"
                && fs::metadata(&preference)?.modified()? == preference_time,
            "History restoration overwrote global view preference",
        )?;
        let restored = history_offset(&after, mode, &first)?;
        require(
            initial > 0.0 && (initial - restored).abs() < 3.0,
            &format!("{mode} viewport not restored: {initial} -> {restored}"),
        )?;
        fixture.key(&["-k", "F5"])?;
        history_wait(&fixture, "refresh", |v| v["ready"] == true)?;
        thread::sleep(Duration::from_millis(250));
        require(
            fixture.state()?["selection"] == before["selection"],
            "Refresh changed selection",
        )?;
        let committed = fixture.state()?;
        history_navigate(&fixture, &format!("{first}/not-a-directory"))?;
        history_wait(&fixture, "failed navigation", |v| {
            v["ready"] == true && v["error"].as_str().is_some_and(|s| !s.is_empty())
        })?;
        thread::sleep(Duration::from_millis(250));
        let failed = fixture.state()?;
        require(
            failed["path"] == committed["path"]
                && failed["historyIndex"] == committed["historyIndex"]
                && failed["selection"] == committed["selection"],
            "Failed navigation changed committed entry",
        )?;
        if mode == "list" {
            let previous = fixture.state()?["selection"].clone();
            for _ in 0..2 {
                fixture.ipc("sort", &[])?;
                history_wait(&fixture, "sort", |v| v["ready"] == true)?;
                thread::sleep(Duration::from_millis(300));
                require(
                    fixture.state()?["selection"] == previous,
                    "Sort changed captured object selection",
                )?;
            }
            checks.push("Changed sort coordinates reconcile captured object identity without selecting a replacement index".to_owned());
        }
        checks.push(format!("{mode} selection, view and scrolled viewport survive Back, refresh and failed navigation"));
    }
    let first_tab = fixture.state()?;
    let pos = fixture.point("document-tab-second", 0.4, 0.5)?;
    fixture.click(&mut children, pointer, pos)?;
    history_wait(&fixture, "second tab", |v| {
        v["ready"] == true && v["activeId"] == "second"
    })?;
    history_select(&fixture)?;
    fixture.key(&["-k", "Down"])?;
    let second_tab = fixture.state()?;
    let pos = fixture.point("document-tab-first", 0.4, 0.5)?;
    fixture.click(&mut children, pointer, pos)?;
    history_wait(&fixture, "first tab", |v| {
        v["ready"] == true && v["activeId"] == "first"
    })?;
    thread::sleep(Duration::from_millis(300));
    let state = fixture.state()?;
    require(
        state["selection"] == first_tab["selection"]
            && state["historyIndex"] == first_tab["historyIndex"],
        "First tab lost committed entry",
    )?;
    let pos = fixture.point("document-tab-second", 0.4, 0.5)?;
    fixture.click(&mut children, pointer, pos)?;
    history_wait(&fixture, "second tab again", |v| {
        v["ready"] == true && v["activeId"] == "second"
    })?;
    thread::sleep(Duration::from_millis(300));
    require(
        fixture.state()?["selection"] == second_tab["selection"],
        "Second tab lost selection",
    )?;
    checks
        .push("Actual tab pointer switches retain independent history entry selection".to_owned());
    history_select(&fixture)?;
    for path in [&first, &second, &third] {
        history_navigate(&fixture, path)?;
        history_path(&fixture, path)?;
    }
    fixture.key(&["-M", "alt", "-k", "Left", "-m", "alt"])?;
    history_path(&fixture, &second)?;
    history_navigate(&fixture, &first)?;
    history_path(&fixture, &first)?;
    let branch = fixture.state()?;
    fixture.key(&["-M", "alt", "-k", "Right", "-m", "alt"])?;
    thread::sleep(Duration::from_millis(200));
    let state = fixture.state()?;
    require(
        state["historyIndex"] == branch["historyIndex"] && state["path"] == branch["path"],
        "Successful branch retained forward history",
    )?;
    checks.push("Successful branch truncates forward entries; forward shortcut stays at committed destination".to_owned());
    history_wait(&fixture, "settled branch presentation", |state| {
        state["ready"] == true && state["restoring"] == false
    })?;
    let window = fixture.window()?;
    call(
        "swaymsg",
        &[&format!(
            "[con_id={}] focus",
            window["id"].as_i64().ok_or("Files window ID missing")?
        )],
        &fixture.envs,
    )?;
    fixture.key(&["-M", "ctrl", "-k", "f", "-m", "ctrl"])?;
    history_wait(&fixture, "Files filter focus", |state| {
        state["focusFilter"] == true
    })?;
    fixture.key(&[
        "-k", "d", "-k", "o", "-k", "c", "-k", "u", "-k", "m", "-k", "e", "-k", "n", "-k", "t",
        "-k", "minus", "-k", "0", "-k", "0",
    ])?;
    history_wait(&fixture, "typed Files filter", |v| {
        v["ready"] == true && v["presentation"]["query"] == "document-00"
    })?;
    let filtered = fixture.state()?;
    history_navigate(&fixture, &second)?;
    history_path(&fixture, &second)?;
    fixture.ipc("back", &[])?;
    history_path(
        &fixture,
        filtered["path"].as_str().ok_or("Filtered path missing")?,
    )?;
    thread::sleep(Duration::from_millis(300));
    require(
        fixture.state()?["presentation"]["query"] == "document-00",
        "History filter query missing",
    )?;
    checks.push("Actual filter typing returns with history query and matching rows".to_owned());
    history_select(&fixture)?;
    let before_race = fixture.state()?;
    let backend = history_backend_pid(&fixture, children.0[shell].id())?;
    require(
        unsafe { libc::kill(backend, libc::SIGSTOP) } == 0,
        "Cannot pause Files backend",
    )?;
    let race_requests = (|| -> Result<()> {
        history_navigate(&fixture, &second)?;
        history_navigate(&fixture, &third)?;
        Ok(())
    })();
    unsafe { libc::kill(backend, libc::SIGCONT) };
    race_requests?;
    history_path(&fixture, &third)?;
    let race = fixture.state()?;
    require(
        race["history"].as_array().is_some_and(|items| {
            items.len() == before_race["historyIndex"].as_u64().unwrap_or(0) as usize + 2
        }),
        "Paused backend race committed intermediate route",
    )?;
    checks.push(
        "Actual paused backend response race commits only latest requested destination".to_owned(),
    );
    let committed = fixture.state()?;
    let backend = history_backend_pid(&fixture, children.0[shell].id())?;
    require(
        unsafe { libc::kill(backend, libc::SIGKILL) } == 0,
        "Cannot terminate Files backend",
    )?;
    wait("backend process termination", || {
        let status = fs::read_to_string(format!("/proc/{backend}/status"));
        Ok(status
            .as_ref()
            .map_or(true, |text| {
                text.lines()
                    .any(|line| line.starts_with("State:") && line.contains("(zombie)"))
            })
            .then_some(()))
    })?;
    fixture.key(&["-k", "F5"])?;
    history_wait(&fixture, "backend reconnect", |v| v["ready"] == true)?;
    thread::sleep(Duration::from_millis(1000));
    let state = fixture.state()?;
    require(
        state["historyIndex"] == committed["historyIndex"]
            && state["selection"] == committed["selection"],
        "Reconnect changed committed entry",
    )?;
    checks.push(
        "Actual backend termination and keyboard reconnect retain committed entry state".to_owned(),
    );
    let before_overflow = fixture.state()?;
    require(
        before_overflow["presentation"]["query"] == "",
        "Overflow setup retained a filter query",
    )?;
    let row: Value = serde_json::from_str(&fixture.ipc("rowGeometry", &["0"])?)?;
    require(!row.is_null(), "Overflow setup has no visible row")?;
    let rect = fixture.window()?["rect"].clone();
    let x = rect["x"].as_f64().ok_or("Window x missing")?
        + row["x"].as_f64().ok_or("Row x missing")?
        + row["width"].as_f64().ok_or("Row width missing")? / 2.0;
    let y = rect["y"].as_f64().ok_or("Window y missing")?
        + row["y"].as_f64().ok_or("Row y missing")?
        + row["height"].as_f64().ok_or("Row height missing")? / 2.0;
    fixture.click(&mut children, pointer, (x, y))?;
    fixture.key(&["-M", "ctrl", "a", "-m", "ctrl"])?;
    require(
        fixture.state()?["selectionCount"].as_u64().unwrap_or(0) > 512,
        "Select All did not exceed history bound",
    )?;
    history_navigate(&fixture, &second)?;
    history_path(&fixture, &second)?;
    fixture.key(&["-M", "alt", "-k", "Left", "-m", "alt"])?;
    history_path(
        &fixture,
        committed["path"].as_str().ok_or("Committed path missing")?,
    )?;
    thread::sleep(Duration::from_millis(300));
    let overflow = fixture.state()?;
    require(
        overflow["selection"] == json!([])
            && overflow["presentation"]["current"].is_null()
            && overflow["warning"]
                .as_str()
                .is_some_and(|s| s.contains("selection")),
        "Large selection restored misleading subset",
    )?;
    checks.push("Large Select All restores no partial selection and reports explicit cleared-selection notice".to_owned());
    let inactive = fixture.state()?["tabs"]
        .as_array()
        .ok_or("Tabs missing")?
        .iter()
        .find(|v| v["id"] == "first")
        .ok_or("First tab missing")?
        .clone();
    fs::remove_dir_all(&first)?;
    let pos = fixture.point("document-tab-first", 0.4, 0.5)?;
    fixture.click(&mut children, pointer, pos)?;
    history_wait(&fixture, "failed first tab", |v| {
        v["activeId"] == "first"
            && (v["backendReady"] == false || v["error"].as_str().is_some_and(|s| !s.is_empty()))
    })?;
    let failed = fixture.state()?;
    let first_failed = failed["tabs"]
        .as_array()
        .ok_or("Tabs missing")?
        .iter()
        .find(|v| v["id"] == "first")
        .ok_or("Failed first tab missing")?;
    require(
        failed["path"] == first && first_failed["history"] == inactive["history"],
        "Failed tab overwrote stored entry stack",
    )?;
    checks.push(
        "Failed tab destination never substitutes prior tab rows or overwrites stored stack"
            .to_owned(),
    );
    let pos = fixture.point("document-tab-second", 0.4, 0.5)?;
    fixture.click(&mut children, pointer, pos)?;
    history_wait(&fixture, "healthy second tab", |v| {
        v["activeId"] == "second" && v["ready"] == true
    })?;
    history_select(&fixture)?;
    let selected = fixture.state()?["selection"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .ok_or("Selected file missing")?
        .to_owned();
    let original = PathBuf::from(&selected);
    fs::rename(&original, original.with_file_name("retired-identity.txt"))?;
    fs::write(&original, "synthetic replacement")?;
    fixture.key(&["-k", "F5"])?;
    history_wait(&fixture, "identity refresh", |v| v["ready"] == true)?;
    thread::sleep(Duration::from_millis(300));
    require(
        !fixture.state()?["selection"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == &selected)),
        "Refresh transferred identity to replacement inode",
    )?;
    checks.push(
        "Refresh never transfers known file identity to replacement at identical path".to_owned(),
    );
    let log = fs::read_to_string(fixture.out.join("shell.log"))?;
    require(
        ![
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Failed to load configuration",
            "Cannot assign",
        ]
        .iter()
        .any(|m| log.contains(m)),
        &format!("QML errors: {log}"),
    )?;
    fs::write(
        fixture.out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"checks":checks,"final":fixture.state()?}),
        )?,
    )?;
    println!("PASS native Files history regression");
    Ok(())
}

struct SelectionFixture(Fixture);
impl SelectionFixture {
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut parts = vec![
            "-p",
            self.0.qml.to_str().ok_or("Invalid QML path")?,
            "ipc",
            "call",
            "selectionAudit",
            method,
        ];
        parts.extend_from_slice(args);
        call("quickshell", &parts, &self.0.envs)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("state", &[])?)?)
    }
    fn geometry(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("geometry", &[])?)?)
    }
    fn wait(&self, label: &str, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
        let result = wait(label, || {
            Ok(self.state().ok().filter(|value| predicate(value)))
        });
        if result.is_err() {
            if let Ok(value) = self.state() {
                let _ = fs::write(
                    self.0.out.join("failed-state.json"),
                    serde_json::to_vec_pretty(&value)?,
                );
            }
            if let Ok(value) = self.geometry() {
                let _ = fs::write(
                    self.0.out.join("failed-geometry.json"),
                    serde_json::to_vec_pretty(&value)?,
                );
            }
            if let Ok(value) = call("swaymsg", &["-r", "-t", "get_tree"], &self.0.envs) {
                let _ = fs::write(self.0.out.join("failed-tree.json"), value);
            }
            let _ = self.0.screenshot("failure.png");
        }
        result
    }
    fn box_geometry(&self) -> Result<Value> {
        wait("selection gutter geometry", || {
            Ok(self.geometry().ok().filter(|value| !value.is_null()))
        })
    }
    fn point(&self, position: &Value, children: &mut Children, pointer: usize) -> Result<()> {
        let rect = self.0.window()?["rect"].clone();
        let x = rect["x"].as_f64().ok_or("Files window x missing")?
            + position["x"].as_f64().ok_or("Selection x missing")?;
        let y = rect["y"].as_f64().ok_or("Files window y missing")?
            + position["y"].as_f64().ok_or("Selection y missing")?;
        self.0.move_to(children, pointer, x, y)
    }
    fn button(&self, children: &mut Children, pointer: usize, action: &str) -> Result<()> {
        self.0.pointer(children, pointer, action)
    }
    fn begin(&self, box_geometry: &Value, children: &mut Children, pointer: usize) -> Result<()> {
        self.point(&box_geometry["start"], children, pointer)?;
        self.button(children, pointer, "p")?;
        self.wait("selection pending", |v| v["pending"] == true)?;
        self.point(&box_geometry["end"], children, pointer)?;
        self.wait("selection active", |v| v["active"] == true)?;
        Ok(())
    }
    fn record(&self, checks: &mut Vec<String>, message: String) -> Result<()> {
        checks.push(message);
        fs::write(
            self.0.out.join("progress.json"),
            serde_json::to_vec_pretty(&json!({"checks":checks}))?,
        )?;
        Ok(())
    }
    fn tree_node_named(&self, name: &str) -> Result<Option<Value>> {
        let tree: Value =
            serde_json::from_str(&call("swaymsg", &["-r", "-t", "get_tree"], &self.0.envs)?)?;
        let mut found = Vec::new();
        nodes(&tree, &mut found);
        Ok(found.into_iter().find(|v| v["name"] == name).cloned())
    }
    fn focus_window(&self, node: &Value) -> Result<()> {
        call(
            "swaymsg",
            &[&format!(
                "[con_id={}] focus",
                node["id"].as_i64().ok_or("Window ID missing")?
            )],
            &self.0.envs,
        )?;
        Ok(())
    }
    fn drag_active(&self) -> Result<bool> {
        let seats: Value =
            serde_json::from_str(&call("swaymsg", &["-r", "-t", "get_seats"], &self.0.envs)?)?;
        Ok(seats
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["lumen_drag"]["active"] == true)))
    }
}
fn selection_inner(options: &Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    let mut envs = launcher_environment(&options.launcher)?;
    let runtime = PathBuf::from(envs.get("XDG_RUNTIME_DIR").ok_or("Runtime missing")?);
    for (key, value) in [
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
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
        ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
    ] {
        envs.insert(key.into(), value.into());
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        envs.remove(key);
    }
    envs.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/no-system-bus", runtime.display()),
    );
    envs.insert(
        "PULSE_SERVER".into(),
        format!("unix:{}/no-audio", runtime.display()),
    );
    envs.insert("PIPEWIRE_RUNTIME_DIR".into(), runtime.display().to_string());
    envs.insert("PIPEWIRE_REMOTE".into(), "no-pipewire".into());
    let home = PathBuf::from(envs.get("HOME").ok_or("HOME missing")?);
    let docs = home.join("Documents");
    fs::create_dir(&docs)?;
    for index in 0..160 {
        fs::write(
            docs.join(format!("item-{index:03}.txt")),
            format!("synthetic {index}"),
        )?;
    }
    let assets_source = PathBuf::from(
        envs.get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Installed assets missing")?,
    );
    let assets = options.output.join("assets");
    copy_tree(&assets_source, &assets)?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = options.output.join("appearance.json");
    fs::write(
        &appearance,
        json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}).to_string(),
    )?;
    envs.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let settings = options.output.join("settings.json");
    fs::write(&settings, "{}")?;
    envs.insert(
        "LUMEN_NIX_SETTINGS_ACTIVE".into(),
        settings.display().to_string(),
    );
    let qml = options.output.join("shell.qml");
    fs::write(
        &qml,
        include_str!("../selection.qml").replace(
            "__DOCS__",
            &serde_json::to_string(&docs.display().to_string())?,
        ),
    )?;
    let fixture = SelectionFixture(Fixture {
        envs,
        qml,
        out: options.output.clone(),
        folders: BTreeMap::new(),
    });
    let mut children = Children(Vec::new());
    let mut checks = Vec::new();
    let file_jobs = fixture
        .0
        .envs
        .get("LUMEN_FILE_JOBS_BIN")
        .ok_or("File jobs binary missing")?;
    children.launch(
        file_jobs,
        &["serve"],
        &fixture.0.envs,
        &fixture.0.out.join("file-jobs.log"),
        false,
    )?;
    wait("Files operations socket", || {
        Ok(runtime
            .join("lumen-file-operations.sock")
            .exists()
            .then_some(()))
    })?;
    let config = fixture.0.out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n",
    )?;
    children.launch(
        options.sway.to_str().ok_or("Sway path invalid")?,
        &[
            "--unsupported-gpu",
            "-c",
            config.to_str().ok_or("Sway config invalid")?,
        ],
        &fixture.0.envs,
        &fixture.0.out.join("sway.log"),
        false,
    )?;
    let socket = wait("Sway socket", || first_match(&runtime, "sway-ipc"))?;
    let display = wait("Wayland display", || first_wayland(&runtime))?;
    let mut fixture = fixture;
    fixture
        .0
        .envs
        .insert("SWAYSOCK".into(), socket.display().to_string());
    fixture.0.envs.insert(
        "WAYLAND_DISPLAY".into(),
        display
            .file_name()
            .ok_or("Display missing")?
            .to_string_lossy()
            .into_owned(),
    );
    children.launch(
        "quickshell",
        &["-p", fixture.0.qml.to_str().ok_or("QML path invalid")?],
        &fixture.0.envs,
        &fixture.0.out.join("shell.log"),
        false,
    )?;
    fixture.wait("Files rows", |v| v["ready"] == true)?;
    let files_window = fixture.0.window()?;
    fixture.focus_window(&files_window)?;
    children.launch(
        "wtype",
        &["-s", "60000"],
        &fixture.0.envs,
        &fixture.0.out.join("keyboard.log"),
        false,
    )?;
    let pointer = children.launch(
        options.pointer.to_str().ok_or("Pointer binary invalid")?,
        &["pointer", "move", "0", "0"],
        &fixture.0.envs,
        &fixture.0.out.join("pointer.log"),
        true,
    )?;
    let views = if options.views.is_empty() {
        vec![
            "list".to_owned(),
            "grid".to_owned(),
            "gallery".to_owned(),
            "columns".to_owned(),
        ]
    } else {
        options.views.clone()
    };
    for mode in &views {
        fixture.ipc("mode", &[mode])?;
        let box_geometry = fixture.box_geometry()?;
        require(
            box_geometry["populated"] == true,
            &format!("{mode} viewport not populated: {box_geometry}"),
        )?;
        fs::write(
            fixture.0.out.join(format!("geometry-{mode}.json")),
            serde_json::to_vec_pretty(&box_geometry)?,
        )?;
        let before = fixture.state()?["selection"].clone();
        fixture.begin(&box_geometry, &mut children, pointer)?;
        let state = fixture.state()?;
        require(
            state["selection"] == before
                && state["preview"].as_array().is_some_and(|v| !v.is_empty()),
            "Band preview wrote selection early",
        )?;
        fixture.button(&mut children, pointer, "r")?;
        fixture.wait("selection band committed", |v| {
            v["pending"] == false
                && v["selection"]
                    .as_array()
                    .is_some_and(|a| a.contains(&box_geometry["path"]))
        })?;
        fixture.record(&mut checks,format!("{mode}: actual blank gutter starts preview without writes; release selects exact row"))?;
        let before = fixture.state()?["selection"].clone();
        fixture.begin(&box_geometry, &mut children, pointer)?;
        fixture.0.key(&["-k", "Escape"])?;
        fixture.button(&mut children, pointer, "r")?;
        let state = fixture.state()?;
        require(
            state["pending"] == false && state["selection"] == before && state["preview"].is_null(),
            "Escape committed band selection",
        )?;
        fixture.record(
            &mut checks,
            format!("{mode}: physical Escape cancels without committing preview"),
        )?;
        fixture.0.key(&["-M", "ctrl"])?;
        fixture.begin(&box_geometry, &mut children, pointer)?;
        fixture.button(&mut children, pointer, "r")?;
        fixture.0.key(&["-m", "ctrl"])?;
        fixture.wait("control band", |v| v["pending"] == false)?;
        require(
            !fixture.state()?["selection"]
                .as_array()
                .is_some_and(|a| a.contains(&box_geometry["path"])),
            "Control band did not toggle captured membership",
        )?;
        fixture.record(
            &mut checks,
            format!("{mode}: physical Control band toggles captured membership"),
        )?;
        fixture.0.key(&["-M", "shift"])?;
        fixture.begin(&box_geometry, &mut children, pointer)?;
        fixture.button(&mut children, pointer, "r")?;
        fixture.0.key(&["-m", "shift"])?;
        fixture.wait("shift band", |v| {
            v["pending"] == false
                && v["selection"]
                    .as_array()
                    .is_some_and(|a| a.contains(&box_geometry["path"]))
        })?;
        fixture.record(
            &mut checks,
            format!("{mode}: physical Shift band adds exact captured membership"),
        )?;
        fixture.point(&box_geometry["row"], &mut children, pointer)?;
        fixture.button(&mut children, pointer, "p")?;
        fixture.button(&mut children, pointer, "r")?;
        fixture.wait("delegate pointer selection", |v| {
            v["selection"] == json!([box_geometry["path"]])
        })?;
        fixture.record(
            &mut checks,
            format!("{mode}: ordinary delegate pointer selection remains available"),
        )?;
        let before = fixture.state()?["selection"].clone();
        fixture.begin(&box_geometry, &mut children, pointer)?;
        fixture.ipc("reverse", &[])?;
        fixture.wait("resort cancellation", |v| {
            v["pending"] == false && v["ready"] == true
        })?;
        fixture.button(&mut children, pointer, "r")?;
        let state = fixture.state()?;
        require(
            state["selection"] == before && state["preview"].is_null(),
            "Resort changed held selection",
        )?;
        fixture.record(
            &mut checks,
            format!("{mode}: actual resort cancels held geometry without selecting another object"),
        )?;
        let box_geometry = fixture.box_geometry()?;
        fixture.point(&box_geometry["row"], &mut children, pointer)?;
        fixture.button(&mut children, pointer, "p")?;
        fixture.button(&mut children, pointer, "r")?;
        let ordered = fixture.state()?["rowPaths"]
            .as_array()
            .ok_or("File rows missing")?
            .clone();
        let index = ordered
            .iter()
            .position(|v| v == &box_geometry["path"])
            .ok_or("Captured row missing")?;
        let step = box_geometry["step"].as_u64().ok_or("Grid step missing")? as usize;
        let next_index = (index + step).min(ordered.len() - 1);
        fixture
            .0
            .key(&["-M", "shift", "-k", "Down", "-m", "shift"])?;
        fixture.wait("shift range", |v| v["current"] == ordered[next_index])?;
        require(
            fixture.state()?["selection"] == json!(ordered[index..=next_index]),
            "Shift Arrow selected wrong anchored range",
        )?;
        fixture.record(
            &mut checks,
            format!("{mode}: physical Shift+Arrow selects exact anchored range after resort"),
        )?;
        fixture.0.screenshot(&format!("selection-{mode}.png"))?;
        let box_geometry = fixture.box_geometry()?;
        let before = fixture.state()?["selection"].clone();
        fixture.begin(&box_geometry, &mut children, pointer)?;
        fixture.button(&mut children, pointer, "w 1")?;
        fixture.wait("wheel cancellation", |v| v["pending"] == false)?;
        fixture.button(&mut children, pointer, "r")?;
        let state = fixture.state()?;
        require(
            state["selection"] == before && state["preview"].is_null(),
            "Wheel committed band preview",
        )?;
        fixture.record(
            &mut checks,
            format!("{mode}: physical wheel cancels held viewport preview"),
        )?;
        let box_geometry = fixture.box_geometry()?;
        let before = fixture.state()?["selection"].clone();
        fixture.begin(&box_geometry, &mut children, pointer)?;
        fixture.ipc("showBystander", &["true"])?;
        let other = wait("selection bystander", || {
            fixture.tree_node_named("Selection bystander")
        })?;
        fixture.focus_window(&other)?;
        fixture.wait("focus transfer cancellation", |v| {
            v["pending"] == false && v["filesActive"] == false
        })?;
        fixture.button(&mut children, pointer, "r")?;
        let state = fixture.state()?;
        require(
            state["selection"] == before && state["preview"].is_null(),
            "Focus transfer committed band preview",
        )?;
        fixture.ipc("showBystander", &["false"])?;
        fixture.focus_window(&files_window)?;
        fixture.wait("Files refocus", |v| v["filesActive"] == true)?;
        fixture.record(
            &mut checks,
            format!("{mode}: actual focus transfer cancels without stealing focus or committing"),
        )?;
        let box_geometry = fixture.box_geometry()?;
        let before = fixture.state()?["selection"].clone();
        fixture.begin(&box_geometry, &mut children, pointer)?;
        let locker = children.launch(
            options
                .lock_client
                .as_ref()
                .ok_or("Lock client missing")?
                .to_str()
                .ok_or("Lock path invalid")?,
            &[],
            &fixture.0.envs,
            &fixture.0.out.join(format!("lock-{mode}.log")),
            true,
        )?;
        fixture.wait("session lock cancellation", |v| {
            v["lock"] == json!({"known":true,"locked":true}) && v["pending"] == false
        })?;
        let state = fixture.state()?;
        require(
            state["selection"] == before && state["preview"].is_null(),
            "Session lock committed band preview",
        )?;
        fixture.button(&mut children, pointer, "r")?;
        let stdin = children.0[locker]
            .stdin
            .as_mut()
            .ok_or("Lock stdin missing")?;
        stdin.write_all(b"u\n")?;
        stdin.flush()?;
        wait("lock client exit", || {
            Ok(children.0[locker]
                .try_wait()?
                .map(|status| status.success())
                .filter(|ok| *ok))
        })?;
        fixture.wait("session unlocked", |v| v["permitted"] == true)?;
        fixture.focus_window(&files_window)?;
        fixture.wait("Files active after unlock", |v| v["filesActive"] == true)?;
        fixture.record(&mut checks,format!("{mode}: actual native session lock invalidates held selection and restores usable state after unlock"))?;
        if mode == "list" || mode == "grid" {
            let box_geometry = fixture.box_geometry()?;
            fixture.point(&box_geometry["row"], &mut children, pointer)?;
            fixture.button(&mut children, pointer, "p")?;
            let row = &box_geometry["row"];
            let x = row["x"].as_f64().ok_or("Row x missing")?;
            let y = row["y"].as_f64().ok_or("Row y missing")?;
            fixture.point(&json!({"x":x+20.0,"y":y+20.0}), &mut children, pointer)?;
            fixture.point(&json!({"x":x+40.0,"y":y+40.0}), &mut children, pointer)?;
            wait("native file drag", || {
                Ok(fixture.drag_active()?.then_some(()))
            })?;
            require(
                fixture.state()?["pending"] == false,
                "Native file drag started band selection",
            )?;
            fixture.0.key(&["-k", "Escape"])?;
            fixture.button(&mut children, pointer, "r")?;
            wait("file drag canceled", || {
                Ok((!fixture.drag_active()?).then_some(()))
            })?;
            fixture.record(
                &mut checks,
                format!(
                    "{mode}: actual delegate drag starts native offer rather than a selection band"
                ),
            )?;
        }
    }
    fixture.0.screenshot("selection.png")?;
    let log = fs::read_to_string(fixture.0.out.join("shell.log"))?;
    require(
        ![
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Failed to load configuration",
            "Cannot assign",
            "Accessible attached property must",
        ]
        .iter()
        .any(|marker| log.contains(marker)),
        &format!("QML error: {log}"),
    )?;
    let source = call(
        "git",
        &[
            "-C",
            env::current_dir()?.to_str().ok_or("Source path invalid")?,
            "rev-parse",
            "HEAD",
        ],
        &fixture.0.envs,
    )?;
    let fixture_sha = hex::encode(Sha256::digest(
        [
            include_bytes!("main.rs").as_slice(),
            include_bytes!("../selection.qml").as_slice(),
        ]
        .concat(),
    ));
    let mut assets_sha = BTreeMap::new();
    asset_hashes(&assets, &assets, &mut assets_sha)?;
    fs::write(
        fixture.0.out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"checks":checks,"source":source,"launcher":options.launcher,"sway":options.sway,"effects":options.effects,"fixtureSha256":fixture_sha,"assetsSha256":assets_sha,"scope":"Installed Files viewport band selection with private file jobs, native pointer and keyboard, session lock and no host files"}),
        )?,
    )?;
    println!("PASS native Files viewport band selection regression");
    Ok(())
}
fn asset_hashes(root: &Path, folder: &Path, values: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            asset_hashes(root, &path, values)?;
        } else {
            let relative = path.strip_prefix(root)?.to_string_lossy().into_owned();
            values.insert(relative, hex::encode(Sha256::digest(fs::read(&path)?)));
        }
    }
    Ok(())
}
