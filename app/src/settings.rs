//! Small things Crate remembers between runs, in `~/.config/crate/state.ini`
//! as `key=value` lines. The full settings screen (M5) will build on this.

use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct Settings {
    values: BTreeMap<String, String>,
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
        Self { values }
    }

    pub fn get<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.values.get(key)?.parse().ok()
    }

    pub fn set(&mut self, key: &str, value: impl ToString) {
        self.values.insert(key.to_owned(), value.to_string());
    }

    /// Best effort: losing a window size is not worth an error dialog.
    pub fn save(&self) {
        let Some(path) = file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
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
}
