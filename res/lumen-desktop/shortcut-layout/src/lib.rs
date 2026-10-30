pub mod xkb;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use xkb::{Activation, Model};

fn text<'a>(value: &'a Value, key: &str, max: usize) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|s| s.len() <= max && !s.chars().any(char::is_control))
        .ok_or_else(|| format!("Invalid keyboard {key}"))
}
fn number(value: &Value, max: u64) -> Result<u32, String> {
    value
        .as_u64()
        .filter(|v| *v <= max)
        .map(|v| v as u32)
        .ok_or_else(|| "Invalid keyboard number".into())
}
fn flag(value: &Value, key: &str) -> Result<bool, String> {
    value[key]
        .as_bool()
        .ok_or_else(|| format!("Invalid keyboard {key}"))
}
fn charge(row: &Value, remaining: &mut usize) -> Result<(), String> {
    let size = serde_json::to_vec(row)
        .map_err(|_| "Keyboard output unavailable")?
        .len();
    *remaining = remaining
        .checked_sub(size)
        .ok_or("Keyboard output row budget exceeded")?;
    Ok(())
}
#[derive(Clone)]
struct Binding {
    kind: String,
    keys: Vec<u32>,
    mode: String,
    device: String,
    mods: u8,
    group: Option<u32>,
    release: bool,
    locked: bool,
    inhibited: bool,
    no_repeat: bool,
}
impl Binding {
    fn read(value: &Value) -> Result<Self, String> {
        let kind = text(value, "kind", 8)?;
        if !["code", "symbol"].contains(&kind) {
            return Err("Unknown keyboard binding kind".into());
        }
        let keys = value["keys"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 8)
            .ok_or("Invalid keyboard keys")?
            .iter()
            .map(|v| number(v, u32::MAX as u64))
            .collect::<Result<Vec<_>, _>>()?;
        if kind == "code" && keys.iter().any(|key| *key > 1023) {
            return Err("Keyboard code exceeds limits".into());
        }
        Ok(Self {
            kind: kind.into(),
            keys,
            mode: text(value, "mode", 128)?.into(),
            device: text(value, "device", 256)?.into(),
            mods: number(&value["modifiers"], 255)? as u8,
            group: if value["group"].is_null() {
                None
            } else {
                Some(number(&value["group"], 7)?)
            },
            release: flag(value, "release")?,
            locked: flag(value, "locked")?,
            inhibited: flag(value, "inhibited")?,
            no_repeat: flag(value, "noRepeat")?,
        })
    }
    fn activation(&self, model: &Model, layout: u32) -> BTreeSet<Activation> {
        if self.keys.len() != 1 || self.group.is_some_and(|v| v != layout) {
            BTreeSet::new()
        } else {
            model.activations(&self.kind, self.keys[0], self.mods)
        }
    }
    fn overlaps(&self, other: &Self) -> bool {
        self.mode == other.mode
            && self.release == other.release
            && (self.device == "*" || other.device == "*" || self.device == other.device)
    }
    fn identity(&self) -> Value {
        json!({"kind":self.kind,"keys":self.keys,"mode":self.mode,"device":self.device,"modifiers":self.mods,"group":self.group,"release":self.release,"locked":self.locked,"inhibited":self.inhibited,"noRepeat":self.no_repeat})
    }
}
#[derive(Default)]
pub struct Observer {
    cache: BTreeMap<String, Model>,
}
impl Observer {
    pub fn observe(
        &mut self,
        seats: &Value,
        registry: &Value,
        session: &str,
    ) -> Result<Value, String> {
        let seats = seats
            .as_array()
            .filter(|s| !s.is_empty() && s.len() <= 16)
            .ok_or("Keyboard seat count exceeds limits")?;
        let scope = &seats[0]["lumen_shortcut_layout"]["bindings"];
        if scope["version"] != 1 || scope["bounded"] != true {
            return Err("Native binding metadata unavailable or exceeds limits".into());
        }
        let mode = text(scope, "activeMode", 128)?;
        let records = scope["records"]
            .as_array()
            .filter(|a| a.len() <= 2048)
            .ok_or("Native binding limit exceeded")?
            .iter()
            .map(Binding::read)
            .collect::<Result<Vec<_>, _>>()?;
        if registry["version"] != 1 || registry["valid"] != true {
            return Err("Configured shortcut registry unavailable".into());
        }
        let rows = registry["bindings"]
            .as_array()
            .filter(|a| a.len() <= 4096)
            .ok_or("Configured shortcut limit exceeded")?;
        let mut output = Vec::new();
        let mut remaining_output = 1_500_000usize;
        let mut fingerprints = Vec::new();
        let mut diagnostics = BTreeSet::new();
        diagnostics
            .insert("Native chord metadata does not verify opaque command identity".to_string());
        if mode != "default" {
            diagnostics.insert(format!(
                "Mode {mode}: default-mode menu hints are suspended"
            ));
        }
        for seat in seats {
            let native = &seat["lumen_shortcut_layout"];
            if native["version"] != 1 || native["available"] != true {
                return Err("Current native keyboard layout unavailable".into());
            }
            let name = text(native, "name", 256)?;
            let seat_name = text(seat, "name", 128)?;
            let map = native["keymap"]
                .as_str()
                .filter(|s| s.len() <= 128 * 1024)
                .ok_or("Keyboard map exceeds limits")?;
            let layout = number(&native["layout"], 7)?;
            let indexes = native["modifierIndexes"]
                .as_array()
                .filter(|v| v.len() == 8)
                .ok_or("Keyboard indexes unavailable")?
                .iter()
                .map(|v| number(v, u32::MAX as u64))
                .collect::<Result<Vec<_>, _>>()?;
            let cache_key = format!(
                "{:x}",
                Sha256::digest(format!("{map}\0{layout}\0{indexes:?}"))
            );
            if !self.cache.contains_key(&cache_key) {
                if self.cache.len() >= 16 {
                    self.cache.clear()
                }
                self.cache
                    .insert(cache_key.clone(), Model::new(map, layout, &indexes)?);
            }
            let model = &self.cache[&cache_key];
            fingerprints.push(json!([seat_name, name, layout, cache_key]));
            let activations = records
                .iter()
                .map(|r| r.activation(model, layout))
                .collect::<Vec<_>>();
            let mut by_activation: BTreeMap<Activation, Vec<usize>> = BTreeMap::new();
            let mut memberships = 0;
            for (index, values) in activations.iter().enumerate() {
                for value in values {
                    memberships += 1;
                    if memberships > 524288 {
                        return Err("Keyboard activation limit exceeded".into());
                    }
                    by_activation.entry(*value).or_default().push(index);
                }
            }
            let mut conflicts: BTreeSet<(usize, usize)> = BTreeSet::new();
            let mut comparisons = 0;
            for group in by_activation.values() {
                for (at, left) in group.iter().enumerate() {
                    for right in &group[at + 1..] {
                        comparisons += 1;
                        if comparisons > 1_048_576 {
                            return Err("Keyboard collision analysis limit exceeded".into());
                        }
                        if records[*left].overlaps(&records[*right]) {
                            conflicts.insert((*left, *right));
                        }
                    }
                }
            }
            if !conflicts.is_empty() {
                diagnostics.insert(format!(
                    "Physical shortcut collision detected on {seat_name}: {} native binding pairs",
                    conflicts.len()
                ));
            }
            if records.iter().any(|r| r.keys.len() != 1) {
                diagnostics
                    .insert("Multi-key native bindings require separate sequence analysis".into());
            }
            if records.iter().any(|r| r.device != "*") {
                diagnostics.insert("Device-specific bindings are observed; current seat keymap cannot certify every device keymap".into());
            }
            let mut native_rows = Vec::new();
            for (index, r) in records.iter().enumerate() {
                let others = conflicts
                    .iter()
                    .filter_map(|(a, b)| {
                        if *a == index {
                            Some(*b)
                        } else if *b == index {
                            Some(*a)
                        } else {
                            None
                        }
                    })
                    .take(16)
                    .collect::<Vec<_>>();
                let row = json!({"index":index,"activeMode":r.mode==mode,"identity":r.identity(),"label":"Custom shortcut / command identity unverified","keycaps":model.labels(&activations[index]),"conflicts":others,"reachable":!activations[index].is_empty()});
                charge(&row, &mut remaining_output)?;
                native_rows.push(row);
            }
            let mut declared = Vec::new();
            for row in rows.iter().filter(|r| r["enabled"] == true) {
                let id = text(row, "id", 512)?;
                let action = text(row, "action", 512)?;
                let physical = &row["physical"];
                let symbol = xkb::symbol(text(physical, "key", 512)?);
                let mut mods = 0u8;
                for modifier in physical["modifiers"]
                    .as_array()
                    .filter(|a| a.len() <= 8)
                    .ok_or("Invalid declared modifiers")?
                {
                    let bit = match modifier.as_str() {
                        Some("Shift") => 0,
                        Some("Lock") => 1,
                        Some("Control") => 2,
                        Some("Mod1") => 3,
                        Some("Mod2") => 4,
                        Some("Mod3") => 5,
                        Some("Mod4") => 6,
                        Some("Mod5") => 7,
                        _ => return Err("Unknown declared modifier".into()),
                    };
                    mods |= 1 << bit;
                }
                let row_mode = text(row, "mode", 128)?;
                let device = text(physical, "device", 256)?;
                let release = text(physical, "trigger", 16)? == "release";
                let matching = records
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        r.kind == "symbol"
                            && r.keys == [symbol]
                            && r.mods == mods
                            && r.mode == row_mode
                            && r.device == device
                            && r.release == release
                    })
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let values = model.activations("symbol", symbol, mods);
                let collision = matching
                    .iter()
                    .any(|i| conflicts.iter().any(|(a, b)| a == i || b == i));
                let row = json!({"id":id,"action":action,"seat":seat_name,"keycaps":model.labels(&values),"present":!matching.is_empty(),"collision":collision,"hintAvailable":mode=="default" && row_mode==mode && device=="*" && matching.len()==1 && !collision && !values.is_empty()});
                charge(&row, &mut remaining_output)?;
                declared.push(row);
            }
            output.push(json!({"seat":seat_name,"name":name,"layout":layout,"declared":declared,"native":native_rows}));
        }
        let generation = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&json!([
                    "lumen-shortcut-layout-v1",
                    session,
                    scope,
                    fingerprints,
                    registry
                ]))
                .map_err(|_| "Keyboard generation unavailable")?
            )
        );
        Ok(
            json!({"version":1,"available":true,"generation":generation,"activeMode":mode,"seats":output,"diagnostics":diagnostics,"commandIdentityVerified":false}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn binding(symbol: &str, mods: u8) -> Value {
        json!({"kind":"symbol","keys":[xkb::symbol(symbol)],"mode":"default","device":"*","modifiers":mods,"group":null,"release":false,"locked":false,"inhibited":false,"noRepeat":false})
    }
    fn seat(layout: &str, records: Vec<Value>, mode: &str) -> Value {
        json!([{"name":"seat0","lumen_shortcut_layout":{"version":1,"available":true,"layout":0,"name":layout,"keymap":xkb::tests::keymap(layout),"modifierIndexes":[0,1,2,3,4,5,6,7],"bindings":{"version":1,"bounded":true,"activeMode":mode,"records":records}}}])
    }
    fn registry() -> Value {
        json!({"version":1,"valid":true,"bindings":[{"id":"test.plus","action":"test.plus","enabled":true,"mode":"default","physical":{"key":"plus","modifiers":["Control"],"device":"*","trigger":"press"}}]})
    }
    #[test]
    fn output_budget_rejects_before_retaining_additional_rows() {
        let row = json!({"keys":["one","two"],"name":"bounded"});
        let mut remaining = serde_json::to_vec(&row).unwrap().len();
        charge(&row, &mut remaining).unwrap();
        assert_eq!(remaining, 0);
        assert!(charge(&row, &mut remaining).is_err());
    }
    #[test]
    fn actual_native_symbol_code_custom_and_mode_inventory_recalculates_shared_labels() {
        let mut observer = Observer::default();
        let registry = registry();
        let bindings = vec![binding("plus", 4), binding("equal", 5)];
        let us = observer
            .observe(
                &seat("us", bindings.clone(), "default"),
                &registry,
                "session-a",
            )
            .unwrap();
        assert_eq!(us["seats"][0]["declared"][0]["collision"], true);
        assert_eq!(us["seats"][0]["declared"][0]["hintAvailable"], false);
        let de = observer
            .observe(
                &seat("de", bindings.clone(), "default"),
                &registry,
                "session-a",
            )
            .unwrap();
        assert_eq!(de["seats"][0]["declared"][0]["collision"], false);
        assert_eq!(de["seats"][0]["declared"][0]["hintAvailable"], true);
        assert_ne!(us["generation"], de["generation"]);
        assert_ne!(
            us["seats"][0]["declared"][0]["keycaps"],
            de["seats"][0]["declared"][0]["keycaps"]
        );
        let repeat = observer
            .observe(
                &seat("de", bindings.clone(), "default"),
                &registry,
                "session-a",
            )
            .unwrap();
        assert_eq!(de, repeat);
        let changed_mode = observer
            .observe(&seat("de", bindings, "resize"), &registry, "session-a")
            .unwrap();
        assert_ne!(de["generation"], changed_mode["generation"]);
        assert_eq!(
            changed_mode["seats"][0]["declared"][0]["hintAvailable"],
            false
        );
        assert_eq!(changed_mode["commandIdentityVerified"], false);
        let mut code = binding("equal", 5);
        code["kind"] = json!("code");
        code["keys"] = json!([21]);
        let code = observer
            .observe(
                &seat("us", vec![binding("plus", 4), code], "default"),
                &registry,
                "session-a",
            )
            .unwrap();
        assert_eq!(code["seats"][0]["native"][0]["conflicts"], json!([1]));
        assert_eq!(
            code["seats"][0]["native"][1]["label"],
            "Custom shortcut / command identity unverified"
        );
        let new_session = observer
            .observe(
                &seat("de", vec![binding("plus", 4)], "default"),
                &registry,
                "session-b",
            )
            .unwrap();
        assert_ne!(de["generation"], new_session["generation"]);
    }
    #[test]
    fn candidate_preflight_checks_real_layout_and_never_excludes_unverified_old_commands() {
        let registry = registry();
        let actual = vec![binding("equal", 5)];
        let us = seat("us", actual.clone(), "default");
        assert!(
            preflight(&us, &registry, "test.plus", "session")
                .unwrap_err()
                .contains("physically overlaps")
        );
        let de = seat("de", actual, "default");
        assert_eq!(
            preflight(&de, &registry, "test.plus", "session").unwrap()["valid"],
            true
        );
        let old = seat("de", vec![binding("plus", 4)], "default");
        assert!(
            preflight(&old, &registry, "test.plus", "session")
                .unwrap_err()
                .contains("not excluded")
        );
        let mode = seat("de", vec![], "resize");
        assert!(preflight(&mode, &registry, "test.plus", "session").is_err());
        assert!(preflight(&json!([]), &registry, "test.plus", "session").is_err());
        assert!(preflight(&json!([3]), &registry, "test.plus", "session").is_err());
    }
    #[test]
    fn absence_bounds_incomplete_sequences_and_device_scope_are_truthful() {
        let mut observer = Observer::default();
        let registry = registry();
        let empty = observer
            .observe(&seat("us", vec![], "default"), &registry, "session")
            .unwrap();
        assert_eq!(empty["seats"][0]["declared"][0]["present"], false);
        assert_eq!(empty["seats"][0]["declared"][0]["hintAvailable"], false);
        let mut value = seat("us", vec![binding("plus", 4)], "default");
        value[0]["lumen_shortcut_layout"]["bindings"]["bounded"] = json!(false);
        assert!(observer.observe(&value, &registry, "session").is_err());
        value[0]["lumen_shortcut_layout"]["bindings"]["bounded"] = json!(true);
        value[0]["lumen_shortcut_layout"]["bindings"]["records"][0]["keys"] =
            json!([1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(observer.observe(&value, &registry, "session").is_err());
        let mut multi = binding("plus", 4);
        multi["keys"] = json!([1, 2]);
        multi["device"] = json!("specific keyboard");
        let value = observer
            .observe(&seat("us", vec![multi], "default"), &registry, "session")
            .unwrap();
        let diagnostics = value["diagnostics"].to_string();
        assert!(diagnostics.contains("Multi-key"));
        assert!(diagnostics.contains("Device-specific"));
        assert_eq!(value["seats"][0]["native"][0]["reachable"], false);
    }
}
/// Candidate preflight is conservative: no native entry is excluded as an
/// allegedly old owned command. Typed metadata alone cannot prove that identity.
pub fn preflight(
    seats: &Value,
    registry: &Value,
    id: &str,
    session: &str,
) -> Result<Value, String> {
    let row = registry["bindings"]
        .as_array()
        .ok_or("Candidate bindings unavailable")?
        .iter()
        .find(|r| r["id"] == id && r["enabled"] == true)
        .ok_or("Candidate binding unavailable")?;
    if row["mode"] != "default" {
        return Err("Candidate must use the default mode".into());
    }
    let physical = &row["physical"];
    let key = xkb::symbol(text(physical, "key", 512)?);
    if key == 0 {
        return Err("Candidate symbol unavailable".into());
    }
    let mut mods = 0u8;
    for modifier in physical["modifiers"]
        .as_array()
        .filter(|a| a.len() <= 8)
        .ok_or("Candidate modifiers unavailable")?
    {
        let bit = match modifier.as_str() {
            Some("Shift") => 0,
            Some("Lock") => 1,
            Some("Control") => 2,
            Some("Mod1") => 3,
            Some("Mod2") => 4,
            Some("Mod3") => 5,
            Some("Mod4") => 6,
            Some("Mod5") => 7,
            _ => return Err("Candidate modifier unavailable".into()),
        };
        mods |= 1 << bit;
    }
    if physical["device"] != "*" || physical["trigger"] != "press" {
        return Err("Candidate must be a wildcard-device press binding".into());
    }
    if seats
        .as_array()
        .is_none_or(|a| a.is_empty() || a.len() > 16)
    {
        return Err("Native candidate seats unavailable".into());
    }
    let scope = &seats[0]["lumen_shortcut_layout"]["bindings"];
    if scope["bounded"] != true || scope["activeMode"] != "default" {
        return Err("Native candidate analysis unavailable outside the default mode".into());
    }
    let mut augmented = seats.clone();
    let records = augmented[0]["lumen_shortcut_layout"]["bindings"]["records"]
        .as_array_mut()
        .filter(|a| a.len() < 2048)
        .ok_or("Native candidate binding limit exceeded")?;
    records.push(json!({"kind":"symbol","keys":[key],"mode":"default","device":"*","modifiers":mods,"group":null,"release":false,"locked":false,"inhibited":false,"noRepeat":false}));
    let result = Observer::default().observe(&augmented, registry, session)?;
    for seat in result["seats"]
        .as_array()
        .ok_or("Candidate seats unavailable")?
    {
        let native = seat["native"]
            .as_array()
            .and_then(|r| r.last())
            .ok_or("Candidate native analysis unavailable")?;
        if native["reachable"] != true {
            return Err("Candidate symbol is unreachable on a current seat keyboard layout".into());
        }
        if native["conflicts"].as_array().is_none_or(|a| !a.is_empty()) {
            return Err("Candidate physically overlaps an actual native binding. Command identity is unverified, so existing bindings were not excluded; previous map retained".into());
        }
        if seat["native"].as_array().is_some_and(|r| {
            r.iter().any(|r| {
                r["identity"]["keys"]
                    .as_array()
                    .is_some_and(|k| k.len() != 1)
                    || r["identity"]["device"] != "*"
            })
        }) {
            return Err("Candidate physical analysis is incomplete for multi-key or device-specific native bindings; previous map retained".into());
        }
    }
    Ok(
        json!({"version":1,"valid":true,"generation":result["generation"],"boundary":"Current seat layouts and actual native chords only; opaque command identity is unverified"}),
    )
}
