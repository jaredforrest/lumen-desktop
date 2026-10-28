//! Validated display changes with a separate rollback process.
use serde_json::{Value, json};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type DisplayResult<T> = Result<T, String>;

fn output(program: &str, args: &[&str], seconds: u64) -> DisplayResult<Vec<u8>> {
    let result = Command::new("timeout").arg(format!("{seconds}s")).arg(program).args(args)
        .output().map_err(|error| error.to_string())?;
    if !result.status.success() { return Err(format!("{program} failed")); }
    Ok(result.stdout)
}

fn output_data() -> DisplayResult<Vec<Value>> {
    serde_json::from_slice::<Vec<Value>>(&output("swaymsg", &["-r", "-t", "get_outputs"], 4)?)
        .map_err(|error| error.to_string())
}

fn mode_string(mode: &Value) -> DisplayResult<String> {
    let width = mode["width"].as_i64().ok_or("Missing display width")?;
    let height = mode["height"].as_i64().ok_or("Missing display height")?;
    let refresh = mode["refresh"].as_f64().ok_or("Missing display refresh")?;
    Ok(format!("{width}x{height}@{:.3}Hz", refresh / 1000.0))
}

fn identity(output: &Value) -> Value {
    json!([output["name"], output["make"], output["model"], output["serial"]])
}

fn setting(output: &Value) -> DisplayResult<Value> {
    Ok(json!({"name":output["name"],"identity":identity(output),
        "mode":mode_string(&output["current_mode"] )?,"scale":output["scale"],
        "x":output["rect"]["x"],"y":output["rect"]["y"]}))
}

fn position(value: Option<&Value>) -> DisplayResult<i64> {
    let number = match value { None => 0, Some(Value::Number(n)) => n.as_i64().ok_or("Invalid position")?,
        Some(Value::String(s)) => s.parse().map_err(|_| "Invalid position")?, _ => return Err("Invalid position".into()) };
    if !(-32768..=32767).contains(&number) { return Err("Display position is out of range".into()); }
    Ok(number)
}

fn validate(data: &Value, outputs: &[Value]) -> DisplayResult<Value> {
    let screen = outputs.iter().find(|item| item["name"] == data["name"] && item["active"] == true)
        .ok_or("The display is no longer connected")?;
    let mode = data["mode"].as_str().ok_or("Choose a mode reported by this display")?;
    let mut reported = screen["modes"].as_array().ok_or("Display modes are unavailable")?
        .iter().map(mode_string).collect::<DisplayResult<Vec<_>>>()?;
    if !screen["current_mode"].is_null() { reported.push(mode_string(&screen["current_mode"])?); }
    if !reported.iter().any(|item| item == mode) { return Err("Choose a mode reported by this display".into()); }
    let scale = match data.get("scale") { None => 1.0, Some(value) => value.as_f64().ok_or("Choose a supported scale")? };
    if ![1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0].contains(&scale) {
        return Err("Choose a supported scale".into());
    }
    Ok(json!({"name":screen["name"],"identity":identity(screen),"mode":mode,"scale":scale,
        "x":position(data.get("x"))?,"y":position(data.get("y"))?}))
}

fn apply_one(data: &Value) -> DisplayResult<()> {
    let name = serde_json::to_string(&data["name"]).map_err(|error| error.to_string())?;
    let mode = serde_json::to_string(&data["mode"]).map_err(|error| error.to_string())?;
    let command = format!("output {name} mode {mode} scale {} position {} {}",
        data["scale"], data["x"], data["y"]);
    let replies: Vec<Value> = serde_json::from_slice(&output("swaymsg", &["-r", &command], 5)?)
        .map_err(|error| error.to_string())?;
    if replies.is_empty() || !replies.iter().all(|reply| reply["success"] == true) {
        return Err("The compositor rejected the display setting".into());
    }
    Ok(())
}

fn pending_path() -> DisplayResult<PathBuf> {
    Ok(PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is missing")?)
        .join("lumen-display-change.json"))
}

fn lock(path: &Path) -> DisplayResult<File> {
    let file = OpenOptions::new().create(true).append(true).mode(0o600).open(path.with_extension("lock"))
        .map_err(|error| error.to_string())?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(file)
}

fn atomic(path: &Path, data: &Value) -> DisplayResult<()> {
    fs::create_dir_all(path.parent().ok_or("Pending path has no parent")?).map_err(|error| error.to_string())?;
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new().create_new(true).write(true).mode(0o600).open(&temp)
            .map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut file, data).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temp, path).map_err(|error| error.to_string())
    })();
    if temp.exists() { let _ = fs::remove_file(temp); }
    result
}

