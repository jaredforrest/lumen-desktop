use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::OwnedValue;

const NAME: &str = "org.freedesktop.impl.portal.PermissionStore";
const PATH: &str = "/org/freedesktop/impl/portal/PermissionStore";
const SCOPE: &str = "These are saved portal decisions, not a list of active capture. Native apps can have access outside portals. Microphone access is not managed here.";
const CATALOGUE: &[(&str, Option<&str>, &str)] = &[
    ("devices", Some("camera"), "Camera"),
    ("screenshot", Some("screenshot"), "Screenshots"),
    ("screencast", None, "Saved screen sharing"),
    ("remote-desktop", None, "Saved remote desktop"),
];

fn digest(value: &str) -> String { format!("{:x}", Sha256::digest(value.as_bytes())) }
fn tuple3(a: &str, b: &str, c: &str) -> String {
    format!("[{}, {}, {}]", json!(a), json!(b), json!(c))
}
fn identity(table: &str, resource: &str, app: &str) -> String { digest(&tuple3(table, resource, app)) }
fn revision(owner: &str, id: &str, permissions: &[String]) -> String {
    let values = permissions.iter().map(|value| json!(value).to_string()).collect::<Vec<_>>().join(", ");
    digest(&format!("[{}, {}, [{}]]", json!(owner), json!(id), values))
}
fn safe_name(value: &str) -> bool { !value.is_empty() && value.chars().count() <= 512 && !value.chars().any(|ch| ch.is_control()) }
fn unavailable(message: &str) -> Value { json!({"available":false,"rows":[],"scope":SCOPE,"error":message}) }
fn not_found(error: &zbus::Error) -> bool { matches!(error, zbus::Error::MethodError(name, _, _) if name.as_str() == "org.freedesktop.portal.Error.NotFound") }

