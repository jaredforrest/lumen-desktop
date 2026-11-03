//! Read-only private Overview corpus for live appearance acceptance.
use serde_json::{Value, json};
use std::io::{self, BufRead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    if request["action"] != "status" {
        return Err("Only private Overview status is allowed".into());
    }
    println!("{}", json!({
        "windows":[{"id":101,"pid":11,"title":"First document","app":"First application",
            "group":"fixture","application":"fixture","workspace":"1","workspaceLabel":"Desktop 1"}],
        "workspaces":[{"logicalId":"desktop-one","name":"1","label":"Desktop 1","outputId":"display-one","output":"HEADLESS-1","available":true,"focused":true}],
        "outputs":[{"logicalId":"display-one","name":"HEADLESS-1","label":"Display 1"}],"revision":1,"sessionEpoch":"private",
        "autoReorder":true,"assignments":{}
    }));
    Ok(())
}
