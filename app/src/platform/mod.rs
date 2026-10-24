//! Platform-only code. Linux is implemented; Windows and macOS are stubs until M7/M8.

/// Reverse-DNS app id; also the Wayland app id and, later, the .desktop file name.
pub const APP_ID: &str = "io.github.mlbrainlab.crate";

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(not(target_os = "linux"))]
mod other;
#[cfg(not(target_os = "linux"))]
pub use other::*;
