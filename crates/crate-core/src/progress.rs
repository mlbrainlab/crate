use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Snapshot passed to progress callbacks.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    /// Entry being processed.
    pub current: String,
}

/// Shared flag for cancelling a running operation from another thread. It is
/// checked between entries and between chunks of a large entry.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
