//! Private clipboard-history command peer for the launcher QML test.
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let root = env::var_os("CLIPBOARD_FIXTURE").expect("fixture directory");
    let root = std::path::PathBuf::from(root);
    let args: Vec<String> = env::args().skip(1).collect();
    let action = args.first().map(String::as_str).unwrap_or_default();
    let mut log = OpenOptions::new().create(true).append(true).open(root.join("commands.log"))?;
    writeln!(log, "{}", args.join("\t"))?;
    let state = root.join("fixture-state");
    let previous = fs::read_to_string(&state).unwrap_or_else(|_| "0 1".to_owned());
    let mut fields = previous.split_whitespace();
    let mut enabled = fields.next() == Some("1");
    let mut has_entry = fields.next() == Some("1");
    match action {
        "enable" => enabled = true,
        "disable" => { enabled = false; has_entry = false; }
        "clear" => has_entry = false,
        "copy" => {
            if !enabled || !has_entry || args.get(1).map(String::as_str) != Some("fixture-one") {
                std::process::exit(2);
            }
        }
        _ => {}
    }
    fs::write(state, format!("{} {}", u8::from(enabled), u8::from(has_entry)))?;
    let mut result = format!("{{\"enabled\":{enabled}");
    if action == "list" {
        let query = args.get(2).map(String::as_str).unwrap_or_default();
        let entries = if has_entry && "fixture copied text".contains(&query.to_lowercase()) {
            "[{\"id\":\"fixture-one\",\"text\":\"Fixture copied text\",\"timestamp\":1}]"
        } else { "[]" };
        result.push_str(&format!(",\"entries\":{entries}"));
    }
    if action == "copy" { result.push_str(",\"copied\":true"); }
    result.push('}');
    println!("{result}");
    Ok(())
}
