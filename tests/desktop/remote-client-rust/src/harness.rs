use crate::{
    Result, accessibility_dbus::BusProvider, fail, keyboard::PhysicalKeyboard,
    vault_artifacts::ArtifactSecrets,
};
use base64::Engine;
use lumen_application_isolation::provenance::{FrozenSource, digest};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use zbus::blocking::{Connection, Proxy};
const TOOLS: &[&str] = &[
    "launcher",
    "sway",
    "keyboard",
    "font-config",
    "software-driver",
    "audio",
    "remmina",
    "wayvnc",
    "tls",
    "at-spi-prefix",
    "keyring",
    "icons",
    "openssh",
    "rdp",
    "xserver",
    "adapter",
    "isolation",
    "posix-shell",
];
#[derive(Default)]
pub struct Options {
    pub tools: BTreeMap<String, PathBuf>,
    pub output: PathBuf,
    pub source: PathBuf,
    pub protocol: String,
    pub scenario: String,
    pub inside: bool,
    pub effects: bool,
}
impl Options {
    pub fn parse() -> Result<Self> {
        let mut o = Self {
            protocol: "vnc".into(),
            scenario: "authenticated".into(),
            source: env::current_dir()?,
            ..Self::default()
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--inside" => o.inside = true,
                "--effects" => o.effects = true,
                "--output" => o.output = PathBuf::from(args.next().ok_or("Missing output")?),
                "--source" => o.source = PathBuf::from(args.next().ok_or("Missing source")?),
                "--protocol" => o.protocol = args.next().ok_or("Missing protocol")?,
                "--scenario" => o.scenario = args.next().ok_or("Missing scenario")?,
                _ => {
                    let name = arg
                        .strip_prefix("--")
                        .filter(|name| TOOLS.contains(name))
                        .ok_or("Unknown fixture option")?;
                    let path = PathBuf::from(args.next().ok_or("Missing tool")?).canonicalize()?;
                    ensure(
                        path.starts_with("/nix/store") && path.exists(),
                        "Immutable tool required",
                    )?;
                    o.tools.insert(name.into(), path);
                }
            }
        }
        ensure(
            TOOLS.iter().all(|key| o.tools.contains_key(*key)),
            "All pinned fixture tools required",
        )?;
        ensure(
            ["vnc", "rdp", "ssh"].contains(&o.protocol.as_str()),
            "Unknown protocol",
        )?;
        ensure(
            ["authenticated", "wrong-identity", "wrong-password"].contains(&o.scenario.as_str()),
            "Unknown scenario",
        )?;
        ensure(!o.output.as_os_str().is_empty(), "Fresh output required")?;
        Ok(o)
    }
    pub fn tool(&self, name: &str) -> PathBuf {
        self.tools[name].clone()
    }
}
pub fn ensure(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(message)) }
}
pub fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
pub fn wait<T>(name: &str, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = Instant::now() + Duration::from_secs(15);
    let mut last = String::new();
    loop {
        match probe() {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => {}
            Err(e) => last = e.to_string(),
        }
        if Instant::now() >= end {
            return Err(fail(format!(
                "Private observation deadline: {name}: {last}"
            )));
        }
        pause(50);
    }
}
pub fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn environment(launcher: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new(launcher).arg("--print-env0").output()?;
    ensure(output.status.success(), "Launcher environment failed")?;
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    for part in output.stdout.split(|v| *v == 0) {
        if let Some(index) = part.iter().position(|v| *v == b'=') {
            values.insert(
                String::from_utf8(part[..index].to_vec())?,
                String::from_utf8(part[index + 1..].to_vec())?,
            );
        }
    }
    Ok(values)
}
pub fn outer(o: &Options) -> Result<()> {
    ensure(!o.output.exists(), "Fresh artifact directory required")?;
    fs::create_dir_all(&o.output)?;
    fs::set_permissions(&o.output, fs::Permissions::from_mode(0o700))?;
    let output = o.output.canonicalize()?;
    let source = o.source.canonicalize()?;
    let executable = env::current_exe()?.canonicalize()?;
    let mut dependencies = vec![executable.clone()];
    for (name, path) in &o.tools {
        if path.is_file() {
            dependencies.push(path.clone());
        } else {
            for relative in match name.as_str() {
                "audio" => vec!["bin/pulseaudio", "bin/pactl"],
                "remmina" => vec!["bin/remmina"],
                "wayvnc" => vec!["bin/wayvnc"],
                "tls" => vec!["bin/openssl"],
                "at-spi-prefix" => vec!["libexec/at-spi-bus-launcher", "libexec/at-spi2-registryd"],
                "keyring" => vec!["bin/gnome-keyring-daemon"],
                "openssh" => vec!["bin/sshd", "bin/ssh-keygen"],
                "rdp" => vec!["bin/freerdp-shadow-cli", "bin/winpr-hash"],
                "xserver" => vec!["bin/Xvfb"],
                _ => vec![],
            } {
                dependencies.push(path.join(relative));
            }
        }
    }
    let frozen = FrozenSource::new(
        &source,
        &source.join("tests/desktop/remote-client-rust/src/main.rs"),
        &dependencies,
    )?;
    let mut command = Command::new(o.tool("isolation"));
    command.arg("run");
    command.args([
        "--source",
        source.to_str().ok_or("source")?,
        "--output",
        output.to_str().ok_or("output")?,
    ]);
    if o.effects {
        command.arg("--software-graphics");
    }
    if o.protocol == "ssh" {
        command.arg("--private-pty");
    }
    command.arg("--").arg(executable).args([
        "--inside",
        "--source",
        "/source",
        "--output",
        "/artifacts/result",
        "--protocol",
        &o.protocol,
        "--scenario",
        &o.scenario,
    ]);
    for (name, path) in &o.tools {
        command.arg(format!("--{name}")).arg(path);
    }
    if o.effects {
        command.arg("--effects");
    }
    let result = command.envs(environment(&o.tool("launcher"))?).status();
    let code = result
        .as_ref()
        .ok()
        .and_then(|status| status.code())
        .unwrap_or(1);
    frozen.finish(&output, code)?;
    ensure(result?.success(), "Isolated remote provider fixture failed")
}
pub fn dispatch() -> Result<()> {
    let log = File::create(env::var("LUMEN_REMOTE_TEST_LOG")?)?;
    fs::write(
        env::var("LUMEN_REMOTE_TEST_PID")?,
        std::process::id().to_string(),
    )?;
    let error = Command::new(env::var("LUMEN_REMOTE_TEST_CLIENT")?)
        .args(env::args_os().skip(1))
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .exec();
    Err(error.into())
}
pub struct Harness {
    pub o: Options,
    pub env: BTreeMap<String, String>,
    pub out: PathBuf,
    pub private: PathBuf,
    pub profile: PathBuf,
    pub bus: Connection,
    pub observer: Option<BusProvider>,
    pub keyboard: Option<PhysicalKeyboard>,
    pub guard: ArtifactSecrets,
    pub password: String,
    pub children: Vec<Child>,
    pub clients: Vec<u32>,
    pub logs: Vec<String>,
}
impl Harness {
    pub fn command(&self, program: impl AsRef<Path>) -> Command {
        let mut command = Command::new(program.as_ref());
        command.env_clear().envs(&self.env);
        command
    }
    pub fn launch(
        &mut self,
        program: impl AsRef<Path>,
        args: &[OsString],
        log: &str,
    ) -> Result<usize> {
        let file = File::create(self.out.join(log))?;
        let mut command = self.command(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn()?;
        self.children.push(child);
        Ok(self.children.len() - 1)
    }
    pub fn launch_s(
        &mut self,
        program: impl AsRef<Path>,
        args: &[&str],
        log: &str,
    ) -> Result<usize> {
        self.launch(
            program,
            &args.iter().map(OsString::from).collect::<Vec<_>>(),
            log,
        )
    }
    pub fn call(&self, program: impl AsRef<Path>, args: &[&str]) -> Result<String> {
        let output = self
            .command("timeout")
            .arg("8s")
            .arg(program.as_ref())
            .args(args)
            .output()?;
        ensure(
            output.status.success(),
            format!(
                "Private tool failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    pub fn quiet(&self, program: impl AsRef<Path>, args: &[&str]) -> Result<()> {
        let status = self
            .command("timeout")
            .arg("10s")
            .arg(program.as_ref())
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        ensure(status.success(), "Private provisioning command failed")
    }
    pub fn stop(&mut self, index: usize) -> Result<()> {
        let child = &mut self.children[index];
        if child.try_wait()?.is_none() {
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGTERM);
            }
            let until = Instant::now() + Duration::from_secs(3);
            while child.try_wait()?.is_none() && Instant::now() < until {
                pause(20);
            }
            if child.try_wait()?.is_none() {
                unsafe {
                    libc::killpg(child.id() as i32, libc::SIGKILL);
                }
                child.wait()?;
            }
        }
        Ok(())
    }
    pub fn client_stop(&mut self) -> Result<()> {
        if let Some(pid) = self.clients.last() {
            unsafe {
                libc::killpg(*pid as i32, libc::SIGTERM);
            }
            let until = Instant::now() + Duration::from_secs(3);
            while Path::new(&format!("/proc/{pid}")).exists() && Instant::now() < until {
                unsafe {
                    libc::waitpid(*pid as i32, std::ptr::null_mut(), libc::WNOHANG);
                }
                pause(20);
            }
            if Path::new(&format!("/proc/{pid}")).exists() {
                unsafe {
                    libc::killpg(*pid as i32, libc::SIGKILL);
                    libc::waitpid(*pid as i32, std::ptr::null_mut(), 0);
                }
            }
        }
        Ok(())
    }
    pub fn ipc(&self, args: &[&str]) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.call(
                self.o
                    .tool("sway")
                    .parent()
                    .ok_or("sway parent")?
                    .join("swaymsg"),
                args,
            )?,
        )?)
    }
    pub fn tree(&self) -> Result<Value> {
        self.ipc(&["-r", "-t", "get_tree"])
    }
    pub fn semantics(&mut self) -> Result<Vec<Value>> {
        Ok(self
            .observer
            .as_mut()
            .ok_or("Accessibility unavailable")?
            .snapshot()?
            .into_iter()
            .filter(|row| row["states"]["showing"] == true)
            .collect())
    }
    pub fn target(&self) -> Result<Value> {
        let proxy = Proxy::new(
            &self.bus,
            "org.lumen.RemoteTarget",
            "/Target",
            "org.lumen.RemoteTarget",
        )?;
        let body: String = proxy.call("Status", &())?;
        Ok(serde_json::from_str(&body)?)
    }
    fn adapter(&self, request: &Value) -> Result<Value> {
        let mut process = self
            .command(self.o.tool("adapter"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        writeln!(process.stdin.take().ok_or("Adapter input")?, "{request}")?;
        let output = process.wait_with_output()?;
        ensure(output.status.success(), "Actual Rust remote adapter failed")?;
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    pub fn connect(&mut self) -> Result<()> {
        let original = digest(&self.profile)?;
        let rows = self.adapter(&json!({"action":"list"}))?;
        ensure(
            rows["connections"]
                .as_array()
                .is_some_and(|rows| rows.len() == 1),
            "Expected one private profile",
        )?;
        ensure(
            original == digest(&self.profile)?,
            "Listing mutated profile",
        )?;
        let name = if self.logs.is_empty() {
            "remmina.log".into()
        } else {
            format!("remmina-reconnected-{}.log", self.logs.len())
        };
        let pid_file = self.private.join("dispatch-pid");
        let _ = fs::remove_file(&pid_file);
        self.env.insert(
            "LUMEN_REMOTE_TEST_LOG".into(),
            self.out.join(&name).display().to_string(),
        );
        self.env.insert(
            "LUMEN_REMOTE_TEST_PID".into(),
            pid_file.display().to_string(),
        );
        let result =
            self.adapter(&json!({"action":"connect","id":rows["connections"][0]["id"]}))?;
        ensure(
            result == json!({"started":true}),
            "Adapter did not start real client",
        )?;
        let pid = wait("actual adapter destination dispatch", || {
            Ok(fs::read_to_string(&pid_file)
                .ok()
                .and_then(|text| text.parse().ok()))
        })?;
        self.clients.push(pid);
        self.logs.push(name);
        wait("actual Remmina window", || {
            Ok(nodes(&self.tree()?)
                .into_iter()
                .find(|node| node["app_id"] == "org.remmina.Remmina"))
        })?;
        Ok(())
    }
    pub fn focus(&mut self, row: &Value) -> Result<()> {
        let client = nodes(&self.tree()?)
            .into_iter()
            .find(|node| node["app_id"] == "org.remmina.Remmina")
            .ok_or("Actual client absent")?;
        let id = client["id"].as_u64().ok_or("Client ID")?;
        self.ipc(&[&format!("[con_id={id}] focus")])?;
        let reference = &row["reference"];
        let reference = (
            reference[0].as_str().ok_or("AT-SPI owner")?.to_owned(),
            reference[1].as_str().ok_or("AT-SPI path")?.to_owned(),
        );
        ensure(
            self.observer
                .as_ref()
                .ok_or("Observer")?
                .focus(&reference, Duration::from_millis(500))?,
            "Protected control refused focus",
        )?;
        let seats = self.ipc(&["-r", "-t", "get_seats"])?;
        ensure(
            seats
                .as_array()
                .ok_or("Seats")?
                .iter()
                .any(|seat| seat["name"] == "seat0" && seat["focus"] == id),
            "Native seat focus not on protected client",
        )
    }
    pub fn password(&mut self, value: &str) -> Result<()> {
        let row = wait("protected credential entry", || {
            Ok(self
                .semantics()?
                .into_iter()
                .find(|row| row["role"] == "password text"))
        })?;
        self.focus(&row)?;
        let keyboard = self.keyboard.as_mut().ok_or("Keyboard")?;
        keyboard.key(&["-M", "ctrl", "-k", "a", "-m", "ctrl"])?;
        keyboard.type_text(value)?;
        keyboard.key(&["-k", "Return"])?;
        Ok(())
    }
    pub fn safe_target(&self) -> Result<Value> {
        let state = self.target()?;
        ensure(
            !state["text"]
                .as_str()
                .unwrap_or("")
                .contains(&self.password),
            "Credential routing failed; no target receipt retained",
        )?;
        Ok(state)
    }
    pub fn capture(&self, name: &str) -> Result<()> {
        self.call(
            "grim",
            &[
                "-o",
                "HEADLESS-1",
                self.out.join(name).to_str().ok_or("Capture path")?,
            ],
        )?;
        Ok(())
    }
    pub fn pixels(&self, name: &str) -> Result<Value> {
        let file = File::open(self.out.join(name))?;
        let mut decoder = png::Decoder::new(file);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut bytes = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut bytes)?;
        let stride = if info.color_type == png::ColorType::Rgba {
            4
        } else {
            3
        };
        let mut counts = [0_u64; 4];
        for pixel in bytes[..info.buffer_size()].chunks_exact(stride) {
            for (index, color) in [
                [0xd0, 0x40, 0x40],
                [0x40, 0xc0, 0x80],
                [0x40, 0x70, 0xd0],
                [0xe0, 0xb0, 0x40],
            ]
            .iter()
            .enumerate()
            {
                if pixel[..3] == color[..] {
                    counts[index] += 1;
                }
            }
        }
        ensure(
            counts.iter().all(|count| *count > 30000),
            format!("Actual remote framebuffer palette absent: {counts:?}"),
        )?;
        Ok(json!({"#d04040":counts[0],"#40c080":counts[1],"#4070d0":counts[2],"#e0b040":counts[3]}))
    }
    pub fn log(&self, name: &str) -> String {
        fs::read_to_string(self.out.join(name)).unwrap_or_default()
    }
    pub fn certificate(&self, cert: &Path, key: &Path, common: &str, san: &str) -> Result<PathBuf> {
        let tls = self.o.tool("tls").join("bin/openssl");
        let ca = self.private.join("ca.crt");
        let ca_key = self.private.join("ca.key");
        let request = self.private.join("server.csr");
        let extensions = self.private.join("server.ext");
        self.quiet(
            &tls,
            &[
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                &s(&ca_key),
                "-out",
                &s(&ca),
                "-days",
                "1",
                "-subj",
                "/CN=Disposable fixture CA",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
            ],
        )?;
        self.quiet(
            &tls,
            &[
                "req",
                "-new",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                &s(key),
                "-out",
                &s(&request),
                "-subj",
                &format!("/CN={common}"),
            ],
        )?;
        fs::write(
            &extensions,
            format!(
                "subjectAltName={san}\nbasicConstraints=critical,CA:FALSE\nextendedKeyUsage=serverAuth\n"
            ),
        )?;
        self.quiet(
            tls,
            &[
                "x509",
                "-req",
                "-in",
                &s(&request),
                "-CA",
                &s(&ca),
                "-CAkey",
                &s(&ca_key),
                "-CAcreateserial",
                "-out",
                &s(cert),
                "-days",
                "1",
                "-extfile",
                &s(&extensions),
            ],
        )?;
        Ok(ca)
    }
}
pub fn s(path: &Path) -> String {
    path.display().to_string()
}
pub fn nodes(tree: &Value) -> Vec<Value> {
    let mut result = vec![tree.clone()];
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = tree[key].as_array() {
            for child in children {
                result.extend(nodes(child));
            }
        }
    }
    result
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.client_stop();
        if let Some(mut keyboard) = self.keyboard.take() {
            let _ = keyboard.close();
        }
        for index in (0..self.children.len()).rev() {
            let _ = self.stop(index);
        }
    }
}
fn owned(bus: &Connection, name: &str) -> Result<bool> {
    Ok(Proxy::new(
        bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?
    .call("NameHasOwner", &(name,))?)
}
pub fn inside(o: Options) -> Result<()> {
    ensure(
        env::var("HOME")? == "/home/fixture" && !Path::new("/.old-root").exists(),
        "Full private namespace required",
    )?;
    fs::create_dir(&o.output)?;
    fs::set_permissions(&o.output, fs::Permissions::from_mode(0o700))?;
    let private = PathBuf::from("/home/fixture/private");
    fs::create_dir(&private)?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    let mut random = [0_u8; 256];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let mut guard = ArtifactSecrets::new();
    let password = guard.add(format!(
        "Disposable-{}",
        random[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))?;
    guard.add(base64::engine::general_purpose::STANDARD.encode(password.as_bytes()))?;
    let mut values = env::vars().collect::<BTreeMap<_, _>>();
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SWAYSOCK",
        "AT_SPI_BUS_ADDRESS",
        "GTK_MODULES",
        "NO_AT_BRIDGE",
    ] {
        values.remove(key);
    }
    for (key, value) in [
        ("WLR_BACKENDS", "headless"),
        ("WLR_HEADLESS_OUTPUTS", "2"),
        ("WLR_RENDERER", if o.effects { "gles2" } else { "pixman" }),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if o.effects { "1" } else { "0" },
        ),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("LIBGL_ALWAYS_SOFTWARE", if o.effects { "0" } else { "1" }),
        ("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe"),
        ("QT_QPA_PLATFORM", "wayland"),
        ("GTK_A11Y", "atspi"),
        ("GDK_BACKEND", "wayland"),
        ("LUMEN_TEST_SEAT", "seat0"),
        ("GIO_USE_VFS", "local"),
        ("GIO_USE_VOLUME_MONITOR", "unix"),
        ("G_MESSAGES_DEBUG", "remmina"),
        ("XDG_CURRENT_DESKTOP", "Lumen"),
        ("DESKTOP_SESSION", "lumen"),
    ] {
        values.insert(key.into(), value.into());
    }
    values.insert("FONTCONFIG_FILE".into(), s(&o.tool("font-config")));
    values.insert(
        "LIBGL_DRIVERS_PATH".into(),
        s(&o.tool("software-driver").join("lib/dri")),
    );
    values.insert(
        "__EGL_VENDOR_LIBRARY_FILENAMES".into(),
        s(&o.tool("software-driver")
            .join("share/glvnd/egl_vendor.d/50_mesa.json")),
    );
    values.insert(
        "XDG_DATA_DIRS".into(),
        format!(
            "{}:{}",
            o.tool("icons").join("share").display(),
            o.tool("remmina").join("share").display()
        ),
    );
    let address = "unix:path=/run/fixture/remote-bus";
    values.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.into());
    values.insert("DBUS_SYSTEM_BUS_ADDRESS".into(), address.into());
    let bus_config = o.output.join("bus.conf");
    fs::write(
        &bus_config,
        format!(
            "<busconfig><type>session</type><listen>{address}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>"
        ),
    )?;
    let bus_log = File::create(o.output.join("bus.log"))?;
    let mut command = Command::new("dbus-daemon");
    command
        .args(["--nofork", "--config-file"])
        .arg(&bus_config)
        .env_clear()
        .envs(&values)
        .stdout(Stdio::from(bus_log.try_clone()?))
        .stderr(Stdio::from(bus_log));
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = command.spawn()?;
    wait("private bus socket", || {
        Ok(Path::new("/run/fixture/remote-bus").exists().then_some(()))
    })?;
    let bus = zbus::blocking::connection::Builder::address(address)?.build()?;
    let out = o.output.clone();
    let profile = PathBuf::from(values.get("XDG_DATA_HOME").ok_or("Private data")?)
        .join("remmina/profile.remmina");
    fs::create_dir_all(profile.parent().ok_or("Profile parent")?)?;
    let mut f = Harness {
        o,
        env: values,
        out,
        private,
        profile,
        bus,
        observer: None,
        keyboard: None,
        guard,
        password,
        children: vec![child],
        clients: vec![],
        logs: vec![],
    };
    let result = (|| -> Result<()> {
        let config = PathBuf::from(f.env.get("XDG_CONFIG_HOME").ok_or("Private config")?);
        let cookie = config.join("pulse/cookie");
        fs::create_dir_all(cookie.parent().ok_or("Cookie parent")?)?;
        fs::write(&cookie, random)?;
        fs::set_permissions(&cookie, fs::Permissions::from_mode(0o600))?;
        let audio = lumen_application_isolation::audio::start_null_sink(
            &f.o.tool("audio"),
            &mut f.env,
            &f.out,
        )?;
        f.children.push(audio);
        let log = File::create(f.out.join("keyring.log"))?;
        let mut command = f.command(f.o.tool("keyring").join("bin/gnome-keyring-daemon"));
        command
            .args(["--foreground", "--components=secrets", "--unlock"])
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log));
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut keyring = command.spawn()?;
        keyring
            .stdin
            .take()
            .ok_or("Keyring input")?
            .write_all(f.password.as_bytes())?;
        f.children.push(keyring);
        wait("private secret service", || {
            Ok(owned(&f.bus, "org.freedesktop.secrets")?.then_some(()))
        })?;
        f.launch_s(
            f.o.tool("at-spi-prefix")
                .join("libexec/at-spi-bus-launcher"),
            &["--launch-immediately", "--a11y=1", "--screen-reader=1"],
            "a11y-bus.log",
        )?;
        wait("private accessibility bus", || {
            Ok(owned(&f.bus, "org.a11y.Bus")?.then_some(()))
        })?;
        let accessibility_address: String =
            Proxy::new(&f.bus, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus")?
                .call("GetAddress", &())?;
        f.env
            .insert("AT_SPI_BUS_ADDRESS".into(), accessibility_address.clone());
        f.launch_s(
            f.o.tool("at-spi-prefix").join("libexec/at-spi2-registryd"),
            &[],
            "a11y-registry.log",
        )?;
        f.observer = Some(BusProvider::connect(&accessibility_address)?);
        for (directory, name, body) in [
            (
                "remmina",
                "remmina.pref",
                "[remmina_pref]\nfloating_toolbar_monitor=0\ndisable_tray_icon=true\n",
            ),
            (
                "gtk-3.0",
                "settings.ini",
                "[Settings]\ngtk-font-name=DejaVu Sans 10\ngtk-icon-theme-name=Adwaita\n",
            ),
        ] {
            fs::create_dir_all(config.join(directory))?;
            fs::write(config.join(directory).join(name), body)?;
        }
        let dispatch_dir = f.private.join("dispatch");
        fs::create_dir(&dispatch_dir)?;
        symlink(env::current_exe()?, dispatch_dir.join("remmina"))?;
        f.env.insert(
            "PATH".into(),
            format!(
                "{}:{}",
                dispatch_dir.display(),
                f.env.get("PATH").ok_or("Pinned PATH")?
            ),
        );
        f.env.insert(
            "LUMEN_REMOTE_TEST_CLIENT".into(),
            s(&f.o.tool("remmina").join("bin/remmina")),
        );
        let compositor_config = f.out.join("sway.conf");
        fs::write(
            &compositor_config,
            "output HEADLESS-1 resolution 1920x1080 position 0 0\noutput HEADLESS-2 resolution 1280x720 position 1920 0\nseat seat0 fallback true\nseat remote fallback false\nxwayland disable\nfocus_follows_mouse no\nfor_window [app_id=\"org.lumen.RemoteTarget\"] move to output HEADLESS-2, fullscreen enable\nfor_window [app_id=\"org.remmina.Remmina\"] floating enable, resize set 1100 700, move to output HEADLESS-1\n",
        )?;
        f.launch_s(
            f.o.tool("sway"),
            &["--unsupported-gpu", "-c", &s(&compositor_config)],
            "sway.log",
        )?;
        let runtime = PathBuf::from(f.env.get("XDG_RUNTIME_DIR").ok_or("Runtime")?);
        let socket = wait("compositor", || {
            Ok(fs::read_dir(&runtime)?
                .flatten()
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("sway-ipc"))
                }))
        })?;
        f.env.insert("SWAYSOCK".into(), s(&socket));
        let display = wait("Wayland display", || {
            Ok(fs::read_dir(&runtime)?
                .flatten()
                .map(|entry| entry.file_name())
                .find(|name| {
                    name.to_string_lossy().starts_with("wayland-")
                        && !name.to_string_lossy().ends_with(".lock")
                }))
        })?;
        f.env.insert(
            "WAYLAND_DISPLAY".into(),
            display.to_string_lossy().into_owned(),
        );
        f.keyboard = Some(PhysicalKeyboard::start(
            &f.o.tool("keyboard"),
            &f.env,
            &f.out.join("keyboard.log"),
        )?);
        crate::protocols::target(&mut f)?;
        crate::protocols::run(&mut f)
    })();
    let _ = f.client_stop();
    if let Some(mut keyboard) = f.keyboard.take() {
        keyboard.close()?;
    }
    for index in (0..f.children.len()).rev() {
        f.stop(index)?;
    }
    let audit = f.guard.audit(&f.out);
    audit?;
    result.map_err(|error| fail(f.guard.redact(&error.to_string())))
}
