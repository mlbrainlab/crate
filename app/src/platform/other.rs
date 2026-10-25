use std::time::Duration;

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
