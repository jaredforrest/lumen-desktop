//! Private, generation-scoped Google calendar cache for the native EDS adapter.
use chrono::{Datelike, NaiveDate, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::eds;

const MAX_RESPONSE: u64 = 8 * 1024 * 1024;

fn read_json(path: &Path, default: Value) -> Value {
    let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    else {
        return default;
    };
    if !file
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_RESPONSE)
    {
        return default;
    }
    let mut bytes = Vec::new();
    if file.take(MAX_RESPONSE + 1).read_to_end(&mut bytes).is_err()
        || bytes.len() as u64 > MAX_RESPONSE
    {
        return default;
    }
    serde_json::from_slice(&bytes).unwrap_or(default)
}

fn private_write(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid calendar cache path")?;
    fs::create_dir_all(parent).map_err(|_| "The calendar cache could not be written")?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|_| "The calendar cache could not be written")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "The calendar cache could not be written")?;
    serde_json::to_writer(&mut temporary, value)
        .map_err(|_| "The calendar cache could not be written")?;
    temporary
        .flush()
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| "The calendar cache could not be written")?;
    temporary
        .persist(path)
        .map_err(|_| "The calendar cache could not be written")?;
    Ok(())
}

fn source_file(settings: &Path) -> PathBuf {
    settings.join("sources.json")
}
fn sources(settings: &Path) -> Value {
    read_json(
        &source_file(settings),
        json!({"accounts":[],"calendars":[]}),
    )
}

