use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

fn request(input: &str) -> (i32, Value) {
    let root = tempfile::tempdir().unwrap();
    request_at(input, root.path())
}

fn request_at(input: &str, root: &std::path::Path) -> (i32, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_desktop-calendar"))
        .env("XDG_DATA_HOME", root)
        .env("CALENDAR_DESKTOP_PREFIX", "fixture")
        .env("CALENDAR_UID_SUFFIX", "@fixture.local")
        .env("LUMEN_CALENDAR_TIMEZONE", "Europe/London")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    let value = serde_json::from_slice(&result.stdout).unwrap();
    (result.status.code().unwrap(), value)
}

#[test]
fn installed_protocol_reads_boundary_events_without_changing_mirror() {
    let root = tempfile::tempdir().unwrap();
    let calendar = root.path().join("calendar");
    for (name, events) in [
        (
            "Personal",
            vec![
                (
                    "all-day",
                    "DTSTART;VALUE=DATE:20261031\nDTEND;VALUE=DATE:20261102\nSUMMARY:Two-day all-day",
                ),
                (
                    "old-day",
                    "DTSTART:20261031T220000Z\nDTEND:20261031T230000Z\nSUMMARY:Previous day only",
                ),
            ],
        ),
        (
            "Travel",
            vec![
                (
                    "west",
                    "DTSTART;TZID=America/Los_Angeles:20261031T183000\nDTEND;TZID=America/Los_Angeles:20261031T200000\nSUMMARY:West overlap",
                ),
                (
                    "east",
                    "DTSTART;TZID=America/New_York:20261031T220000\nDTEND;TZID=America/New_York:20261031T230000\nSUMMARY:East overlap",
                ),
                (
                    "utc",
                    "DTSTART:20261101T030000Z\nDTEND:20261101T040000Z\nSUMMARY:Later UTC",
                ),
            ],
        ),
    ] {
        let folder = calendar.join("icloud").join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("displayname"), name).unwrap();
        for (uid, fields) in events {
            std::fs::write(folder.join(format!("{uid}.ics")), format!(
                "BEGIN:VCALENDAR\nVERSION:2.0\nCOLOR:#aa44cc\nBEGIN:VEVENT\nUID:{uid}\n{fields}\nEND:VEVENT\nEND:VCALENDAR\n"
            )).unwrap();
        }
    }
    let before: Vec<_> = std::fs::read_dir(calendar.join("icloud"))
        .unwrap()
        .flat_map(|directory| std::fs::read_dir(directory.unwrap().path()).unwrap())
        .map(|entry| {
            let path = entry.unwrap().path();
            (path.clone(), std::fs::read(path).unwrap())
        })
        .collect();
    let (code, value) = request_at(
        "{\"action\":\"list\",\"selectedDate\":\"2026-11-01\",\"month\":\"2026-11-01\"}\n",
        root.path(),
    );
    assert_eq!(code, 0, "{value}");
    let rows = value["events"].as_array().unwrap();
    let summary: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row["title"].as_str().unwrap(),
                row["startTime"].as_str().unwrap(),
                row["endTime"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("Two-day all-day", "00:00", "00:00"),
            ("West overlap", "01:30", "03:00"),
            ("East overlap", "02:00", "03:00"),
            ("Later UTC", "03:00", "04:00"),
        ]
    );
    assert_eq!(rows[0]["endDate"], "2026-11-01");
    assert_eq!(rows[0]["color"], "#aa44cc");
    assert_eq!(value["total"], 4);
    for (path, bytes) in before {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    let (code, next) = request_at(
        "{\"action\":\"list\",\"selectedDate\":\"2026-11-02\",\"month\":\"2026-11-01\"}\n",
        root.path(),
    );
    assert_eq!(code, 0, "{next}");
    assert_eq!(next["events"], serde_json::json!([]));
}

#[test]
fn unconfigured_request_is_json_and_does_not_create_state() {
    let (code, value) = request("{\"selectedDate\":\"2026-10-08\"}\n");
    assert_eq!(code, 0);
    assert_eq!(value["configured"], false);
    assert_eq!(value["events"], serde_json::json!([]));
    assert_eq!(value["date"], "2026-10-08");
}

#[test]
fn malformed_and_oversized_requests_fail_as_json() {
    for input in ["not json\n".to_owned(), "x".repeat(65_537)] {
        let (code, value) = request(&input);
        assert_eq!(code, 1);
        assert!(value["error"].as_str().is_some());
    }
}

#[test]
fn background_sync_never_prints_cached_event_content() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_desktop-calendar"))
        .arg("--sync-background")
        .env("XDG_DATA_HOME", root.path())
        .env("CALENDAR_DESKTOP_PREFIX", "fixture")
        .env("CALENDAR_UID_SUFFIX", "@fixture.local")
        .output()
        .unwrap();
    assert!(
        output
            .status
            .code()
            .is_some_and(|code| code == 0 || code == 1)
    );
    assert!(
        output.stdout.is_empty()
            || output.stdout == b"Calendar sync needs attention; cached events were retained.\n"
    );
}
