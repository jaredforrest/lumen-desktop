use chrono::Utc;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    env,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;

fn paths() -> (PathBuf, PathBuf) {
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    let root = data.join("calendar");
    let settings = root.join("icloud-settings");
    (root, settings)
}

fn private_write(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().ok_or("Invalid private file path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .ok_or("Invalid private file name")?
        .to_string_lossy();
    let mut temp = None;
    for attempt in 0..100 {
        let candidate = parent.join(format!(".{name}.{}.{}", std::process::id(), attempt));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&candidate)
        {
            Ok(file) => {
                temp = Some((candidate, file));
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    let (candidate, mut file) = temp.ok_or("Could not create a private temporary file")?;
    let outcome = (|| -> io::Result<()> {
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        fs::rename(&candidate, path)?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&candidate);
    }
    outcome.map_err(|e| e.to_string())
}

fn configuration(root: &Path, settings: &Path, username: &str, secret_tool: &str) -> String {
    let mut output = format!(
        "[general]\nstatus_path = {}\n",
        json!(settings.join("sync-state").to_string_lossy().to_string())
    );
    let contacts = root.parent().unwrap_or(root).join("contacts/icloud");
    for (kind, storage, url, suffix, target) in [
        (
            "calendar",
            "caldav",
            "https://caldav.icloud.com/",
            ".ics",
            root.join("icloud"),
        ),
        (
            "contacts",
            "carddav",
            "https://contacts.icloud.com/",
            ".vcf",
            contacts,
        ),
    ] {
        let fetch = json!([
            "command",
            secret_tool,
            "lookup",
            "service",
            "lumen-desktop-icloud",
            "account",
            username
        ]);
        output.push_str(&format!("\n[pair icloud_{kind}]\na = \"icloud_{kind}_local\"\nb = \"icloud_{kind}_remote\"\ncollections = [\"from b\"]\nconflict_resolution = \"b wins\"\nmetadata = [\"displayname\"]\n\n[storage icloud_{kind}_local]\ntype = \"filesystem\"\npath = {}\nfileext = \"{suffix}\"\n\n[storage icloud_{kind}_remote]\ntype = \"{storage}\"\nurl = \"{url}\"\nusername = {}\npassword.fetch = {fetch}\nread_only = true\n", json!(target.to_string_lossy().to_string()), json!(username)));
    }
    output
}

fn status(settings: &Path) -> Value {
    let saved: Value = fs::read_to_string(settings.join("status.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    json!({"configured": settings.join("vdirsyncer.conf").is_file(), "lastSuccess": saved.get("lastSuccess").and_then(Value::as_str).unwrap_or(""), "error": saved.get("error").and_then(Value::as_str).unwrap_or("")})
}

fn sync_failure(diagnostic: &str) -> &'static str {
    let lower = diagnostic.to_ascii_lowercase();
    if lower.contains("401") || lower.contains("unauthorized") {
        "iCloud rejected the account or password (401 Unauthorized). Use a Lumen app-specific password, not your usual Lumen Account password. Check the account email and update the saved app-specific password in Account Setup."
    } else if lower.contains("403") || lower.contains("forbidden") {
        "iCloud refused access (403 Forbidden). Check account access and the app-specific password in Account Setup."
    } else if lower.contains("certificate") {
        "The secure connection to iCloud could not be verified. Check the system clock and network connection."
    } else if lower.contains("aborted") || lower.contains("eoferror") {
        "Calendar discovery was not completed. Open Account Setup and finish its local collection prompts."
    } else {
        "iCloud sync failed. Check your network and account setup, then retry."
    }
}

fn parse_ini(input: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections = HashMap::<String, HashMap<String, String>>::new();
    let mut current = String::new();
    for line in input.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current = name.to_string();
            sections.entry(current.clone()).or_default();
        } else if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    sections
}

fn validate_local_discovery(config: &Path) -> Result<()> {
    let invalid = "Automatic discovery requires the original read-only iCloud mirror configuration. Custom storage settings need manual discovery.";
    let content = fs::read_to_string(config).map_err(|_| invalid)?;
    let sections = parse_ini(&content);
    let pairs: Vec<_> = sections
        .keys()
        .filter(|s| s.starts_with("pair "))
        .map(String::as_str)
        .collect();
    if pairs.len() != 2
        || !pairs.contains(&"pair icloud_calendar")
        || !pairs.contains(&"pair icloud_contacts")
    {
        return Err(invalid.into());
    }
    let root = config.parent().and_then(Path::parent).ok_or(invalid)?;
    for (kind, storage, url, target) in [
        (
            "calendar",
            "caldav",
            "https://caldav.icloud.com/",
            root.join("icloud"),
        ),
        (
            "contacts",
            "carddav",
            "https://contacts.icloud.com/",
            root.parent().ok_or(invalid)?.join("contacts/icloud"),
        ),
    ] {
        let pair = sections
            .get(&format!("pair icloud_{kind}"))
            .ok_or(invalid)?;
        let local = sections
            .get(&format!("storage icloud_{kind}_local"))
            .ok_or(invalid)?;
        let remote = sections
            .get(&format!("storage icloud_{kind}_remote"))
            .ok_or(invalid)?;
        let equal = |map: &HashMap<String, String>, key: &str, expected: Value| -> bool {
            map.get(key)
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                == Some(expected)
        };
        if !equal(pair, "a", json!(format!("icloud_{kind}_local")))
            || !equal(pair, "b", json!(format!("icloud_{kind}_remote")))
            || !equal(pair, "collections", json!(["from b"]))
            || !equal(pair, "conflict_resolution", json!("b wins"))
            || !equal(local, "type", json!("filesystem"))
            || !equal(local, "path", json!(target.to_string_lossy().to_string()))
            || !equal(remote, "type", json!(storage))
            || !equal(remote, "url", json!(url))
            || !equal(remote, "read_only", json!(true))
            || target.is_symlink()
        {
            return Err(invalid.into());
        }
    }
    Ok(())
}

fn run_command(command: &mut Command, timeout: Duration, discovery: bool) -> Result<()> {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    if discovery {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let stderr_reader = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut diagnostic = String::new();
            let mut chunk = [0u8; 4096];
            while let Ok(bytes) = stderr.read(&mut chunk) {
                if bytes == 0 {
                    break;
                }
                if diagnostic.len() < 65536 {
                    diagnostic.push_str(&String::from_utf8_lossy(&chunk[..bytes]));
                }
            }
            diagnostic
        })
    });
    if discovery {
        if let Some(mut stdin) = child.stdin.take() {
            thread::spawn(move || {
                let _ = stdin.write_all("y\n".repeat(4096).as_bytes());
            });
        }
    }
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let diagnostic = stderr_reader
                    .and_then(|reader| reader.join().ok())
                    .unwrap_or_default();
                return if status.success() {
                    Ok(())
                } else {
                    Err(sync_failure(&diagnostic).into())
                };
            }
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "iCloud did not respond in time. Check the connection and retry.".into(),
                );
            }
            Err(_) => return Err(sync_failure("").into()),
        }
    }
}

