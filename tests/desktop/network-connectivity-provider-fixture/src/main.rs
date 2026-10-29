//! Real NetworkManager D-Bus transitions on a private, activation-free bus.
use serde_json::{Value, json};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, interface};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const NAME: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const WIFI: &str = "/device/wifi";
const ETHERNET: &str = "/device/ethernet";

#[derive(Clone)]
struct Case(Arc<Mutex<String>>);
impl Case {
    fn get(&self) -> String {
        self.0.lock().unwrap().clone()
    }
    fn set(&self, value: &str) {
        *self.0.lock().unwrap() = value.into();
    }
    fn facts(&self) -> (u32, u32) {
        match self.get().as_str() {
            "disconnected" => (30, 4),
            "associating" => (50, 4),
            "acquiring" => (70, 4),
            "unavailable" => (20, 4),
            "none" => (100, 1),
            "portal" => (100, 2),
            "limited" => (100, 3),
            "unknown" => (100, 0),
            _ => (100, 4),
        }
    }
}

struct Root(Case);
#[interface(name = "org.freedesktop.NetworkManager")]
impl Root {
    fn get_device_by_ip_iface(&self, interface: &str) -> OwnedObjectPath {
        assert_eq!(interface, "wlan0");
        OwnedObjectPath::try_from(WIFI).unwrap()
    }
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        [WIFI, ETHERNET]
            .into_iter()
            .map(|path| OwnedObjectPath::try_from(path).unwrap())
            .collect()
    }
    #[zbus(property, name = "WirelessEnabled")]
    fn wireless_enabled(&self) -> bool {
        self.0.get() != "disabled"
    }
    #[zbus(property, name = "WirelessHardwareEnabled")]
    fn wireless_hardware_enabled(&self) -> bool {
        true
    }
    #[zbus(property, name = "ConnectivityCheckAvailable")]
    fn connectivity_check_available(&self) -> bool {
        true
    }
    #[zbus(property, name = "ConnectivityCheckEnabled")]
    fn connectivity_check_enabled(&self) -> bool {
        self.0.get() != "unchecked"
    }
    #[zbus(property, name = "Connectivity")]
    fn connectivity(&self) -> u32 {
        4
    }
}

struct Device {
    case: Case,
    wifi: bool,
}
#[interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property, name = "DeviceType")]
    fn device_type(&self) -> u32 {
        if self.wifi { 2 } else { 1 }
    }
    #[zbus(property, name = "Interface")]
    fn network_interface(&self) -> String {
        if self.wifi { "wlan0" } else { "eth0" }.into()
    }
    #[zbus(property, name = "Managed")]
    fn managed(&self) -> bool {
        true
    }
    #[zbus(property, name = "State")]
    fn state(&self) -> u32 {
        if self.wifi { self.case.facts().0 } else { 100 }
    }
    #[zbus(property, name = "Ip4Connectivity")]
    async fn ip4_connectivity(&self, #[zbus(connection)] conn: &Connection) -> u32 {
        if self.wifi && self.case.get() == "owner-loss" {
            let _ = conn.release_name(NAME).await;
        }
        if self.wifi { self.case.facts().1 } else { 4 }
    }
    #[zbus(property, name = "Ip6Connectivity")]
    fn ip6_connectivity(&self) -> u32 {
        if self.wifi { 1 } else { 4 }
    }
}

struct Control(Case);
#[interface(name = "org.lumen.TestConnectivity")]
impl Control {
    fn set_case(&self, value: &str) {
        self.0.set(value);
    }
}

