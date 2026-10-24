use std::path::PathBuf;

/// Errors that stop an operation. Unsafe or unsupported *entries* don't end
/// up here: extraction skips them and lists them in the report.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}: not an archive format Crate can read")]
    UnknownFormat(PathBuf),
    #[error("damaged or invalid archive: {0}")]
    Corrupt(String),
    #[error("{0} archives are not supported yet")]
    Unsupported(&'static str),
    #[error("the archive is password-protected")]
    PasswordRequired,
    #[error("cancelled")]
    Cancelled,
    #[error("not enough free space: {needed} bytes needed, {available} available")]
    NotEnoughSpace { needed: u64, available: u64 },
    #[error(
        "the archive would expand {ratio:.0}× (limit {limit:.0}×); it may be a decompression bomb"
    )]
    SuspiciousRatio { ratio: f64, limit: f64 },
    #[error("{path}: more data than the archive declares; it may be a decompression bomb")]
    SizeExceeded { path: String },
    #[error("unknown filename encoding: {0}")]
    UnknownEncoding(String),
}

pub type Result<T> = std::result::Result<T, Error>;
