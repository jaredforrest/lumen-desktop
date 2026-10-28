//! Private native parent/modal focus and actual keyboard-recipient regression.
use serde_json::{json, Value};
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
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(fail(message))
    }
}
fn id(value: &Value) -> Result<i64> {
    value["id"]
        .as_i64()
        .ok_or_else(|| fail(format!("Native window ID missing: {value}")))
}
fn context_row(value: &Value, cid: i64) -> Result<&Value> {
    value
        .get(cid.to_string())
        .ok_or_else(|| fail(format!("Desktop context for {cid} missing")))
}

struct Options {
    sway: PathBuf,
    client: PathBuf,
    keyboard: PathBuf,
    state: PathBuf,
    output: PathBuf,
    private: Option<PathBuf>,
    effects: bool,
}
fn options() -> Result<Options> {
    let mut sway = None;
    let mut client = None;
    let mut keyboard = None;
    let mut state = None;
    let mut output = None;
    let mut private = None;
    let mut effects = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
            Some("--client") => client = Some(PathBuf::from(args.next().ok_or("Missing client")?)),
            Some("--keyboard") => {
                keyboard = Some(PathBuf::from(args.next().ok_or("Missing keyboard")?))
            }
            Some("--state-bin") => {
                state = Some(PathBuf::from(
                    args.next().ok_or("Missing desktop-state command")?,
                ))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--private") => {
                private = Some(PathBuf::from(args.next().ok_or("Missing private runtime")?))
            }
            Some("--effects") => effects = true,
            _ => return Err(fail("Unknown window relationships option")),
        }
    }
    Ok(Options {
        sway: sway.ok_or("--sway required")?,
        client: client.ok_or("--client required")?,
        keyboard: keyboard.ok_or("--keyboard required")?,
        state: state.ok_or("--state-bin required")?,
        output: output.ok_or("--output required")?,
        private,
        effects,
    })
}
fn program(name: &str, values: &BTreeMap<String, String>) -> Result<PathBuf> {
    let path = values.get("PATH").ok_or("Pinned PATH missing")?;
    for folder in env::split_paths(path) {
        let candidate = folder.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(fail(format!("{name} unavailable in pinned PATH")))
}
fn environment(root: &Path, effects: bool) -> BTreeMap<String, String> {
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    for key in [
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "SWAYSOCK",
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
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if effects { "gles2" } else { "pixman" }),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
    ] {
        values.insert(key.into(), value.into());
    }
    values.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/absent-system", root.display()),
    );
    values
}
fn outer(options: &Options) -> Result<()> {
    require(!options.output.exists(), "Fresh output directory required")?;
    fs::create_dir_all(&options.output)?;
    let output = fs::canonicalize(&options.output)?;
    let private = tempfile::Builder::new()
        .prefix("lumen-window-relations-")
        .tempdir()?;
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o700))?;
    for key in ["config", "state", "data", "cache"] {
        fs::create_dir(private.path().join(key))?;
    }
    let config = private.path().join("bus.conf");
    fs::write(&config, format!("<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>", private.path().display()))?;
    let values = environment(private.path(), options.effects);
    let keyboard = if options.keyboard == Path::new("wtype") {
        program("wtype", &values)?
    } else {
        fs::canonicalize(&options.keyboard)?
    };
    let mut command = Command::new(program("timeout", &values)?);
    command
        .arg("90s")
        .arg(program("dbus-run-session", &values)?)
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--sway")
        .arg(fs::canonicalize(&options.sway)?)
        .arg("--client")
        .arg(fs::canonicalize(&options.client)?)
        .arg("--keyboard")
        .arg(keyboard)
        .arg("--state-bin")
        .arg(fs::canonicalize(&options.state)?)
        .arg("--output")
        .arg(output)
        .arg("--private")
        .arg(private.path())
        .env_clear()
        .envs(values);
    if options.effects {
        command.arg("--effects");
    }
    let status = command.status()?;
    require(
        status.success(),
        format!("Private window relationships fixture exited {status}"),
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
        interactive: bool,
    ) -> Result<usize> {
        let file = File::create(log)?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(values)
            .stdin(if interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file))
            .process_group(0)
            .spawn()?;
        self.0.push(child);
        Ok(self.0.len() - 1)
    }
    fn send(&mut self, index: usize, command: &str) -> Result<()> {
        let input = self.0[index]
            .stdin
            .as_mut()
            .ok_or("Client input unavailable")?;
        input.write_all(command.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
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
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut last_error = None;
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last_error = Some(error.to_string()),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!(
                "Window relationships timed out waiting for {label}: {}",
                last_error.unwrap_or_default()
            )));
        }
        thread::sleep(Duration::from_millis(30));
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
fn find_node(node: &Value, predicate: &impl Fn(&Value) -> bool, result: &mut Vec<Value>) {
    if predicate(node) {
        result.push(node.clone());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            find_node(child, predicate, result);
        }
    }
}
fn call(program: &Path, args: &[&str], values: &BTreeMap<String, String>) -> Result<String> {
    let output = Command::new("timeout")
        .arg("4s")
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
    Ok(String::from_utf8(output.stdout)?)
}
fn command_json(
    program: &Path,
    action: &str,
    input: &Value,
    values: &BTreeMap<String, String>,
) -> Result<Value> {
    let mut child = Command::new("timeout")
        .arg("5s")
        .arg(program)
        .arg(action)
        .env_clear()
        .envs(values)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("State command stdin missing")?
        .write_all(input.to_string().as_bytes())?;
    let output = child.wait_with_output()?;
    require(
        output.status.success(),
        format!(
            "Desktop state {action} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    Ok(serde_json::from_slice(&output.stdout)?)
}

struct Fixture<'a> {
    options: &'a Options,
    values: BTreeMap<String, String>,
    swaymsg: PathBuf,
    children: Children,
    client: usize,
}
impl<'a> Fixture<'a> {
    fn new(options: &'a Options) -> Result<Self> {
        let private = options.private.as_ref().ok_or("Private runtime required")?;
        let mut values = env::vars().collect::<BTreeMap<_, _>>();
        require(
            values
                .get("DBUS_SESSION_BUS_ADDRESS")
                .is_some_and(|address| address.starts_with("unix:")),
            "Private session bus required",
        )?;
        let swaymsg = options
            .sway
            .parent()
            .ok_or("Sway package path missing")?
            .join("swaymsg");
        require(swaymsg.is_file(), "Pinned compositor IPC command missing")?;
        let mut children = Children(Vec::new());
        let config = options.output.join("sway.conf");
        fs::write(&config, "output HEADLESS-1 resolution 1440x1000\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\n")?;
        children.launch(
            &options.sway,
            &[
                "--unsupported-gpu",
                "-c",
                config.to_str().ok_or("Sway config path invalid")?,
            ],
            &values,
            &options.output.join("sway.log"),
            false,
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
                .ok_or("Display filename invalid")?
                .to_string_lossy()
                .into_owned(),
        );
        let version: Value =
            serde_json::from_str(&call(&swaymsg, &["-r", "-t", "get_version"], &values)?)?;
        require(
            version["native_window_relationships"] == true,
            "Native relationships capability missing",
        )?;
        let client = children.launch(
            &options.client,
            &[],
            &values,
            &options.output.join("client.log"),
            true,
        )?;
        let fixture = Self {
            options,
            values,
            swaymsg,
            children,
            client,
        };
        wait("four relationship windows", || {
            Ok((fixture.observed()?.len() == 4).then_some(()))
        })?;
        // Keep a virtual keyboard device present throughout focus transitions.
        let mut fixture = fixture;
        fixture.children.launch(
            &options.keyboard,
            &["-s", "60000"],
            &fixture.values,
            &options.output.join("keyboard.log"),
            false,
        )?;
        Ok(fixture)
    }
    fn ipc(&self, args: &[&str], allow_error: bool) -> Result<Value> {
        let output = Command::new("timeout")
            .arg("4s")
            .arg(&self.swaymsg)
            .arg("-r")
            .args(args)
            .env_clear()
            .envs(&self.values)
            .output()?;
        if !allow_error {
            require(
                output.status.success(),
                format!(
                    "Sway IPC {:?} failed: {}",
                    args,
                    String::from_utf8_lossy(&output.stderr)
                ),
            )?;
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    fn yes(&self, command: &str) -> Result<()> {
        let reply = self.ipc(&[command], false)?;
        require(
            reply
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["success"] == true)),
            format!("Sway command rejected: {command}: {reply}"),
        )
    }
    fn no(&self, command: &str) -> Result<()> {
        let reply = self.ipc(&[command], true)?;
        require(
            reply
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["success"] == false)),
            format!("Sway command unexpectedly succeeded: {command}: {reply}"),
        )
    }
    fn tree(&self) -> Result<Value> {
        self.ipc(&["-t", "get_tree"], false)
    }
    fn observed(&self) -> Result<BTreeMap<i64, Value>> {
        let mut nodes = Vec::new();
        find_node(
            &self.tree()?,
            &|node| {
                node["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("Relationship "))
            },
            &mut nodes,
        );
        let mut result = BTreeMap::new();
        for node in nodes {
            let name = node["name"].as_str().ok_or("Relationship name missing")?;
            let number: i64 = name
                .split_whitespace()
                .last()
                .ok_or("Relationship number missing")?
                .parse()?;
            result.insert(number, node);
        }
        Ok(result)
    }
    fn row(&self, number: i64) -> Result<Value> {
        self.observed()?
            .remove(&number)
            .ok_or_else(|| fail(format!("Relationship {number} missing")))
    }
    fn context(&self, action: &str, value: &Value) -> Result<Value> {
        command_json(&self.options.state, action, value, &self.values)
    }
    fn focus(&self, number: i64, expected: Option<i64>) -> Result<()> {
        let target = expected.unwrap_or(number);
        let target_text = target.to_string();
        let log = self.options.output.join("client.log");
        let before = fs::read_to_string(&log)?.len();
        self.yes(&format!("[con_id={}] focus", id(&self.row(number)?)?))?;
        wait("focused relationship window", || {
            let rows = self.observed()?;
            let focused = rows
                .values()
                .find(|row| row["focused"] == true)
                .and_then(|row| row["id"].as_i64());
            Ok((focused
                == Some(id(rows
                    .get(&target)
                    .ok_or("Expected focus target missing")?)?))
            .then_some(()))
        })?;
        call(&self.options.keyboard, &["x"], &self.values)?;
        wait("actual keyboard recipient", || {
            let text = fs::read_to_string(&log)?;
            let suffix = text.get(before..).ok_or("Client log shrank")?;
            let keys = suffix
                .lines()
                .filter(|line| line.starts_with("key ") && line.ends_with(" 1"))
                .collect::<Vec<_>>();
            if keys.is_empty() {
                return Ok(None);
            }
            require(
                keys.iter()
                    .all(|line| line.split_whitespace().nth(1) == Some(target_text.as_str())),
                format!("Keyboard reached wrong window: {keys:?}"),
            )?;
            Ok(Some(()))
        })
    }
    fn command(&mut self, text: &str) -> Result<()> {
        let log = self.options.output.join("client.log");
        let before = fs::read_to_string(&log)?.len();
        self.children.send(self.client, text)?;
        wait(&format!("client ack {text}"), || {
            let content = fs::read_to_string(&log)?;
            let suffix = content.get(before..).ok_or("Client log shrank")?;
            Ok(suffix.contains(&format!("ack {text}")).then_some(()))
        })
    }
}

