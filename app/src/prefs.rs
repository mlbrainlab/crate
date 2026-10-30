//! Preferences (`design/canvas/Settings.dc.html`): the typed view of the
//! `pref.*` keys in [`Settings`], and the window that edits them.
//!
//! Each window reads them when it opens. Every archive is its own process,
//! so a change shows in windows opened afterwards; the theme also changes
//! the Preferences window itself (and Home, when it opened it).

use std::cell::RefCell;
use std::rc::Rc;

use crate_core::create::OutFormat;
use crate_core::encoding::NameCodec;
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::i18n::{self, Language};
use crate::platform::{self, Defaults, FILE_TYPES};
use crate::settings::Settings;
use crate::{PrefsUi, PrefsWindow, TypeChoice};

/// "When I double-click an archive".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenWith {
    #[default]
    Mini = 0,
    Full = 1,
    /// Extract everything to the default place straight away.
    Extract = 2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    System = 0,
    Light = 1,
    Dark = 2,
}

/// Default compression level, per format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    Fastest = 0,
    #[default]
    Balanced = 1,
    Smallest = 2,
}

/// The formats Preferences offers for new archives, in its order.
pub const NEW_FORMATS: [OutFormat; 5] = [
    OutFormat::Zip,
    OutFormat::TarZst,
    OutFormat::SevenZ,
    OutFormat::TarGz,
    OutFormat::TarXz,
];

/// What "Always exclude" leaves out of new archives.
pub const ALWAYS_EXCLUDE: [&str; 4] = [".git/", "node_modules/", ".DS_Store", "Thumbs.db"];

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    /// `None`: follow the system.
    pub language: Option<Language>,
    pub open_with: OpenWith,
    /// For names not stored as UTF-8; `None`: detect.
    pub encoding: Option<NameCodec>,
    pub grid_for_media: bool,
    pub theme: Theme,
    pub default_grid: bool,
    pub ratio_column: bool,
    pub new_format: OutFormat,
    pub new_level: Level,
    pub always_exclude: bool,
    pub smart_folder: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            language: None,
            open_with: OpenWith::Mini,
            encoding: None,
            grid_for_media: true,
            theme: Theme::System,
            default_grid: false,
            ratio_column: true,
            new_format: OutFormat::TarZst,
            new_level: Level::Balanced,
            always_exclude: true,
            smart_folder: true,
        }
    }
}

impl Prefs {
    pub fn load() -> Self {
        Self::read(&Settings::load())
    }

    pub fn read(s: &Settings) -> Self {
        let d = Prefs::default();
        let flag = |key: &str, default: bool| s.get::<u8>(key).map_or(default, |v| v != 0);
        Prefs {
            language: match s.get::<String>("pref.language").as_deref() {
                Some("en") => Some(Language::English),
                Some("ar") => Some(Language::Arabic),
                _ => None,
            },
            open_with: match s.get::<u8>("pref.open_with") {
                Some(1) => OpenWith::Full,
                Some(2) => OpenWith::Extract,
                _ => OpenWith::Mini,
            },
            encoding: s
                .get::<String>("pref.encoding")
                .and_then(|l| NameCodec::from_label(&l)),
            grid_for_media: flag("pref.grid_for_media", d.grid_for_media),
            theme: match s.get::<u8>("pref.theme") {
                Some(1) => Theme::Light,
                Some(2) => Theme::Dark,
                _ => Theme::System,
            },
            default_grid: flag("pref.default_grid", d.default_grid),
            ratio_column: flag("pref.ratio_column", d.ratio_column),
            new_format: s
                .get::<usize>("pref.new_format")
                .and_then(|i| NEW_FORMATS.get(i).copied())
                .unwrap_or(d.new_format),
            new_level: match s.get::<u8>("pref.new_level") {
                Some(0) => Level::Fastest,
                Some(2) => Level::Smallest,
                _ => Level::Balanced,
            },
            always_exclude: flag("pref.always_exclude", d.always_exclude),
            smart_folder: flag("pref.smart_folder", d.smart_folder),
        }
    }

