// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // First thing in the process: the startup clock (docs/perf.md).
    let started = std::time::Instant::now();
    crate_app_lib::run(started)
}
