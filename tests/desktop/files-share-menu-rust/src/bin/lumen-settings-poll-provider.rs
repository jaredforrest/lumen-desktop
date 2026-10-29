//! Disposable appearance protocol; every command and mutation is local to the fixture.
use serde_json::{Value, json};
use std::{env, fs, io::Write, time::Duration};
fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let store = env::var_os("SETTINGS_POLL_STORE").unwrap();
    let log = env::var_os("SETTINGS_POLL_LOG").unwrap();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .unwrap();
    writeln!(file, "{}", serde_json::to_string(&args).unwrap()).unwrap();
    let mut preferences: Value = fs::read(&store)
        .map(|b| serde_json::from_slice(&b).unwrap())
        .unwrap_or(json!({"dockPins":["fixture-a","fixture-b"],"dockSize":48}));
    std::thread::sleep(Duration::from_millis(180));
    match args.as_slice() {
        [read] if read == "--settings" => {}
        [set, key, value] if set == "--set" && key == "dockPins" => {
            preferences[key] = serde_json::from_str(value).unwrap();
            fs::write(store, serde_json::to_vec(&preferences).unwrap()).unwrap();
        }
        _ => std::process::exit(2),
    }
    println!(
        "{}",
        json!({"preferences":preferences,"appearance":{},"system":{}})
    );
}
