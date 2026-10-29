//! Development entry point for comparing the native iCloud mirror reader with
//! the existing calendar service before switching its public command.
use chrono::Datelike;
use desktop_calendar_runtime::{parse_day, read_icloud};
use std::path::Path;

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    #[cfg(feature = "eds")]
    if args.next().as_deref() == Some("--discover") {
        if args.next().is_some() {
            return Err("Unexpected argument after --discover".into());
        }
        println!("{}", desktop_calendar_runtime::eds::discover()?);
        return Ok(());
    }
    #[cfg(feature = "eds")]
    let mut args = std::env::args().skip(1);
    #[cfg(feature = "eds")]
    if args.next().as_deref() == Some("--probe-load") {
        let uid = args.next().ok_or("Missing calendar source")?;
        let month = parse_day(&args.next().ok_or("Missing month")?)?;
        if args.next().is_some() || month.day() != 1 {
            return Err("Expected --probe-load SOURCE YYYY-MM-01".into());
        }
        let loaded =
            desktop_calendar_runtime::eds::load(&uid, month, chrono_tz::Europe::London, false)?;
        let snapshot = serde_json::json!({"calendars":{"probe":{
            "name":"Probe","accessRole":loaded["accessRole"],"events":loaded["events"]
        }}});
        let (rendered, days) = desktop_calendar_runtime::google_events::rows_from_snapshot(
            &snapshot,
            "probe",
            month,
            chrono_tz::Europe::London,
        )?;
        println!(
            "{}",
            serde_json::json!({
                "eventCount":loaded["events"].as_array().map_or(0, Vec::len),
                "renderedCount":rendered.len(),"markedDays":days.len(),
                "accessRole":loaded["accessRole"],"online":loaded["online"]
            })
        );
        return Ok(());
    }
    #[cfg(feature = "eds")]
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .ok_or("Usage: calendar-mirror-reader ROOT SELECTED-DATE MONTH")?;
    let selected = parse_day(&args.next().ok_or("Missing selected date")?)?;
    let month = parse_day(&args.next().ok_or("Missing month")?)?;
    if args.next().is_some() || selected.year() != month.year() || selected.month() != month.month()
    {
        return Err("Selected date must be in the displayed month".into());
    }
    let month = month.with_day(1).ok_or("Invalid month")?;
    #[cfg(feature = "color")]
    let tz = std::env::var("LUMEN_CALENDAR_TIMEZONE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(chrono_tz::Europe::London);
    #[cfg(not(feature = "color"))]
    let tz = chrono_tz::Europe::London;
    println!("{}", read_icloud(Path::new(&root), selected, month, tz));
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
