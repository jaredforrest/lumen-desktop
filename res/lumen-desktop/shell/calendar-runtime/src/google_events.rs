//! Render bounded Google cache entries without accessing the network or tokens.
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Timelike};
use chrono_tz::Tz;
use icalendar::{Calendar, CalendarDateTime, Component, DatePerhapsTime, EventLike};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::str::FromStr;

use crate::{bounded, end_for, local, localize, next_month};
use crate::{eds, google_cache};

const MAX_EVENTS: usize = 10_000;
const DISPLAY_PROPERTIES: &[&str] = &[
    "UID",
    "DTSTART",
    "DTEND",
    "DURATION",
    "RRULE",
    "RDATE",
    "EXDATE",
    "EXRULE",
    "RECURRENCE-ID",
    "STATUS",
    "SUMMARY",
    "DESCRIPTION",
    "LOCATION",
];

fn unfolded(raw: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if (line.starts_with(' ') || line.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&line[1..]);
        } else {
            lines.push(line.to_owned());
        }
    }
    lines
}

/// Keep the recurrence/display fields while excluding vendor properties and
/// nested alarms that the narrower recurrence parser cannot interpret.
fn display_component(raw: &str) -> Option<String> {
    let mut result = String::new();
    let mut in_event = false;
    let mut nested = 0usize;
    for line in unfolded(raw) {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VEVENT" {
            in_event = true;
            result.push_str("BEGIN:VEVENT\n");
            continue;
        }
        if !in_event {
            continue;
        }
        if upper.starts_with("BEGIN:") {
            nested += 1;
            continue;
        }
        if upper.starts_with("END:") && upper != "END:VEVENT" {
            nested = nested.saturating_sub(1);
            continue;
        }
        if upper == "END:VEVENT" && nested == 0 {
            result.push_str("END:VEVENT\n");
            return Some(result);
        }
        if nested > 0 {
            continue;
        }
        let Some((head, _)) = line.split_once(':') else {
            continue;
        };
        let key = head.split(';').next()?.to_ascii_uppercase();
        if DISPLAY_PROPERTIES.contains(&key.as_str()) {
            result.push_str(&line);
            result.push('\n');
        }
    }
    None
}

fn has_property(raw: &str, name: &str) -> bool {
    unfolded(raw).iter().any(|line| {
        line.split_once(':').is_some_and(|(head, _)| {
            head.split(';')
                .next()
                .is_some_and(|key| key.eq_ignore_ascii_case(name))
        })
    })
}

fn recurrence_id(start: DateTime<Tz>, value: &DatePerhapsTime, tz: Tz) -> String {
    match value {
        DatePerhapsTime::Date(_) => start.format("%Y%m%d").to_string(),
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(_)) => start
            .with_timezone(&chrono::Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string(),
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(_)) => {
            start.with_timezone(&tz).format("%Y%m%dT%H%M%S").to_string()
        }
        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { tzid, .. }) => {
            let source = tzid.parse::<Tz>().unwrap_or(tz);
            start
                .with_timezone(&source)
                .format("%Y%m%dT%H%M%S")
                .to_string()
        }
    }
}

fn editable(role: &str, event: &impl EventLike, original: &str) -> bool {
    if !matches!(role, "writer" | "owner")
        || original.is_empty()
        || has_property(original, "ATTENDEE")
        || has_property(original, "X-GOOGLE-EVENT-TYPE")
    {
        return false;
    }
    if [("SUMMARY", 1000), ("DESCRIPTION", 8000), ("LOCATION", 1000)]
        .iter()
        .any(|(key, limit)| {
            event
                .property_value(key)
                .is_some_and(|value| value.chars().count() > *limit)
        })
    {
        return false;
    }
    for value in [event.get_start(), event.get_end()].into_iter().flatten() {
        let timed = !matches!(value, DatePerhapsTime::Date(_));
        if let Some(date) = local(value, chrono_tz::UTC) {
            if timed && (date.second() != 0 || date.nanosecond() != 0) {
                return false;
            }
        }
    }
    true
}

pub(crate) fn fingerprint(text: &str) -> String {
    hex::encode(Sha256::digest(text.replace("\r\n", "\n").as_bytes()))
}

/// Return month rows and marked dates from the saved generation only.
pub fn read_cached(
    root: &Path,
    month: NaiveDate,
    tz: Tz,
) -> Result<(Vec<Value>, BTreeSet<String>), String> {
    let cache = google_cache::read_month(root, month);
    let generation = google_cache::metadata_dir(&root.join("google-settings"))
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    rows_from_snapshot(&cache, &generation, month, tz)
}

