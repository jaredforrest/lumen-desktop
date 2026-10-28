//! Explicit local text and image actions. Never monitor typing or open detected links.
use chrono::NaiveDate;
use regex::Regex;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

type ToolResult<T> = Result<T, String>;

fn xdg(name: &str, fallback: &str) -> ToolResult<PathBuf> {
    if let Some(value) = env::var_os(name) { return Ok(PathBuf::from(value)); }
    Ok(PathBuf::from(env::var_os("HOME").ok_or("HOME is missing")?).join(fallback))
}

fn language(value: Option<&str>) -> ToolResult<&str> {
    match value.unwrap_or("en_GB") {
        "en_GB" => Ok("en_GB"),
        "en_US" => Ok("en_US"),
        _ => Err("Choose English (United Kingdom) or English (United States)".into()),
    }
}

fn request_language(value: &Value) -> ToolResult<&str> {
    if value.is_null() { language(None) } else { language(Some(value.as_str().ok_or("Choose English (United Kingdom) or English (United States)")?)) }
}

fn dictionary(locale: &str) -> ToolResult<PathBuf> {
    let folder = xdg("XDG_STATE_HOME", ".local/state")?.join("lumen-desktop/content");
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
    let file = folder.join(format!("dictionary.{locale}.pws"));
    match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&file) {
        Ok(mut output) => output.write_all(b"personal_ws-1.1 en 0 utf-8\n").map_err(|error| error.to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.to_string()),
    }
    Ok(file)
}

fn command(program: &str, args: &[&str], input: Option<&[u8]>, seconds: u64) -> ToolResult<Vec<u8>> {
    let mut child = Command::new("timeout").arg(format!("{seconds}s")).arg(program).args(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|error| error.to_string())?;
    if let Some(bytes) = input {
        child.stdin.take().ok_or("No command input")?.write_all(bytes).map_err(|error| error.to_string())?;
    }
    let result = child.wait_with_output().map_err(|error| error.to_string())?;
    if !result.status.success() { return Err(format!("{program} failed: {}", String::from_utf8_lossy(&result.stderr).trim())); }
    Ok(result.stdout)
}

fn check_spelling(text: &str, locale: &str) -> ToolResult<Value> {
    let file = dictionary(locale)?;
    let mut input = String::new();
    for line in text.lines() { input.push('^'); input.push_str(line); input.push('\n'); }
    input.push('\n');
    let result = command("aspell", &[&format!("--lang={locale}"), "--encoding=utf-8",
        &format!("--personal={}", file.display()), "-a"], Some(input.as_bytes()), 8)?;
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for line in String::from_utf8_lossy(&result).lines() {
        if !line.starts_with("& ") && !line.starts_with("# ") { continue; }
        let Some(word) = line.split_whitespace().nth(1) else { continue };
        if seen.insert(word.to_owned()) {
            let suggestions: Vec<&str> = line.split_once(": ").map(|(_, value)| value.split(", ").take(8).collect())
                .unwrap_or_default();
            rows.push(json!({"word":word,"suggestions":suggestions}));
        }
    }
    Ok(json!({"spelling":rows}))
}

fn valid_word(word: &str) -> bool {
    Regex::new(r"^[A-Za-z][A-Za-z'-]{0,79}$").unwrap().is_match(word)
}

fn learn(word: &str, locale: &str) -> ToolResult<Value> {
    if !valid_word(word) { return Err("Enter one word to learn".into()); }
    let file = dictionary(locale)?;
    command("aspell", &[&format!("--lang={locale}"), "--encoding=utf-8",
        &format!("--personal={}", file.display()), "-a"], Some(format!("*{word}\n#\n").as_bytes()), 5)?;
    Ok(json!({"message":format!("Added {word} to your {} English dictionary", if locale == "en_GB" { "UK" } else { "US" })}))
}

fn quoted(value: &str, keep_slash: bool) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~@".contains(&byte) || (keep_slash && byte == b'/') {
            result.push(byte as char);
        } else { result.push_str(&format!("%{byte:02X}")); }
    }
    result
}