async fn serve() -> Result<()> {
    let case = Case(Arc::new(Mutex::new("full".into())));
    let _conn = zbus::connection::Builder::system()?
        .name(NAME)?
        .serve_at(ROOT, Root(case.clone()))?
        .serve_at(ROOT, Control(case.clone()))?
        .serve_at(
            WIFI,
            Device {
                case: case.clone(),
                wifi: true,
            },
        )?
        .serve_at(ETHERNET, Device { case, wifi: false })?
        .build()
        .await?;
    println!("READY");
    std::future::pending::<()>().await;
    Ok(())
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

struct Processes(Vec<Child>);
impl Drop for Processes {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn command_env(command: &mut Command, root: &Path, address: &str) {
    command
        .env("HOME", root)
        .env("XDG_RUNTIME_DIR", root)
        .env("DBUS_SYSTEM_BUS_ADDRESS", address)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-session", root.display()),
        );
    for part in ["CONFIG", "STATE", "CACHE", "DATA"] {
        command.env(format!("XDG_{part}_HOME"), root.join(part.to_lowercase()));
    }
}

fn snapshot(root: &Path, address: &str, binary: &Path, interface: Option<&str>) -> Result<Value> {
    let mut command = Command::new(binary);
    command_env(&mut command, root, address);
    if let Some(interface) = interface {
        command.arg(interface);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing client stdin")?
        .write_all(b"{\"available\":true,\"state\":\"COMPLETED\"}")?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "connectivity probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

async fn bus_call<T, B>(
    conn: &Connection,
    dest: &str,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> Result<T>
where
    T: serde::de::DeserializeOwned + zbus::zvariant::Type,
    B: serde::ser::Serialize + zbus::zvariant::Type,
{
    let proxy = zbus::Proxy::new(conn, dest, path, interface).await?;
    Ok(proxy.call(method, body).await?)
}

async fn owner(conn: &Connection) -> bool {
    bus_call::<bool, _>(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        &(NAME,),
    )
    .await
    .unwrap_or(false)
}

async fn wait_owner(conn: &Connection, present: bool) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if owner(conn).await == present {
            return Ok(());
        }
        sleep(Duration::from_millis(20)).await;
    }
    Err("Private connectivity provider did not reach expected owner state".into())
}

async fn set_case(conn: &Connection, value: &str) -> Result<()> {
    bus_call::<(), _>(
        conn,
        NAME,
        ROOT,
        "org.lumen.TestConnectivity",
        "SetCase",
        &(value,),
    )
    .await
}

fn expect(value: bool, detail: &str) -> Result<()> {
    if value { Ok(()) } else { Err(detail.into()) }
}

async fn run(output: PathBuf, binary: PathBuf) -> Result<()> {
    fs::create_dir(&output)?;
    let private = tempfile::Builder::new()
        .prefix("lumen-connectivity-")
        .tempdir()?;
    let root = private.path();
    let mut processes = Processes(Vec::new());
    let daemon = env::var_os("LUMEN_DBUS_DAEMON")
        .map(PathBuf::from)
        .unwrap_or_else(|| "dbus-daemon".into());
    let current = env::current_exe()?;
    let guard_dir = output.join("bin");
    fs::create_dir(&guard_dir)?;
    symlink(&current, guard_dir.join("dbus-daemon"))?;
    let config = output.join("bus.conf");
    fs::write(
        &config,
        format!(
            "<busconfig><type>session</type><listen>unix:tmpdir={}</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            root.display()
        ),
    )?;
    let log = File::create(output.join("bus.log"))?;
    let mut command = Command::new(guard_dir.join("dbus-daemon"));
    command
        .args(["--nofork", "--print-address=1"])
        .arg(format!("--config-file={}", config.display()))
        .env("LUMEN_REAL_DBUS_DAEMON", &daemon)
        .stdout(Stdio::piped())
        .stderr(log);
    let mut bus = command.spawn()?;
    let mut ready = libc::pollfd {
        fd: bus.stdout.as_ref().ok_or("Missing bus output")?.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // The private bus must announce its address promptly, including when the guard rejects it.
    let observed = unsafe { libc::poll(&mut ready, 1, 5000) };
    if observed <= 0 || ready.revents & libc::POLLIN == 0 {
        let _ = bus.kill();
        let _ = bus.wait();
        return Err("Private bus failed to start within five seconds".into());
    }
    let mut address = String::new();
    BufReader::new(bus.stdout.take().ok_or("Missing bus output")?).read_line(&mut address)?;
    expect(
        address.starts_with("unix:"),
        "Private bus did not publish an address",
    )?;
    let address = address.trim().to_owned();
    processes.0.push(bus);
    let conn = zbus::connection::Builder::address(address.as_str())?
        .build()
        .await?;
    let names: Vec<String> = bus_call(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListActivatableNames",
        &(),
    )
    .await?;
    expect(
        names == ["org.freedesktop.DBus"],
        "Private connectivity bus can activate non-bus services",
    )?;
    let launch = |index: u8, processes: &mut Processes| -> Result<()> {
        let mut cmd = Command::new(&current);
        cmd.arg("--serve");
        command_env(&mut cmd, root, &address);
        cmd.stdout(File::create(output.join(format!("provider-{index}.log")))?)
            .stderr(Stdio::null());
        processes.0.push(cmd.spawn()?);
        Ok(())
    };
    launch(1, &mut processes)?;
    wait_owner(&conn, true).await?;
    let mut records = Vec::new();
    for (case, phase, connectivity) in [
        ("full", "connected", "full"),
        ("portal", "connected", "portal"),
        ("limited", "connected", "limited"),
        ("none", "connected", "none"),
        ("unknown", "connected", "unknown"),
        ("disabled", "disabled", "unknown"),
        ("disconnected", "disconnected", "unknown"),
        ("associating", "associating", "unknown"),
        ("acquiring", "acquiring", "unknown"),
        ("unchecked", "connected", "unknown"),
        ("unavailable", "unavailable", "unknown"),
    ] {
        set_case(&conn, case).await?;
        let value = snapshot(root, &address, &binary, Some("wlan0"))?;
        expect(
            value["phase"] == phase && value["connectivity"] == connectivity,
            &format!("Wrong connectivity transition for {case}: {value}"),
        )?;
        records.push(json!({"case":case,"status":value}));
    }
    expect(
        snapshot(root, &address, &binary, None)?["phase"] == "unavailable",
        "No-interface unavailable phase was lost",
    )?;
    set_case(&conn, "owner-loss").await?;
    expect(
        snapshot(root, &address, &binary, Some("wlan0"))?["statusSource"] == "supplicant",
        "Snapshot accepted a retired NetworkManager owner",
    )?;
    wait_owner(&conn, false).await?;
    processes.0[1].kill()?;
    processes.0[1].wait()?;
    launch(2, &mut processes)?;
    wait_owner(&conn, true).await?;
    expect(
        snapshot(root, &address, &binary, Some("wlan0"))?["connectivity"] == "full",
        "Replacement NetworkManager owner did not refresh",
    )?;
    processes.0[2].kill()?;
    processes.0[2].wait()?;
    wait_owner(&conn, false).await?;
    expect(
        snapshot(root, &address, &binary, Some("wlan0"))?["statusSource"] == "supplicant",
        "Missing NetworkManager service did not clear facts",
    )?;
    processes.0[0].kill()?;
    processes.0[0].wait()?;
    let started = Instant::now();
    expect(
        snapshot(root, &address, &binary, Some("wlan0"))?["statusSource"] == "supplicant",
        "Dead private bus did not clear facts",
    )?;
    expect(
        started.elapsed() < Duration::from_secs(3),
        "Dead private bus did not fail promptly",
    )?;
    fs::write(
        output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({
                "passed":true,"cases":records,"checks":[
                    "actual per-device transitions","global Ethernet full never overrides Wi-Fi",
                    "unchecked service unknown","owner loss refuses snapshot","new owner fresh facts",
                    "service loss clears facts","activation-free private bus","actual bus daemon loss"
                ]
            }))?
        ),
    )?;
    println!("PASS actual private connectivity transitions and owner loss/replacement");
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
    let mut output = None;
    let mut client = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--serve") if output.is_none() && client.is_none() && args.next().is_none() => return serve().await,
            Some("--output") => output = Some(PathBuf::from(args.next().ok_or("--output requires a path")?)),
            Some("--client") => client = Some(PathBuf::from(args.next().ok_or("--client requires a path")?)),
            _ => return Err("usage: lumen-network-connectivity-provider-fixture --output NEW-DIRECTORY --client BINARY".into()),
        }
    }
    run(
        output.ok_or("--output is required")?,
        client.ok_or("--client is required")?,
    )
    .await
}
