//! Read-only classification audit. Report names and counts without profile values.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn leaves(value: &Value, prefix: &str, output: &mut Vec<String>) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or("Retained state must be a JSON object")?;
    for (key, item) in object {
        let path = format!("{prefix}{key}");
        if item.is_object() {
            leaves(item, &format!("{path}."), output)?;
        } else {
            output.push(path);
        }
    }
    Ok(())
}

fn names(value: &Value) -> Result<BTreeSet<String>, String> {
    Ok(value
        .as_array()
        .ok_or("Classification fields must be an array")?
        .iter()
        .map(|item| {
            item.as_str()
                .ok_or("Classification field must be a string")
                .map(str::to_owned)
        })
        .collect::<Result<_, _>>()?)
}

fn ini_general(path: &Path) -> Result<BTreeSet<String>, String> {
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut section = String::new();
    let mut fields = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_owned();
        } else if section == "General" {
            if let Some((key, _)) = line.split_once(['=', ':']) {
                fields.insert(key.trim().to_owned());
            }
        }
    }
    Ok(fields)
}

fn audit(manifest: &Value, home: &Path, adapter: &Path) -> Result<(Value, bool), String> {
    let categories = manifest["categories"]
        .as_array()
        .ok_or("Missing categories")?;
    if names(&Value::Array(categories.clone()))?
        != BTreeSet::from(["config".into(), "state".into(), "cache".into()])
    {
        return Err("Expected config, state and cache categories".into());
    }
    let rules = manifest["rules"].as_array().ok_or("Missing rules")?;
    for rule in rules {
        if !["config", "state", "cache"].contains(&rule["category"].as_str().unwrap_or("")) {
            return Err("Rule has unknown category".into());
        }
    }
    let mut craft = BTreeMap::new();
    let mut unclassified = false;
    let apps = manifest["craft"]
        .as_object()
        .ok_or("Missing craft classifications")?;
    for (app, files) in apps {
        let mut report = BTreeMap::new();
        for (name, fields) in files.as_object().ok_or("Craft files must be an object")? {
            let config = names(&fields["config"])?;
            let state = names(&fields["state"])?;
            if !config.is_disjoint(&state) {
                return Err("Config and state fields overlap".into());
            }
            let path = home
                .join(".local/state/lumen-desktop/apps")
                .join(app)
                .join(name);
            let value = if path.exists() {
                serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
                    .map_err(|error: serde_json::Error| error.to_string())?
            } else {
                json!({})
            };
            let mut paths = Vec::new();
            leaves(&value, "", &mut paths)?;
            let unexpected: Vec<String> = paths
                .into_iter()
                .filter(|key| {
                    !state
                        .iter()
                        .any(|field| key == field || key.starts_with(&format!("{field}.")))
                })
                .collect();
            unclassified |= !unexpected.is_empty();
            report.insert(
                name.clone(),
                json!({"retainedStatePresent":path.exists(),"unclassifiedStateFields":unexpected}),
            );
        }
        craft.insert(app.clone(), report);
    }
    let prism = &manifest["prism"];
    let path = PathBuf::from(prism["path"].as_str().ok_or("Missing Prism path")?);
    let fields = ini_general(&path)?;
    let mut groups = BTreeMap::new();
    for category in ["config", "state", "cache"] {
        groups.insert(category, names(&prism[category])?);
    }
    for left in ["config", "state", "cache"] {
        for right in ["config", "state", "cache"] {
            if left != right && !groups[left].is_disjoint(&groups[right]) {
                return Err("Prism field groups overlap".into());
            }
        }
    }
    let known: BTreeSet<String> = groups
        .values()
        .flat_map(|group| group.iter().cloned())
        .collect();
    let unknown: Vec<String> = fields.difference(&known).cloned().collect();
    unclassified |= !unknown.is_empty();
    let counts: BTreeMap<&str, usize> = groups
        .iter()
        .map(|(key, group)| (*key, fields.intersection(group).count()))
        .collect();
    let report = json!({
        "categories": categories, "rules": rules.len(), "craft": craft,
        "prism": {"present":path.exists(),"counts":counts,"unclassifiedFieldNames":unknown,"adapterInstalled":adapter.exists()}
    });
    Ok((report, unclassified))
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let manifest = match args.as_slice() {
        [] => "/etc/nix/configuration-boundaries.json",
        [flag, path] if flag == "--manifest" => path,
        _ => {
            eprintln!("Expected optional --manifest PATH");
            std::process::exit(2);
        }
    };
    let result = (|| {
        let source: Value =
            serde_json::from_slice(&fs::read(manifest).map_err(|error| error.to_string())?)
                .map_err(|error: serde_json::Error| error.to_string())?;
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is missing")?;
        audit(&source, &home, Path::new("/etc/nix/desktop-settings.json"))
    })();
    match result {
        Ok((report, unclassified)) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("JSON report")
            );
            if unclassified {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("Profile audit failed: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::audit;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn reports_unclassified_state_names_without_values() {
        let root = std::env::temp_dir().join(format!("profile-audit-{}", std::process::id()));
        let state = root.join(".local/state/lumen-desktop/apps/craft");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join("prefs.json"),
            r#"{"recent":["private.docx"],"unexpected":"secret"}"#,
        )
        .unwrap();
        let manifest = json!({"categories":["config","state","cache"],"rules":[],
            "craft":{"craft":{"prefs.json":{"config":["theme"],"state":["recent"]}}},
            "prism":{"path":root.join("missing.cfg"),"config":[],"state":[],"cache":[]}});
        let (report, unclassified) =
            audit(&manifest, &root, Path::new("/missing-adapter")).unwrap();
        assert!(unclassified);
        let output = report.to_string();
        assert!(output.contains("unexpected"));
        assert!(!output.contains("secret"));
        assert!(!output.contains("private.docx"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
