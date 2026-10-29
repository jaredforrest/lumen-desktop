//! Painted-content audit of owned shell surfaces in a private Wayland session.
use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(message: impl Into<String>) -> Box<dyn Error> {
    message.into().into()
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[derive(Clone)]
struct Options {
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    source: PathBuf,
    theme: String,
    native_frame: String,
    inside: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut output = None;
        let mut launcher = None;
        let mut sway = PathBuf::from("sway");
        let mut source = env::current_dir()?.join("res/lumen-desktop");
        let mut theme = None;
        let mut native_frame = "off".to_owned();
        let mut inside = false;
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Missing output")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Missing launcher")?))
                }
                Some("--sway") => sway = PathBuf::from(args.next().ok_or("Missing sway")?),
                Some("--source") => source = PathBuf::from(args.next().ok_or("Missing source")?),
                Some("--theme") => {
                    theme = Some(
                        args.next()
                            .ok_or("Missing theme")?
                            .to_string_lossy()
                            .into_owned(),
                    )
                }
                Some("--native-frame") => {
                    native_frame = args
                        .next()
                        .ok_or("Missing native frame")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--inside") => inside = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown surface matrix option: {}",
                        arg.to_string_lossy()
                    )));
                }
            }
        }
        let theme = theme.ok_or("--theme required")?;
        require(
            theme == "dark" || theme == "light",
            "Theme must be dark or light",
        )?;
        require(
            native_frame == "on" || native_frame == "off",
            "Native frame must be on or off",
        )?;
        let output = output.ok_or("--output required")?;
        Ok(Self {
            output: if output.is_absolute() {
                output
            } else {
                env::current_dir()?.join(output)
            },
            launcher: launcher.ok_or("--launcher required")?,
            sway,
            source,
            theme,
            native_frame,
            inside,
        })
    }
}
fn command_env(root: &Path) -> BTreeMap<String, String> {
    let mut env = env::vars().collect::<BTreeMap<_, _>>();
    env.insert("HOME".into(), path_string(root));
    env.insert("XDG_RUNTIME_DIR".into(), path_string(root));
    env.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/no-system-bus", root.display()),
    );
    for key in ["CONFIG", "STATE", "CACHE", "DATA"] {
        env.insert(
            format!("XDG_{key}_HOME"),
            path_string(&root.join(key.to_lowercase())),
        );
    }
    for key in [
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        env.remove(key);
    }
    for (key, value) in [
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
    ] {
        env.insert(key.into(), value.into());
    }
    env
}
fn outer(options: &Options) -> Result<()> {
    let root = tempfile::Builder::new()
        .prefix("lumen-surfaces-")
        .tempdir()?;
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))?;
    let mut env = command_env(root.path());
    env.remove("DBUS_SESSION_BUS_ADDRESS");
    let bus_config = root.path().join("bus.conf");
    fs::write(
        &bus_config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            root.path().display()
        ),
    )?;
    let status = Command::new("dbus-run-session")
        .arg(format!("--config-file={}", bus_config.display()))
        .arg("--")
        .arg(env::current_exe()?)
        .arg("--inside")
        .arg("--output")
        .arg(&options.output)
        .arg("--launcher")
        .arg(&options.launcher)
        .arg("--sway")
        .arg(&options.sway)
        .arg("--source")
        .arg(&options.source)
        .arg("--theme")
        .arg(&options.theme)
        .arg("--native-frame")
        .arg(&options.native_frame)
        .env_clear()
        .envs(env)
        .status()?;
    require(
        status.success(),
        format!("Surface matrix private session exited {status}"),
    )
}
use std::os::unix::fs::PermissionsExt;

