//! Exact native window order and immutable preview captures; no title lookup.
use crate::{Lifecycle, Result, mru, valid_app_id};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
const LIMIT: usize = 1024;
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    entries: Vec<Value>,
}
fn foreign(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
}
fn hash(value: &Value) -> bool {
    value.as_str().is_some_and(|id| {
        id.len() == 64
            && id
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}
impl History {
    pub fn from_saved(value: &Value, processes: &[Value]) -> Result<Self> {
        let rows = value
            .as_array()
            .filter(|rows| rows.len() <= LIMIT)
            .ok_or("Invalid native window order")?;
        let mut associations: HashMap<String, HashSet<&str>> = HashMap::new();
        for process in processes {
            if crate::valid_identity(&process["identity"]) {
                if let Some(apps) = process["apps"].as_array() {
                    associations
                        .entry(mru::instance(&process["identity"]))
                        .or_default()
                        .extend(apps.iter().filter_map(Value::as_str));
                }
            }
        }
        let mut ids = HashSet::new();
        let mut foreign_ids = HashSet::new();
        for row in rows {
            let obj = row
                .as_object()
                .filter(|o| {
                    o.len() == 4
                        && ["conId", "foreignId", "appId", "instanceId"]
                            .iter()
                            .all(|key| o.contains_key(*key))
                })
                .ok_or("Invalid native window record")?;
            let id = obj["conId"]
                .as_i64()
                .filter(|id| *id > 0 && ids.insert(*id))
                .ok_or("Invalid native window ID")?;
            let _ = id;
            if !foreign(&row["foreignId"])
                || !foreign_ids.insert(row["foreignId"].as_str().unwrap())
                || !hash(&row["instanceId"])
            {
                return Err("Invalid native surface witness".into());
            }
            let app = row["appId"]
                .as_str()
                .filter(|app| valid_app_id(app))
                .ok_or("Invalid native application")?;
            if !associations
                .get(row["instanceId"].as_str().unwrap())
                .is_some_and(|apps| apps.contains(app))
            {
                return Err("Unassociated native process witness".into());
            }
        }
        Ok(Self {
            entries: rows.clone(),
        })
    }
    pub fn value(&self) -> Value {
        Value::Array(self.entries.clone())
    }
    pub fn retain_processes(&mut self, processes: &[Value]) {
        let mut valid = HashSet::new();
        for process in processes {
            if let Some(apps) = process["apps"].as_array() {
                let instance = mru::instance(&process["identity"]);
                for app in apps {
                    valid.insert((instance.clone(), app.clone()));
                }
            }
        }
        self.entries.retain(|row| {
            valid.contains(&(
                row["instanceId"].as_str().unwrap_or_default().to_owned(),
                row["appId"].clone(),
            ))
        });
    }
    pub fn reconcile(&mut self, current: &[Value]) {
        self.entries.retain(|old| current.contains(old));
        for row in current {
            if self.entries.len() >= LIMIT {
                break;
            }
            if !self.entries.contains(row) {
                self.entries.push(row.clone());
            }
        }
    }
    pub fn promote(&mut self, row: Value) {
        if self.entries.contains(&row) {
            self.entries.retain(|old| old != &row);
            self.entries.insert(0, row);
        }
    }
    pub fn captures(&self, lifecycle: &Lifecycle) -> Vec<Value> {
        self.entries
            .iter()
            .filter_map(|row| lifecycle.capture_record(row))
            .collect()
    }
}
impl Lifecycle {
    pub fn capture_record(&self, row: &Value) -> Option<Value> {
        let apps = self.rows();
        let app = apps.iter().find(|app| app["appId"] == row["appId"])?;
        let mut capture = row.clone();
        capture["epoch"] = json!(self.epoch.as_deref()?);
        capture["groupInstances"] = app["instanceIds"].clone();
        Some(capture)
    }
    pub fn source_focus_matches(&self, captured: &Value) -> bool {
        matches!(captured["kind"].as_str(), Some("native" | "none"))
            && *captured == self.current_source_focus
    }
    pub fn source_focus(&self, nodes: &[Value]) -> Value {
        let Some(epoch) = self.epoch.as_deref() else {
            return Value::Null;
        };
        if nodes.iter().any(|node| !node["focused"].is_boolean()) {
            return json!({"kind":"unsupported","epoch":epoch});
        }
        let focused = nodes
            .iter()
            .filter(|node| node["focused"] == true)
            .collect::<Vec<_>>();
        if focused.is_empty() {
            return json!({"kind":"none","epoch":epoch});
        }
        let unsupported = json!({"kind":"unsupported","epoch":epoch});
        let [node] = focused.as_slice() else {
            return unsupported;
        };
        if node["shell"] != "xdg_shell"
            || !node["window"].is_null()
            || !foreign(&node["foreign_toplevel_identifier"])
        {
            return unsupported;
        }
        let (Some(id), Some(pid), Some(app)) = (
            node["id"].as_i64().filter(|id| *id > 0),
            node["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok()),
            node["app_id"].as_str().filter(|app| {
                !app.is_empty() && app.len() <= 256 && !app.chars().any(char::is_control)
            }),
        ) else {
            return unsupported;
        };
        let Ok(identity) = crate::source_process_identity(pid) else {
            return unsupported;
        };
        json!({"kind":"native","epoch":epoch,"conId":id,"foreignId":node["foreign_toplevel_identifier"],"appId":app,"instanceId":mru::instance(&identity)})
    }
    pub fn native_record(&self, node: &Value) -> Option<Value> {
        if node["shell"] != "xdg_shell"
            || !node["window"].is_null()
            || !foreign(&node["foreign_toplevel_identifier"])
        {
            return None;
        }
        let id = node["id"].as_i64().filter(|id| *id > 0)?;
        let pid = u32::try_from(node["pid"].as_u64()?).ok()?;
        let app = node["app_id"].as_str().filter(|app| valid_app_id(app))?;
        let process = self.processes.get(&pid)?;
        if !process.apps.contains(app)
            || crate::exited(&process.handle)
            || crate::process_identity(pid).ok().as_ref() != Some(&process.identity)
        {
            return None;
        }
        Some(
            json!({"conId":id,"foreignId":node["foreign_toplevel_identifier"],"appId":app,"instanceId":mru::instance(&process.identity)}),
        )
    }
    /// Exact native ordering is usable only when every external surface has a
    /// live witness. Owned shell panels are not external applications.
    pub fn native_coverage_complete(&self, nodes: &[Value]) -> bool {
        nodes.iter().all(|node| {
            if self.native_record(node).is_some() {
                return true;
            }
            let Some(pid) = node["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok()) else {
                return false;
            };
            self.shell_owner.as_ref().is_some_and(|owner| {
                owner["pid"].as_u64() == Some(u64::from(pid))
                    && crate::source_process_identity(pid).ok().as_ref() == Some(owner)
            })
        })
    }
    pub fn native_focus_plan(
        &self,
        capture: &Value,
        tree: &Value,
        switch_space: bool,
    ) -> Result<lumen_window_policy::NativePlan> {
        let obj = capture
            .as_object()
            .filter(|o| {
                o.len() == 6
                    && [
                        "conId",
                        "foreignId",
                        "appId",
                        "instanceId",
                        "epoch",
                        "groupInstances",
                    ]
                    .iter()
                    .all(|key| o.contains_key(*key))
            })
            .ok_or("Invalid native window capture")?;
        if obj["epoch"].as_str() != self.epoch.as_deref() {
            return Err("The compositor session changed".into());
        }
        let groups = obj["groupInstances"]
            .as_array()
            .filter(|groups| !groups.is_empty() && groups.len() <= crate::MAX_PROCESSES)
            .ok_or("Invalid captured process group")?;
        let mut seen = HashSet::new();
        if !groups
            .iter()
            .all(|id| hash(id) && seen.insert(id.as_str().unwrap()))
            || !groups.contains(&capture["instanceId"])
        {
            return Err("Invalid captured process group".into());
        }
        let nodes = crate::window_nodes(tree)?;
        if !self.native_coverage_complete(&nodes) {
            return Err("External window coverage changed; reopen the switcher".into());
        }
        let records = nodes
            .iter()
            .filter_map(|node| self.native_record(node))
            .collect::<Vec<_>>();
        let mut target = capture.clone();
        target.as_object_mut().unwrap().remove("epoch");
        target.as_object_mut().unwrap().remove("groupInstances");
        History::from_saved(&json!([target.clone()]), &self.records())?;
        if !records.contains(&target) {
            return Err("The captured native window or its process is no longer available".into());
        }
        let unhide = records
            .iter()
            .filter(|row| row["appId"] == target["appId"] && groups.contains(&row["instanceId"]))
            .filter_map(|row| row["conId"].as_i64())
            .collect::<Vec<_>>();
        let plan = lumen_window_policy::native_focus_plan(
            tree,
            target["conId"].as_i64().unwrap(),
            &unhide,
            switch_space,
        )?;
        // Modal redirection remains native protocol policy. Its effective target
        // must also have an independently validated live process and surface.
        self.native_record(&plan.target)
            .ok_or("The modal target has no supported native process witness")?;
        Ok(plan)
    }
}