/// Match Python's JSON spelling for the sorted account/source fingerprint.
pub fn generation(source_manifest: &Value) -> String {
    if !source_manifest["accounts"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
    {
        return String::new();
    }
    let mut pairs: Vec<(&str, &str)> = source_manifest["calendars"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| Some((item["accountId"].as_str()?, item["id"].as_str()?)))
        .collect();
    pairs.sort_unstable();
    let parts: Vec<String> = pairs
        .iter()
        .map(|(account, id)| {
            format!(
                "[{}, {}]",
                serde_json::to_string(account).unwrap(),
                serde_json::to_string(id).unwrap()
            )
        })
        .collect();
    let encoded = format!("[{}]", parts.join(", "));
    hex::encode(Sha256::digest(encoded.as_bytes()))
}

pub fn metadata_dir(settings: &Path) -> PathBuf {
    settings.join("eds").join({
        let fingerprint = generation(&sources(settings));
        if fingerprint.is_empty() {
            "unconfigured".into()
        } else {
            fingerprint
        }
    })
}

pub fn month_file(root: &Path, month: NaiveDate) -> PathBuf {
    metadata_dir(&root.join("google-settings"))
        .join("months")
        .join(format!("{:04}-{:02}.json", month.year(), month.month()))
}

pub fn read_month(root: &Path, month: NaiveDate) -> Value {
    read_json(&month_file(root, month), json!({"calendars":{}}))
}

pub fn cached_status(settings: &Path) -> Value {
    status_with(settings, false, || unreachable!())
}

fn status_with<D>(settings: &Path, refresh: bool, discover: D) -> Value
where
    D: FnOnce() -> Result<Value, String>,
{
    let mut native_error = false;
    let manifest = if refresh {
        match discover().and_then(|value| refresh_manifest(settings, value)) {
            Ok(value) => value,
            Err(_) => {
                native_error = true;
                sources(settings)
            }
        }
    } else {
        sources(settings)
    };
    let saved = read_json(&metadata_dir(settings).join("status.json"), json!({}));
    let accounts = manifest["accounts"].as_array();
    let calendars = manifest["calendars"].as_array();
    let mut error = if native_error {
        "Online Accounts could not be checked. Previously downloaded events remain available."
            .to_owned()
    } else {
        saved["error"].as_str().unwrap_or("").to_owned()
    };
    if accounts.is_some_and(|items| !items.is_empty()) && calendars.is_some_and(Vec::is_empty) {
        error = if manifest["pendingDiscovery"] != false {
            "Google is connected. Waiting for calendar discovery; try Sync Now shortly."
        } else {
            "Google is connected, but no calendars are enabled in the calendar service."
        }
        .into();
    }
    if accounts
        .into_iter()
        .flatten()
        .any(|item| item["attentionNeeded"] == true)
    {
        error = "Google needs you to sign in again. Open Google Setup.".into();
    }
    json!({
        "configured":manifest["accounts"].as_array().is_some_and(|items|!items.is_empty()),
        "calendars":manifest["calendars"],"generation":generation(&manifest),
        "lastSuccess":saved["lastSuccess"].as_str().unwrap_or(""),
        "error":error,"mode":"eds"
    })
}

pub fn status(settings: &Path, refresh: bool) -> Value {
    status_with(settings, refresh, eds::discover)
}

pub(crate) fn refresh_manifest(settings: &Path, mut discovered: Value) -> Result<Value, String> {
    let previous = sources(settings);
    eds::preserve_access_roles(&previous, &mut discovered);
    if previous != discovered {
        private_write(&source_file(settings), &discovered)?;
    }
    Ok(discovered)
}

fn sync_month_with<D, L>(root: &Path, month: NaiveDate, discover: D, load: L) -> Result<(), String>
where
    D: FnOnce() -> Result<Value, String>,
    L: Fn(&str) -> Result<Value, String>,
{
    let settings = root.join("google-settings");
    let discovered = discover()?;
    let manifest = refresh_manifest(&settings, discovered)?;
    let calendars = manifest["calendars"]
        .as_array()
        .ok_or("Invalid calendar source list")?;
    if !manifest["accounts"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
    {
        return Ok(());
    }
    if manifest["pendingDiscovery"] == true && calendars.is_empty() {
        return Err(
            "Google is connected. Waiting for calendar discovery; try Sync Now shortly.".into(),
        );
    }
    fs::create_dir_all(&settings).map_err(|_| "The calendar cache could not be written")?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .open(settings.join("sync.lock"))
        .map_err(|_| "The calendar cache could not be written")?;
    // SAFETY: flock only borrows the live file descriptor for this process.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("A calendar sync is already running.".into());
    }
    let directory = metadata_dir(&settings);
    let file = month_file(root, month);
    let previous = read_json(&file, json!({"calendars":{}}));
    let old_status = cached_status(&settings);
    let mut result = json!({"calendars":{},"month":month.to_string()});
    let mut updated = manifest.clone();
    let mut errors = Vec::new();
    for calendar in calendars {
        let Some(uid) = calendar["id"].as_str() else {
            continue;
        };
        match load(uid) {
            Ok(loaded) => {
                let mut entry = calendar.clone();
                entry["accessRole"] = loaded["accessRole"].clone();
                entry["events"] = loaded["events"].clone();
                result["calendars"][uid] = entry.clone();
                if let Some(list) = updated["calendars"].as_array_mut() {
                    if let Some(item) = list.iter_mut().find(|item| item["id"] == uid) {
                        item["accessRole"] = entry["accessRole"].clone();
                    }
                }
                if loaded["online"] != true {
                    errors.push(
                        "Google is offline or still syncing; displaying calendar service cache.",
                    );
                }
            }
            Err(_) => {
                errors
                    .push("A Google calendar could not sync. Its previous download was retained.");
                if !previous["calendars"][uid].is_null() {
                    result["calendars"][uid] = previous["calendars"][uid].clone();
                }
            }
        }
    }
    if serde_json::to_vec(&result).is_ok_and(|encoded| encoded.len() as u64 > MAX_RESPONSE) {
        return Err(
            "This month exceeds the supported cache size. Previous events were kept.".into(),
        );
    }
    private_write(&file, &result)?;
    if generation(&sources(&settings))
        == directory.file_name().unwrap_or_default().to_string_lossy()
    {
        private_write(&source_file(&settings), &updated)?;
    }
    let last_success = if errors.is_empty() {
        Utc::now().to_rfc3339()
    } else {
        old_status["lastSuccess"].as_str().unwrap_or("").to_owned()
    };
    errors.sort_unstable();
    errors.dedup();
    private_write(
        &directory.join("status.json"),
        &json!({"lastSuccess":last_success,"error":errors.join("; ")}),
    )?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(
            "Some Google calendars are offline or still syncing. Cached events are available."
                .into(),
        )
    }
}

