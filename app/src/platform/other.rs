use std::time::Duration;

use super::{Defaults, MimeBackend};

/// Windows and macOS keep the native title bar (M7/M8).
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

/// Default apps come with M7 (Windows: the Settings page) and M8 (macOS).
pub const SYSTEM_MIME: MimeBackend = MimeBackend {
    support: || Defaults::Unsupported,
    query: |_| None,
    set: |_, _| Err(std::io::Error::other("not supported yet")),
};
