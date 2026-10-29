//! Keep native working state private while Nix remains the preference authority.

use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, symlink};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type ProfileResult<T> = Result<T, String>;
static CHILD_PID: AtomicI32 = AtomicI32::new(0);

fn object(value: &Value) -> ProfileResult<&Map<String, Value>> {
    value
        .as_object()
        .ok_or("Native profile must be a JSON object".into())
}

fn atomic(path: &Path, value: &Value) -> ProfileResult<()> {
    let parent = path.parent().ok_or("File has no parent")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| error.to_string())?;
    let name = format!(
        ".profile-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    );
    let temporary = parent.join(name);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut file, value).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn load(path: &Path) -> ProfileResult<Value> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(error) => return Err(error.to_string()),
    };
    let value: Value = serde_json::from_slice(&data).map_err(|error| error.to_string())?;
    object(&value)?;
    Ok(value)
}

fn merge(values: &[&Value]) -> ProfileResult<Value> {
    let mut result = Map::new();
    for value in values {
        for (key, item) in object(value)? {
            if item.is_object() && result.get(key).is_some_and(Value::is_object) {
                result.insert(key.clone(), merge(&[&result[key], item])?);
            } else {
                result.insert(key.clone(), item.clone());
            }
        }
    }
    Ok(Value::Object(result))
}

fn partition(
    value: &Value,
    policy: &Value,
    fields: &Value,
    prefix: &str,
) -> ProfileResult<(Value, Value)> {
    let value = object(value)?;
    let policy = object(policy)?;
    let fields = object(fields)?;
    let unknown: BTreeSet<String> = value
        .keys()
        .filter(|key| !policy.contains_key(*key) && !fields.contains_key(*key))
        .map(|key| format!("{prefix}{key}"))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "Unclassified native profile fields: {}",
            unknown.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    let mut proposed = Map::new();
    let mut state = Map::new();
    for (key, default) in policy {
        let item = value.get(key).unwrap_or(default);
        if default.is_object() {
            if !item.is_object() {
                return Err(format!("Invalid native object: {prefix}{key}"));
            }
            let empty = json!({});
            let (preference, retained) = partition(
                item,
                default,
                fields.get(key).unwrap_or(&empty),
                &format!("{prefix}{key}."),
            )?;
            proposed.insert(key.clone(), preference);
            if retained
                .as_object()
                .is_some_and(|object| !object.is_empty())
            {
                state.insert(key.clone(), retained);
            }
        } else {
            proposed.insert(key.clone(), item.clone());
        }
    }
    for (key, default) in fields {
        if policy.contains_key(key) {
            continue;
        }
        let Some(item) = value.get(key) else {
            continue;
        };
        if default.is_object() {
            if !item.is_object() {
                return Err(format!("Invalid state object: {prefix}{key}"));
            }
            let (_, retained) = partition(item, &json!({}), default, &format!("{prefix}{key}."))?;
            state.insert(key.clone(), retained);
        } else {
            state.insert(key.clone(), item.clone());
        }
    }
    Ok((Value::Object(proposed), Value::Object(state)))
}

fn ini_read(path: &Path) -> ProfileResult<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut section = String::new();
    let mut sections = Vec::new();
    let mut fields = Map::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(['#', ';']) {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed[1..trimmed.len() - 1].to_owned();
            sections.push(section.clone());
            continue;
        }
        if section == "General" {
            let (key, value) = trimmed
                .split_once(['=', ':'])
                .ok_or("Invalid Prism INI entry")?;
            let key = key.trim();
            if fields.contains_key(key) {
                return Err("Duplicate Prism INI field".into());
            }
            fields.insert(key.to_owned(), Value::String(value.trim().to_owned()));
        }
    }
    if sections != ["General"] {
        return Err("Unclassified Prism INI section".into());
    }
    Ok(Value::Object(fields))
}

