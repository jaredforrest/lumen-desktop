//! Explicit, disposable browser font selection and paint audit.
use base64::Engine;
use lumen_browser_workflows_fixture::{Result, fail, protocol::PipeTransport};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn args() -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let mut options = BTreeMap::new();
    let mut input = env::args_os().skip(1);
    while let Some(key) = input.next() {
        let value = input.next().ok_or_else(|| fail("Missing argument value"))?;
        let key = key.to_str().ok_or_else(|| fail("Non-UTF8 argument"))?;
        if !["--browser", "--profile", "--output", "--cases"].contains(&key)
            || options
                .insert(key.to_owned(), PathBuf::from(value))
                .is_some()
        {
            return Err(fail("Unexpected or repeated argument"));
        }
    }
    let get = |key: &str| {
        options
            .get(key)
            .cloned()
            .ok_or_else(|| fail(format!("Missing {key}")))
    };
    Ok((
        get("--browser")?,
        get("--profile")?,
        get("--output")?,
        get("--cases")?,
    ))
}

fn validate(cases: &Value) -> Result<&[Value]> {
    let rows = cases
        .as_array()
        .ok_or_else(|| fail("Font cases must be an array"))?;
    if !(1..=64).contains(&rows.len()) {
        return Err(fail("Bounded font cases required"));
    }
    let mut names = HashSet::new();
    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| fail("Font case must be an object"))?;
        if object.len() != 5
            || ![
                "name",
                "family",
                "text",
                "expectedFamilies",
                "expectedGlyphCount",
            ]
            .iter()
            .all(|key| object.contains_key(*key))
        {
            return Err(fail("Explicit font case fields required"));
        }
        for key in ["name", "family", "text"] {
            let text = row[key]
                .as_str()
                .ok_or_else(|| fail("Font case text required"))?;
            if text.is_empty() || text.len() > 1024 {
                return Err(fail("Bounded font case text required"));
            }
        }
        if !names.insert(row["name"].as_str().unwrap()) {
            return Err(fail("Unique font case names required"));
        }
        let families = row["expectedFamilies"]
            .as_array()
            .ok_or_else(|| fail("Expected families required"))?;
        if !(1..=8).contains(&families.len())
            || families
                .iter()
                .any(|value| value.as_str().is_none_or(|s| s.is_empty() || s.len() > 256))
        {
            return Err(fail("Explicit expected font families required"));
        }
        let glyphs = row["expectedGlyphCount"]
            .as_u64()
            .ok_or_else(|| fail("Expected glyph count required"))?;
        if !(1..=4096).contains(&glyphs) {
            return Err(fail("Bounded glyph count required"));
        }
    }
    Ok(rows)
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn page(rows: &[Value]) -> String {
    let mut page = String::from(
        "<html><head><meta charset=\"utf-8\"><title>Font policy fixture</title></head><body style=\"background:#202124;color:#fff\">",
    );
    for (index, row) in rows.iter().enumerate() {
        page.push_str(&format!(
            "<div id=\"case-{index}\" style=\"font-size:32px;font-family:{}\">{}</div>",
            escape_html(row["family"].as_str().unwrap()),
            escape_html(row["text"].as_str().unwrap())
        ));
    }
    page.push_str("</body></html>");
    page
}

fn request(
    transport: &mut PipeTransport,
    session: Option<&str>,
    method: &str,
    params: Value,
) -> Result<Value> {
    transport.request(method, params, session, Duration::from_secs(15))
}

fn run() -> Result<()> {
    let (browser, profile, output, case_file) = args()?;
    let browser = browser.canonicalize()?;
    if !browser.starts_with("/nix/store") || !browser.is_file() {
        return Err(fail("Immutable browser executable required"));
    }
    if !profile.starts_with("/home/fixture")
        || profile.components().any(|part| part.as_os_str() == "..")
    {
        return Err(fail("Explicit private browser profile required"));
    }
    if !output.starts_with("/artifacts")
        || !output.is_dir()
        || output.components().any(|part| part.as_os_str() == "..")
    {
        return Err(fail("Explicit artifact directory required"));
    }
    let data: Value = serde_json::from_slice(&fs::read(case_file)?)?;
    let rows = validate(&data)?;
    fs::create_dir(&profile)?;
    let html = page(rows);
    let command = vec![
        browser.display().to_string(),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--headless".into(),
        "--disable-gpu".into(),
        "--remote-debugging-pipe".into(),
        "about:blank".into(),
    ];
    let environment: BTreeMap<String, String> = env::vars().collect();
    let mut transport = PipeTransport::start(&command, &environment, &output.join("browser.log"))?;
    let result = audit(&mut transport, rows, &html, &browser, &output, &command);
    let cleanup = transport.close();
    result?;
    cleanup
}

