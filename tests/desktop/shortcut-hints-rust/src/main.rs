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
    inside: bool,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut fixtures = None;
    let mut sway = None;
    let mut output = None;
    let mut effects = false;
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
            "output HEADLESS-1 resolution 1280x900\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\nbindsym {chord} {dispatch}\n"
        ),
    )?;
    Ok(bundle)
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
        &["-s", "60000"],
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
    let after = install("rebound", options, &registry, &config)?;
    call("swaymsg", &["reload"], &envs)?;
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
    let source_revision = call("git", &["rev-parse", "HEAD"], &envs)?;
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
