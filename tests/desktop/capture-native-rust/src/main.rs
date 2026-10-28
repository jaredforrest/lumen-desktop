//! Full installed capture menu against a private, real PipeWire graph.
use lumen_native_input_fixtures::{
    Result, Session, copy_tree, ensure, s, sha_bytes, sha_file, wait,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use lumen_capture_native_fixture::graph::Graph;

#[derive(Default)]
struct Options {
    launcher: PathBuf,
    sway: PathBuf,
    pointer: PathBuf,
    pipewire: PathBuf,
    shim: PathBuf,
    atspi: PathBuf,
    output: PathBuf,
    source: PathBuf,
    egl: Option<PathBuf>,
    effects: bool,
    inside: bool,
    parent_net: Option<String>,
    source_revision: Option<String>,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut result = Self::default();
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            let name = arg.to_string_lossy();
            match name.as_ref() {
                "--launcher" => {
                    result.launcher = PathBuf::from(args.next().ok_or("Missing launcher")?)
                }
                "--sway" => result.sway = PathBuf::from(args.next().ok_or("Missing sway")?),
                "--pointer-tool" => {
                    result.pointer = PathBuf::from(args.next().ok_or("Missing pointer")?)
                }
                "--pipewire" => {
                    result.pipewire = PathBuf::from(args.next().ok_or("Missing PipeWire")?)
                }
                "--capture-shim" => {
                    result.shim = PathBuf::from(args.next().ok_or("Missing capture shim")?)
                }
                "--at-spi-prefix" => {
                    result.atspi = PathBuf::from(args.next().ok_or("Missing AT-SPI")?)
                }
                "--output" => result.output = PathBuf::from(args.next().ok_or("Missing output")?),
                "--source" => result.source = PathBuf::from(args.next().ok_or("Missing source")?),
                "--egl-vendor" => {
                    result.egl = Some(PathBuf::from(args.next().ok_or("Missing EGL vendor")?))
                }
                "--effects" => result.effects = true,
                "--inside" => result.inside = true,
                "--parent-net" => {
                    result.parent_net = Some(
                        args.next()
                            .ok_or("Missing parent network")?
                            .to_string_lossy()
                            .into_owned(),
                    )
                }
                "--source-revision" => {
                    result.source_revision = Some(
                        args.next()
                            .ok_or("Missing source revision")?
                            .to_string_lossy()
                            .into_owned(),
                    )
                }
                _ => return Err(format!("Unknown capture fixture option {name}").into()),
            }
        }
        for (name, path) in [
            ("launcher", &result.launcher),
            ("sway", &result.sway),
            ("pointer", &result.pointer),
            ("pipewire", &result.pipewire),
            ("capture shim", &result.shim),
            ("AT-SPI", &result.atspi),
            ("output", &result.output),
        ] {
            ensure(!path.as_os_str().is_empty(), format!("Missing {name}"))?;
        }
        if result.source.as_os_str().is_empty() {
            result.source = env::current_dir()?;
        }
        if result.egl.is_none() {
            result.egl = env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").map(PathBuf::from);
        }
        Ok(result)
    }
}

fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn bus(address: &str) -> Result<Connection> {
    Ok(zbus::blocking::connection::Builder::address(address)?.build()?)
}
fn owned(connection: &Connection, name: &str) -> Result<bool> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    Ok(proxy.call("NameHasOwner", &(name,))?)
}
fn get_json(f: &Session, qml: &Path) -> Result<Value> {
    serde_json::from_str(&f.call(
        "quickshell",
        &["-p", &s(qml), "ipc", "call", "captureAudit", "state"],
    )?)
    .map_err(Into::into)
}
fn state(f: &Session, qml: &Path) -> Result<Value> {
    let value = get_json(f, qml)?;
    save(&f.out.join("last-state.json"), &value)?;
    Ok(value)
}
fn named(
    connection: &Connection,
    name: &str,
    output: &Path,
) -> Result<Option<(String, OwnedObjectPath)>> {
    let mut queue = VecDeque::from([(
        "org.a11y.atspi.Registry".to_owned(),
        OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root")?,
    )]);
    let mut seen = HashSet::new();
    let mut observed = Vec::new();
    while let Some((owner, path)) = queue.pop_front() {
        if seen.len() >= 4000
            || path.as_str() == "/org/a11y/atspi/null"
            || !seen.insert((owner.clone(), path.clone()))
        {
            continue;
        }
        let record = (|| -> Result<(String, bool, Vec<(String, OwnedObjectPath)>)> {
            let properties = Proxy::new(
                connection,
                owner.as_str(),
                path.as_str(),
                "org.freedesktop.DBus.Properties",
            )?;
            let value: OwnedValue =
                properties.call("Get", &("org.a11y.atspi.Accessible", "Name"))?;
            let title = String::try_from(value)?;
            let accessible = Proxy::new(
                connection,
                owner.as_str(),
                path.as_str(),
                "org.a11y.atspi.Accessible",
            )?;
            let bits: Vec<u32> = accessible.call("GetState", &())?;
            let showing = bits.first().is_some_and(|word| word & (1 << 25) != 0);
            let children = accessible.call("GetChildren", &())?;
            Ok((title, showing, children))
        })();
        if let Ok((title, showing, children)) = record {
            observed
                .push(json!({"name":title,"showing":showing,"owner":owner,"path":path.as_str()}));
            save(
                &output.join("last-accessibility-tree.json"),
                &json!(observed),
            )?;
            if title == name && showing {
                return Ok(Some((owner, path)));
            }
            queue.extend(children);
        }
    }
    Ok(None)
}
fn patch(path: &Path, needle: &str, insertion: &str, manifest: &Path, label: &str) -> Result<()> {
    let original = fs::read_to_string(path)?;
    ensure(
        original.matches(needle).count() == 1,
        format!("{label} marker count"),
    )?;
    let modified = original.replacen(needle, &format!("{needle}{insertion}"), 1);
    ensure(
        modified.replacen(&format!("{needle}{insertion}"), needle, 1) == original,
        "QML patch roundtrip",
    )?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    fs::write(path, &modified)?;
    save(
        manifest,
        &json!({"originalSha256":sha_bytes(original.as_bytes()),"modifiedSha256":sha_bytes(modified.as_bytes()),"transformation":label,"roundtrip":true}),
    )?;
    Ok(())
}

fn outer(o: &Options) -> Result<()> {
    let source = fs::canonicalize(&o.source)?;
    ensure(
        source
            .join("res/lumen-desktop/shell/CaptureState.qml")
            .exists(),
        "Source checkout missing capture assets",
    )?;
    let output = o.output.clone();
    ensure(!output.exists(), "Fresh capture output required")?;
    fs::create_dir_all(&output)?;
    let output = fs::canonicalize(output)?;
    let launcher = fs::canonicalize(&o.launcher)?;
    let sway = fs::canonicalize(&o.sway)?;
    let pointer = fs::canonicalize(&o.pointer)?;
    let pipewire = fs::canonicalize(&o.pipewire)?;
    let shim = fs::canonicalize(&o.shim)?;
    let atspi = fs::canonicalize(&o.atspi)?;
    for path in [&launcher, &sway, &pointer, &pipewire, &shim, &atspi] {
        ensure(
            s(path).starts_with("/nix/store/"),
            format!("Unpinned tool {}", path.display()),
        )?;
    }
    for name in ["pipewire", "pw-dump", "pw-cli", "pw-link"] {
        ensure(
            o.pipewire.join(name).exists(),
            format!("Missing PipeWire {name}"),
        )?;
    }
    let binary = fs::canonicalize(env::current_exe()?)?;
    ensure(
        s(&binary).starts_with("/nix/store/"),
        "Capture fixture must be Nix packaged",
    )?;
    let mut cmd = Command::new("lumen-application-isolation");
    cmd.args(["run", "--source", &s(&source), "--output", &s(&output)]);
    if o.effects {
        cmd.arg("--software-graphics");
    }
    let revision = Command::new("git")
        .args(["-C", &s(&source), "rev-parse", "HEAD"])
        .output()?;
    ensure(revision.status.success(), "Cannot read source revision")?;
    let revision = String::from_utf8(revision.stdout)?.trim().to_owned();
    let parent_net = s(&fs::read_link("/proc/self/ns/net")?);
    cmd.arg("--").arg(binary).args([
        "--inside",
        "--launcher",
        &s(&launcher),
        "--sway",
        &s(&sway),
        "--pointer-tool",
        &s(&pointer),
        "--pipewire",
        &s(&pipewire),
        "--capture-shim",
        &s(&shim),
        "--at-spi-prefix",
        &s(&atspi),
        "--source",
        "/source",
        "--output",
        "/artifacts",
        "--parent-net",
        &parent_net,
        "--source-revision",
        &revision,
    ]);
    if o.effects {
        cmd.arg("--effects");
    }
    if o.effects {
        let egl = fs::canonicalize(
            o.egl
                .as_ref()
                .ok_or("Pinned software EGL vendor required")?,
        )?;
        ensure(
            s(&egl).starts_with("/nix/store/"),
            "Software EGL vendor is not pinned",
        )?;
        cmd.args(["--egl-vendor", &s(&egl)]);
    }
    ensure(cmd.status()?.success(), "Isolated capture fixture failed")
}

