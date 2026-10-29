use super::*;

fn root() -> Result<PathBuf> { Ok(PathBuf::from(env::var("LUMEN_TEST_SAVER_QUEUE_ROOT")?)) }
pub fn release() -> Result<()> {
    fs::write(root()?.join("release"), "")?;
    Ok(())
}
pub fn provider() -> Result<()> {
    let root = root()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line)?;
    let mut log = OpenOptions::new().create(true).append(true).open(root.join("requests.jsonl"))?;
    writeln!(log, "{request}")?;
    if request["action"] == "status" {
        fs::write(root.join("blocked"), "")?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("release").exists() {
            need(Instant::now() < deadline, "Screen saver status barrier timed out")?;
            pause(10);
        }
    }
    let file = root.join("settings.json");
    let mut settings = if file.exists() { load(&file)? } else {
        json!({"enabled":true,"idleSeconds":300,"motion":true})
    };
    if request["action"] == "set" {
        let key = request["key"].as_str().ok_or("Missing setting key")?;
        settings[key] = request["value"].clone();
        save(&file, &settings)?;
    }
    println!("{}", json!({"preferences":settings,"lock":{"known":true,"locked":false}}));
    Ok(())
}
