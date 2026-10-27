//! Home and the New archive dialog (Create.dc.html). Like the archive
//! window, all file work runs on worker threads and comes back through
//! `post`; the controller lives on the UI thread.

use crate::i18n::{tr, trf, trn};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate_core::CancelToken;
use crate_core::create::{self, Collected, CreateOptions, OutFormat};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::prefs::{ALWAYS_EXCLUDE, Prefs};
use crate::settings::Settings;
use crate::{CreateItem, CreateUi, MainWindow, model};

thread_local! {
    static HOME: RefCell<Option<Rc<Home>>> = const { RefCell::new(None) };
}

/// Runs `f` with the controller on the UI thread. Callable from any thread.
pub fn post(f: impl FnOnce(&Home) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(home) = HOME.with(|h| h.borrow().clone()) {
            f(&home);
        }
    });
}

/// "Level 9 of 19", or "No compression" for plain tar.
fn level_text(format: OutFormat, level: u32) -> String {
    let (min, max, _) = format.levels();
    if max > min {
        trf(
            "Level {level} of {max}",
            &[("level", &level.to_string()), ("max", &max.to_string())],
        )
    } else {
        tr("No compression")
    }
}

fn now(f: impl FnOnce(&Home)) {
    if let Some(h) = HOME.with(|h| h.borrow().clone()) {
        f(&h);
    }
}

/// The formats in the dialog's order (`CreateUi.format`).
const FORMATS: [OutFormat; 7] = [
    OutFormat::Zip,
    OutFormat::SevenZ,
    OutFormat::TarZst,
    OutFormat::TarGz,
    OutFormat::TarXz,
    OutFormat::TarBz2,
    OutFormat::Tar,
];
const VOLUMES: [u64; 3] = [25_000_000, 700 * 1024 * 1024, 4 * 1024 * 1024 * 1024 - 1];
/// How long the dialog waits after a change before estimating again.
const ESTIMATE_DELAY: Duration = Duration::from_millis(250);
const ESTIMATE_SAMPLE: u64 = 8 << 20;

/// Opens files and folders with the default app (tests record instead).
pub type Opener = fn(&Path) -> std::io::Result<()>;

thread_local! {
    static OPENER: std::cell::Cell<Opener> = std::cell::Cell::new(|p| open::that_detached(p));
    /// Opens an archive in its own window (normally: start Crate on it).
    static OPEN_ARCHIVE: std::cell::Cell<Opener> = std::cell::Cell::new(open_in_new_window);
}

/// Opens a file, folder or web address with the default app.
pub fn open_path(path: &Path) -> std::io::Result<()> {
    OPENER.with(|o| o.get())(path)
}

pub fn set_opener(opener: Opener) {
    OPENER.with(|o| o.set(opener));
}

pub fn set_archive_opener(opener: Opener) {
    OPEN_ARCHIVE.with(|o| o.set(opener));
}

fn open_in_new_window(archive: &Path) -> std::io::Result<()> {
    std::process::Command::new(std::env::current_exe()?)
        .arg(archive)
        .spawn()
        .map(|_| ())
}

/// A built-in or saved preset.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    pub format: OutFormat,
    pub level: u32,
    pub keep_permissions: bool,
    pub split: Option<usize>,
    pub exclude: Vec<String>,
}

impl Preset {
    pub fn builtin() -> Vec<Preset> {
        vec![
            Preset {
                name: tr("Balanced"),
                format: OutFormat::TarZst,
                level: 9,
                keep_permissions: true,
                split: None,
                exclude: Vec::new(),
            },
            Preset {
                name: tr("Share by email"),
                format: OutFormat::Zip,
                level: 6,
                keep_permissions: false,
                split: Some(0),
                exclude: vec![".DS_Store".into(), "Thumbs.db".into()],
            },
            Preset {
                name: tr("Long-term backup"),
                format: OutFormat::TarXz,
                level: 9,
                keep_permissions: true,
                split: None,
                exclude: Vec::new(),
            },
        ]
    }

    /// `format=TarZst;level=9;perms=1;split=;exclude=*.tmp|.git/`
    fn encode(&self) -> String {
        format!(
            "format={};level={};perms={};split={};exclude={}",
            FORMATS.iter().position(|&f| f == self.format).unwrap_or(0),
            self.level,
            u8::from(self.keep_permissions),
            self.split.map(|s| s.to_string()).unwrap_or_default(),
            self.exclude.join("|")
        )
    }

