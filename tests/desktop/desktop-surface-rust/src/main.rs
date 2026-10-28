//! Private desktop file surface and native drag acceptance.
mod drag;
mod files;
mod harness;

use harness::{Options, Result, private_session};

fn main() -> Result<()> {
    let options = Options::parse()?;
    if !options.inside {
        return private_session();
    }
    match options.scenario.as_str() {
        "files" => files::run(options),
        "drag" => drag::run(options),
        _ => Err("Unknown desktop surface scenario".into()),
    }
}
