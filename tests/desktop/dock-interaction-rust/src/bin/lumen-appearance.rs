//! Private persisted settings transport for Dock acceptance.
use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::var("LUMEN_DOCK_AUDIT_OUTPUT")?);
    let store = root.join("preferences.json");
    let mut data: Value = if store.exists() {
        serde_json::from_slice(&fs::read(&store)?)?
    } else {
        json!({"dockPins":["fixture-a","fixture-b","unknown-retained"],"dockFiles":[root.join("Original report.txt")],"dockFolders":[{"path":root.join("Original folder"),"view":"list"}]})
    };
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("--set") {
        if args.len() != 3 {
            return Err("Expected --set key value".into());
        }
        let value: Value = serde_json::from_str(&args[2])?;
        data[&args[1]] = value.clone();
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("writes.jsonl"))?;
        writeln!(log, "{}", json!({"key":args[1],"value":value}))?;
        fs::write(&store, data.to_string())?;
    }
    println!(
        "{}",
        json!({"preferences":data,"appearance":{},"system":{}})
    );
    Ok(())
}
