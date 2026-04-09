#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use std::sync::Arc;
use std::path::PathBuf;
use std::time::Duration;

use megabasterd_core::clipboard::ClipboardMonitor;
use megabasterd_core::config::{megabasterd_dir, AppConfig};
use megabasterd_core::db::Database;
use megabasterd_core::download::DownloadOrchestrator;
use megabasterd_core::proxy::{SmartProxyConfig, SmartProxyManager};
use megabasterd_core::transfer_manager::TransferManager;
use tauri::{Manager, State};
use tokio::sync::{mpsc, RwLock};
use tracing::info;

use commands::{
    account_cmds::{add_mega_account, get_mega_accounts, remove_mega_account},
    download_cmds::{
        add_downloads, cancel_download, cancel_all, close_all_finished, get_downloads,
        move_download, pause_all, pause_download, resume_all, resume_download, set_download_slots,
    },
    link_cmds::{browse_folder_link, detect_links, enable_clipboard_monitor},
    settings_cmds::{get_settings, save_settings, select_directory},
};
use state::AppState;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("megabasterd=debug,warn")
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            tauri::async_runtime::block_on(async move {
                // Determine data directory
                let home_dir = std::env::var("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::from("."));
                let data_dir = megabasterd_dir(&home_dir);
                std::fs::create_dir_all(&data_dir).expect("Cannot create data dir");

                let db_path = data_dir.join("megabasterd.db");
                let db = Arc::new(Database::open(&db_path).expect("Cannot open DB"));

                let config = AppConfig::load_from_db(&db, home_dir)
                    .unwrap_or_default();

                // Smart proxy manager
                let proxy_manager = if config.use_smart_proxy {
                    let mgr = Arc::new(SmartProxyManager::new(SmartProxyConfig {
                        proxy_list_url: config.smart_proxy_url.clone(),
                        ban_time_secs: config.smart_proxy_ban_time_s,
                        timeout_ms: config.smart_proxy_timeout_ms,
                        autorefresh_min: config.smart_proxy_autorefresh_min,
                        random_select: config.smart_proxy_random_select,
                        reset_slot_proxy: config.smart_proxy_reset_slot,
                        force_smart_proxy: config.force_smart_proxy,
                    }));
                    let mgr_clone = Arc::clone(&mgr);
                    tokio::spawn(async move { mgr_clone.start_auto_refresh().await });
                    Some(mgr)
                } else {
                    None
                };

                let orchestrator = Arc::new(DownloadOrchestrator::new(
                    config.clone(),
                    Arc::clone(&db),
                    proxy_manager.clone(),
                ));

                let transfer_manager = TransferManager::new(
                    Arc::clone(&orchestrator),
                    config.max_downloads,
                );

                // Start scheduler
                let tm_clone = Arc::clone(&transfer_manager);
                tokio::spawn(async move { tm_clone.run().await });

                // Clipboard monitor
                let clipboard_monitor = ClipboardMonitor::new(config.monitor_clipboard);
                let (clipboard_tx, mut clipboard_rx) = mpsc::channel(32);
                let cm_clone = Arc::clone(&clipboard_monitor);
                tokio::spawn(async move { cm_clone.run(clipboard_tx).await });

                // Forward clipboard events to frontend
                let app_handle_clone = app_handle.clone();
                tokio::spawn(async move {
                    while let Some(links) = clipboard_rx.recv().await {
                        let infos: Vec<megabasterd_core::link_parser::LinkInfo> =
                            links.iter().map(megabasterd_core::link_parser::LinkInfo::from).collect();
                        let _ = app_handle_clone.emit("clipboard-links", &infos);
                    }
                });

                // Background progress push task
                let app_handle_push = app_handle.clone();
                let orch_push = Arc::clone(&orchestrator);
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        let handles = orch_push.get_all().await;
                        for h in &handles {
                            if let Some(info) = orch_push.get_download_info(h.id).await {
                                let _ = app_handle_push.emit("download-progress", &info);
                            }
                        }
                    }
                });

                let app_state = AppState {
                    db,
                    config: RwLock::new(config),
                    orchestrator,
                    transfer_manager,
                    proxy_manager,
                    clipboard_monitor,
                };

                app_handle.manage(app_state);
                info!("MegaBasterd started");
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Downloads
            add_downloads,
            pause_download,
            resume_download,
            cancel_download,
            set_download_slots,
            pause_all,
            resume_all,
            get_downloads,
            move_download,
            close_all_finished,
            // Settings
            get_settings,
            save_settings,
            select_directory,
            // Accounts
            add_mega_account,
            remove_mega_account,
            get_mega_accounts,
            // Links
            detect_links,
            browse_folder_link,
            enable_clipboard_monitor,
        ])
        .run(tauri::generate_context!())
        .expect("Error running Tauri application");
}

// Stub for cancel_all (not yet in download_cmds)
#[tauri::command]
async fn cancel_all(state: State<'_, AppState>) -> Result<(), String> {
    state.transfer_manager.cancel_all().await;
    Ok(())
}
