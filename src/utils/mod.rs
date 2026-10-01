//! Utility functions for hhead

pub mod caps;
pub mod color;
pub mod parsing;

pub use color::{rgb_to_256, xterm_256_rgb};
pub use parsing::parse_scale;
