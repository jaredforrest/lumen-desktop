//! Compile and apply the authoritative Nix desktop settings transaction.

use serde_json::{Value, json};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

type SettingsResult<T> = Result<T, String>;
const SECTIONS: [&str; 14] = [
    "applicationPreferences",
    "appearance",
    "displays",
    "textTools",
    "menuShortcuts",
    "notifications",
    "search",
    "controlCentre",
    "windows",
    "desktop",
    "menuBar",
    "inputs",
    "wallpaper",
    "screenSaver",
];

fn required_env(name: &str) -> SettingsResult<String> {
    env::var(name).map_err(|_| format!("{name} is missing"))
}

fn runtime() -> SettingsResult<PathBuf> {
    let base = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    let path = base.join("lumen-nix-settings");
    if !path.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)
            .map_err(|error| error.to_string())?;
    }
    Ok(path)
}

fn run_command(program: &str, args: &[&str], timeout: u64) -> SettingsResult<Output> {
    Command::new("timeout")
        .arg(format!("{timeout}s"))
        .arg(program)
        .args(args)
        .output()
        .map_err(|error| format!("{program}: {error}"))
}

fn successful(program: &str, args: &[&str], timeout: u64) -> SettingsResult<Output> {
    let output = run_command(program, args, timeout)?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{program} failed: {}",
            detail
                .chars()
                .rev()
                .take(4000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ));
    }
    Ok(output)
}

