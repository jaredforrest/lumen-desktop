//! Actual broker, native client identity, and production confirmation lifecycle.
use super::*;
use std::os::unix::fs::symlink;
use std::os::unix::process::ExitStatusExt;
use std::ops::{Deref, DerefMut};
use std::io::{BufRead, BufReader, Write};

struct Keyboard { child: Child, input: std::process::ChildStdin, output: BufReader<std::process::ChildStdout>, serial: u32 }
impl Keyboard {
    fn start(tool: &Path, envs: &BTreeMap<String, String>, log: &Path) -> Result<Self> {
        let stderr = File::create(log)?;
        let mut child = Command::new(tool).env_clear().envs(envs).process_group(0)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::from(stderr)).spawn()?;
        let input = child.stdin.take().ok_or("Keyboard stdin")?;
        let mut output = BufReader::new(child.stdout.take().ok_or("Keyboard stdout")?);
        let mut line = String::new(); output.read_line(&mut line)?;
        truth(line.trim() == "ready", "Physical keyboard did not become ready")?;
        Ok(Self { child, input, output, serial: 0 })
    }
    fn event(&mut self, op: char, value: u32, pressed: u32) -> Result<()> {
        self.serial += 1;
        writeln!(self.input, "{op} {} {value} {pressed}", self.serial)?;
        self.input.flush()?;
        let mut ack = String::new(); self.output.read_line(&mut ack)?;
        truth(ack.trim() == format!("ack {}", self.serial), "Physical keyboard acknowledgement mismatch")
    }
    fn special(&mut self, name: &str) -> Result<()> {
        let code = match name { "Escape" => 1, "Tab" => 15, "space" => 57, _ => return Err(fail("Unsupported physical key")) };
        self.event('k', code, 1)?; self.event('k', code, 0)
    }
    fn text(&mut self, value: &str) -> Result<()> {
        for letter in value.chars() {
            let code = if letter == '-' { 12 } else {
                let lower = letter.to_ascii_lowercase();
                [("qwertyuiop", 16), ("asdfghjkl", 30), ("zxcvbnm", 44)].into_iter()
                    .find_map(|(row, start)| row.find(lower).map(|index| start + index as u32)).ok_or("Unsupported physical text")?
            };
            self.event('k', code, 1)?; self.event('k', code, 0)?;
        }
        Ok(())
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        let _ = self.child.kill(); let _ = self.child.wait();
    }
}

struct Managed(Child);
impl Deref for Managed { type Target = Child; fn deref(&self) -> &Child { &self.0 } }
impl DerefMut for Managed { fn deref_mut(&mut self) -> &mut Child { &mut self.0 } }
impl Drop for Managed {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe { libc::kill(-(self.0.id() as i32), libc::SIGCONT); libc::kill(-(self.0.id() as i32), libc::SIGTERM); }
            let _ = self.0.wait();
        }
    }
}

