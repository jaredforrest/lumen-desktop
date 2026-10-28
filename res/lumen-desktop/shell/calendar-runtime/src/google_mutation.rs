//! Fail-closed Google calendar edits with exact generation and source revision.
use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

use crate::{eds, google_cache, google_edit, google_events, parse_day};

pub trait Backend {
    fn discover(&mut self) -> Result<Value, String>;
    fn current(&mut self, calendar: &str, uid: &str, rid: &str) -> Result<String, String>;
    fn create(&mut self, calendar: &str, ical: &str) -> Result<(), String>;
    fn update(&mut self, calendar: &str, ical: &str, recurring: bool) -> Result<(), String>;
    fn delete(&mut self, calendar: &str, uid: &str, rid: &str) -> Result<(), String>;
    fn sync(&mut self, root: &Path, month: NaiveDate) -> Result<(), String>;
}

#[derive(Default)]
pub struct NativeBackend {
    client: Option<(String, eds::WriteClient)>,
}
impl NativeBackend {
    fn client(&mut self, uid: &str) -> Result<&eds::WriteClient, String> {
        if self.client.as_ref().is_none_or(|(old, _)| old != uid) {
            self.client = Some((uid.to_owned(), eds::WriteClient::connect(uid)?));
        }
        Ok(&self.client.as_ref().unwrap().1)
    }
}
impl Backend for NativeBackend {
    fn discover(&mut self) -> Result<Value, String> {
        eds::discover()
    }
    fn current(&mut self, calendar: &str, uid: &str, rid: &str) -> Result<String, String> {
        self.client(calendar)?.current(uid, rid)
    }
    fn create(&mut self, calendar: &str, ical: &str) -> Result<(), String> {
        self.client(calendar)?.create(ical).map(|_| ())
    }
    fn update(&mut self, calendar: &str, ical: &str, recurring: bool) -> Result<(), String> {
        self.client(calendar)?.update(ical, recurring)
    }
    fn delete(&mut self, calendar: &str, uid: &str, rid: &str) -> Result<(), String> {
        self.client(calendar)?.delete(uid, rid)
    }
    fn sync(&mut self, root: &Path, month: NaiveDate) -> Result<(), String> {
        google_cache::sync_month(root, month, false)
    }
}

fn displayed_row(
    root: &Path,
    request: &Value,
    month: NaiveDate,
    selected: NaiveDate,
    tz: Tz,
) -> Result<Value, String> {
    let (rows, _) = google_events::read_cached(root, month, tz)?;
    let selected = selected.to_string();
    rows.into_iter()
        .find(|row| {
            row["date"]
                .as_str()
                .is_some_and(|start| start <= selected.as_str())
                && row["endDate"]
                    .as_str()
                    .is_some_and(|end| end >= selected.as_str())
                && row["calendarId"] == request["calendarId"]
                && row["eventId"] == request["eventId"]
                && row["recurrenceId"].as_str() == request["recurrenceId"].as_str().or(Some(""))
        })
        .ok_or_else(|| {
            "Sync and reopen this event before editing; some event types require Google Calendar."
                .into()
        })
}

fn invalidate_other_months(root: &Path, month: NaiveDate) -> Result<(), String> {
    let displayed = google_cache::month_file(root, month);
    let Some(parent) = displayed.parent() else {
        return Ok(());
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != displayed && path.extension().is_some_and(|value| value == "json") {
            fs::remove_file(path).map_err(|_| "The calendar cache could not be updated")?;
        }
    }
    Ok(())
}