fn read(path: &Path) -> Value {
    fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(Value::is_object).unwrap_or_else(|| json!({}))
}

fn now() -> DisplayResult<f64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_secs_f64())
}

fn confirmation_live(state: &Value, token: Option<&str>, time: f64) -> bool {
    state["token"].as_str().is_some_and(|expected| Some(expected) == token)
        && time < state["deadline"].as_f64().unwrap_or(0.0)
}

fn revert(pending: &Path, token: Option<&str>) -> DisplayResult<()> {
    let state = read(pending);
    if state.as_object().is_none_or(|object| object.is_empty())
        || token.is_some_and(|token| state["token"] != token) { return Ok(()); }
    let outputs = output_data()?;
    let mut errors = Vec::new();
    for previous in state["previous"].as_array().ok_or("Missing previous layouts")? {
        if outputs.iter().any(|screen| screen["active"] == true && identity(screen) == previous["identity"]) {
            if let Err(error) = apply_one(previous) { errors.push(error); }
        }
    }
    if !errors.is_empty() { return Err(errors.join("; ")); }
    if pending.exists() { fs::remove_file(pending).map_err(|error| error.to_string())?; }
    Ok(())
}

fn settings(args: &[&str]) -> DisplayResult<Value> {
    serde_json::from_slice(&output("lumen-nix-settings", args, if args[0] == "read" {120} else {3600})?)
        .map_err(|error| error.to_string())
}

fn start_guard(token: &str) -> DisplayResult<()> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let mut child = Command::new("lumen-launch-app").arg("--").arg(executable).arg("guard").arg(token)
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().ok_or("No guard output")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send((result, line));
    });
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok((Ok(_), line)) if line.trim() == "ready" => Ok(()),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err("Could not start display rollback protection; no settings changed".into())
        }
    }
}

fn handle(data: &Value) -> DisplayResult<Value> {
    let pending = pending_path()?;
    let action = data["action"].as_str().unwrap_or("status");
    if action == "status" {
        let mut outputs = Vec::new();
        for screen in output_data()? {
            if screen["active"] != true || screen["current_mode"].is_null() { continue; }
            let mut item = setting(&screen)?;
            let mut modes = Vec::new();
            for mode in std::iter::once(&screen["current_mode"])
                .chain(screen["modes"].as_array().ok_or("Missing display modes")?.iter()) {
                let value = mode_string(mode)?;
                if !modes.contains(&value) { modes.push(value); }
            }
            item["modes"] = json!(modes);
            let label = [screen["make"].as_str().unwrap_or(""), screen["model"].as_str().unwrap_or("")]
                .join(" ").trim().to_owned();
            item["label"] = json!(if label.is_empty() { screen["name"].as_str().unwrap_or("") } else { &label });
            outputs.push(item);
        }
        let state = read(&pending);
        let seconds = (state["deadline"].as_f64().unwrap_or(0.0) - now()?).max(0.0) as i64;
        return Ok(json!({"outputs":outputs,"pending":state["token"].as_str().unwrap_or(""),"seconds":seconds}));
    }
    let _guard = lock(&pending)?;
    match action {
        "apply" => {
            if pending.exists() { return Err("Confirm or revert the pending display change first".into()); }
            let outputs = output_data()?;
            let candidate = validate(data, &outputs)?;
            let token = fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error| error.to_string())?
                .trim().replace('-', "");
            let previous = outputs.iter().filter(|screen| screen["active"] == true && !screen["current_mode"].is_null())
                .map(setting).collect::<DisplayResult<Vec<_>>>()?;
            atomic(&pending, &json!({"token":token,"candidate":candidate,"previous":previous,"deadline":now()?+20.0}))?;
            if let Err(error) = start_guard(&token) { let _ = fs::remove_file(&pending); return Err(error); }
            if let Err(error) = apply_one(&candidate) { revert(&pending, Some(&token))?; return Err(error); }
            Ok(json!({"pending":token,"seconds":20}))
        }
        "confirm" => {
            let mut state = read(&pending);
            if !confirmation_live(&state, data["token"].as_str(), now()?) {
                return Err("The confirmation period has expired".into());
            }
            let candidate = state["candidate"].clone();
            if !output_data()?.iter().any(|screen| screen["active"] == true && identity(screen) == candidate["identity"]) {
                return Err("The display disconnected; the change was not saved".into());
            }
            let mut layouts = settings(&["read"])?["displays"].clone();
            let name = candidate["name"].as_str().ok_or("Invalid display name")?.to_owned();
            layouts[&name] = candidate;
            state["confirmed"] = json!(true);
            atomic(&pending, &state)?;
            if let Err(error) = settings(&["set", "displays", &layouts.to_string()]) {
                revert(&pending, state["token"].as_str())?;
                return Err(error);
            }
            fs::remove_file(&pending).map_err(|error| error.to_string())?;
            Ok(json!({"ok":true}))
        }
        "revert" => { revert(&pending, data["token"].as_str())?; Ok(json!({"ok":true})) }
        "forget" => { settings(&["set", "displays", "{}"]) ?; Ok(json!({"ok":true})) }
        _ => Err("Unknown display action".into()),
    }
}

