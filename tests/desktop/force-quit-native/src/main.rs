//! Light/dark Force Quit surface and physical Escape acceptance on private Wayland.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}

struct Options {
    sway: PathBuf,
    wtype: PathBuf,
    source: PathBuf,
    output: PathBuf,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut sway = None;
    let mut wtype = None;
    let mut source = None;
    let mut output = None;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway path")?)),
            Some("--wtype") => {
                wtype = Some(PathBuf::from(args.next().ok_or("Missing keyboard path")?))
            }
            Some("--shell-source") => {
                source = Some(PathBuf::from(args.next().ok_or("Missing shell source")?))
            }
            Some("--output") => {
                output = Some(PathBuf::from(args.next().ok_or("Missing output path")?))
            }
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown Force Quit fixture option")),
        }
    }
    Ok(Options {
        sway: sway.ok_or("Missing Sway")?,
        wtype: wtype.ok_or("Missing keyboard")?,
        source: source.ok_or("Missing shell source")?,
        output: output.ok_or("Missing output")?,
        inside,
    })
}

fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(Some(value)) = probe() {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Private Force Quit {label} timed out")));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn first_match(folder: &Path, prefix: &str, suffix: &str) -> Result<Option<PathBuf>> {
    Ok(fs::read_dir(folder)?
        .filter_map(|row| row.ok().map(|item| item.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && name.ends_with(suffix)
            })
        }))
}

fn spawn(
    program: &Path,
    args: &[&str],
    envs: &BTreeMap<String, String>,
    log: &Path,
) -> Result<Child> {
    let output = File::create(log)?;
    Ok(Command::new(program)
        .args(args)
        .env_clear()
        .envs(envs)
        .process_group(0)
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output))
        .spawn()?)
}

