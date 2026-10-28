//! Real paired-window menu and chooser acceptance in a private compositor.
use serde_json::{json, Value};
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
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(fail(message))
    }
}

struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    keyboard: PathBuf,
    assets: PathBuf,
    output: PathBuf,
    private: Option<PathBuf>,
}
fn options() -> Result<Options> {
    let mut launcher = None;
    let mut sway = None;
    let mut keyboard = None;
    let mut assets = None;
    let mut output = None;
    let mut private = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--launcher") => {
                launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
            }
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
            Some("--keyboard") => {
                keyboard = Some(PathBuf::from(args.next().ok_or("Missing keyboard")?))
            }
            Some("--assets") => assets = Some(PathBuf::from(args.next().ok_or("Missing assets")?)),
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--private") => {
                private = Some(PathBuf::from(args.next().ok_or("Missing private home")?))
            }
            _ => return Err(fail("Unknown paired picker fixture option")),
        }
    }
    Ok(Options {
        launcher: launcher.ok_or("--launcher required")?,
        sway: sway.ok_or("--sway required")?,
        keyboard: keyboard.unwrap_or_else(|| PathBuf::from("wtype")),
        assets: assets.ok_or("--assets required")?,
        output: output.ok_or("--output required")?,
        private,
    })
}
fn program(name: &str, values: &BTreeMap<String, String>) -> Result<PathBuf> {
    let path = values.get("PATH").ok_or("Pinned PATH is missing")?;
    for folder in env::split_paths(path) {
        let candidate = folder.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("{name} is unavailable in pinned PATH")))
}
fn private_base(root: &Path) -> BTreeMap<String, String> {
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    for key in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        values.remove(key);
    }
    values.insert("HOME".into(), root.display().to_string());
    values.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    for key in ["CONFIG", "STATE", "DATA", "CACHE"] {
        values.insert(
            format!("XDG_{key}_HOME"),
            root.join(key.to_lowercase()).display().to_string(),
        );
    }
    values.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/absent-system", root.display()),
    );
    values
}
fn launcher_environment(
    launcher: &Path,
    base: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher)
        .arg("--print-env0")
        .env_clear()
        .envs(base)
        .output()?;
    require(
        output.status.success(),
        "Installed launcher environment failed",
    )?;
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
fn environment(launcher: &Path, root: &Path) -> Result<BTreeMap<String, String>> {
    let base = private_base(root);
    let mut values = launcher_environment(launcher, &base)?;
    for key in [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "DBUS_SYSTEM_BUS_ADDRESS",
    ] {
        values.insert(key.into(), base[key].clone());
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
        values.insert(key.into(), value.into());
    }
    Ok(values)
}
fn outer(options: &Options) -> Result<()> {
    require(!options.output.exists(), "Fresh output directory required")?;
    fs::create_dir_all(&options.output)?;
    let output = fs::canonicalize(&options.output)?;
    let private = tempfile::Builder::new()
        .prefix("lumen-paired-picker-")
        .tempdir()?;
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o700))?;
    let values = environment(&options.launcher, private.path())?;
    for key in ["config", "state", "data", "cache"] {
        fs::create_dir(private.path().join(key))?;
    }
    let config = private.path().join("bus.conf");
    fs::write(&config, format!("<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>", private.path().display()))?;
    let keyboard = if options.keyboard == Path::new("wtype") {
        program("wtype", &values)?
    } else {
        fs::canonicalize(&options.keyboard)?
    };
    let status = Command::new(program("timeout", &values)?)
        .arg("90s")
        .arg(program("dbus-run-session", &values)?)
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--launcher")
        .arg(fs::canonicalize(&options.launcher)?)
        .arg("--sway")
        .arg(fs::canonicalize(&options.sway)?)
        .arg("--keyboard")
        .arg(keyboard)
        .arg("--assets")
        .arg(fs::canonicalize(&options.assets)?)
        .arg("--output")
        .arg(output)
        .arg("--private")
        .arg(private.path())
        .env_clear()
        .envs(&values)
        .status()?;
    require(
        status.success(),
        format!("Private paired picker exited {status}"),
    )
}
struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        program: &Path,
        args: &[&str],
        values: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<()> {
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
        self.0.push(child);
        Ok(())
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
            let deadline = Instant::now() + Duration::from_secs(3);
            while child.try_wait().ok().flatten().is_none() {
                if Instant::now() >= deadline {
                    unsafe {
                        libc::killpg(child.id() as i32, libc::SIGKILL);
                    }
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
    let mut last_error = None;
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last_error = Some(error.to_string()),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!(
                "Paired picker timed out waiting for {label}: {}",
                last_error.unwrap_or_default()
            )));
        }
        thread::sleep(Duration::from_millis(40));
    }
}
fn first_file(root: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(root)?
        .filter_map(|row| row.ok().map(|row| row.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && !name.ends_with(suffix)
            })
        }))
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for row in fs::read_dir(source)? {
        let row = row?;
        let from = row.path();
        let to = target.join(row.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
fn call(program: &Path, args: &[&str], values: &BTreeMap<String, String>) -> Result<String> {
    let output = Command::new("timeout")
        .arg("5s")
        .arg(program)
        .args(args)
        .env_clear()
        .envs(values)
        .output()?;
    require(
        output.status.success(),
        format!(
            "{} {:?} failed: {}",
            program.display(),
            args,
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn sway(program: &Path, values: &BTreeMap<String, String>, command: &str) -> Result<()> {
    let reply: Value = serde_json::from_str(&call(program, &["-r", command], values)?)?;
    require(
        reply
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
        format!("Sway command rejected: {command}: {reply}"),
    )
}
fn tree(program: &Path, values: &BTreeMap<String, String>) -> Result<Value> {
    Ok(serde_json::from_str(&call(
        program,
        &["-r", "-t", "get_tree"],
        values,
    )?)?)
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
fn node_by_app(
    program: &Path,
    values: &BTreeMap<String, String>,
    name: &str,
) -> Result<Option<Value>> {
    Ok(find_node(&tree(program, values)?, &|node| {
        node["app_id"] == name
    }))
}
fn node_by_id(program: &Path, values: &BTreeMap<String, String>, id: i64) -> Result<Option<Value>> {
    Ok(find_node(&tree(program, values)?, &|node| {
        node["id"].as_i64() == Some(id)
    }))
}
fn id(node: &Value) -> Result<i64> {
    node["id"]
        .as_i64()
        .ok_or_else(|| fail("Native window ID missing"))
}
fn ipc(
    qs: &Path,
    fixture: &Path,
    values: &BTreeMap<String, String>,
    method: &str,
    args: &[&str],
) -> Result<String> {
    let fixture = fixture.to_str().ok_or("Fixture path invalid")?;
    let mut parts = vec!["-p", fixture, "ipc", "call", "pairTest", method];
    parts.extend_from_slice(args);
    call(qs, &parts, values)
}
fn state(qs: &Path, fixture: &Path, values: &BTreeMap<String, String>) -> Result<Value> {
    Ok(serde_json::from_str(&ipc(
        qs,
        fixture,
        values,
        "status",
        &[],
    )?)?)
}
fn route(
    qs: &Path,
    fixture: &Path,
    values: &BTreeMap<String, String>,
    id: i64,
    side: &str,
) -> Result<()> {
    ipc(qs, fixture, values, "route", &[&id.to_string(), side])?;
    Ok(())
}
fn select(qs: &Path, fixture: &Path, values: &BTreeMap<String, String>, id: i64) -> Result<()> {
    ipc(qs, fixture, values, "select", &[&id.to_string()])?;
    Ok(())
}
fn click(qs: &Path, fixture: &Path, values: &BTreeMap<String, String>, label: &str) -> Result<()> {
    let value = ipc(qs, fixture, values, "click", &[label])?;
    require(
        value == "true",
        format!(
            "Picker button {label} unavailable: {}",
            state(qs, fixture, values)?
        ),
    )
}
fn loaded(
    qs: &Path,
    fixture: &Path,
    values: &BTreeMap<String, String>,
    source: i64,
) -> Result<Option<Value>> {
    let value = state(qs, fixture, values)?;
    Ok((value["source"]["id"].as_i64() == Some(source) && value["busy"] == false).then_some(value))
}
fn run(options: &Options) -> Result<()> {
    let private = options.private.as_ref().ok_or("Private runtime required")?;
    let out = &options.output;
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    require(
        values
            .get("DBUS_SESSION_BUS_ADDRESS")
            .is_some_and(|address| address.starts_with("unix:")),
        "Private D-Bus address required",
    )?;
    let swaymsg = program("swaymsg", &values)?;
    let quickshell = program("quickshell", &values)?;
    let foot = program("foot", &values)?;
    let grim = program("grim", &values)?;
    let mut children = Children(Vec::new());
    let config = out.join("sway.conf");
    fs::write(
        &config,
        "output * resolution 1200x800\nseat seat0 fallback true\n",
    )?;
    children.launch(
        &options.sway,
        &[
            "--unsupported-gpu",
            "-c",
            config.to_str().ok_or("Sway config path invalid")?,
        ],
        &values,
        &out.join("sway.log"),
    )?;
    let socket = wait("Sway socket", || first_file(private, "sway-ipc", ".lock"))?;
    values.insert("SWAYSOCK".into(), socket.display().to_string());
    let display = wait("Wayland display", || {
        first_file(private, "wayland-", ".lock")
    })?;
    values.insert(
        "WAYLAND_DISPLAY".into(),
        display
            .file_name()
            .ok_or("Wayland name missing")?
            .to_string_lossy()
            .into_owned(),
    );
    for (name, log) in [
        ("overview-source", "source.log"),
        ("overview-bystander", "bystander.log"),
    ] {
        children.launch(
            &foot,
            &[
                "-o",
                "resize-by-cells=no",
                &format!("--app-id={name}"),
                "sleep",
                "180",
            ],
            &values,
            &out.join(log),
        )?;
    }
    let (source, bystander) = wait("source and bystander windows", || {
        let source = node_by_app(&swaymsg, &values, "overview-source")?;
        let bystander = node_by_app(&swaymsg, &values, "overview-bystander")?;
        Ok(source.zip(bystander))
    })?;
    let cid = id(&source)?;
    let bid = id(&bystander)?;
    sway(
        &swaymsg,
        &values,
        &format!("[con_id={cid}] move container to workspace 2"),
    )?;
    children.launch(
        &foot,
        &[
            "-o",
            "resize-by-cells=no",
            "--app-id=overview-replacement",
            "sleep",
            "180",
        ],
        &values,
        &out.join("replacement.log"),
    )?;
    let rid = id(&wait("replacement window", || {
        node_by_app(&swaymsg, &values, "overview-replacement")
    })?)?;
    copy_tree(&options.assets, &out.join("lumen-desktop"))?;
    let palette: Value = serde_json::from_slice(&fs::read(
        out.join("lumen-desktop/appearance/palette.json"),
    )?)?;
    let appearance = private.join("state/lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("Appearance parent missing")?)?;
    fs::write(
        &appearance,
        json!({"palette":{
        "background":"#f2f2f4","surface":"#ffffff","field":"#e5e5e8","text":"#202023",
        "muted":"#6b6b70","border":"#c8c8ce","hover":"#dedee3","accent":"#007aff",
        "selectedText":"#ffffff"},"font":palette["font"],"preferences":{}})
        .to_string(),
    )?;
    let fixture = out.join("picker.qml");
    fs::write(&fixture, include_str!("../fixture.qml"))?;
    children.launch(
        &options.keyboard,
        &["-s", "60000"],
        &values,
        &out.join("keyboard.log"),
    )?;
    children.launch(
        &quickshell,
        &["-p", fixture.to_str().ok_or("Fixture path invalid")?],
        &values,
        &out.join("picker.log"),
    )?;
    wait("initial picker state", || {
        Ok((state(&quickshell, &fixture, &values)?["visible"] == false).then_some(()))
    })?;
    route(&quickshell, &fixture, &values, cid, "left")?;
    wait("left pair picker", || {
        loaded(&quickshell, &fixture, &values, cid)
    })?;
    select(&quickshell, &fixture, &values, bid)?;
    require(
        state(&quickshell, &fixture, &values)?["current"]["id"].as_i64() == Some(bid),
        "Partner selection changed",
    )?;
    call(&options.keyboard, &["-k", "Return"], &values)?;
    wait("pair creation", || {
        Ok((state(&quickshell, &fixture, &values)?["visible"] == false).then_some(()))
    })?;
    let paired = node_by_id(&swaymsg, &values, cid)?.ok_or("Source window disappeared")?;
    require(
        paired["lumen_pair"]["partner"].as_i64() == Some(bid)
            && paired["lumen_pair"]["side"] == "left",
        "Native pair identity or side differs",
    )?;
    route(&quickshell, &fixture, &values, cid, "manage")?;
    wait("management picker", || {
        loaded(&quickshell, &fixture, &values, cid)
    })?;
    call(
        &grim,
        &[out
            .join("pair-management.png")
            .to_str()
            .ok_or("Screenshot path invalid")?],
        &values,
    )?;
    click(&quickshell, &fixture, &values, "Swap Sides")?;
    wait("side swap", || {
        Ok(loaded(&quickshell, &fixture, &values, cid)?
            .filter(|state| state["source"]["pair"]["side"] == "right"))
    })?;
    sway(&swaymsg, &values, &format!("lumen_pair ratio {cid} 0.65"))?;
    click(&quickshell, &fixture, &values, "Reset Division")?;
    wait("division reset", || {
        let node =
            node_by_id(&swaymsg, &values, cid)?.ok_or("Source missing after division reset")?;
        let ratio = node["lumen_pair"]["ratio"].as_f64().unwrap_or(f64::NAN);
        Ok(
            ((ratio - 0.5).abs() < 0.01 && loaded(&quickshell, &fixture, &values, cid)?.is_some())
                .then_some(()),
        )
    })?;
    select(&quickshell, &fixture, &values, rid)?;
    click(&quickshell, &fixture, &values, "Replace Partner")?;
    wait("partner replacement", || {
        Ok((state(&quickshell, &fixture, &values)?["visible"] == false).then_some(()))
    })?;
    let kept = node_by_id(&swaymsg, &values, cid)?.ok_or("Kept window missing")?;
    let prior = node_by_id(&swaymsg, &values, bid)?.ok_or("Old partner missing")?;
    require(
        kept["lumen_pair"]["partner"].as_i64() == Some(rid) && prior["lumen_pair"]["id"].is_null(),
        "Replacement retained the wrong identities",
    )?;
    route(&quickshell, &fixture, &values, cid, "manage")?;
    wait("return picker", || {
        loaded(&quickshell, &fixture, &values, cid)
    })?;
    click(&quickshell, &fixture, &values, "Return to Desktop")?;
    wait("source return", || {
        Ok((state(&quickshell, &fixture, &values)?["visible"] == false).then_some(()))
    })?;
    let kept = node_by_id(&swaymsg, &values, cid)?.ok_or("Source missing after return")?;
    let survivor = node_by_id(&swaymsg, &values, rid)?.ok_or("Survivor missing")?;
    require(
        kept["lumen_pair"]["id"].is_null() && survivor["lumen_pair"]["survivor"] == true,
        "Return did not preserve survivor",
    )?;
    route(&quickshell, &fixture, &values, rid, "manage")?;
    wait("survivor picker", || {
        loaded(&quickshell, &fixture, &values, rid)
    })?;
    click(&quickshell, &fixture, &values, "Return to Desktop")?;
    wait("survivor return", || {
        Ok((state(&quickshell, &fixture, &values)?["visible"] == false).then_some(()))
    })?;
    let restored = node_by_id(&swaymsg, &values, rid)?.ok_or("Restored partner missing")?;
    require(
        restored["fullscreen_mode"] == 0 && restored["lumen_pair"]["survivor"] == false,
        "Survivor did not return to desktop",
    )?;
    route(&quickshell, &fixture, &values, cid, "right")?;
    wait("new pair picker", || {
        loaded(&quickshell, &fixture, &values, cid)
    })?;
    select(&quickshell, &fixture, &values, bid)?;
    sway(&swaymsg, &values, &format!("[con_id={bid}] kill"))?;
    wait("old partner closure", || {
        Ok(node_by_id(&swaymsg, &values, bid)?.is_none().then_some(()))
    })?;
    click(&quickshell, &fixture, &values, "Create Pair")?;
    wait("closed partner refusal", || {
        Ok(state(&quickshell, &fixture, &values)?["error"]
            .as_str()
            .filter(|value| value.contains("no longer available"))
            .map(|_| ()))
    })?;
    thread::sleep(Duration::from_millis(300));
    let kept = node_by_id(&swaymsg, &values, cid)?.ok_or("Source missing after refusal")?;
    let restored =
        node_by_id(&swaymsg, &values, rid)?.ok_or("Replacement missing after refusal")?;
    let final_state = state(&quickshell, &fixture, &values)?;
    require(
        kept["lumen_pair"]["id"].is_null()
            && restored["lumen_pair"]["id"].is_null()
            && final_state["current"].is_null(),
        "Closed candidate was silently replaced",
    )?;
    let log = fs::read_to_string(out.join("picker.log"))?;
    require(
        ![" WARN ", " ERROR ", "TypeError:", "ReferenceError:"]
            .iter()
            .any(|word| log.contains(word)),
        format!("QML warning or error: {log}"),
    )?;
    let checks = [
        "real layout menu captured source routing",
        "native create via production picker and physical Return",
        "overlay usable above full-screen pair",
        "swap exact identities",
        "reset division",
        "replace and restore old partner",
        "exit and survivor restore",
        "closed selected partner rejected without fallback",
    ];
    fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"source":cid,"partner":bid,"replacement":rid,"final":final_state,"checks":checks}),
        )?,
    )?;
    println!("PASS real paired-window menu, picker and native lifecycle");
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let options = options()?;
        if options.private.is_some() {
            run(&options)
        } else {
            outer(&options)
        }
    })();
    if let Err(error) = result {
        eprintln!("Paired picker fixture failed: {error}");
        std::process::exit(1);
    }
}
