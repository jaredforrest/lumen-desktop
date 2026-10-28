//! Bounded iCloud mirror reader for the Rust calendar migration.
//! EDS discovery and writes remain behind the existing service until the native adapter is ported.
#[cfg(feature = "eds")]
pub mod eds;
#[cfg(feature = "eds")]
pub mod google_cache;
#[cfg(feature = "eds")]
pub mod google_edit;
#[cfg(feature = "eds")]
pub mod google_events;
#[cfg(feature = "eds")]
pub mod google_mutation;
#[cfg(feature = "eds")]
pub mod service;
use chrono::{
    DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
};
use chrono_tz::Tz;
use icalendar::{Calendar, CalendarDateTime, Component, DatePerhapsTime, EventLike};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

const MAX_FILES: usize = 10_000;
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_OCCURRENCES: usize = 10_000;

pub fn parse_day(value: &str) -> Result<NaiveDate, String> {
    if value.len() != 10
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
    {
        return Err("Use a date in YYYY-MM-DD format".into());
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "Use a date in YYYY-MM-DD format".into())
}

pub(crate) fn localize(tz: Tz, date: NaiveDateTime) -> Option<DateTime<Tz>> {
    tz.from_local_datetime(&date).earliest()
}

pub(crate) fn local(value: DatePerhapsTime, tz: Tz) -> Option<DateTime<Tz>> {
    match value {
        DatePerhapsTime::Date(day) => localize(tz, day.and_hms_opt(0, 0, 0)?),
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(value)) => Some(value.with_timezone(&tz)),
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(value)) => localize(tz, value),
        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, tzid }) => {
            let source_tz = tzid.parse::<Tz>().ok()?;
            Some(localize(source_tz, date_time)?.with_timezone(&tz))
        }
    }
}

pub(crate) fn next_month(month: NaiveDate) -> NaiveDate {
    if month.month() == 12 {
        NaiveDate::from_ymd_opt(month.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(month.year(), month.month() + 1, 1).unwrap()
    }
}

pub(crate) fn bounded(value: Option<&str>, limit: usize, fallback: &str) -> String {
    value.unwrap_or(fallback).chars().take(limit).collect()
}

pub(crate) fn end_for(
    event: &impl EventLike,
    start: DateTime<Tz>,
    all_day: bool,
    tz: Tz,
) -> DateTime<Tz> {
    let end = event
        .get_end()
        .and_then(|value| local(value, tz))
        .or_else(|| {
            let raw = event.property_value("DURATION")?;
            let parsed = iso8601::duration(raw).ok()?;
            let elapsed = Duration::from_std(parsed.into()).ok()?;
            start.checked_add_signed(elapsed)
        })
        .or_else(|| {
            start.checked_add_signed(if all_day {
                Duration::days(1)
            } else {
                Duration::seconds(1)
            })
        });
    match end {
        Some(end) if end > start => end,
        _ => start + Duration::seconds(1),
    }
}

fn metadata_name(folder: &Path) -> String {
    let path = folder.join("displayname");
    read_regular(&path, 4095)
        .map(|name| bounded(Some(name.trim()), 200, ""))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "iCloud Calendar".into())
}