fn ini_write(path: &Path, value: &Value) -> ProfileResult<()> {
    let parent = path.parent().ok_or("Prism file has no parent")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(
        ".prism-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(b"[General]\n")
            .map_err(|error| error.to_string())?;
        for (key, item) in object(value)? {
            let item = item.as_str().ok_or("Prism value must be a string")?;
            if key.contains(['\n', '\r', '=', ':']) || item.contains(['\n', '\r']) {
                return Err("Invalid Prism INI value".into());
            }
            writeln!(file, "{key} = {item}").map_err(|error| error.to_string())?;
        }
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}

struct Bridge {
    app: String,
    policies: Value,
    fields: Value,
    cache_fields: Value,
    profile: PathBuf,
    state: PathBuf,
    cache: PathBuf,
}

impl Bridge {
    fn new(
        app: &str,
        policies: Value,
        fields: Value,
        profile: PathBuf,
        state_root: PathBuf,
        cache_root: PathBuf,
        cache_fields: Value,
    ) -> Self {
        Self {
            app: app.into(),
            policies,
            fields,
            cache_fields,
            profile,
            state: state_root.join(app),
            cache: cache_root.join(app),
        }
    }

    fn read_native(&self, path: &Path) -> ProfileResult<Value> {
        if self.app == "prism" {
            ini_read(path)
        } else {
            load(path)
        }
    }

    fn cache_defaults(&self, name: &str) -> Value {
        self.cache_fields
            .get(name)
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}))
    }

    fn write_native(&self, path: &Path, value: &Value) -> ProfileResult<()> {
        if self.app == "prism" {
            ini_write(path, value)
        } else {
            atomic(path, value)
        }
    }

    fn split(&self, name: &str, value: &Value) -> ProfileResult<(Value, Value, Value)> {
        let mut defaults = object(&self.fields[name])?.clone();
        defaults.extend(object(&self.cache_defaults(name))?.clone());
        let (proposed, working) =
            partition(value, &self.policies[name], &Value::Object(defaults), "")?;
        let mut state = Map::new();
        let mut cache = Map::new();
        for (key, value) in object(&working)? {
            if self.fields[name].get(key).is_some() {
                state.insert(key.clone(), value.clone());
            }
            if self.cache_defaults(name).get(key).is_some() {
                cache.insert(key.clone(), value.clone());
            }
        }
        Ok((proposed, Value::Object(state), Value::Object(cache)))
    }

    fn link_projection(&self, name: &str) -> ProfileResult<()> {
        let path = self.profile.join(name);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.profile)
            .map_err(|error| error.to_string())?;
        let temporary = self.profile.join(format!(".{name}.projection"));
        if temporary.exists() || temporary.is_symlink() {
            fs::remove_file(&temporary).map_err(|error| error.to_string())?;
        }
        symlink(self.cache.join(name), &temporary).map_err(|error| error.to_string())?;
        fs::rename(temporary, path).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn prepare(&self) -> ProfileResult<()> {
        let mut records = Map::new();
        let mut derived = Map::new();
        for name in object(&self.policies)?.keys() {
            let prior = self.read_native(&self.profile.join(name))?;
            let (_, native_state, native_cache) = self.split(name, &prior)?;
            let retained = load(&self.state.join(name))?;
            partition(&retained, &json!({}), &self.fields[name], "")?;
            records.insert(
                name.clone(),
                merge(&[&self.fields[name], &retained, &native_state])?,
            );
            derived.insert(
                name.clone(),
                merge(&[
                    &self.cache_defaults(name),
                    &load(&self.cache.join(format!("{name}.derived")))?,
                    &native_cache,
                ])?,
            );
        }
        for (name, policy) in object(&self.policies)? {
            atomic(&self.state.join(name), &records[name])?;
            atomic(&self.cache.join(format!("{name}.derived")), &derived[name])?;
            self.write_native(
                &self.cache.join(name),
                &merge(&[&derived[name], &records[name], policy])?,
            )?;
            self.link_projection(name)?;
        }
        Ok(())
    }

    fn collect(&self) -> ProfileResult<Value> {
        let mut proposed = Map::new();
        let mut records = Map::new();
        let mut derived = Map::new();
        for (name, policy) in object(&self.policies)? {
            let path = self.profile.join(name);
            if !path.exists() {
                proposed.insert(name.clone(), policy.clone());
                continue;
            }
            let native = self.read_native(&path)?;
            match self.split(name, &native) {
                Ok((preferences, state, cache)) => {
                    proposed.insert(name.clone(), preferences);
                    records.insert(name.clone(), state);
                    derived.insert(name.clone(), cache);
                }
                Err(error) => {
                    atomic(
                        &self.state.join("unclassified-profile-draft.json"),
                        &json!({"file":name,"status":"needs-classification","native":native}),
                    )?;
                    let known: Map<String, Value> = object(&native)?
                        .iter()
                        .filter(|(key, _)| self.fields[name].get(*key).is_some())
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect();
                    atomic(&self.state.join(name), &Value::Object(known))?;
                    return Err(error);
                }
            }
        }
        for (name, state) in records {
            atomic(&self.state.join(name), &state)?;
        }
        for (name, cache) in derived {
            atomic(&self.cache.join(format!("{name}.derived")), &cache)?;
        }
        Ok(Value::Object(proposed))
    }

    fn project(&self, policies: &Value) -> ProfileResult<()> {
        for (name, policy) in object(policies)? {
            self.write_native(
                &self.cache.join(name),
                &merge(&[
                    &self.cache_defaults(name),
                    &load(&self.cache.join(format!("{name}.derived")))?,
                    &load(&self.state.join(name))?,
                    policy,
                ])?,
            )?;
            self.link_projection(name)?;
        }
        Ok(())
    }
}

