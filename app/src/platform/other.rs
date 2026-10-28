use std::time::Duration;

use super::{Defaults, MimeBackend};

/// macOS keeps the native title bar (M8).
pub const CUSTOM_FRAME: bool = false;

pub fn select_backend() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()
}

/// Time spent before `main`; only measured on Linux for now.
pub fn process_age() -> Option<Duration> {
    None
}

/// The system's monospace family (M7/M8: use the platform's own).
pub fn monospace_family() -> Option<String> {
    None
}

/// Default apps come with M8.
pub const SYSTEM_MIME: MimeBackend = MimeBackend {
    support: || Defaults::Unsupported,
    query: |_| None,
    set: |_, _| Err(std::io::Error::other("not supported yet")),
};

/// Dock progress comes with M8.
pub fn taskbar_progress(_window: &slint::Window, _fraction: Option<f32>) {}

/// The user's Downloads or Desktop folder from the system, when it says
/// (`settings::user_dir` reads the XDG user dirs instead).
pub fn known_folder(_key: &str) -> Option<std::path::PathBuf> {
    None
}
