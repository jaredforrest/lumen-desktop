//! Read-only private Overview corpus for live appearance acceptance.
use serde_json::{Value, json};
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    if request["action"] != "status" {
        return Err("Only private Overview status is allowed".into());
    }
    println!("{}", json!({
        "windows":[{"id":101,"pid":11,"title":"First document","app":"First application",
            "group":"fixture","application":"fixture","workspace":"1"}],
        "workspaces":[],"outputs":[],"revision":1,"sessionEpoch":"private",
        "autoReorder":true,"assignments":{}
    }));
    Ok(())
}
