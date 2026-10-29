//! Private Rust settings transport for physical Dock motion acceptance.
use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = PathBuf::from(env::var("LUMEN_DOCK_MOTION_STORE")?);
    let writes = PathBuf::from(env::var("LUMEN_DOCK_MOTION_WRITES")?);
    let initial = json!({"dockPins":["fixture-a","fixture-b","fixture-c","unknown-retained"],
        "dockFolders":[{"path":"/synthetic/folder","view":"list"}],"dockFiles":[]});
    let mut data: Value = if store.exists() { serde_json::from_slice(&fs::read(&store)?)? } else { initial };
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [flag] if flag == "--settings" => (),
        [flag, key, value] if flag == "--set" => {
            let parsed: Value = serde_json::from_str(value)?;
            data[key] = parsed.clone();
            writeln!(OpenOptions::new().create(true).append(true).open(&writes)?, "{}", json!({"key":key,"value":parsed}))?;
            fs::write(&store, serde_json::to_vec(&data)?)?;
        }
        _ => return Err("Unexpected private settings request".into()),
    }
    println!("{}", json!({"preferences":data,"appearance":{},"system":{}}));
    Ok(())
}
