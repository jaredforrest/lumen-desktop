//! Explicit private standard chooser providers for installed browser tests.
use crate::{Result, fail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub trait PortalHost {
    fn launch(
        &mut self,
        command: &[String],
        log: &str,
        environment: &BTreeMap<String, String>,
    ) -> Result<u32>;
    fn wait_socket(&mut self, path: &Path, owner_pid: u32) -> Result<()>;
    fn wait_name(&mut self, address: &str, name: &str, owner_pid: u32) -> Result<()>;
}
fn immutable(path: &str, kind: &str) -> Result<PathBuf> {
    let path = Path::new(path).canonicalize()?;
    if !path.starts_with("/nix/store")
        || !(if kind == "file" {
            path.is_file()
        } else {
            path.is_dir()
        })
    {
        return Err(fail("Chooser dependency must be immutable"));
    }
    Ok(path)
}
fn hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
pub fn start(
    environment: &mut BTreeMap<String, String>,
    manifest: &Path,
    host: &mut impl PortalHost,
) -> Result<Value> {
    let manifest = immutable(manifest.to_str().ok_or("manifest path")?, "file")?;
    let tools: Value = serde_json::from_slice(&fs::read(&manifest)?)?;
    let mut providers = serde_json::Map::new();
    for key in ["bus", "portal", "backend"] {
        let binary = immutable(tools[key].as_str().ok_or("provider path")?, "file")?;
        providers.insert(key.into(), json!({"path":binary,"sha256":hash(&binary)?}));
    }
    let data = immutable(
        tools["backendData"].as_str().ok_or("backend data")?,
        "directory",
    )?;
    let config = Path::new(
        environment
            .get("XDG_CONFIG_HOME")
            .ok_or("private config home")?,
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
    let bus = host.launch(
        &[
            tools["bus"].as_str().ok_or("bus")?.into(),
            "--nofork".into(),
            format!("--config-file={}", bus_config.display()),
        ],
        "chooser-bus",
        environment,
    )?;
    host.wait_socket(Path::new("/run/fixture/chooser-bus"), bus)?;
    let icon_dirs = tools["iconData"]
        .as_array()
        .ok_or("icon data")?
        .iter()
        .map(|v| immutable(v.as_str().ok_or("icon path")?, "directory"))
        .collect::<Result<Vec<_>>>()?;
    environment.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.into());
    environment.insert(
        "XDG_DATA_DIRS".into(),
        std::iter::once(data)
            .chain(icon_dirs)
            .map(|p| p.display().to_string())
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
        tools["fontConfig"].as_str().ok_or("font config")?.into(),
    );
    let backend = host.launch(
        &[tools["backend"].as_str().ok_or("backend")?.into()],
        "chooser-backend",
        environment,
    )?;
    host.wait_name(address, "org.freedesktop.impl.portal.desktop.gtk", backend)?;
    let portal = host.launch(
        &[tools["portal"].as_str().ok_or("portal")?.into()],
        "chooser-portal",
        environment,
    )?;
    host.wait_name(address, "org.freedesktop.portal.Desktop", portal)?;
    Ok(
        json!({"manifest":manifest,"sha256":hash(&manifest)?,"providers":providers,"backendPid":backend,"address":address,"activation":false,"systemBus":environment.get("DBUS_SYSTEM_BUS_ADDRESS").ok_or("system bus")?}),
    )
}
