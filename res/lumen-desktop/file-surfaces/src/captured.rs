//! Per-provider capabilities bound to exact local filesystem objects.
use gio::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const MAX_CAPABILITIES: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Facts(pub [i128; 6]);

impl Facts {
    pub fn read(path: &Path) -> Result<Self, Unavailable> {
        let info = fs::symlink_metadata(path).map_err(|error| match error.kind() {
            std::io::ErrorKind::PermissionDenied => Unavailable::new(
                "permission_denied",
                "Permission to inspect the selected file was denied",
            ),
            _ => Unavailable::new("source_changed", "The selected file is no longer available"),
        })?;
        Ok(Self::from_metadata(&info))
    }

    pub fn from_metadata(info: &fs::Metadata) -> Self {
        Self([
            i128::from(info.dev()),
            i128::from(info.ino()),
            i128::from(info.mode()),
            i128::from(info.size()),
            i128::from(info.mtime()) * 1_000_000_000 + i128::from(info.mtime_nsec()),
            i128::from(info.ctime()) * 1_000_000_000 + i128::from(info.ctime_nsec()),
        ])
    }

    pub fn wire(self) -> Vec<String> {
        self.0
            .into_iter()
            .map(|number| number.to_string())
            .collect()
    }

    pub fn parse(value: &Value) -> Result<Self, Unavailable> {
        let invalid = || Unavailable::new("invalid_request", "Invalid captured file version");
        let words = value
            .as_array()
            .filter(|words| words.len() == 6)
            .ok_or_else(invalid)?;
        let mut result = [0_i128; 6];
        for (index, word) in words.iter().enumerate() {
            let word = word.as_str().ok_or_else(invalid)?;
            let digits = word.strip_prefix('-').unwrap_or(word);
            if digits.is_empty()
                || digits.len() > 20
                || !digits.bytes().all(|byte| byte.is_ascii_digit())
                || (digits.len() > 1 && digits.starts_with('0'))
                || word == "-0"
            {
                return Err(invalid());
            }
            let number = word.parse::<i128>().map_err(|_| invalid())?;
            if number < -(1_i128 << 63) || number >= (1_i128 << 64) {
                return Err(invalid());
            }
            result[index] = number;
        }
        Ok(Self(result))
    }

    fn supports_actions(self) -> bool {
        let mode = self.0[2] as u32;
        let kind = mode & 0o170000;
        kind == 0o100000 || kind == 0o040000
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Unavailable {
    pub reason: &'static str,
    pub message: &'static str,
}

impl Unavailable {
    fn new(reason: &'static str, message: &'static str) -> Self {
        Self { reason, message }
    }
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(self.message)
    }
}

impl std::error::Error for Unavailable {}

#[derive(Clone, Debug)]
pub struct Selection {
    pub path: PathBuf,
    pub facts: Facts,
    pub resource: Option<PathBuf>,
    pub resource_token: Option<String>,
    choices: Option<(String, HashMap<String, AppChoice>)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppFacts {
    registration: Facts,
    resolved: PathBuf,
    target: Facts,
}

#[derive(Clone, Debug)]
struct AppChoice {
    filename: PathBuf,
    facts: AppFacts,
}

fn application_facts(path: &Path) -> Result<AppFacts, Unavailable> {
    let missing = || {
        Unavailable::new(
            "application_unavailable",
            "This application registration is unavailable",
        )
    };
    let resolved = fs::canonicalize(path).map_err(|_| missing())?;
    let registration = Facts::read(path).map_err(|_| missing())?;
    let target = Facts::read(&resolved).map_err(|_| missing())?;
    if target.0[2] as u32 & 0o170000 != 0o100000 {
        return Err(missing());
    }
    Ok(AppFacts {
        registration,
        resolved,
        target,
    })
}

fn content_type(path: &Path) -> &str {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
}

fn uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut result = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            result.push(*byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

fn run_open(command: &str, args: &[&str]) -> Result<(), Unavailable> {
    let mut launcher = if std::env::var("LUMEN_LAUNCH_SCOPES").as_deref() == Ok("1") {
        let mut scoped = Command::new("lumen-launch-app");
        scoped.arg("--").arg(command).args(args);
        scoped
    } else {
        let mut direct = Command::new(command);
        direct.args(args);
        direct
    };
    let mut child = launcher
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            Unavailable::new(
                "provider_failure",
                "The application launcher is unavailable",
            )
        })?;
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) | Err(_) => {
                return Err(Unavailable::new(
                    "dispatch_failed",
                    "The application launcher reported a failure",
                ));
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Unavailable::new(
                    "dispatch_unknown",
                    "The launcher did not acknowledge Open; check whether the application opened before trying again",
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
        }
    }
}

