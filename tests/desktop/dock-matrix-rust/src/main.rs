//! Private native Dock geometry, edge reveal, and input acceptance.
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

struct Options {
    sway: PathBuf,
    pointer: PathBuf,
    assets: PathBuf,
    output: PathBuf,
    inside: bool,
}
fn options() -> Result<Options> {
    let mut sway = None;
    let mut pointer = None;
    let mut assets = None;
    let mut output = None;
    let mut inside = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Missing Sway path")?)),
            Some("--pointer") => {
                pointer = Some(PathBuf::from(args.next().ok_or("Missing pointer path")?))
            }
            Some("--assets") => {
                assets = Some(PathBuf::from(args.next().ok_or("Missing assets path")?))
            }
            Some("--output") => {
                output = Some(PathBuf::from(args.next().ok_or("Missing output path")?))
            }
            Some("--inside") => inside = true,
            _ => return Err(fail("Unknown Dock matrix option")),
        }
    }
    Ok(Options {
        sway: sway.ok_or("--sway required")?,
        pointer: pointer.ok_or("--pointer required")?,
        assets: assets.ok_or("--assets required")?,
        output: output.ok_or("--output required")?,
        inside,
    })
}
fn program(name: &str) -> Result<PathBuf> {
    for folder in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let path = folder.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(fail(format!("{name} missing from pinned PATH")))
}
fn environment(root: &Path) -> BTreeMap<String, String> {
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    for key in [
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DISPLAY",
        "AT_SPI_BUS_ADDRESS",
    ] {
        values.remove(key);
    }
    values.insert("HOME".into(), root.display().to_string());
    values.insert("XDG_RUNTIME_DIR".into(), root.display().to_string());
    for key in ["CONFIG", "CACHE", "STATE", "DATA"] {
        values.insert(
            format!("XDG_{key}_HOME"),
            root.join(key.to_lowercase()).display().to_string(),
        );
    }
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", "pixman"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_QPA_PLATFORMTHEME", ""),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        values.insert(key.into(), value.into());
    }
    values
}
fn outer(options: &Options) -> Result<()> {
    require(!options.output.exists(), "Fresh output directory required")?;
    let assets = fs::canonicalize(&options.assets)?;
    require(assets.is_dir(), "Existing shell assets required")?;
    fs::create_dir_all(&options.output)?;
    let output = fs::canonicalize(&options.output)?;
    let status = Command::new(program("timeout")?)
        .arg("90s")
        .arg(program("dbus-run-session")?)
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--sway")
        .arg(fs::canonicalize(&options.sway)?)
        .arg("--pointer")
        .arg(fs::canonicalize(&options.pointer)?)
        .arg("--assets")
        .arg(assets)
        .arg("--output")
        .arg(output)
        .status()?;
    require(
        status.success(),
        format!("Private Dock matrix exited {status}"),
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
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last_error = None;
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last_error = Some(error.to_string()),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!(
                "Dock fixture timed out waiting for {label}: {}",
                last_error.unwrap_or_default()
            )));
        }
        thread::sleep(Duration::from_millis(50));
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
    Ok(String::from_utf8(output.stdout)?)
}
fn ipc(
    qs: &Path,
    fixture: &Path,
    values: &BTreeMap<String, String>,
    method: &str,
    args: &[&str],
) -> Result<String> {
    let fixture = fixture.to_str().ok_or("Fixture path is not UTF-8")?;
    let mut parts = vec!["-p", fixture, "ipc", "call", "fixture", method];
    parts.extend_from_slice(args);
    call(qs, &parts, values)
}
fn status(qs: &Path, fixture: &Path, values: &BTreeMap<String, String>) -> Result<Value> {
    Ok(serde_json::from_str(
        ipc(qs, fixture, values, "status", &[])?.trim(),
    )?)
}
fn number(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| fail(format!("Dock {key} missing")))
}
fn move_pointer(
    swaymsg: &Path,
    pointer: &mut Child,
    values: &BTreeMap<String, String>,
    x: i32,
    y: i32,
) -> Result<()> {
    let command = format!("seat seat0 cursor set {x} {y}");
    call(swaymsg, &[&command], values)?;
    let input = pointer
        .stdin
        .as_mut()
        .ok_or("Persistent pointer input missing")?;
    input.write_all(b"m 0.5 0.5\n")?;
    input.flush()?;
    thread::sleep(Duration::from_millis(100));
    Ok(())
}
fn inside(options: &Options) -> Result<()> {
    let private = tempfile::Builder::new().prefix("lumen-dock-").tempdir()?;
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o700))?;
    let mut values = environment(private.path());
    require(
        values
            .get("DBUS_SESSION_BUS_ADDRESS")
            .is_some_and(|address| address.starts_with("unix:")),
        "Private session bus required",
    )?;
    let output = &options.output;
    for key in ["config", "cache", "state", "data"] {
        fs::create_dir(private.path().join(key))?;
    }
    let fixture = output.join("dock-matrix.qml");
    let assets = fs::canonicalize(&options.assets)?;
    let url = format!("file://{}", assets.display());
    let qml = include_str!("../fixture.qml");
    require(
        qml.matches("@SHELL_ASSETS@").count() == 1,
        "QML asset import seam changed",
    )?;
    fs::write(&fixture, qml.replace("@SHELL_ASSETS@", &url))?;
    let sway_config = output.join("sway.conf");
    fs::write(
        &sway_config,
        "output HEADLESS-1 resolution 1000x700\nseat seat0 fallback true\n",
    )?;
    let qs = program("quickshell")?;
    let swaymsg = program("swaymsg")?;
    let grim = program("grim")?;
    let mut children = Children(Vec::new());
    children.launch(
        &options.sway,
        &[
            "--unsupported-gpu",
            "-c",
            sway_config.to_str().ok_or("Sway config path invalid")?,
        ],
        &values,
        &output.join("sway.log"),
        false,
    )?;
    let socket = wait("Sway IPC", || {
        first_file(private.path(), "sway-ipc.", ".lock")
    })?;
    values.insert("SWAYSOCK".into(), socket.display().to_string());
    let display = wait("Wayland display", || {
        first_file(private.path(), "wayland-", ".lock")
    })?;
    values.insert(
        "WAYLAND_DISPLAY".into(),
        display
            .file_name()
            .ok_or("Display filename invalid")?
            .to_string_lossy()
            .into_owned(),
    );
    let pointer_index = children.launch(
        &options.pointer,
        &["pointer", "move", "0", "0"],
        &values,
        &output.join("pointer.log"),
        true,
    )?;
    thread::sleep(Duration::from_millis(200));
    children.launch(
        &qs,
        &["-p", fixture.to_str().ok_or("Fixture path invalid")?],
        &values,
        &output.join("shell.log"),
        false,
    )?;
    wait("Dock initial width", || {
        let state = status(&qs, &fixture, &values)?;
        Ok((number(&state, "width")? > 0.0).then_some(()))
    })?;
    let mut geometry = Vec::new();
    for position in ["bottom", "left", "right"] {
        for size in [32, 48, 72] {
            ipc(
                &qs,
                &fixture,
                &values,
                "configure",
                &[position, "false", &size.to_string()],
            )?;
            let state = wait("Dock geometry", || {
                let state = status(&qs, &fixture, &values)?;
                Ok((state["position"] == position
                    && (number(&state, "iconSize")? - f64::from(size)).abs() < 1.0)
                    .then_some(state))
            })?;
            let vertical = state["vertical"]
                .as_bool()
                .ok_or("Dock orientation missing")?;
            require(
                (number(&state, "height")? > number(&state, "width")?) == vertical,
                format!("Dock orientation mismatch: {state}"),
            )?;
            require(
                number(&state, "zone")? >= f64::from(size + 26),
                format!("Dock exclusion zone too small: {state}"),
            )?;
            geometry.push(state);
        }
        ipc(
            &qs,
            &fixture,
            &values,
            "configure",
            &[position, "true", "48"],
        )?;
        move_pointer(&swaymsg, &mut children.0[pointer_index], &values, 500, 350)?;
        let hidden = wait("hidden Dock", || {
            let state = status(&qs, &fixture, &values)?;
            Ok((state["revealed"] == false).then_some(state))
        })?;
        require(
            number(&hidden, "zone")? == 0.0,
            format!("Hidden Dock reserved space: {hidden}"),
        )?;
        let edge = match position {
            "bottom" => (500, 699),
            "left" => (0, 350),
            _ => (999, 350),
        };
        move_pointer(
            &swaymsg,
            &mut children.0[pointer_index],
            &values,
            edge.0,
            edge.1,
        )?;
        wait("revealed Dock", || {
            let state = status(&qs, &fixture, &values)?;
            Ok((state["revealed"] == true).then_some(()))
        })?;
        let interior = match position {
            "bottom" => (500, 663),
            "left" => (37, 350),
            _ => (963, 350),
        };
        move_pointer(
            &swaymsg,
            &mut children.0[pointer_index],
            &values,
            interior.0,
            interior.1,
        )?;
        let shown = wait("hovered Dock", || {
            let state = status(&qs, &fixture, &values)?;
            Ok((number(&state, "hovered")? >= 0.0).then_some(state))
        })?;
        require(
            number(&shown, "activated")? == 0.0,
            format!("Pointer entered an app: {shown}"),
        )?;
        call(
            &grim,
            &[output
                .join(format!("{position}.png"))
                .to_str()
                .ok_or("Image path invalid")?],
            &values,
        )?;
        move_pointer(&swaymsg, &mut children.0[pointer_index], &values, 500, 350)?;
        wait("Dock re-hidden", || {
            let state = status(&qs, &fixture, &values)?;
            Ok((state["revealed"] == false).then_some(()))
        })?;
    }
    fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"geometry":geometry,"positions":["bottom","left","right"],"sizes":[32,48,72],"edgeRevealChecks":3}),
        )?,
    )?;
    println!("PASS Dock edge placement, size, input and automatic hiding");
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let options = options()?;
        if options.inside {
            inside(&options)
        } else {
            outer(&options)
        }
    })();
    if let Err(error) = result {
        eprintln!("Dock matrix fixture failed: {error}");
        std::process::exit(1);
    }
}
