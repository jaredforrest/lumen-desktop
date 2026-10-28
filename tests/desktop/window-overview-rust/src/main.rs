//! Captured overview identity, delayed backend replies, and retired target authority.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn integer(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| fail(format!("Missing {key} in {value}")))
}
fn immutable(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    require(
        canonical.starts_with("/nix/store") && canonical.is_file(),
        "Pinned executable required",
    )?;
    Ok(path.to_path_buf())
}
fn program(name: &str, values: &BTreeMap<String, String>) -> Result<PathBuf> {
    for folder in env::split_paths(&OsString::from(
        values.get("PATH").ok_or("Packaged PATH missing")?,
    )) {
        let path = folder.join(name);
        if path.is_file() {
            return immutable(&path);
        }
    }
    Err(fail(format!("{name} unavailable in packaged PATH")))
}
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher).arg("--print-env0").output()?;
    require(
        result.status.success(),
        "Packaged shell environment unavailable",
    )?;
    let mut values = BTreeMap::new();
    for part in result
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
    {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            values.insert(key.to_owned(), value.to_owned());
        }
    }
    require(values.contains_key("PATH"), "Packaged PATH missing")?;
    Ok(values)
}
struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    output: PathBuf,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut output = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            _ => return Err(fail("Unknown overview fixture option")),
        }
    }
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        output: output.ok_or("--output required")?,
    })
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = Path::new("/tmp").join(format!("lov-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        Ok(Self(path))
    }
}
impl Drop for PrivateRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
                "Overview lifecycle timed out waiting for {label}: {last}"
            )));
        }
        thread::sleep(Duration::from_millis(40));
    }
}
fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && (suffix.is_empty() || !name.ends_with(suffix))
            })
        }))
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else if path.is_file() {
            fs::copy(&path, &target)?;
        } else {
            return Err(fail(format!(
                "Unsupported asset in overview fixture: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn append_json(path: &Path, value: &Value) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", serde_json::to_string(value)?)?;
    Ok(())
}
fn json_lines(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    fs::read_to_string(path)?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}
fn gate() -> Result<i32> {
    let root = PathBuf::from(env::var("LUMEN_OVERVIEW_GATE_ROOT")?);
    let real = immutable(Path::new(&env::var("LUMEN_REAL_OVERVIEW")?))?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    append_json(&root.join("requests.jsonl"), &request)?;
    let mut process = Command::new("timeout")
        .arg("8s")
        .arg(real)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    process
        .stdin
        .take()
        .ok_or("Backend stdin missing")?
        .write_all(input.as_bytes())?;
    let reply = process.wait_with_output()?;
    let response: Value = serde_json::from_slice(&reply.stdout)?;
    append_json(
        &root.join("responses.jsonl"),
        &json!({"request":request,"reply":response}),
    )?;
    if root.join("hold").exists() && request["action"] == "status" {
        fs::write(root.join("held"), "snapshot-ready")?;
        let until = Instant::now() + Duration::from_secs(10);
        while !root.join("release").exists() && Instant::now() < until {
            thread::sleep(Duration::from_millis(20));
        }
        if !root.join("release").exists() {
            return Ok(8);
        }
    }
    std::io::stdout().write_all(&reply.stdout)?;
    std::io::stderr().write_all(&reply.stderr)?;
    Ok(reply.status.code().unwrap_or(1))
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
fn workspace_node(node: &Value, id: i64, current: &str) -> Option<String> {
    let current = if node["type"] == "workspace" {
        node["name"].as_str().unwrap_or(current)
    } else {
        current
    };
    if node["id"].as_i64() == Some(id) {
        return Some(current.to_owned());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            if let Some(found) = workspace_node(child, id, current) {
                return Some(found);
            }
        }
    }
    None
}
struct Fixture {
    output: PathBuf,
    private: PrivateRoot,
    values: BTreeMap<String, String>,
    scene: PathBuf,
    processes: Vec<Managed>,
}
impl Fixture {
    fn new(options: Options) -> Result<Self> {
        let launcher = immutable(&options.launcher)?;
        let sway = immutable(&options.sway)?;
        require(!options.output.exists(), "Fresh output directory required")?;
        fs::create_dir_all(&options.output)?;
        fs::set_permissions(&options.output, fs::Permissions::from_mode(0o700))?;
        let output = fs::canonicalize(options.output)?;
        let private = PrivateRoot::new()?;
        let mut values = launcher_environment(&launcher)?;
        for key in [
            "HOME",
            "XDG_RUNTIME_DIR",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
        ] {
            let path = private.0.join(key.to_lowercase());
            fs::create_dir(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            values.insert(key.to_owned(), path.display().to_string());
        }
        for key in [
            "SWAYSOCK",
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
        ] {
            values.remove(key);
        }
        for (key, value) in [
            ("WLR_BACKENDS", "headless"),
            ("WLR_HEADLESS_OUTPUTS", "1"),
            ("WLR_RENDERER", "pixman"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
            ("QT_ACCESSIBILITY", "0"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
        ] {
            values.insert(key.to_owned(), value.to_owned());
        }
        let bus_config = private.0.join("bus.conf");
        fs::write(
            &bus_config,
            format!(
                "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
                values["XDG_RUNTIME_DIR"]
            ),
        )?;
        let bus_log = File::create(output.join("bus.log"))?;
        let mut bus = Command::new(program("dbus-daemon", &values)?)
            .arg("--nofork")
            .arg("--print-address=1")
            .arg(format!("--config-file={}", bus_config.display()))
            .env_clear()
            .envs(&values)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(bus_log))
            .process_group(0)
            .spawn()?;
        let stdout = bus.stdout.take().ok_or("Bus address pipe missing")?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut address = String::new();
            let result = BufReader::new(stdout)
                .read_line(&mut address)
                .map(|_| address);
            let _ = sender.send(result);
        });
        let address = receiver.recv_timeout(Duration::from_secs(5))??;
        require(
            address.starts_with("unix:"),
            format!(
                "Private bus did not start: {}",
                fs::read_to_string(output.join("bus.log")).unwrap_or_default()
            ),
        )?;
        values.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.trim().into());
        let mut fixture = Self {
            output,
            private,
            values,
            scene: PathBuf::new(),
            processes: vec![Managed(bus)],
        };
        let sway_config = fixture.private.0.join("sway.conf");
        fs::write(
            &sway_config,
            "output * resolution 1200x800\nseat seat0 fallback true\n",
        )?;
        fixture.launch(
            &sway,
            &[
                "--unsupported-gpu",
                "-c",
                sway_config.to_str().ok_or("Sway config invalid")?,
            ],
            "sway.log",
        )?;
        let runtime = PathBuf::from(&fixture.values["XDG_RUNTIME_DIR"]);
        let sock = wait("compositor IPC", || first_file(&runtime, "sway-ipc", ""))?;
        fixture
            .values
            .insert("SWAYSOCK".into(), sock.display().to_string());
        let display = wait("private display", || {
            first_file(&runtime, "wayland-", ".lock")
        })?;
        fixture.values.insert(
            "WAYLAND_DISPLAY".into(),
            display
                .file_name()
                .ok_or("Wayland name missing")?
                .to_string_lossy()
                .into_owned(),
        );
        fixture.launch(
            &program("foot", &fixture.values)?,
            &[
                "-o",
                "resize-by-cells=no",
                "--app-id=overview-source",
                "sleep",
                "180",
            ],
            "source.log",
        )?;
        fixture.launch(
            &program("foot", &fixture.values)?,
            &[
                "-o",
                "resize-by-cells=no",
                "--app-id=overview-bystander",
                "sleep",
                "180",
            ],
            "bystander.log",
        )?;
        wait("two native source windows", || {
            Ok((fixture.windows()?.len() == 2).then_some(()))
        })?;
        let real = program("lumen-window-overview", &fixture.values)?;
        symlink(
            env::current_exe()?,
            fixture.private.0.join("lumen-window-overview"),
        )?;
        fixture
            .values
            .insert("LUMEN_REAL_OVERVIEW".into(), real.display().to_string());
        fixture.values.insert(
            "LUMEN_OVERVIEW_GATE_ROOT".into(),
            fixture.private.0.display().to_string(),
        );
        fixture.values.insert(
            "PATH".into(),
            format!("{}:{}", fixture.private.0.display(), fixture.values["PATH"]),
        );
        copy_tree(
            &env::current_dir()?.join("res/lumen-desktop"),
            &fixture.private.0.join("lumen-desktop"),
        )?;
        let appearance =
            Path::new(&fixture.values["XDG_STATE_HOME"]).join("lumen-desktop/appearance.json");
        fs::create_dir_all(appearance.parent().ok_or("Appearance parent missing")?)?;
        fs::write(
            &appearance,
            serde_json::to_vec(
                &json!({"palette":{"background":"#f2f2f4","surface":"#ffffff","field":"#e5e5e8","text":"#202023","muted":"#6b6b70","border":"#c8c8ce","hover":"#dedee3","accent":"#007aff","selectedText":"#ffffff"},"font":"sans-serif","preferences":{}}),
            )?,
        )?;
        let source = env::current_exe()?
            .parent()
            .ok_or("Executable parent missing")?
            .parent()
            .ok_or("Package root missing")?
            .join("share/lumen-window-overview-fixture/fixture.qml");
        fixture.scene = fixture.private.0.join("shell.qml");
        fs::copy(source, &fixture.scene)?;
        let scene = fixture.scene.display().to_string();
        fixture.launch(
            &program("quickshell", &fixture.values)?,
            &["-p", &scene],
            "overview.log",
        )?;
        wait("initial overview service", || {
            Ok((fixture.status().ok().is_some_and(|s| s["visible"] == false)).then_some(()))
        })?;
        Ok(fixture)
    }
    fn launch(&mut self, program: &Path, args: &[&str], label: &str) -> Result<()> {
        self.processes.push(Managed::launch(
            program,
            args,
            &self.values,
            &self.output.join(label),
        )?);
        Ok(())
    }
    fn call(&self, name: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program("timeout", &self.values)?)
            .arg("5s")
            .arg(program(name, &self.values)?)
            .args(args)
            .env_clear()
            .envs(&self.values)
            .output()?;
        require(
            output.status.success(),
            format!(
                "{name} command failed {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn sway(&self, request: &str) -> Result<()> {
        let reply: Value = serde_json::from_str(&self.call("swaymsg", &["-r", request])?)?;
        require(
            reply
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
            format!("Sway rejected {request}: {reply}"),
        )
    }
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn window(&self, app: &str) -> Result<Option<Value>> {
        Ok(find_node(&self.tree()?, &|node| node["app_id"] == app))
    }
    fn windows(&self) -> Result<Vec<Value>> {
        let mut found = Vec::new();
        for app in ["overview-source", "overview-bystander"] {
            if let Some(row) = self.window(app)? {
                found.push(row);
            }
        }
        Ok(found)
    }
    fn workspace(&self, id: i64) -> Result<Option<String>> {
        Ok(workspace_node(&self.tree()?, id, ""))
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut command = vec![
            "-p",
            self.scene.to_str().ok_or("Scene path invalid")?,
            "ipc",
            "call",
            "overviewTest",
            method,
        ];
        command.extend(args);
        self.call("quickshell", &command)
    }
    fn status(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("status", &[])?)?)
    }
    fn requests(&self) -> Result<Vec<Value>> {
        json_lines(&self.private.0.join("requests.jsonl"))
    }
    fn responses(&self) -> Result<Vec<Value>> {
        json_lines(&self.private.0.join("responses.jsonl"))
    }
    fn touch(&self, name: &str) -> Result<()> {
        fs::write(self.private.0.join(name), "")?;
        Ok(())
    }
    fn remove(&self, name: &str) -> Result<()> {
        fs::remove_file(self.private.0.join(name))?;
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        while let Some(process) = self.processes.pop() {
            drop(process);
        }
    }
}

