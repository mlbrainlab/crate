use tauri::{Manager, Runtime, WebviewWindowBuilder};

/// Linux draws its own header bar (design/DESIGN.md → Window frame), so the
/// window has no system decorations.
pub fn window_chrome<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    builder.decorations(false)
}

/// How long the process existed before this call: exec, dynamic loading of
/// WebKitGTK/GTK and static initializers all happen before `main` runs.
/// Read from /proc, so the resolution is one clock tick (10 ms).
pub fn process_age() -> Option<std::time::Duration> {
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
    (age >= 0.0).then(|| std::time::Duration::from_secs_f64(age))
}
