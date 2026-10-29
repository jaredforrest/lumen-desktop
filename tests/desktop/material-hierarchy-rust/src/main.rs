//! Patterned material hierarchy using production consumers in full private isolation.
use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::BufReader;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(s: impl Into<String>) -> Box<dyn Error> {
    s.into().into()
}
fn ensure(b: bool, s: impl Into<String>) -> Result<()> {
    if b { Ok(()) } else { Err(fail(s)) }
}
fn save(p: &Path, v: &Value) -> Result<()> {
    fs::write(p, format!("{}\n", serde_json::to_string_pretty(v)?))?;
    Ok(())
}
fn sha(p: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(p)?)))
}
fn shabytes(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn s(p: &Path) -> Result<&str> {
    p.to_str().ok_or_else(|| fail("Non-UTF8 path"))
}
fn n(v: &Value, k: &str) -> Result<f64> {
    v[k].as_f64()
        .ok_or_else(|| fail(format!("Missing {k}: {v}")))
}
fn round(v: f64) -> i64 {
    v.round_ties_even() as i64
}
fn wait<T>(label: &str, mut f: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match f() {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => {}
            Err(e) => last = e.to_string(),
        }
        if Instant::now() >= end {
            return Err(fail(format!("Timed out: {label}: {last}")));
        }
        thread::sleep(Duration::from_millis(40));
    }
}
fn nodes<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    out.push(v);
    for k in ["nodes", "floating_nodes"] {
        if let Some(rows) = v[k].as_array() {
            for row in rows {
                nodes(row, out)
            }
        }
    }
}
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for item in fs::read_dir(src)? {
        let item = item?;
        let from = item.path();
        let to = dst.join(item.file_name());
        let meta = fs::symlink_metadata(&from)?;
        if meta.is_dir() {
            copy_tree(&from, &to)?
        } else if meta.file_type().is_symlink() {
            symlink(fs::read_link(&from)?, &to)?
        } else if meta.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
fn installed(p: &Path) -> Result<PathBuf> {
    let p = p.canonicalize()?;
    ensure(
        p.starts_with("/nix/store") && p.exists(),
        "Pinned immutable fixture dependency required",
    )?;
    Ok(p)
}
#[derive(Clone)]
struct Opt {
    source: PathBuf,
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    font: PathBuf,
    mesa: PathBuf,
    effects: bool,
    inside: bool,
    revision: String,
}
impl Opt {
    fn parse() -> Result<Self> {
        let mut args = env::args_os().skip(1);
        let (mut source, mut output, mut launcher, mut sway, mut font, mut mesa) =
            (None, None, None, None, None, None);
        let (mut effects, mut inside) = (false, false);
        let mut revision = String::new();
        while let Some(a) = args.next() {
            match a.to_str() {
                Some("--source") => {
                    source = Some(PathBuf::from(args.next().ok_or("Source missing")?))
                }
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("Output missing")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("Launcher missing")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(args.next().ok_or("Sway missing")?)),
                Some("--font-directory") => {
                    font = Some(PathBuf::from(args.next().ok_or("Font missing")?))
                }
                Some("--software-driver") => {
                    mesa = Some(PathBuf::from(args.next().ok_or("Mesa missing")?))
                }
                Some("--source-revision") => {
                    revision = args
                        .next()
                        .ok_or("Revision missing")?
                        .to_string_lossy()
                        .into_owned()
                }
                Some("--effects") => effects = true,
                Some("--inside") => inside = true,
                _ => return Err(fail(format!("Unknown option: {}", a.to_string_lossy()))),
            }
        }
        Ok(Self {
            source: if inside {
                PathBuf::from("/source")
            } else {
                source.unwrap_or(env::current_dir()?).canonicalize()?
            },
            output: output.ok_or("--output required")?,
            launcher: launcher.ok_or("--launcher required")?,
            sway: sway.ok_or("--sway required")?,
            font: font.ok_or("--font-directory required")?,
            mesa: mesa.ok_or("--software-driver required")?,
            effects,
            inside,
            revision,
        })
    }
}
fn outer(mut o: Opt) -> Result<()> {
    let output = if o.output.is_absolute() {
        o.output.clone()
    } else {
        env::current_dir()?.join(&o.output)
    };
    fs::create_dir(&output)?;
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
    o.launcher = installed(&o.launcher)?;
    o.sway = installed(&o.sway)?;
    o.font = installed(&o.font)?;
    o.mesa = installed(&o.mesa)?;
    let git = Command::new("git")
        .args(["-C", s(&o.source)?, "rev-parse", "HEAD"])
        .output()?;
    ensure(git.status.success(), "Git revision unavailable")?;
    let revision = String::from_utf8(git.stdout)?.trim().to_owned();
    let exe = env::current_exe()?;
    let child_exe = if exe.starts_with(&o.source) {
        Path::new("/source").join(exe.strip_prefix(&o.source)?)
    } else {
        exe
    };
    let boundary = env::split_paths(&env::var_os("PATH").ok_or("PATH missing")?)
        .map(|p| p.join("lumen-application-isolation"))
        .find(|p| p.is_file())
        .ok_or("Isolation launcher unavailable")?;
    let mut cmd = Command::new(boundary);
    cmd.args(["run", "--source"])
        .arg(&o.source)
        .arg("--output")
        .arg(&output);
    if o.effects {
        cmd.arg("--software-graphics");
    }
    cmd.arg("--").arg(child_exe).arg("--inside");
    for (k, v) in [
        ("--source", o.source.display().to_string()),
        ("--output", "/artifacts".into()),
        ("--launcher", o.launcher.display().to_string()),
        ("--sway", o.sway.display().to_string()),
        ("--font-directory", o.font.display().to_string()),
        ("--software-driver", o.mesa.display().to_string()),
        ("--source-revision", revision.clone()),
    ] {
        cmd.arg(k).arg(v);
    }
    if o.effects {
        cmd.arg("--effects");
    }
    let status = cmd.status()?;
    let sources = [
        "tests/desktop/material-hierarchy-rust/src/main.rs",
        "tests/desktop/material_fixture/hierarchy.qml",
        "tests/desktop/application-isolation/src/main.rs",
    ];
    let mut hashes = serde_json::Map::new();
    for path in sources {
        hashes.insert(path.into(), json!(sha(&o.source.join(path))?));
    }
    save(
        &output.join("source-provenance.json"),
        &json!({"revision":revision,"exitCode":status.code(),"sources":hashes,"launcher":o.launcher,"compositor":o.sway,"fontDirectory":o.font,"softwareDriver":o.mesa,"effects":o.effects}),
    )?;
    ensure(
        status.success(),
        format!("Material hierarchy fixture failed: {status}"),
    )
}
struct Managed(Child);
impl Drop for Managed {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
            let end = Instant::now() + Duration::from_secs(5);
            while self.0.try_wait().ok().flatten().is_none() {
                if Instant::now() > end {
                    unsafe {
                        libc::killpg(self.0.id() as i32, libc::SIGKILL);
                    }
                    break;
                }
                thread::sleep(Duration::from_millis(30));
            }
            let _ = self.0.wait();
        }
    }
}
struct Raster {
    w: i64,
    h: i64,
    p: Vec<u8>,
}
impl Raster {
    fn read(path: &Path) -> Result<Self> {
        let mut dec = Decoder::new(BufReader::new(File::open(path)?));
        dec.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
        let mut reader = dec.read_info()?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf)?;
        let c = match info.color_type {
            ColorType::Rgb => 3,
            ColorType::Rgba => 4,
            ColorType::Grayscale => 1,
            ColorType::GrayscaleAlpha => 2,
            _ => return Err(fail("Unsupported PNG color")),
        };
        let mut p = Vec::with_capacity((info.width * info.height * 3) as usize);
        for pixel in buf[..info.buffer_size()].chunks_exact(c) {
            if c <= 2 {
                p.extend_from_slice(&[pixel[0], pixel[0], pixel[0]])
            } else {
                p.extend_from_slice(&pixel[..3])
            }
        }
        Ok(Self {
            w: info.width as i64,
            h: info.height as i64,
            p,
        })
    }
    fn pixel(&self, x: i64, y: i64) -> [u8; 3] {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return [0; 3];
        }
        let a = ((y * self.w + x) * 3) as usize;
        [self.p[a], self.p[a + 1], self.p[a + 2]]
    }
    fn crop(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        let (w, h) = ((x1 - x0).max(0), (y1 - y0).max(0));
        let mut p = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                p.extend_from_slice(&self.pixel(x0 + x, y0 + y));
            }
        }
        Self { w, h, p }
    }
    fn save(&self, path: &Path) -> Result<()> {
        let mut enc = Encoder::new(File::create(path)?, self.w as u32, self.h as u32);
        enc.set_color(ColorType::Rgb);
        enc.set_depth(BitDepth::Eight);
        enc.write_header()?.write_image_data(&self.p)?;
        Ok(())
    }
}
fn color(v: &Value) -> Result<[u8; 3]> {
    Ok([
        round(n(v, "r")? * 255.0) as u8,
        round(n(v, "g")? * 255.0) as u8,
        round(n(v, "b")? * 255.0) as u8,
    ])
}
fn linear(c: u8) -> f64 {
    let v = c as f64 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn luminance(c: [u8; 3]) -> f64 {
    linear(c[0]) * 0.2126 + linear(c[1]) * 0.7152 + linear(c[2]) * 0.0722
}
fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}
struct Fixture {
    opt: Opt,
    env: BTreeMap<String, String>,
    processes: Vec<Managed>,
    qml: PathBuf,
    appearance: PathBuf,
    palette: Value,
    results: Vec<Value>,
}
impl Fixture {
    fn new(opt: Opt) -> Result<Self> {
        ensure(
            unsafe { libc::getuid() } != 0 && env::var("HOME")? == "/home/fixture",
            "Private non-root home missing",
        )?;
        ensure(!Path::new("/.old-root").exists(), "Old root exposed")?;
        let homes = fs::read_dir("/home")?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect::<Vec<_>>();
        ensure(
            homes == [PathBuf::from("/home/fixture")],
            "Host home entered fixture",
        )?;
        let out = &opt.output;
        let mut ns = serde_json::Map::new();
        for name in ["user", "pid", "mnt", "net", "ipc", "uts"] {
            ns.insert(
                name.into(),
                json!(
                    fs::read_link(format!("/proc/self/ns/{name}"))?
                        .display()
                        .to_string()
                ),
            );
        }
        save(
            &out.join("namespace-proof.json"),
            &json!({"namespaces":ns,"status":fs::read_to_string("/proc/self/status")?,"mounts":fs::read_to_string("/proc/self/mountinfo")?}),
        )?;
        let mut env = env::vars().collect::<BTreeMap<_, _>>();
        let printed = Command::new(&opt.launcher)
            .arg("--print-env0")
            .env_clear()
            .envs(&env)
            .output()?;
        ensure(printed.status.success(), "Launcher environment unavailable")?;
        for row in printed.stdout.split(|v| *v == 0) {
            if let Some(i) = row.iter().position(|v| *v == b'=') {
                env.insert(
                    String::from_utf8(row[..i].to_vec())?,
                    String::from_utf8(row[i + 1..].to_vec())?,
                );
            }
        }
        for (k, v) in [
            ("WLR_BACKENDS", "headless"),
            ("WLR_HEADLESS_OUTPUTS", "1"),
            ("WLR_RENDERER", if opt.effects { "gles2" } else { "pixman" }),
            (
                "WLR_RENDERER_FORCE_SOFTWARE",
                if opt.effects { "1" } else { "0" },
            ),
            ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_NO_XDG_DESKTOP_PORTAL", "1"),
            ("QT_ACCESSIBILITY", "0"),
            ("NO_AT_BRIDGE", "1"),
            ("GTK_A11Y", "none"),
            (
                "LUMEN_COMPOSITOR_EFFECTS",
                if opt.effects { "1" } else { "0" },
            ),
            ("LUMEN_UNIFIED_OWNED_HEADERS", "1"),
            ("LUMEN_NATIVE_WINDOW_CONTROLS", "0"),
            ("LUMEN_FILE_SERVICE", "0"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
            ("LUMEN_NEARBY_SHARING_LABEL", "Nearby Sharing"),
            ("LUMEN_NEARBY_SHARING_PROVIDER", ""),
            ("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe"),
        ] {
            env.insert(k.into(), v.into());
        }
        for key in [
            "SWAYSOCK",
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "AT_SPI_BUS_ADDRESS",
            "GLOBAL_MENU_PYTHON",
        ] {
            env.remove(key);
        }
        env.insert(
            "LIBGL_DRIVERS_PATH".into(),
            opt.mesa.join("lib/dri").display().to_string(),
        );
        env.insert(
            "__EGL_VENDOR_LIBRARY_FILENAMES".into(),
            opt.mesa
                .join("share/glvnd/egl_vendor.d/50_mesa.json")
                .display()
                .to_string(),
        );
        let font = out.join("fonts.conf");
        fs::write(
            &font,
            format!(
                "<fontconfig><dir>{}</dir><cachedir>/home/fixture/.cache/fontconfig</cachedir></fontconfig>",
                opt.font.display()
            ),
        )?;
        env.insert("FONTCONFIG_FILE".into(), font.display().to_string());
        let pinned = PathBuf::from(
            env.get("LUMEN_DESKTOP_ASSETS")
                .ok_or("Pinned LUMEN_DESKTOP_ASSETS missing")?,
        );
        ensure(
            pinned.starts_with("/nix/store") && pinned.join("shell/FilesState.qml").is_file(),
            "Installed assets unavailable",
        )?;
        let assets = out.join("assets");
        copy_tree(&pinned, &assets)?;
        let mut changes = Vec::new();
        for (relative, old, new) in [
            (
                "shell/BrightnessControl.qml",
                "    id: root",
                "    id: root\n    readonly property alias materialAuditTip:brightnessTip",
            ),
            (
                "shell/BrightnessControl.qml",
                "running:root.visible;repeat:true;triggeredOnStart:true;",
                "running:false;repeat:true;triggeredOnStart:true;",
            ),
        ] {
            let path = assets.join(relative);
            let original = fs::read_to_string(&path)?;
            ensure(
                original.matches(old).count() == 1,
                format!("Isolated QML edit ambiguous: {relative}"),
            )?;
            let modified = original.replacen(old, new, 1);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            fs::write(&path, &modified)?;
            changes.push(json!({"path":relative,"originalSha256":shabytes(original.as_bytes()),"modifiedSha256":shabytes(modified.as_bytes()),"old":old,"new":new}));
        }
        save(&out.join("provider-isolation.json"), &json!(changes))?;
        let documents = Path::new("/home/fixture/Documents");
        fs::create_dir(documents)?;
        fs::write(
            documents.join("Document sample.txt"),
            "Self-authored disposable document.\n",
        )?;
        let qml = out.join("hierarchy.qml");
        let qml_text = fs::read_to_string("/source/tests/desktop/material_fixture/hierarchy.qml")?;
        fs::write(&qml,qml_text.replace("persistViewPreferences:true", "persistViewPreferences:false").replace("ready:service.ready", "filesError:service.error,serviceReady:service.ready,serviceBusy:service.busy,rowCount:service.rows.length,servicePath:service.path,ready:service.ready"))?;
        let palette: Value =
            serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
        let appearance = out.join("appearance.json");
        env.insert(
            "LUMEN_APPEARANCE_STATE".into(),
            appearance.display().to_string(),
        );
        let mut fixture = Self {
            opt,
            env,
            processes: Vec::new(),
            qml,
            appearance,
            palette,
            results: Vec::new(),
        };
        fixture.theme("light")?;
        Ok(fixture)
    }
    fn theme(&mut self, mode: &str) -> Result<()> {
        let value = json!({"font":self.palette["font"],"palette":self.palette[mode],"preferences":{"mode":mode,"reduceMotion":true,"reduceTransparency":false,"dockAutoHide":false}});
        let pending = self.appearance.with_extension("pending");
        save(&pending, &value)?;
        fs::rename(pending, &self.appearance)?;
        Ok(())
    }
    fn cmd(&self, program: &str, args: &[&str]) -> Command {
        let mut c = Command::new(program);
        c.args(args).env_clear().envs(&self.env);
        c
    }
    fn call(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = self.cmd(program, args).output()?;
        ensure(
            output.status.success(),
            format!(
                "{program} {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn launch(&mut self, program: &str, args: &[&str], log: &str) -> Result<usize> {
        let file = File::create(self.opt.output.join(log))?;
        let mut c = self.cmd(program, args);
        c.stdout(file.try_clone()?).stderr(file);
        unsafe {
            c.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        self.processes.push(Managed(c.spawn()?));
        Ok(self.processes.len() - 1)
    }
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut argv = vec![
            "-p",
            s(&self.qml)?,
            "ipc",
            "call",
            "materialHierarchy",
            method,
        ];
        argv.extend(args);
        self.call("quickshell", &argv)
    }
    fn state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("state", &[])?)?)
    }
    fn tree(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call("swaymsg", &["-r", "-t", "get_tree"])?,
        )?)
    }
    fn native(&self, title: &str) -> Result<Option<Value>> {
        let tree = self.tree()?;
        let mut all = Vec::new();
        nodes(&tree, &mut all);
        Ok(all
            .into_iter()
            .find(|v| v["name"] == title && v["app_id"] == "org.quickshell")
            .cloned())
    }
    fn capture(&self, name: &str) -> Result<Raster> {
        let path = self.opt.output.join(name);
        self.call("grim", &[s(&path)?])?;
        Raster::read(&path)
    }
    fn start(&mut self) -> Result<Value> {
        let bus = self.opt.output.join("bus.conf");
        let address = "unix:path=/run/fixture/material-bus";
        fs::write(
            &bus,
            format!(
                "<busconfig><type>session</type><listen>{address}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>"
            ),
        )?;
        self.env.insert(
            "DBUS_SYSTEM_BUS_ADDRESS".into(),
            "unix:path=/run/fixture/no-system-bus".into(),
        );
        self.launch(
            "dbus-daemon",
            &["--nofork", &format!("--config-file={}", bus.display())],
            "bus.log",
        )?;
        wait("private bus", || {
            Ok(Path::new("/run/fixture/material-bus")
                .exists()
                .then_some(()))
        })?;
        self.env
            .insert("DBUS_SESSION_BUS_ADDRESS".into(), address.into());
        let config = self.opt.output.join("sway.conf");
        fs::write(
            &config,
            "output HEADLESS-1 resolution 1600x1000\noutput * bg #29445b solid_color\nxwayland disable\nfor_window [all] floating enable, border pixel 0, move position 70 70\n",
        )?;
        let sway = self.opt.sway.display().to_string();
        self.launch(
            &sway,
            &["-d", "--unsupported-gpu", "-c", s(&config)?],
            "compositor.log",
        )?;
        let runtime = PathBuf::from(self.env.get("XDG_RUNTIME_DIR").ok_or("Runtime missing")?);
        let socket = wait("private compositor", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .find(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("sway-ipc"))
                }))
        })?;
        self.env
            .insert("SWAYSOCK".into(), socket.display().to_string());
        let display = wait("private display", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .find(|n| n.starts_with("wayland-") && !n.ends_with(".lock")))
        })?;
        self.env.insert("WAYLAND_DISPLAY".into(), display);
        let qml = self.qml.display().to_string();
        self.launch("quickshell", &["-p", &qml], "shell.log")?;
        wait("owned material fixture IPC ready", || {
            self.state().map(Some)
        })?;
        let outputs: Value =
            serde_json::from_str(&self.call("swaymsg", &["-r", "-t", "get_outputs"])?)?;
        outputs
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|r| r["active"] == true && r["name"] == "HEADLESS-1")
            })
            .map(|r| r["rect"].clone())
            .ok_or_else(|| fail("Active output missing"))
    }
}
fn origin(rect: &Value, offset: (f64, f64)) -> Value {
    json!({"x":rect["x"].as_f64().unwrap_or(0.0)+offset.0,"y":rect["y"].as_f64().unwrap_or(0.0)+offset.1,"width":rect["width"],"height":rect["height"]})
}
impl Fixture {
    fn paint(
        &self,
        mode: &str,
        phase: &str,
        under: &Raster,
        base_origin: (f64, f64),
        output: &Value,
    ) -> Result<Option<Value>> {
        let observed = self.state()?;
        let image = self.capture(&format!("{mode}-{phase}.png"))?;
        let regions = if phase == "base" {
            observed["regions"]
                .as_array()
                .ok_or("Regions missing")?
                .clone()
        } else {
            vec![observed["overlay"].clone()]
        };
        let mut records = Vec::new();
        let mut overlay_offset = (0.0, 0.0);
        for region in &regions {
            let offset = if phase == "base" {
                base_origin
            } else {
                let window = &region["window"];
                let x = n(output, "x")? + n(output, "width")?
                    - n(window, "width")?
                    - n(window, "right")?;
                let y = n(output, "y")? + n(window, "top")?;
                let offset = if phase == "hud" {
                    (
                        n(output, "x")? + (n(output, "width")? - n(window, "width")?) / 2.0,
                        n(output, "y")? + n(output, "height")?
                            - n(window, "height")?
                            - n(window, "bottom")?,
                    )
                } else {
                    (x, y)
                };
                overlay_offset = offset;
                offset
            };
            let box_ = origin(&region["rect"], offset);
            let (x0, y0, x1, y1) = (
                round(n(&box_, "x")?),
                round(n(&box_, "y")?),
                round(n(&box_, "x")? + n(&box_, "width")?),
                round(n(&box_, "y")? + n(&box_, "height")?),
            );
            let crop = image.crop(x0, y0, x1, y1);
            let role = region["role"].as_str().ok_or("Region role missing")?;
            crop.save(
                &self
                    .opt
                    .output
                    .join(format!("{mode}-{phase}-{role}-crop.png")),
            )?;
            let mut points = Vec::new();
            let base_color = color(&region["color"])?;
            let alpha = n(&region["color"], "a")?;
            for fy in [0.2, 0.4, 0.6, 0.8] {
                for fx in [0.15, 0.3, 0.5, 0.7, 0.85] {
                    let x = round(n(&box_, "x")? + n(&box_, "width")? * fx);
                    let y = round(n(&box_, "y")? + n(&box_, "height")? * fy);
                    let underlying = if phase == "base" && role == "sidebar" {
                        under.pixel(
                            round(n(output, "x")?) + (x - round(n(output, "x")?)).rem_euclid(64),
                            round(n(output, "y")?) + (y - round(n(output, "y")?)).rem_euclid(64),
                        )
                    } else {
                        under.pixel(x, y)
                    };
                    let expected = [0, 1, 2].map(|i| {
                        round(base_color[i] as f64 * alpha + underlying[i] as f64 * (1.0 - alpha))
                            as u8
                    });
                    let actual = image.pixel(x, y);
                    let error = (0..3)
                        .map(|i| (actual[i] as i64 - expected[i] as i64).abs())
                        .max()
                        .unwrap_or(0);
                    points.push(
                        json!({"x":x,"y":y,"actual":actual,"expected":expected,"error":error}),
                    );
                }
            }
            let matches = points
                .iter()
                .filter(|p| p["error"].as_i64().unwrap_or(999) <= 3)
                .collect::<Vec<_>>();
            let foreground = color(&observed["foreground"])?;
            let ink = crop
                .p
                .chunks_exact(3)
                .filter(|p| (0..3).all(|i| (p[i] as i64 - foreground[i] as i64).abs() <= 3))
                .count();
            let measured = matches
                .iter()
                .filter_map(|p| {
                    let a = p["actual"].as_array()?;
                    let c = [
                        a[0].as_u64()? as u8,
                        a[1].as_u64()? as u8,
                        a[2].as_u64()? as u8,
                    ];
                    Some(contrast(foreground, c))
                })
                .fold(f64::INFINITY, f64::min);
            let measured = if measured.is_finite() { measured } else { 0.0 };
            records.push(json!({"role":role,"rect":box_,"matches":matches.len(),"samples":points,"textPixels":ink,"minimumContrast":measured}));
            save(&self.opt.output.join("last-paint.json"), &json!(records))?;
            if matches.len() < 8 || ink < 8 {
                return Ok(None);
            }
            ensure(
                measured >= 4.5,
                format!("Contrast failed: {mode} {phase} {role}: {measured}"),
            )?;
        }
        let mut unchanged = Vec::new();
        if phase != "base" {
            let document = observed["regions"]
                .as_array()
                .and_then(|r| r.iter().find(|r| r["role"] == "document"))
                .ok_or("Document region missing")?;
            let box_ = origin(&document["rect"], base_origin);
            let w = &observed["overlay"]["window"];
            let host = (
                overlay_offset.0,
                overlay_offset.1,
                overlay_offset.0 + n(w, "width")?,
                overlay_offset.1 + n(w, "height")?,
            );
            for fy in [0.25, 0.5, 0.75] {
                for fx in [0.1, 0.3, 0.5, 0.7, 0.9] {
                    let x = round(n(&box_, "x")? + n(&box_, "width")? * fx);
                    let y = round(n(&box_, "y")? + n(&box_, "height")? * fy);
                    if host.0 - 16.0 <= x as f64
                        && x as f64 <= host.2 + 16.0
                        && host.1 - 16.0 <= y as f64
                        && y as f64 <= host.3 + 16.0
                    {
                        continue;
                    }
                    let before = under.pixel(x, y);
                    let content = color(&observed["content"])?;
                    if (0..3)
                        .map(|i| (before[i] as i64 - content[i] as i64).abs())
                        .max()
                        .unwrap_or(0)
                        > 3
                    {
                        continue;
                    }
                    let after = image.pixel(x, y);
                    let error = (0..3)
                        .map(|i| (before[i] as i64 - after[i] as i64).abs())
                        .max()
                        .unwrap_or(0);
                    unchanged
                        .push(json!({"x":x,"y":y,"before":before,"after":after,"error":error}));
                }
            }
            if unchanged.len() < 6
                || unchanged
                    .iter()
                    .any(|r| r["error"].as_i64().unwrap_or(999) > 3)
            {
                save(
                    &self.opt.output.join("last-occlusion.json"),
                    &json!(unchanged),
                )?;
                return Ok(None);
            }
        }
        save(&self.opt.output.join("last-paint.json"), &json!(records))?;
        Ok(Some(
            json!({"state":observed,"regions":records,"unoccludedDocument":unchanged}),
        ))
    }
    fn run(&mut self) -> Result<()> {
        let output = self.start()?;
        for mode in ["light", "dark"] {
            self.theme(mode)?;
            self.ipc("open", &["base"])?;
            let current = wait("actual file document material regions", || {
                let state = self.state()?;
                Ok((state["ready"] == true
                    && state["dark"] == (mode == "dark")
                    && state["regions"].as_array().is_some_and(|v| v.len() == 3))
                .then_some(state))
            });
            let current = match current {
                Ok(v) => v,
                Err(e) => {
                    let observed = self.state().unwrap_or(json!({"unavailable":true}));
                    save(
                        &self.opt.output.join("failed-state.json"),
                        &json!({"state":observed,"tree":self.tree().unwrap_or(Value::Null)}),
                    )?;
                    return Err(e);
                }
            };
            let title = current["filesTitle"]
                .as_str()
                .ok_or("Files title missing")?;
            let window = wait("actual owned Files frame", || self.native(title))?;
            let base_origin = (
                n(&window["rect"], "x")? + n(&window["window_rect"], "x")?,
                n(&window["rect"], "y")? + n(&window["window_rect"], "y")?,
            );
            let baseline = format!("{mode}-document-baseline.png");
            let mut under = self.capture(&baseline)?;
            for phase in ["base", "popover", "menu", "notification", "tooltip", "hud"] {
                self.ipc("open", &[phase])?;
                let current = wait(&format!("production {phase} maps"), || {
                    let state = self.state()?;
                    Ok((state["phase"] == phase
                        && (phase == "base" || !state["overlay"].is_null()))
                    .then_some(state))
                })?;
                let painted = wait(&format!("actual painted hierarchy {mode} {phase}"), || {
                    self.paint(mode, phase, &under, base_origin, &output)
                })?;
                if phase == "base" {
                    under = Raster::read(&self.opt.output.join(format!("{mode}-base.png")))?;
                    under.save(&self.opt.output.join(&baseline))?;
                }
                self.results
                    .push(json!({"mode":mode,"phase":phase,"state":current,"paint":painted}));
            }
            let distinct = current["regions"]
                .as_array()
                .ok_or("Regions absent")?
                .iter()
                .map(|r| color(&r["color"]))
                .collect::<Result<Vec<_>>>()?;
            ensure(
                distinct
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    >= 2,
                "Document and navigation roles collapsed",
            )?;
        }
        if self.opt.effects {
            let log = fs::read_to_string(self.opt.output.join("compositor.log"))?;
            ensure(
                [
                    "Creating scenefx FX renderer",
                    "GL renderer: llvmpipe",
                    "FX RENDERER: Shaders Initialized Successfully",
                    "Trying udmabuf allocator",
                    "Created GL FBO for buffer",
                ]
                .iter()
                .all(|s| log.contains(s)),
                "Effects renderer proof missing",
            )?;
        }
        save(
            &self.opt.output.join("result.json"),
            &json!({"passed":true,"sourceRevision":self.opt.revision,"cases":self.results,"isolation":"Private user/PID/network/mount/IPC/UTS namespaces, detached host root, private buses and document","notClaimed":["Reference-exact material measurements","All application interiors","Tooltip hover interaction","Replacement of existing seven-surface live transparency/input corpus"]}),
        )?;
        Ok(())
    }
}
fn main() -> Result<()> {
    let opt = Opt::parse()?;
    if !opt.inside {
        outer(opt)
    } else {
        let mut f = Fixture::new(opt)?;
        f.run()
    }
}
