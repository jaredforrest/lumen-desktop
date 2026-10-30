//! Application order is preference data, anchored to exact observed instances.
use crate::{Result, valid_app_id};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const MAX_ENTRIES: usize = 512;
const MAX_WITNESSES: usize = 2048;
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    entries: Vec<Value>,
}
pub fn instance(identity: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(identity).unwrap_or_default())
    )
}
impl History {
    pub fn from_saved(value: &Value, records: &[Value]) -> Result<Self> {
        let rows = value
            .as_array()
            .filter(|r| r.len() <= MAX_ENTRIES)
            .ok_or("Invalid application order")?;
        let mut associations: HashMap<String, HashSet<&str>> = HashMap::new();
        for record in records {
            if !crate::valid_identity(&record["identity"]) {
                continue;
            }
            if let Some(apps) = record["apps"].as_array() {
                associations
                    .entry(instance(&record["identity"]))
                    .or_default()
                    .extend(
                        apps.iter()
                            .filter_map(Value::as_str)
                            .filter(|app| valid_app_id(app)),
                    );
            }
        }
        let mut apps = HashSet::new();
        let mut witnesses = 0;
        for row in rows {
            let object = row
                .as_object()
                .filter(|o| {
                    o.len() == 2 && o.contains_key("appId") && o.contains_key("instanceIds")
                })
                .ok_or("Invalid application order entry")?;
            let app = object["appId"]
                .as_str()
                .filter(|s| valid_app_id(s) && apps.insert(*s))
                .ok_or("Invalid application order identity")?;
            let ids = object["instanceIds"]
                .as_array()
                .filter(|ids| !ids.is_empty() && ids.len() <= crate::MAX_PROCESSES)
                .ok_or("Invalid application order witnesses")?;
            witnesses += ids.len();
            if witnesses > MAX_WITNESSES {
                return Err("Application order exceeds witness limit".into());
            }
            let mut seen = HashSet::new();
            for id in ids {
                let id = id
                    .as_str()
                    .filter(|id| {
                        id.len() == 64
                            && id
                                .bytes()
                                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                            && seen.insert(*id)
                    })
                    .ok_or("Invalid application order witness")?;
                if !associations.get(id).is_some_and(|apps| apps.contains(app)) {
                    return Err("Application order witness is not an observed association".into());
                }
            }
        }
        Ok(Self {
            entries: rows.clone(),
        })
    }
    pub fn value(&self) -> Value {
        Value::Array(self.entries.clone())
    }
    pub fn promote(&mut self, app: &str, rows: &[Value]) {
        let Some(row) = rows.iter().find(|row| row["appId"] == app) else {
            return;
        };
        let Some(ids) = row["instanceIds"].as_array().filter(|ids| !ids.is_empty()) else {
            return;
        };
        self.entries.retain(|row| row["appId"] != app);
        self.entries
            .insert(0, json!({"appId":app,"instanceIds":ids}));
        self.entries.truncate(MAX_ENTRIES);
        while self
            .entries
            .iter()
            .map(|r| r["instanceIds"].as_array().map_or(0, Vec::len))
            .sum::<usize>()
            > MAX_WITNESSES
        {
            self.entries.pop();
        }
    }
    pub fn prune(&mut self, rows: &[Value]) {
        self.entries.retain_mut(|entry| {
            let Some(live) = rows.iter().find(|row| row["appId"] == entry["appId"]) else {
                return false;
            };
            let Some(ids) = live["instanceIds"].as_array() else {
                return false;
            };
            if let Some(old) = entry["instanceIds"].as_array_mut() {
                old.retain(|id| ids.contains(id));
                return !old.is_empty();
            }
            false
        });
    }
    pub fn order(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|row| row["appId"].as_str().map(str::to_owned))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(pid: u32, app: &str) -> Value {
        json!({"identity":{"pid":pid,"start":10,"device":2,"inode":3},"apps":[app]})
    }
    fn row(record: &Value) -> Value {
        json!({"appId":record["apps"][0],"instanceIds":[instance(&record["identity"])]})
    }
    #[test]
    fn order_survives_serialization_only_for_exact_surviving_instances() {
        let a = record(2, "a");
        let b = record(3, "b");
        let c = record(4, "c");
        let records = vec![a.clone(), b.clone(), c.clone()];
        let rows = records.iter().map(row).collect::<Vec<_>>();
        let mut history = History::default();
        for app in ["a", "b", "a", "c"] {
            history.promote(app, &rows);
        }
        assert_eq!(history.order(), ["c", "a", "b"]);
        let mut restored = History::from_saved(&history.value(), &records).unwrap();
        restored.prune(&rows);
        assert_eq!(restored, history);
        let mut replacement = b;
        replacement["identity"]["start"] = json!(11);
        restored.prune(&[row(&a), row(&replacement), row(&c)]);
        assert_eq!(restored.order(), ["c", "a"]);
        restored.promote("b", &[row(&replacement)]);
        assert_eq!(restored.order(), ["b", "c", "a"]);
    }
    #[test]
    fn corrupt_duplicate_and_unassociated_witnesses_refuse() {
        let records = vec![record(2, "a")];
        let mut h = History::default();
        h.promote("a", &[row(&records[0])]);
        let valid = h.value();
        let mut changed = valid.clone();
        changed[0]["appId"] = json!("b");
        assert!(History::from_saved(&changed, &records).is_err());
        let mut changed = valid.clone();
        changed[0]["instanceIds"] = json!(["bad"]);
        assert!(History::from_saved(&changed, &records).is_err());
        let mut changed = valid.clone();
        let duplicate = changed[0].clone();
        changed.as_array_mut().unwrap().push(duplicate);
        assert!(History::from_saved(&changed, &records).is_err());
        let mut changed = valid;
        changed[0]["extra"] = json!(true);
        assert!(History::from_saved(&changed, &records).is_err());
    }
}