fn s(path: &Path) -> Result<&str> { path.to_str().ok_or_else(|| fail("Invalid fixture path")) }
fn truth(value: bool, label: &str) -> Result<()> { if value { Ok(()) } else { Err(fail(label)) } }
fn ipc(qml: &Path, envs: &BTreeMap<String, String>, method: &str, args: &[&str]) -> Result<String> {
    let mut argv = vec!["-p", s(qml)?, "ipc", "call", "confirmationProof", method];
    argv.extend_from_slice(args);
    call(Path::new("quickshell"), &argv, envs)
}
fn state(qml: &Path, envs: &BTreeMap<String, String>) -> Result<Value> {
    Ok(serde_json::from_str(&ipc(qml, envs, "status", &[])?)?)
}
fn nodes<'a>(node: &'a Value, all: &mut Vec<&'a Value>) {
    if !node["app_id"].is_null() { all.push(node); }
    for kind in ["nodes", "floating_nodes"] {
        if let Some(children) = node[kind].as_array() { for child in children { nodes(child, all); } }
    }
}
fn tree(envs: &BTreeMap<String, String>, sway: &Path) -> Result<Value> {
    let swaymsg = sway.with_file_name("swaymsg");
    Ok(serde_json::from_str(&call(&swaymsg, &["-r", "-t", "get_tree"], envs)?)?)
}
fn mapped(envs: &BTreeMap<String, String>, sway: &Path, pid: u32) -> Result<Option<Value>> {
    let current = tree(envs, sway)?;
    let mut all = Vec::new(); nodes(&current, &mut all);
    Ok(all.into_iter().find(|n| n["pid"] == pid && n["rect"]["width"].as_i64().is_some_and(|w| w > 0)).cloned())
}
fn client_state(path: &Path) -> Result<Value> { Ok(serde_json::from_slice(&fs::read(path)?)?) }
fn open(qml: &Path, envs: &BTreeMap<String, String>, node: &Value) -> Result<()> {
    ipc(qml, envs, "openConfirmation", &[node["app_id"].as_str().ok_or("app id")?, node["name"].as_str().ok_or("title")?])?;
    wait("production confirmation", || {
        let v = state(qml, envs)?;
        Ok((v["phase"] == "ready" && v["focus"] == "Cancel").then_some(()))
    })
}
fn confirm(qml: &Path, envs: &BTreeMap<String, String>, keyboard: &mut Keyboard) -> Result<()> {
    keyboard.special("Tab")?;
    wait("Force Quit keyboard focus", || Ok((state(qml, envs)?["focus"] == "Force Quit").then_some(())))?;
    keyboard.special("space")
}
fn live(path: &Path, child: &mut Child) -> Result<()> {
    truth(child.try_wait()?.is_none() && client_state(path)?["dirty"] == true, "Dirty bystander exited")
}
fn bystander_live(path: &Path, child: &mut Child, node: &Value, envs: &BTreeMap<String, String>, sway: &Path, keyboard: &mut Keyboard) -> Result<()> {
    live(path, child)?;
    let before = client_state(path)?;
    let command = format!("[con_id={}] focus", node["id"].as_i64().ok_or("bystander id")?);
    call(&sway.with_file_name("swaymsg"), &["-r", &command], envs)?;
    keyboard.text("still-dirty-")?;
    wait("bystander physical text and heartbeat", || {
        let value = client_state(path)?;
        Ok((value["ticks"].as_u64().unwrap_or_default() > before["ticks"].as_u64().unwrap_or_default()
            && value["text"].as_str().unwrap_or_default().len() > before["text"].as_str().unwrap_or_default().len()).then_some(()))
    })
}
fn launch_client(root: &Path, envs: &BTreeMap<String, String>, name: &str, app_id: &str, title: &str) -> Result<(Managed, PathBuf)> {
    let executable = env::current_exe()?.with_file_name("lumen-force-quit-client");
    let receipt = root.join(format!("{name}.json"));
    let child = spawn(&executable, &[app_id, title, s(&receipt)?], envs, &root.join(format!("{name}.log")))?;
    Ok((Managed(child), receipt))
}
pub(super) fn run(options: Options) -> Result<()> {
    let broker = fs::canonicalize(options.broker.ok_or("--broker required")?)?;
    let source = fs::canonicalize(options.source)?;
    let root = options.output;
    fs::create_dir(&root)?;
    let runtime = tempfile::tempdir()?;
    let home = root.join("home"); fs::create_dir(&home)?;
    let mut envs = env::vars().collect::<BTreeMap<_, _>>();
    for key in ["WAYLAND_DISPLAY", "SWAYSOCK", "DISPLAY", "AT_SPI_BUS_ADDRESS"] { envs.remove(key); }
    for (key, value) in [
        ("HOME", home.display().to_string()), ("XDG_RUNTIME_DIR", runtime.path().display().to_string()),
        ("WLR_BACKENDS", "headless".into()), ("WLR_RENDERER", if options.effects { "gles2" } else { "pixman" }.into()),
        ("WLR_RENDERER_FORCE_SOFTWARE", if options.effects { "1" } else { "0" }.into()),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1".into()), ("LIBGL_ALWAYS_SOFTWARE", "1".into()),
        ("WLR_LIBINPUT_NO_DEVICES", "1".into()), ("QT_QPA_PLATFORM", "wayland".into()),
        ("QT_QUICK_BACKEND", "software".into()), ("QT_QPA_PLATFORMTHEME", "generic".into()),
        ("GDK_BACKEND", "wayland".into()), ("QT_ACCESSIBILITY", "0".into()),
        ("NO_AT_BRIDGE", "1".into()), ("GTK_A11Y", "none".into()),
    ] { envs.insert(key.into(), value); }
    if options.effects {
        let driver = options.software_driver.as_ref().ok_or("Effects require --software-driver")?;
        envs.insert("LIBGL_DRIVERS_PATH".into(), driver.join("lib/dri").display().to_string());
        envs.insert("__EGL_VENDOR_LIBRARY_FILENAMES".into(), driver.join("share/glvnd/egl_vendor.d/50_mesa.json").display().to_string());
        envs.insert("MESA_LOADER_DRIVER_OVERRIDE".into(), "llvmpipe".into());
    }
    for name in ["CONFIG", "CACHE", "STATE", "DATA"] {
        let dir = root.join(name.to_lowercase()); fs::create_dir(&dir)?;
        envs.insert(format!("XDG_{name}_HOME"), dir.display().to_string());
    }
    let bin = root.join("bin"); fs::create_dir(&bin)?;
    symlink(&broker, bin.join("lumen-application-control"))?;
    envs.insert("PATH".into(), format!("{}:{}", bin.display(), envs.get("PATH").cloned().unwrap_or_default()));
    let qml = root.join("confirmation.qml");
    fs::write(&qml, include_str!("../acceptance.qml").replace("@SHELL_URI@", &format!("file://{}", source.display())))?;
    let config = root.join("sway.conf");
    fs::write(&config, "output HEADLESS-1 resolution 1100x800\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\nfor_window [all] floating enable\n")?;
    let mut children = Vec::<Child>::new();
    let result = (|| -> Result<()> {
        children.push(spawn(&options.sway, &["--unsupported-gpu", "-c", s(&config)?], &envs, &root.join("sway.log"))?);
        let socket = wait("Sway socket", || first_match(runtime.path(), "sway-ipc.", ".sock"))?;
        envs.insert("SWAYSOCK".into(), socket.display().to_string());
        let display = wait("Wayland display", || Ok(fs::read_dir(runtime.path())?.filter_map(|row| row.ok().map(|item| item.path()))
            .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("wayland-") && !n.to_string_lossy().ends_with(".lock")))))?;
        envs.insert("WAYLAND_DISPLAY".into(), display.file_name().ok_or("display name")?.to_string_lossy().into_owned());
        let mut keyboard = Keyboard::start(options.keyboard.as_ref().ok_or("--keyboard required")?, &envs, &root.join("keyboard.log"))?;
        children.push(spawn(Path::new("quickshell"), &["-p", s(&qml)?], &envs, &root.join("shell.log"))?);
        wait("dialog QML", || Ok((state(&qml, &envs).ok().is_some_and(|v| v["phase"] == "closed")).then_some(())))?;
        let (mut bystander, bystander_state) = launch_client(&root, &envs, "bystander", "org.example.DirtyBystander", "Dirty bystander")?;
        let bystander_node = wait("native bystander", || mapped(&envs, &options.sway, bystander.id()))?;
        wait("bystander heartbeat", || Ok(client_state(&bystander_state).ok().filter(|v| v["ticks"].as_u64().is_some_and(|n| n > 1)).map(|_| ())))?;
        let (mut target, target_state) = launch_client(&root, &envs, "target", "org.example.HungDocument", "Hung document")?;
        let target_node = wait("native target", || mapped(&envs, &options.sway, target.id()))?;
        wait("target heartbeat", || Ok(client_state(&target_state).ok().filter(|v| v["ticks"].as_u64().is_some_and(|n| n > 1)).map(|_| ())))?;
        unsafe { libc::kill(target.id() as i32, libc::SIGSTOP); }
        open(&qml, &envs, &target_node)?;
        keyboard.special("Escape")?;
        if let Err(error) = wait("Escape cancellation", || Ok((state(&qml, &envs)?["visible"] == false).then_some(()))) {
            fs::write(root.join("escape-failure-state.json"), serde_json::to_vec_pretty(&state(&qml, &envs)?)?)?;
            fs::write(root.join("escape-failure-tree.json"), serde_json::to_vec_pretty(&tree(&envs, &options.sway)?)?)?;
            call(Path::new("grim"), &[s(&root.join("escape-failure.png"))?], &envs)?;
            return Err(error);
        }
        truth(target.try_wait()?.is_none(), "Cancel terminated target")?;
        bystander_live(&bystander_state, &mut bystander, &bystander_node, &envs, &options.sway, &mut keyboard)?;
        open(&qml, &envs, &target_node)?;
        confirm(&qml, &envs, &mut keyboard)?;
        wait("confirmed target termination", || Ok(target.try_wait()?.and_then(|status| (status.signal() == Some(libc::SIGKILL)).then_some(()))))?;
        wait("target window removal", || Ok(mapped(&envs, &options.sway, target.id())?.is_none().then_some(())))?;
        bystander_live(&bystander_state, &mut bystander, &bystander_node, &envs, &options.sway, &mut keyboard)?;
        let (mut retired, retired_state) = launch_client(&root, &envs, "retired", "org.example.HungDocument", "Hung document")?;
        let retired_node = wait("retired client window", || mapped(&envs, &options.sway, retired.id()))?;
        open(&qml, &envs, &retired_node)?;
        unsafe { libc::kill(retired.id() as i32, libc::SIGKILL); }
        retired.wait()?;
        wait("retired window gone", || Ok(mapped(&envs, &options.sway, retired.id())?.is_none().then_some(())))?;
        let (mut replacement, replacement_state) = launch_client(&root, &envs, "replacement", "org.example.HungDocument", "Hung document")?;
        let replacement_node = wait("same-title replacement", || mapped(&envs, &options.sway, replacement.id()))?;
        truth(replacement_node["id"] != retired_node["id"] && replacement.id() != retired.id(), "Replacement identity reused")?;
        confirm(&qml, &envs, &mut keyboard)?;
        wait("stale confirmation error and Close focus", || {
            let value = state(&qml, &envs)?;
            Ok((value["phase"] == "error" && value["focus"] == "Close").then_some(()))
        })?;
        live(&replacement_state, &mut replacement)?;
        live(&bystander_state, &mut bystander)?;
        keyboard.special("Escape")?;
        wait("stale error dismissed", || Ok((state(&qml, &envs)?["visible"] == false).then_some(())))?;
        bystander_live(&bystander_state, &mut bystander, &bystander_node, &envs, &options.sway, &mut keyboard)?;
        let selection = json!([{"appId":replacement_node["app_id"],"title":replacement_node["name"]},{"appId":bystander_node["app_id"],"title":bystander_node["name"]}]);
        let outcome = Command::new(&broker).args(["--request-id", "mixed", "prepare", &selection.to_string()]).envs(&envs).output()?;
        let response: Value = serde_json::from_slice(&outcome.stdout)?;
        truth(!outcome.status.success() && response["error"].as_str().is_some_and(|v| v.contains("one application")), "Mixed application selection was accepted")?;
        live(&replacement_state, &mut replacement)?;
        bystander_live(&bystander_state, &mut bystander, &bystander_node, &envs, &options.sway, &mut keyboard)?;
        fs::write(root.join("result.json"), serde_json::to_vec_pretty(&json!({"passed":true,"checks":[
            "Escape preserves stopped target", "Tab and Space terminate exact pidfd target", "same-title replacement rejected", "mixed selection refused"],
            "target":target_state,"retired":retired_state,"bystander":bystander_state}))?)?;
        for child in [&mut replacement, &mut bystander] {
            if child.try_wait()?.is_none() { unsafe { libc::kill(-(child.id() as i32), libc::SIGTERM); } }
            child.wait()?;
        }
        println!("PASS production Force Quit broker and confirmation lifecycle");
        Ok(())
    })();
    stop(&mut children);
    result
}