fn detectors(text: &str) -> Value {
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |label: String, uri: String| {
        if rows.len() < 40 && seen.insert(uri.clone()) { rows.push(json!({"label":label,"uri":uri})); }
    };
    for item in Regex::new(r#"https?://[^\s<>\"]+"#).unwrap().find_iter(text) {
        let uri = item.as_str().trim_end_matches([',', '.', ';', ')']).to_owned();
        add(uri.clone(), uri);
    }
    for item in Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap().find_iter(text) {
        let address = item.as_str();
        add(format!("Email {address}"), format!("mailto:{}", quoted(address, false)));
    }
    for item in Regex::new(r"\+?[0-9][0-9 ()-]{7,}[0-9]").unwrap().find_iter(text) {
        if text[..item.start()].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_')
            || text[item.end()..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') { continue; }
        let number = item.as_str();
        let digits: String = number.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
        if (10..=15).contains(&digits.trim_start_matches('+').len()) {
            add(format!("Call {number}"), format!("tel:{digits}"));
        }
    }
    for item in Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap().find_iter(text) {
        if let Ok(date) = NaiveDate::parse_from_str(item.as_str(), "%Y-%m-%d") {
            add(date.format("%A, %B %d, %Y").to_string(), String::new());
        }
    }
    json!({"actions":rows})
}

fn absolute(value: &str) -> ToolResult<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute() { Ok(path) } else { Ok(env::current_dir().map_err(|error| error.to_string())?.join(path)) }
}

fn temporary(prefix: &str) -> ToolResult<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_nanos();
    let path = env::temp_dir().join(format!("{prefix}-{}-{stamp}", std::process::id()));
    fs::create_dir(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

fn ocr(value: &str, page: i64, seconds: Option<f64>) -> ToolResult<Value> {
    let path = absolute(value)?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
        return Err("Choose a local image, PDF or video up to 64 MB".into());
    }
    let work = temporary("lumen-ocr")?;
    let result = (|| {
        let source = if let Some(position) = seconds {
            if !(0.0..=86400.0).contains(&position) { return Err("Invalid video position".into()); }
            let frame = work.join("frame.png");
            command("ffmpeg", &["-v", "error", "-ss", &position.to_string(), "-i", path.to_str().ok_or("Invalid path")?,
                "-frames:v", "1", "-vf", "scale=1600:1600:force_original_aspect_ratio=decrease",
                frame.to_str().ok_or("Invalid path")?], None, 15)?;
            frame
        } else if path.extension().is_some_and(|part| part.eq_ignore_ascii_case("pdf")) {
            let frame = work.join("page.png");
            let details = command("prlimit", &["--as=805306368", "--cpu=6", "--fsize=16777216", "--",
                "pdfinfo", path.to_str().ok_or("Invalid path")?], None, 8)?;
            let count = String::from_utf8_lossy(&details).lines()
                .find_map(|line| line.trim_start().strip_prefix("Pages:")?.trim().parse::<i64>().ok())
                .unwrap_or(1);
            let page = page.clamp(1, 100000).min(count.max(1)).to_string();
            command("prlimit", &["--as=805306368", "--cpu=6", "--fsize=16777216", "--", "pdftoppm",
                "-f", &page, "-singlefile", "-scale-to", "2000", "-png", path.to_str().ok_or("Invalid path")?,
                work.join("page").to_str().ok_or("Invalid path")?], None, 8)?;
            frame
        } else { path.clone() };
        let result = command("prlimit", &["--as=1073741824", "--cpu=20", "--", "tesseract",
            source.to_str().ok_or("Invalid path")?, "stdout", "-l", "eng"], None, 25)?;
        let text: String = String::from_utf8_lossy(&result).chars().take(32768).collect();
        let text = text.trim();
        Ok(json!({"text":text,"message":if text.is_empty() { "No text recognized" } else { "Text recognized locally" }}))
    })();
    let _ = fs::remove_dir_all(work);
    result
}

fn subjects() -> ToolResult<PathBuf> {
    let path = xdg("XDG_CACHE_HOME", ".cache")?.join("lumen-desktop/subjects");
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
    Ok(path)
}

fn subject_source(value: &str) -> ToolResult<PathBuf> {
    let path = PathBuf::from(value);
    let metadata = fs::symlink_metadata(&path).map_err(|_| "The subject preview is no longer available")?;
    let name = path.file_name().and_then(|part| part.to_str()).unwrap_or("");
    let valid_name = Regex::new(r"^[0-9a-f]{32}\.png$").unwrap().is_match(name);
    let parent = path.parent().and_then(|part| fs::canonicalize(part).ok());
    if !metadata.is_file() || metadata.file_type().is_symlink() || !valid_name
        || parent != fs::canonicalize(subjects()?).ok() {
        return Err("The subject preview is no longer available".into());
    }
    Ok(path)
}

fn file_uri(path: &Path) -> ToolResult<String> {
    Ok(format!("file://{}", quoted(path.to_str().ok_or("Invalid path")?, true)))
}

