//! Explicit private standard chooser providers for installed application fixtures.
use crate::{Result, ensure, error};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

fn immutable_file(path: &str) -> Result<PathBuf> {
    let resolved = Path::new(path).canonicalize()?;
    ensure(
        resolved.starts_with("/nix/store") && resolved.is_file(),
        "Chooser provider must be immutable",
    )?;
    Ok(resolved)
}
fn immutable_dir(path: &str) -> Result<PathBuf> {
    let resolved = Path::new(path).canonicalize()?;
    ensure(
        resolved.starts_with("/nix/store") && resolved.is_dir(),
        "Chooser data must be immutable",
    )?;
    Ok(resolved)
}
fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn wait(label: &str, mut probe: impl FnMut() -> Result<bool>) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if probe().unwrap_or(false) {
            return Ok(());
        }
        if Instant::now() >= end {
            return Err(error(format!("Timed out: {label}")));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn launch(
    path: &Path,
    args: &[String],
    label: &str,
    output: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<Child> {
    let log = File::create(output.join(format!("{label}.log")))?;
    Ok(Command::new(path)
        .args(args)
        .env_clear()
        .envs(environment)
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?)
}
pub struct Chooser {
    pub facts: Value,
    children: Vec<Child>,
}
impl Drop for Chooser {
    fn drop(&mut self) {
        for child in self.children.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}
pub fn start(
    environment: &mut BTreeMap<String, String>,
    manifest: &Path,
    output: &Path,
) -> Result<Chooser> {
    let manifest = immutable_file(manifest.to_str().ok_or("Manifest path")?)?;
    let tools: Value = serde_json::from_slice(&fs::read(&manifest)?)?;
    let mut providers = serde_json::Map::new();
    let mut binaries = BTreeMap::new();
    for key in ["bus", "portal", "backend"] {
        let path = immutable_file(tools[key].as_str().ok_or("Chooser binary missing")?)?;
        providers.insert(key.into(), json!({"path":path,"sha256":digest(&path)?}));
        binaries.insert(key, path);
    }
    let data = immutable_dir(
        tools["backendData"]
            .as_str()
            .ok_or("Chooser data missing")?,
    )?;
    let config = Path::new(
        environment
            .get("XDG_CONFIG_HOME")
            .ok_or("Private XDG config missing")?,
    )
    .join("xdg-desktop-portal");
    fs::create_dir_all(&config)?;
    fs::write(
        config.join("portals.conf"),
        "[preferred]\ndefault=none\norg.freedesktop.impl.portal.FileChooser=gtk\n",
    )?;
    let bus_config = Path::new("/run/fixture/chooser-bus.conf");
    let address = "unix:path=/run/fixture/chooser-bus";
    fs::write(
        bus_config,
        format!(
            "<busconfig><type>session</type><listen>{address}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>"
        ),
    )?;
    let mut facts = json!({"manifest":manifest,"sha256":digest(&manifest)?,"providers":providers});
    let mut children = Vec::new();
    children.push(launch(
        &binaries["bus"],
        &[
            "--nofork".into(),
            format!("--config-file={}", bus_config.display()),
        ],
        "chooser-bus",
        output,
        environment,
    )?);
    wait("owned chooser bus", || {
        Ok(fs::metadata("/run/fixture/chooser-bus")
            .is_ok_and(|metadata| metadata.file_type().is_socket()))
    })?;
    let icon_dirs = tools["iconData"]
        .as_array()
        .ok_or("Chooser icon data missing")?
        .iter()
        .map(|value| immutable_dir(value.as_str().ok_or("Icon path")?))
        .collect::<Result<Vec<_>>>()?;
    environment.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.into());
    environment.insert(
        "XDG_DATA_DIRS".into(),
        std::iter::once(data)
            .chain(icon_dirs)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(":"),
    );
    for (key, value) in [
        ("XDG_CURRENT_DESKTOP", "GNOME"),
        ("GIO_USE_VFS", "local"),
        ("GSETTINGS_BACKEND", "memory"),
        ("GTK_USE_PORTAL", "0"),
    ] {
        environment.insert(key.into(), value.into());
    }
    environment.insert(
        "FONTCONFIG_FILE".into(),
        tools["fontConfig"]
            .as_str()
            .ok_or("Font config missing")?
            .into(),
    );
    let connection = zbus::blocking::connection::Builder::address(address)?.build()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    children.push(launch(
        &binaries["backend"],
        &[],
        "chooser-backend",
        output,
        environment,
    )?);
    wait("owned GTK chooser backend", || {
        Ok(proxy.call::<_, _, bool>(
            "NameHasOwner",
            &("org.freedesktop.impl.portal.desktop.gtk",),
        )?)
    })?;
    children.push(launch(
        &binaries["portal"],
        &[],
        "chooser-portal",
        output,
        environment,
    )?);
    wait("owned standard chooser portal", || {
        Ok(proxy.call::<_, _, bool>("NameHasOwner", &("org.freedesktop.portal.Desktop",))?)
    })?;
    facts["backendPid"] = json!(children[1].id());
    facts["address"] = json!(address);
    facts["activation"] = json!(false);
    facts["systemBus"] = json!(environment.get("DBUS_SYSTEM_BUS_ADDRESS"));
    Ok(Chooser { facts, children })
}
