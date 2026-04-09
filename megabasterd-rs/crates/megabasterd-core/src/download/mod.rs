/// Download orchestrator, ported from Download.java.
/// Coordinates chunk downloaders, writer, progress tracking, and pause/resume.
pub mod chunk;
pub mod chunk_downloader;
pub mod chunk_writer;
pub mod progress;
pub mod speed_meter;
pub mod throttle;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::sync::{Notify, RwLock};
use tokio::task::JoinSet;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::{AppConfig, PROGRESS_WATCHDOG_TIMEOUT_S};
use crate::crypto::{init_mega_link_key, init_mega_link_key_iv};
use crate::db::{Database, DownloadRecord};
use crate::download::chunk::{calculate_chunk_offset, calculate_last_written_chunk};
use crate::download::chunk_downloader::{ChunkDownloaderConfig, ChunkIdDispenser, chunk_downloader_worker};
use crate::download::chunk_writer::chunk_writer_worker;
use crate::download::progress::ProgressTracker;
use crate::download::speed_meter::SpeedMeter;
use crate::download::throttle::BandwidthThrottler;
use crate::mega_api::MegaApiClient;
use crate::proxy::SmartProxyManager;
use crate::util::sha1_hex;

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    Queued,
    Provisioning,
    WaitingToStart,
    Running,
    Paused,
    Finished,
    Failed(String),
    Cancelled,
}

// ---------------------------------------------------------------------------
// Download parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadParams {
    pub url: String,
    pub file_id: String,
    pub file_key: String,
    pub file_name: Option<String>,
    pub file_size: Option<u64>,
    pub download_path: PathBuf,
    pub file_pass: Option<String>,
    pub file_noexpire: Option<String>,
    pub mega_account_email: Option<String>,
    pub custom_chunks_dir: Option<PathBuf>,
    pub slots: u32,
}

// ---------------------------------------------------------------------------
// Download handle — shared between orchestrator and callers
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct DownloadHandle {
    pub id: Uuid,
    pub params: DownloadParams,
    pub file_size: u64,
    pub file_name: String,
    pub progress: Arc<ProgressTracker>,
    state: Arc<RwLock<DownloadState>>,
    cancel: CancellationToken,
    pause_notify: Arc<Notify>,
    slots: Arc<AtomicU32>,
}

impl DownloadHandle {
    pub async fn state(&self) -> DownloadState {
        self.state.read().await.clone()
    }

    pub async fn pause(&self) {
        *self.state.write().await = DownloadState::Paused;
    }

