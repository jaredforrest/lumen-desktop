//! Real Files main/utility ownership in a disposable native Qt session.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn save_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    output: PathBuf,
    full_shell: bool,
    effects: bool,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut output = None;
    let mut full_shell = false;
    let mut effects = false;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing sway")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--full-shell") => full_shell = true,
            Some("--effects") => effects = true,
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown owned-window-context fixture option")),
        }
    }
    let output = output.ok_or("--output required")?;
    let output = if output.is_absolute() {
        output
    } else {
        env::current_dir()?.join(output)
    };
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        output,
        full_shell,
        effects,
        inside,
    })
}
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = env::temp_dir().join(format!("lown-{}-{nonce}", std::process::id()));
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
fn private_session() -> Result<()> {
    let root = PrivateRoot::new()?;
    let config = root.0.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            root.0.display()
        ),
    )?;
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    vars.insert("HOME".into(), root.0.display().to_string());
    vars.insert("XDG_RUNTIME_DIR".into(), root.0.display().to_string());
    vars.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}", root.0.join("no-system-bus").display()),
    );
    for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
        vars.insert(
            format!("XDG_{key}_HOME"),
            root.0.join(key.to_lowercase()).display().to_string(),
        );
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
    let status = Command::new("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .args(env::args_os().skip(1))
        .arg("--inside")
        .env_clear()
        .envs(vars)
        .status()?;
    require(
        status.success(),
        format!("Private owned-window fixture exited {status}"),
    )
}
fn launcher_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher).arg("--print-env0").output()?;
    require(result.status.success(), "Launcher environment unavailable")?;
    let mut vars = BTreeMap::new();
    for part in result.stdout.split(|byte| *byte == 0) {
        if let Some((key, value)) = std::str::from_utf8(part)?.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    Ok(vars)
}
fn resolve(program: &str, vars: &BTreeMap<String, String>) -> Result<PathBuf> {
    let path = vars.get("PATH").ok_or("Packaged PATH missing")?;
    for dir in env::split_paths(&OsString::from(path)) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("Missing program {program}")))
}
fn command(program: &Path, args: &[&str], vars: &BTreeMap<String, String>) -> Result<String> {
    let result = Command::new(program)
        .args(args)
        .env_clear()
        .envs(vars)
        .output()?;
    require(
        result.status.success(),
        format!(
            "{} {:?} exited {}: {}",
            program.display(),
            args,
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}
struct Managed(Child);
impl Managed {
    fn launch(
        program: &Path,
        args: &[&str],
        vars: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<Self> {
        let file = File::create(log)?;
        Ok(Self(
            Command::new(program)
                .args(args)
                .env_clear()
                .envs(vars)
                .stdin(Stdio::null())
                .stdout(Stdio::from(file.try_clone()?))
                .stderr(Stdio::from(file))
                .process_group(0)
                .spawn()?,
        ))
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
            let end = Instant::now() + Duration::from_secs(3);
            while self.0.try_wait().ok().flatten().is_none() {
                if Instant::now() > end {
                    unsafe {
                        libc::killpg(self.0.id() as i32, libc::SIGKILL);
                    }
                    let _ = self.0.wait();
                    break;
                }
                pause(30);
            }
        }
    }
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() > end {
            return Err(fail(format!("Timed out waiting for {label}: {last}")));
        }
        pause(60);
    }
}
fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|item| item.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && (suffix.is_empty() || !name.ends_with(suffix))
            })
        }))
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let kind = fs::symlink_metadata(&from)?.file_type();
        if kind.is_symlink() {
            symlink(fs::read_link(&from)?, &to)?;
        } else if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to)?;
            let mut p = fs::metadata(&to)?.permissions();
            p.set_mode(p.mode() | 0o200);
            fs::set_permissions(&to, p)?;
        } else {
            return Err(fail(format!("Unsupported asset {}", from.display())));
        }
    }
    Ok(())
}
fn qml_value(path: &Path) -> Result<String> {
    Ok(serde_json::to_string(&path.display().to_string())?)
}
fn ipc(qml: &Path, method: &str, vars: &BTreeMap<String, String>) -> Result<String> {
    let quickshell = resolve("quickshell", vars)?;
    command(
        &quickshell,
        &[
            "-p",
            &qml.display().to_string(),
            "ipc",
            "call",
            "ownershipAudit",
            method,
        ],
        vars,
    )
}
fn state(qml: &Path, out: &Path, vars: &BTreeMap<String, String>) -> Result<Value> {
    let value: Value = serde_json::from_str(&ipc(qml, "state", vars)?)?;
    save_json(&out.join("last-state.json"), &value)?;
    Ok(value)
}
fn ready(value: &Value, key: &str) -> bool {
    value[key].as_bool() == Some(true)
}
fn property(value: &Value, key: &str, expected: &str) -> bool {
    value[key].as_str() == Some(expected)
}
fn no_main(value: &Value) -> bool {
    value["main"].as_str() == Some("")
}
fn run(options: Options) -> Result<()> {
    require(!options.output.exists(), "Fresh output directory required")?;
    fs::create_dir_all(&options.output)?;
    let out = &options.output;
    let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
    let mut vars = launcher_environment(&options.launcher)?;
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
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
        ("LUMEN_NATIVE_WINDOW_ACTIONS", "0"),
    ] {
        vars.insert(key.into(), value.into());
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        vars.remove(key);
    }
    let dbus_send = resolve("dbus-send", &vars)?;
    let activatable = command(
        &dbus_send,
        &[
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.ListActivatableNames",
        ],
        &vars,
    )?;
    require(
        !activatable.contains("org.freedesktop.portal"),
        "Private bus unexpectedly activates portals",
    )?;
    let folder = PathBuf::from(vars.get("HOME").ok_or("Private HOME missing")?).join("First");
    fs::create_dir(&folder)?;
    let document = folder.join("selected.txt");
    fs::write(&document, "Synthetic document for main/utility ownership")?;
    let applications = PathBuf::from(
        vars.get("XDG_DATA_HOME")
            .ok_or("Private data home missing")?,
    )
    .join("applications");
    fs::create_dir_all(&applications)?;
    fs::write(
        applications.join("lumen-files.desktop"),
        "[Desktop Entry]\nType=Application\nName=Files\nExec=true\nIcon=folder\n",
    )?;
    let assets_source = PathBuf::from(
        vars.get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Packaged assets missing")?,
    );
    let assets = out.join("assets");
    copy_tree(&assets_source, &assets)?;
    let share = env::current_exe()?
        .parent()
        .ok_or("Fixture bin missing")?
        .parent()
        .ok_or("Fixture prefix missing")?
        .join("share/lumen-owned-window-context-fixture");
    let mut qml = out.join("shell.qml");
    let folder_value = qml_value(&folder)?;
    let document_value = qml_value(&document)?;
    let standalone = fs::read_to_string(share.join("standalone.qml"))?
        .replace("@FOLDER@", &folder_value)
        .replace("@DOCUMENT@", &document_value);
    fs::write(&qml, &standalone)?;
    if options.full_shell {
        let provider = resolve("lumen-fixture-providers", &vars)?;
        let isolate = command(
            &provider,
            &[
                "isolate",
                &assets.join("shell").display().to_string(),
                &out.display().to_string(),
            ],
            &vars,
        )?;
        let _: Value = serde_json::from_str(&isolate)?;
        qml = assets.join("shell/shell.qml");
        let original = fs::read_to_string(&qml)?;
        let head = original
            .rsplit_once('}')
            .ok_or("Full shell root missing")?
            .0;
        let fragment = fs::read_to_string(share.join("full-shell.fragment.qml"))?
            .replace("@FOLDER@", &folder_value)
            .replace("@DOCUMENT@", &document_value);
        fs::write(&qml, format!("{head}{fragment}}}"))?;
    }
    let config = out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1280x900\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n",
    )?;
    let _sway = Managed::launch(
        &options.sway,
        &["--unsupported-gpu", "-c", &config.display().to_string()],
        &vars,
        &out.join("sway.log"),
    )?;
    let socket = wait("private compositor", || {
        first_file(&runtime, "sway-ipc.", "")
    })?;
    vars.insert("SWAYSOCK".into(), socket.display().to_string());
    let display = wait("private display", || {
        first_file(&runtime, "wayland-", ".lock")
    })?;
    vars.insert(
        "WAYLAND_DISPLAY".into(),
        display
            .file_name()
            .ok_or("Display name missing")?
            .to_string_lossy()
            .into(),
    );
    let quickshell = resolve("quickshell", &vars)?;
    let _shell = Managed::launch(
        &quickshell,
        &["-p", &qml.display().to_string()],
        &vars,
        &out.join("shell.log"),
    )?;
    wait("Files ready", || {
        let v = state(&qml, out, &vars)?;
        Ok(ready(&v, "ready").then_some(()))
    })?;
    let check = |method: &str| ipc(&qml, method, &vars).map(|_| ());
    let wait_state = |label: &str, pred: &dyn Fn(&Value) -> bool| -> Result<Value> {
        wait(label, || {
            let v = state(&qml, out, &vars)?;
            Ok(pred(&v).then_some(v))
        })
    };
    check("firstFocus")?;
    let first = wait_state("first focus", &|v| property(v, "key", "first"))?;
    require(
        property(&first, "main", "first"),
        "First main association missing",
    )?;
    check("previewFocus")?;
    let preview = wait_state("preview focus", &|v| {
        ready(v, "previewActive") && property(v, "key", "preview")
    })?;
    require(
        property(&preview, "main", "first") && property(&preview, "role", "utility"),
        "Preview lost first main association",
    )?;
    if options.full_shell {
        wait_state("Files group", &|v| property(v, "group", "lumen-files"))?;
    }
    check("commandNewTab")?;
    let tab = wait_state("new tab", &|v| v["firstTabs"].as_i64() == Some(2))?;
    require(
        tab["secondTabs"].as_i64() == Some(1),
        "New tab went to second window",
    )?;
    check("infoFocus")?;
    let info = wait_state("inspector focus", &|v| {
        ready(v, "infoActive") && property(v, "key", "info")
    })?;
    require(
        property(&info, "main", "first") && property(&info, "menuTarget", "first"),
        "Inspector lost first owner",
    )?;
    check("otherFocus")?;
    let other = wait_state("unrelated focus", &|v| {
        ready(v, "otherActive") && property(v, "key", "")
    })?;
    require(
        property(&other, "main", "first"),
        "Unrelated focus discarded main",
    )?;
    if options.full_shell {
        let group = other["filesGroupTitles"]
            .as_array()
            .ok_or("No Files group titles")?;
        require(
            group.iter().any(|v| v == "Quick Look") && group.iter().any(|v| v == "File Info"),
            "Utilities missing from Files group",
        )?;
    }
    check("secondFocus")?;
    let second = wait_state("second focus", &|v| property(v, "key", "second"))?;
    require(
        property(&second, "main", "second"),
        "Second main association missing",
    )?;
    check("secondInfoFocus")?;
    let second_info = wait_state("second inspector", &|v| property(v, "key", "secondInfo"))?;
    require(
        property(&second_info, "main", "second") && property(&second_info, "menuTarget", "second"),
        "Second inspector lost owner",
    )?;
    check("previewFocus")?;
    let returned = wait_state("returned preview", &|v| property(v, "key", "preview"))?;
    require(
        property(&returned, "main", "first") && property(&returned, "menuTarget", "first"),
        "Returned preview lost first owner",
    )?;
    check("closeFirst")?;
    let closed = wait_state("closed first", &no_main)?;
    require(
        property(&closed, "app", ""),
        "Closed main retained application",
    )?;
    let result = json!({"passed":true,"checks":[
        "Owned Files main document reference stays independent of Quick Look and inspector key focus",
        "Document New Tab command from key utility targets associated Files instance",
        "Another application clears owned key recipient without discarding main association",
        "Second Files document and returning utility choose explicit owning instance",
        "Closing main invalidates association"],"final":closed});
    save_json(&out.join("result.json"), &result)?;
    println!("PASS owned main/utility context");
    Ok(())
}
fn main() -> Result<()> {
    let options = options()?;
    if options.inside {
        run(options)
    } else {
        private_session()
    }
}
