//! Validated iCalendar patches that preserve EDS revision and alarm properties.
use chrono::{DateTime, Duration, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::Value;

fn text(value: &Value, key: &str, maximum: usize) -> Result<String, String> {
    let value = value[key].as_str().unwrap_or("");
    if value.chars().count() > maximum || value.contains('\0') {
        return Err(if key == "title" {
            "Enter an event title of at most 1,000 characters."
        } else {
            "Use at most 8,000 characters for notes and 1,000 for location."
        }
        .into());
    }
    Ok(value.to_owned())
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
}

fn utc_value(local: &str, tz: Tz) -> Result<DateTime<Utc>, String> {
    if local.len() != 16 || local.as_bytes().get(10) != Some(&b'T') {
        return Err("Enter valid start and end dates/times, with the end after the start.".into());
    }
    let value = NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M")
        .map_err(|_| "Enter valid start and end dates/times, with the end after the start.")?;
    match tz.from_local_datetime(&value) {
        LocalResult::Single(value)=>Ok(value.with_timezone(&Utc)),
        _=>Err("This time falls within a daylight-saving clock change. Set it in Google Calendar instead.".into())
    }
}

fn fields(value: &Value, tz: Tz) -> Result<Vec<String>, String> {
    let title = text(value, "title", 1000)?;
    if title.trim().is_empty() {
        return Err("Enter an event title of at most 1,000 characters.".into());
    }
    let notes = text(value, "notes", 8000)?;
    let location = text(value, "location", 1000)?;
    let (start, end) = if value["allDay"] == true {
        let start = crate::parse_day(value["start"].as_str().unwrap_or(""))
            .map_err(|_| "Enter valid start and end dates/times, with the end after the start.")?;
        let end = crate::parse_day(value["end"].as_str().unwrap_or(""))
            .map_err(|_| "Enter valid start and end dates/times, with the end after the start.")?
            .checked_add_signed(Duration::days(1))
            .ok_or("Invalid end date")?;
        if end <= start {
            return Err(
                "Enter valid start and end dates/times, with the end after the start.".into(),
            );
        }
        (
            format!("DTSTART;VALUE=DATE:{}", start.format("%Y%m%d")),
            format!("DTEND;VALUE=DATE:{}", end.format("%Y%m%d")),
        )
    } else {
        let start = utc_value(value["start"].as_str().unwrap_or(""), tz)?;
        let end = utc_value(value["end"].as_str().unwrap_or(""), tz)?;
        if end <= start {
            return Err(
                "Enter valid start and end dates/times, with the end after the start.".into(),
            );
        }
        (
            format!("DTSTART:{}", start.format("%Y%m%dT%H%M%SZ")),
            format!("DTEND:{}", end.format("%Y%m%dT%H%M%SZ")),
        )
    };
    Ok(vec![
        format!("SUMMARY:{}", escape(title.trim())),
        format!("DESCRIPTION:{}", escape(&notes)),
        format!("LOCATION:{}", escape(&location)),
        start,
        end,
        format!("DTSTAMP:{}", Utc::now().format("%Y%m%dT%H%M%SZ")),
    ])
}

fn fold(line: &str) -> String {
    let mut result = String::new();
    let mut width = 0;
    for character in line.chars() {
        if width + character.len_utf8() > 75 {
            result.push_str("\r\n ");
            width = 1;
        }
        result.push(character);
        width += character.len_utf8();
    }
    result.push_str("\r\n");
    result
}

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

fn property(line: &str) -> &str {
    line.split_once(':')
        .map_or("", |(head, _)| head.split(';').next().unwrap_or(""))
}

/// Patch a VEVENT, preserving unrecognized properties and nested VALARMs.
/// When editing a generated recurrence, turn the series master into one
/// instance and carry its original recurrence timezone into RECURRENCE-ID.
pub fn patch_event(
    original: &str,
    value: &Value,
    tz: Tz,
    generated_rid: Option<&str>,
) -> Result<String, String> {
    let fields = fields(value, tz)?;
    let lines = unfolded(original);
    let mut recurrence_header = String::new();
    if let Some(rid) = generated_rid {
        if ![8, 15, 16].contains(&rid.len())
            || !rid
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'T' || byte == b'Z')
        {
            return Err("Invalid recurrence identity".into());
        }
        let header = lines
            .iter()
            .find(|line| property(line).eq_ignore_ascii_case("DTSTART"))
            .and_then(|line| line.split_once(':').map(|(head, _)| head))
            .ok_or("Missing event start")?;
        recurrence_header = header.replacen("DTSTART", "RECURRENCE-ID", 1);
    }
    let mut output = String::new();
    let mut in_event = false;
    let mut nested = 0usize;
    let mut ended = false;
    for line in lines {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VEVENT" {
            in_event = true;
            output.push_str(&fold(&line));
            continue;
        }
        if !in_event {
            continue;
        }
        if upper == "END:VEVENT" && nested == 0 {
            if let Some(rid) = generated_rid {
                output.push_str(&fold(&format!("{recurrence_header}:{rid}")));
            }
            for field in &fields {
                output.push_str(&fold(field));
            }
            output.push_str(&fold("END:VEVENT"));
            ended = true;
            break;
        }
        if upper.starts_with("BEGIN:") {
            nested += 1;
            output.push_str(&fold(&line));
            continue;
        }
        if upper.starts_with("END:") {
            nested = nested.saturating_sub(1);
            output.push_str(&fold(&line));
            continue;
        }
        if nested == 0 {
            let key = property(&line).to_ascii_uppercase();
            if [
                "SUMMARY",
                "DESCRIPTION",
                "LOCATION",
                "DTSTART",
                "DTEND",
                "DURATION",
                "DTSTAMP",
            ]
            .contains(&key.as_str())
            {
                continue;
            }
            if generated_rid.is_some()
                && ["RRULE", "RDATE", "EXDATE", "EXRULE", "RECURRENCE-ID"].contains(&key.as_str())
            {
                continue;
            }
        }
        output.push_str(&fold(&line));
    }
    if !ended {
        return Err("This event no longer exists. Sync and reopen it.".into());
    }
    Ok(output)
}

