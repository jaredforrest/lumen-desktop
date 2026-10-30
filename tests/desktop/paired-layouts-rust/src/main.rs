//! Native paired full-output identity, divider, and restore lifecycle acceptance.
mod documented_negatives;
mod scratchpad_preflight;
use serde_json::{Value, json};
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
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn unavailable_reply(status: Option<i32>, stdout: &[u8], expected_error: &str) -> Result<Value> {
    let answer: Value = serde_json::from_slice(stdout)?;
    require(
        status == Some(2)
            && answer.as_array().is_some_and(|rows| rows.len() == 1)
            && answer[0]["success"] == false
            && answer[0]["parse_error"] == true
            && answer[0]["error"] == expected_error,
        format!("Expected exact unavailable action reply, got {status:?}: {answer}"),
    )?;
    Ok(answer)
}
fn i(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| fail(format!("Missing integer {key}: {value}")))
}
fn f(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| fail(format!("Missing number {key}: {value}")))
}
fn id(value: &Value) -> Result<i64> {
    i(value, "id")
}

struct Options {
    sway: PathBuf,
    pointer: PathBuf,
    keyboard: PathBuf,
    constraint: PathBuf,
    state: PathBuf,
    output: PathBuf,
    private: Option<PathBuf>,
    effects: bool,
    divider_only: bool,
    scratchpad_only: bool,
}
fn options() -> Result<Options> {
    let mut sway = None;
    let mut pointer = None;
    let mut keyboard = None;
    let mut constraint = None;
    let mut state = None;
    let mut output = None;
    let mut private = None;
    let mut effects = false;
    let mut divider_only = false;
    let mut scratchpad_only = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway")?)),
            Some("--pointer") => {
                pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer")?))
            }
            Some("--keyboard") => {
                keyboard = Some(PathBuf::from(args.next().ok_or("Missing keyboard")?))
            }
            Some("--constraint-client") => {
                constraint = Some(PathBuf::from(
                    args.next().ok_or("Missing constraint client")?,
                ))
            }
            Some("--state-bin") => {
                state = Some(PathBuf::from(
                    args.next().ok_or("Missing desktop state binary")?,
                ))
            }
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("Missing output")?)),
            Some("--private") => {
                private = Some(PathBuf::from(args.next().ok_or("Missing private runtime")?))
            }
            Some("--effects") => effects = true,
            Some("--divider-only") => divider_only = true,
            Some("--scratchpad-only") => scratchpad_only = true,
            _ => return Err(fail("Unknown paired-layouts fixture option")),
        }
    }
    Ok(Options {
        sway: sway.ok_or("--sway required")?,
        pointer: pointer.ok_or("--pointer required")?,
        keyboard: keyboard.ok_or("--keyboard required")?,
        constraint: constraint.ok_or("--constraint-client required")?,
        state: state.ok_or("--state-bin required")?,
        output: output.ok_or("--output required")?,
        private,
        effects,
        divider_only,
        scratchpad_only,
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
    for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
        values.insert(
            format!("XDG_{key}_HOME"),
            root.join(key.to_lowercase()).display().to_string(),
        );
    }
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_HEADLESS_OUTPUTS", "2"),
        ("WLR_RENDERER", if effects { "gles2" } else { "pixman" }),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("QT_ACCESSIBILITY", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
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
    let private = tempfile::Builder::new().prefix("lumen-pair-").tempdir()?;
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o700))?;
    for key in ["config", "state", "cache", "data"] {
        fs::create_dir(private.path().join(key))?;
    }
    let config = private.path().join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            private.path().display()
        ),
    )?;
    let values = environment(private.path(), options.effects);
    let keyboard = if options.keyboard == Path::new("wtype") {
        program("wtype", &values)?
    } else {
        fs::canonicalize(&options.keyboard)?
    };
    let mut command = Command::new(program("timeout", &values)?);
    command
        .arg("120s")
        .arg(program("dbus-run-session", &values)?)
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--sway")
        .arg(fs::canonicalize(&options.sway)?)
        .arg("--pointer")
        .arg(fs::canonicalize(&options.pointer)?)
        .arg("--keyboard")
        .arg(keyboard)
        .arg("--constraint-client")
        .arg(fs::canonicalize(&options.constraint)?)
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
    if options.divider_only {
        command.arg("--divider-only");
    }
    if options.scratchpad_only {command.arg("--scratchpad-only");}
    let status = command.status()?;
    require(
        status.success(),
        format!("Private paired-layouts fixture exited {status}"),
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
            .ok_or("Interactive child input unavailable")?;
        input.write_all(command.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        Ok(())
    }
    fn send_burst(&mut self, index: usize, commands: &str) -> Result<()> {
        let input = self.0[index]
            .stdin
            .as_mut()
            .ok_or("Interactive child input unavailable")?;
        input.write_all(commands.as_bytes())?;
        input.flush()?;
        Ok(())
    }
    fn wait_exit(&mut self, index: usize) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.0[index].try_wait()? {
                return require(status.success(), format!("Client {index} exited {status}"));
            }
            if Instant::now() >= deadline {
                return Err(fail(format!("Client {index} did not exit")));
            }
            thread::sleep(Duration::from_millis(25));
        }
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
                "Paired layouts timed out waiting for {label}: {}",
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
fn workspace_node(node: &Value, target: i64, current: &str) -> Option<String> {
    let current = if node["type"] == "workspace" {
        node["name"].as_str().unwrap_or(current)
    } else {
        current
    };
    if node["id"].as_i64() == Some(target) {
        return Some(current.to_owned());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in node[key].as_array().into_iter().flatten() {
            if let Some(found) = workspace_node(child, target, current) {
                return Some(found);
            }
        }
    }
    None
}
struct Fixture<'a> {
    options: &'a Options,
    values: BTreeMap<String, String>,
    swaymsg: PathBuf,
    foot: PathBuf,
    grim: PathBuf,
    children: Children,
    pointer: usize,
}
impl<'a> Fixture<'a> {
    fn new(options: &'a Options) -> Result<Self> {
        let private = options.private.as_ref().ok_or("Private runtime required")?;
        let mut values = env::vars().collect::<BTreeMap<_, _>>();
        require(
            values
                .get("DBUS_SESSION_BUS_ADDRESS")
                .is_some_and(|address| address.starts_with("unix:")),
            "Private D-Bus required",
        )?;
        let swaymsg = program("swaymsg", &values)?;
        let foot = program("foot", &values)?;
        let grim = program("grim", &values)?;
        let mut children = Children(Vec::new());
        let config = options.output.join("sway.conf");
        fs::write(
            &config,
            "output HEADLESS-1 resolution 1280x720 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0\nseat seat0 fallback true\ndefault_border normal 2\nno_focus [app_id=\"pair-no-focus\"]\n",
        )?;
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
                .ok_or("Display name invalid")?
                .to_string_lossy()
                .into_owned(),
        );
        let mut fixture = Self {
            options,
            values,
            swaymsg,
            foot,
            grim,
            children,
            pointer: usize::MAX,
        };
        require(
            fixture.query("get_version")?["native_paired_layouts"] == true,
            "Native paired layout capability missing",
        )?;
        fixture.pointer = fixture.children.launch(
            &options.pointer,
            &["pointer", "move", "0", "0"],
            &fixture.values,
            &options.output.join("pointer.log"),
            true,
        )?;
        fixture.motion(0.0, 0.0)?;
        Ok(fixture)
    }
    fn launch_foot(&mut self, name: &str) -> Result<()> {
        self.children.launch(
            &self.foot,
            &[
                "-o",
                "resize-by-cells=no",
                &format!("--app-id={name}"),
                "sleep",
                "180",
            ],
            &self.values,
            &self.options.output.join(format!("{name}.log")),
            false,
        )?;
        wait(&format!("{name} window"), || self.app(name))?;
        Ok(())
    }
    fn launch_client(&mut self, mode: &str, log: &str) -> Result<usize> {
        let index = self.children.launch(
            &self.options.constraint,
            &[mode],
            &self.values,
            &self.options.output.join(log),
            true,
        )?;
        wait("constraint client", || self.app("constraint-test"))?;
        Ok(index)
    }
    fn client(&mut self, index: usize, command: &str) -> Result<()> {
        self.children.send(index, command)
    }
    fn close_client(&mut self, index: usize) -> Result<()> {
        self.client(index, "q")?;
        self.children.wait_exit(index)
    }
    fn query(&self, kind: &str) -> Result<Value> {
        Ok(serde_json::from_str(&call(
            &self.swaymsg,
            &["-r", "-t", kind],
            &self.values,
        )?)?)
    }
    fn ipc(&self, command: &str, success: bool) -> Result<Value> {
        let output = Command::new("timeout")
            .arg("5s")
            .arg(&self.swaymsg)
            .args(["-r", command])
            .env_clear()
            .envs(&self.values)
            .output()?;
        let answer: Value = serde_json::from_slice(&output.stdout)?;
        require(
            answer
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["success"] == success)),
            format!("Sway command {command} returned {answer}"),
        )?;
        thread::sleep(Duration::from_millis(120));
        Ok(answer)
    }
    fn yes(&self, command: &str) -> Result<()> {
        self.ipc(command, true)?;
        Ok(())
    }
    fn no(&self, command: &str) -> Result<()> {
        self.ipc(command, false)?;
        Ok(())
    }
    fn unavailable(&self, command: &str) -> Result<Value> {
        self.rejected(
            command,
            "paired-minimize-command.json",
            "Window action is unavailable",
        )
    }
    fn rejected(&self, command: &str, receipt_name: &str, expected_error: &str) -> Result<Value> {
        let output = Command::new("timeout")
            .arg("5s")
            .arg(&self.swaymsg)
            .args(["-r", command])
            .env_clear()
            .envs(&self.values)
            .output()?;
        let receipt = json!({"command":command,"exitCode":output.status.code(),
            "stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)});
        fs::write(
            self.options.output.join(receipt_name),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
        unavailable_reply(output.status.code(), &output.stdout, expected_error)?;
        Ok(receipt)
    }
    fn tree(&self) -> Result<Value> {
        self.query("get_tree")
    }
    fn find(&self, cid: i64) -> Result<Option<Value>> {
        Ok(find_node(&self.tree()?, &|node| {
            node["id"].as_i64() == Some(cid)
        }))
    }
    fn app(&self, name: &str) -> Result<Option<Value>> {
        Ok(find_node(&self.tree()?, &|node| node["app_id"] == name))
    }
    fn required(&self, cid: i64) -> Result<Value> {
        self.find(cid)?
            .ok_or_else(|| fail(format!("Window {cid} missing")))
    }
    fn box_rect(&self, cid: i64) -> Result<Value> {
        let node = self.required(cid)?;
        let mut rect = node["rect"].clone();
        let deco = i(&node["deco_rect"], "height")?;
        rect["y"] = json!(i(&rect, "y")? - deco);
        rect["height"] = json!(i(&rect, "height")? + deco);
        Ok(rect)
    }
    fn full_parent(&self, cid: i64) -> Result<Option<Value>> {
        Ok(find_node(&self.tree()?, &|node| {
            node["fullscreen_mode"] == 1
                && node["nodes"].as_array().is_some_and(|children| {
                    children
                        .iter()
                        .any(|child| child["id"].as_i64() == Some(cid))
                })
        }))
    }
    fn required_parent(&self, cid: i64) -> Result<Value> {
        self.full_parent(cid)?
            .ok_or_else(|| fail(format!("Full-screen pair parent for {cid} missing")))
    }
    fn workspace(&self, cid: i64) -> Result<String> {
        workspace_node(&self.tree()?, cid, "")
            .ok_or_else(|| fail(format!("Workspace for {cid} missing")))
    }
    fn ratio(&self, cid: i64) -> Result<f64> {
        f(&self.required(cid)?["lumen_pair"], "ratio")
    }
    fn at(&self, x: f64, y: f64) -> Result<()> {
        self.yes(&format!("seat seat0 cursor set {x} {y}"))
    }
    fn motion(&mut self, dx: f64, dy: f64) -> Result<()> {
        self.children.send(self.pointer, &format!("m {dx} {dy}"))?;
        thread::sleep(Duration::from_millis(150));
        Ok(())
    }
    fn button(&mut self, value: &str) -> Result<()> {
        self.children.send(self.pointer, value)?;
        thread::sleep(Duration::from_millis(120));
        Ok(())
    }
    fn pointer_burst(&mut self, value: &str) -> Result<()> {
        self.children.send_burst(self.pointer, value)
    }
    fn begin_divider(&mut self, cid: i64) -> Result<()> {
        let parent = self.required_parent(cid)?;
        let rect = &parent["rect"];
        self.at(
            f64::from(i(rect, "x")? as i32) + f(rect, "width")? * self.ratio(cid)?,
            f64::from(i(rect, "y")? as i32) + 200.0,
        )?;
        self.motion(0.1, 0.0)?;
        self.button("p")
    }
    fn snapshot(&self) -> Result<Value> {
        let input = json!({"tree":self.tree()?,"outputs":self.query("get_outputs")?,"epoch":"disposable-pair"});
        let mut child = Command::new(&self.options.state)
            .arg("--snapshot-json")
            .env_clear()
            .envs(&self.values)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or("State stdin missing")?
            .write_all(input.to_string().as_bytes())?;
        let output = child.wait_with_output()?;
        require(
            output.status.success(),
            format!(
                "Rust desktop-state rejected snapshot: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    fn screenshot(&self, name: &str, output: Option<&str>) -> Result<()> {
        let path = self.options.output.join(name);
        let path = path.to_str().ok_or("Screenshot path invalid")?;
        if let Some(display) = output {
            call(&self.grim, &["-o", display, path], &self.values)?;
        } else {
            call(&self.grim, &[path], &self.values)?;
        }
        Ok(())
    }
}

fn run(options: &Options) -> Result<()> {
    let mut fixture = Fixture::new(options)?;
    let x = &options.output;
    for name in ["pair-a", "pair-b", "pair-c"] {
        fixture.launch_foot(name)?;
    }
    let aa = id(&fixture.app("pair-a")?.ok_or("Pair A missing")?)?;
    let bb = id(&fixture.app("pair-b")?.ok_or("Pair B missing")?)?;
    let cc = id(&fixture.app("pair-c")?.ok_or("Pair C missing")?)?;
    let ids = [aa, bb, cc];
    let mut original = BTreeMap::new();
    let mut original_ws = BTreeMap::new();
    for (index, cid) in ids.iter().copied().enumerate() {
        fixture.yes(&format!("[con_id={cid}] floating enable, resize set 360 px 300 px, move absolute position {} px {} px", 100 + index * 180, 130 + index * 35))?;
        original.insert(cid, fixture.box_rect(cid)?);
        original_ws.insert(cid, fixture.workspace(cid)?);
    }
    scratchpad_preflight::run(&mut fixture,aa,bb)?;
    if options.scratchpad_only {return Ok(());}
    fixture.yes(&format!("lumen_pair create {aa} {bb}"))?;
    let parent = fixture.required_parent(aa)?;
    require(
        id(&fixture.required_parent(bb)?)? == id(&parent)?,
        "Both pair members require the same full-screen parent",
    )?;
    require(
        fixture.workspace(aa)?.starts_with("paired-") && fixture.workspace(aa)? != original_ws[&aa],
        "Pair did not enter dedicated workspace",
    )?;
    require(
        fixture.required(aa)?["lumen_pair"]["partner"].as_i64() == Some(bb)
            && fixture.required(bb)?["lumen_pair"]["side"] == "right",
        "Initial native pair identities differ",
    )?;
    require(
        (fixture.ratio(aa)? - 0.5).abs() < 0.01,
        "Initial pair ratio differs",
    )?;
    let observed = fixture.snapshot()?;
    let rows = observed["windows"]
        .as_array()
        .ok_or("Desktop state windows missing")?;
    require(
        rows.iter()
            .filter(|row| row["id"] == format!("w-{aa}") || row["id"] == format!("w-{bb}"))
            .count()
            == 2,
        "Desktop state omitted pair members",
    )?;
    require(
        rows.iter()
            .filter(|row| row["id"] == format!("w-{aa}") || row["id"] == format!("w-{bb}"))
            .all(|row| row["placement"] == "paired"),
        "Desktop state classified pair incorrectly",
    )?;

    documented_negatives::ratios(&fixture, aa, bb)?;
    documented_negatives::no_focus(&mut fixture, aa)?;
    // Show Desktop hides the pair as one group and either member can restore it.
    let parent_id = id(&parent)?;
    fixture.yes("lumen_window show-desktop-on")?;
    require(
        fixture.workspace(aa)? == "__i3_scratch"
            && fixture.workspace(bb)? == "__i3_scratch"
            && fixture.required(aa)?["lumen_pair"]["partner"].as_i64() == Some(bb),
        "Show Desktop split or forgot the pair",
    )?;
    fixture.yes("lumen_window show-desktop-off")?;
    require(
        id(&fixture.required_parent(aa)?)? == parent_id && (fixture.ratio(aa)? - 0.5).abs() < 0.01,
        "Show Desktop restore changed pair identity or ratio",
    )?;
    fixture.yes("lumen_window show-desktop-on")?;
    fixture.yes(&format!("[con_id={aa}] scratchpad show"))?;
    require(
        id(&fixture.required_parent(aa)?)? == parent_id
            && fixture.workspace(aa)?.starts_with("paired-"),
        "Member activation did not restore pair",
    )?;
    fixture.yes("lumen_window show-desktop-on")?;
    fixture.yes(&format!("lumen_pair ratio {aa} 0.5"))?;
    require(
        id(&fixture.required_parent(aa)?)? == parent_id && (fixture.ratio(aa)? - 0.5).abs() < 0.01,
        "Hidden pair management changed identity",
    )?;
    let before_minimize = json!({"member":fixture.required(aa)?,"partner":fixture.required(bb)?,"parent":fixture.required_parent(aa)?});
    let unavailable = fixture.unavailable(&format!("[con_id={aa}] lumen_window minimize"))?;
    let after_minimize = json!({"member":fixture.required(aa)?,"partner":fixture.required(bb)?,"parent":fixture.required_parent(aa)?});
    fs::write(
        x.join("paired-minimize-rejection.json"),
        serde_json::to_vec_pretty(
            &json!({"before":before_minimize,"after":after_minimize,"command":unavailable}),
        )?,
    )?;
    require(
        before_minimize == after_minimize
            && id(&fixture.required_parent(aa)?)? == parent_id
            && fixture.required(aa)?["lumen_state"]["minimised"] == false,
        "Paired member was minimized",
    )?;

    let paired_space = fixture.workspace(aa)?;
    fixture.launch_foot("pair-new")?;
    let new_id = id(&fixture.app("pair-new")?.ok_or("New window missing")?)?;
    require(
        fixture.workspace(new_id)? != paired_space
            && fixture.required_parent(aa)?["nodes"]
                .as_array()
                .is_some_and(|nodes| nodes.len() == 2),
        "New window entered pair",
    )?;
    require(
        fixture.app("pair-new")?.ok_or("New window disappeared")?["focused"] == true,
        "New ordinary app was not focused",
    )?;
    fixture.yes(&format!("[con_id={new_id}] kill"))?;
    fixture.yes(&format!(
        "workspace {}",
        serde_json::to_string(&paired_space)?
    ))?;

    // Physical divider motion and Escape must preserve the other window.
    let other = fixture.box_rect(cc)?;
    fixture.begin_divider(aa)?;
    fixture.motion(180.0, 0.0)?;
    fixture.button("r")?;
    require(
        fixture.ratio(aa)? > 0.6 && fixture.box_rect(cc)? == other,
        "Physical divider changed wrong window",
    )?;
    let changed = fixture.ratio(aa)?;
    fixture.begin_divider(aa)?;
    fixture.motion(-140.0, 0.0)?;
    call(&options.keyboard, &["-k", "Escape"], &fixture.values)?;
    fixture.button("r")?;
    require(
        (fixture.ratio(aa)? - changed).abs() < 0.01,
        "Escape did not restore ratio",
    )?;
    thread::sleep(Duration::from_millis(350));
    fixture.begin_divider(aa)?;
    fixture.button("r")?;
    fixture.button("p")?;
    fixture.button("r")?;
    require(
        (fixture.ratio(aa)? - 0.5).abs() < 0.01,
        "Stationary double click did not reset divider",
    )?;

    // A rapid drag followed by another press must remain a drag, not a double click.
    let parent = fixture.required_parent(aa)?;
    fixture.at(
        f(&parent["rect"], "x")? + f(&parent["rect"], "width")? * fixture.ratio(aa)?,
        f(&parent["rect"], "y")? + 200.0,
    )?;
    fixture.motion(0.1, 0.0)?;
    thread::sleep(Duration::from_millis(350));
    let started = Instant::now();
    fixture.pointer_burst("p\nm 100 0\nr\np\n")?;
    wait("first rapid divider drag", || {
        Ok((fixture.ratio(aa)? > 0.55).then_some(()))
    })?;
    let after_repress = fixture.ratio(aa)?;
    let elapsed = started.elapsed().as_secs_f64();
    require(
        elapsed < 0.3,
        format!("Rapid pointer burst took {elapsed:.3}s"),
    )?;
    fixture.pointer_burst("m 50 0\nr\n")?;
    wait("second rapid divider drag", || {
        Ok((fixture.ratio(aa)? > after_repress + 0.02).then_some(()))
    })?;
    let after_second_drag = fixture.ratio(aa)?;
    let parent = fixture.required_parent(aa)?;
    fixture.at(
        f(&parent["rect"], "x")? + f(&parent["rect"], "width")? * fixture.ratio(aa)?,
        f(&parent["rect"], "y")? + 200.0,
    )?;
    fixture.motion(0.1, 0.0)?;
    thread::sleep(Duration::from_millis(350));
    fixture.pointer_burst("p\nr\np\nr\n")?;
    wait("genuine divider double click", || {
        Ok(((fixture.ratio(aa)? - 0.5).abs() < 0.01).then_some(()))
    })?;
    if options.divider_only {
        let checks = [
            "physical divider drag",
            "Escape restores ratio",
            "quick drag then press starts another drag instead of resetting",
            "stationary double-click resets division",
        ];
        fs::write(
            x.join("result.json"),
            serde_json::to_vec_pretty(
                &json!({"passed":true,"rapid_press_observation_seconds":elapsed,"ratio_after_rapid_repress":after_repress,"ratio_after_second_drag":after_second_drag,"reset_ratio":fixture.ratio(aa)?,"checks":checks}),
            )?,
        )?;
        println!("PASS physical divider movement slop and genuine double-click reset");
        return Ok(());
    }

    fixture.screenshot("paired.png", Some("HEADLESS-1"))?;
    fixture.yes("lumen_window show-desktop-on")?;
    fixture.yes(&format!("lumen_pair swap {aa}"))?;
    require(
        fixture.required(aa)?["lumen_pair"]["side"] == "right"
            && fixture.required(bb)?["lumen_pair"]["side"] == "left",
        "Swap changed wrong pair sides",
    )?;
    fixture.yes("lumen_window show-desktop-on")?;
    fixture.yes(&format!("lumen_pair replace {aa} {cc}"))?;
    require(
        fixture.required(aa)?["lumen_pair"]["partner"].as_i64() == Some(cc),
        "Replacement selected wrong partner",
    )?;
    require(
        fixture.box_rect(bb)? == original[&bb] && fixture.workspace(bb)? == original_ws[&bb],
        "Replaced partner was not restored",
    )?;
    fixture.yes("lumen_window show-desktop-on")?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    require(
        fixture.box_rect(aa)? == original[&aa] && fixture.workspace(aa)? == original_ws[&aa],
        "Source leave did not restore geometry",
    )?;
    require(
        fixture.required(cc)?["fullscreen_mode"] == 1
            && fixture.required(cc)?["lumen_pair"]["survivor"] == true
            && fixture.workspace(cc)?.starts_with("paired-"),
        "Partner did not survive source leave",
    )?;
    fixture.yes(&format!(
        "workspace {}",
        serde_json::to_string(&fixture.workspace(cc)?)?
    ))?;
    fixture.yes("lumen_window show-desktop-on")?;
    fixture.yes("lumen_window show-desktop-off")?;
    require(
        fixture.required(cc)?["lumen_pair"]["survivor"] == true
            && fixture.required(cc)?["fullscreen_mode"] == 1,
        "Survivor reveal changed identity",
    )?;
    documented_negatives::survivor(&fixture, cc)?;
    fixture.yes("lumen_window show-desktop-on")?;
    fixture.yes(&format!("lumen_pair leave {cc}"))?;
    require(
        fixture.box_rect(cc)? == original[&cc] && fixture.workspace(cc)? == original_ws[&cc],
        "Survivor leave did not restore geometry",
    )?;

    // Closing a partner during a divider operation leaves an explicit survivor.
    fixture.yes(&format!("lumen_pair create {aa} {bb}"))?;
    fixture.begin_divider(aa)?;
    fixture.motion(30.0, 0.0)?;
    fixture.yes(&format!("[con_id={bb}] kill"))?;
    wait("closed divider partner", || {
        Ok(fixture.find(bb)?.is_none().then_some(()))
    })?;
    fixture.button("r")?;
    require(
        fixture.required(aa)?["fullscreen_mode"] == 1
            && fixture.required(aa)?["lumen_pair"]["survivor"] == true,
        "Closed partner left a dangling pair",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    require(
        fixture.box_rect(aa)? == original[&aa],
        "Source did not restore after partner close",
    )?;
    fixture.yes(&format!("lumen_pair create {aa} {cc}"))?;
    fixture.yes(&format!("[con_id={cc}] move container to workspace 3"))?;
    require(
        fixture.workspace(cc)? == "3" && fixture.required(aa)?["fullscreen_mode"] == 1,
        "External partner removal changed wrong window",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;

    fixture.yes(&format!("[con_id={aa}] fullscreen enable"))?;
    fixture.yes(&format!("lumen_pair create {aa} {cc}"))?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    require(
        fixture.required(aa)?["fullscreen_mode"] == 0 && fixture.box_rect(aa)? == original[&aa],
        "Originally fullscreen source did not restore ordinary geometry",
    )?;
    fixture.yes(&format!("lumen_pair leave {cc}"))?;
    fixture.yes(&format!("lumen_pair create {aa} {cc}"))?;
    fixture.yes(&format!("[con_id={aa}] lumen_window hide"))?;
    require(
        fixture.required(aa)?["lumen_state"]["hidden"] == true
            && fixture.required(cc)?["lumen_pair"]["survivor"] == true,
        "Hiding source stranded pair",
    )?;
    fixture.yes(&format!("[con_id={aa}] lumen_window unhide"))?;
    require(
        fixture.box_rect(aa)? == original[&aa],
        "Unhide lost original source geometry",
    )?;
    fixture.yes(&format!("lumen_pair leave {cc}"))?;
    fixture.yes(&format!("lumen_pair create {aa} {cc}"))?;
    fixture.yes(&format!("[con_id={aa}] floating toggle"))?;
    require(
        fixture.box_rect(aa)? == original[&aa]
            && fixture.workspace(aa)? == original_ws[&aa]
            && fixture.required(cc)?["lumen_pair"]["survivor"] == true,
        "Floating transition lost placement",
    )?;
    fixture.yes(&format!("lumen_pair leave {cc}"))?;

    // Invalid IDs and incompatible clients must reject before either placement changes.
    let fixed = fixture.launch_client("fixed", "fixed.log")?;
    let ff = id(&fixture
        .app("constraint-test")?
        .ok_or("Fixed client missing")?)?;
    let before_a = fixture.box_rect(aa)?;
    let before_f = fixture.box_rect(ff)?;
    fixture.no(&format!("lumen_pair create {aa} {ff}"))?;
    require(
        fixture.box_rect(aa)? == before_a && fixture.box_rect(ff)? == before_f,
        "Fixed-size rejection changed placement",
    )?;
    fixture.no(&format!("lumen_pair create {aa} 999999999"))?;
    require(
        fixture.box_rect(aa)? == before_a,
        "Stale ID rejection changed source",
    )?;
    fixture.close_client(fixed)?;

    let bounded = fixture.launch_client("bounded", "bounded.log")?;
    let bound = id(&fixture
        .app("constraint-test")?
        .ok_or("Bounded client missing")?)?;
    let bound_ws = fixture.workspace(bound)?;
    fixture.yes(&format!("lumen_pair create {aa} {bound}"))?;
    require(
        i(&fixture.box_rect(bound)?, "width")? <= 504,
        "Bounded partner exceeded width limit",
    )?;
    fixture.yes(&format!("lumen_pair ratio {aa} 0.1"))?;
    require(
        i(&fixture.box_rect(bound)?, "width")? <= 504 && fixture.ratio(aa)? > 0.5,
        "Command ratio bypassed client bounds",
    )?;
    thread::sleep(Duration::from_millis(350));
    fixture.begin_divider(aa)?;
    fixture.motion(-1000.0, 0.0)?;
    fixture.button("r")?;
    require(
        i(&fixture.box_rect(bound)?, "width")? <= 504,
        "Physical divider bypassed client bounds",
    )?;
    fixture.yes(&format!("lumen_pair leave {bound}"))?;
    require(
        fixture.workspace(bound)? == bound_ws
            && fixture.required(bound)?["lumen_pair"]["id"].is_null()
            && fixture.required(aa)?["fullscreen_mode"] == 1,
        "Bounded partner leave lost restoration",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    fixture.close_client(bounded)?;

    // A protocol-owned transient stays visible and never becomes a primary pane.
    let parent_client = fixture.launch_client("free", "transient-parent.log")?;
    let mut parent_id = id(&fixture
        .app("constraint-test")?
        .ok_or("Transient parent missing")?)?;
    fixture.client(parent_client, "f")?;
    let mut dialog = wait("protocol transient", || fixture.app("constraint-dialog"))?;
    let mut dialog_id = id(&dialog)?;
    let before_a = fixture.box_rect(aa)?;
    require(
        dialog["lumen_pair"]["eligible"] == false,
        "Protocol dialog was eligible for pairing",
    )?;
    fixture.no(&format!("lumen_pair create {aa} {dialog_id}"))?;
    require(
        fixture.box_rect(aa)? == before_a,
        "Dialog rejection moved source",
    )?;
    fixture.no(&format!("lumen_pair create {aa} {parent_id}"))?;
    require(
        fixture.box_rect(aa)? == before_a,
        "Parent with open dialog moved source",
    )?;
    fixture.client(parent_client, "d")?;
    wait("first dialog closure", || {
        Ok(fixture.app("constraint-dialog")?.is_none().then_some(()))
    })?;
    fixture.yes(&format!("lumen_pair create {aa} {parent_id}"))?;
    let pair_space = fixture.workspace(aa)?;
    fixture.client(parent_client, "f")?;
    dialog = wait("paired protocol transient", || {
        fixture.app("constraint-dialog")
    })?;
    dialog_id = id(&dialog)?;
    require(
        fixture.workspace(dialog_id)? == pair_space
            && dialog["visible"] == true
            && dialog["focused"] == true,
        "Paired transient was not visible and focused",
    )?;
    require(
        fixture.required_parent(aa)?["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.len() == 2),
        "Transient became a third pair member",
    )?;
    fixture.screenshot("transient-dialog.png", None)?;

    fixture.yes(&format!("lumen_pair leave {parent_id}"))?;
    require(
        fixture.workspace(dialog_id)? == fixture.workspace(parent_id)?
            && fixture.required(dialog_id)?["visible"] == true,
        "Transient did not follow owner leave",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    fixture.yes(&format!("lumen_pair create {aa} {parent_id}"))?;
    require(
        fixture.workspace(dialog_id)? == fixture.workspace(parent_id)?,
        "Transient did not follow paired owner",
    )?;
    fixture.yes(&format!("lumen_pair replace {aa} {cc}"))?;
    require(
        fixture.workspace(dialog_id)? == fixture.workspace(parent_id)?
            && fixture.workspace(dialog_id)? != fixture.workspace(aa)?,
        "Transient did not follow outgoing partner",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    fixture.yes(&format!("lumen_pair leave {cc}"))?;
    fixture.yes(&format!("lumen_pair create {aa} {parent_id}"))?;
    fixture.yes(&format!(
        "[con_id={parent_id}] move container to workspace 7"
    ))?;
    require(
        fixture.workspace(dialog_id)? == "7" && fixture.workspace(parent_id)? == "7",
        "Transient did not follow external owner move",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;

    // Removing an explicit parent must clear captured protocol ownership.
    fixture.yes(&format!("lumen_pair create {aa} {parent_id}"))?;
    fixture.client(parent_client, "n")?;
    thread::sleep(Duration::from_millis(150));
    if fixture.query("get_version")?["native_window_relationships"] == true {
        wait("explicit parent removal", || {
            Ok(
                (fixture.workspace(dialog_id)? != fixture.workspace(parent_id)?
                    && fixture.required(dialog_id)?["lumen_pair"]["eligible"] == true)
                    .then_some(()),
            )
        })?;
    } else {
        fixture.yes("output HEADLESS-1 resolution 1280x720")?;
    }
    require(
        fixture.workspace(dialog_id)? != fixture.workspace(parent_id)?
            && fixture.required(dialog_id)?["lumen_pair"]["eligible"] == true,
        "Detached dialog still follows former owner",
    )?;
    fixture.yes(&format!("lumen_pair leave {parent_id}"))?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    fixture.client(parent_client, "d")?;
    wait("detached dialog closure", || {
        Ok(fixture.app("constraint-dialog")?.is_none().then_some(()))
    })?;
    fixture.yes(&format!("lumen_pair create {aa} {parent_id}"))?;
    fixture.client(parent_client, "f")?;
    dialog = wait("restored protocol transient", || {
        fixture.app("constraint-dialog")
    })?;
    dialog_id = id(&dialog)?;
    fixture.client(parent_client, "p")?;
    wait("protocol parent destruction", || {
        Ok(fixture.find(parent_id)?.is_none().then_some(()))
    })?;
    require(
        fixture.workspace(dialog_id)? != fixture.workspace(aa)?
            && fixture.required(aa)?["lumen_pair"]["survivor"] == true,
        "Owner destruction stranded dialog in pair",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    fixture.yes(&format!("workspace {}", fixture.workspace(dialog_id)?))?;
    require(
        fixture.required(dialog_id)?["visible"] == true,
        "Surviving dialog became unreachable",
    )?;
    fixture.client(parent_client, "d")?;
    wait("surviving dialog closure", || {
        Ok(fixture.app("constraint-dialog")?.is_none().then_some(()))
    })?;
    fixture.close_client(parent_client)?;

    let survivor_parent = fixture.launch_client("free", "survivor-dialog-parent.log")?;
    parent_id = id(&fixture
        .app("constraint-test")?
        .ok_or("Survivor parent missing")?)?;
    let parent_ws = fixture.workspace(parent_id)?;
    fixture.yes(&format!("lumen_pair create {aa} {parent_id}"))?;
    fixture.client(survivor_parent, "f")?;
    dialog = wait("survivor parent dialog", || {
        fixture.app("constraint-dialog")
    })?;
    dialog_id = id(&dialog)?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    require(
        fixture.required(parent_id)?["lumen_pair"]["survivor"] == true,
        "Owner did not become survivor",
    )?;
    fixture.client(survivor_parent, "p")?;
    wait("survivor parent destruction", || {
        Ok(fixture.find(parent_id)?.is_none().then_some(()))
    })?;
    require(
        fixture.workspace(dialog_id)? == parent_ws,
        "Survivor dialog did not return to saved workspace",
    )?;
    fixture.yes(&format!("workspace {parent_ws}"))?;
    require(
        fixture.required(dialog_id)?["visible"] == true,
        "Returned survivor dialog is hidden",
    )?;
    fixture.close_client(survivor_parent)?;
    wait("survivor dialog closure", || {
        Ok(fixture.app("constraint-dialog")?.is_none().then_some(()))
    })?;

    let revealed_parent = fixture.launch_client("free", "revealed-parent.log")?;
    parent_id = id(&fixture
        .app("constraint-test")?
        .ok_or("Revealed parent missing")?)?;
    fixture.yes(&format!("lumen_pair create {aa} {parent_id}"))?;
    fixture.yes("lumen_window show-desktop-on")?;
    fixture.close_client(revealed_parent)?;
    wait("hidden parent destruction", || {
        Ok(fixture.find(parent_id)?.is_none().then_some(()))
    })?;
    require(
        fixture.required(aa)?["fullscreen_mode"] == 1
            && fixture.required(aa)?["lumen_pair"]["survivor"] == true,
        "Hidden owner destruction did not preserve survivor",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;

    documented_negatives::relationships(&mut fixture, aa)?;
    fixture.yes(&format!("lumen_pair create {aa} {cc}"))?;
    fixture.yes("output HEADLESS-1 disable")?;
    require(
        fixture.full_parent(aa)?.is_some() && fixture.full_parent(cc)?.is_some(),
        "Output loss broke pair parent",
    )?;
    require(
        i(&fixture.box_rect(aa)?, "x")? >= 1280 && i(&fixture.box_rect(cc)?, "x")? >= 1280,
        "Output loss left pair on disabled output",
    )?;
    fixture.yes(&format!("lumen_pair leave {aa}"))?;
    require(
        fixture.required(cc)?["fullscreen_mode"] == 1,
        "Output loss did not preserve survivor",
    )?;
    fixture.yes(&format!("lumen_pair leave {cc}"))?;
    for cid in [aa, cc] {
        let rect = fixture.box_rect(cid)?;
        require(
            i(&rect, "x")? >= 1280
                && i(&rect, "x")? < 2280
                && i(&rect, "x")? + i(&rect, "width")? <= 2280
                && i(&rect, "y")? >= 0
                && i(&rect, "y")? + i(&rect, "height")? <= 800,
            format!("Restored window is unreachable after output loss: {rect}"),
        )?;
    }
    require(
        fixture.query("get_version")?["native_paired_layouts"] == true,
        "Paired capability disappeared",
    )?;
    fixture.launch_foot("pair-minimize-positive")?;
    let ordinary = id(&fixture
        .app("pair-minimize-positive")?
        .ok_or("Ordinary minimize client missing")?)?;
    fixture.yes(&format!("[con_id={ordinary}] floating enable, resize set 360 px 300 px, move absolute position 1400 px 100 px"))?;
    let ordinary_before = fixture.required(ordinary)?;
    let ordinary_box = fixture.box_rect(ordinary)?;
    let ordinary_workspace = fixture.workspace(ordinary)?;
    require(
        ordinary_before["lumen_pair"]["id"].is_null()
            && ordinary_before["lumen_state"]["minimised"] == false,
        "Positive minimize subject is not an ordinary available window",
    )?;
    fixture.yes(&format!("[con_id={ordinary}] lumen_window minimize"))?;
    let ordinary_minimized = fixture.required(ordinary)?;
    require(
        ordinary_minimized["lumen_state"]["minimised"] == true
            && fixture.workspace(ordinary)? == "__i3_scratch",
        "Ordinary minimize did not actually minimize the window",
    )?;
    fixture.yes(&format!("[con_id={ordinary}] scratchpad show"))?;
    let ordinary_restored = fixture.required(ordinary)?;
    require(
        ordinary_restored["pid"] == ordinary_before["pid"]
            && ordinary_restored["lumen_state"]["minimised"] == false
            && ordinary_restored["lumen_state"]["restore_workspace"].is_null()
            && fixture.box_rect(ordinary)? == ordinary_box
            && fixture.workspace(ordinary)? == ordinary_workspace,
        "Ordinary minimize restore lost identity or original placement",
    )?;
    fs::write(
        x.join("ordinary-minimize-positive.json"),
        serde_json::to_vec_pretty(
            &json!({"id":ordinary,"before":ordinary_before,"minimized":ordinary_minimized,"restored":ordinary_restored}),
        )?,
    )?;
    let log = fs::read_to_string(x.join("sway.log"))?;
    require(
        ![
            "Assertion",
            "Segmentation fault",
            "Expected a fullscreen container",
        ]
        .iter()
        .any(|word| log.contains(word)),
        "Compositor asserted during pair lifecycle",
    )?;
    let checks = [
        "dedicated shared full-output space",
        "physical divider drag",
        "Escape restores ratio",
        "double-click reset",
        "swap exact identities",
        "replace restores old partner",
        "exit leaves full-screen survivor",
        "survivor exit restores original",
        "partner close during divider",
        "external partner removal",
        "fixed size and stale IDs reject",
        "bounded client limits constrain physical and command ratios",
        "group reveal and child activation preserve pair",
        "hidden pair management normalizes group before actions",
        "paired minimize rejects with full native state unchanged while ordinary minimize and restore remain functional",
        "nonfinite and out-of-open-interval ratios reject without native state changes",
        "survivor minimize rejects without native state changes",
        "unrelated no-focus window maps outside the pair without stealing focus",
        "actual reparent updates captured owner and modal-only windows reject primary pairing",
        "full-screen source exits to ordinary saved geometry",
        "hide exits without stranding restore state",
        "new app routes outside pair",
        "protocol transient remains visible and focused",
        "transient follows leave replace external move and owner close",
        "explicit parent removal clears captured dialog owner",
        "output loss preserves reachability",
    ];
    fs::write(
        x.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"ids":ids,"original":original,"checks":checks,"effects":options.effects}),
        )?,
    )?;
    println!("PASS native paired placement, divider, identity, exit and output loss");
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
        eprintln!("Paired layouts fixture failed: {error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_action_requires_exact_native_failure() {
        let body =
            br#"[{"success":false,"parse_error":true,"error":"Window action is unavailable"}]"#;
        let expected = "Window action is unavailable";
        assert!(unavailable_reply(Some(2), body, expected).is_ok());
        assert!(unavailable_reply(Some(0), body, expected).is_err());
        assert!(unavailable_reply(None, body, expected).is_err());
        assert!(unavailable_reply(Some(2), b"[]", expected).is_err());
        assert!(
            unavailable_reply(
                Some(2),
                br#"[{"success":false,"parse_error":true,"error":"Unknown command"}]"#,
                expected
            )
            .is_err()
        );
    }
}
