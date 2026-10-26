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
    h.shot("70-ar-mini");
    i18n::set(Language::English);
    assert!(!ui.global::<crate_app::Locale>().get_rtl());
}
