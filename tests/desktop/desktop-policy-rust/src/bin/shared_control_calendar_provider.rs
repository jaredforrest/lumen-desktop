//! Read-only empty Calendar source for shared control presentation fixtures.
use serde_json::{Value, json};
use std::io::{self, BufRead};
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    if let Ok(path) = std::env::var("LUMEN_FIXTURE_SHARED_ACTIONS") {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{}", json!({"provider":"calendar","request":request}))?;
    }
    if request["action"] != "list" && request["action"] != "load" {
        println!("{}", json!({"error":"Private provider rejects mutations"}));
        return Ok(());
    }
    println!("{}", json!({
        "events":[],"days":[],"total":0,"configured":true,
        "providers":[{"id":"google","name":"Private provider","generation":"private-generation",
            "configured":true,"calendars":[{"id":"private","name":"Private writable calendar","accessRole":"owner"}]}]
    }));
    Ok(())
}
