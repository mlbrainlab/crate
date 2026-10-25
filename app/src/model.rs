//! What the mini view shows, worked out from the archive listing. Plain data
//! with no Slint types, so it can be tested without a window.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use crate_core::{Entry, EntryKind};

/// Tile colors in the list (`MiniView.dc.html`); the numbers match `Row.kind`
/// in `mini-view.slint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Folder = 0,
    Pdf = 1,
    Plain = 2,
    Sheet = 3,
    Document = 4,
    Image = 5,
    Media = 6,
    Archive = 7,
    Code = 8,
}

/// Tile and label for a file name: `("PDF", Tile::Pdf)`.
pub fn file_kind(name: &str) -> (String, Tile) {
    let ext = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .filter(|e| !e.is_empty() && e.chars().count() <= 4)
        .unwrap_or_else(|| "FILE".into());
    let tile = match ext.as_str() {
        "PDF" => Tile::Pdf,
        "XLS" | "XLSX" | "ODS" | "CSV" | "NUMB" => Tile::Sheet,
        "DOC" | "DOCX" | "ODT" | "RTF" | "PAGE" => Tile::Document,
        "PNG" | "JPG" | "JPEG" | "GIF" | "WEBP" | "BMP" | "SVG" | "HEIC" | "TIF" | "TIFF"
        | "ICO" | "AVIF" => Tile::Image,
        "MP4" | "MOV" | "MKV" | "AVI" | "WEBM" | "MP3" | "WAV" | "FLAC" | "OGG" | "M4A"
        | "OPUS" => Tile::Media,
        "ZIP" | "7Z" | "RAR" | "TAR" | "GZ" | "XZ" | "ZST" | "BZ2" | "TGZ" | "JAR" | "APK" => {
            Tile::Archive
        }
        "RS" | "PY" | "JS" | "TS" | "C" | "H" | "CPP" | "HPP" | "GO" | "JAVA" | "KT" | "SH"
        | "JSON" | "TOML" | "YAML" | "YML" | "XML" | "HTML" | "CSS" | "SQL" | "LUA" | "RB" => {
            Tile::Code
        }
        _ => Tile::Plain,
    };
    (ext, tile)
}

/// One row in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// What extraction selects: an entry path, or a top-level folder name
    /// (which includes everything inside it).
    pub path: String,
    pub name: String,
    pub ext: String,
    pub meta: String,
    pub tile: Tile,
}

/// Most rows a search shows; the list is for picking, not browsing.
pub const SEARCH_LIMIT: usize = 500;

/// The top level of the archive, folders first, each sorted by name.
/// Folders appear even when the archive has no entry for them.
pub fn top_level(entries: &[Entry]) -> Vec<Row> {
    #[derive(Default)]
    struct Item {
        folder: bool,
        items: u64,
        size: u64,
    }
    let mut top: BTreeMap<String, Item> = BTreeMap::new();
    for e in entries {
        let path = clean(&e.path).trim_end_matches('/');
        let Some(first) = path.split('/').next().filter(|s| !s.is_empty()) else {
            continue;
        };
        let item = top.entry(first.to_owned()).or_default();
        if path.len() > first.len() {
            item.folder = true;
            item.items += 1;
        } else {
            item.folder |= e.is_dir();
            item.size = e.size;
        }
    }
    let mut rows: Vec<(bool, Row)> = top
        .into_iter()
        .map(|(name, item)| {
            let row = if item.folder {
                Row {
                    path: name.clone(),
                    ext: "DIR".into(),
                    meta: count(item.items, "item"),
                    tile: Tile::Folder,
                    name,
                }
            } else {
                file_row(name.clone(), name, item.size)
            };
            (item.folder, row)
        })
        .collect();
    rows.sort_by(|(fa, a), (fb, b)| {
        fb.cmp(fa)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows.into_iter().map(|(_, r)| r).collect()
}

/// Entries whose path contains `query` (ignoring case), in archive order,
/// shown with their full path.
pub fn search(entries: &[Entry], query: &str) -> Vec<Row> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|e| clean(&e.path).to_lowercase().contains(&query))
        .take(SEARCH_LIMIT)
        .map(|e| {
            let path = clean(&e.path).trim_end_matches('/').to_owned();
            match &e.kind {
                EntryKind::Dir => Row {
                    name: path.clone(),
                    path,
                    ext: "DIR".into(),
                    meta: String::new(),
                    tile: Tile::Folder,
                },
                _ => file_row(path.clone(), path, e.size),
            }
        })
        .collect()
}

