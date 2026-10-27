//! The Arabic UI: translated text and the right-to-left layout, in the mini
//! view, the full view, Home and Preferences. Screenshots: 7x-*.png.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use common::{Harness, g};
use crate_app::i18n::{self, Language};
use slint::ComponentHandle;

#[test]
fn arabic_flow() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);
    support::sample_zip(&path("project-assets.zip"));

    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    assert!(!ui.global::<crate_app::Locale>().get_rtl());
    i18n::set(Language::Arabic);
    assert!(
        ui.global::<crate_app::Locale>().get_rtl(),
        "follows the translation"
    );
    assert_eq!(
        i18n::tr("default:LTR"),
        "default:RTL",
        "Rust uses the same catalog"
    );

    // Language: Arabic in Preferences; windows opened afterwards use it.
    set_language("ar");
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    assert!(ui.global::<crate_app::Locale>().get_rtl());
    assert!(
        g(&ui).get_summary().contains("عناصر"),
        "{}",
        g(&ui).get_summary()
    );
    h.shot("70-ar-mini");
    g(&ui).invoke_row_activated(0);
    h.shot("71-ar-mini-folder");
    g(&ui).invoke_toggle_full();
    g(&ui).invoke_row_pressed(1, false, false);
    h.shot("72-ar-full");
    g(&ui).invoke_extract_dialog();
    h.shot("73-ar-extract-dialog");
    g(&ui).invoke_ex_cancel();
    g(&ui).invoke_open_properties();
    h.shot("74-ar-properties");
    g(&ui).invoke_pr_close();

    // MiniViewRTL: old Arabic names, the picker, and the names fixed.
    support::arabic_cp1256_zip(&path("arabic.zip"));
    let ui = h.open(&path("arabic.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    assert!(g(&ui).get_show_banner());
    h.shot("78-ar-banner");
    g(&ui).invoke_open_picker();
    g(&ui).set_codec_remember(true);
    h.shot("79-ar-picker");
    g(&ui).invoke_apply_codec();
    assert!(!g(&ui).get_show_banner());
    h.shot("79b-ar-names-fixed");

    // Home with First run, the New archive dialog, and Preferences.
    let home = h.home(vec![]);
    h.shot("75-ar-first-run");
    home.global::<crate_app::FirstRunUi>().invoke_skip();
    let src = dir.path().join("رحلة");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("notes.txt"), "notes").unwrap();
    let home = h.home(vec![src]);
    h.until("counted", || {
        home.global::<crate_app::CreateUi>()
            .get_summary()
            .contains("عنصر")
    });
    h.shot("76-ar-create");
    let prefs = h.prefs();
    prefs
        .window()
        .set_size(slint::LogicalSize::new(760.0, 1240.0));
    h.shot("77-ar-prefs");

    set_language("system");
    let ui = h.open(&path("project-assets.zip"));
    assert!(!ui.global::<crate_app::Locale>().get_rtl());
}

fn set_language(code: &str) {
    let mut s = crate_app::settings::Settings::load();
    s.set("pref.language", code);
    s.save();
}