    fn decode(name: &str, text: &str) -> Option<Preset> {
        let mut p = Preset {
            name: name.to_owned(),
            format: OutFormat::Zip,
            level: 6,
            keep_permissions: true,
            split: None,
            exclude: Vec::new(),
        };
        for part in text.split(';') {
            let (k, v) = part.split_once('=')?;
            match k {
                "format" => p.format = *FORMATS.get(v.parse::<usize>().ok()?)?,
                "level" => p.level = v.parse().ok()?,
                "perms" => p.keep_permissions = v == "1",
                "split" => p.split = v.parse().ok(),
                "exclude" => {
                    p.exclude = v
                        .split('|')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                }
                _ => {}
            }
        }
        Some(p)
    }
}

/// Saved presets: `preset.<n>.name` / `preset.<n>`.
fn saved_presets(settings: &Settings) -> Vec<Preset> {
    (0..20)
        .filter_map(|i| {
            let name: String = settings.get(&format!("preset.{i}.name"))?;
            let text: String = settings.get(&format!("preset.{i}"))?;
            Preset::decode(&name, &text)
        })
        .collect()
}

/// 0 nothing, 1 weak, 2 fair, 3 good, 4 strong: length plus kinds of characters.
pub fn password_strength(pw: &str) -> i32 {
    if pw.is_empty() {
        return 0;
    }
    let classes = [
        pw.chars().any(|c| c.is_lowercase()),
        pw.chars().any(|c| c.is_uppercase()),
        pw.chars().any(|c| c.is_ascii_digit()),
        pw.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|&&b| b)
    .count();
    let len = pw.chars().count();
    match (len, classes) {
        (0..=7, _) => 1,
        (8..=11, 0..=2) => 2,
        (8..=11, _) | (12..=15, 0..=1) => 3,
        _ => 4,
    }
}

pub struct Home {
    ui: slint::Weak<MainWindow>,
    state: RefCell<State>,
    estimate_timer: slint::Timer,
    items_model: Rc<VecModel<CreateItem>>,
}

struct State {
    sources: Vec<PathBuf>,
    collected: Option<Collected>,
    /// Bumped on every change; stale background results are dropped.
    serial: u64,
    exclude: Vec<String>,
    folder: Option<PathBuf>,
    name: String,
    format: usize,
    level: u32,
    encrypt: bool,
    password: String,
    split: bool,
    volume: usize,
    keep_permissions: bool,
    delete_after: bool,
    more_formats: bool,
    preset: i32,
    presets: Vec<Preset>,
    cancel: Option<CancelToken>,
    created: Option<PathBuf>,
    settings: Settings,
    /// Re-read each time the New archive dialog opens.
    prefs: Prefs,
}

impl Home {
    /// Wires the Home window. With `sources`, the New archive dialog opens
    /// right away (`crate-app --compress …`).
    pub fn start(ui: &MainWindow, sources: Vec<PathBuf>) {
        let settings = Settings::load();
        let prefs = Prefs::read(&settings);
        crate::i18n::set(prefs.ui_language());
        prefs.apply_theme(&ui.global::<crate::Palette>());
        let mut presets = Preset::builtin();
        presets.extend(saved_presets(&settings));
        let home = Rc::new(Home {
            ui: ui.as_weak(),
            state: RefCell::new(State {
                sources: Vec::new(),
                collected: None,
                serial: 0,
                exclude: Vec::new(),
                folder: None,
                name: String::new(),
                format: 2,
                level: 9,
                encrypt: false,
                password: String::new(),
                split: false,
                volume: 2,
                keep_permissions: true,
                delete_after: false,
                more_formats: false,
                preset: 0,
                presets,
                cancel: None,
                created: None,
                settings,
                prefs,
            }),
            estimate_timer: slint::Timer::default(),
            items_model: Rc::new(VecModel::default()),
        });
        HOME.with(|h| *h.borrow_mut() = Some(home.clone()));
        connect(ui);
        crate::shortcuts::connect(ui, crate::shortcuts::Window::Home);
        crate::about::connect(ui);
        crate::ops_panel::connect(ui);
        ui.global::<CreateUi>()
            .set_items(home.items_model.clone().into());
        if sources.is_empty() {
            crate::first_run::maybe_show(ui);
        } else {
            home.open_dialog();
            home.add_sources(sources);
        }
    }

