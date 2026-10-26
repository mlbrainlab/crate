//! First run over Home, and the "Open these with Crate" row in Preferences,
//! with default apps recorded in memory instead of `mimeapps.list`.

mod common;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

use common::Harness;
use crate_app::platform::{self, Defaults, MimeBackend};
use crate_app::prefs::{OpenWith, Prefs};
use crate_app::{FirstRunUi, PrefsUi};
use slint::{ComponentHandle, Model};

/// MIME type → .desktop file, as `xdg-mime` would have set them.
static DEFAULTS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// `Defaults` as a number; 9: Direct, but every write fails.
static MODE: AtomicU8 = AtomicU8::new(0);
const FAILING: u8 = 9;

const FAKE: MimeBackend = MimeBackend {
    support: || match MODE.load(Ordering::Relaxed) {
        1 => Defaults::Flatpak,
        2 => Defaults::NotInstalled,
        3 => Defaults::Unsupported,
        _ => Defaults::Direct,
    },
    query: |mime| DEFAULTS.lock().unwrap().as_ref()?.get(mime).cloned(),
    set: |desktop, mimes| {
        if MODE.load(Ordering::Relaxed) == FAILING {
            return Err(std::io::Error::other("xdg-mime failed (exit status: 4)"));
        }
        let mut d = DEFAULTS.lock().unwrap();
        let map = d.get_or_insert_with(HashMap::new);
        for m in mimes {
            map.insert(m.to_string(), desktop.to_owned());
        }
        Ok(())
    },
};

fn default_for(mime: &str) -> Option<String> {
    DEFAULTS.lock().unwrap().as_ref()?.get(mime).cloned()
}

fn labels_on(types: slint::ModelRc<crate_app::TypeChoice>) -> Vec<String> {
    types
        .iter()
        .filter(|t| t.on)
        .map(|t| t.label.to_string())
        .collect()
}

#[test]
fn first_run_flow() {
    let h = Harness::new();
    platform::set_mime_backend(FAKE);
    let crate_desktop = Some("io.github.mlbrainlab.crate.desktop".to_owned());

    // The first Home shows it, every type checked.
    let home = h.home(vec![]);
    let f = || home.global::<FirstRunUi>();
    assert!(f().get_show());
    assert_eq!(f().get_defaults(), 0);
    assert_eq!(
        labels_on(f().get_types()),
        [".zip", ".7z", ".rar", ".tar.*"]
    );
    h.shot("60-first-run");

    // Full view, everything but 7z; Get started sets both.
    f().invoke_set_open_with(1);
    f().invoke_toggle_type(1);
    assert_eq!(labels_on(f().get_types()), [".zip", ".rar", ".tar.*"]);
    f().invoke_start();
    assert!(!f().get_show());
    assert_eq!(Prefs::load().open_with, OpenWith::Full);
    for mime in [
        "application/zip",
        "application/x-zip-compressed",
        "application/vnd.rar",
        "application/x-zstd-compressed-tar",
    ] {
        assert_eq!(default_for(mime), crate_desktop, "{mime}");
    }
    assert_eq!(default_for("application/x-7z-compressed"), None);

    // Only once.
    let home = h.home(vec![]);
    assert!(!home.global::<FirstRunUi>().get_show());

    // Preferences shows what the system says, and a click adds 7z.
    let prefs = h.prefs();
    let p = || prefs.global::<PrefsUi>();
    assert_eq!(p().get_defaults(), 0);
    assert_eq!(labels_on(p().get_types()), ["zip", "rar", "tar.*"]);
    p().invoke_make_default(1);
    assert_eq!(labels_on(p().get_types()), ["zip", "7z", "rar", "tar.*"]);
    assert_eq!(default_for("application/x-7z-compressed"), crate_desktop);
    assert_eq!(p().get_defaults_error(), "");
    // Tall enough to show the row at the bottom.
    prefs
        .window()
        .set_size(slint::LogicalSize::new(760.0, 1240.0));
    h.shot("63-prefs-file-types");

    // Flatpak: no checkboxes, the steps instead, and nothing is written.
    MODE.store(Defaults::Flatpak as u8, Ordering::Relaxed);
    DEFAULTS.lock().unwrap().take();
    let home = h.home(vec![]);
    crate_app::first_run::show(&home);
    let f = || home.global::<FirstRunUi>();
    assert_eq!(f().get_defaults(), 1);
    h.shot("61-first-run-flatpak");
    f().invoke_start();
    assert!(!f().get_show());
    assert_eq!(default_for("application/zip"), None);

    // Not installed as an app (cargo run): the same, with its own note.
    MODE.store(Defaults::NotInstalled as u8, Ordering::Relaxed);
    let prefs = h.prefs();
    assert_eq!(prefs.global::<PrefsUi>().get_defaults(), 2);
    assert!(labels_on(prefs.global::<PrefsUi>().get_types()).is_empty());

    // xdg-mime fails: the dialog stays with the reason; Skip leaves.
    MODE.store(FAILING, Ordering::Relaxed);
    let home = h.home(vec![]);
    let f = || home.global::<FirstRunUi>();
    crate_app::first_run::show(&home);
    f().invoke_start();
    assert!(f().get_show());
    assert!(
        f().get_error().contains("xdg-mime failed"),
        "{}",
        f().get_error()
    );
    h.shot("62-first-run-error");
    f().invoke_skip();
    assert!(!f().get_show());
}
