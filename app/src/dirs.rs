//! Where Crate keeps its files on each system. The XDG variables win
//! wherever they're set (the tests point them at temporary folders); then
//! Linux uses the XDG defaults under the home folder, Windows `%APPDATA%`
//! and `%LOCALAPPDATA%`.

use std::path::PathBuf;

/// The user's home folder.
pub fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn xdg(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Settings (`state.ini` goes in `crate/` inside): `~/.config`, or
/// `%APPDATA%` (roaming) on Windows.
pub fn config() -> Option<PathBuf> {
    xdg("XDG_CONFIG_HOME").or_else(|| {
        if cfg!(windows) {
            xdg("APPDATA")
        } else {
            home().map(|h| h.join(".config"))
        }
    })
}

/// History and other state worth keeping: `~/.local/state`, or
/// `%LOCALAPPDATA%` on Windows.
pub fn state() -> Option<PathBuf> {
    xdg("XDG_STATE_HOME").or_else(|| {
        if cfg!(windows) {
            xdg("LOCALAPPDATA")
        } else {
            home().map(|h| h.join(".local/state"))
        }
    })
}

/// Things that can be thrown away: `~/.cache`, or `%LOCALAPPDATA%` on
/// Windows.
pub fn cache() -> Option<PathBuf> {
    xdg("XDG_CACHE_HOME").or_else(|| {
        if cfg!(windows) {
            xdg("LOCALAPPDATA")
        } else {
            home().map(|h| h.join(".cache"))
        }
    })
}
