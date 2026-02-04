//! File format detection and metadata extraction

pub mod detection;
pub mod metadata;

pub use detection::detect_file_format;
pub use metadata::extract_format_metadata;