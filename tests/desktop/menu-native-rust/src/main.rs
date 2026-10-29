//! Private installed-shell menu input and accessibility acceptance.
mod harness;
mod keyboard;
mod menubar;

use harness::{Options, Result, private_session, write_result};
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};

fn main() -> Result<()> {
    let arguments: Vec<String> = env::args().collect();
    if arguments
        .get(1)
        .is_some_and(|value| value == "capture-input")
    {
        let path = arguments.get(2).ok_or("capture-input needs a path")?;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        io::copy(&mut io::stdin(), &mut file)?;
        file.flush()?;
        return Ok(());
    }
    if arguments
        .get(1)
        .is_some_and(|value| value == "display-probe")
    {
        let path = arguments.get(2).ok_or("display-probe needs a path")?;
        std::fs::write(path, env::var("DISPLAY").unwrap_or_default())?;
        return Ok(());
    }
    let options = Options::parse()?;
    if !options.inside {
        return private_session(&options);
    }
    let scenario = options.scenario.clone();
    let output = options.output.clone();
    let (checks, extras) = match scenario.as_str() {
        "menubar" => menubar::run(options)?,
        "keyboard" => keyboard::run(options)?,
        _ => return Err("Unknown menu scenario".into()),
    };
    write_result(&output, checks, extras)?;
    Ok(())
}