    fn ui(&self) -> MainWindow {
        self.ui
            .upgrade()
            .expect("the window outlives its controller")
    }

    fn g<R>(&self, f: impl FnOnce(&CreateUi) -> R) -> R {
        let ui = self.ui();
        let g = ui.global::<CreateUi>();
        f(&g)
    }

    /// Preferences, in this process; the theme follows right away.
    fn open_prefs(&self) {
        let weak = self.ui.clone();
        crate::prefs::show(move |p| {
            if let Some(ui) = weak.upgrade() {
                p.apply_theme(&ui.global::<crate::Palette>());
            }
        });
    }

    fn open_dialog(&self) {
        {
            let mut s = self.state.borrow_mut();
            s.sources.clear();
            s.collected = None;
            s.name.clear();
            s.folder = None;
            s.password.clear();
            s.encrypt = false;
            s.delete_after = false;
        }
        self.items_model.set_vec(Vec::new());
        let prefs = Prefs::load();
        // The first preset unless Preferences name another format or level.
        let format = prefs.new_format;
        let level = prefs.level_for(format);
        let first = self.state.borrow().presets.first().cloned();
        let custom = first.is_some_and(|p| p.format != format || p.level != level);
        self.state.borrow_mut().prefs = prefs;
        self.apply_preset(0);
        if custom {
            let mut s = self.state.borrow_mut();
            s.preset = -1;
            s.format = FORMATS.iter().position(|&f| f == format).unwrap_or(0);
            s.level = level;
            s.more_formats |= s.format >= 5;
        }
        self.g(|g| {
            g.set_password("".into());
            g.set_status("".into());
            g.set_busy(false);
            g.set_show(true);
        });
        self.refresh();
    }

    fn close_dialog(&self) {
        let busy = self.state.borrow().cancel.clone();
        match busy {
            Some(cancel) => cancel.cancel(),
            None => self.g(|g| g.set_show(false)),
        }
    }

    // ----- Contents ------------------------------------------------------