fn file_row(path: String, name: String, size: u64) -> Row {
    let (ext, tile) = file_kind(&name);
    Row {
        path,
        name,
        ext,
        meta: size_text(size),
        tile,
    }
}

/// Paths as written by some tools start with `./`.
fn clean(path: &str) -> &str {
    path.trim_start_matches("./")
}

/// "142 items · 2.4 GB → 812 MB".
pub fn summary(entries: &[Entry], archive_size: u64) -> String {
    let unpacked: u64 = entries.iter().map(|e| e.size).sum();
    format!(
        "{} · {} → {}",
        count(entries.len() as u64, "item"),
        size_text(unpacked),
        size_text(archive_size)
    )
}

/// The compression method most files use ("Deflate"), for a chip.
pub fn main_method(entries: &[Entry]) -> Option<String> {
    let mut counts: std::collections::HashMap<&str, usize> = Default::default();
    for e in entries {
        if let Some(m) = &e.method {
            *counts.entry(m.as_str()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(m, n)| (n, std::cmp::Reverse(m)))
        .map(|(m, _)| m.to_owned())
}

/// Share of the unpacked size the archive saves, 0..1.
pub fn space_saved(entries: &[Entry], archive_size: u64) -> Option<f32> {
    let unpacked: u64 = entries.iter().map(|e| e.size).sum();
    (unpacked > 0).then(|| (1.0 - archive_size as f64 / unpacked as f64).clamp(0.0, 1.0) as f32)
}

pub fn encrypted_count(entries: &[Entry]) -> usize {
    entries
        .iter()
        .filter(|e| e.encrypted && !e.is_dir())
        .count()
}

/// True when any selected row (or anything inside a selected folder) is
/// encrypted, or nothing is selected and anything is encrypted.
pub fn selection_needs_password(entries: &[Entry], selected: &HashSet<String>) -> bool {
    entries.iter().any(|e| {
        e.encrypted
            && !e.is_dir()
            && (selected.is_empty() || {
                let path = clean(&e.path);
                selected.iter().any(|s| {
                    let s = clean(s).trim_end_matches('/');
                    path == s
                        || path
                            .strip_prefix(s)
                            .is_some_and(|rest| rest.starts_with('/'))
                })
            })
    })
}

/// Whether `path` matches any of the comma- or space-separated glob
/// patterns (see `crate_core::pattern`).
pub fn matches_pattern(patterns: &str, path: &str) -> bool {
    crate_core::pattern::matches_any(patterns, path)
}

/// "1 item", "38 items".
pub fn count(n: u64, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{} {what}s", thousands(n))
    }
}

/// Decimal units like GNOME Files: "4 KB", "3.2 MB", "812 MB", "2.4 GB".
pub fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return count(bytes, "byte");
    }
    let mut value = bytes as f64;
    let mut unit = "";
    for u in UNITS {
        value /= 1000.0;
        unit = u;
        if value < 999.5 {
            break;
        }
    }
    if value < 9.95 && unit != "KB" {
        format!("{value:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

/// "2.1 s", "48 s", "3 min 05 s"; `None` when too short to be worth saying.
pub fn duration_text(d: std::time::Duration) -> Option<String> {
    let secs = d.as_secs_f64();
    Some(if secs < 0.1 {
        return None;
    } else if secs < 10.0 {
        format!("{secs:.1} s")
    } else if secs < 60.0 {
        format!("{secs:.0} s")
    } else {
        let s = d.as_secs();
        format!("{} min {:02} s", s / 60, s % 60)
    })
}

/// "Oct 9, 2026" (UTC: archives rarely say which zone they mean).
pub fn date_text(t: std::time::SystemTime) -> String {
    let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) else {
        return "—".into();
    };
    // Howard Hinnant's civil_from_days.
    let z = (d.as_secs() / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{} {day}, {year}", MONTHS[(month - 1) as usize])
}

/// 48210 → "48,210".
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `/home/me/Downloads/x` → `~/Downloads/x/`, for labels.
pub fn display_folder(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let shown = match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    };
    format!("{}/", shown.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: u64) -> Entry {
        Entry {
            index: 0,
            path: path.into(),
            kind: if path.ends_with('/') {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            size,
            packed_size: None,
            modified: None,
            mode: None,
            crc32: None,
            encrypted: false,
            method: None,
        }
    }

    fn listing() -> Vec<Entry> {
        vec![
            entry("src/", 0),
            entry("src/main.rs", 10),
            entry("src/lib/mod.rs", 10),
            entry("README.md", 4_000),
            entry("./docs/a.pdf", 3_200_000),
            entry("brief.pdf", 3_200_000),
            entry("LICENSE", 1_000),
        ]
    }

    #[test]
    fn top_level_rows() {
        let rows = top_level(&listing());
        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["docs", "src", "brief.pdf", "LICENSE", "README.md"]);
        assert_eq!(rows[1].meta, "2 items");
        assert_eq!(rows[1].tile, Tile::Folder);
        assert_eq!(rows[0].meta, "1 item");
        assert_eq!(
            (rows[2].ext.as_str(), rows[2].tile, rows[2].meta.as_str()),
            ("PDF", Tile::Pdf, "3.2 MB")
        );
        assert_eq!(rows[3].ext, "FILE");
        assert_eq!(rows[4].meta, "4 KB");
    }

    #[test]
    fn search_rows() {
        let rows = search(&listing(), " MOD ");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "src/lib/mod.rs");
        assert_eq!(rows[0].path, "src/lib/mod.rs");
        assert_eq!(search(&listing(), "pdf").len(), 2);
    }

    #[test]
    fn password_needed_only_for_encrypted_selections() {
        let mut entries = listing();
        entries[2].encrypted = true; // src/lib/mod.rs
        let sel = |paths: &[&str]| paths.iter().map(|p| p.to_string()).collect();
        assert!(selection_needs_password(&entries, &sel(&[])));
        assert!(selection_needs_password(&entries, &sel(&["src"])));
        assert!(selection_needs_password(
            &entries,
            &sel(&["src/lib/mod.rs"])
        ));
        assert!(!selection_needs_password(
            &entries,
            &sel(&["README.md", "sr"])
        ));
        assert_eq!(encrypted_count(&entries), 1);
    }

    #[test]
    fn sizes_and_counts() {
        assert_eq!(size_text(0), "0 bytes");
        assert_eq!(size_text(1), "1 byte");
        assert_eq!(size_text(999), "999 bytes");
        assert_eq!(size_text(4_096), "4 KB");
        assert_eq!(size_text(3_210_000), "3.2 MB");
        assert_eq!(size_text(812_000_000), "812 MB");
        assert_eq!(size_text(999_999_999), "1.0 GB");
        assert_eq!(size_text(2_400_000_000), "2.4 GB");
        assert_eq!(
            summary(&listing(), 812_000_000),
            "7 items · 6.4 MB → 812 MB"
        );
        let time = |ms| duration_text(std::time::Duration::from_millis(ms));
        assert_eq!(time(40), None);
        assert_eq!(time(2_100).as_deref(), Some("2.1 s"));
        assert_eq!(time(185_000).as_deref(), Some("3 min 05 s"));
    }

    #[test]
    fn patterns() {
        assert!(matches_pattern("*.jpg", "photos/IMG_1.JPG"));
        assert!(matches_pattern("*.png, *.jpg", "a/b.png"));
        assert!(!matches_pattern("*.jpg", "a/b.jpeg"));
        assert!(matches_pattern("docs/*.pdf", "docs/x.pdf"));
        assert!(!matches_pattern("docs/*.pdf", "other/docs/x.pdf"));
        assert!(matches_pattern("img_??.*", "IMG_12.heic"));
        assert!(!matches_pattern("", "a"));
    }

    #[test]
    fn dates_and_numbers() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_845_015);
        assert_eq!(date_text(t), "Oct 24, 2026");
        assert_eq!(thousands(48_210), "48,210");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000_000), "1,000,000");
        assert_eq!(file_kind("a.JPG").1, Tile::Image);
        assert_eq!(file_kind("main.rs").1, Tile::Code);
        assert_eq!(file_kind("Makefile").0, "FILE");
    }

    #[test]
    fn folders_for_labels() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            display_folder(&Path::new(&home).join("Downloads/x")),
            "~/Downloads/x/"
        );
        assert_eq!(display_folder(Path::new("/srv/x")), "/srv/x/");
    }
}

