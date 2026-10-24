//! Startup spike: the same static mini view as the Tauri app, in Slint.
//! `--startup-probe` logs the same timeline as `crate-app` and exits after the
//! first rendered frame, so scripts/startup-bench.sh can measure both.

use std::time::{Duration, Instant};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let started = Instant::now();
    let pre_main = process_age();
    let probe = std::env::args().any(|a| a == "--startup-probe");

    let ui = MiniView::new()?;
    let built = started.elapsed();

    let mut shown = false;
    ui.window()
        .set_rendering_notifier(move |state, _| {
            if probe && !shown && matches!(state, slint::RenderingState::AfterRendering) {
                shown = true;
                let ms = |d: Duration| d.as_secs_f64() * 1e3;
                eprintln!(
                    "crate-startup view=mini premain_ms={:.0} window_ms={:.1} ready_ms={:.1} visible_ms={:.1} signal=after-render",
                    pre_main.map(ms).unwrap_or_default(),
                    ms(built),
                    ms(built),
                    ms(started.elapsed()),
                );
                std::process::exit(0);
            }
        })
        .expect("rendering notifier unsupported by this renderer");

    ui.run()
}

/// Process age at this point (exec → main), from /proc; same as the Tauri app.
fn process_age() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
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
    let age = uptime - start_ticks / 100.0;
    (age >= 0.0).then(|| Duration::from_secs_f64(age))
}