    pub async fn resume(&self) {
        *self.state.write().await = DownloadState::Running;
        self.pause_notify.notify_waiters();
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn set_slots(&self, n: u32) {
        self.slots.store(n.clamp(1, 20), Ordering::Relaxed);
    }

    pub fn get_slots(&self) -> u32 {
        self.slots.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Download info for IPC / frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub id: String,
    pub file_name: String,
    pub file_size: u64,
    pub progress: u64,
    pub speed: u64,
    pub state: String,
    pub eta_secs: Option<u64>,
    pub slots: u32,
    pub url: String,
}

// ---------------------------------------------------------------------------
// Download orchestrator
// ---------------------------------------------------------------------------

pub struct DownloadOrchestrator {
    config: AppConfig,
    db: Arc<Database>,
    proxy_manager: Option<Arc<SmartProxyManager>>,
    throttler: Arc<BandwidthThrottler>,
    speed_meter: Arc<SpeedMeter>,
    downloads: RwLock<HashMap<Uuid, DownloadHandle>>,
}

impl DownloadOrchestrator {
    pub fn new(
        config: AppConfig,
        db: Arc<Database>,
        proxy_manager: Option<Arc<SmartProxyManager>>,
    ) -> Self {
        let limit = if config.limit_download_speed {
            config.max_download_speed_kbps as u64 * 1024
        } else {
            0
        };
        Self {
            config,
            db,
            proxy_manager,
            throttler: Arc::new(BandwidthThrottler::new(limit, 16 * 1024)),
            speed_meter: Arc::new(SpeedMeter::new()),
            downloads: RwLock::new(HashMap::new()),
        }
    }

    /// Start a new download. Returns a handle for monitoring and control.
    pub async fn start_download(&self, params: DownloadParams) -> Result<DownloadHandle> {
        let id = Uuid::new_v4();
        let cancel = CancellationToken::new();
        let pause_notify = Arc::new(Notify::new());

        let progress = Arc::new(ProgressTracker::new(0));
        let state = Arc::new(RwLock::new(DownloadState::Provisioning));
        let slots = Arc::new(AtomicU32::new(params.slots));

        let handle = DownloadHandle {
            id,
            file_size: params.file_size.unwrap_or(0),
            file_name: params.file_name.clone().unwrap_or_else(|| "unknown".to_string()),
            params: params.clone(),
            progress: progress.clone(),
            state: state.clone(),
            cancel: cancel.clone(),
            pause_notify: pause_notify.clone(),
            slots: slots.clone(),
        };

        self.downloads.write().await.insert(id, handle.clone());

        // Spawn the download task
        let config = self.config.clone();
        let db = Arc::clone(&self.db);
        let proxy_manager = self.proxy_manager.clone();
        let throttler = Arc::clone(&self.throttler);
        let speed_meter = Arc::clone(&self.speed_meter);

        tokio::spawn(async move {
            if let Err(e) = run_download(
                id, params, config, db, proxy_manager, throttler, speed_meter,
                progress, state.clone(), cancel, pause_notify, slots,
            ).await {
                error!("Download {} failed: {}", id, e);
                *state.write().await = DownloadState::Failed(e.to_string());
            }
        });

        Ok(handle)
    }

    pub async fn get_all(&self) -> Vec<DownloadHandle> {
        self.downloads.read().await.values().cloned().collect()
    }

    pub async fn get(&self, id: Uuid) -> Option<DownloadHandle> {
        self.downloads.read().await.get(&id).cloned()
    }

    pub async fn remove(&self, id: Uuid) -> Option<DownloadHandle> {
        self.downloads.write().await.remove(&id)
    }

    pub async fn get_download_info(&self, id: Uuid) -> Option<DownloadInfo> {
        let handle = self.downloads.read().await.get(&id).cloned()?;
        let progress = handle.progress.get();
        let speed = self.speed_meter.get_speed(id).await.unwrap_or(0);
        let remaining = handle.file_size.saturating_sub(progress);
        let eta = SpeedMeter::eta_secs(remaining, speed);
        let state = handle.state().await;

        Some(DownloadInfo {
            id: id.to_string(),
            file_name: handle.file_name.clone(),
            file_size: handle.file_size,
            progress,
            speed,
            state: format!("{:?}", state),
            eta_secs: eta,
            slots: handle.get_slots(),
            url: handle.params.url.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Core download execution
// ---------------------------------------------------------------------------

async fn run_download(
    id: Uuid,
    params: DownloadParams,
    config: AppConfig,
    db: Arc<Database>,
    proxy_manager: Option<Arc<SmartProxyManager>>,
    throttler: Arc<BandwidthThrottler>,
    speed_meter: Arc<SpeedMeter>,
    progress: Arc<ProgressTracker>,
    state: Arc<RwLock<DownloadState>>,
    cancel: CancellationToken,
    pause_notify: Arc<Notify>,
    slots: Arc<AtomicU32>,
) -> Result<()> {
    info!("Starting download {}: {}", id, params.url);
    *state.write().await = DownloadState::Provisioning;

    // Resolve file metadata if not provided
    let api = MegaApiClient::new()?;
    let file_size = match params.file_size {
        Some(s) => s,
        None => {
            let meta = api.get_file_metadata(&params.file_id, &params.file_key).await
                .map_err(|e| anyhow!("Failed to get metadata: {}", e))?;
            meta.size
        }
    };

    let file_name = params.file_name.clone().unwrap_or_else(|| {
        // Try to get from metadata — placeholder
        "download".to_string()
    });

    // Get download URL
    let download_url = api.get_download_url(&params.file_id).await
        .map_err(|e| anyhow!("Failed to get download URL: {}", e))?;
    let file_url = Arc::new(RwLock::new(download_url));

    // Derive AES key and IV from file key
    let aes_key = init_mega_link_key(&params.file_key);
    let aes_iv = init_mega_link_key_iv(&params.file_key);

    // Set up temp directory
    let chunks_dir = params.custom_chunks_dir.clone().unwrap_or_else(|| {
        params.download_path.join(format!(".MEGABASTERD_CHUNKS_{}", sha1_hex(&params.url)))
    });
    fs::create_dir_all(&chunks_dir).await?;

    // Check for resume
    let output_path = params.download_path.join(&file_name);
    let start_chunk = if output_path.exists() {
        let current_size = fs::metadata(&output_path).await?.len();
        let last = calculate_last_written_chunk(current_size, config.chunk_size_multi);
        progress.set(current_size);
        last + 1
    } else {
        1
    };

    let dispenser = Arc::new(ChunkIdDispenser::new(start_chunk, file_size, config.chunk_size_multi));
    let writer_notify = Arc::new(Notify::new());

    speed_meter.attach(id, progress.get()).await;
    *state.write().await = DownloadState::Running;

    // Save to DB for persistence
    if let Err(e) = db.insert_download(&DownloadRecord {
        url: params.url.clone(),
        email: params.mega_account_email.clone(),
        path: params.download_path.to_string_lossy().to_string(),
        filename: file_name.clone(),
        filekey: params.file_key.clone(),
        filesize: file_size,
        filepass: params.file_pass.clone(),
        filenoexpire: params.file_noexpire.clone(),
        custom_chunks_dir: params.custom_chunks_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
    }) {
        warn!("Failed to persist download to DB: {}", e);
    }

    // Spawn writer task
    let writer_cancel = cancel.clone();
    let writer_notify_clone = Arc::clone(&writer_notify);
    let writer_key = aes_key.clone();
    let writer_iv = aes_iv.clone();
    let writer_chunks_dir = chunks_dir.clone();
    let writer_output = output_path.clone();
    let progress_clone = Arc::clone(&progress);
    let state_clone = Arc::clone(&state);
    let size_multi = config.chunk_size_multi;

    let writer_handle = tokio::spawn(async move {
        chunk_writer_worker(
            writer_key, writer_iv, file_size,
            writer_chunks_dir, writer_output,
            start_chunk, size_multi,
            writer_cancel, writer_notify_clone,
            move |written| { progress_clone.set(written); },
        ).await
    });

    // Spawn chunk downloader workers
    let num_slots = slots.load(Ordering::Relaxed);
    let mut worker_tasks = JoinSet::new();

    for worker_id in 0..num_slots {
        let cfg = ChunkDownloaderConfig {
            download_id: id,
            worker_id,
            file_size,
            size_multi: config.chunk_size_multi,
            chunks_dir: chunks_dir.clone(),
        };
        worker_tasks.spawn(chunk_downloader_worker(
            cfg,
            Arc::clone(&file_url),
            Arc::clone(&dispenser),
            Arc::clone(&progress),
            Arc::clone(&throttler),
            proxy_manager.clone(),
            cancel.clone(),
            Arc::clone(&writer_notify),
        ));
    }

    // Progress watchdog
    let watchdog_cancel = cancel.clone();
    let watchdog_progress = Arc::clone(&progress);
    tokio::spawn(async move {
        let mut last_progress = watchdog_progress.get();
        let interval = Duration::from_secs(PROGRESS_WATCHDOG_TIMEOUT_S);
        loop {
            sleep(interval).await;
            if watchdog_cancel.is_cancelled() {
                break;
            }
            let current = watchdog_progress.get();
            if current == last_progress {
                warn!("Download watchdog: no progress for {}s, cancelling", PROGRESS_WATCHDOG_TIMEOUT_S);
                watchdog_cancel.cancel();
                break;
            }
            last_progress = current;
        }
    });

    // Wait for all workers
    while let Some(result) = worker_tasks.join_next().await {
        if let Err(e) = result {
            warn!("Worker task panicked: {}", e);
        }
    }

    // Notify writer to finish
    writer_notify.notify_waiters();
    let bytes_written = writer_handle.await??;

    if cancel.is_cancelled() {
        *state.write().await = DownloadState::Cancelled;
        return Ok(());
    }

    // Clean up chunks dir
    if let Err(e) = fs::remove_dir(&chunks_dir).await {
        debug!("Could not remove chunks dir (may have files): {}", e);
    }

    // Remove from DB
    if let Err(e) = db.delete_download(&params.url) {
        warn!("Failed to remove download from DB: {}", e);
    }

    speed_meter.detach(id).await;
    *state.write().await = DownloadState::Finished;
    info!("Download {} complete: {} bytes written", id, bytes_written);

    Ok(())
}
