//! Archive engine shared by the Crate app and the `crate` CLI.
//!
//! No UI dependencies belong here. Every file written during extraction goes
//! through [`safety`].

mod archive;
pub mod checksum;
pub mod convert;
pub mod create;
pub mod destination;
pub mod edit;
pub mod encoding;
mod entry;
mod error;
mod formats;
pub mod nested;
pub mod pattern;
mod progress;
pub mod safety;
pub mod volumes;

pub use archive::{
    Archive, ExtractOptions, ExtractReport, ListProgress, OpenOptions, Selection, TestReport,
};
pub use destination::Destination;
pub use encoding::{NameCodec, NameEncoding};
pub use entry::{Entry, EntryKind, Format};
pub use error::{Error, Result};
pub use progress::{CancelToken, Progress};
pub use safety::{Conflict, ConflictResolver, IfExists, Limits, Resolution, SkipReason, Skipped};

/// Version of the engine, as set in the workspace `Cargo.toml`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_package() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        assert!(!version().is_empty());
    }

    /// The app moves an open archive to a worker thread.
    #[test]
    fn archive_is_send() {
        fn is_send<T: Send>() {}
        is_send::<Archive>();
        is_send::<CancelToken>();
    }
}
