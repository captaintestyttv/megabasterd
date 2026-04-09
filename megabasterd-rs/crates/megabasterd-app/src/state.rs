use std::sync::Arc;

use megabasterd_core::config::AppConfig;
use megabasterd_core::db::Database;
use megabasterd_core::download::DownloadOrchestrator;
use megabasterd_core::proxy::SmartProxyManager;
use megabasterd_core::transfer_manager::TransferManager;
use megabasterd_core::clipboard::ClipboardMonitor;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

pub struct AppState {
    pub db: Arc<Database>,
    pub config: RwLock<AppConfig>,
    pub orchestrator: Arc<DownloadOrchestrator>,
    pub transfer_manager: Arc<TransferManager>,
    pub proxy_manager: Option<Arc<SmartProxyManager>>,
    pub clipboard_monitor: Arc<ClipboardMonitor>,
}
