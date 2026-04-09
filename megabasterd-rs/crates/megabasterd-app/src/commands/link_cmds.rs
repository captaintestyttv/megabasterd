use megabasterd_core::link_parser::{detect_mega_links, LinkInfo};
use megabasterd_core::mega_api::FolderNode;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn detect_links(
    _state: State<'_, AppState>,
    text: String,
) -> Result<Vec<LinkInfo>, String> {
    let links = detect_mega_links(&text);
    Ok(links.iter().map(LinkInfo::from).collect())
}

#[tauri::command]
pub async fn enable_clipboard_monitor(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state.clipboard_monitor.set_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub async fn browse_folder_link(
    _state: State<'_, AppState>,
    folder_id: String,
    folder_key: String,
) -> Result<Vec<FolderNode>, String> {
    let api = megabasterd_core::mega_api::MegaApiClient::new()
        .map_err(|e| e.to_string())?;
    api.get_folder_nodes(&folder_id, &folder_key)
        .await
        .map_err(|e| e.to_string())
}
