//! Real Input Settings pane against a private Sway IPC provider and Nushell adapter.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}

fn defaults() -> Vec<Value> {
    vec![
        json!({"identifier":"1:1:Fixture_Keyboard","name":"Fixture Keyboard","type":"keyboard","repeat_rate":25,"repeat_delay":600}),
        json!({"identifier":"2:2:Fixture_Mouse","name":"Fixture Mouse","type":"pointer","scroll_factor":1.0,"libinput":{"accel_speed":0.0,"natural_scroll":"disabled","left_handed":"disabled"}}),
        json!({"identifier":"3:3:Fixture_Trackpad","name":"Fixture Trackpad","type":"touchpad","scroll_factor":1.0,"libinput":{"accel_speed":0.2,"natural_scroll":"enabled","left_handed":"disabled","tap":"disabled","dwt":"enabled"}}),
    ]
}

struct Provider {
    devices: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    listener: Option<thread::JoinHandle<()>>,
}
impl Provider {
    fn start(path: &Path) -> Result<Self> {
        let socket = UnixListener::bind(path)?;
        socket.set_nonblocking(true)?;
        let devices = Arc::new(Mutex::new(defaults()));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::clone(&devices);
        let closing = Arc::clone(&stop);
        let listener = thread::spawn(move || {
            while !closing.load(Ordering::Relaxed) {
                match socket.accept() {
                    Ok((connection, _)) => {
                        let state = Arc::clone(&shared);
                        thread::spawn(move || {
                            let _ = serve(connection, state);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            devices,
            stop,
            listener: Some(listener),
        })
    }
    fn snapshot(&self) -> Vec<Value> {
        self.devices.lock().unwrap().clone()
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

fn receive(stream: &mut UnixStream, count: usize) -> std::io::Result<Vec<u8>> {
    let mut data = vec![0; count];
    stream.read_exact(&mut data)?;
    Ok(data)
}
fn packet(stream: &mut UnixStream, kind: u32, value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    stream.write_all(b"i3-ipc")?;
    stream.write_all(&(body.len() as u32).to_le_bytes())?;
    stream.write_all(&kind.to_le_bytes())?;
    stream.write_all(&body)?;
    Ok(())
}
fn serve(mut stream: UnixStream, devices: Arc<Mutex<Vec<Value>>>) -> Result<()> {
    loop {
        let header = match receive(&mut stream, 14) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if &header[..6] != b"i3-ipc" {
            return Err(fail("Invalid private Sway IPC magic"));
        }
        let length = u32::from_le_bytes(header[6..10].try_into()?) as usize;
        if length > 65536 {
            return Err(fail("Oversized private Sway IPC payload"));
        }
        let kind = u32::from_le_bytes(header[10..14].try_into()?);
        let body = String::from_utf8(receive(&mut stream, length)?)?;
        let reply = match kind {
            100 => json!(devices.lock().unwrap().clone()),
            2 => {
                if serde_json::from_str::<Value>(&body)? != json!(["input"]) {
                    return Err(fail("Unexpected Sway event subscription"));
                }
                json!({"success":true})
            }
            0 => {
                let tokens = shlex::split(&body).ok_or("Invalid input command quoting")?;
                if tokens.len() != 4
                    || tokens[0] != "input"
                    || ["*", "type:keyboard", "type:pointer", "type:touchpad"]
                        .contains(&tokens[1].as_str())
                {
                    return Err(fail("Unsafe private input command"));
                }
                let mut state = devices.lock().unwrap();
                for device in state
                    .iter_mut()
                    .filter(|item| item["identifier"] == tokens[1])
                {
                    let key = tokens[2].as_str();
                    let value = &tokens[3];
                    if ["repeat_rate", "repeat_delay"].contains(&key) {
                        device[key] = json!(value.parse::<i64>()?);
                    } else if key == "scroll_factor" {
                        device[key] = json!(value.parse::<f64>()?);
                    } else if key == "pointer_accel" {
                        device["libinput"]["accel_speed"] = json!(value.parse::<f64>()?);
                    } else {
                        device["libinput"][key] = json!(value);
                    }
                }
                json!([{"success":true}])
            }
            _ => return Err(fail("Unsupported private Sway IPC request")),
        };
        packet(&mut stream, kind, &reply)?;
    }
}

fn parse_args() -> Result<(PathBuf, PathBuf)> {
    let mut launcher = None;
    let mut assets = None;
    let mut args = env::args_os().skip(1);
    while let Some(key) = args.next() {
        let value = PathBuf::from(args.next().ok_or("Missing fixture option value")?);
        match key.to_str() {
            Some("--launcher") => launcher = Some(value),
            Some("--assets") => assets = Some(value),
            _ => return Err(fail("Unknown input pane option")),
        }
    }
    Ok((
        launcher.ok_or("Missing launcher")?,
        assets.ok_or("Missing assets")?,
    ))
}
fn launcher_environment(launcher: &Path, root: &Path) -> Result<BTreeMap<String, String>> {
    let mut setup = env::vars().collect::<BTreeMap<_, _>>();
    setup.insert("HOME".into(), root.display().to_string());
    setup.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    for name in ["CONFIG", "STATE", "CACHE", "DATA"] {
        setup.insert(
            format!("XDG_{name}_HOME"),
            root.join(name.to_lowercase()).display().to_string(),
        );
    }
    for name in [
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        setup.remove(name);
    }
    let output = Command::new(launcher)
        .arg("--print-env0")
        .env_clear()
        .envs(&setup)
        .output()?;
    if !output.status.success() {
        return Err(fail(
            "Installed shell launcher did not export fixture environment",
        ));
    }
    for entry in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let Some(split) = entry.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        setup.insert(
            String::from_utf8(entry[..split].to_vec())?,
            String::from_utf8(entry[split + 1..].to_vec())?,
        );
    }
    Ok(setup)
}

fn main_inner() -> Result<()> {
    let (launcher, assets) = parse_args()?;
    let assets = fs::canonicalize(assets)?;
    let private = tempfile::tempdir()?;
    let root = private.path();
    let mut envs = launcher_environment(&launcher, root)?;
    for (name, value) in [
        ("QT_QPA_PLATFORM", "offscreen"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        envs.insert(name.into(), value.into());
    }
    let settings = root.join("settings.json");
    fs::write(&settings, "{\"inputs\":{}}")?;
    let binary = root.join("bin");
    fs::create_dir(&binary)?;
    let adapter = binary.join("lumen-nix-settings");
    fs::write(
        &adapter,
        format!(
            "#!{}\n{}",
            env!("LUMEN_NU"),
            fs::read_to_string(env!("LUMEN_SETTINGS_ADAPTER"))?
        ),
    )?;
    fs::set_permissions(&adapter, fs::Permissions::from_mode(0o755))?;
    envs.insert(
        "LUMEN_NIX_SETTINGS_ACTIVE".into(),
        settings.display().to_string(),
    );
    envs.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            binary.display(),
            envs.get("PATH").cloned().unwrap_or_default()
        ),
    );
    let server = Provider::start(&root.join("input.sock"))?;
    envs.insert(
        "SWAYSOCK".into(),
        root.join("input.sock").display().to_string(),
    );
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = root.join("appearance.json");
    fs::write(
        &appearance,
        serde_json::to_string(
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{"textScale":1.5}}),
        )?,
    )?;
    envs.insert(
        "LUMEN_APPEARANCE_STATE".into(),
        appearance.display().to_string(),
    );
    let fixture = root.join("shell.qml");
    fs::write(
        &fixture,
        include_str!("../fixture.qml").replace(
            "@SHELL_URI@",
            &format!("file://{}", assets.join("shell").display()),
        ),
    )?;
    let log = root.join("quickshell.log");
    let output = File::create(&log)?;
    let mut child = Command::new("quickshell")
        .args(["-p", fixture.to_str().ok_or("Invalid fixture path")?])
        .env_clear()
        .envs(&envs)
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output))
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err(fail("Input Settings QML timed out"));
        }
        thread::sleep(Duration::from_millis(20));
    };
    let text = fs::read_to_string(&log)?;
    if !status.success()
        || !text.contains("INPUT_SETTINGS_PASS")
        || [
            "INPUT_SETTINGS_FAIL",
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Failed to load configuration",
            "Cannot assign",
        ]
        .iter()
        .any(|marker| text.contains(marker))
    {
        return Err(fail(format!("Input Settings pane failed: {text}")));
    }
    let state = server.snapshot();
    if state[1]["libinput"]["natural_scroll"] != "disabled"
        || state[2]["libinput"]["tap"] != "enabled"
    {
        return Err(fail("Input controls were not applied and reversed"));
    }
    let saved: Value = serde_json::from_slice(&fs::read(settings)?)?;
    let entries = saved["inputs"].as_object().ok_or("Missing saved inputs")?;
    if entries.len() != 1
        || entries.values().next().ok_or("Missing touchpad save")?["identity"]["type"] != "touchpad"
    {
        return Err(fail("Saved input profile did not retain only the touchpad"));
    }
    println!(
        "PASS actual input pane selected-device scope, supported controls, typed request transport and reversal"
    );
    Ok(())
}
fn main() {
    if let Err(error) = main_inner() {
        eprintln!("input pane: {error}");
        std::process::exit(1)
    }
}