fn run_output(program: &str, args: &[&str], env: &BTreeMap<String, String>) -> Result<Vec<u8>> {
    let result = Command::new("timeout")
        .arg("10s")
        .arg(program)
        .args(args)
        .env_clear()
        .envs(env)
        .output()?;
    require(
        result.status.success(),
        format!(
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(result.stdout)
}
fn call(program: &str, args: &[&str], env: &BTreeMap<String, String>) -> Result<String> {
    Ok(String::from_utf8(run_output(program, args, env)?)?
        .trim()
        .to_owned())
}
fn wait<T>(label: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => last = error.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(fail(format!("Timed out waiting for {label}: {last}")));
        }
        thread::sleep(Duration::from_millis(100));
    }
}
fn first_matching(root: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(prefix) && !name.ends_with(suffix)
            })
        })
}
struct Children(Vec<Child>);
impl Children {
    fn launch(
        &mut self,
        program: &str,
        args: &[&str],
        env: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<()> {
        let file = File::create(log)?;
        let child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(env)
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
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().ok().flatten().is_none() {
                if Instant::now() >= deadline {
                    unsafe {
                        libc::killpg(child.id() as i32, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for item in WalkDir::new(from).follow_links(false).min_depth(1) {
        let item = item?;
        let dest = to.join(item.path().strip_prefix(from)?);
        if item.file_type().is_dir() {
            fs::create_dir_all(&dest)?;
        } else if item.file_type().is_symlink() {
            symlink(fs::read_link(item.path())?, &dest)?;
        } else {
            fs::copy(item.path(), &dest)?;
        }
    }
    Ok(())
}
fn nodes<'a>(node: &'a Value, output: &mut Vec<&'a Value>) {
    output.push(node);
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                nodes(child, output);
            }
        }
    }
}
struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
impl Image {
    fn read(path: &Path) -> Result<Self> {
        let mut decoder = Decoder::new(BufReader::new(File::open(path)?));
        decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf)?;
        let raw = &buf[..info.buffer_size()];
        let channels = match info.color_type {
            ColorType::Rgb => 3,
            ColorType::Rgba => 4,
            ColorType::Grayscale => 1,
            ColorType::GrayscaleAlpha => 2,
            _ => return Err(fail("Unsupported PNG color")),
        };
        let mut pixels = Vec::with_capacity((info.width * info.height * 3) as usize);
        for pixel in raw.chunks_exact(channels) {
            match channels {
                1 | 2 => pixels.extend_from_slice(&[pixel[0], pixel[0], pixel[0]]),
                _ => pixels.extend_from_slice(&pixel[..3]),
            }
        }
        Ok(Self {
            width: info.width,
            height: info.height,
            pixels,
        })
    }
    fn pixel(&self, x: i64, y: i64) -> [u8; 3] {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return [0; 3];
        }
        let n = (y as usize * self.width as usize + x as usize) * 3;
        [self.pixels[n], self.pixels[n + 1], self.pixels[n + 2]]
    }
    fn crop(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        let w = (x1 - x0).max(0) as u32;
        let h = (y1 - y0).max(0) as u32;
        let mut pixels = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                pixels.extend_from_slice(&self.pixel(x0 + x as i64, y0 + y as i64));
            }
        }
        Self {
            width: w,
            height: h,
            pixels,
        }
    }
    fn save(&self, path: &Path) -> Result<()> {
        let mut enc = Encoder::new(File::create(path)?, self.width, self.height);
        enc.set_color(ColorType::Rgb);
        enc.set_depth(BitDepth::Eight);
        enc.write_header()?.write_image_data(&self.pixels)?;
        Ok(())
    }
    fn stats(&self) -> ([f64; 3], [f64; 3]) {
        let count = (self.width as usize * self.height as usize) as f64;
        if count == 0.0 {
            return ([0.0; 3], [0.0; 3]);
        }
        let mut sums = [0.0; 3];
        let mut squares = [0.0; 3];
        for pixel in self.pixels.chunks_exact(3) {
            for channel in 0..3 {
                let v = pixel[channel] as f64;
                sums[channel] += v;
                squares[channel] += v * v;
            }
        }
        let mut means = [0.0; 3];
        let mut deviation = [0.0; 3];
        for channel in 0..3 {
            means[channel] = sums[channel] / count;
            deviation[channel] = (squares[channel] / count - means[channel] * means[channel])
                .max(0.0)
                .sqrt();
        }
        (means, deviation)
    }
}
fn number(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_f64()
        .map(|v| v as i64)
        .ok_or_else(|| fail(format!("Missing {key} in {value}")))
}
fn useful(mean: &[f64; 3], std: &[f64; 3]) -> bool {
    mean.iter().copied().fold(0.0, f64::max) > 10.0 && std.iter().copied().fold(0.0, f64::max) > 2.0
}
fn screenshot(out: &Path, name: &str, env: &BTreeMap<String, String>) -> Result<Image> {
    let path = out.join(name);
    call("grim", &[&path_string(&path)], env)?;
    Image::read(&path)
}
fn digest_assets(root: &Path) -> Result<String> {
    let mut paths = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|item| item.ok())
        .filter(|item| {
            item.file_type().is_file()
                && matches!(
                    item.path().extension().and_then(|e| e.to_str()),
                    Some("qml" | "svg")
                )
        })
        .map(|item| item.path().to_owned())
        .collect::<Vec<_>>();
    paths.sort();
    let mut digest = Sha256::new();
    for path in paths {
        digest.update(path.strip_prefix(root)?.to_string_lossy().as_bytes());
        digest.update(fs::read(path)?);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn calendar_stub() -> Result<()> {
    let mut line = String::new();
    io::stdin().read_to_string(&mut line)?;
    let request: Value = serde_json::from_str(line.lines().next().unwrap_or("{}"))?;
    let reply = json!({"configured":false,"events":[],"monthEvents":[],"days":[],"total":0,"providers":[],"month":request.get("month"),"date":request.get("selectedDate")});
    println!("{}", reply);
    Ok(())
}
fn inner(options: &Options) -> Result<()> {
    require(
        !options.output.exists(),
        format!("Output already exists: {}", options.output.display()),
    )?;
    fs::create_dir_all(&options.output)?;
    let out = fs::canonicalize(&options.output)?;
    let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
    let printed = run_output(
        &path_string(&options.launcher),
        &["--print-env0"],
        &env::vars().collect(),
    )?;
    let mut env = printed
        .split(|v| *v == 0)
        .filter_map(|v| {
            v.iter().position(|byte| *byte == b'=').map(|at| {
                let (k, rest) = v.split_at(at);
                let v = &rest[1..];
                (
                    String::from_utf8_lossy(k).into_owned(),
                    String::from_utf8_lossy(v).into_owned(),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", "pixman"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("LUMEN_NATIVE_WINDOW_CONTROLS", "1"),
        (
            "LUMEN_UNIFIED_OWNED_HEADERS",
            if options.native_frame == "on" {
                "0"
            } else {
                "1"
            },
        ),
    ] {
        env.insert(key.into(), value.into());
    }
    for key in [
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "DISPLAY",
        "AT_SPI_BUS_ADDRESS",
    ] {
        env.remove(key);
    }
    let source = fs::canonicalize(&options.source)?;
    let shell_source = source.join("shell");
    let palette: Value =
        serde_json::from_slice(&fs::read(source.join("appearance/palette.json"))?)?;
    let appearance = out.join("appearance.json");
    fs::write(
        &appearance,
        serde_json::to_vec(
            &json!({"font":palette["font"],"palette":palette[options.theme.as_str()],"preferences":{"mode":options.theme,"reduceMotion":true}}),
        )?,
    )?;
    env.insert("LUMEN_APPEARANCE_STATE".into(), path_string(&appearance));
    let fakebin = out.join("bin");
    fs::create_dir(&fakebin)?;
    symlink(env::current_exe()?, fakebin.join("lumen-calendar"))?;
    env.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            fakebin.display(),
            env.get("PATH").cloned().unwrap_or_default()
        ),
    );
    let assets = out.join("assets");
    copy_tree(&source, &assets)?;
    let fixture = assets.join("shell");
    let isolate = call(
        "lumen-fixture-providers",
        &["isolate", &path_string(&fixture), &path_string(&out)],
        &env,
    )?;
    let _: Value = serde_json::from_str(&isolate)?;
    let documents = out.join("Documents");
    fs::create_dir(&documents)?;
    fs::write(documents.join("Example.txt"), "Fixture content")?;
    let original = fs::read_to_string(shell_source.join("shell.qml"))?;
    let insert = include_str!("../surface-audit.qml.inc")
        .replace(
            "__DOCS__",
            &serde_json::to_string(&path_string(&documents))?,
        )
        .replace(
            "__EXAMPLE__",
            &serde_json::to_string(&path_string(&documents.join("Example.txt")))?,
        );
    let index = original
        .rfind('}')
        .ok_or("Shell QML root missing closing brace")?;
    fs::write(
        fixture.join("shell.qml"),
        format!("{}{}{}\n", &original[..index], insert, '}'),
    )?;
    let mut config = String::from(
        "output HEADLESS-1 resolution 1280x900\nxwayland disable\ndefault_border none\nfor_window [app_id=\"org.quickshell\"] floating enable\n",
    );
    if options.native_frame == "on" {
        config.push_str("default_border normal 1\ndefault_floating_border normal 1\nfor_window [app_id=\"org.quickshell\"] border normal 1\n");
    }
    let config_path = out.join("sway.conf");
    fs::write(&config_path, config)?;
    let mut children = Children(Vec::new());
    children.launch(
        &path_string(&options.sway),
        &["--unsupported-gpu", "-c", &path_string(&config_path)],
        &env,
        &out.join("sway.log"),
    )?;
    let socket = wait("Sway socket", || {
        Ok(first_matching(&runtime, "sway-ipc", ".lock"))
    })?;
    let wayland = wait("Wayland socket", || {
        Ok(first_matching(&runtime, "wayland-", ".lock"))
    })?;
    env.insert("SWAYSOCK".into(), path_string(&socket));
    env.insert(
        "WAYLAND_DISPLAY".into(),
        wayland.file_name().unwrap().to_string_lossy().into_owned(),
    );
    children.launch(
        "quickshell",
        &["-p", &path_string(&fixture)],
        &env,
        &out.join("shell.log"),
    )?;
    let ipc = |method: &str, arg: Option<&str>| -> Result<String> {
        let mut args = vec![
            "-p",
            fixture.to_str().ok_or("Non UTF-8 fixture")?,
            "ipc",
            "call",
            "surfaceAudit",
            method,
        ];
        if let Some(arg) = arg {
            args.push(arg);
        }
        call("quickshell", &args, &env)
    };
    wait("surface audit IPC", || {
        Ok((ipc("ready", None)? == "true").then_some(()))
    })?;
    let mut results = Vec::new();
    for (name, title) in [
        ("help", "Desktop Help"),
        ("session", "Session Options"),
        ("remote", "Remote Desktop"),
        ("overview", "Window Overview"),
        ("text", "Text Tools"),
        ("info", "File Info"),
        ("quick", "Quick Look"),
    ] {
        ipc("open", Some(name))?;
        results.push(wait(&format!("painted {name}"),||{
            let tree:Value=serde_json::from_str(&call("swaymsg",&["-r","-t","get_tree"],&env)?)?;
            let mut all=Vec::new();nodes(&tree,&mut all);
            let window=match all.into_iter().find(|n|n["app_id"]=="org.quickshell" && n["name"]==title && n["visible"]==true){Some(v)=>v,None=>return Ok(None)};
            if options.native_frame=="on" && window["deco_rect"]["height"].as_i64().unwrap_or(0)<=0 {
                let response:Value=serde_json::from_str(&call("swaymsg",&[&format!("[con_id={}]",window["id"]),"border","normal","1"],&env)?)?;
                require(response.as_array().is_some_and(|v|v.iter().all(|row|row["success"]==true)),format!("Sway border failed: {response}"))?;
                return Ok(None);
            }
            let image=screenshot(&out,&format!("{name}.png"),&env)?;
            let rect=&window["rect"];
            let crop=image.crop(number(rect,"x")?+15,number(rect,"y")?+55,number(rect,"x")?+number(rect,"width")?-15,number(rect,"y")?+number(rect,"height")?-15);
            let (mean,deviation)=crop.stats();if !useful(&mean,&deviation){return Ok(None)};
            crop.save(&out.join(format!("{name}-content.png")))?;
            Ok(Some(json!({"surface":name,"mean":mean,"deviation":deviation,"rect":rect,"decoration":window["deco_rect"]})))
        })?);
    }
    for name in [
        "Desktop",
        "Application",
        "Controls",
        "Wi-Fi",
        "Bluetooth",
        "VPN",
        "Sound",
        "Focus",
        "Date",
        "Now Playing",
        "Launcher",
    ] {
        ipc("open", Some(name))?;
        results.push(wait(&format!("painted popup {name}"),||{
            let data:Value=serde_json::from_str(&call("quickshell",&["-p",fixture.to_str().unwrap(),"ipc","call","desktop","layout"],&env)?)?;
            let panel=match data.as_array().and_then(|rows|rows.iter().find(|row|row["menu"]==name && row["panelWidth"].as_f64().unwrap_or(0.0)>0.0)){Some(v)=>v,None=>return Ok(None)};
            let shot=format!("popup-{}.png",name.replace(' ',"-"));let image=screenshot(&out,&shot,&env)?;
            let x=number(panel,"x")?;let y=number(panel,"y")?;let w=number(panel,"panelWidth")?;let h=number(panel,"panelHeight")?;
            let crop=image.crop(x+10,y+10,(x+w-10).min(image.width as i64),(y+h-10).min(image.height as i64));
            let (mean,deviation)=crop.stats();if !useful(&mean,&deviation){return Ok(None)};
            let header=if ["Controls","Date","Now Playing","VPN"].contains(&name) {
                let pixel=image.pixel(x+w/2,y+5);if pixel.iter().max().copied().unwrap_or(0)<=10 {return Ok(None)};Some(pixel)
            } else {None};
            Ok(Some(json!({"surface":name,"mean":mean,"deviation":deviation,"layout":panel,"headerMaterialPixel":header})))
        })?);
    }
    for name in [
        "forcequit",
        "folder",
        "switcher",
        "layout",
        "hud",
        "banner",
        "focus-editor",
    ] {
        ipc("open", Some(name))?;
        results.push(wait(&format!("painted overlay {name}"), || {
            let mut rect = if name == "hud" {
                json!({"x":545,"y":634,"width":190,"height":146})
            } else if name == "banner" {
                json!({"x":916,"y":35,"width":350,"height":100})
            } else {
                serde_json::from_str::<Value>(&ipc("geometry", Some(name))?)?
            };
            if rect.is_null() {
                return Ok(None);
            }
            if name == "focus-editor" {
                rect["x"] = json!(968 + number(&rect, "x")?);
                rect["y"] = json!(38 + number(&rect, "y")?);
            }
            let image = screenshot(&out, &format!("overlay-{name}.png"), &env)?;
            let x = number(&rect, "x")?;
            let y = number(&rect, "y")?;
            let w = number(&rect, "width")?;
            let h = number(&rect, "height")?;
            let crop = image.crop(x + 12, y + 12, x + w - 12, y + h - 12);
            let (mean, deviation) = crop.stats();
            if !useful(&mean, &deviation) {
                return Ok(None);
            };
            crop.save(&out.join(format!("overlay-{name}-content.png")))?;
            Ok(Some(
                json!({"surface":name,"mean":mean,"deviation":deviation,"rect":rect}),
            ))
        })?);
    }
    let log = fs::read_to_string(out.join("shell.log"))?;
    for marker in [
        "WARN scene:",
        "ReferenceError:",
        "TypeError:",
        "Failed to load configuration",
        "Binding loop detected",
        "Cannot assign",
        "Cannot open:",
        "Error decoding:",
        "QML Image:",
        "Could not load icon",
    ] {
        require(
            !log.contains(marker),
            format!("Shell log contains {marker}: {log}"),
        )?;
    }
    let digest = digest_assets(&assets)?;
    let result = json!({"passed":true,"theme":options.theme,"renderer":"software","nativeFrame":options.native_frame,"sourceDigest":digest,"surfaces":results});
    fs::write(
        out.join("result.json"),
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )?;
    println!(
        "PASS surface matrix {} {}",
        options.theme, options.native_frame
    );
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        if env::args_os()
            .next()
            .and_then(|p| PathBuf::from(p).file_name().map(|v| v.to_owned()))
            .is_some_and(|n| n == "lumen-calendar")
        {
            return calendar_stub();
        }
        let options = Options::parse()?;
        if options.inside {
            inner(&options)
        } else {
            outer(&options)
        }
    })();
    if let Err(error) = result {
        eprintln!("surface matrix: {error}");
        std::process::exit(1)
    }
}
