//! Replace unrelated provider seams only in disposable fixture copies.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Provider {
    filename: &'static str,
    needle: &'static str,
    kind: &'static str,
}

const PROVIDERS: [Provider; 2] = [
    Provider {
        filename: "VpnState.qml",
        needle: "\"lumen-vpn-control\"",
        kind: "vpn",
    },
    Provider {
        filename: "SystemState.qml",
        needle: "\"lumen-system-controls\"",
        kind: "system",
    },
];

fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn payload(kind: &str) -> Result<Value> {
    match kind {
        "vpn" => Ok(
            json!({"mullvad":{"installed":false,"state":"unavailable","account":false},
            "tailscale":{"installed":false,"state":"unavailable","online":false,"peers":0,"exitNodes":[]}}),
        ),
        "system" => Ok(
            json!({"wifi":{"available":false,"networks":[]},"bluetoothRadios":{},"power":{"available":false}}),
        ),
        _ => Err("Unknown fixture provider".into()),
    }
}

fn serve(kind: &str, output: &Path, command: Option<&str>, input: &str) -> Result<String> {
    let request: Value = if kind == "vpn" {
        serde_json::from_str(input)?
    } else {
        json!({"action":command.unwrap_or("")})
    };
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(output.join(format!("{kind}-provider-requests.jsonl")))?;
    writeln!(log, "{}", request)?;
    if request["action"] != "status" {
        return Err("Unexpected fixture provider action".into());
    }
    Ok(format!("{}\n", payload(kind)?))
}

fn calendar(input: &str) -> Result<String> {
    let request: Value = serde_json::from_str(input)?;
    if !["list", "load", "sync"]
        .iter()
        .any(|action| request["action"] == *action)
    {
        return Err("Unexpected fixture calendar action".into());
    }
    Ok(format!(
        "{}\n",
        json!({
            "configured": false, "events": [], "monthEvents": [], "days": [],
            "total": 0, "providers": [], "month": request["month"],
            "date": request["selectedDate"],
        })
    ))
}

struct Prepared {
    target: PathBuf,
    helper: PathBuf,
    original: String,
    modified: String,
}

fn isolate(shell: &Path, output: &Path, executable: &Path) -> Result<Value> {
    let shell = fs::canonicalize(shell)?;
    let output = fs::canonicalize(output)?;
    if shell == output || !shell.starts_with(&output) {
        return Err("Provider fixtures require a private copied shell".into());
    }
    let mut prepared = Vec::new();
    for provider in &PROVIDERS {
        let target = shell.join(provider.filename);
        if fs::symlink_metadata(&target)?.file_type().is_symlink()
            || !target.is_file()
            || !fs::canonicalize(&target)?.starts_with(&output)
        {
            return Err("Provider fixture target must be a private regular copy".into());
        }
        let original = fs::read_to_string(&target)?;
        if original.matches(provider.needle).count() != 1 {
            return Err(format!("Provider command seam changed: {}", provider.filename).into());
        }
        let helper = output.join(format!("{}-unavailable-provider", provider.kind));
        if helper.exists() || fs::symlink_metadata(&helper).is_ok() {
            return Err(format!("Provider helper already exists: {}", helper.display()).into());
        }
        let replacement = serde_json::to_string(&helper.to_string_lossy())?;
        let modified = original.replacen(provider.needle, &replacement, 1);
        if modified.replacen(&replacement, provider.needle, 1) != original {
            return Err("Provider seam roundtrip failed".into());
        }
        prepared.push(Prepared {
            target,
            helper,
            original,
            modified,
        });
    }
    let helper_hash = digest(&fs::read(executable)?);
    let mut records = Vec::new();
    for item in prepared {
        symlink(executable, &item.helper)?;
        fs::set_permissions(&item.target, fs::Permissions::from_mode(0o600))?;
        fs::write(&item.target, &item.modified)?;
        records.push(json!({
            "file":item.target,"originalSha256":digest(item.original.as_bytes()),
            "modifiedSha256":digest(item.modified.as_bytes()),"helper":item.helper,
            "helperSha256":helper_hash,"roundtrip":true,"originalMatchCount":1,
            "scope":"Only unrelated provider command target; non-status requests rejected"
        }));
    }
    let value = Value::Array(records);
    fs::write(
        output.join("provider-isolation.json"),
        format!("{}\n", serde_json::to_string_pretty(&value)?),
    )?;
    Ok(value)
}

