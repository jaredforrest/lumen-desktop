//! Real private Orca command provider and native compositor lock authority.
use serde_json::{Value, json};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::symlink;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use zbus::{Connection, interface};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SERVICE: &str = "org.gnome.Orca.Service";
const BASE: &str = "/org/gnome/Orca/Service/";

struct Module {
    root: PathBuf,
    path: &'static str,
    action: &'static str,
}
#[interface(name = "org.gnome.Orca.Module")]
impl Module {
    async fn list_commands(&self, #[zbus(connection)] conn: &Connection) -> Vec<(String, String)> {
        if self.root.join("lock-on-list").exists() {
            let _ = fs::write(
                self.root.join("lock.json"),
                json!({"available":true,"locked":true,"revision":2}).to_string(),
            );
        }
        if self.root.join("drop-name").exists() {
            let _ = conn.release_name(SERVICE).await;
        }
        if self.root.join("hang-list").exists() {
            std::future::pending::<()>().await;
        }
        vec![(self.action.into(), "Private supported command".into())]
    }
    async fn execute_command(&self, action: &str, repeated: bool) -> bool {
        if let Ok(mut file) = OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.root.join("commands.jsonl"))
        {
            let _ = writeln!(
                file,
                "{}",
                json!({"path":format!("{BASE}{}", self.path),"args":[action,repeated]})
            );
        }
        if self.root.join("hang-execute").exists() {
            std::future::pending::<()>().await;
        }
        true
    }
}

async fn serve(root: PathBuf) -> Result<()> {
    let mut builder = zbus::connection::Builder::session()?.name(SERVICE)?;
    for (path, action) in [
        ("WhereAmIPresenter", "WhereAmIBasic"),
        ("SayAllPresenter", "SayAll"),
        ("SpeechManager", "InterruptSpeech"),
    ] {
        builder = builder.serve_at(
            format!("{BASE}{path}"),
            Module {
                root: root.clone(),
                path,
                action,
            },
        )?;
    }
    let _conn = builder.build().await?;
    println!("ready");
    std::future::pending::<()>().await;
    Ok(())
}

struct Processes(Vec<Child>);
impl Drop for Processes {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn guard_daemon() -> Result<()> {
    let real = env::var_os("LUMEN_REAL_DBUS_DAEMON").ok_or("Missing real dbus-daemon")?;
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.iter().any(|arg| arg == "--session") {
        return Err("Ambient bus configuration".into());
    }
    let config = args
        .iter()
        .find_map(|arg| arg.to_str()?.strip_prefix("--config-file="))
        .ok_or("Missing explicit bus configuration")?;
    if config.starts_with("/etc/") {
        return Err("Ambient bus configuration".into());
    }
    let source = fs::read_to_string(config)?;
    if !source.contains("<listen>unix:tmpdir=")
        || source.contains("<include")
        || source.contains("<servicedir")
        || source.contains("<standard_session_servicedirs")
    {
        return Err("Bus configuration permits ambient activation".into());
    }
    Err(Command::new(real).args(args).exec().into())
}

fn private_bus(
    output: &Path,
    root: &Path,
    current: &Path,
    processes: &mut Processes,
) -> Result<String> {
    let guard_dir = output.join("bin");
    fs::create_dir(&guard_dir)?;
    symlink(current, guard_dir.join("dbus-daemon"))?;
    let config = output.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            root.display()
        ),
    )?;
    let daemon = env::var_os("LUMEN_DBUS_DAEMON").unwrap_or_else(|| "dbus-daemon".into());
    let mut command = Command::new(guard_dir.join("dbus-daemon"));
    command
        .args(["--nofork", "--print-address=1"])
        .arg(format!("--config-file={}", config.display()))
        .env("LUMEN_REAL_DBUS_DAEMON", daemon)
        .stdout(Stdio::piped())
        .stderr(File::create(output.join("bus.log"))?);
    let mut child = command.spawn()?;
    let mut ready = libc::pollfd {
        fd: child
            .stdout
            .as_ref()
            .ok_or("Missing bus output")?
            .as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let observed = unsafe { libc::poll(&mut ready, 1, 5000) };
    if observed <= 0 || ready.revents & libc::POLLIN == 0 {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Private bus failed to start".into());
    }
    let mut address = String::new();
    BufReader::new(child.stdout.take().ok_or("Missing bus output")?).read_line(&mut address)?;
    if !address.starts_with("unix:") {
        return Err("Private bus did not publish a Unix address".into());
    }
    processes.0.push(child);
    Ok(address.trim().into())
}

