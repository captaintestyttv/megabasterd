/// Bandwidth throttler using a token bucket approach.
/// Replaces Java's ThrottledInputStream with async-compatible throttling.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::time::sleep;

use crate::config::THROTTLE_SLICE_SIZE;

pub struct BandwidthThrottler {
    /// 0 = unlimited
    max_bytes_per_sec: AtomicU64,
    slice_size: usize,
}

impl BandwidthThrottler {
    pub fn new(max_bytes_per_sec: u64, slice_size: usize) -> Self {
        Self {
            max_bytes_per_sec: AtomicU64::new(max_bytes_per_sec),
            slice_size,
        }
    }

    pub fn unlimited() -> Self {
        Self::new(0, THROTTLE_SLICE_SIZE)
    }

    pub fn set_limit(&self, max_bytes_per_sec: u64) {
        self.max_bytes_per_sec.store(max_bytes_per_sec, Ordering::Relaxed);
    }

    pub fn get_limit(&self) -> u64 {
        self.max_bytes_per_sec.load(Ordering::Relaxed)
    }

    pub fn is_limited(&self) -> bool {
        self.get_limit() > 0
    }

    /// Wait the appropriate time before allowing `bytes` to be transmitted.
    /// This is called per-slice by chunk downloaders.
    pub async fn throttle(&self, bytes: usize) {
        let limit = self.get_limit();
        if limit == 0 || bytes == 0 {
            return;
        }
        // Time this many bytes should take at the limit
        let required_ms = (bytes as u64 * 1000) / limit;
        if required_ms > 0 {
            sleep(Duration::from_millis(required_ms)).await;
        }
    }
}