fn main() -> Result<()> {
    let executable = env::current_exe()?;
    let invoked = PathBuf::from(env::args_os().next().ok_or("Missing argv0")?);
    let name = invoked
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    if name == "lumen-fixture-calendar" || name == "lumen-calendar" {
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        print!("{}", calendar(&line)?);
        return Ok(());
    }
    if let Some(kind) = name.strip_suffix("-unavailable-provider") {
        let mut line = String::new();
        if kind == "vpn" {
            io::stdin().lock().read_line(&mut line)?;
        }
        let command = env::args().nth(1);
        let output = invoked.parent().ok_or("Provider helper has no parent")?;
        print!("{}", serve(kind, output, command.as_deref(), &line)?);
        return Ok(());
    }
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("isolate") {
        return Err("Usage: lumen-fixture-providers isolate SHELL OUTPUT".into());
    }
    let shell = PathBuf::from(args.next().ok_or("Missing shell")?);
    let output = PathBuf::from(args.next().ok_or("Missing output")?);
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    println!("{}", isolate(&shell, &output, &executable)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_and_rejection() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            serve("vpn", root.path(), None, "{\"action\":\"status\"}")
                .unwrap()
                .trim()
                .parse::<Value>()
                .unwrap(),
            payload("vpn").unwrap()
        );
        assert!(serve("vpn", root.path(), None, "{\"action\":\"disconnect\"}").is_err());
        assert_eq!(
            serve("system", root.path(), Some("status"), "")
                .unwrap()
                .trim()
                .parse::<Value>()
                .unwrap(),
            payload("system").unwrap()
        );
        assert!(serve("system", root.path(), Some("disconnect"), "").is_err());
        let log = fs::read_to_string(root.path().join("vpn-provider-requests.jsonl")).unwrap();
        assert_eq!(log.lines().count(), 2);
    }
    #[test]
    fn calendar_absent_account_contract() {
        let response: Value = serde_json::from_str(
            &calendar(r#"{"action":"load","month":"2026-10","selectedDate":"2026-10-25"}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            response,
            json!({
                "configured":false,"events":[],"monthEvents":[],"days":[],"total":0,
                "providers":[],"month":"2026-10","date":"2026-10-25"
            })
        );
        assert!(calendar(r#"{"action":"account-list"}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Value>(&calendar(r#"{"action":"list"}"#).unwrap()).unwrap()["month"],
            Value::Null
        );
    }
    #[test]
    fn copy_and_seam_boundaries() {
        let root = tempfile::tempdir().unwrap();
        let shell = root.path().join("assets/shell");
        fs::create_dir_all(&shell).unwrap();
        for provider in &PROVIDERS {
            fs::write(
                shell.join(provider.filename),
                format!("command: [{}]", provider.needle),
            )
            .unwrap();
        }
        let binary = env::current_exe().unwrap();
        assert_eq!(
            isolate(&shell, root.path(), &binary)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(shell.join("VpnState.qml").is_file());
        assert!(
            !fs::read_to_string(shell.join("VpnState.qml"))
                .unwrap()
                .contains("lumen-vpn-control")
        );
        let other = root.path().join("other");
        fs::create_dir(&other).unwrap();
        assert!(isolate(&shell, &other, &binary).is_err());
    }
    #[test]
    fn changed_seam_does_not_rewrite_either_target() {
        let root = tempfile::tempdir().unwrap();
        let shell = root.path().join("assets/shell");
        fs::create_dir_all(&shell).unwrap();
        fs::write(shell.join("VpnState.qml"), PROVIDERS[0].needle).unwrap();
        fs::write(shell.join("SystemState.qml"), "changed provider").unwrap();
        let before = fs::read(shell.join("VpnState.qml")).unwrap();
        assert!(isolate(&shell, root.path(), &env::current_exe().unwrap()).is_err());
        assert_eq!(fs::read(shell.join("VpnState.qml")).unwrap(), before);
        assert!(!root.path().join("vpn-unavailable-provider").exists());
    }
    #[test]
    fn symlink_target_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let shell = root.path().join("assets/shell");
        fs::create_dir_all(&shell).unwrap();
        let external = root.path().join("original.qml");
        fs::write(&external, PROVIDERS[0].needle).unwrap();
        symlink(&external, shell.join("VpnState.qml")).unwrap();
        fs::write(shell.join("SystemState.qml"), PROVIDERS[1].needle).unwrap();
        assert!(isolate(&shell, root.path(), &env::current_exe().unwrap()).is_err());
        assert_eq!(fs::read(&external).unwrap(), PROVIDERS[0].needle.as_bytes());
    }
}
