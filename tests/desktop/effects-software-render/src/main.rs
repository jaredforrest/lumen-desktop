//! Prove Scenefx software shaders, framebuffers, and changed compositor pixels.
use image::RgbImage;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Environment = HashMap<OsString, OsString>;

struct Args {
    sway: PathBuf,
    output: PathBuf,
    require_no_drm: bool,
}

fn args() -> Result<Args> {
    let mut sway = None;
    let mut output = None;
    let mut require_no_drm = false;
    let mut values = env::args_os().skip(1);
    while let Some(option) = values.next() {
        match option.to_str() {
            Some("--sway") => sway = Some(PathBuf::from(values.next().ok_or("--sway needs a value")?)),
            Some("--output") => output = Some(PathBuf::from(values.next().ok_or("--output needs a value")?)),
            Some("--require-no-drm") => require_no_drm = true,
            _ => return Err(format!("Unknown option: {}", option.to_string_lossy()).into()),
        }
    }
    Ok(Args {
        sway: sway.ok_or("--sway is required")?,
        output: output.ok_or("--output is required")?,
        require_no_drm,
    })
}

fn no_render_devices() -> Result<bool> {
    let directory = Path::new("/dev/dri");
    if !directory.exists() { return Ok(true); }
    Ok(!fs::read_dir(directory)?.any(|entry| {
        entry.ok().is_some_and(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
    }))
}

fn private_environment(private: &Path) -> Environment {
    let mut env: Environment = env::vars_os().collect();
    for key in ["WAYLAND_DISPLAY", "DISPLAY", "SWAYSOCK", "DBUS_SESSION_BUS_ADDRESS"] {
        env.remove(OsStr::new(key));
    }
    for (key, value) in [
        ("HOME", private.to_path_buf()),
        ("XDG_RUNTIME_DIR", private.to_path_buf()),
    ] {
        env.insert(key.into(), value.into_os_string());
    }
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", "gles2"),
        ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
    ] {
        env.insert(key.into(), value.into());
    }
    env
}

struct Processes(Vec<Child>);

impl Drop for Processes {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                // Each fixture child starts a new process group; descendants must stop too.
                unsafe { libc::killpg(child.id() as i32, libc::SIGTERM); }
                let deadline = Instant::now() + Duration::from_secs(5);
                while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(40));
                }
                if child.try_wait().ok().flatten().is_none() {
                    unsafe { libc::killpg(child.id() as i32, libc::SIGKILL); }
                }
                let _ = child.wait();
            }
        }
    }
}

fn launch(command: &mut Command, output: &Path, log: &str, env: &Environment, processes: &mut Processes) -> Result<()> {
    let file = File::create(output.join(log))?;
    let child = command.env_clear().envs(env.iter()).stdout(file.try_clone()?).stderr(file)
        .process_group(0).spawn()?;
    processes.0.push(child);
    Ok(())
}

fn call(program: &Path, args: &[&str], env: &Environment) -> Result<String> {
    let result = Command::new("timeout").arg("5").arg(program).args(args)
        .env_clear().envs(env.iter()).output()?;
    if !result.status.success() {
        return Err(format!("{} {:?} failed: {}", program.display(), args, String::from_utf8_lossy(&result.stderr)).into());
    }
    Ok(String::from_utf8(result.stdout)?)
}

fn wait<T>(mut check: impl FnMut() -> Result<Option<T>>, output: &Path) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(value) = check()? { return Ok(value); }
        if Instant::now() >= deadline {
            return Err(format!("Software FX fixture timed out; inspect {}", output.display()).into());
        }
        thread::sleep(Duration::from_millis(40));
    }
}

fn socket(private: &Path, prefix: &str) -> Result<Option<PathBuf>> {
    for entry in fs::read_dir(private)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(prefix)
            && (prefix != "wayland-" || !entry.file_name().to_string_lossy().ends_with(".lock")) {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

fn nodes<'a>(node: &'a Value, result: &mut Vec<&'a Value>) {
    result.push(node);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children { nodes(child, result); }
        }
    }
}

fn window(swaymsg: &Path, env: &Environment) -> Result<Option<Value>> {
    let tree: Value = serde_json::from_str(&call(swaymsg, &["-r", "-t", "get_tree"], env)?)?;
    let mut all = Vec::new();
    nodes(&tree, &mut all);
    Ok(all.into_iter().find(|node| node["name"] == "FX Pixel Fixture").cloned())
}

fn ipc(swaymsg: &Path, command: &str, env: &Environment) -> Result<()> {
    let rows: Value = serde_json::from_str(&call(swaymsg, &["-r", command], env)?)?;
    if !rows.as_array().ok_or("IPC did not return rows")?.iter().all(|row| row["success"] == true) {
        return Err(format!("Sway IPC failed: {rows}").into());
    }
    Ok(())
}

fn pixel(image: &RgbImage, x: i64, y: i64) -> Result<[u8; 3]> {
    if x < 0 || y < 0 || x as u32 >= image.width() || y as u32 >= image.height() {
        return Err(format!("Pixel ({x}, {y}) lies outside screenshot").into());
    }
    Ok(image.get_pixel(x as u32, y as u32).0)
}

