//! Crate Pro: the group in Preferences, the upgrade sheet and entering a
//! key. Test builds have no signer or checkout link, so a store key gets
//! the "can't activate" answer and there's no Buy button.

mod common;

use common::Harness;
use crate_app::ProUi;
use slint::ComponentHandle;

#[test]
fn pro_flow() {
    let h = Harness::new();
    let ui = h.prefs();
    let p = || ui.global::<ProUi>();
    assert!(!p().get_active());
    assert!(!p().get_show());
    assert!(!p().get_can_buy(), "no checkout link in test builds");
    assert_eq!(p().get_price(), "$9");
    h.shot("140-prefs-pro");

    // Upgrade…: what Pro is.
    p().invoke_open(0);
    assert!(p().get_show());
    assert_eq!(p().get_page(), 0);
    h.shot("141-pro-sheet");

    // I have a license key: a key Crate didn't sign.
    p().invoke_open(1);
    assert_eq!(p().get_page(), 1);
    assert_eq!(p().get_key(), "");
    p().set_key("CRATE-PRO-bm90IGEga2V5".into());
    p().invoke_activate(p().get_key());
    h.until("answer", || !p().get_busy());
    assert_eq!(
        p().get_error(),
        "This key isn’t valid. Check that you copied all of it."
    );
    h.shot("142-pro-key-error");

    // A store key, in a build that can't ask the signer.
    p().set_key("38B1F2A4-91C0-4E7B-A2D5-0F6C3E9A7B11".into());
    p().invoke_activate(p().get_key());
    h.until("answer", || !p().get_busy());
    assert!(
        p().get_error()
            .starts_with("This copy of Crate can’t activate store keys"),
        "{}",
        p().get_error()
    );
    assert!(!p().get_active());

    // Cancel closes the sheet; nothing was saved.
    p().invoke_cancel();
    assert!(!p().get_show());
    assert!(crate_app::license::current().is_none());
}