pub fn sync_month(root: &Path, month: NaiveDate, refresh_remote: bool) -> Result<(), String> {
    #[cfg(feature = "color")]
    let tz = std::env::var("LUMEN_CALENDAR_TIMEZONE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(chrono_tz::Europe::London);
    #[cfg(not(feature = "color"))]
    let tz = chrono_tz::Europe::London;
    sync_month_with(root, month, eds::discover, |uid| {
        eds::load(uid, month, tz, refresh_remote)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_matches_existing_cache_generation() {
        let value = json!({"accounts":[{"id":"account"}],"calendars":[{"accountId":"account","id":"calendar"}]});
        assert_eq!(
            generation(&value),
            "0ca1d9f62d295250b019c1d6474dba31f28c695ad38f4e1266b19872bbcf96fa"
        );
    }
    #[test]
    fn failed_sync_keeps_previous_download_and_success_time() {
        let root = tempfile::tempdir().unwrap();
        let month = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let discovery = || {
            Ok(
                json!({"accounts":[{"id":"account"}],"calendars":[{"id":"calendar","accountId":"account","name":"Fixture","accessRole":"unknown"}],"pendingDiscovery":false}),
            )
        };
        sync_month_with(root.path(),month,discovery,|_|Ok(json!({"events":[{"raw":"fixture","ical":"fixture"}],"accessRole":"writer","online":true}))).unwrap();
        let saved = read_json(&month_file(root.path(), month), json!({}));
        let bytes = fs::read(month_file(root.path(), month)).unwrap();
        let success = cached_status(&root.path().join("google-settings"))["lastSuccess"].clone();
        assert!(!success.as_str().unwrap().is_empty());
        let failed = sync_month_with(root.path(), month, discovery, |_| Err("offline".into()));
        assert!(failed.is_err());
        let retained = read_json(&month_file(root.path(), month), json!({}));
        assert_eq!(fs::read(month_file(root.path(), month)).unwrap(), bytes);
        assert_eq!(
            retained["calendars"]["calendar"]["events"],
            saved["calendars"]["calendar"]["events"]
        );
        assert_eq!(
            cached_status(&root.path().join("google-settings"))["lastSuccess"],
            success
        );
    }
    #[test]
    fn status_distinguishes_pending_and_disabled_sources() {
        let root = tempfile::tempdir().unwrap();
        let settings = root.path();
        let pending = status_with(settings, true, || {
            Ok(json!({"accounts":[{"id":"account"}],"calendars":[],"pendingDiscovery":true}))
        });
        assert_eq!(pending["configured"], true);
        assert!(pending["error"].as_str().unwrap().contains("discovery"));
        let disabled = status_with(settings, true, || {
            Ok(json!({"accounts":[{"id":"account"}],"calendars":[],"pendingDiscovery":false}))
        });
        assert!(
            disabled["error"]
                .as_str()
                .unwrap()
                .contains("no calendars are enabled")
        );
        let disconnected =
            status_with(settings, true, || Ok(json!({"accounts":[],"calendars":[]})));
        assert_eq!(disconnected["configured"], false);
    }
    #[test]
    fn empty_first_download_can_be_filled_on_next_local_refresh() {
        let root = tempfile::tempdir().unwrap();
        let month = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let discovery = || {
            Ok(
                json!({"accounts":[{"id":"account"}],"calendars":[{"id":"calendar","accountId":"account","name":"Fixture","accessRole":"unknown"}],"pendingDiscovery":false}),
            )
        };
        sync_month_with(root.path(), month, discovery, |_| {
            Ok(json!({"events":[],"accessRole":"writer","online":true}))
        })
        .unwrap();
        assert_eq!(
            read_month(root.path(), month)["calendars"]["calendar"]["events"],
            json!([])
        );
        sync_month_with(root.path(), month, discovery, |_| Ok(json!({"events":[{"raw":"fixture","ical":"fixture"}],"accessRole":"writer","online":true}))).unwrap();
        assert_eq!(
            read_month(root.path(), month)["calendars"]["calendar"]["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
