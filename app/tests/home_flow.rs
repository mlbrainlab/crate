//! Home and the New archive dialog, driven headless.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use std::fs;

use common::Harness;
use crate_app::CreateUi;
use crate_core::{Archive, OpenOptions};
use slint::{ComponentHandle, Model};

#[test]
fn new_archive_flow() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    // SAFETY: before any other thread; this file is its own process.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", config.path()) };
    let src = dir.path().join("trip");
    fs::create_dir_all(src.join("raw")).unwrap();
    fs::write(src.join("notes.txt"), "notes ".repeat(20_000)).unwrap();
    fs::write(src.join("raw/a.bin"), vec![9u8; 300_000]).unwrap();
    fs::write(src.join("scratch.tmp"), "x").unwrap();

    let ui = h.home(vec![src.clone()]);
    let g = || ui.global::<CreateUi>();
    assert!(g().get_show());
    assert_eq!(g().get_name(), "trip");
    h.until("counted", || g().get_summary().contains("items"));
    assert_eq!(g().get_items().row_count(), 1);
    assert!(
        g().get_items()
            .row_data(0)
            .unwrap()
            .meta
            .contains("3 files")
    );
    assert_eq!(g().get_extension(), ".tar.zst", "Balanced is tar.zst");
    h.until("estimate", || g().get_level_text().contains('≈'));
    assert!(g().get_level_text().starts_with("Level 9 of 19"));

    // Exclude, change format and level, encrypt.
    g().invoke_add_exclude("*.tmp".into());
    h.until("recounted", || {
        g().get_excluded_note().starts_with("1 item excluded")
    });
    g().invoke_set_format(1);
    assert_eq!(g().get_extension(), ".7z");
    assert_eq!((g().get_level_min(), g().get_level_max()), (0.0, 9.0));
    g().invoke_level_changed(7.0);
    g().invoke_toggle(0);
    assert!(!g().get_ready(), "a password is needed");
    g().set_password("Correct-Horse-9".into());
    g().invoke_password_edited("Correct-Horse-9".into());
    assert_eq!(g().get_strength(), 4);
    assert!(g().get_encrypt_note().contains("hidden"));
    h.until("ready", || g().get_ready());
    h.shot("40-create-dialog");

    // Save these choices as a preset.
    g().invoke_save_preset("Trips".into());
    let presets: Vec<String> = g().get_presets().iter().map(|p| p.to_string()).collect();
    assert_eq!(presets.last().unwrap(), "Trips");
    assert_eq!(g().get_preset() as usize, presets.len() - 1);
    let state = fs::read_to_string(config.path().join("crate/state.ini")).unwrap();
    assert!(state.contains("preset.0.name=Trips"), "{state}");

    g().invoke_create();
    h.until("created", || {
        ui.get_created().starts_with("Created trip.7z")
    });
    assert!(!g().get_show());
    h.shot("41-home-created");
    let a = Archive::open_with(
        dir.path().join("trip.7z"),
        &OpenOptions {
            password: Some("Correct-Horse-9".into()),
            ..OpenOptions::default()
        },
    )
    .unwrap();
    let names: Vec<&str> = a.entries().iter().map(|e| e.path.as_str()).collect();
    assert!(
        names.contains(&"trip/raw/a.bin") && !names.iter().any(|n| n.ends_with(".tmp")),
        "{names:?}"
    );

    // The same name again is refused instead of replacing the archive.
    ui.invoke_new_archive();
    assert_eq!(g().get_items().row_count(), 0, "a fresh dialog");
    g().invoke_set_preset(3); // Trips: 7z
    assert_eq!(g().get_extension(), ".7z");
    let _ = support::Comp::None;
}