fn call(program: &Path, args: &[&str], envs: &BTreeMap<String, String>) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .env_clear()
        .envs(envs)
        .output()?;
    if !output.status.success() {
        return Err(fail(format!(
            "{} failed: {}",
            program.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn status(fixture: &Path, envs: &BTreeMap<String, String>) -> Result<Value> {
    Ok(serde_json::from_str(&call(
        Path::new("quickshell"),
        &[
            "-p",
            fixture.to_str().ok_or("Invalid fixture path")?,
            "ipc",
            "call",
            "fixture",
            "status",
        ],
        envs,
    )?)?)
}

fn stop(children: &mut [Child]) {
    for child in children.iter_mut().rev() {
        if child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGTERM);
            }
        }
    }
    for child in children.iter_mut() {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

fn inside(options: Options) -> Result<()> {
    let source = fs::canonicalize(options.source)?;
    if !source.join("ForceQuitDialog.qml").is_file() {
        return Err(fail("Production Force Quit source required"));
    }
    fs::create_dir(&options.output)?;
    let root = fs::canonicalize(options.output)?;
    let runtime = tempfile::tempdir()?;
    let mut envs = env::vars().collect::<BTreeMap<_, _>>();
    for name in [
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DISPLAY",
        "AT_SPI_BUS_ADDRESS",
    ] {
        envs.remove(name);
    }
    for (name, value) in [
        ("HOME", root.join("home").display().to_string()),
        ("XDG_RUNTIME_DIR", runtime.path().display().to_string()),
        ("WLR_BACKENDS", "headless".into()),
        ("WLR_RENDERER", "pixman".into()),
        ("WLR_LIBINPUT_NO_DEVICES", "1".into()),
        ("QT_QPA_PLATFORM", "wayland".into()),
        ("QT_QUICK_BACKEND", "software".into()),
        ("QT_QPA_PLATFORMTHEME", String::new()),
        ("QT_ACCESSIBILITY", "0".into()),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0".into()),
        ("NO_AT_BRIDGE", "1".into()),
        ("GTK_A11Y", "none".into()),
        (
            "LUMEN_FORCE_QUIT_CALLS",
            root.join("requests.jsonl").display().to_string(),
        ),
    ] {
        envs.insert(name.into(), value);
    }
    for name in ["CONFIG", "CACHE", "STATE", "DATA"] {
        let path = root.join(name.to_lowercase());
        fs::create_dir(&path)?;
        envs.insert(format!("XDG_{name}_HOME"), path.display().to_string());
    }
    fs::create_dir(root.join("home"))?;
    let executable = env::current_exe()?;
    let peer = executable.with_file_name("lumen-application-control");
    if !peer.is_file() {
        return Err(fail("Pinned Rust Force Quit peer missing"));
    }
    envs.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            peer.parent().ok_or("Missing peer directory")?.display(),
            envs.get("PATH").cloned().unwrap_or_default()
        ),
    );
    let fixture_dir = root.join("fixture");
    fs::create_dir(&fixture_dir)?;
    let fixture = fixture_dir.join("shell.qml");
    fs::write(
        &fixture,
        include_str!("../fixture.qml")
            .replace("@SHELL_URI@", &format!("file://{}", source.display())),
    )?;
    let sway_config = root.join("sway.conf");
    fs::write(
        &sway_config,
        "output HEADLESS-1 resolution 900x650\nseat seat0 fallback true\n",
    )?;
    let mut children = Vec::<Child>::new();
    let result = (|| {
        children.push(spawn(
            &options.sway,
            &[
                "--unsupported-gpu",
                "-c",
                sway_config.to_str().ok_or("Invalid Sway config")?,
            ],
            &envs,
            &root.join("sway.log"),
        )?);
        let socket = wait("Sway socket", || {
            first_match(runtime.path(), "sway-ipc.", ".sock")
        })?;
        envs.insert("SWAYSOCK".into(), socket.display().to_string());
        let display = wait("Wayland display", || {
            Ok(fs::read_dir(runtime.path())?
                .filter_map(|row| row.ok().map(|item| item.path()))
                .find(|path| {
                    path.file_name().is_some_and(|name| {
                        let name = name.to_string_lossy();
                        name.starts_with("wayland-") && !name.ends_with(".lock")
                    })
                }))
        })?;
        envs.insert(
            "WAYLAND_DISPLAY".into(),
            display
                .file_name()
                .ok_or("Missing display name")?
                .to_string_lossy()
                .into_owned(),
        );
        children.push(spawn(
            &options.wtype,
            &["-s", "30000"],
            &envs,
            &root.join("keyboard.log"),
        )?);
        thread::sleep(Duration::from_millis(200));
        children.push(spawn(
            Path::new("quickshell"),
            &["-p", fixture.to_str().ok_or("Invalid fixture path")?],
            &envs,
            &root.join("shell.log"),
        )?);
        wait("QML readiness", || {
            Ok(status(&fixture, &envs)
                .ok()
                .and_then(|value| (value["phase"] == "closed").then_some(())))
        })?;
        for theme in ["light", "dark"] {
            call(
                Path::new("quickshell"),
                &[
                    "-p",
                    fixture.to_str().ok_or("Invalid fixture path")?,
                    "ipc",
                    "call",
                    "fixture",
                    "openDialog",
                    if theme == "dark" { "true" } else { "false" },
                ],
                &envs,
            )?;
            wait("confirmation readiness", || {
                Ok(status(&fixture, &envs)
                    .ok()
                    .and_then(|value| (value["phase"] == "ready").then_some(())))
            })?;
            thread::sleep(Duration::from_millis(150));
            call(
                Path::new("grim"),
                &[root
                    .join(format!("{theme}.png"))
                    .to_str()
                    .ok_or("Invalid screenshot path")?],
                &envs,
            )?;
            call(&options.wtype, &["-k", "Escape"], &envs)?;
            wait("Escape cancellation", || {
                Ok(status(&fixture, &envs)
                    .ok()
                    .and_then(|value| (value["visible"] == false).then_some(())))
            })?;
        }
        let log = root.join("requests.jsonl");
        let calls = wait("two cancel requests", || {
            let Ok(text) = fs::read_to_string(&log) else {
                return Ok(None);
            };
            let rows = text
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<serde_json::Result<Vec<_>>>()?;
            Ok((rows.iter().filter(|row| row["action"] == "cancel").count() == 2).then_some(rows))
        })?;
        if calls.iter().any(|row| row["action"] == "confirm") {
            return Err(fail("Force Quit fixture sent a termination request"));
        }
        fs::write(
            root.join("result.json"),
            serde_json::to_string_pretty(
                &json!({"passed":true,"themes":["light","dark"],"escapeCancelled":2,"terminationRequests":0}),
            )?,
        )?;
        println!("PASS private Force Quit light/dark surfaces and Escape cancellation");
        Ok(())
    })();
    stop(&mut children);
    result
}

fn main_inner() -> Result<i32> {
    let options = options()?;
    if options.inside {
        inside(options)?;
        return Ok(0);
    }
    let mut child = Command::new(env!("LUMEN_DBUS_RUN_SESSION"));
    child
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--sway")
        .arg(&options.sway)
        .arg("--wtype")
        .arg(&options.wtype)
        .arg("--shell-source")
        .arg(&options.source)
        .arg("--output")
        .arg(&options.output)
        .arg("--inside");
    Ok(child.status()?.code().unwrap_or(1))
}
fn main() {
    match main_inner() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("Force Quit native fixture: {error}");
            std::process::exit(1)
        }
    }
}