pub fn mutate_with<B: Backend>(
    root: &Path,
    request: &Value,
    uid_suffix: &str,
    tz: Tz,
    backend: &mut B,
) -> Result<Value, String> {
    let settings = root.join("google-settings");
    let discovered = backend.discover()?;
    let manifest = google_cache::refresh_manifest(&settings, discovered)?;
    let generation = google_cache::generation(&manifest);
    if request["accountGeneration"].as_str() != Some(generation.as_str()) {
        return Err("Connected calendars changed. Reopen this event before editing.".into());
    }
    let calendar_id = request["calendarId"]
        .as_str()
        .ok_or("Choose a writable Google calendar.")?;
    let calendar = manifest["calendars"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|value| value["id"].as_str() == Some(calendar_id))
        .ok_or("Choose a writable Google calendar.")?;
    if !matches!(calendar["accessRole"].as_str(), Some("owner" | "writer")) {
        return Err("Choose a writable Google calendar.".into());
    }
    let month = parse_day(request["month"].as_str().unwrap_or(""))?;
    let selected = parse_day(request["selectedDate"].as_str().unwrap_or(""))?;
    if month.day() != 1 || month.year() != selected.year() || month.month() != selected.month() {
        return Err("Selected date must be in the displayed month".into());
    }
    match request["action"].as_str().unwrap_or("") {
        "create" => {
            let ical = google_edit::create_event(
                request["operationId"].as_str().unwrap_or(""),
                uid_suffix,
                &request["fields"],
                tz,
            )?;
            backend.create(calendar_id, &ical)?;
        }
        "update" | "delete" => {
            let row = displayed_row(root, request, month, selected, tz)?;
            if row["editable"] != true || row["etag"] != request["etag"] {
                return Err("Sync and reopen this event before editing; some event types require Google Calendar.".into());
            }
            let uid = row["eventId"].as_str().ok_or("Invalid event identity")?;
            let source_rid = row["sourceRecurrenceId"].as_str().unwrap_or("");
            let current = backend.current(calendar_id, uid, source_rid)?;
            if google_events::fingerprint(&current) != row["etag"].as_str().unwrap_or("") {
                return Err(
                    "This event changed elsewhere. Sync and reopen it before editing.".into(),
                );
            }
            if request["action"] == "delete" {
                if request["confirmDelete"] != true {
                    return Err("Confirm deletion of the selected event first.".into());
                }
                backend.delete(calendar_id, uid, row["recurrenceId"].as_str().unwrap_or(""))?;
            } else {
                let generated = if row["recurring"] == true && source_rid.is_empty() {
                    row["recurrenceId"].as_str()
                } else {
                    None
                };
                let ical = google_edit::patch_event(&current, &request["fields"], tz, generated)?;
                backend.update(calendar_id, &ical, row["recurring"] == true)?;
            }
        }
        _ => return Err("Unknown calendar action.".into()),
    }
    let warning = if invalidate_other_months(root, month)
        .and_then(|_| backend.sync(root, month))
        .is_err()
    {
        "The calendar service saved the change. Google sync may still be pending; try Sync Now."
    } else {
        ""
    };
    Ok(json!({"mutationSuccess":true,"mutationWarning":warning}))
}

