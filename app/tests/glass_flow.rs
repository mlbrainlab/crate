//! The Glass theme (Pro): the Theme row in Preferences, and archive windows
//! in glass. Test builds can't make a real key, so the windows get glass the
//! way `Prefs::apply_archive_theme` gives it to them. Shots are composited
//! over a stand-in wallpaper.

#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

mod common;

use common::{Harness, g};
use crate_app::prefs::{Prefs, Theme};
use crate_app::{PrefsUi, ProUi, Tokens};
use slint::ComponentHandle;

#[test]
fn glass_flow() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();

    // Without a key, Glass carries PRO and opens the upgrade sheet.
    let prefs = h.prefs();
    prefs.global::<PrefsUi>().invoke_choose("theme".into(), 3);
    assert_eq!(Prefs::load().theme, Theme::System, "no key, no Glass");
    h.shot("150-prefs-glass-pro");
    prefs.global::<ProUi>().invoke_open(0);
    assert!(prefs.global::<ProUi>().get_show());
    prefs.global::<ProUi>().invoke_cancel();

    // The mini view in glass.
    support::sample_zip(&dir.path().join("project-assets.zip"));
    let ui = h.open(&dir.path().join("project-assets.zip"));
    h.until("listing", || {
        g(&ui).get_status() == "ready" && !g(&ui).get_streaming()
    });
    ui.global::<Tokens>().set_glass(true);
    h.shot("151-glass-mini");
    g(&ui).invoke_row_pressed(0, false, false);
    h.shot("152-glass-mini-selected");

    // The full view in glass.
    g(&ui).invoke_toggle_full();
    h.until("full", || g(&ui).get_full());
    h.shot("153-glass-full");
}
