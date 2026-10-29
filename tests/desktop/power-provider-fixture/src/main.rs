//! Real UPower D-Bus transitions on a private, activation-free bus.
use serde_json::{Value, json};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
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
const NAME: &str = "org.freedesktop.UPower";
const ROOT: &str = "/org/freedesktop/UPower";
const DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";

#[derive(Clone)]
struct Case(Arc<Mutex<String>>);
impl Case {
    fn get(&self) -> String {
        self.0.lock().unwrap().clone()
    }
    fn set(&self, value: &str) {
        *self.0.lock().unwrap() = value.into();
    }
    fn facts(&self) -> (u32, f64) {
        match self.get().as_str() {
            "charging" => (1, 42.),
            "discharging" | "owner-loss" => (2, 42.),
            "low" => (2, 9.),
            "charged" => (4, 100.),
            "empty" => (3, 0.),
            "absent" => (0, 0.),
            "invalid" => (2, 200.),
            _ => (0, 0.),
        }
    }
}

struct Root;
#[interface(name = "org.freedesktop.UPower")]
impl Root {
    fn get_display_device(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(DEVICE).unwrap()
    }
}

struct Control(Case);
#[interface(name = "org.lumen.TestPower")]
impl Control {
    fn set_case(&self, value: &str) {
        self.0.set(value);
    }
}

struct Device(Case);
#[interface(name = "org.freedesktop.UPower.Device")]
impl Device {
    #[zbus(property, name = "IsPresent")]
    fn is_present(&self) -> bool {
        self.0.get() != "absent"
    }
    #[zbus(property, name = "PowerSupply")]
    fn power_supply(&self) -> bool {
        true
    }
    #[zbus(property, name = "Type")]
    fn kind(&self) -> u32 {
        2
    }
    #[zbus(property, name = "Percentage")]
    async fn percentage(&self, #[zbus(connection)] conn: &Connection) -> f64 {
        if self.0.get() == "owner-loss" {
            let _ = conn.release_name(NAME).await;
        }
        self.0.facts().1
    }
    #[zbus(property, name = "State")]
    fn state(&self) -> u32 {
        self.0.facts().0
    }
    #[zbus(property, name = "TimeToEmpty")]
    fn time_to_empty(&self) -> i64 {
        5400
    }
    #[zbus(property, name = "TimeToFull")]
    fn time_to_full(&self) -> i64 {
        1200
    }
}

async fn serve() -> Result<()> {
    let case = Case(Arc::new(Mutex::new("discharging".into())));
    let _conn = zbus::connection::Builder::system()?
        .name(NAME)?
        .serve_at(ROOT, Root)?
        .serve_at(ROOT, Control(case.clone()))?
        .serve_at(DEVICE, Device(case))?
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

fn snapshot(root: &Path, address: &str, binary: &Path) -> Result<Value> {
    let mut command = Command::new(binary);
    command_env(&mut command, root, address);
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "power probe failed: {}",
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
    Err("Private power provider did not reach expected owner state".into())
}

async fn set_case(conn: &Connection, value: &str) -> Result<()> {
    bus_call::<(), _>(
        conn,
        NAME,
        ROOT,
        "org.lumen.TestPower",
        "SetCase",
        &(value,),
    )
    .await
}

fn expect(value: bool, detail: &str) -> Result<()> {
    if value { Ok(()) } else { Err(detail.into()) }
}

async fn run(output: PathBuf) -> Result<()> {
    fs::create_dir(&output)?;
    let private = tempfile::Builder::new()
        .prefix("lumen-power-bus-")
        .tempdir()?;
    let root = private.path();
    let mut processes = Processes(Vec::new());
    let daemon = env::var_os("LUMEN_DBUS_DAEMON")
        .map(PathBuf::from)
        .unwrap_or_else(|| "dbus-daemon".into());
    let binary = env::var_os("LUMEN_POWER_STATUS_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| "lumen-power-status".into());
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
        "Private power bus can activate non-bus services",
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
    let mut checks = Vec::new();
    for case in [
        "charging",
        "discharging",
        "low",
        "charged",
        "empty",
        "absent",
        "invalid",
    ] {
        set_case(&conn, case).await?;
        let value = snapshot(root, &address, &binary)?;
        if case == "absent" {
            expect(
                value["available"] == true
                    && value["present"] == false
                    && value["percentage"].is_null(),
                "Absent battery leaked a percentage",
            )?;
        } else if case == "invalid" {
            expect(
                value["available"] == false && value["percentage"].is_null(),
                "Invalid battery was accepted",
            )?;
        } else {
            let expected = if case == "low" { "discharging" } else { case };
            let percent = match case {
                "low" => 9,
                "charged" => 100,
                "empty" => 0,
                _ => 42,
            };
            expect(
                value["present"] == true
                    && value["state"] == expected
                    && value["percentage"].as_f64() == Some(f64::from(percent))
                    && value["low"] == matches!(case, "low" | "empty"),
                &format!("Wrong battery transition for {case}: {value}"),
            )?;
        }
        checks.push(json!({"case":case,"status":value}));
    }
    set_case(&conn, "owner-loss").await?;
    let value = snapshot(root, &address, &binary)?;
    expect(
        value["available"] == false && value["percentage"].is_null(),
        "Owner loss did not invalidate snapshot",
    )?;
    checks.push(json!({"case":"owner-lost-during-reply","status":value}));
    wait_owner(&conn, false).await?;
    let value = snapshot(root, &address, &binary)?;
    expect(
        value["available"] == false && value["percentage"].is_null(),
        "Absent provider did not invalidate snapshot",
    )?;
    checks.push(json!({"case":"provider-absent","status":value}));
    launch(2, &mut processes)?;
    wait_owner(&conn, true).await?;
    set_case(&conn, "charging").await?;
    let value = snapshot(root, &address, &binary)?;
    expect(
        value["state"] == "charging" && value["percentage"].as_f64() == Some(42.),
        "Replacement provider did not refresh",
    )?;
    checks.push(json!({"case":"provider-replaced","status":value}));
    fs::write(
        output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({
                "passed":true,"privateSystemBus":true,"activationFree":true,"explicitBusConfiguration":true,
                "scope":"Real D-Bus facts and service lifecycle; no physical battery acceptance","checks":checks
            }))?
        ),
    )?;
    println!("PASS private power provider transitions, owner loss and replacement");
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
    match args.next().as_deref() {
        Some(value) if value == "--serve" && args.next().is_none() => serve().await,
        Some(value) if value == "--output" => {
            let output = PathBuf::from(args.next().ok_or("--output requires a path")?);
            expect(args.next().is_none(), "Unexpected argument")?;
            run(output).await
        }
        _ => Err("usage: lumen-power-provider-fixture --output NEW-DIRECTORY".into()),
    }
}
