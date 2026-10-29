//! Synthetic read-only Overview provider for painted typography fixtures.
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    let native = std::env::var("LUMEN_FIXTURE_OVERVIEW_NATIVE").as_deref() == Ok("true");
    if request["action"] == "focus" && native {
        if request["id"] != 102 || request["pid"] != 12 { return Err("Unexpected private focus target".into()); }
        let path = std::env::var("LUMEN_FIXTURE_OVERVIEW_ACTIONS")?;
        writeln!(std::fs::OpenOptions::new().append(true).create(true).open(path)?, "{request}")?;
    } else if request["action"] != "status" {
        return Err("Only private Overview status is allowed".into());
    }
    let mut windows = vec![json!({"id":101,"pid":11,"title":"A private document title with several words",
        "app":"Private application","group":"private","application":"private",
        "workspace":"1","workspaceLabel":"Private desktop"})];
    if native { windows.push(json!({"id":102,"pid":12,"title":"Another private document",
        "app":"Private application","group":"private","application":"private",
        "workspace":"1","workspaceLabel":"Private desktop"})); }
    println!("{}", json!({
        "windows":windows,
        "workspaces":[{"logicalId":"desktop1","name":"1","label":"Private desktop",
            "outputId":"display1","available":true,"focused":true}],
        "outputs":[{"logicalId":"display1","name":"HEADLESS-1","label":"Private display"}],
        "revision":1,"sessionEpoch":"private","autoReorder":true,"assignments":{},
        "windowGroups":{"enabled":true,"locked":false,"separate_displays":true,"revision":1,
            "groups":[{"id":1,"members":[{"id":101,"pid":11}],"selected":true,
            "suspended":false,"workspace":"1"}]}
    }));
    Ok(())
}
