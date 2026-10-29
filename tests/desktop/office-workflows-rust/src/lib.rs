pub mod control;
pub mod corpus;
pub mod dialogs;
pub mod formats;
pub mod keyboard;
pub mod live;
pub mod pdf_viewer;
pub mod rasters;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    message.into().into()
}
pub fn ensure(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(error(message))
    }
}
