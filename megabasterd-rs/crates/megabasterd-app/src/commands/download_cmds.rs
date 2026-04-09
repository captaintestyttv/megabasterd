use std::path::PathBuf;

use megabasterd_core::download::{DownloadInfo, DownloadParams};
use megabasterd_core::transfer_manager::TransferSummary;
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

#[tauri::command]
pub async fn add_downloads(
    state: State<'_, AppState>,
    params: Vec<serde_json::Value>,
) -> Result<Vec<String>, String> {
    let parsed: Vec<DownloadParams> = params
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;

    let ids = state.transfer_manager.add_downloads(parsed).await;
    Ok(ids.into_iter().map(|id| id.to_string()).collect())
}

#[tauri::command]
pub async fn pause_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    if let Some(handle) = state.orchestrator.get(uuid).await {
        handle.pause().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn resume_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    if let Some(handle) = state.orchestrator.get(uuid).await {
        handle.resume().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    if let Some(handle) = state.orchestrator.get(uuid).await {
        handle.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn set_download_slots(
    state: State<'_, AppState>,
    id: String,
    slots: u32,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    if let Some(handle) = state.orchestrator.get(uuid).await {
        handle.set_slots(slots);
    }
    Ok(())
}

#[tauri::command]
pub async fn pause_all(state: State<'_, AppState>) -> Result<(), String> {
    state.transfer_manager.pause_all().await;
    Ok(())
}

#[tauri::command]
pub async fn resume_all(state: State<'_, AppState>) -> Result<(), String> {
    state.transfer_manager.resume_all().await;
    Ok(())
}

#[tauri::command]
pub async fn get_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadInfo>, String> {
    let handles = state.orchestrator.get_all().await;
    let mut infos = Vec::new();
    for h in handles {
        if let Some(info) = state.orchestrator.get_download_info(h.id).await {
            infos.push(info);
        }
    }
    Ok(infos)
}

#[tauri::command]
pub async fn move_download(
    state: State<'_, AppState>,
    id: String,
    direction: String,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    match direction.as_str() {
        "top" => state.transfer_manager.move_to_top(uuid).await,
        "up" => state.transfer_manager.move_up(uuid).await,
        "down" => state.transfer_manager.move_down(uuid).await,
        "bottom" => state.transfer_manager.move_to_bottom(uuid).await,
        _ => return Err(format!("Unknown direction: {}", direction)),
    }
    Ok(())
}

#[tauri::command]
pub async fn close_all_finished(state: State<'_, AppState>) -> Result<(), String> {
    state.transfer_manager.close_finished().await;
    Ok(())
}
