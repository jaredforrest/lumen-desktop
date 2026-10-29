//! Real protocol acceptance through the installed Rust remote-desktop adapter.
#[path = "../../browser-workflows-rust/src/accessibility.rs"]
pub mod accessibility;
#[path = "../../browser-workflows-rust/src/accessibility_dbus.rs"]
pub mod accessibility_dbus;
mod harness;
#[path = "../../browser-workflows-rust/src/keyboard.rs"]
pub mod keyboard;
mod protocols;
#[path = "../../browser-workflows-rust/src/vault_artifacts.rs"]
pub mod vault_artifacts;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn fail(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    message.into().into()
}
fn main() {
    let result = (|| -> Result<()> {
        if std::env::args().next().is_some_and(|name| {
            std::path::Path::new(&name)
                .file_name()
                .is_some_and(|name| name == "remmina")
        }) {
            return harness::dispatch();
        }
        let options = harness::Options::parse()?;
        if !options.inside {
            return harness::outer(&options);
        }
        harness::inside(options)
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