    fn pick_sources(&self, folders: bool) {
        let start = self
            .state
            .borrow()
            .folder
            .clone()
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().set_title(if folders {
                tr("Add folders")
            } else {
                tr("Add files")
            });
            if let Some(dir) = start {
                dialog = dialog.set_directory(dir);
            }
            let picked = if folders {
                dialog.pick_folders()
            } else {
                dialog.pick_files()
            };
            if let Some(paths) = picked {
                post(move |h| h.add_sources(paths));
            }
        });
    }

    pub fn add_sources(&self, paths: Vec<PathBuf>) {
        {
            let mut s = self.state.borrow_mut();
            for p in paths {
                let p = p.canonicalize().unwrap_or(p);
                if !s.sources.contains(&p) {
                    s.sources.push(p);
                }
            }
            if s.folder.is_none() {
                s.folder = s
                    .sources
                    .first()
                    .and_then(|p| p.parent())
                    .map(Path::to_owned);
            }
            if s.name.is_empty() {
                s.name = default_name(&s.sources);
                let name = s.name.clone();
                drop(s);
                self.g(|g| g.set_name(name.into()));
            }
        }
        self.recollect();
    }

    fn remove_item(&self, i: usize) {
        {
            let mut s = self.state.borrow_mut();
            if i < s.sources.len() {
                s.sources.remove(i);
            }
        }
        self.recollect();
    }

    fn add_exclude(&self, pattern: &str) {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return;
        }
        {
            let mut s = self.state.borrow_mut();
            for p in crate_core::pattern::split(pattern) {
                if !s.exclude.iter().any(|e| e == p) {
                    s.exclude.push(p.to_owned());
                }
            }
            s.preset = -1;
        }
        self.recollect();
    }

    fn remove_exclude(&self, i: usize) {
        {
            let mut s = self.state.borrow_mut();
            if i < s.exclude.len() {
                s.exclude.remove(i);
            }
            s.preset = -1;
        }
        self.recollect();
    }

    /// Walks the sources again (on a worker: folders can be huge).
    fn recollect(&self) {
        let (sources, exclude, keep, serial) = {
            let mut s = self.state.borrow_mut();
            s.serial += 1;
            s.collected = None;
            (
                s.sources.clone(),
                s.exclude.clone(),
                s.keep_permissions,
                s.serial,
            )
        };
        // The list shows the chosen items right away; sizes follow.
        let items: Vec<CreateItem> = sources
            .iter()
            .map(|p| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let (ext, kind) = if p.is_dir() {
                    (String::new(), model::Tile::Folder)
                } else {
                    model::file_kind(&name)
                };
                CreateItem {
                    name: name.into(),
                    meta: "…".into(),
                    ext: ext.into(),
                    kind: kind as i32,
                }
            })
            .collect();
        self.items_model.set_vec(items);
        self.refresh();
        if sources.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            let collected = create::collect(&sources, &exclude, keep, None);
            post(move |h| h.collected(serial, collected));
        });
    }

    fn collected(&self, serial: u64, collected: Collected) {
        if self.state.borrow().serial != serial {
            return;
        }
        // Per-source sizes for the list.
        let sizes: Vec<(u64, u64)> = {
            let s = self.state.borrow();
            s.sources
                .iter()
                .map(|src| {
                    let top = src
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    collected
                        .items
                        .iter()
                        .filter(|i| i.name == top || i.name.starts_with(&format!("{top}/")))
                        .fold((0, 0), |(n, b), i| {
                            (n + u64::from(i.kind == create::ItemKind::File), b + i.size)
                        })
                })
                .collect()
        };
        for (i, (files, bytes)) in sizes.into_iter().enumerate() {
            if let Some(mut item) = slint::Model::row_data(&*self.items_model, i) {
                item.meta = if item.kind == model::Tile::Folder as i32 {
                    trf(
                        "{files} · {size}",
                        &[
                            ("files", &model::count(files, "file")),
                            ("size", &model::size_text(bytes)),
                        ],
                    )
                } else {
                    model::size_text(bytes)
                }
                .into();
                slint::Model::set_row_data(&*self.items_model, i, item);
            }
        }
        self.state.borrow_mut().collected = Some(collected);
        self.refresh();
    }

    // ----- Choices ---------------------------------------------------------

    fn apply_preset(&self, i: i32) {
        {
            let mut s = self.state.borrow_mut();
            s.preset = i;
            if let Some(p) = usize::try_from(i)
                .ok()
                .and_then(|i| s.presets.get(i))
                .cloned()
            {
                s.format = FORMATS.iter().position(|&f| f == p.format).unwrap_or(0);
                s.level = p.level;
                s.keep_permissions = p.keep_permissions;
                s.split = p.split.is_some();
                s.volume = p.split.unwrap_or(s.volume);
                s.exclude = p.exclude.clone();
                if s.prefs.always_exclude {
                    for e in ALWAYS_EXCLUDE {
                        if !s.exclude.iter().any(|x| x == e) {
                            s.exclude.push(e.to_owned());
                        }
                    }
                }
                s.more_formats |= s.format >= 5;
            }
        }
        self.recollect();
    }

    fn save_preset(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let mut s = self.state.borrow_mut();
        let preset = Preset {
            name: name.to_owned(),
            format: FORMATS[s.format],
            level: s.level,
            keep_permissions: s.keep_permissions,
            split: s.split.then_some(s.volume),
            exclude: s.exclude.clone(),
        };
        let builtin = Preset::builtin().len();
        let saved = s.presets.len() - builtin;
        // Same name: replace it.
        let slot = s.presets[builtin..]
            .iter()
            .position(|p| p.name == name)
            .unwrap_or(saved);
        s.settings.set(&format!("preset.{slot}.name"), name);
        s.settings.set(&format!("preset.{slot}"), preset.encode());
        s.settings.save();
        if slot < saved {
            s.presets[builtin + slot] = preset;
        } else {
            s.presets.push(preset);
        }
        s.preset = (builtin + slot) as i32;
        drop(s);
        self.refresh();
    }

    fn set_format(&self, i: usize) {
        {
            let mut s = self.state.borrow_mut();
            s.format = i.min(FORMATS.len() - 1);
            s.level = FORMATS[s.format].levels().2;
            s.preset = -1;
        }
        self.refresh();
    }

    fn set_level(&self, level: f32) {
        {
            let mut s = self.state.borrow_mut();
            let (min, max, _) = FORMATS[s.format].levels();
            s.level = (level.round() as u32).clamp(min, max);
            s.preset = -1;
        }
        self.refresh();
    }

    fn toggle(&self, which: i32) {
        let recollect = {
            let mut s = self.state.borrow_mut();
            match which {
                0 => s.encrypt = !s.encrypt,
                1 => s.split = !s.split,
                2 => {
                    s.keep_permissions = !s.keep_permissions;
                    s.preset = -1;
                }
                _ => s.delete_after = !s.delete_after,
            }
            which == 2
        };
        if recollect {
            self.recollect();
        } else {
            self.refresh();
        }
    }

    fn pick_folder(&self) {
        let start = self.state.borrow().folder.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().set_title(tr("Save the archive in"));
            if let Some(dir) = start {
                dialog = dialog.set_directory(dir);
            }
            if let Some(dir) = dialog.pick_folder() {
                post(move |h| {
                    h.state.borrow_mut().folder = Some(dir);
                    h.refresh();
                });
            }
        });
    }

    fn output(&self) -> Option<PathBuf> {
        let s = self.state.borrow();
        let ext = FORMATS[s.format].extension(s.encrypt);
        let name = s.name.trim();
        (!name.is_empty()).then(|| {
            s.folder
                .clone()
                .unwrap_or_default()
                .join(format!("{name}{ext}"))
        })
    }

    /// Everything the dialog shows, from the state.
    fn refresh(&self) {
        let output = self.output();
        let s = self.state.borrow();
        let format = FORMATS[s.format];
        let (min, max, _) = format.levels();
        let items = s
            .collected
            .as_ref()
            .map_or(s.sources.len() as u64, |c| c.items.len() as u64);
        let bytes = s.collected.as_ref().map_or(0, Collected::bytes);
        let summary = match &s.collected {
            Some(_) => trf(
                "{items} · {size}",
                &[
                    ("items", &model::count(items, "item")),
                    ("size", &model::size_text(bytes)),
                ],
            ),
            None if s.sources.is_empty() => tr("Nothing added yet"),
            None => tr("Counting…"),
        };
        let excluded = s
            .collected
            .as_ref()
            .filter(|c| c.excluded > 0)
            .map(|c| {
                trf(
                    "{items} excluded · saves {size}",
                    &[
                        ("items", &model::count(c.excluded, "item")),
                        ("size", &model::size_text(c.excluded_bytes)),
                    ],
                )
            })
            .unwrap_or_default();
        let exists = output.as_ref().is_some_and(|p| p.exists());
        let ready = s.collected.as_ref().is_some_and(|c| !c.items.is_empty())
            && output.is_some()
            && !exists
            && (!s.encrypt || !s.password.is_empty())
            && s.cancel.is_none();
        let status = if exists {
            tr("A file with this name is already there: pick another name")
        } else if s.encrypt && s.password.is_empty() {
            tr("Enter a password, or turn encryption off")
        } else {
            String::new()
        };
        let encrypt_note = match format {
            OutFormat::Zip => {
                tr("AES-256. File names stay visible; opens in 7-Zip, WinRAR, macOS and Crate.")
            }
            OutFormat::SevenZ => tr("AES-256. File names are hidden too."),
            _ => tr(
                "Encrypted with age (scrypt). File names are hidden too. Opens in Crate and the age tool.",
            ),
        };
        let mut presets: Vec<SharedString> =
            s.presets.iter().map(|p| p.name.clone().into()).collect();
        presets.truncate(8);
        let folder = s
            .folder
            .as_deref()
            .map(|f| model::display_folder(f).trim_end_matches('/').to_owned())
            .unwrap_or_default();
        let (
            format_index,
            level,
            encrypt,
            split,
            volume,
            keep,
            delete_after,
            more,
            preset,
            strength,
        ) = (
            s.format as i32,
            s.level,
            s.encrypt,
            s.split,
            s.volume as i32,
            s.keep_permissions,
            s.delete_after,
            s.more_formats,
            s.preset,
            password_strength(&s.password),
        );
        let exclude: Vec<SharedString> = s.exclude.iter().map(|e| e.clone().into()).collect();
        drop(s);
        let busy = self.state.borrow().cancel.is_some();
        self.g(|g| {
            g.set_summary(summary.into());
            g.set_exclude(ModelRc::new(VecModel::from(exclude)));
            g.set_excluded_note(excluded.into());
            g.set_extension(format.extension(encrypt).into());
            g.set_folder(folder.into());
            g.set_format(format_index);
            g.set_more_formats(more);
            g.set_has_levels(max > min);
            g.set_level_min(min as f32);
            g.set_level_max(max as f32);
            g.set_level(level as f32);
            g.set_encrypt(encrypt);
            g.set_strength(strength);
            g.set_encrypt_note(encrypt_note.into());
            g.set_can_split(format.supports_volumes());
            g.set_split(split && format.supports_volumes());
            g.set_volume(volume);
            g.set_keep_permissions(keep);
            g.set_delete_after(delete_after);
            g.set_output(
                output
                    .as_deref()
                    .map(model::display_path)
                    .unwrap_or_default()
                    .into(),
            );
            g.set_presets(ModelRc::new(VecModel::from(presets)));
            g.set_preset(preset);
            g.set_ready(ready);
            if !busy {
                g.set_status(status.into());
            }
        });
        self.schedule_estimate();
    }

    fn schedule_estimate(&self) {
        let level = self.state.borrow().level;
        let base = level_text(FORMATS[self.state.borrow().format], level);
        self.g(|g| g.set_level_text(trf("{level} · estimating…", &[("level", &base)]).into()));
        self.estimate_timer
            .start(slint::TimerMode::SingleShot, ESTIMATE_DELAY, || {
                now(Home::estimate);
            });
    }

    fn estimate(&self) {
        let s = self.state.borrow();
        let Some(collected) = s.collected.clone() else {
            let text = level_text(FORMATS[s.format], s.level);
            drop(s);
            self.g(|g| g.set_level_text(text.into()));
            return;
        };
        let (format, level, serial) = (FORMATS[s.format], s.level, s.serial);
        drop(s);
        std::thread::spawn(move || {
            let e = create::estimate(&collected, format, level, ESTIMATE_SAMPLE);
            post(move |h| h.estimated(serial, format, level, e));
        });
    }

    fn estimated(&self, serial: u64, format: OutFormat, level: u32, e: create::Estimate) {
        let s = self.state.borrow();
        if s.serial != serial || FORMATS[s.format] != format || s.level != level {
            return;
        }
        drop(s);
        let base = level_text(format, level);
        let time = if e.seconds < 1.0 {
            tr("a moment")
        } else if e.seconds < 90.0 {
            trf("~{s} s", &[("s", &e.seconds.round().to_string())])
        } else {
            trf(
                "~{m} min",
                &[("m", &(e.seconds / 60.0).round().to_string())],
            )
        };
        let text = trf(
            "{level} · ≈ {size} · {time}",
            &[
                ("level", &base),
                ("size", &model::size_text(e.bytes)),
                ("time", &time),
            ],
        );
        self.g(|g| g.set_level_text(text.into()));
    }

    // ----- Creating --------------------------------------------------------

    fn create(&self) {
        let Some(output) = self.output() else { return };
        let (collected, options, delete_after, sources) = {
            let s = self.state.borrow();
            let Some(collected) = s.collected.clone() else {
                return;
            };
            let format = FORMATS[s.format];
            let options = CreateOptions {
                format,
                level: s.level,
                password: s.encrypt.then(|| s.password.clone()),
                exclude: s.exclude.clone(),
                keep_permissions: s.keep_permissions,
                volume_size: (s.split && format.supports_volumes())
                    .then(|| VOLUMES[s.volume.min(2)]),
            };
            (collected, options, s.delete_after, s.sources.clone())
        };
        let cancel = CancelToken::new();
        self.state.borrow_mut().cancel = Some(cancel.clone());
        let out_name = output
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let op = crate::ops::Op::start(
            crate::ops::Kind::Create,
            trf("Compressing {name}", &[("name", &out_name)]),
            trf(
                "→ {folder}",
                &[(
                    "folder",
                    &model::display_folder(output.parent().unwrap_or(Path::new(""))),
                )],
            ),
            cancel.clone(),
        );
        self.g(|g| {
            g.set_busy(true);
            g.set_progress(0.0);
            g.set_status(tr("Compressing…").into());
            g.set_ready(false);
        });
        std::thread::spawn(move || {
            let mut last = std::time::Instant::now() - Duration::from_secs(1);
            let result = create::create(
                &collected,
                &output,
                &options,
                &mut |p| {
                    op.progress(p);
                    if last.elapsed() < Duration::from_millis(33) {
                        return;
                    }
                    last = std::time::Instant::now();
                    let f = p.bytes_done as f32 / p.bytes_total.max(1) as f32;
                    let current = p.current.clone();
                    post(move |h| {
                        h.g(|g| {
                            g.set_progress(f);
                            g.set_status(
                                trf(
                                    "Compressing… {pct}% · {file}",
                                    &[
                                        ("pct", &(f * 100.0).floor().to_string()),
                                        ("file", &current),
                                    ],
                                )
                                .into(),
                            );
                        })
                    });
                },
                &cancel,
            );
            // Originals go to the Trash only after the archive is safely written.
            let trashed = match (&result, delete_after) {
                (Ok(_), true) => trash::delete_all(&sources).map_err(|e| e.to_string()),
                _ => Ok(()),
            };
            post(move |h| h.created(result, trashed, &op, &out_name));
        });
    }

    fn created(
        &self,
        result: crate_core::Result<create::CreateReport>,
        trashed: Result<(), String>,
        op: &crate::ops::Op,
        name: &str,
    ) {
        self.state.borrow_mut().cancel = None;
        self.g(|g| g.set_busy(false));
        if let Some(ui) = self.ui.upgrade() {
            let background = crate::ops::in_background(ui.window());
            op.notify
                .store(background, std::sync::atomic::Ordering::Relaxed);
        }
        match &result {
            Ok(r) => op.finish(
                crate::ops::Outcome::Done,
                &trf(
                    "Created {name} · {size}",
                    &[("name", name), ("size", &model::size_text(r.written))],
                ),
                r.outputs.first().and_then(|p| p.parent()),
            ),
            Err(crate_core::Error::Cancelled) => op.finish(
                crate::ops::Outcome::Cancelled,
                &trf("Cancelled: {name}", &[("name", name)]),
                None,
            ),
            Err(e) => op.finish(
                crate::ops::Outcome::Failed,
                &trf(
                    "Couldn’t create {name}: {error}",
                    &[("name", name), ("error", &e.to_string())],
                ),
                None,
            ),
        }
        match result {
            Ok(report) => {
                crate::about::count_success();
                let first = report.outputs[0].clone();
                let name = first
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let size = model::size_text(report.written);
                let mut text = if report.outputs.len() > 1 {
                    trf(
                        &trn(
                            "Created {name} in {n} volume · {size}",
                            "Created {name} in {n} volumes · {size}",
                            report.outputs.len() as u64,
                        ),
                        &[("name", &name), ("size", &size)],
                    )
                } else {
                    trf(
                        "Created {name} · {size}",
                        &[("name", &name), ("size", &size)],
                    )
                };
                if let Err(e) = trashed {
                    text.push_str(&trf(
                        " · couldn’t move the originals to the Trash: {error}",
                        &[("error", &e)],
                    ));
                }
                self.state.borrow_mut().created = Some(first);
                self.g(|g| g.set_show(false));
                self.ui().set_created(text.into());
            }
            Err(crate_core::Error::Cancelled) => {
                self.g(|g| g.set_status(tr("Cancelled; nothing was written").into()));
                self.refresh();
            }
            Err(e) => {
                let text = trf(
                    "Couldn’t create the archive: {error}",
                    &[("error", &e.to_string())],
                );
                self.g(|g| g.set_status(text.into()));
                self.refresh();
            }
        }
    }

    fn open_created(&self) {
        if let Some(p) = self.state.borrow().created.clone() {
            let _ = OPEN_ARCHIVE.with(|o| o.get())(&p);
        }
    }

    fn show_created(&self) {
        if let Some(dir) = self
            .state
            .borrow()
            .created
            .as_ref()
            .and_then(|p| p.parent())
        {
            let _ = OPENER.with(|o| o.get())(dir);
        }
    }

    fn open_archive(&self) {
        std::thread::spawn(|| {
            if let Some(paths) = rfd::FileDialog::new()
                .set_title(tr("Open an archive"))
                .pick_files()
            {
                post(move |home| home.open_paths(paths));
            }
        });
    }

    /// Picked or dropped on Home: one archive opens in its own window,
    /// several go to Batch extract; anything else goes to New archive.
    pub fn open_paths(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if paths.iter().all(|p| crate::batch::is_archive(p)) {
            if let [one] = paths.as_slice() {
                let _ = OPEN_ARCHIVE.with(|o| o.get())(one);
            } else if let Some(ui) = self.ui.upgrade() {
                crate::batch::start(&ui, paths, false);
            }
        } else {
            self.open_dialog();
            self.add_sources(paths);
        }
    }
}