pub fn create_event(
    operation_id: &str,
    uid_suffix: &str,
    value: &Value,
    tz: Tz,
) -> Result<String, String> {
    if operation_id.len() != 32
        || !operation_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("The create request is missing its operation identifier.".into());
    }
    if uid_suffix.is_empty()
        || !uid_suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'@'))
    {
        return Err("Invalid event UID suffix".into());
    }
    let raw = format!("BEGIN:VEVENT\nUID:{operation_id}{uid_suffix}\nEND:VEVENT\n");
    patch_event(&raw, value, tz, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> Value {
        json!({"title":"Updated","notes":"Note","location":"Room","allDay":false,"start":"2026-10-08T11:00","end":"2026-10-08T12:00"})
    }
    #[test]
    fn preserves_revision_alarm_and_unknown_properties() {
        let raw = "BEGIN:VEVENT\nUID:fixture\nDTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nSUMMARY:Old\nX-EVOLUTION-CALDAV-ETAG:version-one\nX-FIXTURE:keep\nBEGIN:VALARM\nACTION:DISPLAY\nTRIGGER:-PT15M\nEND:VALARM\nEND:VEVENT\n";
        let edited = patch_event(raw, &fixture(), chrono_tz::Europe::London, None).unwrap();
        assert!(edited.contains("X-FIXTURE:keep"));
        assert!(edited.contains("BEGIN:VALARM"));
        assert!(edited.contains("X-EVOLUTION-CALDAV-ETAG:version-one"));
        assert!(edited.contains("DTSTART:20261008T100000Z"));
        assert!(!edited.contains("SUMMARY:Old"));
    }
    #[test]
    fn generated_instance_drops_master_rule_and_keeps_recurrence_id() {
        let raw = "BEGIN:VEVENT\nUID:fixture\nDTSTART;TZID=Europe/London:20261001T090000\nDTEND;TZID=Europe/London:20261001T100000\nRRULE:FREQ=WEEKLY;COUNT=4\nEND:VEVENT\n";
        let edited = patch_event(
            raw,
            &fixture(),
            chrono_tz::Europe::London,
            Some("20261008T090000"),
        )
        .unwrap();
        assert!(edited.contains("RECURRENCE-ID;TZID=Europe/London:20261008T090000"));
        assert!(!edited.contains("RRULE:"));
    }
    #[test]
    fn rejects_invalid_fields_and_dst_overlap() {
        let raw = "BEGIN:VEVENT\nUID:fixture\nEND:VEVENT\n";
        let mut fields = fixture();
        fields["title"] = json!("");
        assert!(patch_event(raw, &fields, chrono_tz::Europe::London, None).is_err());
        let mut fields = fixture();
        fields["start"] = json!("2026-10-25T01:30");
        assert!(patch_event(raw, &fields, chrono_tz::Europe::London, None).is_err());
    }
    #[test]
    fn all_day_end_is_saved_exclusive() {
        let mut fields = fixture();
        fields["allDay"] = json!(true);
        fields["start"] = json!("2026-10-08");
        fields["end"] = json!("2026-10-08");
        let raw = "BEGIN:VEVENT\nUID:fixture\nEND:VEVENT\n";
        let edited = patch_event(raw, &fields, chrono_tz::Europe::London, None).unwrap();
        assert!(edited.contains("DTEND;VALUE=DATE:20261009"));
    }
}