fn audit(
    transport: &mut PipeTransport,
    rows: &[Value],
    html: &str,
    browser: &Path,
    output: &Path,
    command: &[String],
) -> Result<()> {
    let version = request(transport, None, "Browser.getVersion", json!({}))?;
    let targets = request(transport, None, "Target.getTargets", json!({}))?;
    let target = targets["targetInfos"]
        .as_array()
        .and_then(|targets| targets.iter().find(|target| target["type"] == "page"))
        .ok_or_else(|| fail("Browser page target absent"))?;
    let session = request(
        transport,
        None,
        "Target.attachToTarget",
        json!({"targetId":target["targetId"],"flatten":true}),
    )?["sessionId"]
        .as_str()
        .ok_or_else(|| fail("Browser session absent"))?
        .to_owned();
    for method in ["Page.enable", "DOM.enable", "CSS.enable"] {
        request(transport, Some(&session), method, json!({}))?;
    }
    let url = format!(
        "data:text/html;charset=utf-8;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(html)
    );
    request(
        transport,
        Some(&session),
        "Page.navigate",
        json!({"url":url}),
    )?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let ready = request(
            transport,
            Some(&session),
            "Runtime.evaluate",
            json!({"expression":"document.title==='Font policy fixture'&&document.readyState==='complete'","returnByValue":true}),
        )?;
        if ready["result"]["value"] == true {
            break;
        }
        if Instant::now() >= deadline {
            return Err(fail("Synthetic font page readiness deadline"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    request(
        transport,
        Some(&session),
        "Runtime.evaluate",
        json!({"expression":"document.fonts.ready.then(()=>true)","awaitPromise":true,"returnByValue":true}),
    )?;
    let document =
        request(transport, Some(&session), "DOM.getDocument", json!({}))?["root"]["nodeId"]
            .as_i64()
            .ok_or_else(|| fail("DOM root missing"))?;
    let mut observations = Vec::new();
    for (index, case) in rows.iter().enumerate() {
        let node = request(
            transport,
            Some(&session),
            "DOM.querySelector",
            json!({"nodeId":document,"selector":format!("#case-{index}")}),
        )?["nodeId"]
            .as_i64()
            .ok_or_else(|| fail("Font case node missing"))?;
        let fonts = request(
            transport,
            Some(&session),
            "CSS.getPlatformFontsForNode",
            json!({"nodeId":node}),
        )?["fonts"]
            .as_array()
            .cloned()
            .ok_or_else(|| fail("Platform font observation missing"))?;
        let observation = json!({"case":case,"fonts":fonts});
        observations.push(observation.clone());
        fs::write(
            output.join("observations.json"),
            format!("{}\n", serde_json::to_string_pretty(&observations)?),
        )?;
        let expected = case["expectedFamilies"].as_array().unwrap();
        let count = fonts
            .iter()
            .map(|font| font["glyphCount"].as_u64().unwrap_or(0))
            .sum::<u64>();
        if fonts.is_empty()
            || count != case["expectedGlyphCount"].as_u64().unwrap()
            || fonts.iter().any(|font| {
                font["glyphCount"].as_u64().unwrap_or(0) == 0
                    || !expected.contains(&font["familyName"])
            })
        {
            return Err(fail(format!("Font case failed: {}", case["name"])));
        }
    }
    let screenshot = request(
        transport,
        Some(&session),
        "Page.captureScreenshot",
        json!({"format":"png"}),
    )?["data"]
        .as_str()
        .ok_or_else(|| fail("Screenshot data absent"))?
        .to_owned();
    let image = base64::engine::general_purpose::STANDARD.decode(screenshot)?;
    if !image.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(fail("Expected painted PNG"));
    }
    fs::write(output.join("browser-fonts.png"), image)?;
    let result = json!({"browser":browser,"launcherSha256":format!("{:x}",Sha256::digest(fs::read(browser)?)),"version":version,"command":command,"observations":observations,"passed":true,"scope":"Actual headless page font selection and screenshot; native chrome and external pages are outside this fixture"});
    fs::write(
        output.join("browser-fonts-result.json"),
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_implicit_font_expectations() {
        assert!(validate(&json!([])).is_err());
        assert!(validate(&json!([{"name":"x","family":"sans","text":"Hi","expectedFamilies":[],"expectedGlyphCount":2}])).is_err());
        assert!(validate(&json!([{"name":"x","family":"sans","text":"Hi","expectedFamilies":["Some Font"],"expectedGlyphCount":2}])).is_ok());
    }
    #[test]
    fn escapes_generated_html() {
        assert!(page(&[json!({"family":"A\"<","text":"<&"})]).contains("A&quot;&lt;"));
    }
}
