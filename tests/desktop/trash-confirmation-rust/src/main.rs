//! Native Files Trash confirmation, partial outcomes and retained restart evidence.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut output = None;
    let mut launcher = None;
    let mut sway = None;
    let mut pointer = None;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
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
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown Trash fixture option")),
        }
    }
    Ok(Options {
        output: output.ok_or("--output required")?,
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        pointer: pointer.ok_or("--pointer-tool required")?,
        effects,
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
        .arg("120s")
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
    let status = command.status()?;
    require(
        status.success(),
        &format!("Private Trash fixture exited {status}"),
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
    fn stop(&mut self, index: usize, seconds: u64) -> Result<()> {
        if self.0[index].try_wait()?.is_none() {
            unsafe { libc::killpg(self.0[index].id() as i32, libc::SIGTERM) };
        }
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            if self.0[index].try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(fail("Child did not stop"));
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
                "Private Trash acceptance timed out ({label}): {last}"
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
fn quote_path(path: &Path) -> String {
    let mut encoded = String::new();
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}
fn trash_item(trash: &Path, home: &Path, name: &str) -> Result<PathBuf> {
    let file = trash.join("files").join(name);
    fs::write(&file, format!("disposable {name}"))?;
    let info = trash.join("info").join(format!("{name}.trashinfo"));
    fs::write(
        &info,
        format!("[Trash Info]\nPath={}\n", quote_path(&home.join(name))),
    )?;
    fs::set_permissions(info, fs::Permissions::from_mode(0o600))?;
    Ok(file)
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
}
impl Fixture {
    fn ipc(&self, method: &str, values: &[&str]) -> Result<String> {
        let mut args = vec![
            "-p",
            self.qml.to_str().ok_or("QML path invalid")?,
            "ipc",
            "call",
            "trashAudit",
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
            .find(|row| {
                row["app_id"] == "org.quickshell"
                    && row["name"]
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
        let geometry: Value = serde_json::from_str(&self.ipc("geometry", &[name])?)?;
        require(!geometry.is_null(), "Trash control geometry missing")?;
        let window = self.window()?.ok_or("Files window missing")?;
        let rect = &window["rect"];
        let coordinate = |axis: &str, dimension: &str| -> Result<String> {
            let origin = rect[axis].as_f64().ok_or("Native coordinate missing")?;
            let offset = geometry[axis]
                .as_f64()
                .ok_or("Control coordinate missing")?;
            let size = geometry[dimension]
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
        for command in ["m 0.5 0.5", "p", "r"] {
            let stream = children.0[pointer]
                .stdin
                .as_mut()
                .ok_or("Pointer input missing")?;
            stream.write_all(command.as_bytes())?;
            stream.write_all(b"\n")?;
            stream.flush()?;
            thread::sleep(Duration::from_millis(120));
        }
        Ok(())
    }
    fn prepare(&self, mode: &str, path: &Path) -> Result<()> {
        self.ipc(
            "prepare",
            &[mode, path.to_str().ok_or("Trash path invalid")?],
        )?;
        Ok(())
    }
}
fn last_operation(state: &Value) -> Option<&Value> {
    state["operations"].as_array()?.last()
}
fn sha(path: &Path) -> Result<String> {
    Ok(hex::encode(Sha256::digest(fs::read(path)?)))
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
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("LUMEN_FILE_SERVICE", "1"),
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
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/inert-system", runtime.display()),
    );
    envs.insert(
        "PULSE_SERVER".into(),
        format!("unix:{}/inert-audio", runtime.display()),
    );
    if options.effects {
        for (name, value) in [
            ("WLR_RENDERER", "gles2"),
            ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
            ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ] {
            envs.insert(name.into(), value.into());
        }
    }
    let repo = env::current_dir()?;
    let assets = options.output.join("assets");
    copy_tree(&repo.join("res/lumen-desktop"), &assets)?;
    let trash = Path::new(envs.get("XDG_DATA_HOME").ok_or("Data home missing")?).join("Trash");
    for folder in [&trash, &trash.join("files"), &trash.join("info")] {
        fs::create_dir_all(folder)?;
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700))?;
    }
    let home = PathBuf::from(envs.get("HOME").ok_or("HOME missing")?);
    let first = trash_item(&trash, &home, "Literal # é.txt")?;
    let second = trash_item(&trash, &home, "Second.txt")?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = options.output.join("appearance.json");
    fs::write(
        &appearance,
        json!({"font":palette["font"],"palette":palette["dark"],"preferences":{"textScale":1.5}})
            .to_string(),
    )?;
    envs.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let qml = options.output.join("shell.qml");
    fs::write(&qml, include_str!("../fixture.qml"))?;
    let mut fixture = Fixture {
        envs,
        qml,
        out: options.output.clone(),
    };
    let mut children = Children(Vec::new());
    let mut checks = Vec::new();
    let config = fixture.out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1000x760\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n",
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
    let file_jobs = fixture
        .envs
        .get("LUMEN_FILE_JOBS_BIN")
        .ok_or("File jobs binary missing")?;
    children.launch(
        file_jobs,
        &["serve"],
        &fixture.envs,
        &fixture.out.join("operations.log"),
        false,
    )?;
    wait("file operations socket", || {
        Ok(runtime
            .join("lumen-file-operations.sock")
            .exists()
            .then_some(()))
    })?;
    let mut shell = children.launch(
        "quickshell",
        &["-p", fixture.qml.to_str().ok_or("QML path invalid")?],
        &fixture.envs,
        &fixture.out.join("shell.log"),
        false,
    )?;
    wait("Trash rows and controller", || {
        let state = fixture.state()?;
        Ok((state["ready"] == true
            && state["controller"] == true
            && state["rows"].as_array().is_some_and(|rows| rows.len() == 2))
        .then_some(()))
    })?;
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
    fixture.prepare("selected", &first)?;
    wait("selected confirmation", || {
        Ok((!fixture.state()?["confirmation"].is_null()).then_some(()))
    })?;
    wait("Cancel default focus", || {
        Ok((fixture.state()?["focus"] == "trash-cancel").then_some(()))
    })?;
    fixture.key(&["-k", "Return"])?;
    wait("Return dismissed", || {
        Ok(fixture.state()?["confirmation"].is_null().then_some(()))
    })?;
    require(first.exists(), "Default Return deleted item")?;
    checks.push("Actual Return activates default Cancel without deleting captured item");
    fixture.prepare("selected", &first)?;
    wait("selected confirmation", || {
        Ok((!fixture.state()?["confirmation"].is_null()).then_some(()))
    })?;
    fixture.key(&["-k", "Escape"])?;
    wait("Escape dismissed", || {
        Ok(fixture.state()?["confirmation"].is_null().then_some(()))
    })?;
    require(first.exists(), "Escape deleted item")?;
    checks.push("Actual Escape retires pending confirmation without deletion");
    fixture.prepare("empty", Path::new(""))?;
    wait("empty Trash confirmation", || {
        Ok((!fixture.state()?["confirmation"].is_null()).then_some(()))
    })?;
    let late = trash_item(&trash, &home, "Later.txt")?;
    call(
        "grim",
        &[fixture
            .out
            .join("confirmation-150-percent.png")
            .to_str()
            .ok_or("Image path invalid")?],
        &fixture.envs,
    )?;
    fixture.click("trash-confirm", &mut children, pointer)?;
    wait("captured empty completed", || {
        let state = fixture.state()?;
        Ok((state["busy"] == false
            && last_operation(&state).is_some_and(|operation| operation["state"] == "complete"))
        .then_some(()))
    })?;
    require(
        !first.exists() && !second.exists() && late.exists(),
        "Empty Trash removed a late arrival or kept captured items",
    )?;
    checks.push(
        "Actual pointer confirms captured Empty Trash; later arrival excluded at enlarged text",
    );
    fixture.ipc("closeResults", &[])?;
    fixture.ipc("refresh", &[])?;
    wait("remaining Trash row", || {
        let state = fixture.state()?;
        Ok(
            (state["ready"] == true
                && state["rows"].as_array().is_some_and(|rows| rows.len() == 1))
            .then_some(()),
        )
    })?;
    fixture.prepare("selected", &late)?;
    wait("late item confirmation", || {
        Ok((!fixture.state()?["confirmation"].is_null()).then_some(()))
    })?;
    let retired = late.with_file_name("retired.txt");
    fs::rename(&late, &retired)?;
    fs::write(&late, "replacement")?;
    fixture.click("trash-confirm", &mut children, pointer)?;
    wait("partial operation", || {
        let state = fixture.state()?;
        Ok((state["busy"] == false
            && last_operation(&state).is_some_and(|operation| operation["state"] == "partial"))
        .then_some(()))
    })?;
    require(
        fs::read_to_string(&late)? == "replacement" && retired.exists(),
        "Replaced Trash path was deleted",
    )?;
    checks.push("Replaced selected path produces truthful kept/partial outcome without deleting replacement");
    let before =
        last_operation(&fixture.state()?).ok_or("Retained operation missing")?["id"].clone();
    children.stop(shell, 5)?;
    shell = children.launch(
        "quickshell",
        &["-p", fixture.qml.to_str().ok_or("QML path invalid")?],
        &fixture.envs,
        &fixture.out.join("shell.log"),
        false,
    )?;
    require(children.0[shell].id() > 0, "Shell restart failed")?;
    wait("retained result after restart", || {
        let state = fixture.state()?;
        Ok((state["ready"] == true
            && state["controller"] == true
            && last_operation(&state).is_some())
        .then_some(()))
    })?;
    require(
        last_operation(&fixture.state()?).ok_or("Retained operation missing")?["id"] == before
            && fs::read_to_string(&late)? == "replacement",
        "Restart replayed destructive confirmation",
    )?;
    checks.push(
        "Private shell restart reads retained outcome and never replays destructive confirmation",
    );
    let log = fs::read_to_string(fixture.out.join("shell.log"))?;
    for marker in [
        "ReferenceError:",
        "TypeError:",
        "Binding loop",
        "Failed to load configuration",
        "Cannot assign",
        "Unable to assign",
    ] {
        require(!log.contains(marker), "QML emitted a runtime error")?;
    }
    let mut source = BTreeMap::new();
    for relative in [
        "res/lumen-desktop/shell/FilesWindow.qml",
        "res/lumen-desktop/file-jobs/src/lib.rs",
        "res/lumen-desktop/file-jobs/src/trash.rs",
    ] {
        source.insert(relative.to_owned(), sha(&repo.join(relative))?);
    }
    source.insert(
        "tests/desktop/trash-confirmation-rust/src/main.rs".into(),
        hex::encode(Sha256::digest(include_bytes!("main.rs"))),
    );
    fs::write(
        fixture.out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"checks":checks,"final":fixture.state()?,"launcher":options.launcher,"sway":options.sway,"effects":options.effects,"source":source,"isolation":"Disposable HOME/XDG/personal Trash, activation-free private session bus, inert system/audio, only owned Files fragment and private operation service"}),
        )?,
    )?;
    println!("PASS native Trash confirmation, partial outcome and restart");
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
        eprintln!("trash confirmation: {error}");
        std::process::exit(1);
    }
}