fn cutout(value: &str) -> ToolResult<Value> {
    let path = absolute(value)?;
    let suffix = path.extension().and_then(|part| part.to_str()).unwrap_or("").to_ascii_lowercase();
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !["png", "jpg", "jpeg", "webp"].contains(&suffix.as_str())
        || !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err("Choose a PNG, JPEG or WebP image up to 32 MB".into());
    }
    let uuid = fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error| error.to_string())?
        .trim().replace('-', "");
    let target = subjects()?.join(format!("{uuid}.png"));
    let result = Command::new("timeout").args(["65s", "prlimit", "--as=4294967296", "--cpu=60", "--",
            "rembg", "i", "-m", "u2netp", path.to_str().ok_or("Invalid path")?, target.to_str().ok_or("Invalid path")?])
        .env("OMP_NUM_THREADS", "4").output().map_err(|error| error.to_string())?;
    if !result.status.success() { let _ = fs::remove_file(&target); return Err("Subject extraction failed".into()); }
    let mut permissions = fs::metadata(&target).map_err(|error| error.to_string())?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&target, permissions).map_err(|error| error.to_string())?;
    Ok(json!({"subject":target,"uri":file_uri(&target)?,"message":"Subject extracted locally · inspect the edges before saving"}))
}

fn file_facts(metadata: &fs::Metadata) -> [i128; 6] {
    [metadata.dev() as i128, metadata.ino() as i128, metadata.mode() as i128,
        metadata.size() as i128, metadata.mtime() as i128 * 1_000_000_000 + metadata.mtime_nsec() as i128,
        metadata.ctime() as i128 * 1_000_000_000 + metadata.ctime_nsec() as i128]
}

fn captured_facts(value: &Value) -> ToolResult<Option<[i128; 6]>> {
    if value.is_null() { return Ok(None); }
    let values = value.as_array().ok_or("Invalid captured file version")?;
    if values.len() != 6 { return Err("Invalid captured file version".into()); }
    let mut facts = [0_i128; 6];
    for (index, item) in values.iter().enumerate() {
        let wire = item.as_str().ok_or("Invalid captured file version")?;
        let number = wire.parse::<i128>().map_err(|_| "Invalid captured file version")?;
        if number.to_string() != wire || number < i64::MIN as i128 || number > u64::MAX as i128 {
            return Err("Invalid captured file version".into());
        }
        facts[index] = number;
    }
    Ok(Some(facts))
}

fn subject_bytes(source: &str, expected: &Value) -> ToolResult<Vec<u8>> {
    let path = subject_source(source)?;
    let before = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    let facts = file_facts(&before);
    if !before.is_file() || before.len() > 64 * 1024 * 1024
        || captured_facts(expected)?.is_some_and(|value| value != facts) {
        return Err("The captured subject changed".into());
    }
    let mut file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path).map_err(|error| error.to_string())?;
    if file_facts(&file.metadata().map_err(|error| error.to_string())?) != facts {
        return Err("The captured subject changed".into());
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    Read::by_ref(&mut file).take(before.len() + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() as u64 != before.len() || file_facts(&fs::metadata(format!("/proc/self/fd/{}", file.as_raw_fd()))
        .map_err(|error| error.to_string())?) != facts {
        return Err("The captured subject changed".into());
    }
    Ok(bytes)
}

fn save_subject(source: &str, destination: &str, overwrite: bool, expected: &Value) -> ToolResult<Value> {
    let path = if let Some(tail) = destination.strip_prefix("~/") {
        PathBuf::from(env::var_os("HOME").ok_or("HOME is missing")?).join(tail)
    } else { absolute(destination)? };
    if !path.extension().is_some_and(|part| part.eq_ignore_ascii_case("png")) {
        return Err("Save the transparent image as PNG".into());
    }
    let bytes = subject_bytes(source, expected)?;
    let name = fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error| error.to_string())?;
    let staged = path.parent().ok_or("Invalid destination")?.join(format!(".lumen-export-{}", name.trim()));
    let result = (|| {
        let mut output = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&staged)
            .map_err(|error| error.to_string())?;
        output.write_all(&bytes).map_err(|error| error.to_string())?;
        output.sync_all().map_err(|error| error.to_string())?;
        let from = std::ffi::CString::new(staged.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
        let to = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
        let flags = if overwrite { 0 } else { libc::RENAME_NOREPLACE };
        let status = unsafe { libc::renameat2(libc::AT_FDCWD, from.as_ptr(), libc::AT_FDCWD, to.as_ptr(), flags) };
        if status != 0 {
            let error = std::io::Error::last_os_error();
            return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
                "Destination exists; confirm replacement before saving".into()
            } else { error.to_string() });
        }
        File::open(path.parent().ok_or("Invalid destination")?).and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    let _ = fs::remove_file(staged);
    result?;
    Ok(json!({"message":format!("Saved {}", path.display())}))
}