fn read_regular(path: &Path, maximum: u64) -> Option<String> {
    // Keep the no-follow check on the opened descriptor so a replaced leaf
    // cannot redirect the reader between metadata inspection and reading.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > maximum {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > maximum {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(feature = "color")]
fn calendar_color(calendar: &Calendar) -> String {
    let raw = calendar
        .property_value("COLOR")
        .or_else(|| calendar.property_value("X-LUMEN-CALENDAR-COLOR"))
        .unwrap_or("");
    if (raw.len() == 7 || raw.len() == 9)
        && raw.starts_with('#')
        && raw[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return raw[..7].to_owned();
    }
    String::new()
}

fn push_warning(warnings: &mut Vec<String>, message: &str) {
    if !warnings.iter().any(|old| old == message) {
        warnings.push(message.into());
    }
}

fn event_rows(
    calendar: &Calendar,
    file: &Path,
    mirror: &Path,
    name: &str,
    tz: Tz,
    month: NaiveDate,
    month_end: NaiveDate,
    rows: &mut Vec<Value>,
    marks: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
    occurrences: &mut usize,
) {
    let lo = localize(tz, month.and_time(NaiveTime::MIN)).unwrap();
    let hi = localize(tz, month_end.and_time(NaiveTime::MIN)).unwrap();
    #[cfg(feature = "color")]
    let color = calendar_color(calendar);
    // A detached VEVENT replaces the matching instance of its master. Include
    // cancelled instances here so the original occurrence also disappears.
    let overrides: HashSet<(String, i64)> = calendar
        .calendar_events()
        .filter_map(|view| {
            let event = view.event();
            let uid = event.property_value("UID")?;
            let rid = local(event.get_recurrence_id()?, tz)?;
            Some((uid.to_owned(), rid.timestamp()))
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
        if event.property_value("RRULE").is_some_and(|rule| {
            rule.to_ascii_uppercase().contains("FREQ=SECONDLY")
                || rule.to_ascii_uppercase().contains("FREQ=MINUTELY")
        }) {
            push_warning(
                warnings,
                "Sub-hourly recurring events are not shown in this calendar view",
            );
            continue;
        }
        let Some(start_value) = event.get_start() else {
            push_warning(warnings, "An unreadable calendar event was skipped");
            continue;
        };
        let all_day = matches!(start_value, DatePerhapsTime::Date(_));
        let Some(base_start) = local(start_value, tz) else {
            push_warning(warnings, "An unreadable calendar event was skipped");
            continue;
        };
        let detached = event.get_recurrence_id().is_some();
        let base_end = end_for(event, base_start, all_day, tz);
        let span = base_end.signed_duration_since(base_start);
        let span_days = base_end
            .date_naive()
            .signed_duration_since(base_start.date_naive())
            .num_days()
            .max(1);
        let Ok(set) = view.get_recurrence() else {
            push_warning(warnings, "An unreadable calendar event was skipped");
            continue;
        };
        let threshold =
            (lo - span - Duration::seconds(1)).with_timezone(&set.get_dt_start().timezone());
        let upper = hi.with_timezone(&set.get_dt_start().timezone());
        let results = set
            .after(threshold)
            .before(upper)
            .all((MAX_OCCURRENCES + 1) as u16);
        *occurrences += results.dates.len();
        if *occurrences > MAX_OCCURRENCES {
            push_warning(
                warnings,
                "Too many events in this month; some events were omitted",
            );
            return;
        }
        for occurrence in results.dates {
            let start = occurrence.with_timezone(&tz);
            if !detached
                && overrides.contains(&(
                    event.property_value("UID").unwrap_or("").to_owned(),
                    start.timestamp(),
                ))
            {
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
            let mut first = month.max(start.date_naive());
            let last =
                (month_end - Duration::days(1)).min((end - Duration::microseconds(1)).date_naive());
            while first <= last {
                marks.insert(first.to_string());
                first += Duration::days(1);
            }
            let identity = format!(
                "iCloud:{}{}{}",
                file.strip_prefix(mirror).unwrap_or(file).display(),
                event.property_value("UID").unwrap_or(""),
                start.to_rfc3339()
            );
            let id = hex::encode(Sha256::digest(identity.as_bytes()));
            let row = json!({
                "id":id,"title":bounded(event.get_summary(),1000,"Untitled event"),
                "notes":bounded(event.property_value("DESCRIPTION"),8000,""),
                "location":bounded(event.property_value("LOCATION"),1000,""),
                "calendar":name,"provider":"iCloud","allDay":all_day,
                "date":start.date_naive().to_string(),
                "endDate":(end-Duration::microseconds(1)).date_naive().to_string(),
                "startTime":format!("{:02}:{:02}",start.hour(),start.minute()),
                "endTime":format!("{:02}:{:02}",end.hour(),end.minute()),
                "start":start.timestamp() as f64,
            });
            #[cfg(feature = "color")]
            let row = {
                let mut row = row;
                row["color"] = json!(color);
                row
            };
            rows.push(row);
        }
    }
}

/// Reads only the dedicated one-level iCloud mirror. No symlinks are followed.
pub fn read_icloud(root: &Path, selected: NaiveDate, month: NaiveDate, tz: Tz) -> Value {
    let mirror = root.join("icloud");
    let month_end = next_month(month);
    let mut rows = Vec::new();
    let mut marks = BTreeSet::new();
    let mut warnings = Vec::new();
    let mut files = 0;
    let mut occurrences = 0;
    if fs::symlink_metadata(&mirror).is_ok_and(|meta| meta.file_type().is_dir()) {
        let mut folders = fs::read_dir(&mirror)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<PathBuf>>();
        folders.sort();
        'folders: for folder in folders {
            if !fs::symlink_metadata(&folder).is_ok_and(|meta| meta.file_type().is_dir()) {
                continue;
            }
            let name = metadata_name(&folder);
            let mut paths = fs::read_dir(&folder)
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "ics"))
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths {
                files += 1;
                if files > MAX_FILES {
                    push_warning(&mut warnings, "Only the first 10,000 event files were read");
                    break 'folders;
                }
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if !meta.file_type().is_file() || meta.len() > MAX_BYTES {
                    push_warning(
                        &mut warnings,
                        "An oversized or linked calendar file was skipped",
                    );
                    continue;
                }
                let Some(raw) = read_regular(&path, MAX_BYTES) else {
                    push_warning(&mut warnings, "An unreadable calendar event was skipped");
                    continue;
                };
                let Ok(calendar) = Calendar::from_str(&raw) else {
                    push_warning(&mut warnings, "An unreadable calendar event was skipped");
                    continue;
                };
                event_rows(
                    &calendar,
                    &path,
                    &mirror,
                    &name,
                    tz,
                    month,
                    month_end,
                    &mut rows,
                    &mut marks,
                    &mut warnings,
                    &mut occurrences,
                );
                if occurrences > MAX_OCCURRENCES {
                    break 'folders;
                }
            }
        }
    }
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
    let selected_rows = rows
        .iter()
        .filter(|row| {
            row["date"]
                .as_str()
                .is_some_and(|start| start <= chosen.as_str())
                && row["endDate"]
                    .as_str()
                    .is_some_and(|end| end >= chosen.as_str())
        })
        .take(100)
        .cloned()
        .collect::<Vec<_>>();
    let total = rows
        .iter()
        .filter(|row| {
            row["date"]
                .as_str()
                .is_some_and(|start| start <= chosen.as_str())
                && row["endDate"]
                    .as_str()
                    .is_some_and(|end| end >= chosen.as_str())
        })
        .count();
    json!({"events":selected_rows,"total":total,"monthEvents":rows,"days":marks.into_iter().collect::<Vec<_>>(),"warning":warnings.join("; ")})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn fixture(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("icloud/personal");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("one.ics");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        (root, path)
    }
    fn dates() -> (NaiveDate, NaiveDate, Tz) {
        (
            parse_day("2026-10-08").unwrap(),
            parse_day("2026-10-01").unwrap(),
            chrono_tz::Europe::London,
        )
    }
    #[test]
    fn parses_london_time_and_selected_date() {
        let (root, _) = fixture(
            "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:fixture\nDTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nSUMMARY:Appointment\nEND:VEVENT\nEND:VCALENDAR\n",
        );
        let (selected, month, tz) = dates();
        let result = read_icloud(root.path(), selected, month, tz);
        assert_eq!(result["events"][0]["startTime"], "09:00");
        assert_eq!(result["days"][0], "2026-10-08");
    }
    #[test]
    fn excludes_weekly_exception_and_rejects_symlink() {
        let (root, path) = fixture(
            "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:fixture\nDTSTART:20261001T080000Z\nDTEND:20261001T090000Z\nRRULE:FREQ=WEEKLY;COUNT=4\nEXDATE:20261015T080000Z\nSUMMARY:Recurring\nEND:VEVENT\nEND:VCALENDAR\n",
        );
        let (selected, month, tz) = dates();
        let result = read_icloud(root.path(), selected, month, tz);
        assert!(
            result["days"]
                .as_array()
                .unwrap()
                .iter()
                .any(|day| day == "2026-10-08")
        );
        assert!(
            !result["days"]
                .as_array()
                .unwrap()
                .iter()
                .any(|day| day == "2026-10-15")
        );
        let link = path.with_file_name("linked.ics");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        let result = read_icloud(root.path(), selected, month, tz);
        assert!(result["warning"].as_str().unwrap().contains("linked"));
    }
    #[test]
    fn detached_instance_replaces_master_and_cancelled_instance_disappears() {
        let (root, _) = fixture(
            "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:fixture\nDTSTART;TZID=Europe/London:20261001T090000\nDTEND;TZID=Europe/London:20261001T100000\nRRULE:FREQ=WEEKLY;COUNT=3\nSUMMARY:Original\nEND:VEVENT\nBEGIN:VEVENT\nUID:fixture\nRECURRENCE-ID;TZID=Europe/London:20261008T090000\nDTSTART;TZID=Europe/London:20261008T120000\nDTEND;TZID=Europe/London:20261008T130000\nSUMMARY:Moved\nEND:VEVENT\nBEGIN:VEVENT\nUID:fixture\nRECURRENCE-ID;TZID=Europe/London:20261015T090000\nDTSTART;TZID=Europe/London:20261015T090000\nSTATUS:CANCELLED\nEND:VEVENT\nEND:VCALENDAR\n",
        );
        let (selected, month, tz) = dates();
        let result = read_icloud(root.path(), selected, month, tz);
        let events = result["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["title"], "Moved");
        assert_eq!(events[0]["startTime"], "12:00");
        assert_eq!(result["days"], json!(["2026-10-01", "2026-10-08"]));
    }
    #[test]
    fn all_day_end_is_exclusive() {
        let (root, _) = fixture(
            "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:fixture\nDTSTART;VALUE=DATE:20261008\nDTEND;VALUE=DATE:20261010\nSUMMARY:Two days\nEND:VEVENT\nEND:VCALENDAR\n",
        );
        let (selected, month, tz) = dates();
        let result = read_icloud(root.path(), selected, month, tz);
        assert_eq!(result["events"][0]["allDay"], true);
        assert_eq!(result["days"], json!(["2026-10-08", "2026-10-09"]));
    }
    #[test]
    fn london_recurrence_keeps_wall_time_when_clocks_change() {
        let (root, _) = fixture(
            "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:fixture\nDTSTART;TZID=Europe/London:20261024T090000\nDTEND;TZID=Europe/London:20261024T100000\nRRULE:FREQ=DAILY;COUNT=3\nSUMMARY:Daily\nEND:VEVENT\nEND:VCALENDAR\n",
        );
        let month = parse_day("2026-10-01").unwrap();
        let tz = chrono_tz::Europe::London;
        let before = read_icloud(root.path(), parse_day("2026-10-24").unwrap(), month, tz);
        let after = read_icloud(root.path(), parse_day("2026-10-25").unwrap(), month, tz);
        assert_eq!(before["events"][0]["startTime"], "09:00");
        assert_eq!(after["events"][0]["startTime"], "09:00");
        assert_eq!(
            after["events"][0]["start"].as_f64().unwrap()
                - before["events"][0]["start"].as_f64().unwrap(),
            25.0 * 3600.0
        );
    }
}
