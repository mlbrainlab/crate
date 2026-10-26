//! The keyboard shortcuts sheet (`design/canvas/Shortcuts.dc.html`): what it
//! lists in each window, its search and its Linux / macOS keys. The keys
//! themselves are handled in `archive-window.slint` and `main-window.slint`;
//! this list must say what they do.

use std::cell::Cell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::i18n::tr;
use crate::{ShortcutGroup, ShortcutRow, ShortcutsUi};

/// Where a shortcut works.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Home,
    Archive,
}

struct Shortcut {
    group: usize,
    label: fn() -> String,
    pc: &'static [&'static str],
    mac: &'static [&'static str],
    home: bool,
    archive: bool,
}

const fn s(
    group: usize,
    label: fn() -> String,
    pc: &'static [&'static str],
    mac: &'static [&'static str],
    home: bool,
    archive: bool,
) -> Shortcut {
    Shortcut {
        group,
        label,
        pc,
        mac,
        home,
        archive,
    }
}

fn group_title(i: usize) -> String {
    match i {
        0 => tr("Archive"),
        1 => tr("Browse"),
        2 => tr("Edit"),
        _ => tr("App"),
    }
}

/// The board's list, minus what Crate can't do yet (copy, paste into the
/// archive, operations, zoom, update edited files).
const SHORTCUTS: &[Shortcut] = &[
    s(
        0,
        || tr("New archive"),
        &["Ctrl", "N"],
        &["⌘", "N"],
        true,
        false,
    ),
    s(
        0,
        || tr("Open archive"),
        &["Ctrl", "O"],
        &["⌘", "O"],
        true,
        false,
    ),
    s(
        0,
        || tr("Extract all"),
        &["Ctrl", "E"],
        &["⌘", "E"],
        false,
        true,
    ),
    s(
        0,
        || tr("Extract selected"),
        &["Ctrl", "Shift", "E"],
        &["⌘", "⇧", "E"],
        false,
        true,
    ),
    s(
        0,
        || tr("Test integrity"),
        &["Ctrl", "T"],
        &["⌘", "T"],
        false,
        true,
    ),
    s(
        0,
        || tr("Properties"),
        &["Alt", "Enter"],
        &["⌘", "I"],
        false,
        true,
    ),
    s(
        0,
        || tr("Close window"),
        &["Ctrl", "W"],
        &["⌘", "W"],
        true,
        true,
    ),
    s(
        1,
        || tr("Quick preview"),
        &["Space"],
        &["Space"],
        false,
        true,
    ),
    s(1, || tr("Open file"), &["Enter"], &["⌘", "↓"], false, true),
    s(
        1,
        || tr("Up one folder"),
        &["Alt", "↑"],
        &["⌘", "↑"],
        false,
        true,
    ),
    s(
        1,
        || tr("Filter / search"),
        &["Ctrl", "F"],
        &["⌘", "F"],
        false,
        true,
    ),
    s(
        1,
        || tr("Select all"),
        &["Ctrl", "A"],
        &["⌘", "A"],
        false,
        true,
    ),
    s(
        1,
        || tr("List / grid view"),
        &["Ctrl", "G"],
        &["⌘", "G"],
        false,
        true,
    ),
    s(1, || tr("Toggle sidebar"), &["F9"], &["F9"], false, true),
    s(
        2,
        || tr("Add files"),
        &["Ctrl", "Shift", "A"],
        &["⌘", "⇧", "A"],
        false,
        true,
    ),
    s(
        2,
        || tr("New folder"),
        &["Ctrl", "Shift", "N"],
        &["⌘", "⇧", "N"],
        false,
        true,
    ),
    s(2, || tr("Rename"), &["F2"], &["Return"], false, true),
    s(
        2,
        || tr("Delete from archive"),
        &["Del"],
        &["⌘", "⌫"],
        false,
        true,
    ),
    s(
        3,
        || tr("Mini ↔ full view"),
        &["Ctrl", "Enter"],
        &["⌘", "Enter"],
        false,
        true,
    ),
    s(
        3,
        || tr("Preferences"),
        &["Ctrl", ","],
        &["⌘", ","],
        true,
        true,
    ),
    s(
        3,
        || tr("Shortcuts"),
        &["Ctrl", "?"],
        &["⌘", "?"],
        true,
        true,
    ),
    s(3, || tr("Quit"), &["Ctrl", "Q"], &["⌘", "Q"], true, true),
];