fn json_file(path: &Path) -> SettingsResult<Value> {
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn normalize_menu(mut value: Value) -> SettingsResult<Value> {
    let Some(object) = value.as_object_mut() else { return Ok(value); };
    if let Some(legacy) = object.remove("fullscreenVisibility") {
        if object.contains_key("visibility") { return Err("Use only menuBar.visibility; remove the legacy fullscreenVisibility alias".into()); }
        let visibility = match legacy.as_str() {
            Some("automatic") => "desktop", Some("always") => "always",
            _ => return Err("Invalid legacy menu visibility".into()),
        };
        object.insert("visibility".into(), json!(visibility));
    }
    Ok(value)
}

fn normalize_settings(mut value: Value) -> SettingsResult<Value> {
    if let Some(menu) = value.get_mut("menuBar") {
        *menu = normalize_menu(menu.clone())?;
    }
    Ok(value)
}

fn merge_section(current: &Value, patch: &Value) -> SettingsResult<Value> {
    let (Some(current), Some(patch)) = (current.as_object(), patch.as_object()) else {
        return Err("Preference patches must be objects".into());
    };
    let mut merged = current.clone();
    for (key, value) in patch {
        if !current.contains_key(key) { return Err("Unsupported preference key".into()); }
        merged.insert(key.clone(), value.clone());
    }
    Ok(Value::Object(merged))
}

fn output_path(output: &Output) -> SettingsResult<PathBuf> {
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    value[0]["outputs"]["out"]
        .as_str()
        .map(PathBuf::from)
        .ok_or("Nix returned no output path".into())
}

fn build(source: &Path) -> SettingsResult<PathBuf> {
    let builder = required_env("LUMEN_NIX_SETTINGS_BUILDER")?;
    let source = source.to_str().ok_or("Invalid Nix source path")?;
    let output = run_command(
        "nix",
        &[
            "build",
            "--file",
            &builder,
            "--argstr",
            "source",
            source,
            "--no-link",
            "--json",
        ],
        90,
    )?;
    if !output.status.success() {
        return Err(format!(
            "Nix rejected the settings; previous configuration kept.\n{}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .rev()
                .take(4000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ));
    }
    output_path(&output)
}

fn activate(bundle: &Path) -> SettingsResult<()> {
    let runtime = runtime()?;
    let root = runtime.join("gc-root");
    successful(
        "nix-store",
        &[
            "--add-root",
            root.to_str().ok_or("Invalid runtime path")?,
            "--indirect",
            "--realise",
            bundle.to_str().ok_or("Invalid bundle path")?,
        ],
        15,
    )?;
    let next = runtime.join("next");
    if next.exists() || next.is_symlink() {
        fs::remove_file(&next).map_err(|error| error.to_string())?;
    }
    symlink(bundle, &next).map_err(|error| error.to_string())?;
    fs::rename(next, runtime.join("current")).map_err(|error| error.to_string())?;
    Ok(())
}

fn nix_string(value: &str) -> SettingsResult<String> {
    let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
    if value
        .chars()
        .any(|c| (c as u32) < 32 && !matches!(c, '\n' | '\r' | '\t'))
    {
        let nested = serde_json::to_string(&encoded).map_err(|error| error.to_string())?;
        Ok(format!(
            "(builtins.fromJSON {})",
            nested.replace("${", "\\${")
        ))
    } else {
        Ok(encoded.replace("${", "\\${"))
    }
}

fn encode(value: &Value, depth: usize) -> SettingsResult<String> {
    match value {
        Value::Object(entries) => {
            let mut rows = Vec::new();
            for (key, item) in entries {
                rows.push(format!(
                    "{}{} = {};",
                    "  ".repeat(depth + 1),
                    nix_string(key)?,
                    encode(item, depth + 1)?
                ));
            }
            Ok(format!("{{\n{}\n{}}}", rows.join("\n"), "  ".repeat(depth)))
        }
        Value::Array(items) => Ok(format!(
            "[ {} ]",
            items
                .iter()
                .map(|item| encode(item, depth + 1))
                .collect::<SettingsResult<Vec<_>>>()?
                .join(" ")
        )),
        Value::String(value) => nix_string(value),
        _ => serde_json::to_string(value).map_err(|error| error.to_string()),
    }
}

fn serialize(values: &Value) -> SettingsResult<String> {
    Ok(format!(
        "# Authoritative desktop settings. Edited by the Settings interface.\n# Never put secrets here: Nix build outputs are publicly readable.\n{}\n",
        encode(values, 0)?
    ))
}

fn rebuild_system(_candidate: &Path) -> SettingsResult<PathBuf> {
    if run_command("pgrep", &["-x", "resize2fs"], 4)?.status.success() {
        return Err("Finish the active filesystem resize before rebuilding configuration.".into());
    }
    let deploy = required_env("LUMEN_NIX_SETTINGS_DEPLOY")?;
    let output = run_command(&deploy, &["build"], 3300)?;
    if !output.status.success() {
        return Err(format!("NixOS rebuild failed; committed source retained.\n{}",
            String::from_utf8_lossy(&output.stdout).chars().rev().take(4000).collect::<String>().chars().rev().collect::<String>()));
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    value["system"].as_str().map(PathBuf::from).ok_or("Deployment returned no system path".into())
}

fn lock() -> SettingsResult<File> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(runtime()?.join("transaction.lock"))
        .map_err(|error| error.to_string())?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(file)
}

fn sync() -> SettingsResult<Value> {
    if env::var_os("LUMEN_NIX_SETTINGS_DEPLOY").is_some_and(|value| !value.is_empty()) {
        return read_settings();
    }
    let _guard = lock()?;
    if let Some(active) = env::var("LUMEN_NIX_SETTINGS_ACTIVE")
        .ok()
        .filter(|value| !value.is_empty())
    {
        return normalize_settings(json_file(Path::new(&active))?);
    }
    let bundle = build(Path::new(&required_env("LUMEN_NIX_SETTINGS_SOURCE")?))?;
    activate(&bundle)?;
    normalize_settings(json_file(&bundle.join("settings.json"))?)
}

fn read_settings() -> SettingsResult<Value> {
    if let Some(active) = env::var("LUMEN_NIX_SETTINGS_ACTIVE")
        .ok()
        .filter(|value| !value.is_empty())
    {
        return normalize_settings(json_file(Path::new(&active))?);
    }
    let installed = Path::new("/etc/nix/desktop-settings.json");
    if installed.exists() {
        return normalize_settings(json_file(installed)?);
    }
    let file = runtime()?.join("current/settings.json");
    if !file.exists() {
        sync()?;
    }
    normalize_settings(json_file(&file)?)
}

fn git(repo: &Path, args: &[&str]) -> SettingsResult<String> {
    let repo = repo.to_str().ok_or("Invalid repository path")?;
    let mut command = vec!["-C", repo];
    command.extend_from_slice(args);
    let output = run_command("git", &command, 30)?;
    if !output.status.success() {
        return Err(format!("Git operation failed: {}", String::from_utf8_lossy(&output.stderr)
            .chars().rev().take(2000).collect::<String>().chars().rev().collect::<String>()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

fn git_transaction(section: &str, value: Value, application: Option<&str>, merge: bool) -> SettingsResult<Value> {
    let _guard = lock()?;
    if !SECTIONS.contains(&section) { return Err("Unsupported Nix settings section".into()); }
    let source = PathBuf::from(required_env("LUMEN_NIX_SETTINGS_SOURCE")?);
    let repo = fs::canonicalize(required_env("LUMEN_NIX_SETTINGS_BUILD_SOURCE")?).map_err(|e| e.to_string())?;
    if fs::canonicalize(&source).map_err(|e| e.to_string())? != repo.join("settings/desktop.nix") {
        return Err("Settings source is outside the authoritative repository".into());
    }
    if !git(&repo, &["status","--porcelain"])?.is_empty() {
        return Err("Commit or set aside pending configuration edits before changing Settings".into());
    }
    let previous = git(&repo, &["rev-parse","HEAD"])?;
    let original = fs::read(&source).map_err(|e| e.to_string())?;
    let mut values = json_file(&build(&source)?.join("settings.json"))?;
    if let Some(app) = application {
        if section != "applicationPreferences" || values[section].get(app).is_none() {
            return Err("Unsupported application preferences".into());
        }
        values[section][app] = value;
    } else {
        values[section] = if merge { merge_section(&values[section], &value)? } else { value };
    }
    let candidate = source.parent().ok_or("Settings source has no parent")?.join(format!(
        ".desktop-settings-{}-{}.nix",std::process::id(),SystemTime::now()
            .duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos()));
    let mut committed = None::<String>;
    let mut system = None::<PathBuf>;
    let mut replaced = false;
    let mut candidate_bytes = Vec::new();
    let result = (|| -> SettingsResult<(Value,bool)> {
        let mut file = OpenOptions::new().create_new(true).write(true).mode(0o600)
            .open(&candidate).map_err(|e| e.to_string())?;
        file.write_all(serialize(&values)?.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        let bundle = build(&candidate)?;
        if git(&repo,&["rev-parse","HEAD"])? != previous || fs::read(&source).map_err(|e| e.to_string())? != original {
            return Err("Configuration changed during evaluation; retry".into());
        }
        candidate_bytes = fs::read(&candidate).map_err(|e| e.to_string())?;
        let result = normalize_settings(json_file(&bundle.join("settings.json"))?)?;
        if candidate_bytes == original { return Ok((result,false)); }
        fs::rename(&candidate,&source).map_err(|e| e.to_string())?;
        replaced = true;
        git(&repo,&["add","--","settings/desktop.nix"])?;
        git(&repo,&["commit","--only","-m",&format!("Update desktop setting: {section}"),"--","settings/desktop.nix"])?;
        committed = Some(git(&repo,&["rev-parse","HEAD"])?);
        system = Some(rebuild_system(&source)?);
        let system_path = system.as_ref().and_then(|path| path.to_str()).ok_or("Invalid system path")?;
        let previous_system = fs::canonicalize("/run/current-system").map_err(|e| e.to_string())?;
        let pkexec = env::var("LUMEN_NIX_SETTINGS_PKEXEC").unwrap_or_else(|_| "/run/wrappers/bin/pkexec".into());
        successful(&pkexec,&["/run/current-system/sw/bin/lumen-activate-settings",system_path,
            previous_system.to_str().ok_or("Invalid system path")?],300)?;
        Ok((result,true))
    })();
    let _ = fs::remove_file(&candidate);
    let (result, changed) = match result {
        Ok(value) => value,
        Err(error) => {
            if system.as_ref().is_some_and(|path| fs::canonicalize("/run/current-system").ok().as_ref() == Some(path)) {
                return Err(error);
            }
            if let Some(commit) = committed {
                if git(&repo,&["rev-parse","HEAD"])? != commit || fs::read(&source).map_err(|e| e.to_string())? != candidate_bytes {
                    return Err("Configuration changed concurrently; automatic rollback stopped".into());
                }
                fs::write(&source,&original).map_err(|e| e.to_string())?;
                git(&repo,&["add","--","settings/desktop.nix"])?;
                git(&repo,&["commit","--only","-m","Restore settings after unsuccessful activation","--","settings/desktop.nix"])?;
            } else if replaced {
                if fs::read(&source).map_err(|e| e.to_string())? != candidate_bytes {
                    return Err("Source changed concurrently; automatic rollback stopped".into());
                }
                fs::write(&source,&original).map_err(|e| e.to_string())?;
                git(&repo,&["reset","--","settings/desktop.nix"])?;
            }
            return Err(error);
        }
    };
    if changed {
        let deploy = required_env("LUMEN_NIX_SETTINGS_DEPLOY")?;
        let synced = run_command(&deploy,&["sync"],65).is_ok_and(|output| output.status.success());
        let marker = runtime()?.join("git-sync-pending");
        if synced { let _ = fs::remove_file(marker); }
        else {
            if let Some(commit) = &committed { fs::write(marker,format!("{commit}\n")).map_err(|e| e.to_string())?; }
            let _ = run_command("notify-send",&["Configuration saved locally","GitHub sync is pending; it will retry automatically."],5);
        }
    }
    Ok(result)
}

fn set(section: &str, mut value: Value, application: Option<&str>, merge: bool) -> SettingsResult<Value> {
    if section == "menuBar" { value = normalize_menu(value)?; }
    if env::var_os("LUMEN_NIX_SETTINGS_DEPLOY").is_some_and(|item| !item.is_empty()) {
        return git_transaction(section, value, application, merge);
    }
    let _guard = lock()?;
    if !SECTIONS.contains(&section) {
        return Err("Unsupported Nix settings section".into());
    }
    let source = PathBuf::from(required_env("LUMEN_NIX_SETTINGS_SOURCE")?);
    let original = fs::read(&source).map_err(|error| error.to_string())?;
    let current = build(&source)?;
    if fs::read(&source).map_err(|error| error.to_string())? != original {
        return Err("The Nix source changed during evaluation; retry the edit".into());
    }
    let mut values = json_file(&current.join("settings.json"))?;
    if let Some(app) = application {
        if section != "applicationPreferences" || values[section].get(app).is_none() {
            return Err("Unsupported application preferences".into());
        }
        values[section][app] = value;
    } else {
        values[section] = if merge { merge_section(&values[section], &value)? } else { value };
    }
    let candidate = source
        .parent()
        .ok_or("Settings source has no parent")?
        .join(format!(
            ".desktop-settings-{}-{}.nix",
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
            .open(&candidate)
            .map_err(|error| error.to_string())?;
        file.write_all(serialize(&values)?.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        let bundle = build(&candidate)?;
        let system = if env::var_os("LUMEN_NIX_SETTINGS_BUILD_SOURCE")
            .is_some_and(|value| !value.is_empty())
        {
            Some(rebuild_system(&candidate)?)
        } else {
            None
        };
        if fs::read(&source).map_err(|error| error.to_string())? != original {
            return Err("The Nix source changed during validation; retry the edit".into());
        }
        fs::rename(&candidate, &source).map_err(|error| error.to_string())?;
        let activated = if let Some(system) = system {
            let previous =
                fs::canonicalize("/run/current-system").map_err(|error| error.to_string())?;
            successful(
                "pkexec",
                &[
                    "/run/current-system/sw/bin/lumen-activate-settings",
                    system.to_str().ok_or("Invalid system path")?,
                    previous.to_str().ok_or("Invalid system path")?,
                ],
                300,
            )
            .map(|_| ())
        } else {
            activate(&bundle)
        };
        if let Err(error) = activated {
            fs::write(&source, &original)
                .map_err(|restore| format!("{error}; source restoration failed: {restore}"))?;
            return Err(error);
        }
        normalize_settings(json_file(&bundle.join("settings.json"))?)
    })();
    if candidate.exists() {
        let _ = fs::remove_file(candidate);
    }
    result
}

fn run() -> SettingsResult<Value> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => read_settings(),
        [command] if command == "read" => read_settings(),
        [command] if command == "sync" => sync(),
        [command, section, value] if command == "set" => set(
            section,
            serde_json::from_str(value).map_err(|error| error.to_string())?,
            None, false,
        ),
        [command, section, value] if command == "patch" => set(
            section,
            serde_json::from_str(value).map_err(|error| error.to_string())?,
            None, true,
        ),
        [command, app, value] if command == "set-application" => set(
            "applicationPreferences",
            serde_json::from_str(value).map_err(|error| error.to_string())?,
            Some(app), false,
        ),
        [command] if command == "apply" => Err(
            "Manual configuration edits require a NixOS rebuild and authenticated activation."
                .into(),
        ),
        _ => Err("Unknown Nix settings command".into()),
    }
}

fn main() {
    let output = match run() {
        Ok(value) => value,
        Err(error) => json!({"error":error}),
    };
    println!("{}", output);
    if output.get("error").is_some() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::serialize;
    use serde_json::json;

    #[test]
    fn nix_strings_keep_interpolation_literal() {
        let text = "${builtins.abort \"must not evaluate\"}\nQuotes: \" and \\ and é";
        let serialized = serialize(&json!({"menuShortcuts":{"1":{"app":text}}})).unwrap();
        assert!(serialized.contains("\\${builtins.abort"));
        assert!(serialized.contains("Quotes:"));
    }

    #[test]
    fn control_characters_use_json_decoder() {
        let serialized = serialize(&json!({"x":"a\u{0001}b"})).unwrap();
        assert!(serialized.contains("builtins.fromJSON"));
    }
}
