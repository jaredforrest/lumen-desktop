use chrono::Datelike;
use desktop_calendar_runtime::service::{Options, data_root, handle, timezone, today};
use serde_json::{Value, json};
use std::io::{self, Read};

fn run() -> Result<i32, String> {
    let root = data_root();
    let tz = timezone();
    let prefix = std::env::var("CALENDAR_DESKTOP_PREFIX").unwrap_or_default();
    let uid_suffix = std::env::var("CALENDAR_UID_SUFFIX").unwrap_or_default();
    let options = Options {
        root: &root,
        command_prefix: &prefix,
        uid_suffix: &uid_suffix,
        timezone: tz,
    };
    if std::env::args()
        .skip(1)
        .eq(["--sync-background".to_owned()])
    {
        let day = today(tz);
        let request = json!({"action":"sync","selectedDate":day.to_string(),
            "month":day.with_day(1).unwrap().to_string()});
        let result = handle(&request, &options)?;
        if !result["syncError"].as_str().unwrap_or("").is_empty() {
            println!("Calendar sync needs attention; cached events were retained.");
            return Ok(1);
        }
        return Ok(0);
    }
    if std::env::args().len() > 1 {
        return Err("Unknown calendar argument".into());
    }
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|_| "Calendar request could not be read")?;
    let line = bytes.split(|byte| *byte == b'\n').next().unwrap_or(&[]);
    if line.len() > 65_536 {
        return Err("Calendar request is too large".into());
    }
    let request: Value =
        serde_json::from_slice(line).map_err(|_| "Calendar request must be valid JSON")?;
    let result = handle(&request, &options)?;
    println!("{result}");
    Ok(0)
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            println!("{}", json!({"error":error}));
            std::process::exit(1);
        }
    }
}