fn run(options: &Options) -> Result<()> {
    let mut fixture = Fixture::new(options)?;
    let first = fixture.observed()?;
    fs::write(
        options.output.join("initial-tree.json"),
        serde_json::to_vec_pretty(&first)?,
    )?;
    let parent = id(first.get(&0).ok_or("Parent window missing")?)?;
    let panel = id(first.get(&2).ok_or("Utility panel missing")?)?;
    let other = id(first.get(&3).ok_or("Other app missing")?)?;
    require(
        first.get(&2).ok_or("Utility panel missing")?["lumen_relationships"]
            == json!({"version":1,"parent_supported":true,"parent":parent,"modal":null}),
        "Initial native parent metadata differs",
    )?;
    let initial = fixture.context("--tree-context-json", &fixture.tree()?)?;
    require(
        context_row(&initial, panel)?["application_hint"] == "org.example.Editor"
            && context_row(&initial, panel)?["context_window"].as_i64() == Some(parent),
        "Utility context did not inherit editor document",
    )?;
    require(
        initial
            .as_object()
            .is_some_and(|rows| rows.values().all(|row| row["main_window"].is_null())),
        "Native context invented a main document",
    )?;
    let mut checks = Vec::new();
    for target in [2, 0, 3, 1] {
        fixture.focus(target, None)?;
    }
    checks.push("Utility, document and other application receive their own actual keyboard input");

    let rows = initial
        .as_object()
        .ok_or("Native contexts object missing")?;
    let windows = rows.iter().map(|(key, value)| {
        Ok(json!({"id":key.parse::<i64>()?,"app_hint":value["application_hint"],"parent":value["parent"]}))
    }).collect::<Result<Vec<_>>>()?;
    let adapted = fixture.context(
        "--contexts-json",
        &json!({"windows":windows,"mainByApp":{"org.example.Editor":parent}}),
    )?;
    require(
        context_row(&adapted, panel)?["main_window"].as_i64() == Some(parent)
            && context_row(&adapted, other)?["main_window"].is_null(),
        "Adapter invented or lost document association",
    )?;
    checks.push("Independent adapter document association survives utility key focus without inventing an external main document");

    fixture.focus(0, None)?;
    fixture.command("modal")?;
    wait("modal utility focus", || {
        let row = fixture.row(2)?;
        Ok((row["lumen_relationships"]["modal"] == true && row["focused"] == true).then_some(()))
    })?;
    fixture.focus(0, Some(2))?;
    fixture.focus(1, None)?;
    fixture.focus(3, None)?;
    checks.push("Real xdg-dialog modal blocks only its parent branch; sibling document and other app remain independently key");

    fixture.command("unmodal")?;
    wait("modal utility unset", || {
        Ok((fixture.row(2)?["lumen_relationships"]["modal"] == false).then_some(()))
    })?;
    fixture.focus(0, None)?;
    fixture.command("reparent")?;
    wait("utility reparenting", || {
        Ok(
            (fixture.row(2)?["lumen_relationships"]["parent"] == fixture.row(1)?["id"])
                .then_some(()),
        )
    })?;
    fixture.command("modal")?;
    fixture.focus(0, None)?;
    fixture.focus(1, Some(2))?;
    checks.push("Dynamic unset/reparent/set modal updates actual focus scope");

    fixture.command("unparent")?;
    wait("utility parent removal", || {
        Ok(fixture.row(2)?["lumen_relationships"]["parent"]
            .is_null()
            .then_some(()))
    })?;
    fixture.focus(0, None)?;
    fixture.focus(1, None)?;
    fixture.focus(2, None)?;
    require(
        context_row(
            &fixture.context("--tree-context-json", &fixture.tree()?)?,
            panel,
        )?["application_hint"]
            == "org.example.Utility",
        "Parentless utility inherited wrong application hint",
    )?;
    let before_panel = fixture.row(2)?["rect"].clone();
    let before_other = fixture.row(3)?["rect"].clone();
    fixture.no(&format!("lumen_pair create {other} {panel}"))?;
    require(
        fixture.row(2)?["lumen_pair"]["eligible"] == false
            && fixture.row(2)?["rect"] == before_panel
            && fixture.row(3)?["rect"] == before_other,
        "Parentless modal dialog changed pair placement",
    )?;
    checks.push("Parentless modal dialog rejects paired-primary admission atomically");

    fixture.command("destroy-dialog")?;
    wait("dialog object removal", || {
        Ok(fixture.row(2)?["lumen_relationships"]["modal"]
            .is_null()
            .then_some(()))
    })?;
    fixture.focus(0, None)?;
    require(
        fixture.row(2)?["lumen_pair"]["eligible"] == true,
        "Former dialog remained ineligible",
    )?;
    fixture.yes(&format!("lumen_pair create {other} {panel}"))?;
    require(
        fixture.row(2)?["lumen_pair"]["partner"].as_i64() == Some(other),
        "Eligible utility paired with wrong partner",
    )?;
    for member in [panel, other] {
        fixture.yes(&format!("lumen_pair leave {member}"))?;
    }
    checks.push(
        "Removing dialog hint restores independent pairing eligibility without stale ownership",
    );

    fixture.command("reparent")?;
    fixture.command("modal")?;
    fixture.focus(1, Some(2))?;
    fixture.command("close-second")?;
    wait("second editor closure", || {
        Ok((!fixture.observed()?.contains_key(&1)).then_some(()))
    })?;
    require(
        fixture.row(2)?["lumen_relationships"]["parent"].is_null(),
        "Closing parent retained stale relationship",
    )?;
    fixture.focus(2, None)?;
    fixture.focus(3, None)?;
    checks.push("Unparenting, dialog object destruction, parentless modal and live orphan panel retain exact protocol scope");

    fixture.command("close-panel")?;
    wait("panel closure", || {
        Ok((!fixture.observed()?.contains_key(&2)).then_some(()))
    })?;
    fixture.focus(0, None)?;
    fixture.command("close-parent")?;
    wait("editor parent closure", || {
        Ok((!fixture.observed()?.contains_key(&0)).then_some(()))
    })?;
    fixture.focus(3, None)?;
    checks.push("Destroyed modal and parent leave no stale focus target or application context");
    fs::write(
        options.output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"checks":checks,"scope":"Isolated native compositor, disposable Wayland clients and virtual keyboard only","effects":options.effects}),
        )?,
    )?;
    println!("PASS native parent/modal/key-window regression");
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
        eprintln!("Window relationships fixture failed: {error}");
        std::process::exit(1);
    }
}