fn run(options: Options) -> Result<()> {
    let fixture = Fixture::new(options)?;
    let source = fixture
        .window("overview-source")?
        .ok_or("Source window missing")?;
    let bystander = fixture
        .window("overview-bystander")?
        .ok_or("Bystander window missing")?;
    let captured = integer(&source, "id")?;
    let other = integer(&bystander, "id")?;
    let other_workspace = fixture
        .workspace(other)?
        .ok_or("Bystander workspace missing")?;
    fixture.sway(&format!(
        "[con_id={captured}] move container to workspace 2"
    ))?;
    wait("overview initially hidden", || {
        Ok((fixture.status()?["visible"] == false).then_some(()))
    })?;
    fixture.ipc("capture", &[&captured.to_string()])?;
    wait("captured overview target", || {
        let status = fixture.status()?;
        Ok((status["current"] == captured && status["busy"] == false).then_some(()))
    })?;
    let initial = fixture.status()?;
    require(
        initial["moveEnabled"] == true,
        format!("Move action unavailable for captured window: {initial}"),
    )?;

    // The production button targets the captured source, even after focus moves.
    fixture.sway(&format!("[con_id={other}] focus"))?;
    require(
        fixture.ipc("move", &[])?.trim() == "true",
        "Captured Move Window button refused",
    )?;
    wait("captured source moved to desktop 1", || {
        Ok((fixture.workspace(captured)?.as_deref() == Some("1")).then_some(()))
    })?;
    require(
        fixture.workspace(other)?.as_deref() == Some(other_workspace.as_str()),
        "Bystander workspace changed",
    )?;

    // A held real status snapshot must delay focus without losing its ID.
    wait("overview worker idle", || {
        Ok((fixture.status()?["busy"] == false).then_some(()))
    })?;
    fixture.sway(&format!("[con_id={other}] focus"))?;
    fixture.touch("hold")?;
    fixture.ipc("activateAndRefresh", &[])?;
    wait("held real backend status", || {
        Ok(fixture.private.0.join("held").exists().then_some(()))
    })?;
    let delayed = fixture.status()?;
    require(
        delayed["busy"] == true && delayed["visible"] == false,
        format!("Delayed activation state differs: {delayed}"),
    )?;
    thread::sleep(Duration::from_millis(200));
    require(
        !fixture
            .requests()?
            .iter()
            .any(|row| row["action"] == "focus"),
        "Focus escaped before delayed status resolved",
    )?;
    fixture.touch("release")?;
    wait("delayed focus request for captured source", || {
        Ok(fixture
            .responses()?
            .iter()
            .any(|row| {
                row["request"]["action"] == "focus"
                    && row["request"]["id"] == captured
                    && row["reply"]["ok"] == true
            })
            .then_some(()))
    })?;
    require(
        fixture
            .window("overview-source")?
            .ok_or("Source disappeared early")?["focused"]
            == true,
        "Delayed activation focused the wrong window",
    )?;
    wait("overview worker idle after delayed focus", || {
        Ok((fixture.status()?["busy"] == false).then_some(()))
    })?;
    fixture.remove("hold")?;
    fixture.remove("release")?;
    fixture.remove("held")?;
    fixture.ipc("capture", &[&captured.to_string()])?;
    wait("captured overview reopened", || {
        let status = fixture.status()?;
        Ok((status["current"] == captured && status["busy"] == false).then_some(()))
    })?;

    // Retire the captured window while an old backend snapshot is held.
    fixture.touch("hold")?;
    fixture.ipc("refresh", &[])?;
    wait("old backend snapshot held", || {
        Ok(fixture.private.0.join("held").exists().then_some(()))
    })?;
    fixture.ipc("capture", &[&captured.to_string()])?;
    let busy = fixture.status()?;
    require(
        busy["busy"] == true,
        format!("Overview did not become busy: {busy}"),
    )?;
    for key in [
        "moveEnabled",
        "assignEnabled",
        "orderEnabled",
        "outputEnabled",
        "desktopEnabled",
    ] {
        require(
            busy[key] == false,
            format!("Stale snapshot left {key} enabled: {busy}"),
        )?;
    }
    require(
        fixture.ipc("move", &[])?.trim() == "false",
        "Busy overview allowed move",
    )?;
    fixture.ipc("activate", &[])?;
    require(
        fixture.status()?["visible"] == true,
        "Overview did not remain visible during retirement",
    )?;
    fixture.sway(&format!("[con_id={captured}] kill"))?;
    wait("captured source destroyed", || {
        Ok(fixture.window("overview-source")?.is_none().then_some(()))
    })?;
    fixture.touch("release")?;
    wait("old snapshot processed after source destruction", || {
        let status = fixture.status()?;
        Ok((status["current"] == captured && status["busy"] == false).then_some(()))
    })?;
    thread::sleep(Duration::from_millis(150));
    require(
        fixture.ipc("move", &[])?.trim() == "true",
        "Retired target was not sent to backend for explicit refusal",
    )?;
    wait("retired move refused by backend", || {
        Ok(fixture
            .responses()?
            .iter()
            .any(|row| {
                row["request"]["action"] == "move"
                    && row["reply"]["error"] == "This window is no longer available"
            })
            .then_some(()))
    })?;
    require(
        fixture.workspace(other)?.as_deref() == Some(other_workspace.as_str()),
        "Bystander moved after stale action",
    )?;
    let stale = fixture.status()?;
    fixture.remove("hold")?;
    fixture.ipc("refresh", &[])?;
    wait("missing target status", || {
        let status = fixture.status()?;
        Ok((status["current"].is_null()
            && status["error"] == "The dragged window is no longer available")
            .then_some(()))
    })?;
    let absent = fixture.status()?;
    require(
        absent["dragged"] == captured && absent["selected"] == -1 && absent["moveEnabled"] == false,
        format!("Missing target retained action authority: {absent}"),
    )?;
    wait("overview worker idle after missing target", || {
        Ok((fixture.status()?["busy"] == false).then_some(()))
    })?;
    let before = fixture
        .requests()?
        .into_iter()
        .filter(|row| row["action"] == "move" || row["action"] == "focus")
        .collect::<Vec<_>>();
    require(
        fixture.ipc("move", &[])?.trim() == "false",
        "Missing target allowed move",
    )?;
    fixture.ipc("activate", &[])?;
    thread::sleep(Duration::from_millis(200));
    let after = fixture
        .requests()?
        .into_iter()
        .filter(|row| row["action"] == "move" || row["action"] == "focus")
        .collect::<Vec<_>>();
    require(
        after == before,
        "Missing target sent another move or focus request",
    )?;
    require(
        fixture.workspace(other)?.as_deref() == Some(other_workspace.as_str()),
        "Bystander workspace changed after missing target",
    )?;
    fixture.call(
        "grim",
        &[fixture
            .output
            .join("missing-captured-window.png")
            .to_str()
            .ok_or("Screenshot path invalid")?],
    )?;
    let log = fs::read_to_string(fixture.output.join("overview.log"))?;
    for problem in [
        " WARN ",
        " ERROR ",
        "TypeError:",
        "ReferenceError:",
        "Failed to load",
    ] {
        require(
            !log.contains(problem),
            format!("QML log contains {problem}"),
        )?;
    }
    let result = json!({
        "passed": true, "capturedID": captured, "bystanderID": other,
        "initial": initial, "busyDisabled": busy, "delayedActivation": delayed,
        "staleRejected": stale, "absent": absent,
        "requests": fixture.requests()?, "responses": fixture.responses()?,
    });
    fs::write(
        fixture.output.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!(
        "PASS real overview captured identity, delayed stale snapshot and missing-target refusal"
    );
    Ok(())
}

fn main() {
    let invoked = env::args_os().next().unwrap_or_default();
    let is_gate = Path::new(&invoked)
        .file_name()
        .is_some_and(|name| name == "lumen-window-overview");
    if is_gate {
        match gate() {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("Overview backend gate: {error}");
                std::process::exit(1);
            }
        }
    }
    if let Err(error) = options().and_then(run) {
        eprintln!("Overview lifecycle fixture: {error}");
        std::process::exit(1);
    }
}
