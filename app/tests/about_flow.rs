//! About & support (About.dc.html): the About sheet from Home's header, its
//! links, the Support box once there are donation links, and the one-time
//! nudge. Screenshots 9x-*.png.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use common::Harness;
use crate_app::AboutUi;
use crate_app::settings::Settings;
use slint::ComponentHandle;
use slint::platform::Key;

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn record(path: &Path) -> std::io::Result<()> {
    OPENED.lock().unwrap().push(path.to_owned());
    Ok(())
}

fn set(key: &str, value: impl ToString) {
    let mut s = Settings::load();
    s.set(key, value);
    s.save();
}

#[test]
fn about_flow() {
    let h = Harness::new();
    crate_app::home::set_opener(record);
    set("first_run_done", 1);

    // The sheet, as it ships today: no donation accounts, so no Support box.
    let home = h.home(vec![]);
    let a = || home.global::<AboutUi>();
    assert!(!a().get_nudge() && !a().get_has_support());
    a().invoke_open();
    assert!(a().get_show());
    assert!(
        a().get_version().starts_with("Version "),
        "{}",
        a().get_version()
    );
    h.shot("90-about");
    a().invoke_open_link("issues".into());
    assert_eq!(
        OPENED.lock().unwrap().last().unwrap(),
        Path::new("https://github.com/mlbrainlab/crate/issues/new")
    );
    h.key(Key::Escape);
    assert!(!a().get_show());

    // With donation links (sample ones), the Support box shows.
    a().set_sponsor("https://github.com/sponsors/example".into());
    a().set_patreon("https://patreon.com/example".into());
    a().set_paypal("https://paypal.me/example".into());
    a().invoke_open();
    h.shot("91-about-support");
    h.key(Key::Escape);

    // 50 operations: the nudge on Home. "Maybe later" puts it off 50 more.
    for _ in 0..50 {
        crate_app::about::count_success();
    }
    let home = h.home(vec![]);
    let a = || home.global::<AboutUi>();
    assert!(a().get_nudge());
    assert_eq!(
        a().get_nudge_title(),
        "Crate has handled 50 archives for you"
    );
    a().set_sponsor("https://github.com/sponsors/example".into());
    h.shot("92-nudge");
    a().invoke_nudge_later();
    assert!(!a().get_nudge());
    assert_eq!(Settings::load().get::<u64>("support.next"), Some(100));
    let home = h.home(vec![]);
    assert!(
        !home.global::<AboutUi>().get_nudge(),
        "not again before 100"
    );

    // "Support" opens About and the nudge never comes back.
    set("support.next", 50);
    let home = h.home(vec![]);
    let a = || home.global::<AboutUi>();
    assert!(a().get_nudge());
    a().invoke_nudge_support();
    assert!(a().get_show() && !a().get_nudge());
    assert_eq!(Settings::load().get::<u8>("support.never"), Some(1));

    // Arabic: mirrored.
    set("pref.language", "ar");
    let home = h.home(vec![]);
    let a = || home.global::<AboutUi>();
    a().set_sponsor("https://github.com/sponsors/example".into());
    a().set_patreon("https://patreon.com/example".into());
    a().set_paypal("https://paypal.me/example".into());
    a().invoke_open();
    h.shot("93-ar-about");
}
