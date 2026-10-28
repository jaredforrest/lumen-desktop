//! Private Hot Corners command peer used by the offscreen editor test.
use std::env;
use std::fs;
use std::io::{self, BufRead};

fn main() -> io::Result<()> {
    let saved = env::var_os("LUMEN_HOT_CORNERS_SAVED").expect("saved path");
    let command = env::args().nth(1).unwrap_or_default();
    if command == "configure" {
        let mut request = String::new();
        io::stdin().lock().read_line(&mut request)?;
        fs::write(saved, request)?;
        println!(r#"{{"ok":true}}"#);
    } else {
        let corners = fs::read_to_string(saved).unwrap_or_else(|_| {
            let disabled = r#"{"action":"disabled","modifiers":[],"allowFullscreen":false,"allowAdjacent":false}"#;
            format!("{{\"top-left\":{disabled},\"top-right\":{disabled},\"bottom-left\":{disabled},\"bottom-right\":{disabled}}}")
        });
        println!("{{\"corners\":{corners},\"nativeAvailable\":true}}");
    }
    Ok(())
}
