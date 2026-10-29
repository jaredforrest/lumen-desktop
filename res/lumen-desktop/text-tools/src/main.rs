use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, String>;
static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn signal_stop(_: libc::c_int) { STOP.store(true, Ordering::SeqCst); }
fn command(name: &str, args: &[&str], input: Option<&[u8]>, timeout: Duration) -> Result<Vec<u8>> {
    let mut child = Command::new(name).args(args).stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().ok_or("Missing command stdout")?;
    let mut stderr = child.stderr.take().ok_or("Missing command stderr")?;
    let out_reader = thread::spawn(move || { let mut data=Vec::new(); stdout.read_to_end(&mut data).map(|_|data) });
    let err_reader = thread::spawn(move || { let mut data=Vec::new(); stderr.read_to_end(&mut data).map(|_|data) });
    if let Some(bytes) = input { child.stdin.take().unwrap().write_all(bytes).map_err(|e| e.to_string())?; }
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if STOP.load(Ordering::SeqCst) {
            let _=child.kill(); let _=child.wait();
            let _=out_reader.join(); let _=err_reader.join();
            return Err("Speech stopped".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let stdout = out_reader.join().map_err(|_|"Command reader failed")?.map_err(|e|e.to_string())?;
            let stderr = err_reader.join().map_err(|_|"Command reader failed")?.map_err(|e|e.to_string())?;
            if status.success() { return Ok(stdout); }
            if let Ok(reply) = serde_json::from_slice::<Value>(&stdout) {
                if let Some(error) = reply["error"].as_str() { return Err(error.to_owned()); }
            }
            let error = String::from_utf8_lossy(&stderr).trim().to_owned();
            return Err(if error.is_empty() { format!("{name} failed") } else { error });
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = out_reader.join();
    let _ = err_reader.join();
    Err(format!("{name} timed out"))
}
fn output(name: &str, args: &[&str], timeout: u64) -> Result<String> {
    String::from_utf8(command(name, args, None, Duration::from_secs(timeout))?).map_err(|e| e.to_string())
}
fn settings() -> Result<Value> {
    let bin = env::var("LUMEN_NIX_SETTINGS_BIN").unwrap_or_else(|_| "lumen-nix-settings".into());
    serde_json::from_str(&output(&bin, &["read"], 120)?).map_err(|e| e.to_string())
}
fn set_section(section: &str, value: &Value) -> Result<Value> {
    let bin = env::var("LUMEN_NIX_SETTINGS_BIN").unwrap_or_else(|_| "lumen-nix-settings".into());
    serde_json::from_str(&output(&bin, &["set", section, &value.to_string()], 3600)?).map_err(|e| e.to_string())
}
fn language(value: Option<&str>) -> Result<&str> {
    let value = value.unwrap_or("en_GB");
    if ["en_GB", "en_US"].contains(&value) { Ok(value) }
    else { Err("Choose English (United Kingdom) or English (United States)".into()) }
}
fn state_path() -> Result<PathBuf> {
    let home = env::var_os("HOME").ok_or("HOME is missing")?;
    Ok(env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(home).join(".local/state"))
        .join("lumen-desktop/content/snippets.json"))
}
fn snippets(value: Option<&Value>) -> Result<Value> {
    let path = state_path()?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let lock_path = path.with_extension("lock");
    let lock = OpenOptions::new().write(true).create(true).mode(0o600).open(lock_path).map_err(|e| e.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 { return Err("Snippet lock failed".into()); }
    if let Some(value) = value {
        let temp = path.with_extension(format!("tmp-{}", std::process::id()));
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp).map_err(|e| e.to_string())?;
        file.write_all(value.to_string().as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(temp, &path).map_err(|e| e.to_string())?;
    }
    let data = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).unwrap_or_else(|| json!([]));
    Ok(if data.is_array() { data } else { json!([]) })
}
fn required<'a>(data: &'a Value, key: &str) -> Result<&'a str> { data[key].as_str().ok_or(format!("Missing {key}")) }
fn selection() -> Result<String> {
    let text = output("wl-paste", &["--primary","--no-newline","--type","text"], 3)
        .map_err(|_| "This app does not expose selected text. Paste text into this panel to use the tools.".to_owned())?;
    if text.trim().is_empty() || text.chars().count() > 16384 { return Err("Select between 1 and 16,384 characters".into()); }
    Ok(text)
}
fn menu_state() -> Result<Value> {
    let raw = output("busctl", &["--user","--json=short","call","com.canonical.AppMenu.Registrar",
        "/com/canonical/AppMenu/Registrar","org.lumendesktop.GlobalMenuPrototype","State"], 5)?;
    let reply: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let encoded = reply["data"][0].as_str().ok_or("Invalid menu response")?;
    serde_json::from_str(encoded).map_err(|e| e.to_string())
}
fn clean_label(label: &str) -> String { label.replace("__", "\0").replace('_', "").replace('\0', "_") }
fn find_menu(node: &Value, parents: &[String], wanted: &str, matches: &mut Vec<i64>) {
    let label = clean_label(node["label"].as_str().unwrap_or(""));
    let mut path = parents.to_vec();
    if !label.is_empty() { path.push(label); }
    if node["enabled"] == true && node["visible"] == true && node["submenu"] != true
        && node["separator"] != true && path.join("/") == wanted {
        if let Some(id) = node["id"].as_i64() { matches.push(id); }
    }
    if let Some(children) = node["children"].as_array() { for child in children { find_menu(child, &path, wanted, matches); } }
}
fn menu(path: &str, app: Option<&str>) -> Result<Value> {
    let state = menu_state()?;
    if let Some(app) = app { if state["appName"] != app { return Err("This shortcut belongs to another application".into()); } }
    if state["supported"] != true { return Err("The focused app has no exported menu".into()); }
    let mut matches = Vec::new();
    for child in state["menu"]["children"].as_array().ok_or("Invalid menu")? { find_menu(child, &[], path, &mut matches); }
    if matches.len() != 1 { return Err("Menu command is unavailable or ambiguous".into()); }
    let token = state["token"].as_str().ok_or("Missing menu token")?;
    let raw = output("busctl", &["--user","--json=short","call","com.canonical.AppMenu.Registrar",
        "/com/canonical/AppMenu/Registrar","org.lumendesktop.GlobalMenuPrototype","Activate","si",token,&matches[0].to_string()], 5)?;
    let reply: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if reply["data"][0] != true { return Err("Menu focus changed".into()); }
    Ok(json!({"ok":true}))
}
fn runtime() -> Result<PathBuf> { Ok(PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is missing")?)) }
fn stop(unit: &str) { let _ = output("systemctl", &["--user","stop",unit], 8); }
fn dictation(action: &str) -> Result<Value> {
    let recording = runtime()?.join("lumen-dictation.wav");
    let path = recording.to_str().ok_or("Invalid recording path")?;
    match action {
        "dictation-start" => {
            stop("lumen-dictation.service"); let _ = fs::remove_file(&recording);
            output("systemd-run", &["--user","--collect","--quiet","--unit=lumen-dictation",
                "--property=KillSignal=SIGINT","--property=RuntimeMaxSec=120","--property=UMask=0077",
                &format!("--setenv=PATH={}",env::var("PATH").unwrap_or_default()),
                &format!("--setenv=XDG_RUNTIME_DIR={}",runtime()?.display()),
                "pw-record","--rate=16000","--channels=1","--format=s16",path], 8)?;
            Ok(json!({"recording":true,"message":"Recording · microphone active · maximum 2 minutes"}))
        }
        "dictation-cancel" | "dictation-finish" => {
            stop("lumen-dictation.service");
            let result = (|| -> Result<Value> {
                if action == "dictation-cancel" { return Ok(json!({"recording":false,"message":"Recording discarded"})); }
                if fs::metadata(&recording).map(|m| m.len()).unwrap_or(0) < 1000 { return Err("No microphone audio was captured".into()); }
                let model = env::var("LUMEN_DICTATION_MODEL").unwrap_or_default();
                if !Path::new(&model).is_file() { return Err("The local speech model is unavailable".into()); }
                let base = runtime()?.join(format!("lumen-transcript-{}", std::process::id()));
                fs::create_dir(&base).map_err(|e| e.to_string())?;
                let transcript = base.join("transcript");
                let result = output("whisper-cli", &["-m",&model,"-f",path,"-l","en","-t","4","-otxt","-of",
                    transcript.to_str().ok_or("Invalid transcript path")?,"-nt","-np"], 90)
                    .and_then(|_| fs::read_to_string(transcript.with_extension("txt")).map_err(|e| e.to_string()));
                let _ = fs::remove_dir_all(base);
                Ok(json!({"text":result?.trim(),"message":"Transcribed locally · review before inserting","recording":false}))
            })();
            let _ = fs::remove_file(recording);
            result
        }
        _ => Err("Unknown dictation action".into()),
    }
}
fn text(data: &Value) -> Result<&str> {
    let value = required(data, "text")?;
    if value.is_empty() || value.chars().count() > 16384 { return Err("Enter between 1 and 16,384 characters".into()); }
    Ok(value)
}
fn valid_lookup(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && word.len() <= 80
        && chars.all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '\'' || c == '-')
}
fn focused(node: &Value) -> Option<i64> {
    if node["focused"] == true && ["con","floating_con"].contains(&node["type"].as_str().unwrap_or("")) { return node["id"].as_i64(); }
    for key in ["nodes","floating_nodes"] { if let Some(children) = node[key].as_array() {
        for child in children { if let Some(id) = focused(child) { return Some(id); } }
    }}
    None
}
fn handle(data: &Value) -> Result<Value> {
    let action = required(data, "action")?;
    if ["spelling","learn","detect","copy","ocr"].contains(&action) {
        let helper = env::var("LUMEN_CONTEXT_TOOLS").unwrap_or_else(|_| "lumen-context-tools".into());
        return serde_json::from_slice(&command(&helper, &[], Some(format!("{data}\n").as_bytes()), Duration::from_secs(75))?)
            .map_err(|e| e.to_string());
    }
    if action.starts_with("dictation-") { return dictation(action); }
    match action {
        "status" | "automatic-replacements" | "language" => {
            let mut options = settings()?["textTools"].clone();
            if action == "automatic-replacements" {
                options["automaticReplacements"] = json!(data["enabled"].as_bool().ok_or("Missing enabled")?);
                options = set_section("textTools", &options)?["textTools"].clone();
            }
            if action == "language" {
                options["language"] = json!(language(data["value"].as_str())?);
                options = set_section("textTools", &options)?["textTools"].clone();
            }
            Ok(json!({"snippets":snippets(None)?,"automaticReplacements":options["automaticReplacements"],
                "language":options["language"].as_str().unwrap_or("en_GB")}))
        }
        "save-shortcut" => {
            let slot = data["slot"].as_i64().ok_or("Invalid slot")?;
            let path = required(data,"path")?; let app = required(data,"app")?;
            if !(1..=12).contains(&slot) || app.is_empty() || app.len()>256 || !path.contains('/') || path.len()>512 {
                return Err("Choose a slot, application and exact menu path such as File/Save".into());
            }
            let mut shortcuts = settings()?["menuShortcuts"].clone();
            shortcuts[slot.to_string()] = json!({"app":app,"path":path});
            set_section("menuShortcuts", &shortcuts)?;
            Ok(json!({"message":format!("Saved Super+Alt+F{slot} in Nix for {app}")}))
        }
        "selection" => Ok(json!({"text":selection()?})),
        "save" => {
            let name = required(data,"name")?.trim(); let content = required(data,"text")?;
            if name.is_empty() || name.chars().count()>80 || content.is_empty() || content.chars().count()>8192 {
                return Err("Provide a short name and 1–8,192 characters of replacement text".into());
            }
            let mut rows = snippets(None)?.as_array().cloned().unwrap_or_default();
            rows.retain(|row| row["name"] != name);
            rows.push(json!({"name":name,"text":content}));
            if rows.len()>100 { return Err("Keep no more than 100 replacements".into()); }
            Ok(json!({"snippets":snippets(Some(&json!(rows)))?}))
        }
        "delete" => {
            let mut rows = snippets(None)?.as_array().cloned().unwrap_or_default();
            rows.retain(|row| row["name"] != data["name"]);
            Ok(json!({"snippets":snippets(Some(&json!(rows)))?}))
        }
        "menu" => menu(required(data,"path")?, None),
        "stop" => { stop("lumen-text-speech.service"); Ok(json!({"ok":true})) },
        "lookup" => {
            let word = text(data)?.trim();
            if !valid_lookup(word) { return Err("Look up one English word or short phrase".into()); }
            let result = output("wn", &[word,"-over"], 6)?;
            let definition = result.chars().take(24000).collect::<String>();
            Ok(json!({"definition":if definition.trim().is_empty() { "No entry in the local English dictionary" } else { definition.trim() }}))
        }
        "speak" => {
            let content = text(data)?; stop("lumen-text-speech.service");
            let speech_language = language(data["language"].as_str())?;
            let unique = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
            let path = runtime()?.join(format!("lumen-speech-{}-{unique}.txt",std::process::id()));
            OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).map_err(|e| e.to_string())?
                .write_all(content.as_bytes()).map_err(|e| e.to_string())?;
            let result = output("systemd-run", &["--user","--collect","--quiet","--unit=lumen-text-speech",
                "--property=RuntimeMaxSec=600",&format!("--setenv=PATH={}",env::var("PATH").unwrap_or_default()),
                env::current_exe().map_err(|e| e.to_string())?.to_str().ok_or("Invalid executable path")?,"speech-worker",
                path.to_str().ok_or("Invalid speech path")?,speech_language], 5);
            if result.is_err() { let _ = fs::remove_file(path); }
            result?; Ok(json!({"ok":true}))
        }
        "insert" => {
            let content = text(data)?; let target = data["target"].as_i64().ok_or("Invalid target")?;
            let deadline = Instant::now()+Duration::from_millis(600);
            while Instant::now()<deadline {
                let tree: Value = serde_json::from_str(&output("swaymsg", &["-r","-t","get_tree"], 2)?).map_err(|e| e.to_string())?;
                if let Some(id) = focused(&tree) {
                    if id != target { return Err("The original text field is no longer focused; nothing was inserted".into()); }
                    command("wtype", &["--",content], None, Duration::from_secs(15))?;
                    return Ok(json!({"ok":true}));
                }
                thread::sleep(Duration::from_millis(30));
            }
            Err("The original window did not regain focus".into())
        }
        _ => Err("Unknown text action".into()),
    }
}
fn main() {
    unsafe { libc::umask(0o077); }
    let args: Vec<String> = env::args().collect();
    let result: Result<Option<Value>> = match args.get(1).map(String::as_str) {
        Some("speech-worker") => {
            unsafe {
                libc::signal(libc::SIGTERM, signal_stop as *const () as libc::sighandler_t);
                libc::signal(libc::SIGINT, signal_stop as *const () as libc::sighandler_t);
            }
            let path = args.get(2).ok_or("Missing speech file".to_owned()).map(PathBuf::from);
            path.and_then(|path| {
                let voice = if language(args.get(3).map(String::as_str))? == "en_GB" { "en-gb" } else { "en-us" };
                let result = output("espeak-ng", &["-v",voice,"-f",path.to_str().ok_or("Invalid path")?], 600);
                let _=fs::remove_file(path); result.map(|_| None)
            })
        }
        Some("slot") => args.get(2).ok_or("Missing slot".to_owned()).and_then(|slot| {
            let settings=settings()?; let item=&settings["menuShortcuts"][slot];
            if item.is_null() { Ok(None) } else { menu(required(item,"path")?,Some(required(item,"app")?)).map(|_|None) }
        }),
        Some("menu") => args.get(2).ok_or("Missing menu path".to_owned()).and_then(|path| menu(path,None).map(|_|None)),
        _ => {
            let mut input=String::new();
            std::io::stdin().read_to_string(&mut input).map_err(|e|e.to_string())
                .and_then(|_|serde_json::from_str::<Value>(&input).map_err(|e|e.to_string()))
                .and_then(|data|handle(&data)).map(Some)
        }
    };
    match result { Ok(Some(value))=>println!("{value}"), Ok(None)=>(), Err(error)=>{
        if args.len()>1 { eprintln!("{error}"); std::process::exit(1); }
        println!("{}",json!({"error":error}));
    }}
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn menu_labels_preserve_escaped_underscores() {
        assert_eq!(clean_label("_File"), "File");
        assert_eq!(clean_label("Save__As"), "Save_As");
    }
    #[test] fn lookup_rejects_command_payloads() {
        assert!(valid_lookup("don't know"));
        assert!(!valid_lookup("x; rm -rf /"));
    }
    #[test] fn language_choices_match_desktop_settings() {
        assert_eq!(language(None).unwrap(), "en_GB");
        assert_eq!(language(Some("en_US")).unwrap(), "en_US");
        assert!(language(Some("fr_FR")).is_err());
    }
    #[test] fn shortcuts_reject_invalid_slots_before_settings_write() {
        assert_eq!(handle(&json!({"action":"save-shortcut","slot":0,"app":"Fixture","path":"File/Save"})).unwrap_err(),
            "Choose a slot, application and exact menu path such as File/Save");
    }
    #[test] fn snippet_storage_preserves_literal_text() {
        let root = env::temp_dir().join(format!("text-tools-test-{}",std::process::id()));
        fs::create_dir(&root).unwrap();
        let previous = env::var_os("XDG_STATE_HOME");
        unsafe { env::set_var("XDG_STATE_HOME", &root); }
        let payload = "Literal $HOME `command`\nSecond line";
        assert_eq!(handle(&json!({"action":"save","name":";test","text":payload})).unwrap()["snippets"][0]["text"], payload);
        assert_eq!(handle(&json!({"action":"delete","name":";test"})).unwrap()["snippets"], json!([]));
        if let Some(value)=previous { unsafe { env::set_var("XDG_STATE_HOME", value); } }
        else { unsafe { env::remove_var("XDG_STATE_HOME"); } }
        fs::remove_dir_all(root).unwrap();
    }
}
