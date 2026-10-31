//! Installed shortcut registry, physical dispatch, and painted hint acceptance.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
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
    launcher: PathBuf,
    fixtures: PathBuf,
    sway: PathBuf,
    output: PathBuf,
    effects: bool,
    fontconfig: Option<PathBuf>,
    software_driver: Option<PathBuf>,
    source_revision: Option<String>,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut fixtures = None;
    let mut sway = None;
    let mut output = None;
    let mut effects = false;
    let mut fontconfig = None;
    let mut software_driver = None;
    let mut source_revision = None;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--fixtures") => {
                fixtures = Some(PathBuf::from(args.next().ok_or("Missing fixtures")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--effects") => effects = true,
            Some("--source-revision") => {
                let value = args.next().ok_or("Missing source revision")?;
                let value = value.to_str().ok_or("Source revision must be text")?;
                require(
                    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()),
                    "Exact source revision required",
                )?;
                source_revision = Some(value.to_owned());
            }
            Some("--font-config") => {
                fontconfig = Some(PathBuf::from(
                    args.next().ok_or("Missing font configuration")?,
                ))
            }
            Some("--software-driver") => {
                software_driver = Some(PathBuf::from(args.next().ok_or("Missing software driver")?))
            }
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown shortcut hints option")),
        }
    }
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        fixtures: fixtures.ok_or("--fixtures required")?,
        sway: sway.ok_or("--sway required")?,
        output: output.ok_or("--output required")?,
        effects,
        fontconfig,
        software_driver,
        source_revision,
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
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        values.remove(name);
    }
    values.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/no-system-bus", root.display()),
    );
    values
}
fn outer(options: &Options) -> Result<()> {
    let private = tempfile::tempdir()?;
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
        .arg("90s")
        .arg("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--launcher")
        .arg(&options.launcher)
        .arg("--fixtures")
        .arg(&options.fixtures)
        .arg("--sway")
        .arg(&options.sway)
        .arg("--output")
        .arg(&options.output)
        .env_clear()
        .envs(private_environment(private.path()));
    if options.effects {
        command.arg("--effects");
    }
    if let Some(path) = &options.fontconfig {
        command.arg("--font-config").arg(path);
    }
    if let Some(path) = &options.software_driver {
        command.arg("--software-driver").arg(path);
    }
    if let Some(revision) = &options.source_revision {
        command.arg("--source-revision").arg(revision);
    }
    let status = command.status()?;
    require(
        status.success(),
        &format!("Private shortcut fixture exited {status}"),
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
fn launcher_environment(
    launcher: &Path,
    envs: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher)
        .arg("--print-env0")
        .env_clear()
        .envs(envs)
        .output()?;
    require(output.status.success(), "Shell launcher environment failed")?;
    let mut values = BTreeMap::new();
    for row in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|row| !row.is_empty())
    {
        let text = std::str::from_utf8(row)?;
        if let Some((key, value)) = text.split_once('=') {
            values.insert(key.to_owned(), value.to_owned());
        }
    }
    Ok(values)
}
struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        program: &str,
        args: &[&str],
        envs: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<usize> {
        let stream = File::create(log)?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(envs)
            .stdout(Stdio::from(stream.try_clone()?))
            .stderr(Stdio::from(stream))
            .process_group(0)
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
fn wait<T>(description: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Timed out: {description}: {last}")));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn second_session_runtime(previous: &Path) -> Result<PathBuf> {
    require(previous.is_absolute(), "Private runtime must be absolute")?;
    // Artifact directories can be relative or exceed sockaddr_un's limit.
    // Reuse the already working short private runtime's lifetime instead.
    let path = previous.join("s2");
    fs::create_dir(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
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
fn install(name: &str, options: &Options, registry: &Path, config: &Path) -> Result<Value> {
    let bundle: Value =
        serde_json::from_slice(&fs::read(options.fixtures.join(format!("{name}.json")))?)?;
    let source = Path::new(
        bundle["registry"]
            .as_str()
            .ok_or("Shortcut bundle lost registry")?,
    );
    let temporary = registry.with_extension("new");
    fs::write(&temporary, fs::read(source)?)?;
    fs::rename(&temporary, registry)?;
    let editor = registry.with_file_name("shortcut-editor.json");
    let next = editor.with_extension("new");
    fs::write(
        &next,
        fs::read(bundle["editor"].as_str().ok_or("Editor catalog")?)?,
    )?;
    fs::rename(next, editor)?;
    let chord = if name == "default" {
        "Mod4+Control+f"
    } else {
        "Mod4+Control+x"
    };
    let dispatch = bundle["dispatch"][chord]
        .as_str()
        .ok_or("Shortcut dispatch missing")?;
    require(
        dispatch.starts_with("exec /nix/store/")
            && dispatch.ends_with("/bin/swaymsg fullscreen toggle"),
        "Shortcut dispatch escaped pinned store",
    )?;
    fs::write(
        config,
        format!(
            "output HEADLESS-1 resolution 1280x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\nbindsym {chord} {dispatch}\nexec_always --no-startup-id {}\n",
            bundle["runtimeCommand"]
                .as_str()
                .ok_or("Module runtime witness command missing")?
        ),
    )?;
    Ok(bundle)
}
fn runtime_witness(bundle: &Value, envs: &BTreeMap<String, String>) -> Result<Value> {
    let command = bundle["runtimeCommand"]
        .as_str()
        .ok_or("Runtime command")?
        .split_whitespace()
        .next()
        .ok_or("Runtime command")?;
    require(
        command.starts_with("/nix/store/")
            && command.ends_with("/bin/lumen-shortcut-runtime-witness"),
        "Unexpected runtime observer",
    )?;
    let output = Command::new(command)
        .args(["--check", bundle["registry"].as_str().ok_or("Registry")?])
        .env_clear()
        .envs(envs)
        .output()?;
    Ok(
        json!({"confirmed":output.status.success(),"result":serde_json::from_slice::<Value>(&output.stdout).unwrap_or(Value::Null),"diagnostic":String::from_utf8_lossy(&output.stderr)}),
    )
}
fn tree_nodes<'a>(node: &'a Value, found: &mut Vec<&'a Value>) {
    found.push(node);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                tree_nodes(child, found);
            }
        }
    }
}
fn window(envs: &BTreeMap<String, String>) -> Result<Option<Value>> {
    let tree: Value = serde_json::from_str(&call("swaymsg", &["-r", "-t", "get_tree"], envs)?)?;
    let mut nodes = Vec::new();
    tree_nodes(&tree, &mut nodes);
    Ok(nodes
        .into_iter()
        .find(|node| node["app_id"] == "lumen-shortcut-fixture")
        .cloned())
}
fn audit(shell: u32, command: &str, envs: &BTreeMap<String, String>) -> Result<String> {
    call(
        "quickshell",
        &[
            "ipc",
            "--pid",
            &shell.to_string(),
            "call",
            "shortcutAudit",
            command,
        ],
        envs,
    )
}
fn state(shell: u32, envs: &BTreeMap<String, String>) -> Result<Value> {
    Ok(serde_json::from_str(&audit(shell, "status", envs)?)?)
}
fn key(key: &str, envs: &BTreeMap<String, String>) -> Result<()> {
    call(
        "wtype",
        &[
            "-M", "logo", "-M", "ctrl", "-k", key, "-m", "ctrl", "-m", "logo",
        ],
        envs,
    )?;
    Ok(())
}
fn focus(id: u64, shell: u32, envs: &BTreeMap<String, String>) -> Result<()> {
    call("swaymsg", &[&format!("[con_id={id}] focus")], envs)?;
    wait("owned native focus", || {
        Ok((state(shell, envs)?["active"] == "Shortcut Fixture").then_some(()))
    })
}
fn inner(options: &Options) -> Result<()> {
    fs::create_dir(&options.output)?;
    let mut envs = launcher_environment(&options.launcher, &env::vars().collect())?;
    for (name, value) in [
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
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
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
    ] {
        envs.insert(name.into(), value.into());
    }
    for key in ["WAYLAND_DISPLAY", "SWAYSOCK", "DISPLAY"] {
        envs.remove(key);
    }
    if let Some(font) = &options.fontconfig {
        require(
            font.is_file() && font.starts_with("/nix/store"),
            "Immutable font configuration required",
        )?;
        envs.insert("FONTCONFIG_FILE".into(), font.display().to_string());
    }
    if let Some(driver) = &options.software_driver {
        let driver = fs::canonicalize(driver)?;
        require(
            driver.starts_with("/nix/store")
                && driver.join("lib/dri").is_dir()
                && driver
                    .join("share/glvnd/egl_vendor.d/50_mesa.json")
                    .is_file(),
            "Immutable Mesa package directory required",
        )?;
        envs.insert(
            "LIBGL_DRIVERS_PATH".into(),
            driver.join("lib/dri").display().to_string(),
        );
        envs.insert(
            "__EGL_VENDOR_LIBRARY_FILENAMES".into(),
            driver
                .join("share/glvnd/egl_vendor.d/50_mesa.json")
                .display()
                .to_string(),
        );
        envs.insert("MESA_LOADER_DRIVER_OVERRIDE".into(), "llvmpipe".into());
        envs.remove("LIBGL_ALWAYS_SOFTWARE");
    }
    let assets_source = Path::new(
        envs.get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Desktop asset path missing")?,
    );
    let assets = options.output.join("assets");
    copy_tree(assets_source, &assets)?;
    let source = assets.join("shell");
    let isolated = call(
        "lumen-fixture-providers",
        &[
            "isolate",
            source.to_str().ok_or("Invalid source")?,
            options.output.to_str().ok_or("Invalid output")?,
        ],
        &envs,
    )?;
    let _: Value = serde_json::from_str(&isolated)?;
    let shell_qml = source.join("shell.qml");
    let original = fs::read_to_string(&shell_qml)?;
    let (head, tail) = original
        .rsplit_once('}')
        .ok_or("Shell root missing closing brace")?;
    fs::set_permissions(&shell_qml, fs::Permissions::from_mode(0o600))?;
    fs::write(
        &shell_qml,
        format!("{head}{}\n}}{tail}", include_str!("../shortcut-audit.qml")),
    )?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = Path::new(envs.get("XDG_STATE_HOME").ok_or("State home missing")?)
        .join("lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("Appearance parent missing")?)?;
    fs::write(
        &appearance,
        json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}).to_string(),
    )?;
    let config = options.output.join("sway.conf");
    let registry = Path::new(envs.get("XDG_CONFIG_HOME").ok_or("Config home missing")?)
        .join("lumen-desktop/shortcuts.json");
    fs::create_dir_all(registry.parent().ok_or("Registry parent missing")?)?;
    let before = install("default", options, &registry, &config)?;
    let mut children = Children(Vec::new());
    children.launch(
        options.sway.to_str().ok_or("Invalid Sway path")?,
        &[
            "--unsupported-gpu",
            "-d",
            "-c",
            config.to_str().ok_or("Invalid config")?,
        ],
        &envs,
        &options.output.join("sway.log"),
    )?;
    let runtime = Path::new(envs.get("XDG_RUNTIME_DIR").ok_or("Runtime missing")?);
    let socket = wait("private compositor socket", || {
        first_match(runtime, "sway-ipc")
    })?;
    let wayland = wait("private Wayland display", || first_wayland(runtime))?;
    envs.insert("SWAYSOCK".into(), socket.display().to_string());
    envs.insert(
        "WAYLAND_DISPLAY".into(),
        wayland
            .file_name()
            .ok_or("Wayland name missing")?
            .to_string_lossy()
            .into_owned(),
    );
    children.launch(
        "wtype",
        &["-k", "Shift_L", "-s", "60000"],
        &envs,
        &options.output.join("keyboard-seat.log"),
    )?;
    let shell = children.launch(
        "quickshell",
        &["-p", source.to_str().ok_or("Invalid source")?],
        &envs,
        &options.output.join("shell.log"),
    )?;
    let shell_pid = children.0[shell].id();
    wait("effective registry readiness", || {
        let value = state(shell_pid, &envs)?;
        Ok((value["ready"] == true && value["available"] == true).then_some(()))
    })?;
    let client = children.launch(
        "foot",
        &[
            "--app-id=lumen-shortcut-fixture",
            "--title=Shortcut Fixture",
            "sleep",
            "600",
        ],
        &envs,
        &options.output.join("client.log"),
    )?;
    let target = wait("native fixture window", || window(&envs))?;
    let id = target["id"].as_u64().ok_or("Window ID missing")?;
    let pid = target["pid"].as_u64().ok_or("Window PID missing")?;
    require(
        pid == children.0[client].id() as u64,
        "Shortcut target PID changed",
    )?;
    let mut checks = Vec::new();
    focus(id, shell_pid, &envs)?;
    wait("owned current keyboard and effective default hint", || {
        let value = state(shell_pid, &envs)?;
        fs::write(
            options.output.join("default-hint-observation.json"),
            serde_json::to_vec_pretty(&value)?,
        )?;
        Ok((value["layout"]["available"] == true && value["hint"] == "Ctrl+Super+F").then_some(()))
    })?;
    require(
        state(shell_pid, &envs)?["hint"] == "Ctrl+Super+F",
        "Default hint changed",
    )?;
    key("f", &envs)?;
    wait("default exact dispatch", || {
        Ok(window(&envs)?
            .and_then(|row| row["fullscreen_mode"].as_i64())
            .is_some_and(|mode| mode > 0)
            .then_some(()))
    })?;
    key("f", &envs)?;
    wait("default reverse toggle", || {
        Ok(window(&envs)?
            .and_then(|row| row["fullscreen_mode"].as_i64())
            .is_some_and(|mode| mode == 0)
            .then_some(()))
    })?;
    checks.push("Actual module default registry hint and physical configured dispatch agree");
    let default_runtime = wait("Actual default configuration load witness", || {
        let value = runtime_witness(&before, &envs)?;
        Ok((value["confirmed"] == true).then_some(value))
    })?;
    let old_configuration = fs::read_to_string(&config)?;
    let after = install("rebound", options, &registry, &config)?;
    let valid_rebound_configuration = fs::read_to_string(&config)?;
    let exported_only = runtime_witness(&after, &envs)?;
    require(
        exported_only["confirmed"] == false,
        "Published exports without reload claimed active dispatch",
    )?;
    fs::write(
        options.output.join("export-only.json"),
        serde_json::to_vec_pretty(&exported_only)?,
    )?;
    fs::write(
        &config,
        format!(
            "{old_configuration}\ninvalid_shortcut_fixture_command\nexec_always --no-startup-id {}\n",
            after["runtimeCommand"].as_str().unwrap()
        ),
    )?;
    call("swaymsg", &["reload"], &envs)?;
    let invalid = wait("Actual compositor configuration error guard", || {
        let value: Value =
            serde_json::from_str(&call("swaymsg", &["-t", "get_config", "-r"], &envs)?)?;
        Ok((value["lumen_config_valid"] == false).then_some(value))
    })?;
    require(
        runtime_witness(&after, &envs)?["confirmed"] == false
            && runtime_witness(&before, &envs)?["confirmed"] == false,
        "Invalid reload received a runtime acknowledgement",
    )?;
    focus(id, shell_pid, &envs)?;
    key("f", &envs)?;
    wait("Old dispatch retained in invalid-reload fixture", || {
        Ok(window(&envs)?
            .and_then(|w| w["fullscreen_mode"].as_i64())
            .is_some_and(|mode| mode > 0)
            .then_some(()))
    })?;
    key("f", &envs)?;
    wait("Old dispatch reset after negative", || {
        Ok(window(&envs)?
            .is_some_and(|w| w["fullscreen_mode"] == 0)
            .then_some(()))
    })?;
    fs::write(
        options.output.join("invalid-reload.json"),
        serde_json::to_vec_pretty(
            &json!({"loaded":invalid,"oldRuntime":default_runtime,"oldBindingRetained":true,"scope":"Old binding deliberately retained in invalid input; no atomic reload rollback claim"}),
        )?,
    )?;
    fs::write(&config, valid_rebound_configuration)?;
    call("swaymsg", &["reload"], &envs)?;
    let rebound_runtime = wait("Actual rebound configuration load witness", || {
        let value = runtime_witness(&after, &envs)?;
        Ok((value["confirmed"] == true).then_some(value))
    })?;
    fs::write(
        options.output.join("runtime-reload.json"),
        serde_json::to_vec_pretty(&rebound_runtime)?,
    )?;
    checks.push("Export-only and parse-error configurations remain unconfirmed; actual valid reload publishes matching runtime generation");
    wait("consumer rebind watched", || {
        Ok((state(shell_pid, &envs)?["hint"] == "Ctrl+Super+X").then_some(()))
    })?;
    focus(id, shell_pid, &envs)?;
    key("f", &envs)?;
    thread::sleep(Duration::from_millis(250));
    require(
        window(&envs)?.ok_or("Window lost")?["fullscreen_mode"] == 0,
        "Old binding still active",
    )?;
    key("x", &envs)?;
    wait("new physical binding dispatch", || {
        Ok(window(&envs)?
            .and_then(|row| row["fullscreen_mode"].as_i64())
            .is_some_and(|mode| mode > 0)
            .then_some(()))
    })?;
    key("x", &envs)?;
    wait("new binding reverse toggle", || {
        Ok(window(&envs)?
            .and_then(|row| row["fullscreen_mode"].as_i64())
            .is_some_and(|mode| mode == 0)
            .then_some(()))
    })?;
    require(
        before["dispatch"]["Mod4+Control+f"] == after["dispatch"]["Mod4+Control+x"],
        "Rebound command changed target",
    )?;
    checks.push("Consumer moved exact dispatch command; old key is inert and new key toggles exact native window");
    focus(id, shell_pid, &envs)?;
    audit(shell_pid, "open", &envs)?;
    wait("actual owned menu painted effective hint", || {
        let row = state(shell_pid, &envs)?;
        Ok((row["menu"] == "Window"
            && row["painted"].as_array().is_some_and(|texts| {
                texts.iter().any(|text| {
                    text.as_str()
                        .is_some_and(|text| text.contains("Ctrl+Super+X"))
                })
            }))
        .then_some(()))
    })?;
    call(
        "grim",
        &[options
            .output
            .join("menu-hint.png")
            .to_str()
            .ok_or("Invalid image output")?],
        &envs,
    )?;
    call("wtype", &["-k", "Escape"], &envs)?;
    wait("menu dismissed", || {
        Ok((state(shell_pid, &envs)?["menu"] == "").then_some(()))
    })?;
    checks
        .push("Actual owned Window menu renders the rebound hint and Escape restores normal input");
    fs::write(&registry, "{bad")?;
    wait("invalid metadata clears hints", || {
        let row = state(shell_pid, &envs)?;
        Ok((row["available"] == false && row["hint"] == "").then_some(()))
    })?;
    let final_target = window(&envs)?.ok_or("Target vanished")?;
    require(
        final_target["id"] == id && final_target["pid"] == pid,
        "Invalid registry retargeted a window",
    )?;
    checks.push("Invalid registry cannot retain a stale hint or retarget a native window");
    let restored = install("rebound", options, &registry, &config)?;
    let prior = wait("Runtime witness before session replacement", || {
        let value = runtime_witness(&restored, &envs)?;
        Ok((value["confirmed"] == true).then_some(value))
    })?;
    let previous_runtime = PathBuf::from(envs.get("XDG_RUNTIME_DIR").ok_or("Runtime")?);
    let old_record = fs::read(previous_runtime.join("lumen-desktop/shortcuts-runtime.json"))?;
    drop(children);
    let second_runtime = second_session_runtime(&previous_runtime)?;
    envs.insert(
        "XDG_RUNTIME_DIR".into(),
        second_runtime.display().to_string(),
    );
    envs.remove("SWAYSOCK");
    envs.remove("WAYLAND_DISPLAY");
    let mut second = Children(Vec::new());
    second.launch(
        options.sway.to_str().ok_or("Sway")?,
        &[
            "--unsupported-gpu",
            "-d",
            "-c",
            config.to_str().ok_or("Config")?,
        ],
        &envs,
        &options.output.join("sway-session2.log"),
    )?;
    let second_socket = wait("Second actual compositor socket", || {
        first_match(&second_runtime, "sway-ipc")
    })?;
    envs.insert("SWAYSOCK".into(), second_socket.display().to_string());
    let current = wait("Second actual compositor load witness", || {
        let value = runtime_witness(&restored, &envs)?;
        Ok((value["confirmed"] == true).then_some(value))
    })?;
    require(
        prior["result"]["epoch"]["pid"] != current["result"]["epoch"]["pid"],
        "Compositor session was not replaced",
    )?;
    fs::write(
        second_runtime.join("lumen-desktop/shortcuts-runtime.json"),
        old_record,
    )?;
    let stale = runtime_witness(&restored, &envs)?;
    require(
        stale["confirmed"] == false
            && stale["diagnostic"]
                .as_str()
                .is_some_and(|text| text.contains("another session")),
        "Old-session runtime record survived new manager epoch",
    )?;
    fs::write(
        options.output.join("session-epoch.json"),
        serde_json::to_vec_pretty(
            &json!({"previous":prior,"current":current,"carriedOldRecord":stale,
                "previousRuntime":previous_runtime,"secondRuntime":second_runtime}),
        )?,
    )?;
    let publisher = restored["runtimeCommand"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    call(publisher, &[restored["registry"].as_str().unwrap()], &envs)?;
    require(
        runtime_witness(&restored, &envs)?["confirmed"] == true,
        "Current session witness could not recover",
    )?;
    drop(second);
    checks.push("A real second compositor rejects the previous session record by observed socket/process epoch");
    if options.effects && options.software_driver.is_some() {
        let log = fs::read_to_string(options.output.join("sway.log"))?
            + &fs::read_to_string(options.output.join("sway-session2.log"))?;
        let observed = log
            .lines()
            .filter(|line| {
                let text = line.to_lowercase();
                text.contains("renderer")
                    && (text.contains("llvmpipe") || text.contains("softpipe"))
            })
            .collect::<Vec<_>>();
        require(
            !observed.is_empty(),
            "Effects renderer was not observed as software",
        )?;
        fs::write(
            options.output.join("renderer.json"),
            serde_json::to_vec_pretty(
                &json!({"software":true,"driver":options.software_driver,"observedRendererLines":observed}),
            )?,
        )?;
    }
    let source_revision = if let Some(revision) = &options.source_revision {
        revision.clone()
    } else {
        call("git", &["rev-parse", "HEAD"], &envs)?
    };
    fs::write(
        options.output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"passed":true,"checks":checks,"target":[id,pid],"source":source_revision,"fixture":options.fixtures})
            )?
        ),
    )?;
    println!("PASS shortcut hints, physical dispatch, rebind and owned menu");
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
        eprintln!("shortcut hints: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_runtime_is_absolute_private_and_accepts_real_unix_sockets() {
        let root = tempfile::Builder::new()
            .prefix("lsh-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in("/tmp")
            .unwrap();
        let second = second_session_runtime(root.path()).unwrap();
        assert!(second.is_absolute());
        assert_eq!(second.parent(), Some(root.path()));
        assert_eq!(
            fs::metadata(&second).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let _wayland = std::os::unix::net::UnixListener::bind(second.join("wayland-0")).unwrap();
        let _ipc =
            std::os::unix::net::UnixListener::bind(second.join("sway-ipc.1000.1234567.sock"))
                .unwrap();
        assert!(second_session_runtime(Path::new("relative-artifacts")).is_err());
    }
}
