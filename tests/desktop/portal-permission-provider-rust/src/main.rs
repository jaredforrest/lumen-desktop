use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

const NAME: &str = "org.freedesktop.impl.portal.PermissionStore";
const PATH: &str = "/org/freedesktop/impl/portal/PermissionStore";
type Entries = BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<String>>>>;
type Permissions = HashMap<String, Vec<String>>;
type AppResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop")]
enum ProviderError {
    #[zbus(name = "portal.Error.NotFound")]
    NotFound(String),
    #[zbus(name = "DBus.Error.AccessDenied")]
    AccessDenied(String),
    #[zbus(name = "DBus.Error.UnknownMethod")]
    UnknownMethod(String),
    #[zbus(name = "DBus.Error.InvalidArgs")]
    InvalidArgs(String),
    #[zbus(name = "DBus.Error.Failed")]
    Failed(String),
}

fn metadata(value: &str) -> Result<OwnedValue, ProviderError> {
    OwnedValue::try_from(zbus::zvariant::Value::new(value))
        .map_err(|error| ProviderError::Failed(error.to_string()))
}

struct State {
    version: u32,
    ignore: bool,
    blocked: String,
    tables: Entries,
    calls: Vec<Json>,
}

impl State {
    fn new() -> Self {
        let mut tables = Entries::new();
        for (table, resource, app, values) in [
            ("devices", "camera", "org.example.First", vec!["yes"]),
            ("devices", "camera", "org.example.Second", vec!["no"]),
            ("devices", "camera", "", vec!["yes"]),
            ("screenshot", "screenshot", "org.example.First", vec!["ask"]),
            (
                "screenshot",
                "screenshot",
                "org.example.Unknown",
                vec!["future-format"],
            ),
            (
                "screencast",
                "private-restore-resource-one",
                "org.example.First",
                vec!["yes"],
            ),
            (
                "screencast",
                "private-restore-resource-two",
                "org.example.First",
                vec!["yes"],
            ),
            (
                "remote-desktop",
                "private-remote-resource",
                "org.example.Second",
                vec!["yes"],
            ),
        ] {
            tables
                .entry(table.into())
                .or_default()
                .entry(resource.into())
                .or_default()
                .insert(app.into(), values.into_iter().map(str::to_owned).collect());
        }
        Self {
            version: 2,
            ignore: false,
            blocked: String::new(),
            tables,
            calls: Vec::new(),
        }
    }

    fn entry(&self, table: &str, resource: &str) -> Result<Permissions, ProviderError> {
        if table == self.blocked {
            return Err(ProviderError::AccessDenied("Fixture policy denied".into()));
        }
        self.tables
            .get(table)
            .and_then(|resources| resources.get(resource))
            .map(|entry| {
                entry
                    .iter()
                    .map(|(app, values)| (app.clone(), values.clone()))
                    .collect()
            })
            .ok_or_else(|| ProviderError::NotFound("No entry".into()))
    }
}

#[derive(Clone)]
struct Store(Arc<Mutex<State>>);

#[interface(name = "org.freedesktop.impl.portal.PermissionStore")]
impl Store {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        self.0.lock().unwrap().version
    }

    fn list(&self, table: &str) -> Result<Vec<String>, ProviderError> {
        let state = self.0.lock().unwrap();
        if table == state.blocked {
            return Err(ProviderError::AccessDenied("Fixture policy denied".into()));
        }
        Ok(state
            .tables
            .get(table)
            .map(|rows| rows.keys().cloned().collect())
            .unwrap_or_default())
    }

    fn lookup(
        &self,
        table: &str,
        resource: &str,
    ) -> Result<(Permissions, OwnedValue), ProviderError> {
        let values = self.0.lock().unwrap().entry(table, resource)?;
        Ok((values, metadata("PRIVATE_PROVIDER_METADATA_DO_NOT_EXPORT")?))
    }

    async fn set_permission(
        &self,
        table: &str,
        create: bool,
        resource: &str,
        app: &str,
        values: Vec<String>,
        #[zbus(signal_emitter)] context: SignalEmitter<'_>,
    ) -> Result<(), ProviderError> {
        if create {
            return Err(ProviderError::InvalidArgs("Create is unsupported".into()));
        }
        let changed = {
            let mut state = self.0.lock().unwrap();
            let mut entry = state.entry(table, resource)?;
            state
                .calls
                .push(json!(["SetPermission", table, resource, app, values]));
            if state.ignore {
                None
            } else {
                entry.insert(app.into(), values);
                state
                    .tables
                    .get_mut(table)
                    .unwrap()
                    .insert(resource.into(), entry.clone().into_iter().collect());
                Some(entry)
            }
        };
        if let Some(permissions) = changed {
            Self::changed(
                &context,
                table,
                resource,
                false,
                metadata("private")?,
                permissions,
            )
            .await
            .map_err(|error| ProviderError::Failed(error.to_string()))?;
        }
        Ok(())
    }

    async fn delete_permission(
        &self,
        table: &str,
        resource: &str,
        app: &str,
        #[zbus(signal_emitter)] context: SignalEmitter<'_>,
    ) -> Result<(), ProviderError> {
        let changed = {
            let mut state = self.0.lock().unwrap();
            if state.version < 2 {
                return Err(ProviderError::UnknownMethod("Not supported".into()));
            }
            let mut entry = state.entry(table, resource)?;
            state
                .calls
                .push(json!(["DeletePermission", table, resource, app]));
            if state.ignore {
                None
            } else {
                entry.remove(app);
                state
                    .tables
                    .get_mut(table)
                    .unwrap()
                    .insert(resource.into(), entry.clone().into_iter().collect());
                Some(entry)
            }
        };
        if let Some(permissions) = changed {
            Self::changed(
                &context,
                table,
                resource,
                false,
                metadata("private")?,
                permissions,
            )
            .await
            .map_err(|error| ProviderError::Failed(error.to_string()))?;
        }
        Ok(())
    }

    #[zbus(signal)]
    async fn changed(
        context: &SignalEmitter<'_>,
        table: &str,
        resource: &str,
        deleted: bool,
        data: OwnedValue,
        permissions: Permissions,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
struct Control(Arc<Mutex<State>>);

#[interface(name = "org.example.PermissionFixture")]
impl Control {
    fn dump(&self) -> String {
        let state = self.0.lock().unwrap();
        json!({"tables": state.tables, "calls": state.calls}).to_string()
    }

    fn configure(&self, version: u32, ignore: bool, blocked: &str) {
        let mut state = self.0.lock().unwrap();
        state.version = version;
        state.ignore = ignore;
        state.blocked = blocked.into();
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--ready")) {
        return Err("Usage: lumen-portal-permission-provider-fixture --ready PATH".into());
    }
    let ready = args.next().ok_or("Missing ready path")?;
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    let state = Arc::new(Mutex::new(State::new()));
    let _connection = zbus::connection::Builder::session()?
        .name(NAME)?
        .serve_at(PATH, Store(state.clone()))?
        .serve_at(PATH, Control(state))?
        .build()
        .await?;
    fs::write(ready, "ready")?;
    std::future::pending::<()>().await;
    Ok(())
}