fn run_sync(settings: &Path, discover: bool, quiet: bool) -> Result<()> {
    let config = settings.join("vdirsyncer.conf");
    if !config.is_file() {
        return Err("iCloud is not configured. Run lumen-icloud-setup setup in a terminal.".into());
    }
    fs::create_dir_all(settings).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .open(settings.join("sync.lock"))
        .map_err(|e| e.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("An iCloud sync is already running".into());
    }
    let mut result = status(settings);
    let outcome = (|| -> Result<()> {
        if discover {
            validate_local_discovery(&config)?;
            if !quiet { println!("Preparing local calendar and contact folders. Nothing will be created in iCloud."); }
            let mut command = Command::new("vdirsyncer");
            command.args(["--config", &config.to_string_lossy(), "discover"]);
            run_command(&mut command, Duration::from_secs(120), true)?;
        }
        for (action, seconds) in [("sync", 75), ("metasync", 15)] {
            let mut command = Command::new("vdirsyncer");
            command.args(["--config", &config.to_string_lossy(), action]);
            run_command(&mut command, Duration::from_secs(seconds), false)?;
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => {
            result["lastSuccess"] =
                json!(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false));
            result["error"] = json!("");
            private_write(&settings.join("status.json"), &result.to_string())?;
            if !quiet { println!("iCloud calendars and contacts downloaded. Remote data is read-only."); }
            Ok(())
        }
        Err(error) => {
            result["error"] = json!(error);
            private_write(&settings.join("status.json"), &result.to_string())?;
            Err(error)
        }
    }
}

fn secret_tool() -> Result<PathBuf> {
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let candidate = directory.join("secret-tool");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("secret-tool is unavailable. Install the desktop integration before setup.".into())
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    Ok(answer.trim().to_string())
}

fn hidden_prompt(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let fd = libc::STDIN_FILENO;
    let mut old = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(fd, old.as_mut_ptr()) } != 0 {
        return Err("A local terminal is required".into());
    }
    let old = unsafe { old.assume_init() };
    let mut hidden = old;
    hidden.c_lflag &= !libc::ECHO;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &hidden) } != 0 {
        return Err("Could not hide password input".into());
    }
    let answer = prompt("");
    unsafe {
        libc::tcsetattr(fd, libc::TCSANOW, &old);
    }
    println!();
    answer
}

