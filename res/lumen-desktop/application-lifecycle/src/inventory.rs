//! Bounded preview membership observations, independent of presentation aliases.
use crate::{Lifecycle, Result, mru};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::File,
    io::Read,
};

pub const MAX_VIEWS: usize = 1024;
pub const MAX_BYTES: usize = 1024 * 1024;
const MAX_REVISION: u64 = 9_007_199_254_740_991;
const DOMAIN: &[u8] = b"Lumen.ManagedWindowInventory.v1\0";
#[derive(Debug, PartialEq)]
struct View {
    node: Value,
    canonical: Value,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_exhaustion_never_wraps_or_revives() {
        let inventory = Inventory::default();
        inventory.state.borrow_mut().value = MAX_REVISION;
        inventory.observe_event(&json!({"change":"new"}));
        assert!(!inventory.state.borrow().valid);
        assert_eq!(inventory.state.borrow().value, MAX_REVISION);
        inventory.observe_event(&json!({"change":"close"}));
        assert!(!inventory.state.borrow().valid);
    }
}
#[derive(Debug)]
pub struct Inventory {
    observer: Option<String>,
    views: Option<Vec<View>>,
    state: RefCell<Revision>,
}
#[derive(Debug)]
struct Revision {
    value: u64,
    last: Option<String>,
    valid: bool,
}
impl Revision {
    fn advance(&mut self) {
        self.last = None;
        if let Some(next) = self
            .value
            .checked_add(1)
            .filter(|next| *next <= MAX_REVISION)
        {
            self.value = next;
        } else {
            self.valid = false;
        }
    }
}
impl Default for Inventory {
    fn default() -> Self {
        let mut bytes = [0_u8; 32];
        let observer = File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .ok()
            .map(|()| bytes.iter().map(|byte| format!("{byte:02x}")).collect());
        Self {
            observer,
            views: None,
            state: RefCell::new(Revision {
                value: 0,
                last: None,
                valid: true,
            }),
        }
    }
}
fn text(value: &Value, maximum: usize) -> Result<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    let value = value
        .as_str()
        .filter(|s| s.len() <= maximum && !s.chars().any(char::is_control))
        .ok_or("Invalid inventory text")?;
    Ok(json!(value))
}
fn number(value: &Value, minimum: i64, maximum: i64) -> Result<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    value
        .as_i64()
        .filter(|n| *n >= minimum && *n <= maximum)
        .map(|n| json!(n))
        .ok_or("Invalid inventory number".into())
}
fn relationships(value: &Value) -> Result<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    let object = value
        .as_object()
        .filter(|v| {
            v.len() == 4
                && ["version", "parent_supported", "parent", "modal"]
                    .iter()
                    .all(|k| v.contains_key(*k))
        })
        .ok_or("Invalid inventory relationships")?;
    if object["version"] != 1
        || !object["parent_supported"].is_boolean()
        || !(object["modal"].is_null() || object["modal"].is_boolean())
        || (object["parent_supported"] == false
            && (!object["parent"].is_null() || !object["modal"].is_null()))
    {
        return Err("Invalid inventory relationships".into());
    }
    Ok(json!([
        1,
        object["parent_supported"],
        number(&object["parent"], 0, i64::MAX)?,
        object["modal"]
    ]))
}
fn view(node: &Value) -> Result<View> {
    if !node.is_object() {
        return Err("Invalid managed inventory view".into());
    }
    let id = node["id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or("Missing managed view identity")?;
    let foreign = node["foreign_toplevel_identifier"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
        .ok_or("Missing managed surface identifier")?;
    let shell = text(&node["shell"], 64)?;
    let pid = number(&node["pid"], -1, i64::from(u32::MAX))?;
    let window = number(&node["window"], 0, i64::from(u32::MAX))?;
    let app = text(&node["app_id"], 256)?;
    let properties = &node["window_properties"];
    if !(properties.is_null() || properties.is_object()) {
        return Err("Invalid inventory X11 properties".into());
    }
    let canonical = json!([
        id,
        foreign,
        shell,
        pid,
        window,
        app,
        text(&properties["class"], 256)?,
        text(&properties["instance"], 256)?,
        text(&properties["window_role"], 256)?,
        number(&properties["transient_for"], 0, i64::from(u32::MAX))?,
        relationships(&node["lumen_relationships"])?
    ]);
    Ok(View {
        node: json!({"id":id,"foreign_toplevel_identifier":foreign,"shell":shell,"pid":pid,"window":window,"app_id":app}),
        canonical,
    })
}
fn identity(value: &Value) -> Result<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    if !crate::valid_identity(value) {
        return Err("Invalid inventory process identity".into());
    }
    Ok(json!([
        value["pid"],
        value["start"],
        value["device"],
        value["inode"]
    ]))
}
fn hex(value: &Value, length: usize) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == length
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn valid_witness(value: &Value) -> bool {
    value.as_object().is_some_and(|v| {
        v.len() == 6
            && [
                "version", "observer", "epoch", "digest", "count", "revision",
            ]
            .iter()
            .all(|k| v.contains_key(*k))
    }) && value["version"] == 1
        && hex(&value["observer"], 64)
        && hex(&value["epoch"], 24)
        && hex(&value["digest"], 64)
        && value["count"]
            .as_u64()
            .is_some_and(|n| n <= MAX_VIEWS as u64)
        && value["revision"]
            .as_u64()
            .is_some_and(|n| n <= MAX_REVISION)
}
struct Fact {
    actual: Value,
    alive: bool,
}
impl Inventory {
    pub fn clear(&mut self) {
        self.views = None;
        self.state.borrow_mut().advance();
    }
    pub fn observe_event(&self, event: &Value) {
        if matches!(
            event["change"].as_str(),
            Some("new" | "close" | "app_id" | "lumen_relationships")
        ) {
            self.state.borrow_mut().advance();
        }
    }
    pub fn refresh(&mut self, nodes: &[Value]) {
        let next = (|| -> Result<Vec<View>> {
            if nodes.len() > MAX_VIEWS {
                return Err("Managed inventory exceeds view limit".into());
            }
            let mut ids = HashSet::new();
            let mut foreign = HashSet::new();
            let mut used = 0_usize;
            let mut views = nodes
                .iter()
                .map(|node| {
                    let row = view(node)?;
                    if !ids.insert(row.node["id"].as_i64().unwrap())
                        || !foreign.insert(
                            row.node["foreign_toplevel_identifier"]
                                .as_str()
                                .unwrap()
                                .to_owned(),
                        )
                    {
                        return Err("Duplicate managed inventory identity".into());
                    }
                    used = used
                        .checked_add(
                            serde_json::to_vec(&row.canonical)
                                .map_err(|_| "Inventory encoding failed")?
                                .len()
                                + 4,
                        )
                        .filter(|used| *used <= MAX_BYTES)
                        .ok_or("Managed inventory exceeds byte limit")?;
                    Ok(row)
                })
                .collect::<Result<Vec<_>>>()?;
            views.sort_by_key(|view| view.node["id"].as_i64().unwrap());
            Ok(views)
        })()
        .ok();
        if next.is_none()
            || self
                .views
                .as_ref()
                .zip(next.as_ref())
                .is_some_and(|(old, new)| old != new)
        {
            self.state.borrow_mut().advance();
        }
        self.views = next;
    }
    pub fn observation(&self, lifecycle: &Lifecycle) -> Result<Value> {
        if !self.state.borrow().valid {
            return Err("Inventory revision exhausted; restart observation".into());
        }
        let views = self.views.as_ref().ok_or("Managed inventory unavailable")?;
        let observer = self
            .observer
            .as_ref()
            .ok_or("Inventory observer unavailable")?;
        let epoch = lifecycle
            .epoch
            .as_ref()
            .filter(|epoch| hex(&json!(epoch), 24))
            .ok_or("Inventory epoch unavailable")?;
        if lifecycle.processes.len() > crate::MAX_PROCESSES {
            return Err("Inventory process limit".into());
        }
        let mut pids = lifecycle.processes.keys().copied().collect::<BTreeSet<_>>();
        for view in views {
            if let Some(pid) = view.node["pid"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|pid| *pid >= 2)
            {
                pids.insert(pid);
            }
        }
        let mut facts = BTreeMap::new();
        let mut hasher = Sha256::new();
        hasher.update(DOMAIN);
        hasher.update(observer.as_bytes());
        hasher.update(epoch.as_bytes());
        let mut used = DOMAIN.len() + observer.len() + epoch.len();
        let mut append = |section: u8, value: &Value| -> Result<()> {
            let encoded = serde_json::to_vec(value).map_err(|_| "Inventory encoding failed")?;
            used = used
                .checked_add(encoded.len() + 5)
                .filter(|used| *used <= MAX_BYTES)
                .ok_or("Managed inventory exceeds byte limit")?;
            hasher.update([section]);
            hasher.update((encoded.len() as u32).to_be_bytes());
            hasher.update(&encoded);
            Ok(())
        };
        for pid in pids {
            let actual = crate::source_process_identity(pid).unwrap_or(Value::Null);
            let registered = lifecycle.processes.get(&pid);
            let alive = registered.is_some_and(|process| {
                !crate::exited(&process.handle) && process.identity == actual
            });
            if registered.is_some_and(|process| {
                process.apps.len() > crate::MAX_APP_IDS
                    || process.apps.iter().any(|app| !crate::valid_app_id(app))
            }) {
                return Err("Invalid inventory process advertisements".into());
            }
            let apps = registered
                .map(|process| process.apps.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let canonical = json!([
                pid,
                identity(&actual)?,
                identity(registered.map(|p| &p.identity).unwrap_or(&Value::Null))?,
                apps,
                alive
            ]);
            append(0, &canonical)?;
            facts.insert(pid, Fact { actual, alive });
        }
        let mut ordered = views.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|view| view.node["id"].as_i64().unwrap());
        let mut supported = Vec::new();
        for view in ordered {
            let fact = view.node["pid"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .and_then(|pid| facts.get(&pid));
            let record = lifecycle.native_record(&view.node).filter(|record| {
                fact.is_some_and(|fact| {
                    fact.alive
                        && !fact.actual.is_null()
                        && record["instanceId"] == mru::instance(&fact.actual)
                })
            });
            append(1, &json!([view.canonical, record.is_some()]))?;
            if let Some(record) = record {
                supported.push(record);
            }
        }
        // Source and captures come from this same immutable publication. A
        // process changing between samples makes native observation unavailable.
        let source = &lifecycle.current_source_focus;
        if source["kind"] == "native" {
            let view = views
                .iter()
                .find(|view| view.node["id"] == source["conId"])
                .ok_or("Inventory source retired")?;
            let fact = view.node["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok())
                .and_then(|pid| facts.get(&pid))
                .ok_or("Inventory source process unavailable")?;
            if fact.actual.is_null()
                || source["instanceId"] != mru::instance(&fact.actual)
                || source["appId"] != view.node["app_id"]
                || source["foreignId"] != view.node["foreign_toplevel_identifier"]
            {
                return Err("Inventory source process changed".into());
            }
        }
        let captures = lifecycle
            .window_history
            .captures(lifecycle)
            .into_iter()
            .filter(|capture| {
                let mut row = capture.clone();
                row.as_object_mut().unwrap().remove("epoch");
                row.as_object_mut().unwrap().remove("groupInstances");
                supported.contains(&row)
            })
            .collect::<Vec<_>>();
        drop(append);
        let digest = format!("{:x}", hasher.finalize());
        let mut revision = self.state.borrow_mut();
        if revision.last.as_ref().is_some_and(|last| *last != digest) {
            revision.advance();
        }
        if !revision.valid {
            return Err("Inventory revision exhausted; restart observation".into());
        }
        revision.last = Some(digest.clone());
        let witness = json!({"version":1,"observer":observer,"epoch":epoch,"digest":digest,"count":views.len(),"revision":revision.value});
        Ok(
            json!({"version":1,"epoch":epoch,"inventoryWitness":witness,"captures":captures,"source":source}),
        )
    }
    pub fn require(&self, lifecycle: &Lifecycle, captured: &Value) -> Result<()> {
        if !valid_witness(captured) {
            return Err("Invalid captured inventory witness".into());
        }
        if self.observation(lifecycle)?["inventoryWitness"] != *captured {
            return Err("Managed window inventory changed; reopen the switcher".into());
        }
        Ok(())
    }
}
