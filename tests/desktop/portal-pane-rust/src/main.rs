//! Real offscreen permission pane against a private Rust PermissionStore provider.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}

struct Options {
    launcher: PathBuf,
    assets: PathBuf,
    provider: PathBuf,
    inner: bool,
}

fn options() -> Result<Options> {
    let mut launcher = None;
    let mut assets = None;
    let mut provider = None;
    let mut inner = false;
    let mut arguments = env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--inner") => inner = true,
            Some("--launcher") => {
                launcher = Some(PathBuf::from(
                    arguments.next().ok_or("Missing launcher path")?,
                ))
            }
            Some("--assets") => {
                assets = Some(PathBuf::from(
                    arguments.next().ok_or("Missing assets path")?,
                ))
            }
            Some("--provider") => {
                provider = Some(PathBuf::from(
                    arguments.next().ok_or("Missing provider path")?,
                ))
            }
            _ => return Err(fail("Unknown permission pane fixture option")),
        }
    }
    Ok(Options {
        launcher: launcher.ok_or("Missing shell launcher")?,
        assets: assets.ok_or("Missing desktop assets")?,
        provider: provider.ok_or("Missing Rust permission provider")?,
        inner,
    })
}

fn private_environment(root: &Path) -> BTreeMap<String, String> {
    let mut environment = env::vars().collect::<BTreeMap<_, _>>();
    environment.insert("HOME".into(), root.display().to_string());
    environment.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    for name in ["CONFIG", "STATE", "CACHE", "DATA"] {
        environment.insert(
            format!("XDG_{name}_HOME"),
            root.join(name.to_lowercase()).display().to_string(),
        );
    }
    for name in [
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
    ] {
        environment.remove(name);
    }
    environment
}

fn shell_environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    if !output.status.success() {
        return Err(fail("Shell launcher did not export fixture environment"));
    }
    let mut environment = BTreeMap::new();
    for item in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|item| !item.is_empty())
    {
        let split = item
            .iter()
            .position(|byte| *byte == b'=')
            .ok_or("Invalid launcher environment")?;
        environment.insert(
            String::from_utf8(item[..split].to_vec())?,
            String::from_utf8(item[split + 1..].to_vec())?,
        );
    }
    for (name, value) in [
        ("QT_QPA_PLATFORM", "offscreen"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        environment.insert(name.into(), value.into());
    }
    Ok(environment)
}

struct Provider(Child);
impl Drop for Provider {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let _ = self.0.wait();
    }
}

fn wait_for_provider(provider: &mut Child, ready: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        if let Some(status) = provider.try_wait()? {
            return Err(fail(format!(
                "Permission provider exited before readiness: {status}"
            )));
        }
        if Instant::now() >= deadline {
            return Err(fail("Permission provider did not become ready"));
        }
        thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}

fn run_inner(options: &Options) -> Result<()> {
    let private = tempfile::tempdir()?;
    let root = private.path();
    let ready = root.join("ready");
    let environment = shell_environment(&options.launcher)?;
    let provider_log = File::create(root.join("provider.log"))?;
    let mut provider = Provider(
        Command::new(&options.provider)
            .args(["--ready", ready.to_str().ok_or("Invalid ready path")?])
            .env_clear()
            .envs(&environment)
            .stdout(Stdio::from(provider_log.try_clone()?))
            .stderr(Stdio::from(provider_log))
            .spawn()?,
    );
    wait_for_provider(&mut provider.0, &ready)?;

    let assets = fs::canonicalize(&options.assets)?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = root.join("appearance.json");
    fs::write(
        &appearance,
        serde_json::to_vec(
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}),
        )?,
    )?;
    let mut environment = environment;
    environment.insert(
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

    let qml_log = root.join("quickshell.log");
    let output = File::create(&qml_log)?;
    let mut qml = Command::new("quickshell")
        .args(["-p", fixture.to_str().ok_or("Invalid QML path")?])
        .env_clear()
        .envs(&environment)
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output))
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = qml.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            qml.kill()?;
            qml.wait()?;
            return Err(fail(format!(
                "Permission pane timed out: {}",
                fs::read_to_string(&qml_log)?
            )));
        }
        thread::sleep(Duration::from_millis(20));
    };
    let output = fs::read_to_string(&qml_log)?;
    if !status.success()
        || !output.contains("PORTAL_PANE_PASS")
        || [
            "PORTAL_PANE_FAIL",
            "ReferenceError:",
            "TypeError:",
            "Binding loop",
            "Failed to load configuration",
            "Cannot assign",
        ]
        .iter()
        .any(|marker| output.contains(marker))
    {
        return Err(fail(format!("Permission pane failed: {output}")));
    }
    println!(
        "PASS real permission pane confirmation/cancel/stale-target transport on private provider"
    );
    Ok(())
}

fn run_outer(options: &Options) -> Result<()> {
    let private = tempfile::tempdir()?;
    let root = private.path();
    let config = root.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            root.display()
        ),
    )?;
    let mut command = Command::new("dbus-run-session");
    command
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .args(["--inner", "--launcher"])
        .arg(&options.launcher)
        .arg("--assets")
        .arg(&options.assets)
        .arg("--provider")
        .arg(&options.provider)
        .env_clear()
        .envs(private_environment(root));
    let output = command.output()?;
    if !output.status.success() {
        return Err(fail(format!(
            "Private permission pane failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn main() {
    let result = options().and_then(|options| {
        if options.inner {
            run_inner(&options)
        } else {
            run_outer(&options)
        }
    });
    if let Err(error) = result {
        eprintln!("portal pane: {error}");
        std::process::exit(1);
    }
}