fn home() -> ProfileResult<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is missing".into())
}

fn xdg(name: &str, fallback: &str) -> ProfileResult<PathBuf> {
    Ok(env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or(home()?.join(fallback)))
}

fn runtime() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })))
}

fn lock(path: &Path) -> ProfileResult<File> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(file)
}

fn alive(pid: i32) -> bool {
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

fn running(registry: &Path, exclude: Option<i32>) -> ProfileResult<Vec<i32>> {
    Ok(load(registry)?["pids"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_i64().and_then(|n| i32::try_from(n).ok()))
                .filter(|pid| Some(*pid) != exclude && alive(*pid))
                .collect()
        })
        .unwrap_or_default())
}

extern "C" fn forward(signal: i32) {
    let pid = CHILD_PID.load(Ordering::Relaxed);
    if pid > 0 {
        unsafe {
            libc::kill(pid, signal);
        }
    }
}

fn stamp(bridge: &Bridge) -> ProfileResult<Vec<(String, Option<u128>, Option<u64>)>> {
    object(&bridge.policies)?
        .keys()
        .map(|name| {
            let path = bridge.profile.join(name);
            match fs::metadata(&path) {
                Ok(metadata) => Ok((
                    name.clone(),
                    Some(
                        metadata
                            .modified()
                            .map_err(|error| error.to_string())?
                            .duration_since(UNIX_EPOCH)
                            .map_err(|error| error.to_string())?
                            .as_nanos(),
                    ),
                    Some(metadata.len()),
                )),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok((name.clone(), None, None))
                }
                Err(error) => Err(error.to_string()),
            }
        })
        .collect()
}

fn alert(title: &str, message: &str) {
    let _ = Command::new("zenity")
        .args([
            "--error",
            &format!("--title={title}"),
            &format!("--text={message}"),
        ])
        .status();
}

fn active_policies(app: &str) -> ProfileResult<Value> {
    let path = env::var_os("LUMEN_PROFILE_ACTIVE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/nix/desktop-settings.json"));
    let compiled = load(&path)?;
    let policy = &compiled["applicationPreferences"][app];
    object(policy)?;
    Ok(policy.clone())
}

