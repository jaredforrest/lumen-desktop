use chrono::Datelike;
use desktop_calendar_runtime::service::{data_root, timezone, today};
use desktop_calendar_runtime::{google_cache, parse_day};

fn run() -> Result<(), String> {
    let tz = timezone();
    let mut action = "sync".to_owned();
    let mut month = today(tz).with_day(1).ok_or("Invalid calendar month")?;
    let mut arguments = std::env::args().skip(1).peekable();
    if arguments
        .peek()
        .is_some_and(|value| value == "sync" || value == "status")
    {
        action = arguments.next().unwrap();
    }
    while let Some(argument) = arguments.next() {
        if argument == "--month" {
            let value = arguments.next().ok_or("Missing calendar month")?;
            month = parse_day(&value)?
                .with_day(1)
                .ok_or("Invalid calendar month")?;
        } else {
            return Err("Unknown Google calendar argument".into());
        }
    }
    let root = data_root();
    if action == "status" {
        println!(
            "{}",
            google_cache::status(&root.join("google-settings"), true)
        );
    } else {
        google_cache::sync_month(&root, month, true)?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
