//! Real Files batch-rename dialog with physical pointer and keyboard input.
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
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    output: PathBuf,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut pointer = None;
    let mut output = None;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
            Some("--pointer-tool") => {
                pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown batch rename fixture option")),
        }
    }
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        pointer: pointer.ok_or("--pointer-tool required")?,
        output: output.ok_or("--output required")?,
        effects,
        inside,
    })
}
fn private_environment(root: &Path) -> BTreeMap<String, String> {
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    values.insert("HOME".into(), root.display().to_string());
    values.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    values.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/no-system-bus", root.display()),
    );
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
    let mut command = Command::new("timeout");
    command
        .arg("120s")
        .arg("dbus-run-session")
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--launcher")
        .arg(&options.launcher)
        .arg("--sway")
        .arg(&options.sway)
        .arg("--pointer-tool")
        .arg(&options.pointer)
        .arg("--output")
        .arg(&options.output)
        .env_clear()
        .envs(private_environment(private.path()));
    if options.effects {
        command.arg("--effects");
    }
    let status = command.status()?;
    require(
        status.success(),
        &format!("Private batch rename fixture exited {status}"),
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
        .arg("10s")
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
        let stream = File::create(log)?;
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
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!(
                "Batch input condition timed out ({label}): {last}"
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
fn nodes<'a>(root: &'a Value, found: &mut Vec<&'a Value>) {
    found.push(root);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = root[key].as_array() {
            for child in children {
                nodes(child, found);
            }
        }
    }
}
struct Fixture {
    envs: BTreeMap<String, String>,
    qml: PathBuf,
    docs: PathBuf,
    out: PathBuf,
}
impl Fixture {
    fn ipc(&self, method: &str, values: &[&str]) -> Result<String> {
        let mut args = vec![
            "-p",
            self.qml.to_str().ok_or("Invalid fixture QML")?,
            "ipc",
            "call",
            "batchAudit",
            method,
        ];
        args.extend_from_slice(values);
        call("quickshell", &args, &self.envs)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("state", &[])?)?)
    }
    fn window(&self) -> Result<Option<Value>> {
        let tree: Value =
            serde_json::from_str(&call("swaymsg", &["-r", "-t", "get_tree"], &self.envs)?)?;
        let mut found = Vec::new();
        nodes(&tree, &mut found);
        Ok(found
            .into_iter()
            .find(|node| {
                node["app_id"] == "org.quickshell"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.starts_with("Files"))
            })
            .cloned())
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        call("wtype", args, &self.envs)?;
        thread::sleep(Duration::from_millis(120));
        Ok(())
    }
    fn click(&self, name: &str, children: &mut Children, pointer: usize) -> Result<()> {
        let box_value: Value = wait("dialog geometry", || {
            let value: Value = serde_json::from_str(&self.ipc("geometry", &[name])?)?;
            Ok((!value.is_null()).then_some(value))
        })?;
        require(box_value["visible"] == true, "Batch control not visible")?;
        let native = self.window()?.ok_or("Files window missing")?;
        let rect = &native["rect"];
        let coordinate = |axis: &str, extent: &str| -> Result<String> {
            let origin = rect[axis].as_f64().ok_or("Native origin missing")?;
            let offset = box_value[axis].as_f64().ok_or("Control origin missing")?;
            let size = box_value[extent]
                .as_f64()
                .ok_or("Control dimension missing")?;
            Ok(((origin + offset + size / 2.0).round() as i64).to_string())
        };
        let x = coordinate("x", "width")?;
        let y = coordinate("y", "height")?;
        call(
            "swaymsg",
            &["seat", "seat0", "cursor", "set", &x, &y],
            &self.envs,
        )?;
        let input = children.0[pointer]
            .stdin
            .as_mut()
            .ok_or("Pointer input missing")?;
        input.write_all(b"m 0.5 0.5\np\nr\n")?;
        input.flush()?;
        thread::sleep(Duration::from_millis(150));
        Ok(())
    }
    fn field(
        &self,
        name: &str,
        value: &str,
        children: &mut Children,
        pointer: usize,
    ) -> Result<()> {
        self.click(name, children, pointer)?;
        self.key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        self.key(&[value])
    }
    fn open_batch(&self) -> Result<()> {
        wait("Files ready", || {
            Ok((self.state()?["ready"] == true).then_some(()))
        })?;
        self.key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        wait("two selected files", || {
            Ok(self.state()?["selection"]
                .as_array()
                .is_some_and(|rows| rows.len() == 2)
                .then_some(()))
        })?;
        self.key(&["-k", "F2"])?;
        wait("batch rename open", || {
            Ok((self.state()?["open"] == true).then_some(()))
        })
    }
    fn preview(&self, children: &mut Children, pointer: usize) -> Result<()> {
        let previous = self.state()?["reply"]["id"].as_i64().unwrap_or(0);
        self.click("batch-rename-preview-button", children, pointer)?;
        wait("rename preview reply", || {
            let state = self.state()?;
            Ok((state["reply"]["action"] == "rename-preview"
                && state["reply"]["id"].as_i64().unwrap_or(0) > previous
                && state["error"].as_str().is_some_and(str::is_empty))
            .then_some(()))
        })
    }
}
fn names(folder: &Path) -> Result<Vec<String>> {
    let mut names = fs::read_dir(folder)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}