fn save_password_with(username: &str, password: &str, quiet: bool) -> Result<()> {
    let tool = secret_tool()?;
    if password.is_empty() || password.contains(['\n', '\r']) {
        return Err("An app-specific password is required".into());
    }
    let mut child = Command::new(tool)
        .args([
            "store",
            "--label=Lumen Desktop iCloud",
            "service",
            "lumen-desktop-icloud",
            "account",
            username,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not save the app password in the unlocked desktop keyring")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(password.as_bytes())
            .map_err(|_| "Could not save the app password in the unlocked desktop keyring")?;
    }
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                if !quiet { println!("Password received and saved to the desktop keyring. iCloud sign-in still needs to be verified."); }
                return Ok(());
            }
            Ok(Some(_)) | Err(_) => {
                return Err(
                    "Could not save the app password in the unlocked desktop keyring".into(),
                );
            }
            Ok(None) if start.elapsed() < Duration::from_secs(60) => {
                thread::sleep(Duration::from_millis(50))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "Could not save the app password in the unlocked desktop keyring".into(),
                );
            }
        }
    }
}

fn save_password(username: &str) -> Result<()> {
    println!("Copy a Lumen app-specific password from Bitwarden, then paste with Ctrl+Shift+V.\nThe password prompt shows no characters or dots. Paste once, then press Enter.");
    let password = hidden_prompt("App-specific password (hidden): ")?;
    save_password_with(username, &password, false)
}

fn setup_gui(root: &Path, settings: &Path, request: &Value) -> Result<()> {
    let username = request.get("email").and_then(Value::as_str).ok_or("Enter a valid account email and app-specific password.")?;
    let password = request.get("password").and_then(Value::as_str).ok_or("Enter a valid account email and app-specific password.")?;
    if username.len() > 254 || username.is_empty() || username.bytes().any(|b| b.is_ascii_whitespace())
        || username.split('@').count() != 2 || username.starts_with('@') || username.ends_with('@')
        || password.trim().is_empty() || password.len() > 1024 || password.contains(['\n','\r']) {
        return Err("Enter a valid account email and app-specific password.".into());
    }
    let config = settings.join("vdirsyncer.conf");
    if config.exists() {
        let ini = parse_ini(&fs::read_to_string(&config).map_err(|_| "The existing account configuration needs review before changing it.")?);
        let existing = ini.get("storage icloud_calendar_remote")
            .and_then(|section| section.get("username"))
            .and_then(|raw| serde_json::from_str::<String>(raw).ok())
            .ok_or("The existing account configuration needs review before changing it.")?;
        if existing != username {
            return Err("Use the existing iCloud account email to update its password. Switching accounts requires a separate migration.".into());
        }
        validate_local_discovery(&config)?;
    }
    let tool = secret_tool().map_err(|_| "The desktop keyring is unavailable.")?;
    save_password_with(username, password.trim(), true)?;
    if !config.exists() {
        private_write(&config, &configuration(root,settings,username,&tool.to_string_lossy()))?;
    }
    run_sync(settings,true,true)
}

fn setup(root: &Path, settings: &Path) -> Result<()> {
    if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
        return Err("Account setup requires a local interactive terminal; never send passwords through chat.".into());
    }
    let config = settings.join("vdirsyncer.conf");
    if config.exists() {
        println!("Existing iCloud configuration retained.");
        if prompt("Update the saved app-specific password? [y/N]: ")?.eq_ignore_ascii_case("y") {
            let ini = parse_ini(&fs::read_to_string(&config).map_err(|e| e.to_string())?);
            let raw = ini
                .get("storage icloud_calendar_remote")
                .and_then(|s| s.get("username"))
                .ok_or("Saved account is unreadable")?;
            let username: String =
                serde_json::from_str(raw).map_err(|_| "Saved account is unreadable")?;
            save_password(&username)?;
        }
        println!("Finishing discovery and syncing read-only mirrors.");
        return run_sync(settings, true, false);
    }
    let tool = secret_tool()?;
    println!(
        "Use an app-specific password from your cloud account sign-in and security settings.\nOnly local read-only calendar and contact mirrors will be created. No remote events or contacts are modified."
    );
    let username = prompt("Lumen Account email: ")?;
    if username.is_empty()
        || username.len() > 254
        || username.contains(['\n', '\r'])
        || !username.contains('@')
    {
        return Err("Enter your Lumen Account email address".into());
    }
    save_password(&username)?;
    private_write(
        &config,
        &configuration(root, settings, &username, &tool.to_string_lossy()),
    )?;
    run_sync(settings, true, false)
}

