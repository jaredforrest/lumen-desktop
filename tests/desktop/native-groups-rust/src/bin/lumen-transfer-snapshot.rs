//! Inspect a retained bounded native tree without creating a desktop session.
use std::{env, fs, io::Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let file = args.next().ok_or("Expected retained native tree")?;
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(file)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err("Native tree exceeds bound".into());
    }
    let tree = serde_json::from_slice(&bytes)?;
    println!("{}", lumen_spaces::transfer::snapshot(&tree)?);
    Ok(())
}
