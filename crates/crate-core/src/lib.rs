//! Archive engine shared by the Crate app and the `crate` CLI.
//!
//! No UI or Tauri dependencies belong here.

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
}
