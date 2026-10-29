//! First run (`design/canvas/FirstRun.dc.html`): once, over Home, the
//! double-click choice and which file types Crate should open. Get started
//! saves both and makes Crate the default app (on Linux with `xdg-mime`;
//! Flatpak can't, so it shows the steps instead; on Windows only the user
//! can, so it opens Settings → Default apps).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::i18n::{self, Language};
use crate::platform::{self, Defaults, FILE_TYPES};
use crate::prefs::Prefs;
use crate::settings::Settings;
use crate::{FirstRunUi, MainWindow, TypeChoice};

/// Set once First run is done or skipped.
const DONE_KEY: &str = "first_run_done";

/// Shows First run over Home, unless it ran before.
pub fn maybe_show(ui: &MainWindow) {
    if Settings::load().get::<u8>(DONE_KEY) == Some(1) {
        return;
    }
    show(ui);
}

/// Shows First run over Home.
pub fn show(ui: &MainWindow) {
    let defaults = platform::defaults_support();
    // Checked by default as in the mockup: every type Crate opens.
    let chosen = Rc::new(RefCell::new(vec![true; FILE_TYPES.len()]));
    let open_with = Rc::new(RefCell::new(Prefs::load().open_with as i32));
    let g = ui.global::<FirstRunUi>();
    fill(&g, &chosen.borrow());
    g.set_open_with(*open_with.borrow());
    let prefs = Prefs::load();
    g.set_language(prefs.language.map_or(0, |l| l as i32 + 1));
    g.set_system_language(crate::prefs::language_name(Language::system()).into());
    let weak = ui.as_weak();
    g.on_set_language(move |i| {
        // Saved at once, so the dialog itself switches.
        let mut s = Settings::load();
        s.set(
            "pref.language",
            ["system", "en", "ar"][i.clamp(0, 2) as usize],
        );
        s.save();
        i18n::set(Prefs::read(&s).ui_language());
        if let Some(ui) = weak.upgrade() {
            ui.global::<FirstRunUi>().set_language(i);
        }
    });
    g.set_defaults(defaults as i32);
    g.set_error("".into());

    let weak = ui.as_weak();
    let ow = open_with.clone();
    g.on_set_open_with(move |i| {
        *ow.borrow_mut() = i;
        if let Some(ui) = weak.upgrade() {
            ui.global::<FirstRunUi>().set_open_with(i);
        }
    });
    let weak = ui.as_weak();
    let ch = chosen.clone();
    g.on_toggle_type(move |i| {
        if let Some(on) = ch.borrow_mut().get_mut(i as usize) {
            *on = !*on;
        }
        if let Some(ui) = weak.upgrade() {
            fill(&ui.global::<FirstRunUi>(), &ch.borrow());
        }
    });
    let weak = ui.as_weak();
    g.on_skip(move || {
        if let Some(ui) = weak.upgrade() {
            finish(&ui, None);
        }
    });
    let weak = ui.as_weak();
    g.on_start(move || {
        let Some(ui) = weak.upgrade() else { return };
        // Windows: opens Settings → Default apps, where people pick Crate.
        if matches!(
            defaults,
            Defaults::Direct | Defaults::SettingsPage | Defaults::Confirm
        ) {
            let types: Vec<_> = FILE_TYPES
                .iter()
                .zip(chosen.borrow().iter())
                .filter(|(_, on)| **on)
                .map(|(t, _)| t)
                .collect();
            if let Err(e) = platform::make_default(&types) {
                // Stay open: Get started tries again, Skip leaves.
                ui.global::<FirstRunUi>().set_error(
                    i18n::trf(
                        "Couldn’t make Crate the default app: {error}",
                        &[("error", &e.to_string())],
                    )
                    .into(),
                );
                return;
            }
        }
        finish(&ui, Some(*open_with.borrow()));
    });
    g.set_show(true);
}

fn fill(g: &FirstRunUi, chosen: &[bool]) {
    let types: Vec<TypeChoice> = FILE_TYPES
        .iter()
        .zip(chosen)
        .map(|(t, &on)| TypeChoice {
            label: t.label.into(),
            on,
        })
        .collect();
    g.set_types(ModelRc::new(VecModel::from(types)));
}

/// Saves the choice (Get started) or nothing (Skip), and never shows again.
fn finish(ui: &MainWindow, open_with: Option<i32>) {
    let mut s = Settings::load();
    if let Some(i) = open_with {
        s.set("pref.open_with", i);
    }
    s.set(DONE_KEY, 1);
    s.save();
    ui.global::<FirstRunUi>().set_show(false);
}
