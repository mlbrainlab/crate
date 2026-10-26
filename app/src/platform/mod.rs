//! Platform-only code. Linux is implemented; Windows and macOS are stubs until M7/M8.

/// Reverse-DNS app id; also the Wayland app id and, later, the .desktop file name.
pub const APP_ID: &str = "io.github.mlbrainlab.crate";

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(not(target_os = "linux"))]
mod other;
#[cfg(not(target_os = "linux"))]
pub use other::*;

/// A file type Crate can be the default app for (First run, Preferences).
#[derive(Debug)]
pub struct FileType {
    /// ".zip", ".tar.*"
    pub label: &'static str,
    /// The MIME types it covers, the most common first; the same ones the
    /// .desktop file lists.
    pub mimes: &'static [&'static str],
}

pub const FILE_TYPES: [FileType; 4] = [
    FileType {
        label: ".zip",
        mimes: &["application/zip", "application/x-zip-compressed"],
    },
    FileType {
        label: ".7z",
        mimes: &["application/x-7z-compressed"],
    },
    FileType {
        label: ".rar",
        mimes: &[
            "application/vnd.rar",
            "application/x-rar",
            "application/x-rar-compressed",
        ],
    },
    FileType {
        label: ".tar.*",
        mimes: &[
            "application/x-compressed-tar",
            "application/x-tar",
            "application/x-gtar",
            "application/x-xz-compressed-tar",
            "application/x-zstd-compressed-tar",
            "application/x-bzip2-compressed-tar",
            "application/x-bzip-compressed-tar",
        ],
    },
];

/// Whether Crate can make itself the default app here. The numbers are
/// what the UI's `defaults` properties take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Defaults {
    /// Set directly (xdg-mime on Linux).
    Direct = 0,
    /// Inside Flatpak: the sandbox can't, so the user does it in Files.
    Flatpak = 1,
    /// No installed .desktop file to point at (e.g. `cargo run`).
    NotInstalled = 2,
    /// Windows and macOS until M7/M8.
    Unsupported = 3,
}

/// How default apps are read and written: the system's tools, or a stand-in
/// for tests (so they never touch the real `mimeapps.list`).
#[derive(Clone, Copy)]
pub struct MimeBackend {
    pub support: fn() -> Defaults,
    /// The .desktop file that opens `mime` by default.
    pub query: fn(&str) -> Option<String>,
    /// Makes `desktop` the default for every one of `mimes`.
    pub set: fn(&str, &[&str]) -> std::io::Result<()>,
}

thread_local! {
    static MIME: std::cell::Cell<MimeBackend> = const { std::cell::Cell::new(SYSTEM_MIME) };
}

pub fn set_mime_backend(backend: MimeBackend) {
    MIME.with(|m| m.set(backend));
}

pub fn defaults_support() -> Defaults {
    (MIME.with(|m| m.get()).support)()
}

fn desktop_file() -> String {
    format!("{APP_ID}.desktop")
}

/// True when Crate opens this type by default (judged by its main MIME type).
pub fn is_default(t: &FileType) -> bool {
    (MIME.with(|m| m.get()).query)(t.mimes[0]).as_deref() == Some(desktop_file().as_str())
}

/// Makes Crate the default app for these types.
pub fn make_default(types: &[&FileType]) -> std::io::Result<()> {
    let mimes: Vec<&str> = types.iter().flat_map(|t| t.mimes.iter().copied()).collect();
    if mimes.is_empty() {
        return Ok(());
    }
    (MIME.with(|m| m.get()).set)(&desktop_file(), &mimes)
}