fn run() -> Result<()> {
    let args = args()?;
    fs::create_dir(&args.output)?;
    if OpenOptions::new().read(true).write(true).open("/dev/udmabuf").is_err() {
        return Err("Software FX requires a readable/writable /dev/udmabuf allocator; see CONTRIBUTING.md".into());
    }
    if args.require_no_drm && !no_render_devices()? {
        return Err("Render devices must be hidden before starting this fixture".into());
    }
    let private = tempfile::Builder::new().prefix("lumen-fx-software-").tempdir()?;
    let mut env = private_environment(private.path());
    env.insert("DBUS_SYSTEM_BUS_ADDRESS".into(), format!("unix:path={}/no-system-bus", private.path().display()).into());
    let mut processes = Processes(Vec::new());
    let bus_config = args.output.join("bus.conf");
    fs::write(&bus_config, format!("<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>", private.path().display()))?;
    let bus_log = File::create(args.output.join("bus.log"))?;
    let bus = Command::new("dbus-daemon")
        .arg("--nofork").arg("--print-address=1").arg(format!("--config-file={}", bus_config.display()))
        .env_clear().envs(env.iter()).stdout(Stdio::piped()).stderr(bus_log).process_group(0).spawn()?;
    processes.0.push(bus);
    let address = {
        let stdout = processes.0.last_mut().ok_or("Missing private bus process")?
            .stdout.take().ok_or("Missing private bus address pipe")?;
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });
        receiver.recv_timeout(Duration::from_secs(5))??
    };
    if !address.starts_with("unix:") { return Err("Private bus did not publish an address".into()); }
    env.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.trim().into());
    let conf = args.output.join("sway.conf");
    let initial = "output HEADLESS-1 resolution 800x600\noutput HEADLESS-1 bg #000000 solid_color\nxwayland disable\ndefault_border none\ncorner_radius 0\nshadows disable\nanimation_duration_ms 0\nfor_window [title=\"^FX Pixel Fixture$\"] floating enable, resize set 400 300, move position 200 150\n";
    fs::write(&conf, initial)?;
    launch(Command::new(&args.sway).args(["--unsupported-gpu", "-d", "-c"]).arg(&conf), &args.output, "sway.log", &env, &mut processes)?;
    let sock = wait(|| socket(private.path(), "sway-ipc"), &args.output)?;
    env.insert("SWAYSOCK".into(), sock.into_os_string());
    let wayland = wait(|| socket(private.path(), "wayland-"), &args.output)?;
    env.insert("WAYLAND_DISPLAY".into(), wayland.file_name().ok_or("Wayland socket has no filename")?.to_os_string());
    let swaymsg = args.sway.parent().ok_or("Sway has no parent directory")?.join("swaymsg");
    launch(Command::new("foot").args(["--title=FX Pixel Fixture", "--override=colors-dark.background=ff0000", "sleep", "600"]), &args.output, "foot.log", &env, &mut processes)?;
    wait(|| window(&swaymsg, &env), &args.output)?;
    ipc(&swaymsg, "seat seat0 cursor set 10 10", &env)?;
    thread::sleep(Duration::from_millis(300));
    let rect = window(&swaymsg, &env)?.ok_or("FX window disappeared")?["rect"].clone();
    let (x, y, width, height) = (
        rect["x"].as_i64().ok_or("No rect x")?, rect["y"].as_i64().ok_or("No rect y")?,
        rect["width"].as_i64().ok_or("No rect width")?, rect["height"].as_i64().ok_or("No rect height")?,
    );
    if width < 350 || height < 250 { return Err(format!("Unexpected FX window rect: {rect}").into()); }
    call(Path::new("grim"), &[args.output.join("square.png").to_str().ok_or("Non-UTF8 output path")?], &env)?;
    fs::write(&conf, initial.replace("corner_radius 0", "corner_radius 32"))?;
    ipc(&swaymsg, "reload", &env)?;
    thread::sleep(Duration::from_millis(300));
    call(Path::new("grim"), &[args.output.join("rounded.png").to_str().ok_or("Non-UTF8 output path")?], &env)?;
    let square = image::open(args.output.join("square.png"))?.to_rgb8();
    let rounded = image::open(args.output.join("rounded.png"))?.to_rgb8();
    let corner = (x + 1, y + height - 2);
    let interior = (x + 80, y + height - 30);
    if pixel(&square, corner.0, corner.1)? != [255, 0, 0]
        || pixel(&rounded, corner.0, corner.1)? != [0, 0, 0]
        || pixel(&square, interior.0, interior.1)? != [255, 0, 0]
        || pixel(&rounded, interior.0, interior.1)? != [255, 0, 0] {
        return Err(format!("Software FX corner/interior pixels differ: corner {corner:?}, interior {interior:?}").into());
    }
    let log = fs::read_to_string(args.output.join("sway.log"))?;
    let proofs = ["Using software rendering", "Creating scenefx FX renderer", "FX RENDERER: Shaders Initialized Successfully", "Created GL FBO for buffer", "Trying udmabuf allocator"];
    for proof in proofs { if !log.contains(proof) { return Err(format!("Missing compositor proof: {proof}").into()); } }
    if !log.contains("GL renderer: llvmpipe") && !log.contains("GL renderer: softpipe") {
        return Err("Compositor did not report a software GL renderer".into());
    }
    let result = json!({"passed": true, "allocator": "udmabuf", "noRenderDevices": no_render_devices()?, "checks": [proofs[0], proofs[1], proofs[2], proofs[3], proofs[4], "Actual corner effect changes corner pixels and preserves interior pixels"]});
    fs::write(args.output.join("result.json"), serde_json::to_string_pretty(&result)?)?;
    println!("{}", args.output.join("result.json").display());
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Software FX fixture failed: {error}");
        std::process::exit(1);
    }
}