/// `report.pdf` → `report`; several items → their folder's name.
pub fn default_name(sources: &[PathBuf]) -> String {
    match sources {
        [] => String::new(),
        [one] => {
            let name = one
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if one.is_dir() {
                name
            } else {
                crate_core::safety::split_extension(&name).0.to_owned()
            }
        }
        [first, ..] => first
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tr("Archive")),
    }
}

fn connect(ui: &MainWindow) {
    fn with(f: impl Fn(&Home) + 'static) -> impl Fn() + 'static {
        move || now(&f)
    }
    ui.on_new_archive(with(Home::open_dialog));
    ui.on_open_archive(with(Home::open_archive));
    ui.on_open_created(with(Home::open_created));
    ui.on_show_created(with(Home::show_created));
    ui.on_open_prefs(with(Home::open_prefs));
    let g = ui.global::<CreateUi>();
    g.on_add_files(with(|h| h.pick_sources(false)));
    g.on_add_folder(with(|h| h.pick_sources(true)));
    g.on_remove_item(|i| now(move |h| h.remove_item(i as usize)));
    g.on_add_exclude(|p| now(move |h| h.add_exclude(&p)));
    g.on_remove_exclude(|i| now(move |h| h.remove_exclude(i as usize)));
    g.on_name_edited(|t| {
        now(move |h| {
            h.state.borrow_mut().name = t.to_string();
            h.refresh();
        })
    });
    g.on_pick_folder(with(Home::pick_folder));
    g.on_set_format(|i| now(move |h| h.set_format(i as usize)));
    g.on_toggle_more(with(|h| {
        h.state.borrow_mut().more_formats = true;
        h.refresh();
    }));
    g.on_level_changed(|l| now(move |h| h.set_level(l)));
    g.on_toggle(|i| now(move |h| h.toggle(i)));
    g.on_password_edited(|t| {
        now(move |h| {
            h.state.borrow_mut().password = t.to_string();
            h.refresh();
        })
    });
    g.on_set_volume(|i| {
        now(move |h| {
            h.state.borrow_mut().volume = i.clamp(0, 2) as usize;
            h.refresh();
        })
    });
    g.on_set_preset(|i| now(move |h| h.apply_preset(i)));
    g.on_save_preset(|n| now(move |h| h.save_preset(&n)));
    g.on_cancel(with(Home::close_dialog));
    g.on_create(with(Home::create));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength() {
        assert_eq!(password_strength(""), 0);
        assert_eq!(password_strength("hunter2"), 1);
        assert_eq!(password_strength("hunter22"), 2);
        assert_eq!(password_strength("Hunter22!"), 3);
        assert_eq!(password_strength("correct horse battery"), 4);
    }

    #[test]
    fn presets_round_trip() {
        let p = Preset {
            name: "Photos".into(),
            format: OutFormat::SevenZ,
            level: 7,
            keep_permissions: false,
            split: Some(1),
            exclude: vec!["*.tmp".into(), ".git/".into()],
        };
        assert_eq!(Preset::decode("Photos", &p.encode()), Some(p));
    }

    #[test]
    fn names() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("report.final.pdf");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(default_name(std::slice::from_ref(&file)), "report");
        assert_eq!(
            default_name(&[dir.path().to_owned()]),
            dir.path().file_name().unwrap().to_string_lossy()
        );
        let parent = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(default_name(&[file.clone(), file]), parent);
    }
}
