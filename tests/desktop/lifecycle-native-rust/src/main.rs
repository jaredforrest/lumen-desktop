//! Native lifecycle and session end acceptance with disposable GTK clients.
mod harness;
mod identity;
mod lifecycle;
mod mru;
mod recovery;
mod session;
mod window_mru;

use harness::{Options, Result, private_session};

fn main() -> Result<()> {
    let options = Options::parse()?;
    if !options.inside {
        return private_session();
    }
    match options.scenario.as_str() {
        "lifecycle" => lifecycle::run(options),
        "session" => session::run(options),
        "recovery" => recovery::run(options),
        "mru" => mru::run(options),
        "window-mru" | "window-mru-baseline" => window_mru::run(options),
        "identity" | "identity-absent" => identity::run(options),
        _ => Err("Unknown scenario".into()),
    }
}