/// Render an already fetched bounded snapshot, useful before publishing it.
pub fn rows_from_snapshot(
    cache: &Value,
    generation: &str,
    month: NaiveDate,
    tz: Tz,
) -> Result<(Vec<Value>, BTreeSet<String>), String> {
    let lo = localize(tz, month.and_time(NaiveTime::MIN)).ok_or("Invalid calendar month")?;
    let hi =
        localize(tz, next_month(month).and_time(NaiveTime::MIN)).ok_or("Invalid calendar month")?;
    let mut rows = Vec::new();
    let mut marks = BTreeSet::new();
    let mut count = 0;
    let Some(calendars) = cache["calendars"].as_object() else {
        return Ok((rows, marks));
    };
    for (calendar_id, source) in calendars {
        let name = source["name"].as_str().unwrap_or("");
        let role = source["accessRole"].as_str().unwrap_or("unknown");
        let mut originals = BTreeMap::new();
        let mut text = String::from("BEGIN:VCALENDAR\nVERSION:2.0\n");
        for entry in source["events"]
            .as_array()
            .into_iter()
            .flatten()
            .take(MAX_EVENTS + 1)
        {
            let Some(raw) = entry["raw"].as_str() else {
                continue;
            };
            let Some((uid, rid, _)) = eds::series_identity(raw) else {
                continue;
            };
            let Some(clean) = display_component(raw) else {
                continue;
            };
            if Calendar::from_str(&format!(
                "BEGIN:VCALENDAR\nVERSION:2.0\n{clean}END:VCALENDAR\n"
            ))
            .is_err()
            {
                continue;
            }
            originals.insert((uid, rid), raw.to_owned());
            text.push_str(&clean);
        }
        text.push_str("END:VCALENDAR\n");
        let Ok(calendar) = Calendar::from_str(&text) else {
            continue;
        };
        if calendar.calendar_events().any(|view| {
            view.event().property_value("RRULE").is_some_and(|rule| {
                rule.to_ascii_uppercase().contains("FREQ=SECONDLY")
                    || rule.to_ascii_uppercase().contains("FREQ=MINUTELY")
            })
        }) {
            continue;
        }
        let overrides: HashSet<(String, i64)> = calendar
            .calendar_events()
            .filter_map(|view| {
                let event = view.event();
                Some((
                    event.get_uid()?.to_owned(),
                    local(event.get_recurrence_id()?, tz)?.timestamp(),
                ))
            })
            .collect();
        for view in calendar.calendar_events() {
            let event = view.event();
            if event
                .property_value("STATUS")
                .is_some_and(|value| value.eq_ignore_ascii_case("CANCELLED"))
            {
                continue;
            }
            let Some(uid) = event.get_uid() else { continue };
            let Some(start_value) = event.get_start() else {
                continue;
            };
            let all_day = matches!(start_value, DatePerhapsTime::Date(_));
            let Some(base_start) = local(start_value.clone(), tz) else {
                continue;
            };
            let base_end = end_for(event, base_start, all_day, tz);
            let span = base_end.signed_duration_since(base_start);
            let span_days = base_end
                .date_naive()
                .signed_duration_since(base_start.date_naive())
                .num_days()
                .max(1);
            let Ok(set) = view.get_recurrence() else {
                continue;
            };
            let threshold =
                (lo - span - Duration::seconds(1)).with_timezone(&set.get_dt_start().timezone());
            let upper = hi.with_timezone(&set.get_dt_start().timezone());
            let expanded = set
                .after(threshold)
                .before(upper)
                .all((MAX_EVENTS + 1) as u16);
            count += expanded.dates.len();
            if count > MAX_EVENTS {
                return Err("Too many events in this month to display safely.".into());
            }
            let detached = event.get_recurrence_id().is_some();
            let master = originals.get(&(uid.to_owned(), String::new()));
            let master_recurring = master.is_some_and(|raw| {
                ["RRULE", "RDATE", "EXDATE", "EXRULE"]
                    .iter()
                    .any(|key| has_property(raw, key))
            });
            for occurrence in expanded.dates {
                let start = occurrence.with_timezone(&tz);
                if !detached && overrides.contains(&(uid.to_owned(), start.timestamp())) {
                    continue;
                }
                let end = if all_day {
                    localize(
                        tz,
                        (start.date_naive() + Duration::days(span_days)).and_time(NaiveTime::MIN),
                    )
                    .unwrap_or(start + span)
                } else {
                    start + span
                };
                if start >= hi || end <= lo {
                    continue;
                }
                let rid = if detached {
                    event
                        .property_value("RECURRENCE-ID")
                        .unwrap_or("")
                        .to_owned()
                } else if master_recurring {
                    recurrence_id(start, &start_value, tz)
                } else {
                    String::new()
                };
                let source_rid = if originals.contains_key(&(uid.to_owned(), rid.clone())) {
                    rid.clone()
                } else {
                    String::new()
                };
                let original = originals
                    .get(&(uid.to_owned(), source_rid.clone()))
                    .map(String::as_str)
                    .unwrap_or("");
                let mut first = month.max(start.date_naive());
                let last = (next_month(month) - Duration::days(1))
                    .min((end - Duration::microseconds(1)).date_naive());
                while first <= last {
                    marks.insert(first.to_string());
                    first += Duration::days(1);
                }
                let row = json!({
                    "id":fingerprint(&format!("{generation}{calendar_id}{uid}{rid}")),
                    "provider":"Google","calendar":name,"calendarId":calendar_id,
                    "eventId":uid,"recurrenceId":rid,"sourceRecurrenceId":source_rid,
                    "etag":if original.is_empty(){String::new()}else{fingerprint(original)},
                    "accountGeneration":generation,"recurring":!rid.is_empty(),
                    "editable":editable(role,event,original),
                    "title":bounded(event.get_summary(),1000,"Untitled event"),
                    "notes":bounded(event.property_value("DESCRIPTION"),8000,""),
                    "location":bounded(event.property_value("LOCATION"),1000,""),
                    "allDay":all_day,"date":start.date_naive().to_string(),
                    "endDate":(end-Duration::microseconds(1)).date_naive().to_string(),
                    "startTime":start.format("%H:%M").to_string(),
                    "endTime":end.format("%H:%M").to_string(),"start":start.timestamp() as f64,
                    "editStart":if all_day {start.date_naive().to_string()}else{start.format("%Y-%m-%dT%H:%M").to_string()},
                    "editEnd":if all_day {(end-Duration::days(1)).date_naive().to_string()}else{end.format("%Y-%m-%dT%H:%M").to_string()}
                });
                #[cfg(feature = "color")]
                let row = {
                    let mut row = row;
                    row["color"] = json!(source["color"].as_str().unwrap_or(""));
                    row
                };
                rows.push(row);
            }
        }
    }
    Ok((rows, marks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn fixture(events: Vec<&str>) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let settings = root.path().join("google-settings");
        fs::create_dir_all(&settings).unwrap();
        fs::write(settings.join("sources.json"),serde_json::to_vec(&json!({"accounts":[{"id":"account"}],"calendars":[{"id":"source","accountId":"account"}]})).unwrap()).unwrap();
        let month = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let path = google_cache::month_file(root.path(), month);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let events:Vec<Value>=events.into_iter().map(|raw|json!({"raw":raw,"ical":format!("BEGIN:VCALENDAR\nVERSION:2.0\n{raw}END:VCALENDAR\n")})).collect();
        fs::write(path,serde_json::to_vec(&json!({"calendars":{"source":{"id":"source","name":"Fixture","accessRole":"writer","color":"#ac44cf","events":events}}})).unwrap()).unwrap();
        root
    }
    #[test]
    fn oneoff_and_vendor_fields_keep_editable_london_row() {
        let root = fixture(vec![
            "BEGIN:VEVENT\nUID:fixture\nDTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nSUMMARY:Appointment\nX-EVOLUTION-CALDAV-ETAG:version-one\nEND:VEVENT\n",
        ]);
        let (rows, days) = read_cached(
            root.path(),
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            chrono_tz::Europe::London,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["startTime"], "09:00");
        assert_eq!(rows[0]["recurrenceId"], "");
        assert_eq!(rows[0]["editable"], true);
        assert_eq!(rows[0]["color"], "#ac44cf");
        assert!(days.contains("2026-10-08"));
    }
    #[test]
    fn weekly_exception_and_detached_override() {
        let master = "BEGIN:VEVENT\nUID:fixture\nDTSTART:20261001T080000Z\nDTEND:20261001T090000Z\nRRULE:FREQ=WEEKLY;COUNT=4\nEXDATE:20261015T080000Z\nSUMMARY:Master\nEND:VEVENT\n";
        let moved = "BEGIN:VEVENT\nUID:fixture\nRECURRENCE-ID:20261008T080000Z\nDTSTART:20261008T110000Z\nDTEND:20261008T120000Z\nSUMMARY:Moved\nEND:VEVENT\n";
        let root = fixture(vec![master, moved]);
        let (rows, days) = read_cached(
            root.path(),
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            chrono_tz::Europe::London,
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert!(!days.contains("2026-10-15"));
        assert_eq!(
            rows.iter()
                .filter(|row| row["date"] == "2026-10-08")
                .count(),
            1
        );
        assert_eq!(
            rows.iter().find(|row| row["date"] == "2026-10-08").unwrap()["title"],
            "Moved"
        );
    }
    #[test]
    fn all_day_end_is_exclusive_and_attendee_events_are_protected() {
        let day = "BEGIN:VEVENT\nUID:day\nDTSTART;VALUE=DATE:20261008\nDTEND;VALUE=DATE:20261010\nSUMMARY:Two days\nEND:VEVENT\n";
        let attendee = "BEGIN:VEVENT\nUID:guest\nDTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nATTENDEE:mailto:fixture@example.org\nSUMMARY:Invitation\nEND:VEVENT\n";
        let root = fixture(vec![day, attendee]);
        let (rows, days) = read_cached(
            root.path(),
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            chrono_tz::Europe::London,
        )
        .unwrap();
        let all_day = rows.iter().find(|row| row["eventId"] == "day").unwrap();
        assert_eq!(all_day["allDay"], true);
        assert_eq!(all_day["editEnd"], "2026-10-09");
        assert!(days.contains("2026-10-09"));
        assert!(!days.contains("2026-10-10"));
        assert_eq!(
            rows.iter().find(|row| row["eventId"] == "guest").unwrap()["editable"],
            false
        );
    }
}