/// List selection with the usual desktop rules. Rows are addressed by
/// position in the current list; what is selected is kept by path, so it
/// survives searches.
#[derive(Debug, Default)]
pub struct Picker {
    pub selected: HashSet<String>,
    /// Where Shift ranges start.
    anchor: Option<usize>,
    /// The row the keyboard is on.
    pub cursor: Option<usize>,
}

impl Picker {
    /// A click: plain selects only this row, Ctrl toggles it, Shift selects
    /// the range from the anchor (Ctrl+Shift adds the range).
    pub fn click(&mut self, rows: &[String], index: usize, ctrl: bool, shift: bool) {
        if index >= rows.len() {
            return;
        }
        match (ctrl, shift, self.anchor) {
            (_, true, Some(anchor)) => {
                if !ctrl {
                    self.selected.clear();
                }
                self.select_range(rows, anchor, index);
            }
            (true, _, _) => {
                let path = &rows[index];
                if !self.selected.remove(path) {
                    self.selected.insert(path.clone());
                }
                self.anchor = Some(index);
            }
            _ => {
                self.selected.clear();
                self.selected.insert(rows[index].clone());
                self.anchor = Some(index);
            }
        }
        self.cursor = Some(index);
    }

    /// Arrow keys: move the cursor and select it; with Shift, extend the
    /// range from the anchor.
    pub fn step(&mut self, rows: &[String], delta: i32, shift: bool) {
        if rows.is_empty() {
            return;
        }
        let last = rows.len() as i64 - 1;
        let to = match self.cursor {
            Some(c) => (c as i64 + delta as i64).clamp(0, last),
            None if delta < 0 => last,
            None => 0,
        } as usize;
        if shift {
            let anchor = *self.anchor.get_or_insert(self.cursor.unwrap_or(to));
            self.selected.clear();
            self.select_range(rows, anchor, to);
            self.cursor = Some(to);
        } else {
            self.click(rows, to, false, false);
        }
    }

