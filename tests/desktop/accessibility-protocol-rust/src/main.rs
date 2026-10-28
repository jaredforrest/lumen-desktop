use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn fail(s: impl Into<String>) -> Box<dyn Error> {
    s.into().into()
}
fn ensure(v: bool, label: impl Into<String>) -> Result<()> {
    if v { Ok(()) } else { Err(fail(label)) }
}
fn save(path: &Path, v: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(v)?))?;
    Ok(())
}
fn load(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn wait<T>(label: &str, mut f: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let until = Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    loop {
        match f() {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => (),
            Err(e) => last = e.to_string(),
        }
        if Instant::now() > until {
            return Err(fail(format!("Timed out: {label}: {last}")));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn check(label: &str, mut f: impl FnMut() -> Result<bool>) -> Result<()> {
    wait(label, || Ok(f()?.then_some(())))
}
#[derive(Clone)]
struct Options {
    output: PathBuf,
    launcher: PathBuf,
    sway: PathBuf,
    atspi: PathBuf,
    quickshell: PathBuf,
    full_shell: bool,
    inside: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut output = None;
        let mut launcher = None;
        let mut sway = PathBuf::from("sway");
        let mut atspi = env::var_os("LUMEN_TEST_AT_SPI").map(PathBuf::from);
        let mut quickshell = PathBuf::from("quickshell");
        let mut full_shell = false;
        let mut inside = false;
        let mut args = env::args_os().skip(1);
        while let Some(a) = args.next() {
            match a.to_str() {
                Some("--output") => {
                    output = Some(PathBuf::from(args.next().ok_or("--output value")?))
                }
                Some("--launcher") => {
                    launcher = Some(PathBuf::from(args.next().ok_or("--launcher value")?))
                }
                Some("--sway") => sway = PathBuf::from(args.next().ok_or("--sway value")?),
                Some("--at-spi-prefix") => {
                    atspi = Some(PathBuf::from(args.next().ok_or("--at-spi-prefix value")?))
                }
                Some("--quickshell") => {
                    quickshell = PathBuf::from(args.next().ok_or("--quickshell value")?)
                }
                Some("--full-shell") => full_shell = true,
                Some("--inside") => inside = true,
                _ => return Err(fail(format!("Unknown argument: {}", a.to_string_lossy()))),
            }
        }
        let output = output.ok_or("--output required")?;
        Ok(Self {
            output: if output.is_absolute() {
                output
            } else {
                env::current_dir()?.join(output)
            },
            launcher: launcher.ok_or("--launcher required")?,
            sway,
            atspi: atspi.ok_or("--at-spi-prefix or LUMEN_TEST_AT_SPI required")?,
            quickshell,
            full_shell,
            inside,
        })
    }
}
fn live_inode() -> Option<u64> {
    fs::metadata(format!("/run/user/{}/at-spi/bus_0", unsafe {
        libc::getuid()
    }))
    .ok()
    .map(|m| m.ino())
}
fn private_session(o: &Options) -> Result<()> {
    let before = live_inode();
    let tmp = tempfile::Builder::new().prefix("lumen-atspi-").tempdir()?;
    let dir = tmp.path();
    fs::write(
        dir.join("bus.conf"),
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            dir.display()
        ),
    )?;
    let mut vars: BTreeMap<String, String> = env::vars().collect();
    vars.insert("HOME".into(), dir.display().to_string());
    vars.insert("XDG_RUNTIME_DIR".into(), dir.display().to_string());
    for k in ["CONFIG", "STATE", "CACHE", "DATA"] {
        vars.insert(
            format!("XDG_{k}_HOME"),
            dir.join(k.to_lowercase()).display().to_string(),
        );
    }
    for k in [
        "AT_SPI_BUS_ADDRESS",
        "NO_AT_BRIDGE",
        "GTK_A11Y",
        "DBUS_SESSION_BUS_ADDRESS",
        "SWAYSOCK",
        "WAYLAND_DISPLAY",
        "DISPLAY",
    ] {
        vars.remove(k);
    }
    for (k, v) in [
        ("GSETTINGS_BACKEND", "memory"),
        ("QT_ACCESSIBILITY", "1"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1"),
    ] {
        vars.insert(k.into(), v.into());
    }
    vars.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/no-system-bus", dir.display()),
    );
    let status = Command::new("dbus-run-session")
        .arg(format!("--config-file={}", dir.join("bus.conf").display()))
        .arg("--")
        .arg(env::current_exe()?)
        .args(env::args_os().skip(1))
        .arg("--inside")
        .env_clear()
        .envs(&vars)
        .status()?;
    ensure(before == live_inode(), "Live accessibility socket changed")?;
    ensure(status.success(), format!("Inside fixture exited {status}"))?;
    let result = o.output.join("result.json");
    let mut v = load(&result)?;
    v["liveSocketUnchanged"] = json!(true);
    save(&result, &v)
}
struct ChildProcess(Child);
impl Drop for ChildProcess {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGTERM);
            }
        }
        let _ = self.0.wait();
    }
}
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir(dst)?;
    for item in fs::read_dir(src)? {
        let item = item?;
        let from = item.path();
        let to = dst.join(item.file_name());
        let ty = fs::symlink_metadata(&from)?.file_type();
        if ty.is_dir() {
            copy_tree(&from, &to)?
        } else if ty.is_symlink() {
            symlink(fs::read_link(from)?, to)?
        } else if ty.is_file() {
            fs::copy(from, &to)?;
            let mut p = fs::metadata(&to)?.permissions();
            p.set_mode(p.mode() | 0o200);
            fs::set_permissions(to, p)?;
        }
    }
    Ok(())
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Ref(String, OwnedObjectPath);
#[derive(Clone)]
struct Row {
    reference: Ref,
    name: String,
    description: String,
    role: String,
    focused: bool,
    checked: bool,
    focusable: bool,
    showing: bool,
    selected: bool,
    enabled: bool,
    sensitive: bool,
}
impl Row {
    fn json(&self) -> Value {
        json!({"name":self.name,"description":self.description,"role":self.role,"focused":self.focused,"checked":self.checked,"focusable":self.focusable,"showing":self.showing,"selected":self.selected,"enabled":self.enabled,"sensitive":self.sensitive})
    }
}
struct Fixture {
    o: Options,
    env: BTreeMap<String, String>,
    runtime: PathBuf,
    source: PathBuf,
    dir: PathBuf,
    children: Vec<ChildProcess>,
    a11y: Option<Connection>,
    bus_address: String,
    expected_stale: HashSet<String>,
    events: Vec<Value>,
}
impl Fixture {
    fn new(o: Options) -> Result<Self> {
        fs::create_dir(&o.output)?;
        let runtime = PathBuf::from(env::var("XDG_RUNTIME_DIR")?);
        let dir = o.output.join("fixture");
        fs::create_dir(&dir)?;
        Ok(Self {
            o,
            env: env::vars().collect(),
            runtime,
            source: PathBuf::new(),
            dir,
            children: Vec::new(),
            a11y: None,
            bus_address: String::new(),
            expected_stale: HashSet::new(),
            events: Vec::new(),
        })
    }
    fn output(&self, n: &str) -> PathBuf {
        self.o.output.join(n)
    }
    fn cmd(&self, args: &[&str]) -> Result<String> {
        let out = Command::new(args[0])
            .args(&args[1..])
            .env_clear()
            .envs(&self.env)
            .output()?;
        ensure(
            out.status.success(),
            format!(
                "{} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr)
            ),
        )?;
        Ok(String::from_utf8(out.stdout)?.trim().into())
    }
    fn start(&mut self, args: &[&str], log: &str) -> Result<()> {
        let file = File::create(self.output(log))?;
        let mut cmd = Command::new(args[0]);
        cmd.args(&args[1..])
            .env_clear()
            .envs(&self.env)
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file));
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        self.children.push(ChildProcess(cmd.spawn()?));
        Ok(())
    }
    fn launcher_env(&mut self) -> Result<()> {
        let out = Command::new(&self.o.launcher)
            .arg("--print-env0")
            .output()?;
        ensure(out.status.success(), "launcher env")?;
        for part in out.stdout.split(|b| *b == 0) {
            if let Some(i) = part.iter().position(|b| *b == b'=') {
                self.env.insert(
                    String::from_utf8(part[..i].to_vec())?,
                    String::from_utf8(part[i + 1..].to_vec())?,
                );
            }
        }
        Ok(())
    }
    fn setup(&mut self) -> Result<()> {
        self.launcher_env()?;
        for (k, v) in [
            ("QT_QPA_PLATFORM", "wayland"),
            ("QT_QPA_PLATFORMTHEME", "generic"),
            ("QT_QUICK_BACKEND", "software"),
            ("QT_ACCESSIBILITY", "1"),
            ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1"),
            ("GSETTINGS_BACKEND", "memory"),
            ("WLR_BACKENDS", "headless"),
            ("WLR_RENDERER", "pixman"),
            ("WLR_LIBINPUT_NO_DEVICES", "1"),
            ("GLOBAL_MENU_ENABLED", "0"),
            ("LUMEN_FILE_SERVICE", "0"),
            ("LUMEN_LAUNCH_SCOPES", "0"),
        ] {
            self.env.insert(k.into(), v.into());
        }
        for k in [
            "NO_AT_BRIDGE",
            "GTK_A11Y",
            "AT_SPI_BUS_ADDRESS",
            "WAYLAND_DISPLAY",
            "SWAYSOCK",
            "DISPLAY",
        ] {
            self.env.remove(k);
        }
        let assets = PathBuf::from(
            self.env
                .get("LUMEN_DESKTOP_ASSETS")
                .ok_or("LUMEN_DESKTOP_ASSETS missing")?,
        );
        copy_tree(&assets, &self.dir.join("assets"))?;
        self.source = self.dir.join("assets/shell");
        let palette = load(&self.dir.join("assets/appearance/palette.json"))?;
        let appearance = Path::new(self.env.get("XDG_STATE_HOME").ok_or("XDG_STATE_HOME")?)
            .join("lumen-desktop/appearance.json");
        fs::create_dir_all(appearance.parent().ok_or("appearance parent")?)?;
        save(
            &appearance,
            &json!({"font":palette["font"],"palette":palette["light"],"preferences":{}}),
        )?;
        fs::write(self.dir.join("shell.qml"), include_str!("../shell.qml"))?;
        let bin = self.dir.join("bin");
        fs::create_dir(&bin)?;
        symlink(env::current_exe()?, bin.join("lumen-window-overview"))?;
        self.env.insert(
            "PATH".into(),
            format!(
                "{}:{}",
                bin.display(),
                self.env.get("PATH").unwrap_or(&String::new())
            ),
        );
        self.env.insert(
            "LUMEN_FIXTURE_OUT".into(),
            self.o.output.display().to_string(),
        );
        Ok(())
    }
    fn a11y(&self) -> Result<&Connection> {
        self.a11y.as_ref().ok_or_else(|| fail("AT-SPI not started"))
    }
    fn start_a11y(&mut self) -> Result<()> {
        let exe = self.o.atspi.join("libexec/at-spi-bus-launcher");
        self.start(
            &[
                exe.to_str().ok_or("AT-SPI path")?,
                "--launch-immediately",
                "--a11y=1",
                "--screen-reader=1",
            ],
            "bus.log",
        )?;
        let bus = Connection::session()?;
        check("private bus launcher ownership", || {
            has_owner(&bus, "org.a11y.Bus")
        })?;
        let proxy = Proxy::new(&bus, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus")?;
        let address: String = proxy.call("GetAddress", &())?;
        ensure(
            address.starts_with(&format!("unix:path={}/", self.runtime.display())),
            "AT-SPI bus outside private runtime",
        )?;
        self.bus_address = address.clone();
        self.env
            .insert("AT_SPI_BUS_ADDRESS".into(), address.clone());
        let props = Proxy::new(
            &bus,
            "org.a11y.Bus",
            "/org/a11y/bus",
            "org.freedesktop.DBus.Properties",
        )?;
        let all: std::collections::HashMap<String, OwnedValue> =
            props.call("GetAll", &("org.a11y.Status",))?;
        ensure(
            bool::try_from(all.get("IsEnabled").ok_or("IsEnabled")?.try_clone()?)?
                && bool::try_from(
                    all.get("ScreenReaderEnabled")
                        .ok_or("ScreenReaderEnabled")?
                        .try_clone()?,
                )?,
            "private AT-SPI status",
        )?;
        save(
            &self.output("bus-status.json"),
            &json!({"IsEnabled":true,"ScreenReaderEnabled":true}),
        )?;
        let reg = self.o.atspi.join("libexec/at-spi2-registryd");
        self.start(&[reg.to_str().ok_or("registry path")?], "registry.log")?;
        let a11y = zbus::blocking::connection::Builder::address(address.as_str())?.build()?;
        check("private accessibility registry", || {
            has_owner(&a11y, "org.a11y.atspi.Registry")
        })?;
        self.a11y = Some(a11y);
        Ok(())
    }
    fn start_sway(&mut self) -> Result<()> {
        fs::write(
            self.output("sway.conf"),
            "output HEADLESS-1 resolution 1000x1000\nseat seat0 fallback true\n",
        )?;
        let sway = self.o.sway.clone();
        let config = self.output("sway.conf");
        self.start(
            &[
                sway.to_str().ok_or("sway path")?,
                "--unsupported-gpu",
                "-c",
                config.to_str().ok_or("config path")?,
            ],
            "sway.log",
        )?;
        let runtime = self.runtime.clone();
        let socket = wait("private compositor", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|v| v.ok())
                .map(|v| v.path())
                .find(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("sway-ipc."))
                }))
        })?;
        self.env
            .insert("SWAYSOCK".into(), socket.display().to_string());
        let display = wait("private display", || {
            Ok(fs::read_dir(&runtime)?
                .filter_map(|v| v.ok())
                .map(|v| v.file_name().to_string_lossy().to_string())
                .find(|n| n.starts_with("wayland-") && !n.ends_with(".lock")))
        })?;
        self.env.insert("WAYLAND_DISPLAY".into(), display);
        self.start(&["wtype", "-s", "30000"], "keyboard.log")?;
        thread::sleep(Duration::from_millis(100));
        Ok(())
    }
    fn shell(&mut self) -> Result<()> {
        let qs = self.o.quickshell.clone();
        let dir = self.dir.clone();
        self.start(
            &[
                qs.to_str().ok_or("quickshell path")?,
                "-p",
                dir.to_str().ok_or("fixture path")?,
            ],
            "fixture.log",
        )?;
        check("fixture window", || {
            Ok(self.ipc("status")?["visible"] == true)
        })
    }
    fn ipc(&self, method: &str) -> Result<Value> {
        let s = self.cmd(&[
            self.o.quickshell.to_str().ok_or("quickshell path")?,
            "-p",
            self.dir.to_str().ok_or("fixture path")?,
            "ipc",
            "call",
            "a11yFixture",
            method,
        ])?;
        Ok(serde_json::from_str(&s).unwrap_or(Value::String(s)))
    }
    fn desktop_ipc(&self, method: &str, values: &[&str]) -> Result<Value> {
        let mut args = vec![
            self.o.quickshell.to_str().ok_or("quickshell path")?,
            "-p",
            self.source.to_str().ok_or("source path")?,
            "ipc",
            "call",
            "desktop",
            method,
        ];
        args.extend(values);
        let s = self.cmd(&args)?;
        Ok(serde_json::from_str(&s).unwrap_or(Value::String(s)))
    }
    fn keys(&self, keys: &[&str]) -> Result<()> {
        let mut args = vec!["wtype", "-s", "200"];
        args.extend(keys);
        self.cmd(&args)?;
        Ok(())
    }
    fn tree(&self) -> Result<Vec<Row>> {
        let mut pending = VecDeque::from([Ref(
            "org.a11y.atspi.Registry".into(),
            OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root")?,
        )]);
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        while let Some(r) = pending.pop_front() {
            if rows.len() >= 500 {
                break;
            }
            if r.1.as_str() == "/org/a11y/atspi/null" || !seen.insert(r.clone()) {
                continue;
            }
            let result = (|| -> Result<(Row, Vec<(String, OwnedObjectPath)>)> {
                let item = Proxy::new(
                    self.a11y()?,
                    r.0.as_str(),
                    r.1.as_str(),
                    "org.a11y.atspi.Accessible",
                )?;
                let props = Proxy::new(
                    self.a11y()?,
                    r.0.as_str(),
                    r.1.as_str(),
                    "org.freedesktop.DBus.Properties",
                )?;
                let name: OwnedValue = props.call("Get", &("org.a11y.atspi.Accessible", "Name"))?;
                let description: OwnedValue =
                    props.call("Get", &("org.a11y.atspi.Accessible", "Description"))?;
                let states: Vec<u32> = item.call("GetState", &())?;
                let has = |i: usize| {
                    states
                        .get(i / 32)
                        .is_some_and(|v| *v & (1 << (i % 32)) != 0)
                };
                let row = Row {
                    reference: r.clone(),
                    name: String::try_from(name)?,
                    description: String::try_from(description)?,
                    role: item.call("GetRoleName", &())?,
                    focused: has(12),
                    checked: has(4),
                    focusable: has(11),
                    showing: has(25),
                    selected: has(23),
                    enabled: has(8),
                    sensitive: has(24),
                };
                let children = item.call("GetChildren", &())?;
                Ok((row, children))
            })();
            match result {
                Ok((row, children)) => {
                    rows.push(row);
                    pending.extend(children.into_iter().map(|(n, p)| Ref(n, p)));
                }
                Err(e) => {
                    let mut log = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(self.output("protocol-errors.log"))?;
                    writeln!(log, "{} {}: {e}", r.0, r.1)?;
                }
            }
        }
        Ok(rows)
    }
    fn named(&self, name: &str, showing: bool) -> Result<Option<Row>> {
        let rows = self.tree()?;
        save(
            &self.output("last-tree.json"),
            &json!(rows.iter().map(Row::json).collect::<Vec<_>>()),
        )?;
        Ok(rows
            .into_iter()
            .find(|r| r.name == name && (!showing || r.showing)))
    }
    fn must(&self, name: &str, showing: bool) -> Result<Row> {
        wait(name, || self.named(name, showing))
    }
    fn proxy<'a>(&'a self, r: &'a Ref, interface: &'a str) -> Result<Proxy<'a>> {
        Ok(Proxy::new(
            self.a11y()?,
            r.0.as_str(),
            r.1.as_str(),
            interface,
        )?)
    }
    fn focus(&self, r: &Ref) -> Result<()> {
        let ok: bool = self
            .proxy(r, "org.a11y.atspi.Component")?
            .call("GrabFocus", &())?;
        ensure(ok, "AT-SPI GrabFocus failed")
    }
    fn action(&self, r: &Ref) -> Result<bool> {
        Ok(self
            .proxy(r, "org.a11y.atspi.Action")?
            .call("DoAction", &(0_i32,))?)
    }
    fn disabled_action(&self, r: &Ref) -> Result<()> {
        let _ = self
            .proxy(r, "org.a11y.atspi.Action")?
            .call_method("DoAction", &(0_i32,))?;
        Ok(())
    }
    fn edit(&self, r: &Ref, text: &str) -> Result<()> {
        let _ = self
            .proxy(r, "org.a11y.atspi.EditableText")?
            .call_method("SetTextContents", &(text,))?;
        Ok(())
    }
    fn text(&self, r: &Ref) -> Result<String> {
        Ok(self
            .proxy(r, "org.a11y.atspi.Text")?
            .call("GetText", &(0_i32, -1_i32))?)
    }
    fn subtree_names(&self, r: &Ref) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut pending = VecDeque::from([r.clone()]);
        let mut seen = HashSet::new();
        while let Some(r) = pending.pop_front() {
            if !seen.insert(r.clone()) {
                continue;
            }
            let item = self.proxy(&r, "org.a11y.atspi.Accessible")?;
            let props = self.proxy(&r, "org.freedesktop.DBus.Properties")?;
            let name: OwnedValue = props.call("Get", &("org.a11y.atspi.Accessible", "Name"))?;
            result.push(String::try_from(name)?);
            let children: Vec<(String, OwnedObjectPath)> = item.call("GetChildren", &())?;
            for (n, p) in children {
                if p.as_str() != "/org/a11y/atspi/null" {
                    pending.push_back(Ref(n, p));
                }
            }
        }
        Ok(result)
    }
    fn stale_action(&mut self, r: &Ref) -> Result<()> {
        self.expected_stale.insert(r.1.to_string());
        let mut record = json!({"path":r.1.as_str()});
        match self.action(r) {
            Ok(v) => record["accepted"] = json!(v),
            Err(e) => {
                let text = e.to_string();
                ensure(
                    text.contains("UnknownObject") || text.contains("UnknownMethod"),
                    format!("Unexpected stale action error: {text}"),
                )?;
                record["error"] = json!(if text.contains("UnknownObject") {
                    "org.freedesktop.DBus.Error.UnknownObject"
                } else {
                    "org.freedesktop.DBus.Error.UnknownMethod"
                });
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.output("stale-actions.jsonl"))?;
        writeln!(file, "{record}")?;
        Ok(())
    }
    fn tree_artifact(&self, name: &str) -> Result<()> {
        save(
            &self.output(name),
            &json!(self.tree()?.iter().map(Row::json).collect::<Vec<_>>()),
        )
    }
}
fn has_owner(bus: &Connection, name: &str) -> Result<bool> {
    let dbus = Proxy::new(
        bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    Ok(dbus.call("NameHasOwner", &(name,))?)
}
fn overview_helper() -> Result<()> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let v: Value = serde_json::from_str(&line)?;
    let output = PathBuf::from(env::var("LUMEN_FIXTURE_OUT")?);
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(output.join("group-requests.jsonl"))?;
    writeln!(log, "{v}")?;
    println!(
        "{}",
        json!({"windows":[{"id":101,"pid":202,"title":"Fixture document","app":"Editor","application":"org.example.Editor","workspace":"1","group":"Editor"}],"workspaces":[],"outputs":[],"assignments":{},"autoReorder":false,"revision":1,"sessionEpoch":"fixture-epoch","windowGroups":{"enabled":true,"locked":false,"revision":7,"groups":[{"id":1,"members":[{"id":101,"pid":202}],"selected":true,"suspended":false},{"id":2,"members":[{"id":102,"pid":203}],"selected":false,"suspended":false}]}})
    );
    Ok(())
}
fn calendar_helper() -> Result<()> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let _: Value = serde_json::from_str(&line)?;
    println!(
        "{}",
        json!({"configured":false,"events":[],"monthEvents":[],"days":[],"total":0,"providers":[]})
    );
    Ok(())
}
fn main() -> Result<()> {
    let name = env::args_os().next().unwrap_or_default();
    let name = Path::new(&name)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    if name == "lumen-window-overview" {
        return overview_helper();
    }
    if name == "lumen-calendar" {
        return calendar_helper();
    }
    let o = Options::parse()?;
    if !o.inside {
        return private_session(&o);
    }
    let mut f = Fixture::new(o)?;
    f.run()
}
impl Fixture {
    fn group_actions(&self) -> Result<Vec<Value>> {
        let path = self.output("group-requests.jsonl");
        if !path.exists() {
            return Ok(vec![]);
        }
        Ok(fs::read_to_string(path)?
            .lines()
            .filter_map(|s| serde_json::from_str::<Value>(s).ok())
            .filter(|v| v["action"] == "window-groups")
            .collect())
    }
    fn native_windows(&self) -> Result<BTreeMap<String, Value>> {
        fn walk(v: &Value, m: &mut BTreeMap<String, Value>) {
            if v["app_id"]
                .as_str()
                .is_some_and(|s| s.starts_with("accessibility-dock-"))
            {
                if let Some(n) = v["name"].as_str() {
                    m.insert(n.into(), v.clone());
                }
            }
            for key in ["nodes", "floating_nodes"] {
                if let Some(a) = v[key].as_array() {
                    for x in a {
                        walk(x, m);
                    }
                }
            }
        }
        let tree: Value = serde_json::from_str(&self.cmd(&["swaymsg", "-r", "-t", "get_tree"])?)?;
        let mut m = BTreeMap::new();
        walk(&tree, &mut m);
        Ok(m)
    }
    fn run(&mut self) -> Result<()> {
        self.setup()?;
        self.start_a11y()?;
        self.start_sway()?;
        self.shell()?;
        self.keys(&["-k", "Tab"])?;
        // Register the wire event subscription before exercising focus and checked states.
        let registry = Proxy::new(
            self.a11y()?,
            "org.a11y.atspi.Registry",
            "/org/a11y/atspi/registry",
            "org.a11y.atspi.Registry",
        )?;
        let _: () = registry.call(
            "RegisterEvent",
            &("object:state-changed", Vec::<String>::new()),
        )?;
        let event_bus = self.a11y()?.clone();
        let event_file = self.output("state-events.jsonl");
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.a11y.atspi.Event.Object")?
            .member("StateChanged")?
            .build();
        let mut signals =
            zbus::blocking::MessageIterator::for_match_rule(rule, &event_bus, Some(256))?;
        thread::spawn(move || {
            for msg in &mut signals {
                if let Ok(msg) = msg {
                    if let Ok(body) = msg.body().deserialize::<zbus::zvariant::Structure>() {
                        let fields = body.fields();
                        if fields.len() >= 2 {
                            let detail = String::try_from(fields[0].try_clone().unwrap());
                            let value = i32::try_from(fields[1].try_clone().unwrap());
                            if let (Ok(detail), Ok(value)) = (detail, value) {
                                if let Ok(mut log) = OpenOptions::new()
                                    .create(true)
                                    .append(true)
                                    .open(&event_file)
                                {
                                    let _ =
                                        writeln!(log, "{}", json!({"detail":detail,"value":value}));
                                }
                            }
                        }
                    }
                }
            }
        });
        self.must("Focus, Off", false)?;
        self.tree_artifact("initial-tree.json")?;
        let focus = self.must("Focus, Off", false)?;
        ensure(
            focus.role == "check box" && !focus.checked,
            "Focus initial role/state",
        )?;
        self.focus(&focus.reference)?;
        check("Focus keyboard focus", || {
            Ok(self.must("Focus, Off", false)?.focused)
        })?;
        self.keys(&["-k", "space"])?;
        check("Space activates Focus", || {
            Ok(self.ipc("status")?["focus"] == true)
        })?;
        check("checked accessibility state", || {
            Ok(self.must("Focus, Work", false)?.checked)
        })?;
        let toggle = self.must("Show Calendar", false)?;
        ensure(
            toggle.role == "check box" && !toggle.checked,
            "Settings toggle initial role/state",
        )?;
        self.focus(&toggle.reference)?;
        self.keys(&["-k", "space"])?;
        check("settings toggle checked state", || {
            Ok(self.must("Show Calendar", false)?.checked)
        })?;
        for (label, key) in [
            ("Allow critical notifications", "allowCritical"),
            ("Chat", "allowedApps"),
        ] {
            let item = self.must(label, false)?;
            ensure(
                item.role == "check box" && !item.checked,
                format!("{label} initial state"),
            )?;
            self.focus(&item.reference)?;
            self.keys(&["-k", "space"])?;
            check(&format!("Focus policy checkbox {label}"), || {
                Ok(self.must(label, false)?.checked)
            })?;
            let status = self.ipc("status")?;
            if key == "allowCritical" {
                ensure(status["profile"][key] == true, "critical policy")?
            } else {
                ensure(
                    status["profile"][key] == json!(["chat"]),
                    "allowed application policy",
                )?
            }
        }
        check("checked state event", || {
            Ok(self
                .state_events()?
                .iter()
                .any(|v| v["detail"] == "checked" && v["value"] == 1))
        })?;
        check("focused state event", || {
            Ok(self
                .state_events()?
                .iter()
                .any(|v| v["detail"] == "focused" && v["value"] == 1))
        })?;
        let unavailable = self.must("Unavailable command", false)?;
        let _ = self.disabled_action(&unavailable.reference)?;
        ensure(
            self.ipc("status")?["disabledPresses"] == 0,
            "Disabled command activated",
        )?;
        ensure(
            self.must("For one hour", false)?.role == "push button",
            "Focus duration role",
        )?;
        ensure(
            ["text", "entry"].contains(&self.must("Focus profile name", false)?.role.as_str()),
            "Profile name role",
        )?;
        self.ipc("close")?;
        self.ipc("widgetsShow")?;
        let group = self.must("Calendar widget 1", false)?;
        ensure(
            ["panel", "grouping"].contains(&group.role.as_str()),
            "Widget row group",
        )?;
        let names = self.subtree_names(&group.reference)?;
        let expected = [
            "Size for Calendar widget 1",
            "Display for Calendar widget 1",
            "Calendar layout for Calendar widget 1",
            "Remove Calendar widget 1",
        ];
        ensure(
            names
                .iter()
                .filter(|s| expected.contains(&s.as_str()))
                .map(String::as_str)
                .collect::<Vec<_>>()
                == expected,
            "Widget child order",
        )?;
        ensure(
            self.must("Add Calendar", false)?.role == "push button",
            "Widget add button",
        )?;
        let size = self.must("Size for Calendar widget 1", false)?;
        ensure(size.role == "combo box", "Widget size role")?;
        self.focus(&size.reference)?;
        self.keys(&["-k", "Down"])?;
        check("actual widget choice via keyboard", || {
            Ok(self.ipc("widgetsStatus")?[0]["size"] == "medium")
        })?;
        self.keys(&["-k", "Tab"])?;
        check("widget keyboard order", || {
            Ok(self.must("Display for Calendar widget 1", false)?.focused)
        })?;
        let remove = self.must("Remove Calendar widget 1", false)?;
        ensure(remove.role == "push button", "Widget remove role")?;
        self.focus(&remove.reference)?;
        ensure(
            self.action(&remove.reference)?,
            "Widget removal AT-SPI action",
        )?;
        check("AT-SPI removes captured widget", || {
            Ok(self
                .ipc("widgetsStatus")?
                .as_array()
                .is_some_and(|a| a.len() == 1 && a[0]["id"] == "clock-two"))
        })?;
        check("removed widget restores meaningful focus", || {
            Ok(self.must("Add Calendar", false)?.focused)
        })?;
        self.tree_artifact("widget-controls-tree.json")?;
        self.ipc("widgetsClose")?;
        self.ipc("groupsShow")?;
        let manage = self.must("Manage window groups", false)?;
        ensure(self.action(&manage.reference)?, "Manage group action")?;
        let select = self.must("Select window group 1", false)?;
        self.focus(&select.reference)?;
        self.keys(&["-k", "Return"])?;
        check("Return dispatches captured group operation", || {
            Ok(!self.group_actions()?.is_empty())
        })?;
        let expected = json!({"action":"window-groups","groupAction":"select","groupId":1,"groupRevision":7,"sessionEpoch":"fixture-epoch","id":101,"pid":202});
        ensure(
            self.group_actions()?[0] == expected,
            "First group intent identity",
        )?;
        check("group request completes", || {
            Ok(self.ipc("groupsStatus")?["busy"] == false)
        })?;
        let select = self.must("Select window group 1", false)?;
        ensure(
            self.action(&select.reference)?,
            "Second group AT-SPI action",
        )?;
        check("AT-SPI dispatches fresh captured group operation", || {
            Ok(self.group_actions()?.len() == 2)
        })?;
        ensure(
            self.group_actions()?[1] == expected,
            "Second group intent identity",
        )?;
        check("second group request completes", || {
            Ok(self.ipc("groupsStatus")?["busy"] == false)
        })?;
        let select = self.must("Select window group 1", false)?;
        self.focus(&select.reference)?;
        self.keys(&["-k", "space"])?;
        check("Space press captures first group", || {
            Ok(self.group_actions()?.len() == 3)
        })?;
        check("pressed group operation completes", || {
            Ok(self.ipc("groupsStatus")?["busy"] == false)
        })?;
        let choice = self.must("Window group", false)?;
        self.focus(&choice.reference)?;
        self.keys(&["-k", "Down"])?;
        let select = self.must("Select window group 2", false)?;
        ensure(self.action(&select.reference)?, "Group 2 AT-SPI action")?;
        check(
            "assistive activation does not reuse prior pressed group",
            || Ok(self.group_actions()?.len() == 4),
        )?;
        let mut expected2 = expected.clone();
        expected2["groupId"] = json!(2);
        ensure(
            self.group_actions()?[3] == expected2,
            "Fresh group identity",
        )?;
        let close = self.must("Close window groups", false)?;
        self.focus(&close.reference)?;
        self.keys(&["-k", "Return"])?;
        check("group dialog restores opener focus", || {
            Ok(self.must("Manage window groups", false)?.focused)
        })?;
        self.tree_artifact("owned-controls-tree.json")?;
        let manage = self.must("Manage window groups", false)?;
        ensure(self.action(&manage.reference)?, "Reopen group dialog")?;
        check("group dialog initial keyboard focus", || {
            Ok(self.must("Window group", false)?.focused)
        })?;
        self.ipc("groupsClose")?;
        check("hidden Overview closes nested group dialog", || {
            Ok(!self
                .tree()?
                .iter()
                .any(|r| r.showing && r.name == "Window Groups" && r.role == "dialog"))
        })?;
        self.ipc("sessionShow")?;
        let dialog = self.must("Restart confirmation", false)?;
        ensure(
            dialog.role == "dialog"
                && dialog.description.contains("Fixture editor")
                && dialog.description.contains("Cancel"),
            "Session dialog context",
        )?;
        let names = self.subtree_names(&dialog.reference)?;
        ensure(
            names.iter().position(|s| s == "Cancel") < names.iter().position(|s| s == "Waiting…"),
            "Session read order",
        )?;
        self.tree_artifact("session-controls-tree.json")?;
        let waiting = self.must("Waiting…", false)?;
        ensure(
            !waiting.enabled && !waiting.sensitive,
            "Waiting action enabled",
        )?;
        let _ = self.disabled_action(&waiting.reference)?;
        ensure(
            self.ipc("sessionStatus")?["finalActions"] == 0,
            "Waiting action executed",
        )?;
        let cancel = self.must("Cancel", false)?;
        ensure(cancel.role == "push button", "Cancel role")?;
        self.focus(&cancel.reference)?;
        self.keys(&["-k", "Return"])?;
        check("keyboard cancels waiting session", || {
            Ok(self.ipc("sessionStatus")?["visible"] == false)
        })?;
        let status = self.ipc("sessionStatus")?;
        ensure(
            status["commands"]
                .as_array()
                .is_some_and(|a| a.last().is_some_and(|v| v["action"] == "session_cancel"))
                && status["finalActions"] == 0,
            "Session cancelled without host action",
        )?;
        self.run_actions()?;
        self.run_results()?;
        self.run_dock()?;
        if self.o.full_shell {
            self.run_full_shell()?;
        }
        self.finish()
    }
    fn state_events(&self) -> Result<Vec<Value>> {
        let path = self.output("state-events.jsonl");
        if !path.exists() {
            return Ok(vec![]);
        }
        Ok(fs::read_to_string(path)?
            .lines()
            .filter_map(|s| serde_json::from_str(s).ok())
            .collect())
    }
}
impl Fixture {
    fn run_actions(&mut self) -> Result<()> {
        self.ipc("actionsShow")?;
        let card = self.must("Fixture, Protocol subject", false)?;
        ensure(card.role == "push button", "Notification card role")?;
        ensure(
            self.action(&card.reference)?,
            "Notification primary AT-SPI action",
        )?;
        check("AT-SPI invokes notification primary action", || {
            Ok(self.ipc("actionsStatus")?["invocations"]
                .as_array()
                .is_some_and(|a| a.len() == 1))
        })?;
        self.focus(&card.reference)?;
        self.keys(&["-k", "Return"])?;
        check("keyboard notification primary action", || {
            Ok(self.ipc("actionsStatus")?["invocations"]
                .as_array()
                .is_some_and(|a| a.len() == 2))
        })?;
        ensure(
            self.ipc("actionsStatus")?["invocations"]
                == json!([{"id":77,"action":"default"},{"id":77,"action":"default"}]),
            "Notification action identity",
        )?;
        self.ipc("actionsStack")?;
        let stack = self.must(
            "Fixture, Protocol subject, 3 notifications. Expand stack",
            false,
        )?;
        ensure(self.action(&stack.reference)?, "Stack expansion AT-SPI")?;
        check("assistive stack expansion", || {
            Ok(self.ipc("actionsStatus")?["expansions"] == 1)
        })?;
        ensure(
            self.ipc("actionsStatus")?["invocations"]
                .as_array()
                .is_some_and(|a| a.len() == 2),
            "Stack invoked primary",
        )?;
        let preview = self.must("Toggle preview", false)?;
        ensure(
            preview.role == "check box" && !preview.checked,
            "Preview initial role/state",
        )?;
        let last_filter = self.must("Clipboard", false)?;
        self.focus(&last_filter.reference)?;
        self.keys(&["-k", "Tab"])?;
        check("Tab reaches preview toolbar control", || {
            Ok(self.must("Toggle preview", false)?.focused)
        })?;
        self.keys(&["-k", "Tab"])?;
        check("Tab reaches grid toolbar control", || {
            Ok(self.must("Application grid", false)?.focused)
        })?;
        self.keys(&["-M", "shift", "-k", "Tab", "-m", "shift"])?;
        check("reverse Tab returns to preview", || {
            Ok(self.must("Toggle preview", false)?.focused)
        })?;
        self.keys(&["-k", "space"])?;
        check("Space toggles launcher preview", || {
            Ok(self.ipc("actionsStatus")?["preview"] == true)
        })?;
        check("preview check state through AT-SPI", || {
            Ok(self.must("Toggle preview", false)?.checked)
        })?;
        ensure(
            self.must("Toggle preview", false)?.focused,
            "Preview lost focus",
        )?;
        self.keys(&["-k", "space"])?;
        check("second Space reverses preview", || {
            Ok(self.ipc("actionsStatus")?["preview"] == false)
        })?;
        ensure(self.action(&preview.reference)?, "Assistive preview toggle")?;
        check("assistive preview toggle", || {
            Ok(self.ipc("actionsStatus")?["preview"] == true)
        })?;
        let grid = self.must("Application grid", false)?;
        ensure(
            grid.role == "check box" && !grid.checked,
            "Grid initial role/state",
        )?;
        ensure(self.action(&grid.reference)?, "Assistive grid action")?;
        check("assistive grid toggle", || {
            Ok(self.ipc("actionsStatus")?["grid"] == true)
        })?;
        check("grid checked state", || {
            Ok(self.must("Application grid", false)?.checked)
        })?;
        self.ipc("actionsDisableApps")?;
        check("disabled provider disables grid action", || {
            Ok(!self.must("Application grid", false)?.enabled)
        })?;
        let _ = self.disabled_action(&grid.reference)?;
        ensure(
            self.ipc("actionsStatus")?["grid"] == false
                && !self.must("Application grid", false)?.checked,
            "Disabled grid action changed state",
        )?;
        self.tree_artifact("shell-action-controls-tree.json")?;
        self.ipc("actionsEnableApps")?;
        check("restored provider enables grid action", || {
            Ok(self.must("Application grid", false)?.enabled)
        })?;
        self.focus(&grid.reference)?;
        self.keys(&["-k", "Escape"])?;
        check("Escape closes launcher from toolbar focus", || {
            Ok(self.ipc("actionsStatus")?["visible"] == false)
        })?;
        Ok(())
    }
    fn run_results(&mut self) -> Result<()> {
        self.ipc("resultsSetup")?;
        check("visible launcher results fixture", || {
            Ok(self.ipc("resultsStatus")?["visible"] == true)
        })?;
        thread::sleep(Duration::from_millis(300));
        let query = self.must(
            "Search applications, files, actions and clipboard history",
            false,
        )?;
        self.focus(&query.reference)?;
        self.edit(&query.reference, "Fixture")?;
        save(
            &self.output("results-state.json"),
            &self.ipc("resultsStatus")?,
        )?;
        let initial = self.named("Fixture Alpha", false)?;
        save(
            &self.output("result-row-protocol.json"),
            &initial.as_ref().map(Row::json).unwrap_or(Value::Null),
        )?;
        let alpha = self.must("Fixture Alpha", true)?;
        ensure(
            alpha.role == "push button" && alpha.description == "Synthetic app",
            "Result row protocol",
        )?;
        self.focus(&alpha.reference)?;
        self.keys(&["-k", "Return"])?;
        check("Return launches captured app once", || {
            Ok(self.ipc("resultsStatus")?["launches"] == json!(["fixture.alpha"]))
        })?;
        self.stale_action(&alpha.reference)?;
        thread::sleep(Duration::from_millis(200));
        ensure(
            self.ipc("resultsStatus")?["launches"] == json!(["fixture.alpha"]),
            "Stale result launched bystander",
        )?;
        self.ipc("resultsSetup")?;
        thread::sleep(Duration::from_millis(300));
        let query = self.must(
            "Search applications, files, actions and clipboard history",
            false,
        )?;
        self.edit(&query.reference, "Fixture")?;
        let old_alpha = self.must("Fixture Alpha", true)?;
        self.focus(&old_alpha.reference)?;
        self.ipc("resultsRemove")?;
        check("selected app result removed", || {
            Ok(self.ipc("resultsStatus")?["results"] == json!(["fixture.beta"]))
        })?;
        self.stale_action(&old_alpha.reference)?;
        thread::sleep(Duration::from_millis(200));
        ensure(
            self.ipc("resultsStatus")?["launches"] == json!(["fixture.alpha"]),
            "Removed result launched bystander",
        )?;
        let beta = self.must("Fixture Beta", true)?;
        ensure(self.action(&beta.reference)?, "Remaining result AT-SPI")?;
        check("assistive result targets remaining app once", || {
            Ok(self.ipc("resultsStatus")?["launches"] == json!(["fixture.alpha", "fixture.beta"]))
        })?;
        self.ipc("resultsSetup")?;
        thread::sleep(Duration::from_millis(300));
        self.ipc("resultsGrid")?;
        let grid_alpha = self.must("Fixture Alpha", true)?;
        ensure(grid_alpha.role == "push button", "Grid result role")?;
        self.focus(&grid_alpha.reference)?;
        self.keys(&["-k", "space"])?;
        check("grid keyboard action targets captured app once", || {
            Ok(self.ipc("resultsStatus")?["launches"]
                == json!(["fixture.alpha", "fixture.beta", "fixture.alpha"]))
        })?;
        Ok(())
    }
    fn run_dock(&mut self) -> Result<()> {
        for (app, title) in [
            ("source", "Dock Source"),
            ("other", "Dock Other"),
            ("bystander", "Dock Bystander"),
        ] {
            self.start(
                &[
                    "foot",
                    &format!("--app-id=accessibility-dock-{app}"),
                    &format!("--title={title}"),
                    "sleep",
                    "180",
                ],
                &format!("dock-{app}.log"),
            )?;
        }
        check("three disposable Dock windows", || {
            Ok(self.native_windows()?.len() == 3)
        })?;
        check("Dock native foreign-toplevel handles", || {
            Ok(self.ipc("dockStatus")?["windows"]
                .as_array()
                .is_some_and(|a| a.len() == 2))
        })?;
        self.ipc("dockOpen")?;
        check("Dock menu window", || {
            Ok(self.ipc("dockStatus")?["open"] == true)
        })?;
        thread::sleep(Duration::from_millis(300));
        let row = self.named("Open New", false)?;
        save(
            &self.output("dock-row-protocol.json"),
            &row.as_ref().map(Row::json).unwrap_or(Value::Null),
        )?;
        self.keys(&["-k", "Down", "-k", "Down"])?;
        check("keyboard captures first native menu window", || {
            Ok(self.ipc("dockStatus")?["index"] == 1)
        })?;
        let captured = self.ipc("dockStatus")?["windows"][0]
            .as_str()
            .ok_or("captured Dock window")?
            .to_string();
        let con = self
            .native_windows()?
            .get(&captured)
            .ok_or("captured native window")?["id"]
            .as_i64()
            .ok_or("captured con id")?;
        self.cmd(&["swaymsg", &format!("[con_id={con}]"), "kill"])?;
        check("selected native window closes", || {
            Ok(self.ipc("dockStatus")?["windows"]
                .as_array()
                .is_some_and(|a| a.len() == 1))
        })?;
        let bystander = self
            .native_windows()?
            .get("Dock Bystander")
            .ok_or("Dock bystander")?["id"]
            .as_i64()
            .ok_or("bystander id")?;
        self.cmd(&["swaymsg", &format!("[con_id={bystander}]"), "focus"])?;
        self.keys(&["-k", "Return"])?;
        check("stale Dock confirmation resolves", || {
            Ok(self.ipc("dockStatus")?["open"] == false)
        })?;
        ensure(
            self.ipc("dockStatus")?["focuses"] == json!([]),
            "Stale Dock window focus",
        )?;
        ensure(
            self.native_windows()?
                .get("Dock Bystander")
                .ok_or("bystander")?["focused"]
                == true,
            "Bystander lost focus",
        )?;
        self.ipc("dockOpen")?;
        let open_new = self.must("Open New", true)?;
        ensure(
            open_new.role == "menu item" && open_new.enabled,
            "Dock open-new role/state",
        )?;
        let unavailable = self.must("Request Quit", true)?;
        ensure(
            !unavailable.enabled && !unavailable.sensitive,
            "Dock unavailable action enabled",
        )?;
        let _ = self.disabled_action(&unavailable.reference)?;
        ensure(
            self.ipc("dockStatus")?["quits"] == json!([]),
            "Unavailable Dock quit executed",
        )?;
        let remaining = self.ipc("dockStatus")?["windows"][0]
            .as_str()
            .ok_or("remaining Dock window")?
            .to_string();
        let target = self.must(&remaining, true)?;
        ensure(target.role == "menu item", "Native Dock item role")?;
        ensure(self.action(&target.reference)?, "Native Dock window AT-SPI")?;
        check("assistive Dock window action once", || {
            Ok(self.ipc("dockStatus")?["focuses"] == json!([remaining]))
        })?;
        check("actual native captured Dock window focus", || {
            Ok(self
                .native_windows()?
                .get(&remaining)
                .is_some_and(|v| v["focused"] == true))
        })?;
        self.stale_action(&target.reference)?;
        thread::sleep(Duration::from_millis(200));
        ensure(
            self.ipc("dockStatus")?["focuses"] == json!([remaining]),
            "Stale Dock window action replay",
        )?;
        self.ipc("dockOpen")?;
        let open_new = self.must("Open New", true)?;
        ensure(self.action(&open_new.reference)?, "Current Dock app action")?;
        check("assistive Dock app action once", || {
            Ok(self.ipc("dockStatus")?["activations"] == json!(["fixture-dock"]))
        })?;
        self.ipc("dockOpen")?;
        check("fresh Dock menu", || {
            Ok(self.ipc("dockStatus")?["open"] == true)
        })?;
        self.stale_action(&open_new.reference)?;
        thread::sleep(Duration::from_millis(200));
        ensure(
            self.ipc("dockStatus")?["activations"] == json!(["fixture-dock"]),
            "Old Dock action replayed",
        )?;
        self.keys(&["-k", "Down", "-k", "Return"])?;
        check("keyboard Dock app action once", || {
            Ok(self.ipc("dockStatus")?["activations"] == json!(["fixture-dock", "fixture-dock"]))
        })?;
        thread::sleep(Duration::from_millis(200));
        ensure(
            self.ipc("dockStatus")?["activations"] == json!(["fixture-dock", "fixture-dock"]),
            "Duplicate keyboard Dock action",
        )?;
        save(
            &self.output("dock-result-identity.json"),
            &json!({"dock":self.ipc("dockStatus")?,"launcher":self.ipc("resultsStatus")?}),
        )?;
        for v in self.native_windows()?.values() {
            if let Some(id) = v["id"].as_i64() {
                self.cmd(&["swaymsg", &format!("[con_id={id}]"), "kill"])?;
            }
        }
        Ok(())
    }
    fn run_full_shell(&mut self) -> Result<()> {
        self.ipc("close")?;
        let documents = self.output("Documents");
        fs::create_dir(&documents)?;
        fs::write(
            documents.join("example.txt"),
            "Accessibility fixture document",
        )?;
        self.env.insert(
            "LUMEN_SEARCH_ROOTS".into(),
            json!([documents.display().to_string()]).to_string(),
        );
        let bin = self.output("bin");
        fs::create_dir(&bin)?;
        symlink(env::current_exe()?, bin.join("lumen-calendar"))?;
        self.env.insert(
            "PATH".into(),
            format!(
                "{}:{}",
                bin.display(),
                self.env.get("PATH").unwrap_or(&String::new())
            ),
        );
        let qs = self.o.quickshell.clone();
        let src = self.source.clone();
        self.start(
            &[
                qs.to_str().ok_or("quickshell path")?,
                "-p",
                src.to_str().ok_or("source path")?,
            ],
            "desktop.log",
        )?;
        check("full owned desktop", || {
            Ok(self.desktop_ipc("status", &[])?["screens"]
                .as_array()
                .is_some_and(|a| !a.is_empty()))
        })?;
        self.desktop_ipc("settings", &["Appearance"])?;
        let search = self.must("Search settings", false)?;
        ensure(
            ["text", "entry"].contains(&search.role.as_str()),
            "Settings search role",
        )?;
        self.focus(&search.reference)?;
        self.edit(&search.reference, "appearance")?;
        ensure(
            self.text(&search.reference)? == "appearance",
            "Settings editable text",
        )?;
        self.edit(&search.reference, "")?;
        self.desktop_ipc("files", &[documents.to_str().ok_or("documents path")?])?;
        check("owned Files fixture", || {
            Ok(self.desktop_ipc("filesStatus", &[])?["path"] == documents.display().to_string())
        })?;
        let view = self.must("Files view", false)?;
        ensure(view.role == "combo box", "Files view selector role")?;
        self.focus(&view.reference)?;
        self.keys(&["-k", "Tab"])?;
        check("Files real keyboard traversal", || {
            Ok(self.tree()?.iter().any(|r| r.focused))
        })?;
        self.tree_artifact("owned-windows-tree.json")?;
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        for log in ["fixture.log", "desktop.log"] {
            if log == "desktop.log" && !self.o.full_shell {
                continue;
            }
            let data = fs::read_to_string(self.output(log))?;
            for line in data
                .lines()
                .filter(|l| l.contains("qt.accessibility.atspi:"))
            {
                let marker = "Could not find accessible on path: \"";
                ensure(
                    line.starts_with("  WARN qt.accessibility.atspi: ") && line.contains(marker),
                    format!("Unexpected AT-SPI warning: {line}"),
                )?;
                let path = line
                    .split(marker)
                    .nth(1)
                    .and_then(|v| v.strip_suffix('"'))
                    .ok_or("AT-SPI warning path")?;
                ensure(
                    self.expected_stale.contains(path),
                    format!("Unexpected stale path {path}"),
                )?;
            }
            for bad in [
                "Failed to load configuration",
                "ReferenceError:",
                "TypeError:",
            ] {
                ensure(!data.contains(bad), format!("{log}: {bad}"))?;
            }
        }
        self.tree_artifact("final-tree.json")?;
        let mut checks = vec![
            "private AT-SPI bus",
            "actual roles and names",
            "protocol focus reflected in state",
            "physical Space activates controls",
            "checked state propagated",
            "widget row role/context/read order and keyboard selection",
            "removed widget restores focus",
            "Overview group Return and AT-SPI dispatch exact fresh intents",
            "Overview dialog returns focus to opener",
            "session dialog role/context/read order and disabled waiting action",
            "keyboard session cancellation without host action",
            "notification primary and stack actions work through AT-SPI and keyboard",
            "launcher toolbar toggles expose checkbox state and accept keyboard and assistive actions",
            "disabled application provider blocks grid action",
            "Escape closes launcher from toolbar focus",
            "launcher list/grid result roles and captured-key keyboard and assistive actions",
            "removed/hidden result references cannot launch another app",
            "Dock menu-item actions activate exact native window once",
            "window removal invalidates Dock menu without bystander focus",
            "retired Dock menu actions cannot replay into a fresh menu",
        ];
        if self.o.full_shell {
            checks.extend([
                "actual Settings search named, editable and focusable through AT-SPI",
                "actual Files view selector exposes combo-box role and keyboard traversal",
            ]);
        }
        self.events = self.state_events()?;
        save(
            &self.output("result.json"),
            &json!({"passed":true,"bus":self.bus_address,"status":{"IsEnabled":true,"ScreenReaderEnabled":true},"events":self.events,"checks":checks}),
        )?;
        println!("{}", self.output("result.json").display());
        Ok(())
    }
}
