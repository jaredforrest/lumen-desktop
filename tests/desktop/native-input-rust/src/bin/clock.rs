//! Physical clock toggle in an isolated full desktop shell.
use lumen_native_input_fixtures::{
    Result, Session, copy_tree, ensure, fail, s, sha_bytes, sha_file, wait,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    env, fs,
    io::{self, BufRead},
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};
use zbus::blocking::{Connection, Proxy};

#[derive(Clone)]
struct Opt {
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    atspi: PathBuf,
    output: PathBuf,
    source: PathBuf,
    egl: Option<PathBuf>,
    effects: bool,
    inside: bool,
    parent_net: Option<String>,
    revision: Option<String>,
}
impl Opt {
    fn parse() -> Result<Self> {
        let mut launcher = None;
        let mut sway = None;
        let mut pointer = None;
        let mut atspi = None;
        let mut output = None;
        let mut source = env::current_dir()?;
        let mut egl = env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").map(PathBuf::from);
        let mut effects = false;
        let mut inside = false;
        let mut parent_net = None;
        let mut revision = None;
        let mut a = env::args_os().skip(1);
        while let Some(x) = a.next() {
            match x.to_str() {
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(a.next().ok_or("launcher value")?))
                }
                Some("--sway") => sway = Some(PathBuf::from(a.next().ok_or("sway value")?)),
                Some("--pointer-tool") => {
                    pointer = Some(PathBuf::from(a.next().ok_or("pointer value")?))
                }
                Some("--at-spi-prefix") => {
                    atspi = Some(PathBuf::from(a.next().ok_or("atspi value")?))
                }
                Some("--output") => output = Some(PathBuf::from(a.next().ok_or("output value")?)),
                Some("--source") => source = PathBuf::from(a.next().ok_or("source value")?),
                Some("--egl-vendor") => egl = Some(PathBuf::from(a.next().ok_or("egl value")?)),
                Some("--parent-net") => {
                    parent_net = Some(a.next().ok_or("net value")?.to_string_lossy().into_owned())
                }
                Some("--source-revision") => {
                    revision = Some(
                        a.next()
                            .ok_or("revision value")?
                            .to_string_lossy()
                            .into_owned(),
                    )
                }
                Some("--effects") => effects = true,
                Some("--inside") => inside = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown clock option: {}",
                        x.to_string_lossy()
                    )));
                }
            }
        }
        Ok(Self {
            launcher: launcher.ok_or("launcher required")?,
            sway: sway.ok_or("sway required")?,
            pointer: pointer.ok_or("pointer required")?,
            atspi: atspi.ok_or("atspi required")?,
            output: output.unwrap_or_else(|| PathBuf::from("/artifacts")),
            source,
            egl,
            effects,
            inside,
            parent_net,
            revision,
        })
    }
}
fn calendar() -> Result<()> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let r: Value = serde_json::from_str(&line)?;
    ensure(
        ["list", "load", "sync"].contains(&r["action"].as_str().unwrap_or("")),
        "Calendar action",
    )?;
    let d = r["selectedDate"].as_str().unwrap_or("2026-10-20");
    let e = json!({"id":"synthetic-event","title":"Synthetic clock appointment","date":d,"endDate":d,"startTime":"12:00","endTime":"13:00","allDay":false,"provider":"Fixture","calendar":"Local"});
    println!(
        "{}",
        json!({"configured":true,"events":[e.clone()],"monthEvents":[e],"days":[d],"total":1,"providers":[],"month":r["month"],"date":d})
    );
    Ok(())
}
fn netns() -> Result<String> {
    Ok(fs::read_link("/proc/self/ns/net")?
        .to_string_lossy()
        .into_owned())
}
fn which(name: &str) -> Result<PathBuf> {
    for dir in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let p = dir.join(name);
        if p.is_file() {
            return Ok(p);
        }
    }
    Err(fail(format!("Missing {name}")))
}
fn outer(o: Opt) -> Result<()> {
    ensure(!o.output.exists(), "Fresh clock output required")?;
    fs::create_dir_all(&o.output)?;
    let output = fs::canonicalize(&o.output)?;
    let source = fs::canonicalize(&o.source)?;
    let launcher = fs::canonicalize(&o.launcher)?;
    let sway = fs::canonicalize(&o.sway)?;
    let pointer = fs::canonicalize(&o.pointer)?;
    let atspi = fs::canonicalize(&o.atspi)?;
    for p in [&launcher, &sway, &pointer, &atspi] {
        ensure(
            s(p).starts_with("/nix/store/"),
            format!("Pinned tool required: {}", p.display()),
        )?
    }
    let revision = String::from_utf8(
        Command::new("git")
            .args(["-C", &s(&source), "rev-parse", "HEAD"])
            .output()?
            .stdout,
    )?
    .trim()
    .to_owned();
    let mut args = vec![
        "--inside".to_owned(),
        "--launcher".into(),
        s(&launcher),
        "--sway".into(),
        s(&sway),
        "--pointer-tool".into(),
        s(&pointer),
        "--at-spi-prefix".into(),
        s(&atspi),
        "--parent-net".into(),
        netns()?,
        "--source-revision".into(),
        revision,
    ];
    if o.effects {
        let egl = fs::canonicalize(o.egl.as_ref().ok_or("Effects EGL vendor required")?)?;
        ensure(
            s(&egl).starts_with("/nix/store/") && egl.is_file(),
            "Pinned Mesa EGL vendor required",
        )?;
        args.extend(["--effects".into(), "--egl-vendor".into(), s(&egl)])
    }
    let bin = fs::canonicalize(env::current_exe()?)?;
    ensure(
        s(&bin).starts_with("/nix/store/"),
        "Clock fixture must be Nix packaged",
    )?;
    let mut cmd = Command::new(which("lumen-application-isolation")?);
    cmd.args(["run", "--source", &s(&source), "--output", &s(&output)]);
    if o.effects {
        cmd.arg("--software-graphics");
    }
    let status = cmd.arg("--").arg(bin).args(args).status()?;
    ensure(
        status.success(),
        format!("Clock isolation failed: {status}"),
    )
}
fn connect_bus(address: &str) -> Result<Connection> {
    Ok(zbus::blocking::connection::Builder::address(address)?.build()?)
}
fn owner(bus: &Connection, name: &str) -> Result<bool> {
    let p = Proxy::new(
        bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    Ok(p.call("NameHasOwner", &(name,))?)
}
fn patch(
    path: &Path,
    needle: &str,
    replacement: &str,
    manifest: &Path,
    description: &str,
    matches: usize,
) -> Result<()> {
    let old = fs::read_to_string(path)?;
    ensure(
        old.matches(needle).count() == matches,
        format!("QML patch points: {}", path.display()),
    )?;
    let new = old.replace(needle, replacement);
    ensure(new.replace(replacement, needle) == old, "QML roundtrip")?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    fs::write(path, &new)?;
    fs::write(
        manifest,
        serde_json::to_vec_pretty(
            &json!({"originalSha256":sha_bytes(old.as_bytes()),"modifiedSha256":sha_bytes(new.as_bytes()),"transformation":description,"matches":matches,"roundtrip":true}),
        )?,
    )?;
    Ok(())
}
fn node(tree: &Value, pid: u32) -> Option<Value> {
    if tree["pid"].as_u64() == Some(pid as u64) && tree["name"] == "Clock return target" {
        return Some(tree.clone());
    }
    for k in ["nodes", "floating_nodes"] {
        for v in tree[k].as_array().into_iter().flatten() {
            if let Some(row) = node(v, pid) {
                return Some(row);
            }
        }
    }
    None
}
struct Clock {
    f: Session,
    o: Opt,
    source: PathBuf,
    pointer: usize,
    client: u32,
    bus: Connection,
    checks: Vec<&'static str>,
}
impl Clock {
    fn state(&self) -> Result<Value> {
        let raw = self.f.call(
            "quickshell",
            &["-p", &s(&self.source), "ipc", "call", "clockAudit", "state"],
        )?;
        let v: Value = serde_json::from_str(&raw)?;
        fs::write(
            self.f.out.join("last-state.json"),
            serde_json::to_vec_pretty(&v)?,
        )?;
        Ok(v)
    }
    fn client_node(&self) -> Result<Option<Value>> {
        Ok(node(&self.f.tree()?, self.client))
    }
    fn cursor(&mut self, x: i64, y: i64) -> Result<()> {
        self.f.call(
            "swaymsg",
            &[
                "seat",
                "seat0",
                "cursor",
                "set",
                &x.to_string(),
                &y.to_string(),
            ],
        )?;
        self.f.write(self.pointer, "m 0.5 0.5\n")
    }
    fn click(&mut self, hover: bool) -> Result<()> {
        let r = self.state()?["bar"]["clock"].clone();
        let (x, y, w, h) = (
            r["x"].as_f64().ok_or("x")?,
            r["y"].as_f64().ok_or("y")?,
            r["w"].as_f64().ok_or("w")?,
            r["h"].as_f64().ok_or("h")?,
        );
        ensure(w > 0. && h > 0., "Invalid clock rect")?;
        self.cursor((x + w / 2.) as i64, (y + h / 2.) as i64)?;
        if hover {
            if wait("physical clock hover", 15, || {
                Ok(self.state()?["bar"]["clock"]["hovered"]
                    .as_bool()
                    .filter(|v| *v))
            })
            .is_err()
            {
                self.f
                    .capture(&self.f.out.join("pointer-failure.png"), &[])?;
                fs::write(
                    self.f.out.join("pointer-failure-seats.json"),
                    self.f.call("swaymsg", &["-r", "-t", "get_seats"])?,
                )?;
                return Err(fail("Clock hover acknowledgement failed"));
            }
        }
        self.f.write(self.pointer, "p\nr\n")
    }
    fn opened(&self) -> Result<Option<Value>> {
        let v = self.state()?;
        let bar = &v["bar"];
        let cal = &bar["layout"]["calendar"];
        Ok((bar["menu"] == "Date" && cal["events"] == 1 && cal["busy"] == false).then_some(v))
    }
    fn closed(&self) -> Result<Option<()>> {
        Ok((self.state()?["bar"]["menu"] == "").then_some(()))
    }
    fn run(&mut self) -> Result<()> {
        let owned = wait("exact private return window", 15, || self.client_node())?;
        let id = owned["id"].as_i64().ok_or("return id")?;
        self.f
            .call("swaymsg", &[&format!("[con_id={id}]"), "focus"])?;
        wait("native return focus", 15, || {
            Ok(self.client_node()?.filter(|v| v["focused"] == true))
        })?;
        wait("actual installed clock owner", 15, || {
            Ok(self.state()?["ready"].as_bool().filter(|v| *v))
        })?;
        let helper = self
            .f
            .env
            .get("LUMEN_NOTIFICATION_HELPER")
            .ok_or("notification helper missing")?
            .clone();
        self.f
            .launch(&helper, &["serve"], "notifications.log", false)?;
        wait("private notification service", 15, || {
            Ok(owner(&self.bus, "org.freedesktop.Notifications")?.then_some(()))
        })?;
        let api = Proxy::new(
            &self.bus,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )?;
        let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
        let notice: u32 = api.call(
            "Notify",
            &(
                "Clock fixture",
                0u32,
                "",
                "Synthetic retained notification",
                "Synthetic private body",
                Vec::<&str>::new(),
                hints,
                0i32,
            ),
        )?;
        wait("real retained notification", 15, || {
            Ok(self.state()?["history"]
                .as_array()
                .and_then(|rows| {
                    rows.iter().find(|r| {
                        r["id"] == notice && r["summary"] == "Synthetic retained notification"
                    })
                })
                .cloned())
        })?;
        self.checks.push(
            "Production notification transport retains exact synthetic content on private bus",
        );
        self.click(true)?;
        let v = wait(
            "first physical clock click opens populated surface",
            15,
            || self.opened(),
        )?;
        let l = &v["bar"]["layout"];
        let get = |k| {
            l[k].as_f64()
                .ok_or_else(|| fail(format!("Layout {k} missing: {l}")))
        };
        ensure(
            get("panelWidth")? > 0.
                && get("panelHeight")? > 0.
                && get("x")? >= 0.
                && get("y")? >= 0.
                && get("x")? + get("panelWidth")? <= get("width")? + 2.
                && get("y")? + get("panelHeight")? <= get("height")? + 2.,
            format!("Clock panel offscreen: {l}"),
        )?;
        self.f.capture(&self.f.out.join("clock-open.png"), &[])?;
        fs::write(
            self.f.out.join("opened-state.json"),
            serde_json::to_vec_pretty(&v)?,
        )?;
        self.checks.push(
            "First actual clock press/release opens measured notification and calendar surface",
        );
        self.click(false)?;
        wait("second physical clock click closes", 15, || self.closed())?;
        self.f
            .capture(&self.f.out.join("clock-second-click.png"), &[])?;
        self.checks
            .push("Second actual clock press/release dismisses the existing surface");
        self.click(true)?;
        wait("physical reopen", 15, || self.opened())?;
        self.cursor(100, 600)?;
        self.f.write(self.pointer, "p\nr\n")?;
        wait("actual outside press dismissal", 15, || self.closed())?;
        self.f.capture(&self.f.out.join("clock-outside.png"), &[])?;
        wait("exact pre-popup window focus", 15, || {
            Ok(self
                .client_node()?
                .filter(|v| v["id"] == id && v["focused"] == true))
        })?;
        self.checks
            .push("Outside physical click dismisses and restores exact native window focus");
        self.click(true)?;
        wait("physical reopen for Escape", 15, || self.opened())?;
        self.f.call("wtype", &["-k", "Escape"])?;
        wait("physical Escape", 15, || self.closed())?;
        wait("exact native focus after Escape", 15, || {
            Ok(self
                .client_node()?
                .filter(|v| v["id"] == id && v["focused"] == true))
        })?;
        self.checks
            .push("Physical Escape dismisses populated surface and restores captured native focus");
        ensure(
            self.state()?["history"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|r| r["id"] == notice && r["summary"] == "Synthetic retained notification")
            }),
            "Retained notification changed",
        )?;
        self.checks
            .push("Toggle and dismissal preserve retained notification identity and content");
        if self.o.effects {
            let log = fs::read_to_string(self.f.out.join("sway.log"))?;
            for marker in [
                "Creating scenefx FX renderer",
                "Using OpenGL ES",
                "GL renderer: llvmpipe",
                "FX RENDERER: Shaders Initialized Successfully",
                "Trying udmabuf allocator",
                "Created GL FBO for buffer",
            ] {
                ensure(
                    log.contains(marker),
                    format!("Missing effects marker: {marker}"),
                )?
            }
        }
        let packages = json!({"launcher":{"path":s(&self.o.launcher),"sha256":sha_file(&self.o.launcher)?},"compositor":{"path":s(&self.o.sway),"sha256":sha_file(&self.o.sway)?},"pointer":{"path":s(&self.o.pointer),"sha256":sha_file(&self.o.pointer)?}});
        fs::write(
            self.f.out.join("result.json"),
            serde_json::to_vec_pretty(
                &json!({"passed":true,"checks":self.checks,"effects":self.o.effects,"sourceRevision":self.o.revision,"fixtureSha256":sha_file(&env::current_exe()?)?,"packages":packages,"scope":"Actual full shell clock and physical pointer/keyboard; synthetic private notifications/calendar, no account or reference measurement claim"}),
            )?,
        )?;
        println!("PASS actual installed clock {} checks", self.checks.len());
        Ok(())
    }
}
fn inner(o: Opt) -> Result<()> {
    ensure(
        unsafe { libc::geteuid() } != 0,
        "Clock isolation must be non-root",
    )?;
    ensure(
        Some(netns()?) != o.parent_net,
        "Clock network namespace not isolated",
    )?;
    for name in ["dri", "snd", "video0"] {
        ensure(
            !Path::new("/dev").join(name).exists(),
            format!("Host device exposed: {name}"),
        )?
    }
    ensure(
        Path::new("/dev/udmabuf").exists() == o.effects,
        "Software graphics device mismatch",
    )?;
    ensure(
        fs::read_dir("/home")?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect::<Vec<_>>()
            == vec![PathBuf::from(env::var("HOME")?)],
        "Home isolation failed",
    )?;
    let out = PathBuf::from("/artifacts");
    if o.effects {
        let dev = fs::metadata("/dev/udmabuf")?;
        ensure(
            dev.file_type().is_char_device(),
            "udmabuf is not a character device",
        )?;
        fs::write(
            out.join("software-device.json"),
            serde_json::to_vec_pretty(
                &json!({"device":"udmabuf","major":libc::major(dev.rdev()),"minor":libc::minor(dev.rdev()),"hardwarePathsAbsent":true,"renderer":"gles2","forceSoftware":true}),
            )?,
        )?
    }
    let mut f = Session::existing(&out, &o.launcher, &o.sway, "lumen-clock-native-")?;
    f.set_many(&[
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "1"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if o.effects { "gles2" } else { "pixman" }),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("WLR_RENDERER_FORCE_SOFTWARE", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
    ]);
    if let Some(egl) = &o.egl {
        f.set("__EGL_VENDOR_LIBRARY_FILENAMES", s(egl))
    }
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "NO_AT_BRIDGE",
        "GTK_A11Y",
        "AT_SPI_BUS_ADDRESS",
    ] {
        f.env.remove(name);
    }
    f.start_bus()?;
    f.launch(
        &s(&o.atspi.join("libexec/at-spi-bus-launcher")),
        &["--launch-immediately", "--a11y=1"],
        "a11y.log",
        false,
    )?;
    let bus = connect_bus(
        f.env
            .get("DBUS_SESSION_BUS_ADDRESS")
            .ok_or("Session bus address")?,
    )?;
    wait("private accessibility bus", 15, || {
        Ok(owner(&bus, "org.a11y.Bus")?.then_some(()))
    })?;
    let a11y = Proxy::new(&bus, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus")?;
    let address: String = a11y.call("GetAddress", &())?;
    ensure(
        address.starts_with(&format!("unix:path={}/", f.root.path().display())),
        "Accessibility bus escaped private runtime",
    )?;
    f.set("AT_SPI_BUS_ADDRESS", address.clone());
    f.launch(
        &s(&o.atspi.join("libexec/at-spi2-registryd")),
        &[],
        "registry.log",
        false,
    )?;
    let registry = connect_bus(&address)?;
    wait("private accessibility registry", 15, || {
        Ok(owner(&registry, "org.a11y.atspi.Registry")?.then_some(()))
    })?;
    let installed = PathBuf::from(
        f.env
            .get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Installed desktop assets missing")?,
    );
    let assets = out.join("assets");
    copy_tree(&installed, &assets)?;
    let source = assets.join("shell");
    for name in [
        "MenuBar.qml",
        "NotificationPanel.qml",
        "CalendarCard.qml",
        "shell.qml",
    ] {
        ensure(
            fs::read(source.join(name))?
                == fs::read(Path::new("/source/res/lumen-desktop/shell").join(name))?,
            format!("Installed clock source mismatch: {name}"),
        )?
    }
    f.call(
        "lumen-fixture-providers",
        &["isolate", &s(&source), &s(&out)],
    )?;
    f.set("LUMEN_NEARBY_SHARING_PROVIDER", "");
    let probe = r#"
 function clockProbe(){
  function hover(item){if(item.containsMouse!==undefined && item.containsMouse)return true;return (item.children || []).some(child=>hover(child));}
  const point=clockButton.mapToItem(bar.contentItem,0,0);
  return {menu:menu,clock:{x:point.x,y:point.y,w:clockButton.width,h:clockButton.height,hovered:hover(clockButton)},layout:popupLayout(),popupFocused:popupContents.activeFocus};
 }
"#;
    patch(
        &source.join("MenuBar.qml"),
        "id: bar",
        &format!("id: bar{probe}"),
        &out.join("menu-probe-manifest.json"),
        "Read-only clockProbe geometry/hover introspection after id:bar",
        1,
    )?;
    let target = source.join("shell.qml");
    let original = fs::read_to_string(&target)?;
    let (head, tail) = original
        .rsplit_once('}')
        .ok_or("shell root brace missing")?;
    let addition = r#"
 IpcHandler {target:"clockAudit"
  function state():string{const bar=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!bar,history:root.notifications.history.map(row=>({id:row.id,summary:row.summary,body:row.body})),bar:bar?bar.clockProbe():null});}
 }