fn guard(token: &str) -> DisplayResult<()> {
    let pending = pending_path()?;
    println!("ready");
    std::io::stdout().flush().map_err(|error| error.to_string())?;
    loop {
        let state = read(&pending);
        if state["token"] != token || state["confirmed"] == true { return Ok(()); }
        if now()? >= state["deadline"].as_f64().unwrap_or(0.0) {
            let _lock = lock(&pending)?;
            return revert(&pending, Some(token));
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn restore_saved() -> DisplayResult<()> {
    if pending_path()?.exists() { return Ok(()); }
    let current = output_data()?;
    if let Some(layouts) = settings(&["read"])?["displays"].as_object() {
        for value in layouts.values() {
            if !value.is_object() { continue; }
            if let Some(screen) = current.iter().find(|screen| screen["active"] == true && identity(screen) == value["identity"]) {
                if setting(screen).ok().as_ref() != Some(value) {
                    if let Ok(candidate) = validate(value, &current) { let _ = apply_one(&candidate); }
                }
            }
        }
    }
    Ok(())
}

fn watch() -> DisplayResult<()> {
    restore_saved()?;
    let mut child = Command::new("swaymsg").args(["-m", "-r", "-t", "subscribe", "[\"output\"]"])
        .stdout(Stdio::piped()).spawn().map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().ok_or("No Sway subscription")?;
    for line in BufReader::new(stdout).lines() {
        line.map_err(|error| error.to_string())?;
        thread::sleep(Duration::from_millis(300));
        restore_saved()?;
    }
    Ok(())
}

fn main() {
    unsafe { libc::umask(0o077); }
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.as_slice() {
        [mode, token] if mode == "guard" => guard(token),
        [mode] if mode == "watch" => watch(),
        [] => {
            let mut line = String::new();
            let request = std::io::stdin().lock().read_line(&mut line)
                .map_err(|error| error.to_string())
                .and_then(|_| serde_json::from_str::<Value>(line.lines().next().unwrap_or(""))
                    .map_err(|error| error.to_string()))
                .and_then(|data| handle(&data));
            match request { Ok(value) => println!("{value}"), Err(error) => println!("{}", json!({"error":error})) }
            Ok(())
        }
        _ => Err("Unknown display settings command".into()),
    };
    if let Err(error) = result { eprintln!("{error}"); std::process::exit(1); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_reported_mode_and_exact_identity() {
        let outputs = vec![json!({"name":"DP-1","active":true,"make":"Acme","model":"A","serial":"4",
            "current_mode":{"width":1920,"height":1080,"refresh":60000},
            "modes":[{"width":1920,"height":1080,"refresh":60000}],
            "scale":1,"rect":{"x":0,"y":0}})];
        let requested = json!({"name":"DP-1","mode":"1920x1080@60.000Hz","scale":1.25,"x":-5,"y":10});
        let accepted = validate(&requested, &outputs).unwrap();
        assert_eq!(accepted["identity"], json!(["DP-1","Acme","A","4"]));
        assert_eq!(accepted["x"], -5);
        assert!(validate(&json!({"name":"DP-1","mode":"1024x768@60.000Hz"}), &outputs).is_err());
        assert!(validate(&json!({"name":"DP-1","mode":"1920x1080@60.000Hz","scale":4}), &outputs).is_err());
        assert!(validate(&json!({"name":"DP-1","mode":"1920x1080@60.000Hz","x":99999}), &outputs).is_err());
    }
    #[test]
    fn expired_confirmation_cannot_be_accepted() {
        let pending = json!({"token":"pending","deadline":100.0});
        assert!(confirmation_live(&pending, Some("pending"), 99.0));
        assert!(!confirmation_live(&pending, Some("pending"), 100.0));
        assert!(!confirmation_live(&pending, Some("other"), 99.0));
    }
}