fn main() -> Result<()> {
    let o = Options::parse()?;
    if o.inside { inside(&o) } else { outer(&o) }
}

const MENU_PROBE: &str = r#"
 function captureProbe(){
  function scan(item,name,text){if((name && item.objectName===name) || (text && item.text===text && item.hovered!==undefined && item.visible))return item;for(const child of item.children || []){const found=scan(child,name,text);if(found)return found;}return null;}
  function rect(item,owner){if(!item)return null;const point=item.mapToItem(owner,0,0);return {x:point.x,y:point.y,w:item.width,h:item.height,visible:item.visible,hovered:item.hovered===undefined?false:item.hovered,focused:item.activeFocus};}
  const controls=scan(bar.contentItem,"status-item-controls","");const row=scan(dismiss.contentItem,"","Capture observations…");
  return {menu:menu,kind:systemMenu,indicator:rect(captureIndicator,bar.contentItem),controls:rect(controls,bar.contentItem),row:rect(row,dismiss.contentItem),layout:popupLayout(),headings:menuLayoutStatus(),popupFocused:popupContents.activeFocus};
 }
"#;
const SHELL_PROBE: &str = r#"
 IpcHandler {target:"captureAudit"
  function state():string{const bar=root.bars["HEADLESS-1"];return JSON.stringify({ready:!!bar,capture:root.capture.status,lastPublished:root.capture.lastPublished,bar:bar?bar.captureProbe():null});}
 }
"#;
const CLIENT_QML: &str = "import Quickshell\nimport QtQuick\nShellRoot {FloatingWindow {visible:true;title:\"Capture return target\";implicitWidth:500;implicitHeight:320;Rectangle {anchors.fill:parent;color:\"#334455\";TextInput {anchors.centerIn:parent;focus:true;text:\"Synthetic return target\"}}}}";
const CHECKS: [&str; 10] = [
    "empty private real graph has no capture dot and no global idle assertion",
    "real synthetic PipeWire link drives production capture indicator",
    "physical indicator opens actual measured capture report",
    "Escape returns exact pre-popup native window focus",
    "production Control Centre keyboard button opens the same report",
    "crowded narrow output retains separate indicator/menu bounds and clipped report viewport",
    "actual graph unlink retires observation and dot",
    "held exact provider child causes natural stale expiry and dot removal",
    "fresh generation recovers real graph",
    "provider disappearance fails unavailable without retaining a positive dot",
];

