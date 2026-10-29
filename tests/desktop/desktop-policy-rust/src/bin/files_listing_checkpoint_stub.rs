use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        if request["action"] != "list" { continue; }
        let path = request["path"].as_str().ok_or("missing path")?;
        let mut response = json!({"id":request["id"],"action":"list","path":path,
            "parent":"/","rows":[],"breadcrumbs":[],"writable":true});
        match path {
            "/error" => response["error"] = json!("private expected error"),
            "/partial" => response["limited"] = json!(true),
            "/unreadable" => response["unreadable"] = json!(3),
            "/stale" => response["id"] = json!(request["id"].as_i64().ok_or("missing id")? - 1),
            _ => {}
        }
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}