fn main() {
    unsafe {
        libc::umask(0o077);
    }
    let (root, settings) = paths();
    let action = env::args().nth(1).unwrap_or_else(|| "setup".into());
    let outcome = match action.as_str() {
        "status" => {
            println!("{}", status(&settings));
            Ok(())
        }
        "sync" => run_sync(&settings, false, false),
        "setup" => setup(&root, &settings),
        "setup-json" => {
            let mut input = String::new();
            let read = io::stdin().take(8193).read_to_string(&mut input);
            let result = if read.is_err() || input.len() > 8192 {
                Err("Account setup request could not be read.".to_owned())
            } else {
                serde_json::from_str::<Value>(&input)
                    .map_err(|_| "Account setup request could not be read.".to_owned())
                    .and_then(|request| setup_gui(&root,&settings,&request))
            };
            match result {
                Ok(()) => println!("{}",json!({"success":true})),
                Err(error) => { println!("{}",json!({"error":error}));std::process::exit(1); }
            }
            Ok(())
        }
        _ => Err("Usage: lumen-icloud-setup [setup|setup-json|sync|status]".into()),
    };
    if let Err(error) = outcome {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn read_only_discovery_policy() {
        let root = env::temp_dir().join(format!("icloud-policy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let settings = root.join("icloud-settings");
        let config = settings.join("vdirsyncer.conf");
        let original = configuration(&root, &settings, "me@example.org", "/bin/secret-tool");
        assert_eq!(original.matches("read_only = true").count(), 2);
        assert_eq!(original.matches("collections = [\"from b\"]").count(), 2);
        assert!(original.contains("password.fetch") && !original.contains("password ="));
        private_write(&config, &original).unwrap();
        assert_eq!(
            fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o600
        );
        validate_local_discovery(&config).unwrap();
        for changed in [
            original.replace("read_only = true", "read_only = false"),
            original.replace("[\"from b\"]", "[\"from a\"]"),
            original.replace("type = \"filesystem\"", "type = \"caldav\""),
            original.replace(
                &root.join("icloud").to_string_lossy().to_string(),
                "/different/location",
            ),
            format!("{original}\n[pair extra]\na=\"unsafe\"\n"),
        ] {
            private_write(&config, &changed).unwrap();
            assert!(validate_local_discovery(&config).is_err());
        }
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn errors_do_not_expose_diagnostics() {
        assert!(sync_failure("401 account@example.org secret").contains("401 Unauthorized"));
        assert!(!sync_failure("account@example.org secret").contains("example.org"));
    }
    #[test]
    fn malformed_gui_request_does_not_create_settings() {
        let root=env::temp_dir().join(format!("lumen-icloud-invalid-{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        let settings=root.join("icloud-settings");
        for request in [json!({"email":"bad","password":"pw"}),json!({"email":"a@b","password":"\nsecret"}),json!([]),json!({"email":"a@b","password":""})] {
            assert!(setup_gui(&root,&settings,&request).is_err());
            assert!(!settings.exists());
        }
    }
    #[test]
    fn existing_gui_account_cannot_silently_switch() {
        let root=env::temp_dir().join(format!("lumen-icloud-account-{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        let settings=root.join("icloud-settings");
        let config=settings.join("vdirsyncer.conf");
        let original=configuration(&root,&settings,"one@example.test","/fixture/secret-tool");
        private_write(&config,&original).unwrap();
        let result=setup_gui(&root,&settings,&json!({"email":"two@example.test","password":"fixture"}));
        assert!(result.unwrap_err().contains("existing iCloud account"));
        assert_eq!(fs::read_to_string(&config).unwrap(),original);
        fs::remove_dir_all(root).unwrap();
    }
}
