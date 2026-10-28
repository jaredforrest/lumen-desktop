//! Provider-scoped office records from immutable, partial fixture evidence.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

const PROVIDERS: [(&str, &str, &str, &str); 3] = [
    ("writer", "ai.storyteller.wordcraft", "wordcraft", "docx"),
    ("sheet", "ai.storyteller.gridcraft", "gridcraft", "xlsx"),
    ("slides", "ai.storyteller.deckcraft", "deckcraft", "pptx"),
];

fn scope(capability: &str) -> &'static str {
    match capability {
        "APP-OFFICE-01" => "interactive styling, paragraphs, sections, tables, images, links, stories and review operations",
        "APP-OFFICE-02" => "interactive calculation, formatting, sorting, filtering, charts, sheets and print ranges",
        "APP-OFFICE-03" => "interactive layouts, notes, transitions and presenter view",
        "APP-OFFICE-04" => "template creation, recent-entry removal, killed-process recovery and newer-source protection",
        "APP-OFFICE-05" => "conversion-loss warning cancellation, save failure and print-dialog acceptance",
        "APP-OFFICE-06" => "collaboration, change review and version history assessed separately",
        "APP-OFFICE-07" => "interactive PDF navigation, search, selection, zoom, rotation, annotations, forms and signatures",
        "WF-OFFICE-01" => "complete native reopen, reversible edit, dirty-close Save/Discard/error and killed-process recovery",
        _ => unreachable!("known office capability"),
    }
}

fn load(path: &Path) -> Result<(Value, Value)> {
    let raw = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value = serde_json::from_slice(&raw).map_err(|error| format!("{}: {error}", path.display()))?;
    let reference = json!({
        "path": path.to_string_lossy(),
        "sha256": format!("{:x}", Sha256::digest(&raw)),
    });
    Ok((value, reference))
}

fn provider_version<'a>(path: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("-{name}-");
    path.match_indices(&marker).find_map(|(offset, _)| {
        let remainder = &path[offset + marker.len()..];
        let (version, _) = remainder.split_once("/bin/")?;
        (!version.is_empty() && !version.contains('/')).then_some(version)
    })
}

fn generate(cli_path: &Path, gui_paths: &[PathBuf]) -> Result<Value> {
    let (cli, cli_reference) = load(cli_path)?;
    if cli.get("harnessPassed") != Some(&Value::Bool(true)) {
        return Err("A successful scoped CLI report is required".into());
    }
    let mut gui = Vec::new();
    for path in gui_paths {
        let (value, reference) = load(path)?;
        if value.get("passed") != Some(&Value::Bool(true)) {
            return Err("GUI report did not pass".into());
        }
        let executable = value.pointer("/facts/native/executable").and_then(Value::as_str).unwrap_or("");
        let matches = PROVIDERS.iter().filter(|(_, _, name, _)| executable.contains(&format!("/.{name}-native"))).collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("GUI provider identity is not exact".into());
        }
        gui.push((matches[0].0, value, reference));
    }

    let mut records = Vec::new();
    let installed = cli.get("installedTools").and_then(Value::as_object);
    for (app, provider, name, extension) in PROVIDERS {
        let suffix = format!("/{name}-cli");
        let tools = installed.into_iter().flat_map(|tools| tools.iter())
            .filter(|(path, _)| path.ends_with(&suffix)).collect::<Vec<_>>();
        if tools.len() != 1 {
            return Err("Expected one exact provider tool".into());
        }
        let (tool, digest) = tools[0];
        let version = provider_version(tool, name).ok_or("Provider version was not captured")?;
        let own = match app {
            "writer" => "APP-OFFICE-01",
            "sheet" => "APP-OFFICE-02",
            _ => "APP-OFFICE-03",
        };
        for capability in [own, "APP-OFFICE-04", "APP-OFFICE-05", "APP-OFFICE-06", "WF-OFFICE-01"] {
            let mut observations = Vec::new();
            for observed in cli.get("capabilities").and_then(Value::as_array).into_iter().flatten() {
                if observed.get("id").and_then(Value::as_str) != Some(capability) {
                    continue;
                }
                let action = observed.get("action").and_then(Value::as_str).unwrap_or("");
                if capability == "APP-OFFICE-05" && !action.starts_with(&format!("{app} ")) {
                    continue;
                }
                if capability == "WF-OFFICE-01" && !action.starts_with(&format!("{extension} ")) {
                    continue;
                }
                observations.push(json!({"kind": "cli_action", "evidence": cli_reference, "result": observed}));
            }
            if capability == "WF-OFFICE-01" {
                for (gui_app, value, reference) in &gui {
                    if *gui_app == app {
                        for check in value.get("checks").and_then(Value::as_array).into_iter().flatten() {
                            observations.push(json!({"kind": "native_action", "evidence": reference, "result": {"action": check, "status": "fixture_verified"}}));
                        }
                    }
                }
            }
            let supported = observations.iter().filter(|item| item["result"]["status"] == "fixture_verified")
                .filter_map(|item| item["result"].get("action").cloned()).collect::<Vec<_>>();
            let unsupported = observations.iter().filter(|item| {
                matches!(item["result"]["status"].as_str(), Some("unsupported" | "provider_defect"))
            }).map(|item| json!({
                "action": item["result"]["action"],
                "outcome": item["result"]["status"],
                "reason": item["result"].get("reason").cloned().unwrap_or(Value::Null),
            })).collect::<Vec<_>>();
            let differences = observations.iter().filter(|item| {
                matches!(item["result"]["status"].as_str(), Some("degraded" | "provider_defect"))
            }).map(|item| item["result"].clone()).collect::<Vec<_>>();
            let status = if supported.is_empty() && unsupported.is_empty() && differences.is_empty() {
                "unassessed"
            } else {
                "implemented_unverified"
            };
            records.push(json!({
                "capability_id": capability,
                "provider_app_id": provider,
                "provider_version": version,
                "supported_actions": supported,
                "unsupported_actions": unsupported,
                "backend": {"kind": "installed_provider", "tool": tool, "sha256": digest, "scope": "Synthetic CLI and explicitly listed native scenarios only"},
                "permission_set": {"files": "Disposable namespace artifact files only", "network": "Loopback only; no external routes", "devices": "No host display, audio, camera or render devices"},
                "offline_policy": {"tested": "All retained scenarios ran without external networking", "unassessed": "Provider network features and accounts"},
                "scenario_results": observations,
                "reference_evidence": [],
                "compatibility_differences": differences,
                "status": status,
                "unverified_actions": [scope(capability)],
                "scope_complete": false,
            }));
        }
    }
    records.push(json!({
        "capability_id": "APP-OFFICE-07", "provider_app_id": null, "provider_version": null,
        "supported_actions": [], "unsupported_actions": [], "backend": null,
        "permission_set": {},
        "offline_policy": {"unassessed": "No interactive PDF provider has been selected or assessed"},
        "scenario_results": [], "reference_evidence": [], "compatibility_differences": [],
        "status": "unassessed", "unverified_actions": [scope("APP-OFFICE-07")], "scope_complete": false,
    }));
    Ok(json!({
        "schema": 1,
        "records": records,
        "complete": false,
        "notes": [
            "Historical CLI capability entries are scoped action observations, not full capability records.",
            "No interactive reference platform measurements are available.",
            "An untested action is not an unsupported or externally blocked action.",
        ],
    }))
}

