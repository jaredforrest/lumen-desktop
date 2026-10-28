//! Private Dock pointer intent and GTK chooser acceptance.
mod drag;
mod harness;
mod interaction;
use harness::{Options, Result, private_session};
fn main() -> Result<()> {
    let options = Options::parse()?;
    if !options.inside {
        return private_session();
    }
    match options.scenario.as_str() {
        "drag" => drag::run(options),
        "interaction" => interaction::run(options),
        _ => Err("Unknown Dock scenario".into()),
    }
}
