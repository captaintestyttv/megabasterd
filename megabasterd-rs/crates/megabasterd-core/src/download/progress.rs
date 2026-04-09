/// Progress tracking for downloads.
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Tracks bytes downloaded for a single download.
/// Thread-safe: multiple chunk workers update it concurrently.
pub struct ProgressTracker {
    /// Total bytes confirmed written to disk
    confirmed: AtomicU64,
    /// Pending delta (can be negative for rollback)
    pending: AtomicI64,
}

impl ProgressTracker {
    pub fn new(initial_bytes: u64) -> Self {
        Self {
            confirmed: AtomicU64::new(initial_bytes),
            pending: AtomicI64::new(0),
        }
    }

    /// Add a partial progress update (bytes downloaded by a chunk worker).
    pub fn add_partial(&self, bytes: i64) {
        self.pending.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Drain pending and add to confirmed. Returns new confirmed total.
    pub fn flush(&self) -> u64 {
        let delta = self.pending.swap(0, Ordering::AcqRel);
        if delta >= 0 {
            self.confirmed.fetch_add(delta as u64, Ordering::Relaxed) + delta as u64
        } else {
            let sub = (-delta) as u64;
            self.confirmed.fetch_sub(sub, Ordering::Relaxed).saturating_sub(sub)
        }
    }

    pub fn get(&self) -> u64 {
        let confirmed = self.confirmed.load(Ordering::Relaxed);
        let pending = self.pending.load(Ordering::Relaxed);
        if pending >= 0 {
            confirmed + pending as u64
        } else {
            confirmed.saturating_sub((-pending) as u64)
        }
    }

    pub fn set(&self, value: u64) {
        self.confirmed.store(value, Ordering::Relaxed);
        self.pending.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_tracking() {
        let p = ProgressTracker::new(0);
        p.add_partial(1024);
        p.add_partial(2048);
        assert_eq!(p.get(), 3072);
        let flushed = p.flush();
        assert_eq!(flushed, 3072);
        assert_eq!(p.get(), 3072);
    }

    #[test]
    fn test_initial_bytes() {
        let p = ProgressTracker::new(5000);
        assert_eq!(p.get(), 5000);
        p.add_partial(1000);
        assert_eq!(p.get(), 6000);
    }
}
