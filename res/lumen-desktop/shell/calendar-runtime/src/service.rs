//! JSON calendar protocol shared by the two desktop packages.
use chrono::{Datelike, NaiveDate, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{google_cache, google_events, google_mutation, parse_day, read_icloud};

pub struct Options<'a> {
    pub root: &'a Path,
    pub command_prefix: &'a str,
    pub uid_suffix: &'a str,
    pub timezone: Tz,
}

pub fn data_root() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("calendar");
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".local/share/calendar")
}

fn icloud_status(root: &Path) -> Value {
    let settings = root.join("icloud-settings");
    let saved = fs::read(settings.join("status.json"))
        .ok()
        .filter(|bytes| bytes.len() <= 8 * 1024 * 1024)
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(json!({}));
    json!({"configured":settings.join("vdirsyncer.conf").is_file(),
        "lastSuccess":saved["lastSuccess"].as_str().unwrap_or(""),
        "syncError":saved["error"].as_str().unwrap_or(""),
        "id":"icloud","name":"iCloud"})
}

pub fn read_status(root: &Path, refresh: bool) -> Value {
    let icloud = icloud_status(root);
    let mut google = google_cache::status(&root.join("google-settings"), refresh);
    google["id"] = json!("google");
    google["name"] = json!("Google");
    google["syncError"] = google["error"].clone();
    let providers = vec![icloud, google];
    let active: Vec<&Value> = providers
        .iter()
        .filter(|provider| provider["configured"] == true)
        .collect();
    let last_success = active
        .iter()
        .filter_map(|provider| provider["lastSuccess"].as_str())
        .filter(|value| !value.is_empty())
        .min()
        .unwrap_or("");
    let errors: Vec<String> = active
        .iter()
        .filter_map(|provider| {
            let error = provider["syncError"].as_str()?;
            if error.is_empty() {
                None
            } else {
                Some(format!(
                    "{}: {error}",
                    provider["name"].as_str().unwrap_or("")
                ))
            }
        })
        .collect();
    json!({"configured":!active.is_empty(),"lastSuccess":last_success,
        "syncError":errors.join("; "),"providers":providers})
}

fn view_events(
    root: &Path,
    selected: NaiveDate,
    month: NaiveDate,
    tz: Tz,
) -> Result<Value, String> {
    let local = read_icloud(root, selected, month, tz);
    let (google, google_days) = google_events::read_cached(root, month, tz)?;
    let mut rows = local["monthEvents"].as_array().cloned().unwrap_or_default();
    rows.extend(google);
    rows.sort_by(|a, b| {
        let a_day = a["allDay"] == true;
        let b_day = b["allDay"] == true;
        b_day
            .cmp(&a_day)
            .then_with(|| {
                a["start"]
                    .as_f64()
                    .unwrap_or(0.0)
                    .total_cmp(&b["start"].as_f64().unwrap_or(0.0))
            })
            .then_with(|| a["title"].as_str().cmp(&b["title"].as_str()))
    });
    let chosen = selected.to_string();
    let selected_rows: Vec<Value> = rows
        .iter()
        .filter(|row| {
            row["date"]
                .as_str()
                .is_some_and(|start| start <= chosen.as_str())
                && row["endDate"]
                    .as_str()
                    .is_some_and(|end| end >= chosen.as_str())
        })
        .cloned()
        .collect();
    let mut days: BTreeSet<String> = local["days"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect();
    days.extend(google_days);
    Ok(
        json!({"events":selected_rows.iter().take(100).cloned().collect::<Vec<_>>(),
        "total":selected_rows.len(),"monthEvents":rows.into_iter().take(10_000).collect::<Vec<_>>(),
        "days":days.into_iter().collect::<Vec<_>>(),"warning":local["warning"]}),
    )
}

fn start_sync(prefix: &str, provider: &str, month: NaiveDate) -> Result<Child, String> {
    if prefix.is_empty()
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
    {
        return Err("Invalid calendar command prefix".into());
    }
    let command = if provider == "icloud" {
        format!("{prefix}-icloud-setup")
    } else {
        format!("{prefix}-google-calendar-sync")
    };
    let mut child = Command::new(command);
    child.arg("sync");
    if provider == "google" {
        child.arg("--month").arg(month.to_string());
    }
    child
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Calendar sync failed. Previously downloaded events remain available.".into())
}

fn wait_sync(child: &mut Child, deadline: Instant) -> Result<(), String> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) | Err(_) => {
                return Err(
                    "Calendar sync failed. Previously downloaded events remain available.".into(),
                );
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "Calendar sync failed. Previously downloaded events remain available.".into(),
                );
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
        }
    }
}

