//! What Crate remembers between runs, in `~/.config/crate/state.ini` as
//! `key=value` lines: window sizes, presets, and the Preferences
//! (`crate::prefs`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Settings {
    values: BTreeMap<String, String>,
    /// Keys set since loading. Every window is its own process, so saving
    /// writes only these over what is on disk now.
    changed: BTreeSet<String>,
}

impl Settings {
    pub fn load() -> Self {
        let text = file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .unwrap_or_default();
        Self::parse(&text)
    }

    fn parse(text: &str) -> Self {
        let values = text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .collect();
        Self {
            values,
            changed: BTreeSet::new(),
        }
    }

    pub fn get<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.values.get(key)?.parse().ok()
    }

    pub fn set(&mut self, key: &str, value: impl ToString) {
        self.values.insert(key.to_owned(), value.to_string());
        self.changed.insert(key.to_owned());
    }

    /// Best effort: losing a window size is not worth an error dialog.
    /// Values other windows saved in the meantime are kept.
    pub fn save(&mut self) {
        let Some(path) = file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        let mut merged = Self::parse(&on_disk).values;
        for key in std::mem::take(&mut self.changed) {
            if let Some(v) = self.values.get(&key) {
                merged.insert(key, v.clone());
            }
        }
        self.values = merged;
        let text: String = self
            .values
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect();
        // Write next to it, then rename, so a crash never leaves half a file.
        let tmp = path.with_extension("ini.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

/// A key for something remembered about one archive, such as
/// `archive.<hash>.encoding`. The path is hashed (FNV-1a, stable between
/// builds) so names with `=` or line breaks can't break the file.
pub fn archive_key(archive: &Path, what: &str) -> String {
    let path = std::fs::canonicalize(archive).unwrap_or_else(|_| archive.to_owned());
    let hash = path
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    format!("archive.{hash:016x}.{what}")
}

fn file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("crate").join("state.ini"))
}

/// The user's Downloads folder (XDG user dirs on Linux), or `~/Downloads`.
pub fn downloads_dir() -> PathBuf {
    user_dir("DOWNLOAD", "Downloads")
}

pub fn desktop_dir() -> PathBuf {
    user_dir("DESKTOP", "Desktop")
}

fn user_dir(key: &str, fallback: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let wanted = format!("XDG_{key}_DIR=");
    std::fs::read_to_string(config.join("user-dirs.dirs"))
        .ok()
        .and_then(|text| {
            text.lines().find_map(|l| {
                let v = l.trim().strip_prefix(&wanted)?.trim_matches('"');
                Some(match v.strip_prefix("$HOME") {
                    Some(rest) => home.join(rest.trim_start_matches('/')),
                    None => PathBuf::from(v),
                })
            })
        })
        .unwrap_or_else(|| home.join(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_reads_values() {
        let mut s = Settings::parse("full_width=1200\n# comment\nfull_height = 800\nbad");
        assert_eq!(s.get::<u32>("full_width"), Some(1200));
        assert_eq!(s.get::<u32>("full_height"), Some(800));
        assert_eq!(s.get::<u32>("missing"), None);
        s.set("full_width", 900);
        assert_eq!(s.get::<u32>("full_width"), Some(900));
    }

    #[test]
    fn archive_keys_are_stable_and_safe() {
        let k = archive_key(Path::new("/no/such/a=b\nc.zip"), "encoding");
        assert!(k.starts_with("archive.") && k.ends_with(".encoding"), "{k}");
        assert!(!k.contains('=') && !k.contains('\n'));
        assert_eq!(k, archive_key(Path::new("/no/such/a=b\nc.zip"), "encoding"));
        assert_ne!(k, archive_key(Path::new("/no/such/other.zip"), "encoding"));
    }

    #[test]
    fn saving_keeps_what_other_windows_saved() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: tests in this binary don't read XDG_CONFIG_HOME concurrently.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", dir.path()) };
        let mut a = Settings::load();
        let mut b = Settings::load();
        a.set("theme", 2);
        a.save();
        b.set("full_width", 900);
        b.save();
        let disk = Settings::load();
        assert_eq!(disk.get::<u32>("theme"), Some(2));
        assert_eq!(disk.get::<u32>("full_width"), Some(900));
        assert_eq!(b.get::<u32>("theme"), Some(2), "b sees it after saving");
    }
}