    /// The UI language these preferences ask for.
    pub fn ui_language(&self) -> Language {
        self.language.unwrap_or_else(Language::system)
    }

    /// The compression level `new_level` means for `format`.
    pub fn level_for(&self, format: OutFormat) -> u32 {
        let (min, max, default) = format.levels();
        match self.new_level {
            Level::Fastest => min.max(1).min(max),
            // zstd's own default (3) is close to gzip's speed; 9 matches the
            // other formats' middle ground.
            Level::Balanced if format == OutFormat::TarZst => 9,
            Level::Balanced => default,
            Level::Smallest => max,
        }
    }

    /// Applies the theme to a window's color scheme.
    pub fn apply_theme(&self, palette: &crate::Palette<'_>) {
        palette.set_color_scheme(match self.theme {
            Theme::System => slint::language::ColorScheme::Unknown,
            Theme::Light => slint::language::ColorScheme::Light,
            Theme::Dark => slint::language::ColorScheme::Dark,
        });
    }
}

/// A language's own name, for the pickers: "English", "العربية".
pub fn language_name(lang: Language) -> &'static str {
    match lang {
        Language::English => "English",
        Language::Arabic => "العربية",
    }
}

/// The encodings the picker offers after "Auto-detect".
fn codecs() -> Vec<NameCodec> {
    NameCodec::common()
        .into_iter()
        .filter(|c| *c != NameCodec::Utf8)
        .collect()
}

thread_local! {
    /// The open Preferences window: one per process.
    static WINDOW: RefCell<Option<PrefsWindow>> = const { RefCell::new(None) };
}

/// Shows the Preferences window, or brings it back if it's open already.
/// `on_theme` runs after the theme changes, so the window that opened it
/// can follow.
pub fn show(on_theme: impl Fn(&Prefs) + 'static) {
    WINDOW.with(|w| {
        let mut w = w.borrow_mut();
        if w.is_none() {
            let Ok(ui) = PrefsWindow::new() else { return };
            ui.set_custom_frame(crate::platform::CUSTOM_FRAME);
            ui.global::<crate::Locale>()
                .set_system_fonts(crate::platform::SYSTEM_FONTS);
            let weak = ui.as_weak();
            ui.on_start_drag(move || {
                if let Some(ui) = weak.upgrade() {
                    crate::begin_window_drag(&ui);
                }
            });
            let weak = ui.as_weak();
            ui.on_minimize(move || {
                if let Some(ui) = weak.upgrade() {
                    ui.window().with_winit_window(|w| w.set_minimized(true));
                }
            });
            let weak = ui.as_weak();
            ui.on_toggle_maximize(move || {
                if let Some(ui) = weak.upgrade() {
                    ui.window()
                        .with_winit_window(|w| w.set_maximized(!w.is_maximized()));
                }
            });
            start(&ui, on_theme);
            *w = Some(ui);
        }
        if let Some(ui) = w.as_ref() {
            let _ = ui.show();
        }
    });
}

/// The Preferences window, if this process has opened one.
pub fn window() -> Option<PrefsWindow> {
    WINDOW.with(|w| w.borrow().as_ref().map(|ui| ui.clone_strong()))
}

