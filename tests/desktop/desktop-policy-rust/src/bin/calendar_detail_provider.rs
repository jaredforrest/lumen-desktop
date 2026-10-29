//! Read-only private Calendar detail data for typography fixtures.
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    if request["action"] != "list" && request["action"] != "load" {
        return Err("Only private Calendar reads are allowed".into());
    }
    if let Ok(path) = std::env::var("LUMEN_FIXTURE_CALENDAR_ACTIONS") {
        writeln!(std::fs::OpenOptions::new().append(true).create(true).open(path)?, "{request}")?;
    }
    let native = std::env::var("LUMEN_FIXTURE_CALENDAR_NATIVE").as_deref() == Ok("true");
    let editable = std::env::var("LUMEN_FIXTURE_CALENDAR_READONLY").as_deref() != Ok("true");
    println!("{}", json!({
        "events":[{"id":"private-event","title":"Private appointment","date":"2026-09-28",
            "endDate":"2026-09-28","startTime":"09:30","endTime":"10:45",
            "provider":"Google","calendar":"Private calendar","location":if native {"Room 42"} else {"Room 42, north wing"},
            "notes":if native {"Private notes for the selected appointment."} else {"Private notes: bring the printed agenda and review the schedule."},
            "editable":editable,"recurring":true,"accountGeneration":"private-generation",
            "calendarId":"private","eventId":"private-event","allDay":false,
            "editStart":"2026-09-28T09:30","editEnd":"2026-09-28T10:45"}],
        "days":["2026-09-28"],"total":1,"configured":true,"timezone":"Europe/London",
        "providers":[{"id":"google","name":"Private provider","generation":"private-generation",
            "configured":true,"calendars":[{"id":"private","name":"Private calendar","accessRole":"owner"}]}]
    }));
    Ok(())
}
