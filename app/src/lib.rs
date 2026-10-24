//! Crate desktop app (Slint UI): the windows and their behavior. Archive work
//! lives in `crate-core`; `main.rs` only parses the command line.

pub mod instance;
pub mod mini;
pub mod model;
pub mod platform;

slint::include_modules!();
