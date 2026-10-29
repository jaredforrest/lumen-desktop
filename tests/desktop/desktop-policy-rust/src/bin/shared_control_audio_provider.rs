//! Private wpctl command recorder for native shared control acceptance.
use serde_json::json;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = std::env::var("LUMEN_FIXTURE_SHARED_ACTIONS")?;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", json!({"provider":"audio","args":args}))?;
    Ok(())
}