    /// Space: toggle the row under the cursor.
    pub fn toggle_cursor(&mut self, rows: &[String]) {
        if let Some(c) = self.cursor {
            self.click(rows, c, true, false);
        }
    }

    pub fn select_all(&mut self, rows: &[String]) {
        self.selected.extend(rows.iter().cloned());
    }

    pub fn clear(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// The list changed (search, new names): positions mean something else now.
    pub fn rows_changed(&mut self) {
        self.anchor = None;
        self.cursor = None;
    }

    fn select_range(&mut self, rows: &[String], a: usize, b: usize) {
        let (lo, hi) = (a.min(b), a.max(b).min(rows.len() - 1));
        self.selected.extend(rows[lo..=hi].iter().cloned());
    }
}

#[cfg(test)]
mod picker_tests {
    use super::*;

    fn rows(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("r{i}")).collect()
    }

    fn sel(p: &Picker) -> Vec<String> {
        let mut v: Vec<String> = p.selected.iter().cloned().collect();
        v.sort();
        v
    }

    #[test]
    fn clicks() {
        let r = rows(6);
        let mut p = Picker::default();
        p.click(&r, 1, false, false);
        p.click(&r, 3, false, false);
        assert_eq!(sel(&p), ["r3"], "a plain click replaces");
        p.click(&r, 5, true, false);
        assert_eq!(sel(&p), ["r3", "r5"], "Ctrl adds");
        p.click(&r, 3, true, false);
        assert_eq!(sel(&p), ["r5"], "Ctrl toggles off");
        p.click(&r, 2, false, true);
        // The anchor is the last Ctrl-clicked row (3).
        assert_eq!(sel(&p), ["r2", "r3"], "Shift: range from the anchor");
        p.click(&r, 5, true, false);
        p.click(&r, 0, true, true);
        assert_eq!(sel(&p).len(), 6, "Ctrl+Shift adds a range");
    }

    #[test]
    fn keys() {
        let r = rows(4);
        let mut p = Picker::default();
        p.step(&r, 1, false);
        assert_eq!(sel(&p), ["r0"]);
        p.step(&r, 1, true);
        p.step(&r, 1, true);
        assert_eq!(sel(&p), ["r0", "r1", "r2"]);
        p.step(&r, -1, true);
        assert_eq!(sel(&p), ["r0", "r1"], "Shift+Up shrinks the range");
        p.step(&r, 10, false);
        assert_eq!((sel(&p), p.cursor), (vec!["r3".to_string()], Some(3)));
        p.toggle_cursor(&r);
        assert!(p.selected.is_empty());
        p.select_all(&r);
        assert_eq!(sel(&p).len(), 4);
        p.clear();
        assert!(p.selected.is_empty());
    }
}
