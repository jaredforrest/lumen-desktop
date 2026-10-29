//! Client-side keyboard and pointer focus ordering by Wayland resource.
use regex::Regex;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn inspect(text: &str) -> Value {
    let event =
        Regex::new(r"\b(wl_keyboard|wl_pointer)#(\d+)\.(enter|leave)\(\d+, wl_surface#(\d+)")
            .unwrap();
    let mut focus = BTreeMap::<(String, String), String>::new();
    let mut counts = json!({"keyboard":{"enter":0,"leave":0},"pointer":{"enter":0,"leave":0}});
    let mut errors = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some(hit) = event.captures(line) else {
            continue;
        };
        let (kind, resource, action, surface) = (&hit[1], &hit[2], &hit[3], &hit[4]);
        let group = kind.trim_start_matches("wl_");
        counts[group][action] = json!(counts[group][action].as_u64().unwrap() + 1);
        let key = (kind.to_owned(), resource.to_owned());
        if action == "enter" {
            focus.insert(key, surface.to_owned());
        } else {
            let expected = focus.remove(&key);
            if expected.as_deref() != Some(surface) {
                errors.push(json!({"line":index+1,"kind":kind,"resource":resource,"surface":surface,"expected":expected}));
            }
        }
    }
    json!({"counts":counts,"errors":errors})
}

pub fn require_ordered(text: &str) -> crate::Result<Value> {
    let result = inspect(text);
    for kind in ["keyboard", "pointer"] {
        if result["counts"][kind]["enter"].as_u64() == Some(0)
            || result["counts"][kind]["leave"].as_u64() == Some(0)
        {
            return Err("Complete keyboard and pointer trace required".into());
        }
    }
    if result["errors"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err("Focus event ordering failed".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_resources_are_complete() {
        let trace = "wl_keyboard#24.enter(1, wl_surface#29)\nwl_keyboard#24.leave(2, wl_surface#29)\nwl_pointer#25.enter(3, wl_surface#29)\nwl_pointer#25.leave(4, wl_surface#29)\nwl_keyboard#30.enter(5, wl_surface#41)\nwl_keyboard#30.leave(6, wl_surface#41)";
        assert!(require_ordered(trace).is_ok());
    }
    #[test]
    fn duplicate_and_mismatched_leaves_fail() {
        let trace = "wl_keyboard#24.enter(1, wl_surface#29)\nwl_keyboard#30.enter(2, wl_surface#29)\nwl_keyboard#24.leave(3, wl_surface#29)\nwl_keyboard#24.leave(4, wl_surface#29)";
        assert_eq!(inspect(trace)["errors"][0]["expected"], Value::Null);
        let mismatch = "wl_pointer#1.enter(1, wl_surface#2)\nwl_pointer#1.leave(2, wl_surface#3)";
        assert_eq!(inspect(mismatch)["errors"][0]["expected"], "2");
    }
    #[test]
    fn recreated_resource_and_incomplete_trace() {
        let trace = "wl_keyboard#24.enter(1, wl_surface#29)\nwl_keyboard#24.leave(2, wl_surface#29)\nwl_keyboard#24.enter(3, wl_surface#41)\nwl_keyboard#24.leave(4, wl_surface#41)";
        assert!(inspect(trace)["errors"].as_array().unwrap().is_empty());
        assert!(require_ordered("").is_err());
        assert!(require_ordered(trace).is_err());
    }
}
