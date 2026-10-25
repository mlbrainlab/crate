use std::time::Duration;

use slint::winit_030::winit::platform::wayland::WindowAttributesExtWayland;

/// Linux draws its own header bar (design/DESIGN.md → Window frame).
pub const CUSTOM_FRAME: bool = true;

/// Winit backend with Crate's app id, so GNOME matches windows to the
/// .desktop file and its icon.
pub fn select_backend() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(|attrs| attrs.with_name(super::APP_ID, "crate"))
        .select()
}

/// How long the process existed before this call: exec, dynamic loading and
/// static initializers all happen before `main` runs. Read from /proc, so the
/// resolution is one clock tick (10 ms).
pub fn process_age() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Fields after the ")" that closes the command name; starttime is field 22 overall.
    let start_ticks: f64 = stat
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()?;
    let uptime: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let ticks_per_sec = 100.0; // USER_HZ, fixed at 100 on Linux.
    let age = uptime - start_ticks / ticks_per_sec;
    (age >= 0.0).then(|| Duration::from_secs_f64(age))
}

/// The family fontconfig picks for "monospace" (e.g. "DejaVu Sans Mono").
pub fn monospace_family() -> Option<String> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}", "monospace"])
        .output()
        .ok()?;
    let family = String::from_utf8(out.stdout).ok()?;
    let family = family.trim();
    (out.status.success() && !family.is_empty()).then(|| family.to_owned())
}
