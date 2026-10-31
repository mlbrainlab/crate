//! Accessibility pass: every control a screen reader can land on has a
//! name, and the keyboard reaches the controls (Tab, then Space or Enter).
//! Uses Slint's element search, which needs the element debug info that
//! `build.rs` emits for debug builds. Screenshot 95: a focus ring.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use common::{Harness, g};
use crate_app::{AboutUi, ShortcutsUi};
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::ComponentHandle;
use slint::platform::Key;
use std::ops::ControlFlow;

/// Controls without a name: "<role> in <screen>". Also checks the screen
/// has at least `at_least` controls, so a broken search can't pass.
fn unnamed(root: &impl ElementRoot, screen: &str, at_least: usize) -> Vec<String> {
    let mut missing = Vec::new();
    let mut seen = 0;
    root.root_element().visit_descendants(|e| {
        let role = e.accessible_role();
        let interactive = matches!(
            role,
            Some(
                AccessibleRole::Button
                    | AccessibleRole::Checkbox
                    | AccessibleRole::Switch
                    | AccessibleRole::Combobox
                    | AccessibleRole::Tab
                    | AccessibleRole::ListItem
            )
        );
        seen += usize::from(interactive);
        if interactive && e.accessible_label().unwrap_or_default().trim().is_empty() {
            missing.push(format!(
                "{role:?} {} in {screen}",
                e.id().unwrap_or_default()
            ));
        }
        ControlFlow::<()>::Continue(())
    });
    eprintln!("{screen}: {seen} controls");
    assert!(seen >= at_least, "{screen}: only {seen} controls found");
    missing
}

#[test]
fn accessibility_flow() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);
    let mut missing = Vec::new();

    // Home: Tab reaches the header buttons; Space presses them.
    let home = h.home(vec![]);
    home.global::<crate_app::FirstRunUi>().invoke_skip();
    missing.extend(unnamed(&home, "Home", 5));
    h.key(Key::Tab);
    h.shot("95-focus-ring");
    h.key(" ");
    assert!(
        home.global::<crate_app::OpsUi>().get_show(),
        "Tab, Space: Operations"
    );
    h.key(Key::Escape);
    h.key(Key::Tab);
    h.key(" ");
    assert!(
        home.global::<ShortcutsUi>().get_show(),
        "Tab again, Space: Keyboard shortcuts"
    );
    missing.extend(unnamed(&home, "Shortcuts", 6));
    h.key(Key::Escape);
    // A screen reader's "press" (the default action) works like a click.
    let about = ElementHandle::find_by_accessible_label(&home, "About Crate")
        .next()
        .expect("About button");
    about.invoke_accessible_default_action();
    assert!(home.global::<AboutUi>().get_show());
    missing.extend(unnamed(&home, "About", 8));
    h.key(Key::Escape);

    // Operations (with a finished job in the history).
    crate_app::ops::Op::start(
        crate_app::ops::Kind::Extract,
        "Extracting a.zip".into(),
        String::new(),
        crate_core::CancelToken::new(),
    )
    .finish(
        crate_app::ops::Outcome::Done,
        "Extracted a.zip → ~/a/",
        Some(std::path::Path::new("/tmp")),
    );
    home.global::<crate_app::OpsUi>().invoke_open();
    missing.extend(unnamed(&home, "Operations", 7));
    h.key(Key::Escape);

    // Preferences.
    let prefs = h.prefs();
    missing.extend(unnamed(&prefs, "Preferences", 10));

    // Mini view, full view and the dialogs over them.
    support::sample_zip(&path("project-assets.zip"));
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || {
        g(&ui).get_status() == "ready" && !g(&ui).get_streaming()
    });
    missing.extend(unnamed(&ui, "mini view", 4));
    g(&ui).invoke_toggle_full();
    g(&ui).invoke_row_pressed(0, false, false);
    missing.extend(unnamed(&ui, "full view", 15));
    g(&ui).invoke_extract_dialog();
    missing.extend(unnamed(&ui, "Extract dialog", 12));
    g(&ui).invoke_ex_cancel();
    g(&ui).invoke_open_properties();
    missing.extend(unnamed(&ui, "Properties", 5));
    g(&ui).invoke_pr_close();

    assert!(
        missing.is_empty(),
        "controls without a name:\n{}",
        missing.join("\n")
    );
}