fn handle(data: &Value) -> ToolResult<Value> {
    let action = data["action"].as_str().unwrap_or("");
    match action {
        "cutout" => cutout(data["path"].as_str().ok_or("Missing path")?),
        "save-subject" => save_subject(data["source"].as_str().ok_or("Missing source")?,
            data["destination"].as_str().ok_or("Missing destination")?, data["overwrite"] == true, &data["sourceFacts"]),
        "copy-subject" => {
            let bytes = subject_bytes(data["source"].as_str().ok_or("Missing source")?, &data["sourceFacts"])?;
            command("wl-copy", &["--type", "image/png"], Some(&bytes), 5)?;
            Ok(json!({"message":"Transparent image copied"}))
        }
        "ocr" => ocr(data["path"].as_str().ok_or("Missing path")?,
            data["page"].as_i64().unwrap_or(1), data["seconds"].as_f64()),
        _ => {
            let text = data["text"].as_str().unwrap_or("");
            if text.is_empty() || text.chars().count() > 32768 {
                return Err("Enter between 1 and 32,768 characters".into());
            }
            match action {
                "spelling" => check_spelling(text, request_language(&data["language"])?),
                "learn" => learn(text, request_language(&data["language"])?),
                "detect" => Ok(detectors(text)),
                "copy" => {
                    command("wl-copy", &["--type", "text/plain;charset=utf-8"], Some(text.as_bytes()), 5)?;
                    Ok(json!({"message":"Copied"}))
                }
                _ => Err("Unknown contextual action".into()),
            }
        }
    }
}

fn main() {
    unsafe { libc::umask(0o077); }
    let mut line = String::new();
    let result = std::io::stdin().lock().read_line(&mut line)
        .map_err(|error| error.to_string())
        .and_then(|_| serde_json::from_str::<Value>(&line).map_err(|error| error.to_string()))
        .and_then(|request| handle(&request));
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => println!("{}", json!({"error":error})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detectors_only_report_actions() {
        let actions = detectors("https://example.org. x@example.org 2026-10-08 2026-02-31 +1 (415) 555-1234");
        let uris: Vec<&str> = actions["actions"].as_array().unwrap().iter()
            .filter_map(|row| row["uri"].as_str()).collect();
        assert_eq!(uris, ["https://example.org", "mailto:x@example.org", "tel:+14155551234", ""]);
        assert!(actions["actions"][3]["label"].as_str().unwrap().contains("October 08"));
    }
    #[test]
    fn rejects_protocol_commands_as_words() {
        for word in ["test\n#", "*learn", "two words", "/tmp/file", ""] {
            assert!(!valid_word(word));
        }
    }
    #[test]
    fn subject_file_must_be_owned_cache_entry() {
        let root = temporary("subject-test").unwrap();
        let cache = root.join("lumen-desktop/subjects");
        fs::create_dir_all(&cache).unwrap();
        let outside = root.join(format!("{}.png", "a".repeat(32)));
        fs::write(&outside, b"test").unwrap();
        unsafe { env::set_var("XDG_CACHE_HOME", &root); }
        assert!(subject_source(outside.to_str().unwrap()).is_err());
        let link = cache.join(outside.file_name().unwrap());
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(subject_source(link.to_str().unwrap()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn locale_and_subject_export_preserve_explicit_choices() {
        assert_eq!(language(None).unwrap(), "en_GB");
        assert_eq!(language(Some("en_US")).unwrap(), "en_US");
        assert!(request_language(&json!(false)).is_err());
        let root = temporary("subject-export-test").unwrap();
        let cache = root.join("lumen-desktop/subjects");
        fs::create_dir_all(&cache).unwrap();
        unsafe { env::set_var("XDG_CACHE_HOME", &root); }
        let source = cache.join(format!("{}.png", "b".repeat(32)));
        fs::write(&source, b"new PNG").unwrap();
        let facts = json!(file_facts(&fs::metadata(&source).unwrap()).map(|part| part.to_string()));
        let destination = root.join("output.png");
        fs::write(&destination, b"previous PNG").unwrap();
        assert!(save_subject(source.to_str().unwrap(), destination.to_str().unwrap(), false, &facts).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"previous PNG");
        save_subject(source.to_str().unwrap(), destination.to_str().unwrap(), true, &facts).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"new PNG");
        assert!(save_subject(source.to_str().unwrap(), destination.to_str().unwrap(), true, &json!(vec!["0"; 6])).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"new PNG");
        fs::remove_dir_all(root).unwrap();
    }
}