fn serve_ipc(mut stream: UnixStream, root: PathBuf, stopping: Arc<AtomicBool>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    let mut header = [0u8; 14];
    stream.read_exact(&mut header)?;
    if &header[..6] != b"i3-ipc" {
        return Err("Wrong IPC magic".into());
    }
    let length = u32::from_le_bytes(header[6..10].try_into()?) as usize;
    let kind = u32::from_le_bytes(header[10..14].try_into()?);
    if length > 65536 {
        return Err("IPC request too large".into());
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    let result = if kind == 2 {
        json!({"success":true})
    } else {
        let state: Value = serde_json::from_slice(&fs::read(root.join("lock.json"))?)?;
        json!({"lumen_session_lock":{"version":1,"available":state["available"],"locked":state["locked"],"revision":state["revision"]}})
    };
    let payload = result.to_string();
    header[6..10].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    stream.write_all(&header)?;
    stream.write_all(payload.as_bytes())?;
    if kind == 2 {
        let mut byte = [0u8; 1];
        while !stopping.load(Ordering::Relaxed) {
            match stream.read(&mut byte) {
                Ok(0) => break,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => break,
                _ => {}
            }
        }
    }
    Ok(())
}

struct SwayServer {
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    peers: Arc<Mutex<Vec<JoinHandle<()>>>>,
}
impl SwayServer {
    fn start(root: &Path) -> Result<Self> {
        let listener = UnixListener::bind(root.join("ipc.sock"))?;
        listener.set_nonblocking(true)?;
        let stopping = Arc::new(AtomicBool::new(false));
        let peers = Arc::new(Mutex::new(Vec::new()));
        let control = stopping.clone();
        let handles = peers.clone();
        let root = root.to_path_buf();
        let worker = thread::spawn(move || {
            while !control.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let root = root.clone();
                        let stop = control.clone();
                        handles.lock().unwrap().push(thread::spawn(move || {
                            let _ = serve_ipc(stream, root, stop);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            stopping,
            worker: Some(worker),
            peers,
        })
    }
}
impl Drop for SwayServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        for peer in self.peers.lock().unwrap().drain(..) {
            let _ = peer.join();
        }
    }
}

fn command_env(command: &mut Command, root: &Path, address: &str) {
    command
        .env("HOME", root)
        .env("XDG_RUNTIME_DIR", root)
        .env("DBUS_SESSION_BUS_ADDRESS", address)
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}/no-system-bus", root.display()),
        )
        .env("SWAYSOCK", root.join("ipc.sock"));
}

fn run_client(
    package: &Path,
    root: &Path,
    address: &str,
    arguments: &[&str],
) -> Result<(i32, Value)> {
    let mut command = Command::new(package.join("bin/lumen-reader-command"));
    command.args(arguments);
    command_env(&mut command, root, address);
    let output = command.output()?;
    let value: Value = serde_json::from_slice(&output.stdout)?;
    Ok((
        output
            .status
            .code()
            .ok_or("Reader client ended by signal")?,
        value,
    ))
}