pub fn mutate(root: &Path, request: &Value, uid_suffix: &str, tz: Tz) -> Result<Value, String> {
    mutate_with(root, request, uid_suffix, tz, &mut NativeBackend::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    const RAW: &str = "BEGIN:VEVENT\nUID:fixture\nDTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nSUMMARY:Appointment\nX-EVOLUTION-CALDAV-ETAG:version-one\nEND:VEVENT\n";
    fn manifest() -> Value {
        json!({"accounts":[{"id":"account"}],"calendars":[{"id":"source","accountId":"account","name":"Fixture","accessRole":"writer"}],"pendingDiscovery":false})
    }
    struct Fake {
        raw: String,
        created: Vec<String>,
        updated: Vec<String>,
        update_recurring: Vec<bool>,
        deleted: Vec<String>,
        sync_fail: bool,
    }
    impl Fake {
        fn new() -> Self {
            Self {
                raw: RAW.into(),
                created: Vec::new(),
                updated: Vec::new(),
                update_recurring: Vec::new(),
                deleted: Vec::new(),
                sync_fail: false,
            }
        }
    }
    impl Backend for Fake {
        fn discover(&mut self) -> Result<Value, String> {
            Ok(manifest())
        }
        fn current(&mut self, _: &str, _: &str, _: &str) -> Result<String, String> {
            Ok(self.raw.clone())
        }
        fn create(&mut self, _: &str, ical: &str) -> Result<(), String> {
            self.created.push(ical.into());
            Ok(())
        }
        fn update(&mut self, _: &str, ical: &str, recurring: bool) -> Result<(), String> {
            self.updated.push(ical.into());
            self.update_recurring.push(recurring);
            Ok(())
        }
        fn delete(&mut self, _: &str, uid: &str, rid: &str) -> Result<(), String> {
            self.deleted.push(format!("{uid}:{rid}"));
            Ok(())
        }
        fn sync(&mut self, _: &Path, _: NaiveDate) -> Result<(), String> {
            if self.sync_fail {
                Err("offline".into())
            } else {
                Ok(())
            }
        }
    }
    fn fixture_with(raw: &str) -> (tempfile::TempDir, Value) {
        let root = tempfile::tempdir().unwrap();
        let settings = root.path().join("google-settings");
        fs::create_dir_all(&settings).unwrap();
        fs::write(
            settings.join("sources.json"),
            serde_json::to_vec(&manifest()).unwrap(),
        )
        .unwrap();
        let month = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let path = google_cache::month_file(root.path(), month);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path,serde_json::to_vec(&json!({"calendars":{"source":{"id":"source","name":"Fixture","accessRole":"writer","events":[{"raw":raw,"ical":""}]}}})).unwrap()).unwrap();
        let (rows, _) =
            google_events::read_cached(root.path(), month, chrono_tz::Europe::London).unwrap();
        let mut request = rows
            .iter()
            .find(|row| row["date"] == "2026-10-08")
            .unwrap()
            .clone();
        request["month"] = json!(month.to_string());
        request["selectedDate"] = json!("2026-10-08");
        request["fields"] = json!({"title":"Updated","notes":"Note","location":"Room","allDay":false,"start":"2026-10-08T11:00","end":"2026-10-08T12:00"});
        request["operationId"] = json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        (root, request)
    }
    fn fixture() -> (tempfile::TempDir, Value) {
        fixture_with(RAW)
    }
    #[test]
    fn update_preserves_revision_and_stale_remote_is_refused() {
        let (root, mut request) = fixture();
        let mut backend = Fake::new();
        request["action"] = json!("update");
        let saved = mutate_with(
            root.path(),
            &request,
            "@fixture.local",
            chrono_tz::Europe::London,
            &mut backend,
        )
        .unwrap();
        assert_eq!(saved["mutationSuccess"], true);
        assert_eq!(backend.updated.len(), 1);
        assert!(backend.updated[0].contains("X-EVOLUTION-CALDAV-ETAG:version-one"));
        let mut stale = Fake::new();
        stale.raw = RAW.replace("version-one", "version-two");
        assert!(
            mutate_with(
                root.path(),
                &request,
                "@fixture.local",
                chrono_tz::Europe::London,
                &mut stale
            )
            .unwrap_err()
            .contains("changed elsewhere")
        );
        assert!(stale.updated.is_empty());
    }
    #[test]
    fn delete_needs_confirmation_and_create_has_stable_uid() {
        let (root, mut request) = fixture();
        let mut backend = Fake::new();
        request["action"] = json!("delete");
        assert!(
            mutate_with(
                root.path(),
                &request,
                "@fixture.local",
                chrono_tz::Europe::London,
                &mut backend
            )
            .unwrap_err()
            .contains("Confirm deletion")
        );
        request["confirmDelete"] = json!(true);
        mutate_with(
            root.path(),
            &request,
            "@fixture.local",
            chrono_tz::Europe::London,
            &mut backend,
        )
        .unwrap();
        assert_eq!(backend.deleted, vec!["fixture:"]);
        request["action"] = json!("create");
        mutate_with(
            root.path(),
            &request,
            "@fixture.local",
            chrono_tz::Europe::London,
            &mut backend,
        )
        .unwrap();
        assert!(backend.created[0].contains("UID:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@fixture.local"));
    }
    #[test]
    fn accepted_edit_stays_success_when_cache_refresh_fails() {
        let (root, mut request) = fixture();
        let mut backend = Fake::new();
        backend.sync_fail = true;
        request["action"] = json!("update");
        let result = mutate_with(
            root.path(),
            &request,
            "@fixture.local",
            chrono_tz::Europe::London,
            &mut backend,
        )
        .unwrap();
        assert_eq!(result["mutationSuccess"], true);
        assert!(
            result["mutationWarning"]
                .as_str()
                .unwrap()
                .contains("pending")
        );
    }
    #[test]
    fn generated_occurrence_update_is_scoped_to_this_instance() {
        let raw = "BEGIN:VEVENT\nUID:fixture\nDTSTART:20261001T080000Z\nDTEND:20261001T090000Z\nRRULE:FREQ=WEEKLY;COUNT=3\nSUMMARY:Master\nX-EVOLUTION-CALDAV-ETAG:version-one\nEND:VEVENT\n";
        let (root, mut request) = fixture_with(raw);
        request["action"] = json!("update");
        let mut backend = Fake::new();
        backend.raw = raw.into();
        mutate_with(
            root.path(),
            &request,
            "@fixture.local",
            chrono_tz::Europe::London,
            &mut backend,
        )
        .unwrap();
        assert_eq!(backend.update_recurring, vec![true]);
        assert!(backend.updated[0].contains("RECURRENCE-ID:20261008T080000Z"));
        assert!(!backend.updated[0].contains("RRULE:"));
        assert!(backend.updated[0].contains("X-EVOLUTION-CALDAV-ETAG:version-one"));
    }
}