pub struct Captured {
    generation: String,
    entries: Mutex<HashMap<String, Selection>>,
}

fn random_token() -> Result<String, Unavailable> {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).map_err(|_| {
        Unavailable::new(
            "provider_failure",
            "The provider could not create a private capability",
        )
    })?;
    Ok(hex::encode(bytes))
}

impl Captured {
    pub fn new() -> Result<Self, Unavailable> {
        Ok(Self {
            generation: random_token()?,
            entries: Mutex::new(HashMap::new()),
        })
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    pub fn capture(
        &self,
        path: &Path,
        expected: Option<Facts>,
        resource: Option<PathBuf>,
    ) -> Result<String, Unavailable> {
        if !path.is_absolute() || path.as_os_str().as_encoded_bytes().contains(&0) {
            return Err(Unavailable::new(
                "invalid_request",
                "An absolute local file is required",
            ));
        }
        let current = Facts::read(path)?;
        if !current.supports_actions() {
            return Err(Unavailable::new(
                "unsupported",
                "This file type does not support captured actions",
            ));
        }
        if expected.is_some_and(|old| old != current) {
            return Err(Unavailable::new(
                "source_changed",
                "The selected file changed",
            ));
        }
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= MAX_CAPABILITIES {
            return Err(Unavailable::new(
                "policy_limit",
                "Too many retained action capabilities; refresh this provider",
            ));
        }
        let key = random_token()?;
        entries.insert(
            key.clone(),
            Selection {
                path: path.to_owned(),
                facts: current,
                resource,
                resource_token: None,
                choices: None,
            },
        );
        Ok(key)
    }

    pub fn selected(&self, key: &str) -> Result<Selection, Unavailable> {
        let selection = self
            .entries
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| {
                Unavailable::new(
                    "source_changed",
                    "This captured selection is no longer available",
                )
            })?;
        if Facts::read(&selection.path)? != selection.facts {
            return Err(Unavailable::new(
                "source_changed",
                "The selected file changed",
            ));
        }
        Ok(selection)
    }

    pub fn bind_resource(
        &self,
        key: &str,
        resource: PathBuf,
        token: Option<String>,
    ) -> Result<(), Unavailable> {
        self.selected(key)?;
        let mut entries = self.entries.lock().unwrap();
        let selection = entries.get_mut(key).ok_or_else(|| {
            Unavailable::new(
                "source_changed",
                "This captured selection is no longer available",
            )
        })?;
        selection.resource = Some(resource);
        selection.resource_token = token;
        Ok(())
    }

    pub fn retire(&self, keys: &[String]) {
        let mut entries = self.entries.lock().unwrap();
        for key in keys {
            entries.remove(key);
        }
    }

    pub fn applications(&self, key: &str) -> Result<Value, Unavailable> {
        let selected = self.selected(key)?;
        let mime = content_type(&selected.path);
        let default = gio::AppInfo::default_for_type(mime, false).and_then(|app| app.id());
        let mut choices = HashMap::new();
        let mut rows = Vec::new();
        for app in gio::AppInfo::all_for_type(mime) {
            if !app.should_show() {
                continue;
            }
            let Some(id) = app.id() else {
                continue;
            };
            let Some(filename) = app
                .downcast_ref::<gio::DesktopAppInfo>()
                .and_then(|desktop| desktop.filename())
            else {
                continue;
            };
            let Ok(facts) = application_facts(&filename) else {
                continue;
            };
            rows.push(serde_json::json!({"id":id.to_string(),"name":app.display_name().to_string(),"default":default.as_deref()==Some(id.as_str())}));
            choices.insert(id.to_string(), AppChoice { filename, facts });
        }
        self.selected(key)?;
        let generation = random_token()?;
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(key).ok_or_else(|| {
            Unavailable::new(
                "source_changed",
                "This captured selection is no longer available",
            )
        })?;
        entry.choices = Some((generation.clone(), choices));
        Ok(serde_json::json!({"applications":rows,"choice":generation}))
    }

