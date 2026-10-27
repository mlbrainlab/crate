//! Preferences: the window, what it saves, and what archive windows and the
//! New archive dialog do with it.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use common::{Harness, g, rows};
use crate_app::prefs::{Level, OpenWith, Prefs, Theme};
use crate_app::{CreateUi, PrefsUi};
use crate_core::create::OutFormat;
use slint::{ComponentHandle, Model};

#[test]
fn preferences_flow() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);

    // Defaults, as the mockup shows them.
    let ui = h.prefs();
    let p = || ui.global::<PrefsUi>();
    // (With CRATE_UI_DARK the harness starts the theme at Dark.)
    let (theme, theme_index) = if h.dark {
        (Theme::Dark, 2)
    } else {
        (Theme::System, 0)
    };
    assert_eq!(
        Prefs::load(),
        Prefs {
            theme,
            ..Prefs::default()
        }
    );
    assert_eq!((p().get_open_with(), p().get_theme()), (0, theme_index));
    assert_eq!(p().get_encoding(), 0);
    assert_eq!(p().get_encodings().row_data(0).unwrap(), "Auto-detect");
    assert_eq!(p().get_new_format(), 1, "TAR.ZST");
    assert!(p().get_smart_folder() && p().get_always_exclude() && p().get_ratio_column());
    h.shot("50-prefs");

    // The theme applies to the window itself at once.
    p().invoke_choose("theme".into(), 2);
    assert_eq!(p().get_theme(), 2);
    assert_eq!(
        ui.global::<crate_app::Palette>().get_color_scheme(),
        slint::language::ColorScheme::Dark
    );
    h.shot("51-prefs-dark");
    p().invoke_choose("theme".into(), theme_index);

    // Change the rest; every change is saved straight away.
    let arabic = p()
        .get_encodings()
        .iter()
        .position(|l| l == "Arabic (Windows-1256)")
        .unwrap() as i32;
    p().invoke_choose("open_with".into(), 1);
    p().invoke_choose("encoding".into(), arabic);
    p().invoke_choose("ratio_column".into(), 0);
    p().invoke_choose("smart_folder".into(), 0);
    p().invoke_choose("new_format".into(), 0);
    p().invoke_choose("new_level".into(), 2);
    let saved = Prefs::load();
    assert_eq!(saved.open_with, OpenWith::Full);
    assert_eq!(saved.theme, theme);
    assert_eq!(
        saved.encoding.map(|c| c.label()).as_deref(),
        Some("Arabic (Windows-1256)")
    );
    assert!(!saved.ratio_column && !saved.smart_folder);
    assert_eq!(
        (saved.new_format, saved.new_level),
        (OutFormat::Zip, Level::Smallest)
    );
    assert_eq!(
        p().get_encoding(),
        arabic,
        "the window shows what was saved"
    );

    // An archive now opens in the full view, without the Ratio column, and
    // Extract all always makes a folder named after the archive.
    support::sample_zip(&path("project-assets.zip"));
    let view = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&view).get_status() == "ready");
    assert!(g(&view).get_full());
    assert!(!g(&view).get_ratio_column());
    assert_eq!(
        g(&view).get_dest_name(),
        "project-assets",
        "no smart folder"
    );
    h.shot("52-full-no-ratio");

    // Old names are read with the chosen encoding: no guess, no banner.
    p().invoke_choose("open_with".into(), 0);
    support::arabic_cp1256_zip(&path("arabic.zip"));
    let view = h.open(&path("arabic.zip"));
    h.until("listing", || g(&view).get_status() == "ready");
    assert!(!g(&view).get_full());
    assert!(!g(&view).get_show_banner());
    assert!(
        rows(&view).iter().any(|r| r == "الصور"),
        "{:?}",
        rows(&view)
    );

    // "Extract right away": everything is extracted as soon as it's listed.
    p().invoke_choose("open_with".into(), 2);
    p().invoke_choose("smart_folder".into(), 1);
    support::sample_zip(&path("right-away.zip"));
    let view = h.open(&path("right-away.zip"));
    h.until("done popup", || g(&view).get_show_done());
    assert!(path("project/README.md").is_file(), "smart folder again");
    h.shot("53-extracted-right-away");
    p().invoke_choose("open_with".into(), 0);

    // New archives: the default format and level, and the usual excludes.
    let src = dir.path().join("trip");
    std::fs::create_dir_all(src.join(".git")).unwrap();
    std::fs::write(src.join("notes.txt"), "notes").unwrap();
    std::fs::write(src.join(".git/HEAD"), "ref").unwrap();
    let home = h.home(vec![src]);
    let c = || home.global::<CreateUi>();
    assert_eq!(c().get_extension(), ".zip");
    assert_eq!(c().get_level(), 9.0, "Smallest is ZIP's 9");
    assert_eq!(c().get_preset(), -1, "not the Balanced preset any more");
    let exclude: Vec<String> = c().get_exclude().iter().map(|e| e.to_string()).collect();
    assert!(exclude.contains(&".git/".to_owned()), "{exclude:?}");
    h.until("counted", || c().get_excluded_note().contains("excluded"));
}