async fn call<R, B>(proxy: &zbus::Proxy<'_>, name: &str, body: &B) -> Result<R, zbus::Error>
where R: for<'de> zbus::zvariant::DynamicDeserialize<'de>, B: serde::ser::Serialize + zbus::zvariant::DynamicType {
    match tokio::time::timeout(Duration::from_secs(4), proxy.call(name, body)).await {
        Ok(result) => result,
        Err(_) => Err(zbus::Error::Failure("Portal request timed out".into())),
    }
}

#[derive(Clone)]
struct Target { table: String, resource: String, app: String, values: Vec<String> }

struct Provider {
    conn: zbus::Connection,
    owner: String,
    version: u32,
}

impl Provider {
    async fn new() -> Result<Self, String> {
        let conn = zbus::Connection::session().await.map_err(|error| error.to_string())?;
        let bus = zbus::Proxy::new(&conn, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus").await.map_err(|error| error.to_string())?;
        let owner: String = call(&bus, "GetNameOwner", &(NAME,)).await.map_err(|error| error.to_string())?;
        if !owner.starts_with(':') { return Err("Permission provider owner is unavailable".into()); }
        let properties = zbus::Proxy::new(&conn, owner.as_str(), PATH, "org.freedesktop.DBus.Properties").await.map_err(|error| error.to_string())?;
        let value: OwnedValue = call(&properties, "Get", &(NAME, "version")).await.map_err(|error| error.to_string())?;
        let version = u32::try_from(value).map_err(|error| error.to_string())?;
        Ok(Self { conn, owner, version })
    }

    async fn proxy(&self) -> Result<zbus::Proxy<'_>, String> {
        zbus::Proxy::new(&self.conn, self.owner.as_str(), PATH, NAME).await.map_err(|error| error.to_string())
    }

    async fn lookup(&self, table: &str, resource: &str) -> Result<HashMap<String, Vec<String>>, zbus::Error> {
        let proxy = self.proxy().await.map_err(zbus::Error::Failure)?;
        let (permissions, _metadata): (HashMap<String, Vec<String>>, OwnedValue) = call(&proxy, "Lookup", &(table, resource)).await?;
        Ok(permissions)
    }

    async fn entries(&self) -> Result<(Vec<Value>, HashMap<String, Target>, Vec<Value>), String> {
        let proxy = self.proxy().await?;
        let mut rows = Vec::new();
        let mut targets = HashMap::new();
        let mut coverage = Vec::new();
        for &(table, fixed, capability) in CATALOGUE {
            let listed = if let Some(resource) = fixed { Ok(vec![resource.to_owned()]) }
                else { call::<Vec<String>, _>(&proxy, "List", &(table,)).await };
            let resources = match listed {
                Ok(resources) => resources,
                Err(error) if not_found(&error) => { coverage.push(json!({"capability":capability,"state":"available","count":0})); continue; },
                Err(_) => { coverage.push(json!({"capability":capability,"state":"unavailable","count":0})); continue; },
            };
            let mut scoped_rows = Vec::new();
            let mut scoped_targets = HashMap::new();
            let mut failed = false;
            for resource in resources {
                let permissions = match self.lookup(table, &resource).await {
                    Ok(permissions) => permissions,
                    Err(error) if not_found(&error) => continue,
                    Err(_) => { failed = true; break; },
                };
                for (app, values) in permissions {
                    let id = identity(table, &resource, &app);
                    let known = values.len() == 1 && matches!(values[0].as_str(), "yes" | "no" | "ask");
                    let named = safe_name(&app);
                    let saved = fixed.is_none();
                    let mut actions = Vec::new();
                    if named && known {
                        if !saved && values[0] != "no" { actions.push("deny"); }
                        if self.version >= 2 { actions.push(if saved { "forget" } else { "reset" }); }
                    }
                    let label = if known { match values[0].as_str() { "yes" => "Allowed", "no" => "Denied", _ => "Ask" } } else { "Unrecognised decision" };
                    let detail = if !named { "The provider has no distinct app ID. This entry cannot safely identify one client." }
                        else if !known { "Unrecognised provider format. This entry is read-only." }
                        else if saved { "Forgets this saved session only. It does not stop active sharing." }
                        else { "Applies to future portal requests. Existing device access may continue." };
                    scoped_rows.push(json!({"id":id,"appId":if named {app.as_str()} else {""},"appName":if named {app.as_str()} else {"Unidentified portal app"},"capability":capability,
                        "resource":if saved {format!("Session {}", &digest(&resource)[..16])} else {resource.clone()},"decision":label,
                        "revision":revision(&self.owner,&id,&values),"actions":actions,"detail":detail}));
                    scoped_targets.insert(id, Target { table:table.into(), resource:resource.clone(), app, values });
                }
            }
            if failed { coverage.push(json!({"capability":capability,"state":"unavailable","count":0})); }
            else {
                let count = scoped_rows.len();
                rows.extend(scoped_rows);
                targets.extend(scoped_targets);
                coverage.push(json!({"capability":capability,"state":"available","count":count}));
            }
        }
        rows.sort_by_key(|row| (row["capability"].as_str().unwrap_or("").to_owned(),row["appId"].as_str().unwrap_or("").to_owned(),row["resource"].as_str().unwrap_or("").to_owned()));
        Ok((rows, targets, coverage))
    }

    async fn snapshot(&self) -> Result<Value, String> {
        let (rows, _, coverage) = self.entries().await?;
        Ok(json!({"available":true,"version":self.version,"rows":rows,"coverage":coverage,"scope":SCOPE}))
    }

    async fn apply(&self, request: &Value) -> Result<Value, String> {
        let object = request.as_object().ok_or("An explicit app and resource confirmation is required.")?;
        if object.len() != 4 || !["action","id","revision","confirmed"].iter().all(|key| object.contains_key(*key)) || object["confirmed"] != true {
            return Err("An explicit app and resource confirmation is required.".into());
        }
        let (rows, targets, _) = self.entries().await?;
        let id = object["id"].as_str().unwrap_or("");
        let row = rows.iter().find(|row| row["id"] == id).ok_or("The saved decision changed. Refresh and confirm its current app and resource.")?;
        if row["revision"] != object["revision"] { return Err("The saved decision changed. Refresh and confirm its current app and resource.".into()); }
        let action = object["action"].as_str().unwrap_or("");
        if !row["actions"].as_array().is_some_and(|actions| actions.iter().any(|item| item == action)) { return Err("This action is not supported for this saved decision.".into()); }
        let target = targets.get(id).ok_or("The saved decision changed. Refresh and confirm its current app and resource.")?;
        let current = self.lookup(&target.table, &target.resource).await.map_err(|_| "The portal permission provider is unavailable or did not complete the request.")?;
        if current.get(&target.app) != Some(&target.values) { return Err("The saved decision changed. Refresh before trying again.".into()); }
        let proxy = self.proxy().await?;
        let result: Result<(), zbus::Error> = if action == "deny" {
            call(&proxy,"SetPermission",&(target.table.as_str(),false,target.resource.as_str(),target.app.as_str(),vec!["no"])).await
        } else {
            call(&proxy,"DeletePermission",&(target.table.as_str(),target.resource.as_str(),target.app.as_str())).await
        };
        result.map_err(|_| "The portal permission provider is unavailable or did not complete the request.")?;
        let mut latest = self.snapshot().await?;
        let updated = latest["rows"].as_array().and_then(|rows| rows.iter().find(|item| item["id"] == id));
        if action == "deny" && updated.is_none_or(|row| row["decision"] != "Denied") || action != "deny" && updated.is_some() {
            return Err("The provider did not confirm the requested change. Refresh to inspect its current state.".into());
        }
        latest["message"] = json!(format!("{} Existing capture may continue; stop it in the sharing app.", if action == "deny" { "Portal decision denied." } else { "Saved portal decision removed." }));
        Ok(latest)
    }
}

#[tokio::main]
async fn main() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut line = String::new();
    let input = BufReader::new(tokio::io::stdin()).read_line(&mut line).await;
    let request = match input.ok().and_then(|_| serde_json::from_str::<Value>(&line).ok()).filter(Value::is_object) {
        Some(value) => value,
        None => { println!("{}", unavailable("Invalid permission request.")); return; }
    };
    let result = match Provider::new().await {
        Ok(provider) => if request.get("action").and_then(Value::as_str) == Some("list") { provider.snapshot().await } else { provider.apply(&request).await },
        Err(_) => Err("The portal permission provider is unavailable or did not complete the request.".into()),
    };
    println!("{}", match result { Ok(value) => value, Err(error) => unavailable(&error) });
}