fn sha(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
fn asset_hashes(
    folder: &Path,
    relative: &Path,
    result: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        let name = relative.join(entry.file_name());
        if path.is_dir() {
            asset_hashes(&path, &name, result)?;
        } else {
            result.insert(name.display().to_string(), sha(&fs::read(path)?));
        }
    }
    Ok(())
}
fn inner(options: &Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    let mut envs = launcher_environment(&options.launcher)?;
    for (name, value) in [
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
        envs.insert(name.into(), value.into());
    }
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        envs.remove(name);
    }
    let runtime = PathBuf::from(envs.get("XDG_RUNTIME_DIR").ok_or("Runtime missing")?);
    envs.insert(
        "PULSE_SERVER".into(),
        format!("unix:{}/no-audio", runtime.display()),
    );
    envs.insert("PIPEWIRE_RUNTIME_DIR".into(), runtime.display().to_string());
    envs.insert("PIPEWIRE_REMOTE".into(), "no-pipewire".into());
    let docs = Path::new(envs.get("HOME").ok_or("HOME missing")?).join("Documents");
    fs::create_dir(&docs)?;
    fs::write(docs.join("lower-one.txt"), "one")?;
    fs::write(docs.join("lower-two.txt"), "two")?;
    let assets_source = Path::new(
        envs.get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Desktop assets missing")?,
    );
    let assets = options.output.join("assets");
    copy_tree(assets_source, &assets)?;
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
    let qml = options.output.join("shell.qml");
    let escaped = serde_json::to_string(&docs.display().to_string())?;
    fs::write(
        &qml,
        include_str!("../fixture.qml").replace("__DOCS__", &escaped),
    )?;
    let fixture = Fixture {
        envs,
        qml,
        docs,
        out: options.output.clone(),
    };
    let mut children = Children(Vec::new());
    let mut checks = Vec::new();
    let file_jobs = fixture
        .envs
        .get("LUMEN_FILE_JOBS_BIN")
        .ok_or("File jobs binary missing")?;
    children.launch(
        file_jobs,
        &["serve"],
        &fixture.envs,
        &fixture.out.join("file-jobs.log"),
        false,
    )?;
    wait("file operations socket", || {
        Ok(runtime
            .join("lumen-file-operations.sock")
            .exists()
            .then_some(()))
    })?;
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
    let wayland = wait("Wayland display", || first_wayland(&runtime))?;
    let mut fixture = fixture;
    fixture
        .envs
        .insert("SWAYSOCK".into(), socket.display().to_string());
    fixture.envs.insert(
        "WAYLAND_DISPLAY".into(),
        wayland
            .file_name()
            .ok_or("Wayland name missing")?
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
        Ok((fixture.state()?["ready"] == true).then_some(()))
    })?;
    let native = wait("Files window", || fixture.window())?;
    call(
        "swaymsg",
        &[&format!(
            "[con_id={}] focus",
            native["id"].as_u64().ok_or("Native ID missing")?
        )],
        &fixture.envs,
    )?;
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
    fixture.open_batch()?;
    fixture.field("batch-rename-first", "lower", &mut children, pointer)?;
    fixture.field("batch-rename-second", "LOWER", &mut children, pointer)?;
    let before = names(&fixture.docs)?;
    fixture.preview(&mut children, pointer)?;
    require(
        fixture.state()?["canConfirm"] == true && names(&fixture.docs)? == before,
        "Preview changed files or disabled confirmation",
    )?;
    call(
        "grim",
        &[fixture
            .out
            .join("preview.png")
            .to_str()
            .ok_or("Preview path invalid")?],
        &fixture.envs,
    )?;
    checks.push("Actual Ctrl+A/F2 opens captured sibling preview and writes no files");
    let started = Instant::now();
    fixture.key(&["-k", "Tab", "-k", "Return"])?;
    wait("rename confirmation", || {
        Ok((fixture.state()?["reply"]["action"] == "rename-confirm").then_some(()))
    })?;
    let latency = started.elapsed().as_secs_f64();
    require(latency <= 5.0, "Confirmation exceeded five seconds")?;
    require(
        fs::read_to_string(fixture.docs.join("LOWER-one.txt"))? == "one"
            && fs::read_to_string(fixture.docs.join("LOWER-two.txt"))? == "two",
        "Case-only rename failed",
    )?;
    require(
        fixture.state()?["reply"]["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| row["status"] == "renamed")),
        "Per-item rename results missing",
    )?;
    checks.push(
        "Actual keyboard confirmation commits case-only changes and exposes per-item results",
    );
    fixture.click("batch-rename-cancel", &mut children, pointer)?;
    wait("dialog closed", || {
        Ok((fixture.state()?["open"] == false).then_some(()))
    })?;
    wait("Files ready", || {
        Ok((fixture.state()?["ready"] == true).then_some(()))
    })?;
    fixture.open_batch()?;
    fixture.field("batch-rename-first", "LOWER", &mut children, pointer)?;
    fixture.field("batch-rename-second", "UPPER", &mut children, pointer)?;
    fixture.preview(&mut children, pointer)?;
    fixture.key(&["-k", "Escape"])?;
    wait("Escape close", || {
        Ok((fixture.state()?["open"] == false).then_some(()))
    })?;
    require(
        !fixture.docs.join("UPPER-one.txt").exists(),
        "Escape committed rename",
    )?;
    checks.push("Escape cancels a prepared preview without any rename");
    fixture.open_batch()?;
    fixture.field("batch-rename-first", "LOWER", &mut children, pointer)?;
    fixture.field("batch-rename-second", "UPPER", &mut children, pointer)?;
    fixture.preview(&mut children, pointer)?;
    fs::write(fixture.docs.join("replacement"), "replacement")?;
    fs::rename(
        fixture.docs.join("replacement"),
        fixture.docs.join("LOWER-two.txt"),
    )?;
    fixture.click("batch-rename-confirm", &mut children, pointer)?;
    wait("replaced source rejection", || {
        Ok((fixture.state()?["reply"]["action"] == "rename-confirm").then_some(()))
    })?;
    require(
        fixture.state()?["reply"]["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty())
            && !fixture.docs.join("UPPER-one.txt").exists()
            && fs::read_to_string(fixture.docs.join("LOWER-two.txt"))? == "replacement",
        "Source replacement did not reject confirmation",
    )?;
    checks.push("Source replacement after actual preview rejects the entire confirmation");
    fixture.click("batch-rename-cancel", &mut children, pointer)?;
    wait("dialog closed", || {
        Ok((fixture.state()?["open"] == false).then_some(()))
    })?;
    fixture.open_batch()?;
    fixture.field("batch-rename-first", "LOWER", &mut children, pointer)?;
    fixture.field("batch-rename-second", "UPPER", &mut children, pointer)?;
    fixture.preview(&mut children, pointer)?;
    fs::write(fixture.docs.join("UPPER-two.txt"), "bystander")?;
    fixture.click("batch-rename-confirm", &mut children, pointer)?;
    wait("destination collision rejection", || {
        Ok((fixture.state()?["reply"]["action"] == "rename-confirm").then_some(()))
    })?;
    require(
        fixture.state()?["reply"]["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty())
            && !fixture.docs.join("UPPER-one.txt").exists()
            && fs::read_to_string(fixture.docs.join("UPPER-two.txt"))? == "bystander",
        "Destination collision partially renamed files",
    )?;
    checks.push("Destination created after actual preview cannot be replaced or partially rename earlier rows");
    let log = fs::read_to_string(fixture.out.join("shell.log"))?;
    for marker in [
        "ReferenceError:",
        "TypeError:",
        "Binding loop",
        "Failed to load configuration",
        "Cannot assign",
        "Accessible attached property must",
    ] {
        require(!log.contains(marker), "QML emitted a runtime error")?;
    }
    let revision = call("git", &["rev-parse", "HEAD"], &fixture.envs)?;
    let mut hashes = BTreeMap::new();
    asset_hashes(&assets, Path::new(""), &mut hashes)?;
    let source_hash =
        sha(concat!(include_str!("main.rs"), include_str!("../fixture.qml")).as_bytes());
    fs::write(
        fixture.out.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"passed":true,"checks":checks,"source":revision,"launcher":options.launcher,"sway":options.sway,"effects":options.effects,"confirmationLatencySeconds":latency,"fixtureSha256":source_hash,"assetsSha256":hashes})
            )?
        ),
    )?;
    println!("PASS native Files batch rename preview, confirmation and atomic rejection");
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
        eprintln!("batch rename: {error}");
        std::process::exit(1);
    }
}
