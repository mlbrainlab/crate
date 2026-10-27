//! The keyboard shortcuts sheet (Ctrl+?) in Home, the full view and the mini
//! view, and the shortcuts it lists that are new in M5. Screenshots 8x-*.png.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use common::{Harness, g};
use crate_app::ShortcutsUi;
use slint::platform::Key;
use slint::{ComponentHandle, Model};

fn labels<C>(ui: &C) -> Vec<String>
where
    C: ComponentHandle,
    for<'a> ShortcutsUi<'a>: slint::Global<'a, C>,
{
    ui.global::<ShortcutsUi>()
        .get_groups()
        .iter()
        .flat_map(|g| {
            g.items
                .iter()
                .map(|i| i.label.to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn shortcuts_flow() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);

    // Home: Ctrl+? opens the sheet with what works there.
    let home = h.home(vec![]);
    home.global::<crate_app::FirstRunUi>().invoke_skip();
    h.key_with(Key::Control, "?");
    assert!(home.global::<ShortcutsUi>().get_show());
    let l = labels(&home);
    assert!(
        l.contains(&"New archive".to_owned()) && !l.contains(&"Extract all".to_owned()),
        "{l:?}"
    );
    h.shot("80-shortcuts-home");
    h.key(Key::Escape);
    assert!(!home.global::<ShortcutsUi>().get_show());
    // F1 and Ctrl+/ too, for layouts where Ctrl+? doesn't come through.
    h.key(Key::F1);
    assert!(home.global::<ShortcutsUi>().get_show(), "F1");
    h.key(Key::F1);
    assert!(!home.global::<ShortcutsUi>().get_show());
    h.key_with(Key::Control, "/");
    assert!(home.global::<ShortcutsUi>().get_show(), "Ctrl+/");
    h.key(Key::Escape);

    // The full view: four groups; search and the macOS keys.
    support::sample_zip(&path("project-assets.zip"));
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_toggle_full();
    h.key_with_all(&[Key::Control, Key::Shift], "/");
    let s = || ui.global::<ShortcutsUi>();
    assert!(s().get_show());
    assert_eq!(s().get_groups().row_count(), 4);
    h.shot("81-shortcuts-full");
    s().set_query("sidebar".into());
    s().invoke_search("sidebar".into());
    assert_eq!(labels(&ui), ["Toggle sidebar"]);
    s().set_query("".into());
    s().invoke_search("".into());
    s().invoke_set_mac(true);
    assert_eq!(s().get_modifier(), "⌘");
    s().invoke_set_mac(false);
    h.key(Key::Escape);
    assert!(!s().get_show());

    // What the sheet promises.
    let grid = g(&ui).get_grid();
    h.key_with(Key::Control, "g");
    assert_eq!(g(&ui).get_grid(), !grid, "Ctrl+G");
    h.key_with(Key::Control, "g");
    let sidebar = g(&ui).get_sidebar();
    h.key(Key::F9);
    assert_eq!(g(&ui).get_sidebar(), !sidebar, "F9");
    h.key(Key::F9);
    h.key_with(Key::Alt, Key::Return);
    assert!(g(&ui).get_pr_show(), "Alt+Enter");
    h.key(Key::Escape);

    // Ctrl+Shift+E: the selection; Ctrl+E: everything, whatever is selected.
    g(&ui).invoke_row_activated(0); // into project/
    g(&ui).invoke_row_pressed(1, false, false); // README.md
    h.key_with_all(&[Key::Control, Key::Shift], "e");
    h.until("done", || g(&ui).get_show_done());
    assert!(path("project/README.md").is_file());
    assert!(!path("project/run.sh").exists(), "only the selection");
    g(&ui).invoke_back_to_archive();
    h.key_with(Key::Control, "e");
    h.until("done", || g(&ui).get_show_done());
    assert!(
        path("project-assets/project/run.sh").is_file(),
        "everything"
    );
    g(&ui).invoke_back_to_archive();
    // Last: it moves the keyboard focus into the search field.
    let asked = g(&ui).get_focus_search();
    h.key_with(Key::Control, "f");
    assert_eq!(g(&ui).get_focus_search(), asked + 1, "Ctrl+F");

    // Mini view: the sheet in one column; Ctrl+T shows the test in Properties.
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    h.key_with(Key::Control, "?");
    h.shot("82-shortcuts-mini");
    h.key(Key::Escape);
    h.key_with(Key::Control, "t");
    assert!(g(&ui).get_pr_show() && g(&ui).get_pr_tab() == 1);
    h.until("tested", || g(&ui).get_test_state() == 2);

    // Arabic: mirrored, keys still read left to right.
    let mut st = crate_app::settings::Settings::load();
    st.set("pref.language", "ar");
    st.save();
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_toggle_full();
    h.key_with(Key::Control, "?");
    assert!(labels(&ui).contains(&"استخراج الكل".to_owned()));
    h.shot("83-ar-shortcuts");
}
