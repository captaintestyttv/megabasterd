use megabasterd_core::config::AppConfig;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppConfig,
) -> Result<(), String> {
    let db = &state.db;
    settings.save_to_db(db).map_err(|e| e.to_string())?;
    *state.config.write().await = settings;
    Ok(())
}

#[tauri::command]
pub async fn select_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app.dialog().file().blocking_pick_folder();
    Ok(path.map(|p| p.to_string_lossy().to_string()))
}