fn find_program(env: &BTreeMap<String, String>, name: &str) -> Result<PathBuf> {
    for directory in env.get("PATH").ok_or("PATH absent")?.split(':') {
        let candidate = Path::new(directory).join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("{name} absent from launcher PATH").into())
}
fn client(f: &Session, pid: u32) -> Result<Option<Value>> {
    fn visit(node: &Value, pid: u32) -> Option<Value> {
        if node["pid"].as_u64() == Some(pid as u64) && node["name"] == "Capture return target" {
            return Some(node.clone());
        }
        for key in ["nodes", "floating_nodes"] {
            for child in node[key].as_array().into_iter().flatten() {
                if let Some(found) = visit(child, pid) {
                    return Some(found);
                }
            }
        }
        None
    }
    Ok(visit(&f.tree()?, pid))
}
fn available(rows: &Value) -> bool {
    rows["capture"]["available"] == true
}
fn click(f: &mut Session, pointer: usize, rect: &Value, qml: &Path, hover: bool) -> Result<()> {
    let x = rect["x"].as_f64().ok_or("Missing click x")?
        + rect["w"].as_f64().ok_or("Missing click width")? / 2.;
    let y = rect["y"].as_f64().ok_or("Missing click y")?
        + rect["h"].as_f64().ok_or("Missing click height")? / 2.;
    ensure(
        rect["w"].as_f64().unwrap_or_default() > 0. && rect["h"].as_f64().unwrap_or_default() > 0.,
        "Click target has no area",
    )?;
    f.ipc(&format!("seat seat0 cursor set {} {}", x as i64, y as i64))?;
    f.write(pointer, "m 0.5 0.5\n")?;
    if hover {
        wait("physical pointer hover", 15, || {
            Ok((state(f, qml)?["bar"]["indicator"]["hovered"] == true).then_some(()))
        })?;
    }
    f.write(pointer, "p\nr\n")
}
fn escape(f: &Session, qml: &Path, pid: u32, con_id: u64) -> Result<()> {
    f.call("wtype", &["-k", "Escape"])?;
    wait("physical report dismissal", 15, || {
        Ok((state(f, qml)?["bar"]["menu"] == "").then_some(()))
    })?;
    wait("exact return focus", 15, || {
        Ok(client(f, pid)?
            .filter(|v| v["id"].as_u64() == Some(con_id) && v["focused"] == true)
            .map(|_| ()))
    })
}
fn report(f: &Session, qml: &Path, atspi: &Connection, label: &str) -> Result<Value> {
    wait("capture panel routing", 15, || {
        let value = state(f, qml)?;
        Ok((value["bar"]["kind"] == "Capture" && value["bar"]["menu"] == "Capture").then_some(()))
    })?;
    f.capture(
        &f.out.join(format!("{label}-before-accessibility.png")),
        &[],
    )?;
    wait("capture report title", 15, || {
        named(atspi, "Capture observations", &f.out)
    })?;
    wait("capture active-link text", 15, || {
        named(atspi, "Active PipeWire capture links", &f.out)
    })?;
    let layout = state(f, qml)?["bar"]["layout"].clone();
    for key in ["panelWidth", "panelHeight"] {
        ensure(
            layout[key].as_f64().unwrap_or_default() > 0.,
            format!("Capture report {key}"),
        )?;
    }
    let x = layout["x"].as_f64().ok_or("Layout x")?;
    let y = layout["y"].as_f64().ok_or("Layout y")?;
    ensure(
        x >= 0.
            && y >= 0.
            && x + layout["panelWidth"].as_f64().ok_or("Panel width")?
                <= layout["width"].as_f64().ok_or("Layout width")? + 2.
            && y + layout["panelHeight"].as_f64().ok_or("Panel height")?
                <= layout["height"].as_f64().ok_or("Layout height")? + 2.,
        "Capture report escaped viewport",
    )?;
    f.capture(&f.out.join(format!("{label}.png")), &[])?;
    save(&f.out.join(format!("{label}-layout.json")), &layout)?;
    Ok(layout)
}
fn inside(o: &Options) -> Result<()> {
    ensure(
        unsafe { libc::geteuid() } != 0,
        "Capture boundary unexpectedly root",
    )?;
    ensure(
        !Path::new("/.old-root").exists(),
        "Host root remains attached",
    )?;
    ensure(
        Some(s(&fs::read_link("/proc/self/ns/net")?)) != o.parent_net,
        "Capture network namespace was not isolated",
    )?;
    ensure(
        fs::read_dir("/home")?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect::<Vec<_>>()
            == vec![PathBuf::from("/home/fixture")],
        "Capture boundary exposed another home",
    )?;
    for name in ["dri", "snd", "video0"] {
        ensure(
            !Path::new("/dev").join(name).exists(),
            format!("Hardware {name} visible"),
        )?;
    }
    ensure(
        Path::new("/dev/udmabuf").exists() == o.effects,
        "Software allocator boundary mismatch",
    )?;
    if o.effects {
        let meta = fs::metadata("/dev/udmabuf")?;
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        ensure(
            meta.file_type().is_char_device(),
            "Software allocator is not a character device",
        )?;
        let device = meta.rdev();
        save(
            &o.output.join("software-device.json"),
            &json!({
                "device":"udmabuf","major":libc::major(device),"minor":libc::minor(device),
                "hardwarePathsAbsent":true,"renderer":"gles2","forceSoftware":true,
            }),
        )?;
    }
    let mut f = Session::existing(&o.output, &o.launcher, &o.sway, "lumen-capture-native-")?;
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
        ("PIPEWIRE_REMOTE", "capture-fixture"),
    ]);
    f.set("PIPEWIRE_RUNTIME_DIR", s(f.root.path()));
    if let Some(egl) = &o.egl {
        f.set("__EGL_VENDOR_LIBRARY_FILENAMES", s(egl));
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "NO_AT_BRIDGE",
        "GTK_A11Y",
        "AT_SPI_BUS_ADDRESS",
    ] {
        f.env.remove(key);
    }
    f.start_bus()?;
    let mut graph = Graph::new(&o.pipewire, f.root.path(), &f.out, f.env.clone());
    graph.start()?;
    graph.prepare()?;
    f.launch(
        &s(&o.atspi.join("libexec/at-spi-bus-launcher")),
        &["--launch-immediately", "--a11y=1"],
        "a11y.log",
        false,
    )?;
    let session = bus(f
        .env
        .get("DBUS_SESSION_BUS_ADDRESS")
        .ok_or("Private bus missing")?)?;
    wait("private accessibility bus", 15, || {
        Ok(owned(&session, "org.a11y.Bus")?.then_some(()))
    })?;
    let service = Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus")?;
    let address: String = service.call("GetAddress", &())?;
    ensure(
        address.starts_with(&format!("unix:path={}/", f.root.path().display())),
        "AT-SPI bus escaped private runtime",
    )?;
    f.set("AT_SPI_BUS_ADDRESS", address.clone());
    f.launch(
        &s(&o.atspi.join("libexec/at-spi2-registryd")),
        &[],
        "registry.log",
        false,
    )?;
    let accessibility = bus(&address)?;
    wait("private accessibility registry", 15, || {
        Ok(owned(&accessibility, "org.a11y.atspi.Registry")?.then_some(()))
    })?;
    let installed = PathBuf::from(
        f.env
            .get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Installed shell assets missing")?,
    );
    let assets = f.out.join("assets");
    copy_tree(&installed, &assets)?;
    let shell = assets.join("shell");
    for name in ["CaptureState.qml", "CaptureReport.qml"] {
        ensure(
            fs::read(shell.join(name))?
                == fs::read(o.source.join("res/lumen-desktop/shell").join(name))?,
            format!("Installed capture source mismatch: {name}"),
        )?;
    }
    f.call(
        "lumen-fixture-providers",
        &["isolate", &s(&shell), &s(&f.out)],
    )?;
    patch(
        &shell.join("MenuBar.qml"),
        "id: bar",
        MENU_PROBE,
        &f.out.join("menu-probe-manifest.json"),
        "Read-only captureProbe geometry and hover introspection",
    )?;
    let shell_qml = shell.join("shell.qml");
    let original = fs::read_to_string(&shell_qml)?;
    let last = original.rfind('}').ok_or("Shell root closure absent")?;
    let mut modified = original.clone();
    modified.insert_str(last, SHELL_PROBE);
    fs::set_permissions(&shell_qml, fs::Permissions::from_mode(0o600))?;
    fs::write(&shell_qml, &modified)?;
    save(
        &f.out.join("shell-probe-manifest.json"),
        &json!({"originalSha256":sha_bytes(original.as_bytes()),"modifiedSha256":sha_bytes(modified.as_bytes()),"transformation":"Append read-only capture/status observer","roundtrip":modified.replacen(SHELL_PROBE,"",1)==original}),
    )?;
    let actual = find_program(&f.env, "lumen-capture-observation")?;
    fs::write(f.out.join("capture-real-path"), s(&actual))?;
    std::os::unix::fs::symlink(&o.shim, f.out.join("lumen-capture-observation"))?;
    f.set(
        "PATH",
        format!(
            "{}:{}",
            f.out.display(),
            f.env.get("PATH").ok_or("PATH absent")?
        ),
    );
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = PathBuf::from(f.env.get("XDG_STATE_HOME").ok_or("State home missing")?)
        .join("lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("Appearance parent")?)?;
    save(
        &appearance,
        &json!({"font":palette["font"],"palette":palette["light"],"preferences":{"reduceMotion":true}}),
    )?;
    let config = f.out.join("sway.conf");
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
    wait("actual compositor virtual pointer registration", 15, || {
        Ok(fs::read_to_string(f.out.join("sway.log"))?
            .contains("adding virtual pointer: '0:0:wlr_virtual_pointer_v1'")
            .then_some(()))
    })?;
    f.launch("quickshell", &["-p", &s(&shell)], "shell.log", false)?;
    let client_qml = f.out.join("client.qml");
    fs::write(&client_qml, CLIENT_QML)?;
    let client_index = f.launch("quickshell", &["-p", &s(&client_qml)], "client.log", false)?;
    let client_pid = f.child_pid(client_index);
    let owned = wait("exact private return window", 15, || client(&f, client_pid))?;
    let con_id = owned["id"].as_u64().ok_or("Return conId missing")?;
    f.ipc(&format!("[con_id={con_id}] focus"))?;
    wait("native return focus", 15, || {
        Ok(client(&f, client_pid)?
            .filter(|v| v["focused"] == true)
            .map(|_| ()))
    })?;
    let mut checks = Vec::new();
    let empty = wait("real empty graph UI", 15, || {
        let value = state(&f, &shell)?;
        Ok((value["ready"] == true
            && available(&value)
            && value["capture"]["rows"]
                .as_array()
                .is_some_and(Vec::is_empty))
        .then_some(value))
    })?;
    ensure(
        empty["bar"]["indicator"]["visible"] != true,
        "Capture dot visible on empty graph",
    )?;
    checks.push(CHECKS[0]);
    graph.link()?;
    wait("actual linked graph dot", 15, || {
        let value = state(&f, &shell)?;
        Ok((available(&value)
            && value["capture"]["rows"]
                .as_array()
                .is_some_and(|rows| rows.len() == 1)
            && value["bar"]["indicator"]["visible"] == true)
            .then_some(()))
    })?;
    ensure(
        state(&f, &shell)?["capture"]["rows"][0]["kind"] == "audio-input",
        "Wrong capture kind",
    )?;
    checks.push(CHECKS[1]);
    let indicator = state(&f, &shell)?["bar"]["indicator"].clone();
    click(&mut f, pointer, &indicator, &shell, true)?;
    report(&f, &shell, &accessibility, "indicator-report")?;
    checks.push(CHECKS[2]);
    escape(&f, &shell, client_pid, con_id)?;
    checks.push(CHECKS[3]);
    let controls = state(&f, &shell)?["bar"]["controls"].clone();
    click(&mut f, pointer, &controls, &shell, false)?;
    wait("Control Centre open", 15, || {
        Ok((state(&f, &shell)?["bar"]["menu"] == "Controls").then_some(()))
    })?;
    let row = wait("actual Control Centre report button", 15, || {
        named(
            &accessibility,
            "Capture observations…, PipeWire-visible activity",
            &f.out,
        )
    })?;
    let component = Proxy::new(
        &accessibility,
        row.0.as_str(),
        row.1.as_str(),
        "org.a11y.atspi.Component",
    )?;
    let grabbed: bool = component.call("GrabFocus", &())?;
    ensure(grabbed, "Control Centre row rejected accessibility focus")?;
    wait("Control Centre row focus", 15, || {
        let row = state(&f, &shell)?["bar"]["row"].clone();
        Ok((!row.is_null() && row["focused"] == true).then_some(()))
    })?;
    f.call("wtype", &["-k", "space"])?;
    report(&f, &shell, &accessibility, "control-centre-report")?;
    checks.push(CHECKS[4]);
    escape(&f, &shell, client_pid, con_id)?;
    f.ipc("output HEADLESS-1 resolution 640x480")?;
    wait("native narrow bar configure", 15, || {
        Ok((state(&f, &shell)?["bar"]["headings"]["width"] == 640).then_some(()))
    })?;
    let bar = state(&f, &shell)?["bar"].clone();
    ensure(
        bar["headings"]["menuEnd"].as_f64().ok_or("Menu end")?
            <= bar["headings"]["statusStart"]
                .as_f64()
                .ok_or("Status start")?
                + 2.,
        "Narrow menu overlaps status area",
    )?;
    ensure(
        bar["indicator"]["x"].as_f64().ok_or("Indicator x")? >= 0.
            && bar["indicator"]["x"].as_f64().ok_or("Indicator x")?
                + bar["indicator"]["w"].as_f64().ok_or("Indicator width")?
                <= 640.,
        "Narrow capture indicator outside output",
    )?;
    let indicator = bar["indicator"].clone();
    click(&mut f, pointer, &indicator, &shell, true)?;
    report(&f, &shell, &accessibility, "narrow-report")?;
    checks.push(CHECKS[5]);
    escape(&f, &shell, client_pid, con_id)?;
    graph.unlink()?;
    wait("actual graph unlink retires dot", 15, || {
        let value = state(&f, &shell)?;
        Ok((available(&value)
            && value["capture"]["rows"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && value["bar"]["indicator"]["visible"] != true)
            .then_some(()))
    })?;
    checks.push(CHECKS[6]);
    graph.link()?;
    wait("fresh graph observation", 15, || {
        Ok((state(&f, &shell)?["bar"]["indicator"]["visible"] == true).then_some(()))
    })?;
    fs::write(f.out.join("hold-capture"), "")?;
    let facts = wait("exact held observation child", 15, || {
        let path = f.out.join("held-child.json");
        Ok(if path.exists() {
            Some(serde_json::from_slice::<Value>(&fs::read(path)?)?)
        } else {
            None
        })
    })?;
    let child_pid = facts["pid"].as_i64().ok_or("Held child PID missing")?;
    let start = facts["start"].as_str().ok_or("Held child start missing")?;
    let shell_pid = f.child_pid(f.child_count() - 2); // Shell precedes the return client.
    let proc_stat = fs::read_to_string(format!("/proc/{child_pid}/stat"))?;
    let fields = proc_stat
        .rsplit_once(") ")
        .ok_or("Malformed held-child stat")?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    ensure(
        fields.get(19).copied() == Some(start),
        "Held child start changed",
    )?;
    ensure(
        fields.get(1).and_then(|v| v.parse::<u32>().ok()) == Some(shell_pid),
        "Held child parent is not shell",
    )?;
    ensure(
        facts["parent"].as_u64() == Some(shell_pid as u64),
        "Held child record parent mismatch",
    )?;
    wait("natural observation expiry", 15, || {
        let value = state(&f, &shell)?;
        Ok((!available(&value) && value["bar"]["indicator"]["visible"] != true).then_some(()))
    })?;
    checks.push(CHECKS[7]);
    fs::remove_file(f.out.join("hold-capture"))?;
    let proc_stat = fs::read_to_string(format!("/proc/{child_pid}/stat"))?;
    ensure(
        proc_stat
            .rsplit_once(") ")
            .ok_or("Malformed held child")?
            .1
            .split_whitespace()
            .nth(19)
            == Some(start),
        "Held child reused",
    )?;
    unsafe {
        libc::kill(child_pid as i32, libc::SIGKILL);
    }
    wait("fresh generation recovery", 15, || {
        Ok((state(&f, &shell)?["bar"]["indicator"]["visible"] == true).then_some(()))
    })?;
    checks.push(CHECKS[8]);
    graph.close();
    wait("private provider disappearance", 15, || {
        let value = state(&f, &shell)?;
        Ok((!available(&value) && value["bar"]["indicator"]["visible"] != true).then_some(()))
    })?;
    checks.push(CHECKS[9]);
    if o.effects {
        let log = fs::read_to_string(f.out.join("sway.log"))?;
        for text in [
            "Creating scenefx FX renderer",
            "Using OpenGL ES",
            "GL renderer: llvmpipe",
            "FX RENDERER: Shaders Initialized Successfully",
            "Trying udmabuf allocator",
            "Created GL FBO for buffer",
        ] {
            ensure(
                log.contains(text),
                format!("Effects renderer proof missing {text}"),
            )?;
        }
    }
    save(
        &f.out.join("result.json"),
        &json!({
            "passed":true,"checks":checks,"effects":o.effects,
            "sourceRevision":o.source_revision,
            "fixtureSha256":sha_file(&o.source.join("tests/desktop/capture-native-rust/src/main.rs"))?,
            "packages":{"launcher":s(&o.launcher),"compositor":s(&o.sway),"pointer":s(&o.pointer)},
            "scope":"Actual full shell, private real generated audio graph; no hardware, host graph, permission or global idle claim"
        }),
    )?;
    save(
        &f.out.join("child-status-before-cleanup.json"),
        &f.process_status(),
    )?;
    println!("PASS actual full capture menu {} checks", checks.len());
    Ok(())
}