fn process_profile(
    bridge: &Bridge,
    registry: &Path,
    lockfile: &Path,
    binary: &str,
    args: &[String],
) -> ProfileResult<i32> {
    let _guard = lock(lockfile)?;
    let prior = running(registry, None)?;
    if prior.is_empty() {
        bridge.prepare()?;
    }
    let mut child = Command::new(binary)
        .args(args)
        .spawn()
        .map_err(|error| error.to_string())?;
    let pid = i32::try_from(child.id()).map_err(|error| error.to_string())?;
    let mut pids = prior;
    pids.push(pid);
    atomic(registry, &json!({"pids":pids}))?;
    drop(_guard);
    CHILD_PID.store(pid, Ordering::Relaxed);
    unsafe {
        libc::signal(libc::SIGTERM, forward as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, forward as *const () as libc::sighandler_t);
    }
    let mut previous = None;
    while child
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_none()
    {
        let current = stamp(bridge)?;
        if previous.as_ref() != Some(&current) {
            match bridge.collect() {
                Ok(_) => previous = Some(current),
                Err(error) if error.contains("Unclassified") => previous = Some(current),
                Err(_) => (), // The native application may still be writing JSON.
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    CHILD_PID.store(0, Ordering::Relaxed);
    let status = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1));
    let _guard = lock(lockfile)?;
    let remaining = running(registry, Some(pid))?;
    atomic(registry, &json!({"pids":remaining}))?;
    let proposed = bridge.collect()?;
    if !remaining.is_empty() {
        return Ok(status);
    }
    if bridge.app == "prism" {
        let changed = &proposed["prismlauncher.cfg"];
        let original = &bridge.policies["prismlauncher.cfg"];
        for key in [
            "Env",
            "JvmArgs",
            "PreLaunchCommand",
            "PostExitCommand",
            "WrapperCommand",
        ] {
            if changed[key] != original[key] {
                atomic(
                    &bridge.state.join("private-preference-proposal.json"),
                    &json!({"status":"needs-private-review","preferences":proposed}),
                )?;
                bridge.project(&bridge.policies)?;
                alert(
                    "Private configuration needs review",
                    "Command and environment preferences may contain secrets. They were not copied into the public Nix store. State and credentials were preserved. A private draft was saved for review; these fields need secret handling before they can be applied.",
                );
                return Ok(status);
            }
        }
    }
    let current = active_policies(&bridge.app)?;
    if proposed != bridge.policies && current != bridge.policies {
        atomic(
            &bridge.state.join("preference-proposal.json"),
            &json!({"status":"configuration-conflict","preferences":proposed}),
        )?;
        bridge.project(&current)?;
        alert(
            "Preferences changed elsewhere",
            "Nix preferences changed while the app was open. Your working state and a private preference draft were saved; the newer Nix configuration was preserved.",
        );
        return Ok(status);
    }
    if proposed != bridge.policies {
        let question = format!(
            "Save the preference changes from {} to NixOS?\n\nThis rebuilds the system and requests authentication. Recent documents and window state are already saved. The preferences will take effect on the next launch.",
            bridge.app
        );
        let accepted = Command::new("zenity")
            .args([
                "--question",
                "--title=Apply NixOS preferences",
                &format!("--text={question}"),
            ])
            .status()
            .is_ok_and(|status| status.success());
        if accepted {
            let encoded = serde_json::to_string(&proposed).map_err(|error| error.to_string())?;
            let result = Command::new("lumen-nix-settings")
                .args(["set-application", &bridge.app, &encoded])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            if !result.is_ok_and(|output| output.status.success()) {
                alert(
                    "NixOS preferences not applied",
                    "The rebuild or authentication failed. Your working state was saved; previous configured preferences remain active.",
                );
            }
        }
    }
    bridge.project(&active_policies(&bridge.app)?)?;
    Ok(status)
}

fn windows(pid: u32) -> ProfileResult<Vec<i64>> {
    let output = Command::new("swaymsg")
        .args(["-r", "-t", "get_tree"])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("Sway tree query failed".into());
    }
    let tree: Value = serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    fn visit(node: &Value, pid: u32, ids: &mut Vec<i64>) {
        if node["pid"].as_u64() == Some(u64::from(pid))
            && ["con", "floating_con"].contains(&node["type"].as_str().unwrap_or(""))
        {
            if let Some(id) = node["id"].as_i64() {
                ids.push(id);
            }
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = node[key].as_array() {
                for child in children {
                    visit(child, pid, ids);
                }
            }
        }
    }
    let mut ids = Vec::new();
    visit(&tree, pid, &mut ids);
    Ok(ids)
}

