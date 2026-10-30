//! Presentation placement is separate from structural ownership and restoration.
use crate::Result;
use serde_json::Value;
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub id: u64,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Projection {
    pub enabled: bool,
    pub supported: bool,
    pub projected: bool,
    pub revision: u64,
    pub origin_workspace: Option<Place>,
    pub origin_output: Option<Place>,
    pub effective_workspace: Option<Place>,
    pub effective_output: Option<Place>,
}
fn decimal(value: &Value, positive: bool) -> Result<u64> {
    let s = value.as_str().ok_or("Invalid projection identity")?;
    if s.is_empty()
        || s.len() > 20
        || !s.bytes().all(|b| b.is_ascii_digit())
        || (s.len() > 1 && s.starts_with('0'))
    {
        return Err("Invalid projection identity".into());
    }
    let n = s
        .parse::<u64>()
        .map_err(|_| "Invalid projection identity")?;
    if positive && n == 0 {
        return Err("Invalid projection identity".into());
    }
    Ok(n)
}
fn place(value: &Value) -> Result<Option<Place>> {
    if value.is_null() {
        return Ok(None);
    }
    let o = value
        .as_object()
        .filter(|o| o.len() == 2)
        .ok_or("Invalid projection placement")?;
    let name = o
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control))
        .ok_or("Invalid projection placement")?;
    Ok(Some(Place {
        id: decimal(&value["id"], true)?,
        name: name.into(),
    }))
}
pub fn parse(node: &Value) -> Result<Option<Projection>> {
    let Some(v) = node.get("lumen_all_desktops") else {
        return Ok(None);
    };
    let o = v
        .as_object()
        .filter(|o| o.len() == 10)
        .ok_or("Invalid projection metadata")?;
    if ![
        "version",
        "enabled",
        "supported",
        "projected",
        "reason",
        "revision",
        "origin_workspace",
        "origin_output",
        "effective_workspace",
        "effective_output",
    ]
    .iter()
    .all(|k| o.contains_key(*k))
        || v["version"] != 1
        || !["enabled", "supported", "projected"]
            .iter()
            .all(|k| o.get(*k).is_some_and(Value::is_boolean))
        || !(v["reason"].is_null()
            || v["reason"]
                .as_str()
                .is_some_and(|s| s.len() <= 256 && !s.chars().any(char::is_control)))
    {
        return Err("Invalid projection metadata".into());
    }
    let p = Projection {
        enabled: v["enabled"].as_bool().unwrap(),
        supported: v["supported"].as_bool().unwrap(),
        projected: v["projected"].as_bool().unwrap(),
        revision: decimal(&v["revision"], false)?,
        origin_workspace: place(&v["origin_workspace"])?,
        origin_output: place(&v["origin_output"])?,
        effective_workspace: place(&v["effective_workspace"])?,
        effective_output: place(&v["effective_output"])?,
    };
    if p.enabled
        && p.supported
        && (p.origin_workspace.is_none()
            || p.origin_output.is_none()
            || p.effective_workspace.is_none()
            || p.effective_output.is_none()
            || p.origin_output != p.effective_output
            || p.projected != (p.origin_workspace != p.effective_workspace))
    {
        return Err("Unsupported projection scope".into());
    }
    if !p.enabled
        && (p.projected
            || p.origin_workspace != p.effective_workspace
            || p.origin_output != p.effective_output)
    {
        return Err("Invalid disabled projection placement".into());
    }
    Ok(Some(p))
}
pub fn workspace(node: &Value, structural: &str) -> Result<String> {
    Ok(match parse(node)? {
        Some(p) if p.enabled && !p.supported => return Err("Unsupported projection scope".into()),
        Some(p) if p.enabled => p.effective_workspace.unwrap().name,
        _ => structural.into(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn node() -> Value {
        json!({"lumen_all_desktops":{"version":1,"enabled":true,"supported":true,"projected":true,"reason":null,"revision":"0","origin_workspace":{"id":"1","name":"one"},"effective_workspace":{"id":"2","name":"two"},"origin_output":{"id":"3","name":"display"},"effective_output":{"id":"3","name":"display"}}})
    }
    #[test]
    fn scoped_and_structural() {
        let mut n = node();
        assert_eq!(workspace(&n, "one").unwrap(), "two");
        assert_eq!(workspace(&json!({}), "one").unwrap(), "one");
        n["lumen_all_desktops"]["enabled"] = json!(false);
        assert!(workspace(&n, "one").is_err());
        n["lumen_all_desktops"]["projected"] = json!(false);
        n["lumen_all_desktops"]["effective_workspace"] =
            n["lumen_all_desktops"]["origin_workspace"].clone();
        assert_eq!(workspace(&n, "one").unwrap(), "one");
    }
    #[test]
    fn malformed_fails_closed() {
        for (key, value) in [
            ("version", json!(2)),
            ("supported", json!(false)),
            ("revision", json!("01")),
            ("revision", json!("18446744073709551616")),
            ("effective_workspace", Value::Null),
        ] {
            let mut n = node();
            n["lumen_all_desktops"][key] = value;
            assert!(workspace(&n, "one").is_err(), "{key}");
        }
    }
    #[test]
    fn topology_binds_projection() {
        let a = node();
        let mut b = a.clone();
        b["lumen_all_desktops"]["revision"] = json!("1");
        assert_ne!(
            crate::guard::canonical_topology(&a).unwrap(),
            crate::guard::canonical_topology(&b).unwrap()
        );
    }
}