fn parse_args() -> Result<(PathBuf, Vec<PathBuf>, PathBuf)> {
    let mut cli = None;
    let mut gui = Vec::new();
    let mut output = None;
    let mut args = env::args_os().skip(1);
    while let Some(argument) = args.next() {
        let value = args.next().ok_or_else(|| format!("Missing value after {}", argument.to_string_lossy()))?;
        match argument.to_str() {
            Some("--cli") if cli.is_none() => cli = Some(PathBuf::from(value)),
            Some("--gui") => gui.push(PathBuf::from(value)),
            Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
            _ => return Err(format!("Unexpected argument {}", argument.to_string_lossy())),
        }
    }
    Ok((cli.ok_or("--cli is required")?, gui, output.ok_or("--output is required")?))
}

fn run() -> Result<()> {
    let (cli, gui, output) = parse_args()?;
    let record = generate(&cli, &gui)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(&output)
        .map_err(|error| format!("{}: {error}", output.display()))?;
    serde_json::to_writer_pretty(&mut file, &record).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_observations_never_close_capability() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.json");
        let original = json!({
            "harnessPassed": true,
            "installedTools": {
                "/nix/store/fake-wordcraft-0.5.0/bin/wordcraft-cli": "000",
                "/nix/store/fake-gridcraft-0.5.0/bin/gridcraft-cli": "000",
                "/nix/store/fake-deckcraft-0.5.0/bin/deckcraft-cli": "000",
            },
            "capabilities": [
                {"id": "APP-OFFICE-01", "action": "create docx", "status": "fixture_verified"},
                {"id": "APP-OFFICE-02", "action": "ODS export", "status": "unsupported", "reason": "Data import only"},
                {"id": "APP-OFFICE-03", "action": "ODP export", "status": "provider_defect", "actualFormat": "deck-native"},
            ],
        });
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let before = fs::read(&path).unwrap();
        let result = generate(&path, &[]).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        let records = result["records"].as_array().unwrap();
        assert_eq!(records.len(), 16);
        for record in records {
            assert_eq!(record["scope_complete"], false);
            assert!(!["verified_compatible", "verified_reference", "dependency_blocked", "explicitly_excluded"]
                .contains(&record["status"].as_str().unwrap()));
        }
        let sheet = records.iter().find(|record| record["capability_id"] == "APP-OFFICE-02").unwrap();
        assert_eq!(sheet["unsupported_actions"][0]["reason"], "Data import only");
        let slides = records.iter().find(|record| record["capability_id"] == "APP-OFFICE-03").unwrap();
        assert_eq!(slides["compatibility_differences"][0]["actualFormat"], "deck-native");
    }

    #[test]
    fn unsuccessful_evidence_and_ambiguous_tools_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.json");
        fs::write(&path, br#"{"harnessPassed":false}"#).unwrap();
        assert!(generate(&path, &[]).is_err());
        fs::write(&path, br#"{"harnessPassed":true,"installedTools":{}}"#).unwrap();
        assert!(generate(&path, &[]).is_err());
    }
}
