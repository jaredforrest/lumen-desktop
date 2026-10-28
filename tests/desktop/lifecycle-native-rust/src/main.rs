//! Native lifecycle and session end acceptance with disposable GTK clients.
mod harness;
mod lifecycle;
mod session;

use harness::{Options, Result, private_session};

fn main() -> Result<()> {
    let options = Options::parse()?;
    if !options.inside {
        return private_session();
    }
    match options.scenario.as_str() {
        "lifecycle" => lifecycle::run(options),
        "session" => session::run(options),
        _ => Err("Unknown scenario".into()),
    }
}