"#;
    let modified = format!("{head}{addition}}}{tail}");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
    fs::write(&target, &modified)?;
    fs::write(
        out.join("shell-probe-manifest.json"),
        serde_json::to_vec_pretty(
            &json!({"originalSha256":sha_bytes(original.as_bytes()),"modifiedSha256":sha_bytes(modified.as_bytes()),"transformation":"Append read-only clock/history status observer","roundtrip":format!("{head}}}{tail}")==original}),
        )?,
    )?;
    let provider = out.join("calendar-provider");
    symlink(env::current_exe()?, &provider)?;
    patch(
        &source.join("CalendarCard.qml"),
        "[\"lumen-calendar\"]",
        &serde_json::to_string(&vec![s(&provider)])?,
        &out.join("calendar-provider-manifest.json"),
        "Fixed local synthetic calendar command only",
        2,
    )?;
    let state = f.root.path().join("history");
    f.set("LUMEN_NOTIFICATION_STATE", s(&state));
    let settings = f.root.path().join("settings.json");
    fs::write(
        &settings,
        serde_json::to_vec(&json!({"notifications":{"preview":"when_unlocked","policies":{}}}))?,
    )?;
    f.set("LUMEN_NIX_SETTINGS_ACTIVE", s(&settings));
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = PathBuf::from(f.env.get("XDG_STATE_HOME").ok_or("State home")?)
        .join("lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("Appearance parent")?)?;
    fs::write(
        &appearance,
        serde_json::to_vec(
            &json!({"font":palette["font"],"palette":palette["light"],"preferences":{"reduceMotion":true}}),
        )?,
    )?;
    let config = out.join("sway.conf");
    fs::write(
        &config,
        "output HEADLESS-1 resolution 1280x800\nseat seat0 fallback true\nfocus_follows_mouse no\nfor_window [all] floating enable\n",
    )?;
    f.launch(
        &s(&o.sway),
        &["-d", "--unsupported-gpu", "-c", &s(&config)],
        "sway.log",
        false,
    )?;
    f.wait_sway(1)?;
    f.launch("wtype", &["-s", "60000"], "keyboard.log", false)?;
    let pointer = f.launch(
        &s(&o.pointer),
        &["pointer", "move", "0", "0"],
        "pointer.log",
        true,
    )?;
    f.write(pointer, "m 0 0\n")?;
    wait("virtual pointer registration", 15, || {
        Ok(fs::read_to_string(out.join("sway.log"))?
            .contains("adding virtual pointer: '0:0:wlr_virtual_pointer_v1'")
            .then_some(()))
    })?;
    f.launch("quickshell", &["-p", &s(&source)], "shell.log", false)?;
    let client_file = out.join("client.qml");
    fs::write(
        &client_file,
        "import Quickshell\nimport QtQuick\nShellRoot {FloatingWindow {visible:true;title:\"Clock return target\";implicitWidth:500;implicitHeight:320;Rectangle {anchors.fill:parent;color:\"#334455\";TextInput {anchors.centerIn:parent;focus:true;text:\"Synthetic return target\"}}}}",
    )?;
    let index = f.launch("quickshell", &["-p", &s(&client_file)], "client.log", false)?;
    let client = f.child_pid(index);
    let mut clock = Clock {
        f,
        o,
        source,
        pointer,
        client,
        bus,
        checks: Vec::new(),
    };
    let result = clock.run();
    fs::write(
        clock.f.out.join("child-status-before-cleanup.json"),
        serde_json::to_vec_pretty(&clock.f.process_status())?,
    )?;
    result
}
fn main() {
    let result = (|| -> Result<()> {
        if env::args_os()
            .next()
            .and_then(|x| PathBuf::from(x).file_name().map(|v| v.to_owned()))
            .is_some_and(|v| v == "calendar-provider")
        {
            return calendar();
        }
        let o = Opt::parse()?;
        if o.inside { inner(o) } else { outer(o) }
    })();
    if let Err(e) = result {
        eprintln!("Clock native fixture: {e}");
        std::process::exit(1)
    }
}