    pub fn dispatch(
        &self,
        key: &str,
        action: &str,
        application: Option<&str>,
        choice: Option<&str>,
    ) -> Result<Value, Unavailable> {
        let selected = self.selected(key)?;
        if action == "open" {
            self.selected(key)?;
            run_open("xdg-open", &[&uri(&selected.path)])?;
            return Ok(serde_json::json!({"ok":true,"dispatched":true}));
        }
        if action != "open-with" && action != "set-default" {
            return Err(Unavailable::new(
                "invalid_request",
                "Unsupported captured action",
            ));
        }
        let (generation, choices) = selected.choices.as_ref().ok_or_else(|| {
            Unavailable::new(
                "application_unavailable",
                "This application chooser is no longer current",
            )
        })?;
        if Some(generation.as_str()) != choice {
            return Err(Unavailable::new(
                "application_unavailable",
                "This application chooser is no longer current",
            ));
        }
        let app_id = application.ok_or_else(|| {
            Unavailable::new(
                "application_unavailable",
                "This application chooser is no longer current",
            )
        })?;
        let selected_app = choices.get(app_id).ok_or_else(|| {
            Unavailable::new(
                "application_unavailable",
                "This application chooser is no longer current",
            )
        })?;
        if application_facts(&selected_app.filename).ok().as_ref() != Some(&selected_app.facts) {
            return Err(Unavailable::new(
                "application_unavailable",
                "The selected application changed",
            ));
        }
        let mime = content_type(&selected.path);
        let app = gio::AppInfo::all_for_type(mime)
            .into_iter()
            .find(|app| {
                app.should_show()
                    && app.id().as_deref() == Some(app_id)
                    && app
                        .downcast_ref::<gio::DesktopAppInfo>()
                        .and_then(|desktop| desktop.filename())
                        .as_deref()
                        == Some(selected_app.filename.as_path())
            })
            .ok_or_else(|| {
                Unavailable::new(
                    "application_unavailable",
                    "This application is no longer available",
                )
            })?;
        self.selected(key)?;
        if application_facts(&selected_app.filename).ok().as_ref() != Some(&selected_app.facts) {
            return Err(Unavailable::new(
                "application_unavailable",
                "The selected application changed",
            ));
        }
        if action == "set-default" {
            app.set_as_default_for_type(mime).map_err(|_| {
                Unavailable::new(
                    "provider_failure",
                    "The default application could not be changed",
                )
            })?;
            return Ok(serde_json::json!({"ok":true}));
        }
        self.selected(key)?;
        if application_facts(&selected_app.filename).ok().as_ref() != Some(&selected_app.facts) {
            return Err(Unavailable::new(
                "application_unavailable",
                "The selected application changed",
            ));
        }
        let filename = selected_app.filename.to_str().ok_or_else(|| {
            Unavailable::new(
                "application_unavailable",
                "This application cannot open files",
            )
        })?;
        run_open("gio", &["launch", filename, &uri(&selected.path)])?;
        Ok(serde_json::json!({"ok":true,"dispatched":true}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn wire_facts_accept_exact_high_identities_only() {
        let facts = Facts([u64::MAX as i128, 9, 0o100600, 1, -1, 10]);
        assert_eq!(
            Facts::parse(&serde_json::json!(facts.wire())).unwrap(),
            facts
        );
        for invalid in [
            serde_json::json!(["01", "2", "3", "4", "5", "6"]),
            serde_json::json!(["-0", "2", "3", "4", "5", "6"]),
            serde_json::json!(["18446744073709551616", "2", "3", "4", "5", "6"]),
        ] {
            assert_eq!(
                Facts::parse(&invalid).unwrap_err().reason,
                "invalid_request"
            );
        }
    }

    #[test]
    fn selection_rejects_symlink_replacement_and_retirement() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file");
        let link = root.path().join("link");
        fs::write(&file, "first").unwrap();
        symlink(&file, &link).unwrap();
        let captured = Captured::new().unwrap();
        assert_eq!(
            captured.capture(&link, None, None).unwrap_err().reason,
            "unsupported"
        );
        let key = captured
            .capture(&file, Some(Facts::read(&file).unwrap()), None)
            .unwrap();
        assert_eq!(captured.selected(&key).unwrap().path, file);
        fs::write(&file, "replacement").unwrap();
        assert_eq!(
            captured.selected(&key).unwrap_err().reason,
            "source_changed"
        );
        captured.retire(&[key.clone()]);
        assert_eq!(
            captured.selected(&key).unwrap_err().reason,
            "source_changed"
        );
    }

    #[test]
    fn application_registration_tracks_link_and_target_identity() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.desktop");
        let replacement = root.path().join("replacement.desktop");
        let registration = root.path().join("registration.desktop");
        fs::write(&target, "[Desktop Entry]\nName=Fixture\n").unwrap();
        symlink(&target, &registration).unwrap();
        let prior = application_facts(&registration).unwrap();
        fs::write(&replacement, "[Desktop Entry]\nName=Changed\n").unwrap();
        fs::rename(&replacement, &target).unwrap();
        assert_ne!(application_facts(&registration).unwrap(), prior);
    }

    #[test]
    fn file_uri_encodes_unusual_names_without_shell_interpretation() {
        assert_eq!(uri(Path::new("/tmp/a # $b")), "file:///tmp/a%20%23%20%24b");
    }
}