/// Wires a Preferences window to the saved settings.
pub fn start(ui: &PrefsWindow, on_theme: impl Fn(&Prefs) + 'static) {
    let settings = Rc::new(RefCell::new(Settings::load()));
    let codecs = codecs();
    let g = ui.global::<PrefsUi>();
    let mut labels: Vec<SharedString> = vec![i18n::tr("Auto-detect").into()];
    labels.extend(codecs.iter().map(|c| SharedString::from(c.label())));
    g.set_encodings(ModelRc::new(VecModel::from(labels)));
    fill(ui, &Prefs::read(&settings.borrow()), &codecs);

    let weak = ui.as_weak();
    g.on_choose(move |key, value| {
        let Some(ui) = weak.upgrade() else { return };
        let mut s = settings.borrow_mut();
        let key = key.as_str();
        if key == "language" {
            s.set(
                "pref.language",
                ["system", "en", "ar"][value.clamp(0, 2) as usize],
            );
        } else if key == "encoding" {
            match usize::try_from(value - 1).ok().and_then(|i| codecs.get(i)) {
                Some(c) => s.set("pref.encoding", c.name()),
                None => s.set("pref.encoding", "auto"),
            }
        } else {
            s.set(&format!("pref.{key}"), value);
        }
        s.save();
        let prefs = Prefs::read(&s);
        drop(s);
        fill(&ui, &prefs, &codecs);
        if key == "theme" {
            on_theme(&prefs);
        }
        if key == "language" {
            // This process's windows switch now; others when they next open.
            i18n::set(prefs.ui_language());
        }
    });
    show_defaults(ui);
    let weak = ui.as_weak();
    g.on_make_default(move |i| {
        let Some(ui) = weak.upgrade() else { return };
        let Some(t) = FILE_TYPES.get(i as usize) else {
            return;
        };
        let error = match platform::make_default(&[t]) {
            Ok(()) => String::new(),
            Err(e) => i18n::trf(
                "Couldn’t make Crate the default app: {error}",
                &[("error", &e.to_string())],
            ),
        };
        ui.global::<PrefsUi>().set_defaults_error(error.into());
        show_defaults(&ui);
    });
    let weak = ui.as_weak();
    g.on_close(move || {
        if let Some(ui) = weak.upgrade() {
            let _ = ui.hide();
        }
    });
}

/// Which types Crate opens now, as the system reports it.
fn show_defaults(ui: &PrefsWindow) {
    let support = platform::defaults_support();
    let types: Vec<TypeChoice> = FILE_TYPES
        .iter()
        .map(|t| TypeChoice {
            label: t.label.trim_start_matches('.').into(),
            on: matches!(
                support,
                Defaults::Direct | Defaults::SettingsPage | Defaults::Confirm
            ) && platform::is_default(t),
        })
        .collect();
    let g = ui.global::<PrefsUi>();
    g.set_types(ModelRc::new(VecModel::from(types)));
    g.set_defaults(support as i32);
}

fn fill(ui: &PrefsWindow, p: &Prefs, codecs: &[NameCodec]) {
    p.apply_theme(&ui.global::<crate::Palette>());
    let g = ui.global::<PrefsUi>();
    g.set_language(p.language.map_or(0, |l| l as i32 + 1));
    g.set_system_language(language_name(Language::system()).into());
    g.set_open_with(p.open_with as i32);
    g.set_encoding(
        p.encoding
            .and_then(|e| codecs.iter().position(|&c| c == e))
            .map_or(0, |i| i as i32 + 1),
    );
    g.set_grid_for_media(p.grid_for_media);
    g.set_theme(p.theme as i32);
    g.set_default_grid(p.default_grid);
    g.set_ratio_column(p.ratio_column);
    g.set_new_format(
        NEW_FORMATS
            .iter()
            .position(|&f| f == p.new_format)
            .unwrap_or(1) as i32,
    );
    g.set_new_level(p.new_level as i32);
    g.set_always_exclude(p.always_exclude);
    g.set_smart_folder(p.smart_folder);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_levels() {
        let p = Prefs::read(&Settings::default());
        assert_eq!(p, Prefs::default());
        assert_eq!(p.level_for(OutFormat::TarZst), 9);
        assert_eq!(p.level_for(OutFormat::Zip), 6);
        let fast = Prefs {
            new_level: Level::Fastest,
            ..p.clone()
        };
        assert_eq!(fast.level_for(OutFormat::Zip), 1);
        assert_eq!(fast.level_for(OutFormat::Tar), 0);
        let small = Prefs {
            new_level: Level::Smallest,
            ..p
        };
        assert_eq!(small.level_for(OutFormat::TarZst), 19);
    }
}
