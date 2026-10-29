use super::*;

fn frames(log: &str) -> Result<Vec<Value>> {
    let mut frames = Vec::new();
    for line in log.lines() {
        if let Some(payload) = line.split_once("EXPIRY_FRAME ") {
            frames.push(serde_json::from_str(payload.1)?);
        }
    }
    Ok(frames)
}

pub fn run(fixture: &mut Fixture) -> Result<()> {
    let palette = read(&fixture.assets.join("appearance/palette.json"))?;
    save(&fixture.private.join("appearance.json"),
        &json!({"font":palette["font"],"palette":palette["light"],"preferences":{}}))?;
    fixture.vars.insert("LUMEN_FOCUS_CAPTURE_DIR".into(), path(&fixture.options.output));
    let qml = path(&fixture.qml);
    let index = fixture.start("focus-presentation.log", "quickshell", &["-p", &qml], false)?;
    wait("painted focus expiry", || {
        let status = fixture.children.0[index].try_wait()?;
        Ok(status)
    })?;
    let log = fs::read_to_string(fixture.options.output.join("focus-presentation.log"))?;
    require(log.contains("EXPIRY_PASS") && !log.contains("EXPIRY_FAIL"),
        format!("Focus presentation failed: {log}"))?;
    require(!["ReferenceError", "TypeError", "Binding loop", "Failed to load"].iter().any(|term| log.contains(term)),
        format!("Focus presentation QML warning: {log}"))?;
    let rows = frames(&log)?;
    require(rows.len() == 2 && rows[0]["name"] == "active" && rows[1]["name"] == "expired"
        && rows[0]["label"] == "Do Not Disturb is on\nNo notifications"
        && rows[1]["label"] == "No notifications"
        && rows[0]["dnd"] == true && rows[1]["dnd"] == false
        && rows[1]["revision"].as_i64().unwrap_or(0) > rows[0]["revision"].as_i64().unwrap_or(0),
        format!("Focus expiry frame metadata failed: {rows:?}"))?;
    let active = fixture.options.output.join("active.png");
    let expired = fixture.options.output.join("expired.png");
    require(active.is_file() && expired.is_file() && fs::read(&active)? != fs::read(&expired)?,
        "Focus expiry did not repaint distinct frames")?;
    for frame in [&active, &expired] {
        let description = call("magick", &["identify", "-format", "%w %h %[pixel:p{10,10}]", &path(frame)], &fixture.vars)?;
        require(description.starts_with("360 110 ") && description.contains("242,242,244"),
            format!("Unexpected focus frame geometry or background: {description}"))?;
    }
    let comparison = Command::new("magick").args(["compare", "-metric", "AE", &path(&active), &path(&expired), "null:"])
        .env_clear().envs(&fixture.vars).output()?;
    let changed = String::from_utf8_lossy(&comparison.stderr).split_whitespace().next()
        .ok_or("No painted difference metric")?.parse::<f64>()?;
    require(changed > 64.0, format!("Focus centre did not repaint enough pixels: {changed}"))?;
    save(&fixture.options.output.join("result.json"), &json!({"passed":true,"frames":rows,
        "changedPixels":changed,"scope":"Private Rust notification daemon, live QML transport and painted timed focus expiry"}))?;
    Ok(())
}
