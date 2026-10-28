//! Read-only installed desktop checks without user content or window titles.

use serde_json::{Value, json};
use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn command(program: &str, args: &[&str]) -> (bool, String) {
    match Command::new("timeout")
        .arg("8s")
        .arg(program)
        .args(args)
        .output()
    {
        Ok(output) => (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ),
        Err(_) => (false, String::new()),
    }
}

fn samefile(left: &Path, right: &Path) -> bool {
    match (fs::metadata(left), fs::metadata(right)) {
        (Ok(left), Ok(right)) => left.dev() == right.dev() && left.ino() == right.ino(),
        _ => false,
    }
}

fn persistence_backing(user: &Path, roots: &[PathBuf]) -> Option<String> {
    for root in roots {
        let candidate = root.join(user.strip_prefix("/").unwrap_or(user));
        if samefile(user, &candidate) {
            return Some(root.display().to_string());
        }
    }
    None
}

fn report() -> Value {
    let current = fs::canonicalize("/run/current-system").unwrap_or_default();
    let boot = fs::canonicalize("/nix/var/nix/profiles/system").unwrap_or_default();
    let mut checks = Vec::new();
    let mut add = |name: String, ok: bool, detail: Value| {
        checks.push(json!({"check":name,"ok":ok,"detail":detail}))
    };
    add(
        "Installed generation".into(),
        current == boot,
        json!({"running":current,"nextBoot":boot}),
    );
    for unit in [
        "lumen-desktop-shell",
        "lumen-notifications",
        "lumen-character-input",
        "lumen-file-operations",
        "lumen-display-layout",
    ] {
        let service = format!("{unit}.service");
        let (success, state) = command(
            "systemctl",
            &[
                "--user",
                "show",
                &service,
                "--property=ActiveState,SubState,NRestarts",
                "--no-pager",
            ],
        );
        let properties: serde_json::Map<String, Value> = state
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_owned(), Value::String(value.to_owned())))
            .collect();
        add(
            unit.into(),
            success && properties.get("ActiveState").and_then(Value::as_str) == Some("active"),
            Value::Object(properties),
        );
    }
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let roots = [PathBuf::from("/persist"), PathBuf::from("/persist-fast")];
    for item in [
        ".local/state/lumen-desktop/operations",
        ".local/state/lumen-desktop/notifications",
        ".local/share/Trash",
        "data",
    ] {
        let backing = persistence_backing(&home.join(item), &roots);
        add(
            format!("Persistence: {item}"),
            backing.is_some(),
            Value::String(
                backing
                    .map(|root| format!("same backing directory on {root}"))
                    .unwrap_or_else(|| "not verified".into()),
            ),
        );
    }
    let emoji = env::var("LUMEN_EMOJI_FONT_FAMILY").unwrap_or_else(|_| "Noto Color Emoji".into());
    for (pattern, label, expected) in [
        ("emoji", "Emoji font", emoji.as_str()),
        ("Interlude Variable", "Interface font", "Interlude"),
    ] {
        let (success, name) = command("fc-match", &["--format=%{family}", pattern]);
        add(
            label.into(),
            success && name.contains(expected),
            Value::String(name),
        );
    }
    let (success, scheme) = command(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "color-scheme"],
    );
    add(
        "Appearance preference".into(),
        success,
        Value::String(scheme),
    );
    let ok = checks.iter().all(|entry| entry["ok"] == true);
    json!({"ok":ok,"checks":checks,"pending":["Physical reboot, sleep/wake and peripheral checks","Off-machine personal-file backup: no destination configured"]})
}

fn main() {
    let value = report();
    println!(
        "{}",
        serde_json::to_string_pretty(&value).expect("diagnostic JSON")
    );
    if value["ok"] != true {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::persistence_backing;
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn identity_requires_same_inode_and_accepts_fast_root() {
        let root = std::env::temp_dir().join(format!("doctor-{}", std::process::id()));
        let user = root.join("home/user/notifications");
        let fast = root.join("persist-fast");
        let target = fast.join(user.strip_prefix("/").unwrap());
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(user.parent().unwrap()).unwrap();
        symlink(&target, &user).unwrap();
        assert_eq!(
            persistence_backing(&user, &[root.join("persist"), fast.clone()]),
            Some(fast.display().to_string())
        );
        fs::remove_file(&user).unwrap();
        fs::create_dir(&user).unwrap();
        assert_eq!(persistence_backing(&user, &[fast]), None);
        fs::remove_dir_all(root).unwrap();
    }
}
