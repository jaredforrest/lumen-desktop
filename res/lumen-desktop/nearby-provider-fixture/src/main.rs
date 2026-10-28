//! Immutable disposable provider for the nearby-sharing boundary tests.
use std::env;
use std::fs;
use std::process;
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("status") {
        let path = env::var("NEARBY_FIXTURE_LOG").unwrap();
        fs::write(path, args.join("\n") + "\n").unwrap();
        return;
    }
    match env::var("NEARBY_FIXTURE_MODE").as_deref().unwrap_or("good") {
        "malformed" => println!("{{}}"),
        "diagnostic" => { println!(r#"{{"available":true,"active":false}}"#); eprintln!("warning"); }
        "overflow" => print!("{}", "\0".repeat(20_000)),
        "no-send" => println!(r#"{{"available":true,"active":false,"canSend":false,"detail":"On"}}"#),
        "timeout" => thread::sleep(Duration::from_secs(10)),
        "good" => println!(r#"{{"available":true,"active":false,"canSend":true,"detail":"On","extra":42}}"#),
        _ => process::exit(2),
    }
}
