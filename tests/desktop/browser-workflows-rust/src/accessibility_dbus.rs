//! Private AT-SPI D-Bus implementation with one bounded future per provider call.
use crate::accessibility::{Observer, Provider};
use crate::{Result, fail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::runtime::{Builder, Runtime};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, Proxy};

pub struct BusProvider {
    runtime: Runtime,
    connection: Connection,
}
fn remaining(deadline: Instant) -> Result<Duration> {
    let time = deadline.saturating_duration_since(Instant::now());
    if time.is_zero() {
        Err(fail(
            "Private accessibility observation exceeded its budget",
        ))
    } else {
        Ok(time)
    }
}
impl BusProvider {
    pub fn snapshot(&mut self) -> Result<Vec<Value>> {
        let start = Instant::now();
        Observer {
            provider: self,
            clock: || start.elapsed().as_secs_f64(),
        }
        .snapshot()
    }
    pub fn connect(address: &str) -> Result<Self> {
        if !address.starts_with("unix:path=/run/fixture/") {
            return Err(fail("Explicit private accessibility bus required"));
        }
        let runtime = Builder::new_current_thread().enable_all().build()?;
        let builder = zbus::connection::Builder::address(address)?;
        let connection = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), builder.build()).await
        })??;
        Ok(Self {
            runtime,
            connection,
        })
    }
    fn call<T>(
        &self,
        timeout: Duration,
        future: impl Future<Output = zbus::Result<T>>,
    ) -> Result<T> {
        Ok(self
            .runtime
            .block_on(async { tokio::time::timeout(timeout, future).await })??)
    }
    fn property(
        &self,
        reference: &(String, String),
        name: &str,
        timeout: Duration,
    ) -> Result<String> {
        let connection = self.connection.clone();
        let destination = reference.0.clone();
        let object = reference.1.clone();
        let name = name.to_owned();
        let value: OwnedValue = self.call(timeout, async move {
            let proxy = Proxy::new(
                &connection,
                destination.as_str(),
                object.as_str(),
                "org.freedesktop.DBus.Properties",
            )
            .await?;
            proxy
                .call("Get", &("org.a11y.atspi.Accessible", name.as_str()))
                .await
        })?;
        Ok(String::try_from(value)?)
    }
    pub fn focus(&self, reference: &(String, String), timeout: Duration) -> Result<bool> {
        let connection = self.connection.clone();
        let destination = reference.0.clone();
        let object = reference.1.clone();
        self.call(timeout, async move {
            let proxy = Proxy::new(
                &connection,
                destination.as_str(),
                object.as_str(),
                "org.a11y.atspi.Component",
            )
            .await?;
            proxy.call("GrabFocus", &()).await
        })
    }
    pub fn bounds(
        &self,
        reference: &(String, String),
        timeout: Duration,
    ) -> Result<(i32, i32, i32, i32)> {
        let connection = self.connection.clone();
        let destination = reference.0.clone();
        let object = reference.1.clone();
        self.call(timeout, async move {
            let proxy = Proxy::new(
                &connection,
                destination.as_str(),
                object.as_str(),
                "org.a11y.atspi.Component",
            )
            .await?;
            proxy.call("GetExtents", &(0_u32,)).await
        })
    }
    pub fn focus_captured(&mut self, row: &Value) -> Result<()> {
        let reference = captured_reference(row)?;
        let current = self.describe(&reference, Duration::from_secs(3))?;
        for key in ["pid", "name", "role"] {
            if current[key] != row[key] {
                return Err(fail("Captured application control changed"));
            }
        }
        if current["states"]["showing"] != true || current["states"]["sensitive"] != true {
            return Err(fail("Captured application control changed"));
        }
        if !self.focus(&reference, Duration::from_millis(500))? {
            return Err(fail(
                "Application control does not support accessible focus",
            ));
        }
        Ok(())
    }
    pub fn bounds_captured(&mut self, row: &Value) -> Result<(i32, i32, i32, i32)> {
        let reference = captured_reference(row)?;
        let current = self.describe(&reference, Duration::from_secs(3))?;
        for key in ["pid", "name", "role", "description"] {
            if current[key] != row[key] {
                return Err(fail("Captured application control changed"));
            }
        }
        if current["states"]["showing"] != true || current["states"]["sensitive"] != true {
            return Err(fail("Captured application control changed"));
        }
        let bounds = self.bounds(&reference, Duration::from_millis(500))?;
        if bounds.2 <= 0 || bounds.3 <= 0 {
            return Err(fail("Application control has no painted extent"));
        }
        Ok(bounds)
    }
}
impl Provider for &mut BusProvider {
    fn list_names(&mut self, timeout: Duration) -> Result<Vec<String>> {
        (**self).list_names(timeout)
    }
    fn pid(&mut self, owner: &str, timeout: Duration) -> Result<u32> {
        (**self).pid(owner, timeout)
    }
    fn describe(&mut self, reference: &(String, String), timeout: Duration) -> Result<Value> {
        (**self).describe(reference, timeout)
    }
    fn children(
        &mut self,
        reference: &(String, String),
        timeout: Duration,
    ) -> Result<Vec<(String, String)>> {
        (**self).children(reference, timeout)
    }
}
fn captured_reference(row: &Value) -> Result<(String, String)> {
    let values = row["reference"].as_array().ok_or("accessible reference")?;
    Ok((
        values
            .first()
            .and_then(Value::as_str)
            .ok_or("accessible owner")?
            .into(),
        values
            .get(1)
            .and_then(Value::as_str)
            .ok_or("accessible object")?
            .into(),
    ))
}
pub trait AccessibilityHost {
    fn launch(
        &mut self,
        command: &[String],
        log: &str,
        environment: &BTreeMap<String, String>,
    ) -> Result<u32>;
    fn wait_name(&mut self, address: &str, name: &str, owner_pid: u32) -> Result<()>;
}
pub fn start(
    prefix: &Path,
    environment: &mut BTreeMap<String, String>,
    host: &mut impl AccessibilityHost,
) -> Result<BusProvider> {
    let session = environment
        .get("DBUS_SESSION_BUS_ADDRESS")
        .ok_or("private session bus")?
        .clone();
    let launcher = prefix.join("libexec/at-spi-bus-launcher");
    let launcher_pid = host.launch(
        &[
            launcher.display().to_string(),
            "--launch-immediately".into(),
            "--a11y=1".into(),
            "--screen-reader=1".into(),
        ],
        "a11y-bus.log",
        environment,
    )?;
    host.wait_name(&session, "org.a11y.Bus", launcher_pid)?;
    let runtime = Builder::new_current_thread().enable_all().build()?;
    let builder = zbus::connection::Builder::address(session.as_str())?;
    let connection = runtime.block_on(async {
        tokio::time::timeout(Duration::from_millis(500), builder.build()).await
    })??;
    let address: String = runtime.block_on(async {
        tokio::time::timeout(Duration::from_millis(500), async {
            let proxy =
                Proxy::new(&connection, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus").await?;
            proxy.call("GetAddress", &()).await
        })
        .await
    })??;
    if !address.starts_with("unix:path=/run/fixture/") {
        return Err(fail("Accessibility bus escaped private runtime"));
    }
    environment.insert("AT_SPI_BUS_ADDRESS".into(), address.clone());
    environment.insert("GTK_A11Y".into(), "atspi".into());
    environment.insert(
        "GTK_MODULES".into(),
        prefix
            .join("lib/gtk-2.0/modules/libatk-bridge.so")
            .display()
            .to_string(),
    );
    environment.insert("NO_AT_BRIDGE".into(), "0".into());
    let registry = prefix.join("libexec/at-spi2-registryd");
    let registry_pid = host.launch(
        &[registry.display().to_string()],
        "a11y-registry.log",
        environment,
    )?;
    host.wait_name(&address, "org.a11y.atspi.Registry", registry_pid)?;
    BusProvider::connect(&address)
}
impl Provider for BusProvider {
    fn list_names(&mut self, timeout: Duration) -> Result<Vec<String>> {
        let connection = self.connection.clone();
        self.call(timeout, async move {
            let proxy = Proxy::new(
                &connection,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
            )
            .await?;
            proxy.call("ListNames", &()).await
        })
    }
    fn pid(&mut self, owner: &str, timeout: Duration) -> Result<u32> {
        let connection = self.connection.clone();
        let owner = owner.to_owned();
        self.call(timeout, async move {
            let proxy = Proxy::new(
                &connection,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
            )
            .await?;
            proxy
                .call("GetConnectionUnixProcessID", &(owner.as_str(),))
                .await
        })
    }
    fn describe(&mut self, reference: &(String, String), timeout: Duration) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        let name = self.property(reference, "Name", remaining(deadline)?)?;
        let description = self.property(reference, "Description", remaining(deadline)?)?;
        let connection = self.connection.clone();
        let destination = reference.0.clone();
        let object = reference.1.clone();
        let words: Vec<u32> = self.call(remaining(deadline)?, async move {
            let proxy = Proxy::new(
                &connection,
                destination.as_str(),
                object.as_str(),
                "org.a11y.atspi.Accessible",
            )
            .await?;
            proxy.call("GetState", &()).await
        })?;
        let connection = self.connection.clone();
        let destination = reference.0.clone();
        let object = reference.1.clone();
        let role: String = self.call(remaining(deadline)?, async move {
            let proxy = Proxy::new(
                &connection,
                destination.as_str(),
                object.as_str(),
                "org.a11y.atspi.Accessible",
            )
            .await?;
            proxy.call("GetRoleName", &()).await
        })?;
        let pid = self.pid(&reference.0, remaining(deadline)?)?;
        let mut states = serde_json::Map::new();
        for (label, index) in [
            ("enabled", 8),
            ("focusable", 11),
            ("focused", 12),
            ("modal", 16),
            ("sensitive", 24),
            ("showing", 25),
            ("visible", 30),
        ] {
            states.insert(
                label.into(),
                json!(
                    words
                        .get(index / 32)
                        .is_some_and(|word| word & (1 << (index % 32)) != 0)
                ),
            );
        }
        Ok(
            json!({"reference":[reference.0,reference.1],"pid":pid,"name":name,"description":description,"role":role,"states":states}),
        )
    }
    fn children(
        &mut self,
        reference: &(String, String),
        timeout: Duration,
    ) -> Result<Vec<(String, String)>> {
        let connection = self.connection.clone();
        let destination = reference.0.clone();
        let object = reference.1.clone();
        let rows: Vec<(String, OwnedObjectPath)> = self.call(timeout, async move {
            let proxy = Proxy::new(
                &connection,
                destination.as_str(),
                object.as_str(),
                "org.a11y.atspi.Accessible",
            )
            .await?;
            proxy.call("GetChildren", &()).await
        })?;
        Ok(rows
            .into_iter()
            .map(|(name, path)| (name, path.to_string()))
            .collect())
    }
}
