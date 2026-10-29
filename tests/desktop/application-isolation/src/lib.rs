//! Shared Rust 2024 helpers for isolated desktop acceptance fixtures.
pub mod audio;
pub mod provenance;
pub mod seat_protocol;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
