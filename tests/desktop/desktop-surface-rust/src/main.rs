//! Private desktop file surface and native drag acceptance.
mod drag;
mod files;
mod files_drag;
mod files_copy_undo;
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
        "files-drag" | "files-drag-gtk" | "files-drag-copy-undo" => files_drag::run(options),
        _ => Err("Unknown desktop surface scenario".into()),
    }
}
