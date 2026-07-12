//! hhead - Hex dump utility with color and UTF-8 support
//!
//! This library provides file format detection, hex dumping, and image minimap functionality.

pub mod cli;
pub mod display;
pub mod formats;
pub mod io;
pub mod utils;

/// Serializes tests that flip the `colored` crate's global override flag;
/// without it, parallel tests can unset each other's override mid-capture.
#[cfg(test)]
pub(crate) static COLOR_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
