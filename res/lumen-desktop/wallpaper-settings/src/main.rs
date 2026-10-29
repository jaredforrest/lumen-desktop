//! Local PNG/JPEG references and exact-display background selection.
use image::{ImageFormat, ImageReader};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

type Result<T> = std::result::Result<T, String>;
fn runtime() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is missing")?))
}
fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new().create(true).append(true).mode(0o600).open(path).map_err(|e| e.to_string())?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(file)
}
fn key_for(value: &str) -> String { format!("{:x}", Sha256::digest(value.as_bytes())) }
fn settings(args: &[&str]) -> Result<Value> {
    let output = Command::new("lumen-nix-settings").args(args).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(serde_json::from_slice::<Value>(&output.stdout).ok()
            .and_then(|value| value["error"].as_str().map(str::to_owned))
            .unwrap_or_else(|| String::from_utf8_lossy(&output.stderr).trim().to_owned()));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
fn saved() -> Result<Value> { Ok(settings(&["read"])?["wallpaper"].clone()) }
fn save(value: &Value) -> Result<()> { settings(&["set", "wallpaper", &value.to_string()]).map(|_| ()) }
fn sway(args: &[&str]) -> Result<Value> {
    let output = Command::new("timeout").arg("5s").arg("swaymsg").arg("-r")
        .args(args).output().map_err(|e| e.to_string())?;
    if !output.status.success() { return Err("The compositor did not respond".into()); }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
fn outputs() -> Result<Vec<Value>> {
    Ok(sway(&["-t", "get_outputs"])?.as_array().cloned().ok_or("Invalid display inventory")?
        .into_iter().filter(|item| item["active"] == true).collect())
}
fn identity(output: &Value) -> String {
    let values: Vec<&str> = ["make", "model", "serial"].iter()
        .map(|key| output[*key].as_str().unwrap_or("")).collect();
    if values.iter().all(|value| !value.is_empty()) && !matches!(values[2], "Unknown" | "0") {
        format!("display:{}", json!(values))
    } else { format!("connector:{}", output["name"].as_str().unwrap_or("")) }
}
fn image_info(path: &str) -> Result<Value> {
    if !path.starts_with('/') || path.contains(['\n', '\r', '\0']) {
        return Err("Choose an absolute local PNG or JPEG file.".into());
    }
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    let signature = json!([metadata.dev(), metadata.ino(), metadata.len(), metadata.mtime_nsec() as i128 + metadata.mtime() as i128*1_000_000_000,
        metadata.ctime_nsec() as i128 + metadata.ctime() as i128*1_000_000_000]);
    let directory = runtime()?.join("wallpaper-metadata");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let cache = directory.join(format!("{}.json", key_for(path)));
    if let Ok(saved) = fs::read(&cache).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).ok_or(()) {
        if saved["signature"] == signature && saved["metadata"].is_object() { return Ok(saved["metadata"].clone()); }
    }
    let reader = ImageReader::open(path).map_err(|e| e.to_string())?
        .with_guessed_format().map_err(|e| e.to_string())?;
    let format = match reader.format() {
        Some(ImageFormat::Png) => "PNG",
        Some(ImageFormat::Jpeg) => "JPEG",
        _ => return Err("Only still PNG and JPEG images are supported.".into()),
    };
    if format == "PNG" && animated_png(path)? { return Err("Only still PNG and JPEG images are supported.".into()); }
    let (width, height) = image::image_dimensions(path).map_err(|e| e.to_string())?;
    if u64::from(width) * u64::from(height) > 64_000_000 {
        return Err("Image exceeds the 64 million pixel limit.".into());
    }
    let decoded = reader.decode().map_err(|e| e.to_string())?;
    if decoded.width() != width || decoded.height() != height { return Err("Image dimensions changed".into()); }
    let result = json!({"width":width,"height":height,"format":format});
    let pending = directory.join(format!("{}.{}.new", key_for(path), std::process::id()));
    let mut file = OpenOptions::new().create_new(true).write(true).mode(0o600).open(&pending).map_err(|e| e.to_string())?;
    file.write_all(json!({"signature":signature,"metadata":result}).to_string().as_bytes()).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(pending, cache).map_err(|e| e.to_string())?;
    Ok(result)
}
fn animated_png(path: &str) -> Result<bool> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut signature = [0u8; 8];
    file.read_exact(&mut signature).map_err(|e| e.to_string())?;
    if signature != [137, 80, 78, 71, 13, 10, 26, 10] { return Err("Invalid PNG signature".into()); }
    loop {
        let mut header = [0u8; 8];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as i64;
        match &header[4..] {
            b"acTL" => return Ok(true),
            b"IDAT" | b"IEND" => return Ok(false),
            _ => { file.seek(SeekFrom::Current(length + 4)).map_err(|e| e.to_string())?; }
        }
    }
}
fn fallback() -> String { env::var("LUMEN_WALLPAPER_FALLBACK").unwrap_or_else(|_| "wallpaper.png".into()) }
fn selected(settings: &Value, output: &Value) -> Value {
    let identity = identity(output);
    let display = &settings["displays"][key_for(&identity)];
    if display["identity"] == identity { display["image"].clone() } else { settings["defaultImage"].clone() }
}
fn status() -> Result<Value> {
    let settings = saved()?;
    let outputs = outputs()?;
    let connected: Vec<String> = outputs.iter().map(identity).collect();
    let mut rows = Vec::new();
    for output in outputs {
        let identity = identity(&output);
        let image = selected(&settings, &output);
        let (metadata, error) = match image.as_str() {
            Some(path) => match image_info(path) { Ok(metadata) => (metadata, "".to_owned()), Err(error) => (Value::Null, error) },
            None => (Value::Null, String::new()),
        };
        rows.push(json!({"identity":identity,"key":key_for(&identity),"name":output["name"],
            "scale":output.get("scale").unwrap_or(&json!(1)),"image":image,
            "metadata":metadata,"fallback":metadata.is_null(),"error":error}));
    }
    let (default_metadata, default_error) = match settings["defaultImage"].as_str() {
        Some(path) => match image_info(path) { Ok(metadata) => (metadata, "".to_owned()), Err(error) => (Value::Null, error) },
        None => (Value::Null, String::new()),
    };
    let disconnected: Vec<Value> = settings["displays"].as_object().into_iter().flat_map(|map| map.iter())
        .filter(|(_, value)| !connected.iter().any(|identity| value["identity"] == *identity))
        .map(|(key, value)| { let mut row = object(value); row.insert("key".into(), json!(key)); Value::Object(row) }).collect();
    Ok(json!({"available":true,"outputs":rows,"settings":settings,"defaultMetadata":default_metadata,
        "defaultError":default_error,"disconnected":disconnected,"defaultImage":settings["defaultImage"],"error":"",
        "capabilities":{"perDisplay":true,"perDesktop":false,"formats":["PNG","JPEG"],"maxPixels":64000000}}))
}
fn object(value: &Value) -> Map<String, Value> { value.as_object().cloned().unwrap_or_default() }
fn browse(path: Option<&str>) -> Result<Value> {
    let home = env::var("HOME").map_err(|e| e.to_string())?;
    let mut folder = PathBuf::from(path.unwrap_or(&home));
    if !folder.is_absolute() { return Err("Choose an absolute local folder.".into()); }
    let mut selected = Value::Null;
    if folder.is_file() {
        selected = json!(folder.to_string_lossy().as_ref());
        folder = folder.parent().ok_or("Invalid image folder")?.to_path_buf();
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&folder).map_err(|e| e.to_string())? {
        let Ok(entry) = entry else { continue; };
        let path = entry.path();
        let directory = path.is_dir();
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase();
        if directory || matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
            entries.push(json!({"name":entry.file_name().to_string_lossy(),"path":path.to_string_lossy(),"directory":directory}));
        }
    }
    entries.sort_by_key(|row| (row["directory"] != true, row["name"].as_str().unwrap_or("").to_lowercase()));
    let truncated = entries.len() > 1000;
    entries.truncate(1000);
    Ok(json!({"action":"browse","folder":folder.to_string_lossy(),"parent":folder.parent().unwrap_or(&folder).to_string_lossy(),
        "selected":selected,"entries":entries,"truncated":truncated,"error":""}))
}
fn apply() -> Result<Value> {
    let _lock = lock(&runtime()?.join("wallpaper-render.lock"))?;
    let snapshot = status()?;
    let directory = runtime()?.join("wallpaper-images");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    for output in snapshot["outputs"].as_array().ok_or("Invalid wallpaper status")? {
        let image = if output["fallback"] == true { fallback() }
            else { output["image"].as_str().ok_or("Missing image")?.to_owned() };
        let reference = directory.join(format!("{}.png", output["key"].as_str().ok_or("Missing display key")?));
        let pending = reference.with_extension("new");
        if pending.exists() { fs::remove_file(&pending).map_err(|e| e.to_string())?; }
        symlink(&image, &pending).map_err(|e| e.to_string())?;
        fs::rename(&pending, &reference).map_err(|e| e.to_string())?;
        let command = format!("output {} bg {} fill \"#202024\"", output["name"], json!(reference.to_string_lossy()));
        let reply = sway(&[&command])?;
        if reply.as_array().is_none_or(|rows| rows.is_empty() || !rows.iter().all(|row| row["success"] == true)) {
            return Err(format!("The compositor rejected the background for {}", output["name"].as_str().unwrap_or("")));
        }
    }
    Ok(snapshot)
}
fn change(request: &Value) -> Result<Value> {
    let scope = request["scope"].as_str().ok_or("Choose all displays or one connected display.")?;
    if !matches!(scope, "default" | "display") { return Err("Choose all displays or one connected display.".into()); }
    let image = &request["image"];
    if let Some(path) = image.as_str() { image_info(path)?; }
    else if !image.is_null() { return Err("Choose an absolute local PNG or JPEG file.".into()); }
    let _lock = lock(&runtime()?.join("wallpaper.lock"))?;
    let current = saved()?;
    let mut value = json!({"defaultImage":current["defaultImage"],"displays":object(&current["displays"])});
    if scope == "default" { value["defaultImage"] = image.clone(); }
    else {
        let target = request["identity"].as_str().unwrap_or("");
        let matching: Vec<Value> = status()?["outputs"].as_array().into_iter().flat_map(|rows| rows.iter())
            .filter(|row| row["identity"] == target).cloned().collect();
        if matching.len() != 1 { return Err("The selected display is disconnected or ambiguous. Refresh and choose again.".into()); }
        let key = key_for(target);
        if image.is_null() { value["displays"].as_object_mut().unwrap().remove(&key); }
        else { value["displays"][key] = json!({"identity":target,"image":image}); }
    }
    save(&value)?;
    drop(_lock);
    apply()
}
fn watch() -> Result<()> {
    let mut previous = Value::Null;
    loop {
        let result = (|| -> Result<()> {
            let snapshot = status()?;
            let signatures: Vec<Value> = snapshot["outputs"].as_array().ok_or("Invalid wallpaper status")?
                .iter().map(|row| {
                    let path = row["image"].as_str().unwrap_or("");
                    let path = if path.is_empty() { fallback() } else { path.to_owned() };
                    let stamp = fs::metadata(&path).ok().map(|item| json!([item.mtime_nsec() as i128 + item.mtime() as i128*1_000_000_000,item.len(),item.ino()]));
                    json!([row, stamp])
                }).collect();
            let signature = json!(signatures);
            if signature != previous { apply()?; previous = signature; }
            Ok(())
        })();
        if let Err(error) = result { println!("{}", json!({"error":error})); }
        thread::sleep(Duration::from_secs(2));
    }
}
fn main() {
    if env::args().nth(1).as_deref() == Some("watch") {
        if let Err(error) = watch() { eprintln!("{error}"); std::process::exit(1); }
        return;
    }
    let mut line = String::new();
    let result = std::io::stdin().lock().read_line(&mut line).map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_str::<Value>(line.lines().next().unwrap_or("")).map_err(|e| e.to_string()))
        .and_then(|request| {
            if !request.is_object() { return Err("A wallpaper request object is required.".into()); }
            match request["action"].as_str() {
                Some("browse") => browse(request["path"].as_str()),
                Some("status") => status(),
                Some("set") => change(&request),
                _ => Err("Unsupported wallpaper action.".into()),
            }
        });
    match result { Ok(value) => println!("{value}"),
        Err(error) => println!("{}", json!({"available":false,"error":error})) }
}
