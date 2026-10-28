//! Synthetic editable terminal document, without a shell command.
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("Missing document path")?;
    let mut file = File::create(path)?;
    let mut input = io::stdin().lock();
    let mut buffer = [0u8; 4096];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])?;
        file.flush()?;
    }
    Ok(())
}
