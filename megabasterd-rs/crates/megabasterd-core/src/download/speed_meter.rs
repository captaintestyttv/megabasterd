/// Real-time speed and ETA tracking.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use uuid::Uuid;

const SAMPLE_WINDOW_MS: u64 = 3000; // 3-second rolling window

struct Sample {
    bytes: u64,
    time: Instant,
}

struct DownloadSamples {
    samples: Vec<Sample>,
    last_progress: u64,
}

pub struct SpeedMeter {
    downloads: RwLock<HashMap<Uuid, DownloadSamples>>,
}

impl SpeedMeter {
    pub fn new() -> Self {
        Self {
            downloads: RwLock::new(HashMap::new()),
        }
    }

    pub async fn attach(&self, id: Uuid, initial_progress: u64) {
        let mut map = self.downloads.write().await;
        map.insert(id, DownloadSamples {
            samples: Vec::new(),
            last_progress: initial_progress,
        });
    }

    pub async fn detach(&self, id: Uuid) {
        self.downloads.write().await.remove(&id);
    }

    pub async fn update(&self, id: Uuid, current_progress: u64) {
        let mut map = self.downloads.write().await;
        if let Some(entry) = map.get_mut(&id) {
            let bytes_since_last = current_progress.saturating_sub(entry.last_progress);
            entry.last_progress = current_progress;
            if bytes_since_last > 0 {
                entry.samples.push(Sample {
                    bytes: bytes_since_last,
                    time: Instant::now(),
                });
            }
            // Prune old samples
            let cutoff = Instant::now() - Duration::from_millis(SAMPLE_WINDOW_MS);
            entry.samples.retain(|s| s.time >= cutoff);
        }
    }

    pub async fn get_speed(&self, id: Uuid) -> Option<u64> {
        let map = self.downloads.read().await;
        let entry = map.get(&id)?;
        let cutoff = Instant::now() - Duration::from_millis(SAMPLE_WINDOW_MS);
        let recent_bytes: u64 = entry.samples.iter()
            .filter(|s| s.time >= cutoff)
            .map(|s| s.bytes)
            .sum();
        Some(recent_bytes * 1000 / SAMPLE_WINDOW_MS) // bytes per second
    }

    pub async fn get_global_speed(&self) -> u64 {
        let map = self.downloads.read().await;
        let mut total = 0u64;
        let cutoff = Instant::now() - Duration::from_millis(SAMPLE_WINDOW_MS);
        for entry in map.values() {
            let bytes: u64 = entry.samples.iter()
                .filter(|s| s.time >= cutoff)
                .map(|s| s.bytes)
                .sum();
            total += bytes * 1000 / SAMPLE_WINDOW_MS;
        }
        total
    }

    /// Estimate ETA in seconds given remaining bytes and current speed.
    pub fn eta_secs(remaining_bytes: u64, speed_bps: u64) -> Option<u64> {
        if speed_bps == 0 {
            None
        } else {
            Some(remaining_bytes / speed_bps)
        }
    }
}
