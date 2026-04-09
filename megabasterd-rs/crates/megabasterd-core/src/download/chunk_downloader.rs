/// HTTP chunk downloader worker, ported from ChunkDownloader.java.
/// Runs as a Tokio task. Downloads a single chunk at a time from the MEGA CDN.
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::config::{HTTP_CONNECT_TIMEOUT_MS, HTTP_READ_TIMEOUT_MS, MAX_CHUNK_ERRORS};
use crate::config::DEFAULT_USER_AGENT;
use crate::download::chunk::{calculate_chunk_offset, calculate_chunk_size, gen_chunk_url, is_valid_chunk_id};
use crate::download::progress::ProgressTracker;
use crate::download::throttle::BandwidthThrottler;
use crate::proxy::SmartProxyManager;
use crate::util::wait_time_exp_backoff;

/// Shared dispenser for chunk IDs across all workers of a download.
pub struct ChunkIdDispenser {
    next: AtomicU64,
    rejected: Mutex<Vec<u64>>,
    file_size: u64,
    size_multi: u32,
}

impl ChunkIdDispenser {
    pub fn new(start_chunk: u64, file_size: u64, size_multi: u32) -> Self {
        Self {
            next: AtomicU64::new(start_chunk),
            rejected: Mutex::new(Vec::new()),
            file_size,
            size_multi,
        }
    }

    /// Get the next chunk ID to download. Returns None if all chunks are done.
    pub async fn next(&self) -> Option<u64> {
        // Check rejected queue first
        {
            let mut rejected = self.rejected.lock().await;
            if let Some(id) = rejected.pop() {
                return Some(id);
            }
        }
        // Then advance the counter
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        if is_valid_chunk_id(id, self.file_size, self.size_multi) {
            Some(id)
        } else {
            None
        }
    }

    /// Return a chunk ID to the queue (e.g., if download failed mid-chunk).
    pub async fn reject(&self, id: u64) {
        self.rejected.lock().await.push(id);
    }
}

pub struct ChunkDownloaderConfig {
    pub download_id: Uuid,
    pub worker_id: u32,
    pub file_size: u64,
    pub size_multi: u32,
    pub chunks_dir: PathBuf,
}

/// The main chunk downloader worker — runs as a Tokio task.
/// Downloads chunks one at a time until the dispenser is exhausted.
pub async fn chunk_downloader_worker(
    config: ChunkDownloaderConfig,
    file_url: Arc<RwLock<String>>,
    dispenser: Arc<ChunkIdDispenser>,
    progress: Arc<ProgressTracker>,
    throttler: Arc<BandwidthThrottler>,
    proxy_manager: Option<Arc<SmartProxyManager>>,
    cancel: CancellationToken,
    writer_notify: Arc<Notify>,
) -> Result<()> {
    let http = reqwest::Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .connect_timeout(Duration::from_millis(HTTP_CONNECT_TIMEOUT_MS))
        .timeout(Duration::from_millis(HTTP_READ_TIMEOUT_MS))
        .build()?;

    let mut error_count = 0u32;
    let mut excluded_proxies: Vec<String> = Vec::new();
    let mut current_proxy: Option<String> = None;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        let chunk_id = match dispenser.next().await {
            Some(id) => id,
            None => break, // All chunks assigned
        };

        let offset = calculate_chunk_offset(chunk_id, config.size_multi);
        let chunk_size = calculate_chunk_size(chunk_id, config.file_size, config.size_multi);
        if chunk_size == 0 {
            break;
        }

        let url = {
            let base = file_url.read().await.clone();
            gen_chunk_url(&base, offset, chunk_size)
        };

        let tmp_path = config.chunks_dir.join(format!(".chunk{}.tmp", chunk_id));
        let final_path = config.chunks_dir.join(format!(".chunk{}", chunk_id));

        // Already downloaded?
        if final_path.exists() {
            debug!("Worker {}: chunk {} already exists, skipping", config.worker_id, chunk_id);
            writer_notify.notify_one();
            continue;
        }

        debug!("Worker {}: downloading chunk {} (offset={}, size={})",
            config.worker_id, chunk_id, offset, chunk_size);

        let response = http.get(&url).send().await;

        match response {
            Err(e) => {
                warn!("Worker {}: network error on chunk {}: {}", config.worker_id, chunk_id, e);
                dispenser.reject(chunk_id).await;
                error_count += 1;
                if error_count >= MAX_CHUNK_ERRORS {
                    return Err(anyhow::anyhow!("Too many chunk errors"));
                }
                let wait = wait_time_exp_backoff(error_count);
                sleep(Duration::from_secs(wait)).await;
                continue;
            }
            Ok(resp) => {
                let status = resp.status().as_u16();
                match status {
                    509 => {
                        // Bandwidth limit — switch to smart proxy
                        warn!("Worker {}: bandwidth limit (509) on chunk {}", config.worker_id, chunk_id);
                        if let Some(ref pm) = proxy_manager {
                            if let Some(proxy) = current_proxy.take() {
                                pm.block_proxy(&proxy, "509").await;
                                excluded_proxies.push(proxy);
                            }
                            if let Some((addr, _)) = pm.get_proxy(&excluded_proxies).await {
                                current_proxy = Some(addr.clone());
                            }
                        }
                        dispenser.reject(chunk_id).await;
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                    429 => {
                        warn!("Worker {}: rate limited (429) on chunk {}", config.worker_id, chunk_id);
                        dispenser.reject(chunk_id).await;
                        let wait = wait_time_exp_backoff(error_count);
                        sleep(Duration::from_secs(wait)).await;
                        error_count += 1;
                        continue;
                    }
                    403 => {
                        // Forbidden — URL may have expired; the orchestrator will refresh it
                        warn!("Worker {}: forbidden (403) on chunk {}", config.worker_id, chunk_id);
                        dispenser.reject(chunk_id).await;
                        sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    200 | 206 => {
                        // Success
                    }
                    code => {
                        warn!("Worker {}: unexpected status {} on chunk {}", config.worker_id, code, chunk_id);
                        dispenser.reject(chunk_id).await;
                        error_count += 1;
                        sleep(Duration::from_secs(wait_time_exp_backoff(error_count))).await;
                        continue;
                    }
                }

                // Stream response body to temp file
                let mut file = fs::File::create(&tmp_path).await?;
                let mut bytes_written = 0u64;
                let mut stream = resp.bytes_stream();

                use futures_util::StreamExt;
                while let Some(chunk) = stream.next().await {
                    if cancel.is_cancelled() {
                        drop(file);
                        let _ = fs::remove_file(&tmp_path).await;
                        dispenser.reject(chunk_id).await;
                        return Ok(());
                    }
                    let data = chunk?;
                    file.write_all(&data).await?;
                    bytes_written += data.len() as u64;
                    progress.add_partial(data.len() as i64);
                    throttler.throttle(data.len()).await;
                }
                file.flush().await?;
                drop(file);

                // Rename temp → final
                fs::rename(&tmp_path, &final_path).await?;
                debug!("Worker {}: chunk {} complete ({} bytes)", config.worker_id, chunk_id, bytes_written);
                error_count = 0;

                // Notify writer that a new chunk is available
                writer_notify.notify_one();
            }
        }
    }

    Ok(())
}
