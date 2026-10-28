use crate::{Result, fail};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub struct Inventory {
    version: Value,
    actions: BTreeMap<String, Vec<String>>,
    results: BTreeMap<String, Vec<(String, Value)>>,
}
impl Inventory {
    pub fn new(version: Value) -> Result<Self> {
        let actions: BTreeMap<String, Vec<String>> =
            serde_json::from_str(include_str!("../actions.json"))?;
        let results = actions
            .keys()
            .map(|key| (key.clone(), Vec::new()))
            .collect();
        Ok(Self {
            version,
            actions,
            results,
        })
    }
    pub fn record(
        &mut self,
        capability: &str,
        action: &str,
        status: &str,
        evidence: Value,
        reason: &str,
    ) -> Result<()> {
        if !self
            .actions
            .get(capability)
            .is_some_and(|actions| actions.iter().any(|name| name == action))
        {
            return Err(fail("Unknown application action"));
        }
        if ![
            "verified_compatible",
            "dependency_blocked",
            "implemented_unverified",
        ]
        .contains(&status)
        {
            return Err(fail(
                "Observed action requires a supported conformance status",
            ));
        }
        let empty_evidence = match &evidence {
            Value::Null => true,
            Value::Bool(value) => !value,
            Value::Number(value) => value.as_f64() == Some(0.0),
            Value::String(value) => value.is_empty(),
            Value::Array(value) => value.is_empty(),
            Value::Object(value) => value.is_empty(),
        };
        if empty_evidence || status == "dependency_blocked" && reason.is_empty() {
            return Err(fail(
                "Observed status requires concrete evidence and blocker details",
            ));
        }
        let rows = self
            .results
            .get_mut(capability)
            .expect("validated capability");
        if rows.iter().any(|(name, _)| name == action) {
            return Err(fail("Action already has an immutable observation"));
        }
        rows.push((
            action.into(),
            json!({"status":status,"evidence":evidence,"reason":reason}),
        ));
        Ok(())
    }
    pub fn snapshot(&self) -> Vec<Value> {
        self.actions.iter().map(|(capability, actions)| {
            let results = &self.results[capability];
            let supported:Vec<_> = results.iter().filter(|(_,value)|value["status"]=="verified_compatible").map(|(key,_)|key.clone()).collect();
            let remaining:Vec<_> = actions.iter().filter(|name|!results.iter().any(|(recorded,_)|recorded==*name)).cloned().collect();
            let status = if results.len()==actions.len() && results.iter().all(|(_,value)|value["status"]=="verified_compatible") { "verified_compatible" } else { "unassessed" };
            let scenario_results:BTreeMap<_,_>=results.iter().cloned().collect();
            json!({"capability_id":capability,"provider_app_id":"brave-browser","provider_version":self.version,"supported_actions":supported,"unsupported_actions":[],"backend":"actual retained browser, private profile and synthetic loopback provider","permission_set":"Actual origin permissions; no diagnostic permission overrides","offline_policy":"Network namespace has no external route","scenario_results":scenario_results,"remaining_actions":remaining,"reference_evidence":[],"compatibility_differences":["Reference measurements unavailable; application compatibility only"],"status":status})
        }).collect()
    }
}
