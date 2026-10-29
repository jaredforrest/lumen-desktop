//! Native workspace and output placement, in a disposable compositor and bus.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn check(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn path(value: &Path) -> String {
    value.to_string_lossy().into_owned()
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn unique_dir() -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = PathBuf::from(format!(
        "/tmp/lsp-{}-{}",
        std::process::id(),
        stamp % 1_000_000_000
    ));
    fs::create_dir(&root)?;
    Ok(root)
}
fn output(command: &str, args: &[&str], envs: &BTreeMap<String, String>) -> Result<String> {
    let result = Command::new(command)
        .args(args)
        .env_clear()
        .envs(envs)
        .output()?;
    check(
        result.status.success(),
        format!(
            "{command} {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}
fn json_call(command: &str, args: &[&str], envs: &BTreeMap<String, String>) -> Result<Value> {
    let response = output(command, args, envs)?;
    if response.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&response)
        .map_err(|error| fail(format!("{command} {args:?}: {error}: {response}")))
}
fn wait<T>(name: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Timed out waiting for {name}: {last}")));
        }
        pause(50);
    }
}
fn launch(
    command: &str,
    args: &[&str],
    envs: &BTreeMap<String, String>,
    log: &Path,
) -> Result<Child> {
    let file = File::create(log)?;
    let stderr = file.try_clone()?;
    let mut child = Command::new(command);
    child
        .args(args)
        .env_clear()
        .envs(envs)
        .stdout(file)
        .stderr(stderr);
    unsafe {
        child.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    Ok(child.spawn()?)
}
struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        command: &str,
        args: &[&str],
        envs: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<usize> {
        self.0.push(launch(command, args, envs, log)?);
        Ok(self.0.len() - 1)
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                unsafe {
                    libc::killpg(child.id() as i32, libc::SIGTERM);
                }
            }
        }
        for child in self.0.iter_mut().rev() {
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().ok().flatten().is_none() {
                if Instant::now() >= deadline {
                    unsafe {
                        libc::killpg(child.id() as i32, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    break;
                }
                pause(25);
            }
        }
    }
}
fn first(root: &Path, prefix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name().is_some_and(|name| {
                name.to_string_lossy().starts_with(prefix)
                    && !name.to_string_lossy().ends_with(".lock")
            })
        }))
}
fn launcher_env(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let result = Command::new(launcher).arg("--print-env0").output()?;
    check(result.status.success(), "Launcher environment unavailable")?;
    let mut vars = BTreeMap::new();
    for bytes in result.stdout.split(|b| *b == 0) {
        if let Some((key, value)) = String::from_utf8_lossy(bytes).split_once('=') {
            vars.insert(key.to_owned(), value.to_owned());
        }
    }
    Ok(vars)
}
fn nodes<'a>(node: &'a Value, result: &mut Vec<&'a Value>) {
    result.push(node);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                nodes(child, result);
            }
        }
    }
}
fn sway(args: &[&str], vars: &BTreeMap<String, String>) -> Result<Value> {
    let mut all = vec!["-r"];
    all.extend_from_slice(args);
    json_call("swaymsg", &all, vars)
}
fn windows<'a>(tree: &'a Value, app: &str) -> Vec<&'a Value> {
    let mut all = Vec::new();
    nodes(tree, &mut all);
    all.into_iter().filter(|n| n["app_id"] == app).collect()
}
fn spaces(state: &Path, args: &[&str], vars: &BTreeMap<String, String>) -> Result<Value> {
    let state = path(state);
    let mut all = vec!["--state", state.as_str()];
    all.extend_from_slice(args);
    json_call("lumen-spaces", &all, vars)
}
fn desktop_id(status: &Value, name: &str) -> Result<String> {
    status["desktops"]
        .as_object()
        .ok_or("Missing desktops")?
        .iter()
        .find(|(_, value)| value["name"] == name)
        .map(|(id, _)| id.clone())
        .ok_or_else(|| fail(format!("Missing desktop {name}")))
}
fn order(status: &Value) -> Result<Vec<String>> {
    Ok(status["order"]
        .as_array()
        .ok_or("Missing order")?
        .iter()
        .take(3)
        .map(|id| {
            status["desktops"][id.as_str().unwrap_or("")]["name"]
                .as_str()
                .unwrap_or("")
                .to_owned()
        })
        .collect())
}
fn placed(app: &str, target: &str, count: usize, vars: &BTreeMap<String, String>) -> Result<bool> {
    let workspaces = sway(&["-t", "get_workspaces"], vars)?;
    let id = workspaces
        .as_array()
        .ok_or("Missing workspaces")?
        .iter()
        .find(|w| w["name"] == target)
        .map(|w| w["id"].clone());
    let Some(id) = id else { return Ok(false) };
    let tree = sway(&["-t", "get_tree"], vars)?;
    let mut all = Vec::new();
    nodes(&tree, &mut all);
    let Some(workspace) = all.into_iter().find(|n| n["id"] == id) else {
        return Ok(false);
    };
    Ok(windows(workspace, app).len() == count)
}
fn inside(output_dir: &Path, launcher: &Path, compositor: &Path) -> Result<()> {
    fs::create_dir(output_dir)?;
    let mut vars = launcher_env(launcher)?;
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", "pixman"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("NO_AT_BRIDGE", "1"),
        ("QT_ACCESSIBILITY", "0"),
        ("GTK_A11Y", "none"),
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
    let runtime = PathBuf::from(
        vars.get("XDG_RUNTIME_DIR")
            .ok_or("Missing private runtime")?,
    );
    let state = output_dir.join("policy.json");
    let mut children = Children(Vec::new());
    fs::write(
        output_dir.join("sway.conf"),
        "output HEADLESS-1 resolution 1280x720\nseat seat0 fallback true\n",
    )?;
    let config = path(&output_dir.join("sway.conf"));
    children.launch(
        &path(compositor),
        &["--unsupported-gpu", "-c", &config],
        &vars,
        &output_dir.join("sway.log"),
    )?;
    vars.insert(
        "SWAYSOCK".into(),
        path(&wait("compositor socket", || first(&runtime, "sway-ipc."))?),
    );
    vars.insert(
        "WAYLAND_DISPLAY".into(),
        wait("Wayland display", || {
            Ok(first(&runtime, "wayland-")?
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())))
        })?,
    );
    sway(&["workspace \"1\""], &vars)?;
    children.launch(
        "foot",
        &["--app-id=spaces-first", "sleep", "120"],
        &vars,
        &output_dir.join("first.log"),
    )?;
    wait("first window", || {
        Ok((!windows(&sway(&["-t", "get_tree"], &vars)?, "spaces-first").is_empty()).then_some(()))
    })?;
    sway(&["workspace \"2\""], &vars)?;
    children.launch(
        "foot",
        &["--app-id=spaces-anchor", "sleep", "120"],
        &vars,
        &output_dir.join("anchor.log"),
    )?;
    wait("anchor window", || {
        Ok(
            (!windows(&sway(&["-t", "get_tree"], &vars)?, "spaces-anchor").is_empty())
                .then_some(()),
        )
    })?;
    sway(&["workspace \"1\""], &vars)?;
    let initial = spaces(&state, &["status"], &vars)?;
    let target = desktop_id(&initial, "2")?;
    spaces(
        &state,
        &["assign", "wayland:spaces-editor", "desktop", &target],
        &vars,
    )?;
    let state_text = path(&state);
    let watcher = children.launch(
        "lumen-spaces",
        &["--state", &state_text, "watch"],
        &vars,
        &output_dir.join("watch.log"),
    )?;
    pause(300);
    children.launch(
        "foot",
        &["--app-id=spaces-editor", "sleep", "120"],
        &vars,
        &output_dir.join("editor.log"),
    )?;
    wait("first assigned editor", || {
        Ok(placed("spaces-editor", "2", 1, &vars)?.then_some(()))
    })?;
    children.0[watcher].kill()?;
    children.0[watcher].wait()?;
    children.launch(
        "lumen-spaces",
        &["--state", &state_text, "watch"],
        &vars,
        &output_dir.join("watch-restarted.log"),
    )?;
    pause(300);
    children.launch(
        "foot",
        &["--app-id=spaces-editor", "sleep", "120"],
        &vars,
        &output_dir.join("editor-second.log"),
    )?;
    wait("second assigned editor", || {
        Ok(placed("spaces-editor", "2", 2, &vars)?.then_some(()))
    })?;
    sway(&["workspace \"3\""], &vars)?;
    children.launch(
        "foot",
        &["--app-id=spaces-third", "sleep", "120"],
        &vars,
        &output_dir.join("third.log"),
    )?;
    wait("third window", || {
        Ok((!windows(&sway(&["-t", "get_tree"], &vars)?, "spaces-third").is_empty()).then_some(()))
    })?;
    for name in ["1", "2", "3"] {
        sway(&[&format!("workspace {name:?}")], &vars)?;
        pause(200);
    }
    let before = spaces(&state, &["status"], &vars)?;
    let identities: BTreeMap<String, String> = ["1", "2", "3"]
        .iter()
        .map(|name| Ok((name.to_string(), desktop_id(&before, name)?)))
        .collect::<Result<_>>()?;
    check(
        order(&before)? == ["3", "2", "1"],
        "Recent desktop order changed",
    )?;
    spaces(&state, &["ordering", "false"], &vars)?;
    let ordered = spaces(&state, &["status"], &vars)?;
    check(
        order(&ordered)? == ["1", "2", "3"],
        "Fixed desktop order changed",
    )?;
    check(
        ["1", "2", "3"]
            .iter()
            .all(|name| desktop_id(&ordered, name).ok().as_ref() == identities.get(*name)),
        "Desktop identity changed",
    )?;
    sway(&["create_output"], &vars)?;
    wait("second output", || {
        Ok(sway(&["-t", "get_outputs"], &vars)?
            .as_array()
            .and_then(|outputs| outputs.iter().find(|o| o["name"] == "HEADLESS-2").cloned()))
    })?;
    sway(
        &["workspace \"4\"; move workspace to output \"HEADLESS-2\""],
        &vars,
    )?;
    let status = spaces(&state, &["status"], &vars)?;
    let display_id = status["outputs"]
        .as_object()
        .ok_or("Missing outputs")?
        .iter()
        .find(|(_, value)| *value == "HEADLESS-2")
        .map(|(key, _)| key.clone())
        .ok_or("Missing second output identity")?;
    spaces(
        &state,
        &["assign", "wayland:spaces-display", "display", &display_id],
        &vars,
    )?;
    sway(&["workspace \"1\""], &vars)?;
    children.launch(
        "foot",
        &["--app-id=spaces-display", "sleep", "120"],
        &vars,
        &output_dir.join("display.log"),
    )?;
    wait("assigned output", || {
        let tree = sway(&["-t", "get_tree"], &vars)?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok(all
            .into_iter()
            .find(|node| node["type"] == "output" && node["name"] == "HEADLESS-2")
            .and_then(|output| (!windows(output, "spaces-display").is_empty()).then_some(())))
    })?;
    sway(&["output \"HEADLESS-2\" disable"], &vars)?;
    pause(300);
    let disconnected = spaces(&state, &["status"], &vars)?;
    check(
        disconnected["outputs"].get(&display_id).is_none(),
        "Disconnected display still present",
    )?;
    check(
        disconnected["assignments"]["wayland:spaces-display"]["target_id"] == display_id,
        "Display assignment was lost",
    )?;
    sway(&["output \"HEADLESS-2\" enable"], &vars)?;
    wait("reconnected output", || {
        Ok(sway(&["-t", "get_outputs"], &vars)?
            .as_array()
            .and_then(|outputs| {
                outputs
                    .iter()
                    .find(|o| o["name"] == "HEADLESS-2" && o["active"] == true)
                    .cloned()
            }))
    })?;
    check(
        spaces(&state, &["status"], &vars)?["outputs"]
            .get(&display_id)
            .is_some(),
        "Display identity did not recover",
    )?;
    fs::write(
        output_dir.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"passed":true,"newWindowsPlaced":2,"watcherRestartPersistence":true,"displayAssignment":true,"displayRemovalRetained":true,"displayReconnectRecovered":true,"recentOrderSeparateFromIdentity":true})
            )?
        ),
    )?;
    Ok(())
}
fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let mut output_dir = None;
    let mut launcher = None;
    let mut compositor = PathBuf::from("sway");
    let mut inside_flag = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--output") => {
                output_dir = Some(PathBuf::from(args.next().ok_or("Missing output")?))
            }
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => compositor = PathBuf::from(args.next().ok_or("Missing compositor")?),
            Some("--inside") => inside_flag = true,
            _ => return Err(fail(format!("Unknown argument: {}", arg.to_string_lossy()))),
        }
    }
    let output_dir = output_dir.ok_or("Missing --output")?;
    let output_dir = if output_dir.is_absolute() {
        output_dir
    } else {
        env::current_dir()?.join(output_dir)
    };
    let launcher = launcher.ok_or("Missing --launcher")?.canonicalize()?;
    let compositor = compositor.canonicalize()?;
    if inside_flag {
        return inside(&output_dir, &launcher, &compositor);
    }
    let private = unique_dir()?;
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    vars.insert("HOME".into(), path(&private));
    vars.insert("XDG_RUNTIME_DIR".into(), path(&private));
    vars.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/absent-system", private.display()),
    );
    for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
        vars.insert(
            format!("XDG_{key}_HOME"),
            path(&private.join(key.to_lowercase())),
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
    let current = env::current_exe()?;
    let status = Command::new("dbus-run-session")
        .arg("--")
        .arg(current)
        .args([
            "--output",
            &path(&output_dir),
            "--launcher",
            &path(&launcher),
            "--sway",
            &path(&compositor),
            "--inside",
        ])
        .env_clear()
        .envs(&vars)
        .status();
    let cleanup = fs::remove_dir_all(&private);
    cleanup?;
    check(status?.success(), "Native spaces placement failed")
}