fn smoke() -> ProfileResult<i32> {
    let policies = load(Path::new(&env::var("LUMEN_PROFILE_SMOKE_POLICIES")
        .map_err(|_| "LUMEN_PROFILE_SMOKE_POLICIES is missing")?))?;
    let fields = load(Path::new(&env::var("LUMEN_PROFILE_SMOKE_STATE_FIELDS")
        .map_err(|_| "LUMEN_PROFILE_SMOKE_STATE_FIELDS is missing")?))?;
    let cache_fields = load(Path::new(
        &env::var("LUMEN_PROFILE_CACHE_FIELDS")
            .map_err(|_| "LUMEN_PROFILE_CACHE_FIELDS is missing")?,
    ))?;
    let root = env::temp_dir().join(format!(
        "native-projection-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&root).map_err(|error| error.to_string())?;
    let result = (|| {
        let apps = env::var("PROFILE_SMOKE_APPS")
            .unwrap_or_else(|_| "wordcraft,gridcraft,deckcraft,prism".into());
        for app in apps.split(',') {
            if !["wordcraft", "gridcraft", "deckcraft", "prism"].contains(&app) {
                return Err(format!("Unsupported smoke application: {app}"));
            }
            let app_root = root.join(app);
            let profile = app_root.join("profile");
            let state = app_root.join("state");
            let cache = app_root.join("cache");
            let bridge = Bridge::new(
                app,
                policies[app].clone(),
                fields[app].clone(),
                profile.clone(),
                state.clone(),
                cache.clone(),
                cache_fields.get(app).cloned().unwrap_or_else(|| json!({})),
            );
            bridge.prepare()?;
            if app == "prism" {
                let real = ini_read(&xdg("XDG_DATA_HOME", ".local/share")?
                    .join("PrismLauncher/prismlauncher.cfg"))?;
                let mut detected = Map::new();
                for key in object(&cache_fields[app]["prismlauncher.cfg"])?.keys() {
                    detected.insert(
                        key.clone(),
                        real.get(key)
                            .cloned()
                            .unwrap_or(Value::String(String::new())),
                    );
                }
                atomic(
                    &bridge.cache.join("prismlauncher.cfg.derived"),
                    &Value::Object(detected),
                )?;
                bridge.project(&policies[app])?;
            }
            let config_home = app_root.join("config");
            let data_home = app_root.join("data");
            fs::create_dir_all(&config_home).map_err(|error| error.to_string())?;
            fs::create_dir_all(&data_home).map_err(|error| error.to_string())?;
            if app == "prism" {
                symlink(&profile, data_home.join("PrismLauncher"))
                    .map_err(|error| error.to_string())?;
            } else {
                symlink(&profile, config_home.join(app)).map_err(|error| error.to_string())?;
            }
            let binary = if app == "prism" { "prismlauncher" } else { app };
            let log =
                File::create(app_root.join("launch.log")).map_err(|error| error.to_string())?;
            let mut child = Command::new(binary)
                .env("XDG_CONFIG_HOME", &config_home)
                .env("XDG_CACHE_HOME", &cache)
                .env("XDG_STATE_HOME", &state)
                .env("XDG_DATA_HOME", &data_home)
                .stdout(Stdio::from(
                    log.try_clone().map_err(|error| error.to_string())?,
                ))
                .stderr(Stdio::from(log))
                .spawn()
                .map_err(|error| error.to_string())?;
            let session = (|| {
                let deadline = std::time::Instant::now() + Duration::from_secs(15);
                while windows(child.id())?.is_empty() {
                    if child
                        .try_wait()
                        .map_err(|error| error.to_string())?
                        .is_some()
                    {
                        return Err(format!("{app} failed to open an isolated window"));
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(format!("{app} window timeout"));
                    }
                    thread::sleep(Duration::from_millis(200));
                }
                thread::sleep(Duration::from_secs(2));
                for id in windows(child.id())? {
                    let status = Command::new("swaymsg")
                        .args([format!("[con_id={id}]"), "kill".into()])
                        .status()
                        .map_err(|error| error.to_string())?;
                    if !status.success() {
                        return Err("Sway could not close the test window".into());
                    }
                }
                let deadline = std::time::Instant::now() + Duration::from_secs(12);
                while child
                    .try_wait()
                    .map_err(|error| error.to_string())?
                    .is_none()
                    && std::time::Instant::now() < deadline
                {
                    for id in windows(child.id())? {
                        let _ = Command::new("swaymsg")
                            .args([format!("[con_id={id}]"), "focus".into()])
                            .status();
                        let _ = Command::new("wtype")
                            .args(["-M", "ctrl", "-k", "q", "-m", "ctrl"])
                            .status();
                    }
                    thread::sleep(Duration::from_millis(400));
                }
                let status = child
                    .try_wait()
                    .map_err(|error| error.to_string())?
                    .ok_or("Application did not close")?;
                let observed = bridge.collect()?;
                let mut changes = Map::new();
                for (name, values) in object(&observed)? {
                    let changed: Vec<String> = object(values)?
                        .iter()
                        .filter(|(key, value)| policies[app][name].get(*key) != Some(*value))
                        .map(|(key, _)| key.clone())
                        .collect();
                    changes.insert(name.clone(), json!(changed));
                }
                println!(
                    "{}",
                    json!({"app":app,"exit":status.code().unwrap_or(1),"configFieldsChangedOnNormalClose":changes,"stateSaved":true})
                );
                Ok(())
            })();
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                let _ = child.kill();
                let _ = child.wait();
            }
            session?;
        }
        Ok(0)
    })();
    let _ = fs::remove_dir_all(&root);
    result
}

fn run() -> ProfileResult<i32> {
    let mut args = env::args().skip(1);
    let app = args.next().ok_or("Missing application")?;
    if app == "smoke" {
        return smoke();
    }
    let binary = args.next().ok_or("Missing application binary")?;
    if !["wordcraft", "gridcraft", "deckcraft", "prism"].contains(&app.as_str()) {
        return Err("Unsupported native profile".into());
    }
    let args: Vec<String> = args.collect();
    if args
        .iter()
        .any(|arg| ["--version", "--help", "-h"].contains(&arg.as_str()))
        || env::var_os(format!("{}_NO_PREFS", app.to_uppercase())).is_some()
    {
        let error = Command::new(&binary).args(&args).exec();
        return Err(error.to_string());
    }
    let policies = active_policies(&app)?;
    let fields = load(Path::new(
        &env::var("LUMEN_PROFILE_STATE_FIELDS")
            .map_err(|_| "LUMEN_PROFILE_STATE_FIELDS is missing")?,
    ))?[&app]
        .clone();
    object(&fields)?;
    let cache_fields = load(Path::new(
        &env::var("LUMEN_PROFILE_CACHE_FIELDS")
            .map_err(|_| "LUMEN_PROFILE_CACHE_FIELDS is missing")?,
    ))?[&app]
        .clone();
    let cache_fields = if cache_fields.is_null() {
        json!({})
    } else {
        cache_fields
    };
    let profile = if app == "prism" {
        xdg("XDG_DATA_HOME", ".local/share")?.join("PrismLauncher")
    } else {
        xdg("XDG_CONFIG_HOME", ".config")?.join(&app)
    };
    let bridge = Bridge::new(
        &app,
        policies,
        fields,
        profile,
        xdg("XDG_STATE_HOME", ".local/state")?.join("lumen-desktop/apps"),
        xdg("XDG_CACHE_HOME", ".cache")?.join("lumen-desktop/profiles"),
        cache_fields,
    );
    let runtime = runtime();
    let lockfile = runtime.join(format!("lumen-profile-{app}.lock"));
    let registry = runtime.join(format!("lumen-profile-{app}.json"));
    process_profile(&bridge, &registry, &lockfile, &binary, &args)
}

fn main() {
    match run() {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("Native profile separation: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Bridge, atomic, load, merge, partition};
    use serde_json::json;
    use std::fs;

    #[test]
    fn nested_fields_separate_from_preference_policy() {
        let policy = json!({"editing":{"markup":{"trackFormatting":true}}});
        let fields = json!({"editing":{"markup":{"comments":true,"ink":true}}});
        let value =
            json!({"editing":{"markup":{"trackFormatting":true,"comments":false,"ink":true}}});
        let (preference, state) = partition(&value, &policy, &fields, "").unwrap();
        assert_eq!(preference, policy);
        assert_eq!(
            state,
            json!({"editing":{"markup":{"comments":false,"ink":true}}})
        );
        assert_eq!(merge(&[&state, &policy]).unwrap(), value);
        assert!(partition(&preference, &json!({}), &fields, "").is_err());
    }

    #[test]
    fn prepare_and_collect_keep_unclassified_fields_private() {
        let root = std::env::temp_dir().join(format!("native-profile-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let bridge = Bridge::new(
            "fixture",
            json!({"ui.json":{"theme":"light"}}),
            json!({"ui.json":{"recent":[]}}),
            root.join("profile"),
            root.join("state"),
            root.join("cache"),
            json!({}),
        );
        atomic(
            &bridge.profile.join("ui.json"),
            &json!({"theme":"light","recent":["my.docx"]}),
        )
        .unwrap();
        bridge.prepare().unwrap();
        assert_eq!(
            load(&bridge.state.join("ui.json")).unwrap(),
            json!({"recent":["my.docx"]})
        );
        assert!(bridge.profile.join("ui.json").is_symlink());
        atomic(
            &bridge.cache.join("ui.json"),
            &json!({"theme":"dark","recent":["new.docx"]}),
        )
        .unwrap();
        assert_eq!(
            bridge.collect().unwrap(),
            json!({"ui.json":{"theme":"dark"}})
        );
        atomic(
            &bridge.cache.join("ui.json"),
            &json!({"theme":"dark","recent":["new.docx"],"token":"private"}),
        )
        .unwrap();
        assert!(bridge.collect().is_err());
        let draft = load(&bridge.state.join("unclassified-profile-draft.json")).unwrap();
        assert_eq!(draft["native"]["token"], "private");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prism_credentials_remain_state_and_detection_is_cache() {
        let root = std::env::temp_dir().join(format!("native-prism-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let bridge = Bridge::new(
            "prism",
            json!({"prismlauncher.cfg":{"ApplicationTheme":"system","MaxMemAlloc":"8192"}}),
            json!({"prismlauncher.cfg":{"ProxyPass":"","SelectedInstance":""}}),
            root.join("profile"),
            root.join("state"),
            root.join("cache"),
            json!({"prismlauncher.cfg":{"JavaSignature":""}}),
        );
        bridge
            .write_native(
                &bridge.profile.join("prismlauncher.cfg"),
                &json!({
                    "ApplicationTheme":"system","MaxMemAlloc":"8192","ProxyPass":"private-fixture",
                    "SelectedInstance":"My game","JavaSignature":"probe-result"
                }),
            )
            .unwrap();
        bridge.prepare().unwrap();
        assert_eq!(
            load(&bridge.state.join("prismlauncher.cfg")).unwrap(),
            json!({"ProxyPass":"private-fixture","SelectedInstance":"My game"})
        );
        assert_eq!(
            load(&bridge.cache.join("prismlauncher.cfg.derived")).unwrap()["JavaSignature"],
            "probe-result"
        );
        fs::remove_file(bridge.cache.join("prismlauncher.cfg")).unwrap();
        fs::remove_file(bridge.cache.join("prismlauncher.cfg.derived")).unwrap();
        bridge.prepare().unwrap();
        let restored = bridge
            .read_native(&bridge.cache.join("prismlauncher.cfg"))
            .unwrap();
        assert_eq!(restored["ProxyPass"], "private-fixture");
        assert_eq!(restored["JavaSignature"], "");
        bridge
            .write_native(
                &bridge.cache.join("prismlauncher.cfg"),
                &json!({
                    "ApplicationTheme":"dark","MaxMemAlloc":"8192","ProxyPass":"updated-private",
                    "SelectedInstance":"My game","JavaSignature":"","NewField":"unclassified"
                }),
            )
            .unwrap();
        assert!(bridge.collect().is_err());
        assert_eq!(
            load(&bridge.state.join("unclassified-profile-draft.json")).unwrap()["native"]["NewField"],
            "unclassified"
        );
        assert_eq!(
            load(&bridge.state.join("prismlauncher.cfg")).unwrap()["ProxyPass"],
            "updated-private"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