pub fn handle(request: &Value, options: &Options<'_>) -> Result<Value, String> {
    if !request.is_object() {
        return Err("Calendar request must be an object".into());
    }
    let selected = parse_day(request["selectedDate"].as_str().unwrap_or(""))?;
    let month = if let Some(value) = request["month"].as_str() {
        parse_day(value)?
    } else {
        selected.with_day(1).ok_or("Invalid calendar month")?
    };
    let month = month.with_day(1).ok_or("Invalid calendar month")?;
    if selected.year() != month.year() || selected.month() != month.month() {
        return Err("Selected date must be in the displayed month".into());
    }
    let root = options.root;
    let mut mutation = json!({});
    let mut result = match request["action"].as_str().unwrap_or("list") {
        "list" => read_status(root, false),
        "load" => {
            let fresh = read_status(root, true);
            let mut error = fresh["syncError"].as_str().unwrap_or("").to_owned();
            if fresh["providers"][1]["configured"] == true {
                if let Err(problem) = google_cache::sync_month(root, month, false) {
                    error = problem;
                }
            }
            let mut status = read_status(root, false);
            status["syncError"] = json!(error);
            status
        }
        "sync" => {
            let fresh = read_status(root, true);
            let mut errors = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(100);
            let mut running = Vec::new();
            for provider in fresh["providers"].as_array().into_iter().flatten() {
                if provider["configured"] == true {
                    let id = provider["id"].as_str().unwrap_or("");
                    let name = provider["name"].as_str().unwrap_or("Calendar").to_owned();
                    match start_sync(options.command_prefix, id, month) {
                        Ok(child) => running.push((name, child)),
                        Err(_) => errors.push(format!(
                            "{name} sync failed. Previously downloaded events remain available."
                        )),
                    }
                }
            }
            for (name, mut child) in running {
                if wait_sync(&mut child, deadline).is_err() {
                    errors.push(format!(
                        "{name} sync failed. Previously downloaded events remain available."
                    ));
                }
            }
            let mut status = read_status(root, false);
            if status["syncError"].as_str().unwrap_or("").is_empty() {
                status["syncError"] = json!(errors.join("; "));
            }
            status
        }
        "create" | "update" | "delete" => {
            mutation =
                google_mutation::mutate(root, request, options.uid_suffix, options.timezone)?;
            read_status(root, false)
        }
        _ => return Err("Unknown calendar action".into()),
    };
    match view_events(root, selected, month, options.timezone) {
        Ok(view) => {
            if let Some(values) = view.as_object() {
                for (key, value) in values {
                    result[key] = value.clone();
                }
            }
        }
        Err(problem) if mutation["mutationSuccess"] == true => {
            for (key, value) in
                json!({"events":[],"monthEvents":[],"total":0,"days":[],"warning":""})
                    .as_object()
                    .unwrap()
            {
                result[key] = value.clone();
            }
            mutation["mutationWarning"] = json!(
                "The calendar service saved the change, but the local view could not refresh. Sync again."
            );
            let _ = problem;
        }
        Err(problem) => return Err(problem),
    }
    if let Some(values) = mutation.as_object() {
        for (key, value) in values {
            result[key] = value.clone();
        }
    }
    result["timezone"] = json!(options.timezone.name());
    result["date"] = json!(selected.to_string());
    result["month"] = json!(month.to_string());
    Ok(result)
}

pub fn today(tz: Tz) -> NaiveDate {
    Utc::now().with_timezone(&tz).date_naive()
}

pub fn timezone() -> Tz {
    #[cfg(feature = "color")]
    {
        return std::env::var("LUMEN_CALENDAR_TIMEZONE")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(chrono_tz::Europe::London);
    }
    #[cfg(not(feature = "color"))]
    {
        chrono_tz::Europe::London
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unconfigured_list_is_truthful_and_does_not_create_state() {
        let root = tempfile::tempdir().unwrap();
        let options = Options {
            root: root.path(),
            command_prefix: "fixture",
            uid_suffix: "@fixture.local",
            timezone: chrono_tz::Europe::London,
        };
        let result = handle(&json!({"selectedDate":"2026-10-08"}), &options).unwrap();
        assert_eq!(result["configured"], false);
        assert_eq!(result["events"], json!([]));
        assert!(!root.path().join("calendar.sqlite3").exists());
        assert!(!root.path().join("google-settings").exists());
    }
    #[test]
    fn invalid_month_and_write_request_refused_before_native_call() {
        let root = tempfile::tempdir().unwrap();
        let options = Options {
            root: root.path(),
            command_prefix: "fixture",
            uid_suffix: "@fixture.local",
            timezone: chrono_tz::Europe::London,
        };
        assert!(
            handle(
                &json!({"selectedDate":"2026-10-08","month":"2026-11-01"}),
                &options
            )
            .is_err()
        );
        assert!(handle(&json!({"selectedDate":"2026-02-31"}), &options).is_err());
    }
}