fn commands(root: &Path) -> Result<Vec<Value>> {
    let path = root.join("commands.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    fs::read_to_string(path)?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

fn write_state(root: &Path, available: bool, locked: bool, revision: u32) -> Result<()> {
    fs::write(
        root.join("lock.json"),
        json!({"available":available,"locked":locked,"revision":revision}).to_string(),
    )?;
    Ok(())
}
fn expect(ok: bool, detail: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(detail.into()) }
}

async fn run(disabled: PathBuf, enabled: PathBuf, output: PathBuf) -> Result<()> {
    fs::create_dir(&output)?;
    let private = tempfile::Builder::new()
        .prefix("lumen-reader-provider-")
        .tempdir()?;
    let root = private.path();
    let current = env::current_exe()?;
    let mut processes = Processes(Vec::new());
    let address = private_bus(&output, root, &current, &mut processes)?;
    let conn = zbus::connection::Builder::address(address.as_str())?
        .build()
        .await?;
    let bus = zbus::Proxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;
    let activatable: Vec<String> = bus.call("ListActivatableNames", &()).await?;
    expect(
        activatable == ["org.freedesktop.DBus"],
        "Private reader bus can activate host services",
    )?;
    write_state(root, true, false, 1)?;
    let _sway = SwayServer::start(root)?;
    let mut checks = Vec::new();
    let (code, value) = run_client(&disabled, root, &address, &["where-am-i"])?;
    expect(
        code == 1 && value["reason"] == "disabled",
        "Packaged default did not refuse reader service",
    )?;
    checks.push("packaged default disabled without reader service");
    let (code, value) = run_client(&enabled, root, &address, &["--enabled", "where-am-i"])?;
    expect(
        code == 1 && value["reason"] == "unsupported_command",
        "Public command forwarded arbitrary options",
    )?;
    checks.push("public command cannot forward options or arbitrary arguments");

    let mut provider = Command::new(&current);
    provider.arg("--serve").arg(root);
    command_env(&mut provider, root, &address);
    provider
        .stdout(Stdio::piped())
        .stderr(File::create(output.join("provider.log"))?);
    let mut provider = provider.spawn()?;
    let mut ready = libc::pollfd {
        fd: provider
            .stdout
            .as_ref()
            .ok_or("Missing provider output")?
            .as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let observed = unsafe { libc::poll(&mut ready, 1, 5000) };
    if observed <= 0 || ready.revents & libc::POLLIN == 0 {
        let _ = provider.kill();
        let _ = provider.wait();
        return Err("Reader provider did not start".into());
    }
    let mut line = String::new();
    BufReader::new(provider.stdout.take().ok_or("Missing provider output")?)
        .read_line(&mut line)?;
    expect(
        line.trim() == "ready",
        "Reader provider did not become ready",
    )?;
    processes.0.push(provider);

    let (code, value) = run_client(&enabled, root, &address, &["where-am-i"])?;
    expect(
        code == 0 && value["accepted"] == true && commands(root)?.len() == 1,
        "Typed command did not dispatch to unique provider",
    )?;
    checks.push("unique owned provider actual typed command");
    write_state(root, true, true, 2)?;
    let (code, value) = run_client(&enabled, root, &address, &["read-all"])?;
    expect(
        code == 1 && value["reason"] == "session_unavailable" && commands(root)?.len() == 1,
        "Locked session accepted reading",
    )?;
    checks.push("authoritative locked session rejects reading");
    write_state(root, false, true, 3)?;
    let (code, value) = run_client(&enabled, root, &address, &["where-am-i"])?;
    expect(
        code == 1 && value["reason"] == "session_unavailable" && commands(root)?.len() == 1,
        "Unknown native capability accepted reading",
    )?;
    checks.push("unknown native capability rejects reading");
    let (code, value) = run_client(&enabled, root, &address, &["stop"])?;
    expect(
        code == 0 && value["accepted"] == true && commands(root)?.len() == 2,
        "Owned stop was unavailable",
    )?;
    checks.push("safe owned stop remains available with unknown lock authority");
    write_state(root, true, false, 1)?;
    File::create(root.join("lock-on-list"))?;
    let (code, value) = run_client(&enabled, root, &address, &["where-am-i"])?;
    expect(
        code == 1 && value["accepted"] == false && commands(root)?.len() == 2,
        "Lock transition dispatched a read",
    )?;
    fs::remove_file(root.join("lock-on-list"))?;
    checks.push("lock transition during discovery produces zero read dispatch");
    write_state(root, true, false, 1)?;
    File::create(root.join("hang-list"))?;
    let started = Instant::now();
    let (code, value) = run_client(&enabled, root, &address, &["where-am-i"])?;
    expect(
        code == 1
            && value["reason"] == "provider_timeout"
            && started.elapsed() >= Duration::from_secs(2)
            && started.elapsed() < Duration::from_secs(4)
            && commands(root)?.len() == 2,
        &format!("Withheld discovery did not time out correctly: {value}"),
    )?;
    fs::remove_file(root.join("hang-list"))?;
    checks.push("actual withheld provider reply expires within fixed budget without dispatch");
    File::create(root.join("hang-execute"))?;
    let (code, value) = run_client(&enabled, root, &address, &["where-am-i"])?;
    expect(
        code == 1
            && value["reason"] == "dispatch_unknown"
            && value["dispatched"] == true
            && commands(root)?.len() == 3,
        &format!("Unacknowledged dispatch did not stay ambiguous: {value}"),
    )?;
    fs::remove_file(root.join("hang-execute"))?;
    checks.push("missing command acknowledgement remains explicitly ambiguous and never retries");
    write_state(root, true, false, 1)?;
    File::create(root.join("drop-name"))?;
    let (code, value) = run_client(&enabled, root, &address, &["where-am-i"])?;
    expect(
        code == 1 && value["accepted"] == false && commands(root)?.len() == 3,
        "Retired reader owner dispatched a read",
    )?;
    checks.push("reader owner retirement during discovery produces zero dispatch and no retry");
    let result = json!({"passed":true,"checks":checks,"commands":commands(root)?});
    fs::write(
        output.join("result.json"),
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )?;
    println!("PASS private reader provider {} checks", checks.len());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    if env::args_os().next().is_some_and(|name| {
        Path::new(&name)
            .file_name()
            .is_some_and(|base| base == "dbus-daemon")
    }) {
        return guard_daemon();
    }
    let mut args = env::args_os().skip(1);
    let mut disabled = None;
    let mut enabled = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--serve") if disabled.is_none() && enabled.is_none() && output.is_none() => {
                let root=PathBuf::from(args.next().ok_or("--serve requires root")?);
                expect(args.next().is_none(),"Unexpected provider argument")?;
                return serve(root).await;
            }
            Some("--disabled") => disabled=Some(PathBuf::from(args.next().ok_or("--disabled requires path")?)),
            Some("--enabled") => enabled=Some(PathBuf::from(args.next().ok_or("--enabled requires path")?)),
            Some("--output") => output=Some(PathBuf::from(args.next().ok_or("--output requires path")?)),
            _ => return Err("usage: lumen-reader-commands-provider-fixture --disabled PACKAGE --enabled PACKAGE --output NEW-DIRECTORY".into()),
        }
    }
    run(
        disabled.ok_or("--disabled is required")?,
        enabled.ok_or("--enabled is required")?,
        output.ok_or("--output is required")?,
    )
    .await
}