/// The groups to show: in `window`, matching `query` (ignoring case).
fn groups(window: Window, mac: bool, query: &str) -> Vec<ShortcutGroup> {
    let query = query.trim().to_lowercase();
    (0..4)
        .filter_map(|g| {
            let items: Vec<ShortcutRow> = SHORTCUTS
                .iter()
                .filter(|s| s.group == g)
                .filter(|s| match window {
                    Window::Home => s.home,
                    Window::Archive => s.archive,
                })
                .map(|s| ((s.label)(), s))
                .filter(|(label, s)| {
                    query.is_empty()
                        || label.to_lowercase().contains(&query)
                        || s.pc.join(" ").to_lowercase().contains(&query)
                })
                .map(|(label, s)| {
                    let keys: Vec<SharedString> = (if mac { s.mac } else { s.pc })
                        .iter()
                        .map(|k| match *k {
                            "Space" => tr("Space"),
                            other => other.to_owned(),
                        })
                        .map(SharedString::from)
                        .collect();
                    ShortcutRow {
                        label: label.into(),
                        keys: ModelRc::new(VecModel::from(keys)),
                    }
                })
                .collect();
            (!items.is_empty()).then(|| ShortcutGroup {
                title: group_title(g).into(),
                items: ModelRc::new(VecModel::from(items)),
            })
        })
        .collect()
}

/// Wires the sheet of one window: Ctrl+? calls `open`, the search field
/// `search`, the platform switch `set-mac`.
pub fn connect<C>(ui: &C, window: Window)
where
    C: ComponentHandle + 'static,
    for<'a> ShortcutsUi<'a>: slint::Global<'a, C>,
{
    let mac = Rc::new(Cell::new(cfg!(target_os = "macos")));
    let fill = {
        let weak = ui.as_weak();
        let mac = mac.clone();
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let g = ui.global::<ShortcutsUi>();
            let query = g.get_query();
            g.set_mac(mac.get());
            g.set_modifier(if mac.get() { "⌘" } else { "Ctrl" }.into());
            g.set_groups(ModelRc::new(VecModel::from(groups(
                window,
                mac.get(),
                &query,
            ))));
        }
    };
    let fill = Rc::new(fill);
    let g = ui.global::<ShortcutsUi>();
    let f = fill.clone();
    let weak = ui.as_weak();
    g.on_open(move || {
        if let Some(ui) = weak.upgrade() {
            ui.global::<ShortcutsUi>().set_query("".into());
        }
        f();
        if let Some(ui) = weak.upgrade() {
            ui.global::<ShortcutsUi>().set_show(true);
        }
    });
    let f = fill.clone();
    g.on_search(move |_| f());
    g.on_set_mac(move |m| {
        mac.set(m);
        fill();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    fn labels(groups: &[ShortcutGroup]) -> Vec<String> {
        groups
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
    fn lists_per_window_and_filters() {
        let home = groups(Window::Home, false, "");
        assert!(labels(&home).contains(&"New archive".to_owned()));
        assert!(!labels(&home).contains(&"Extract all".to_owned()));
        let archive = groups(Window::Archive, false, "");
        assert_eq!(archive.len(), 4);
        assert!(!labels(&archive).contains(&"New archive".to_owned()));
        assert_eq!(labels(&groups(Window::Archive, false, "EXTRACT")).len(), 2);
        assert_eq!(
            labels(&groups(Window::Archive, false, "f9")),
            ["Toggle sidebar"]
        );
        assert!(groups(Window::Archive, false, "nothing like this").is_empty());
        let mac = groups(Window::Archive, true, "properties");
        assert_eq!(mac[0].items.row_data(0).unwrap().keys.row_count(), 2);
    }
}
