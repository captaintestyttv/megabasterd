use megabasterd_core::db::MegaAccountRecord;
use megabasterd_core::mega_api::MegaApiClient;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn add_mega_account(
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<(), String> {
    // Verify credentials by attempting login
    let mut api = MegaApiClient::new().map_err(|e| e.to_string())?;
    api.login(&email, &password, None).await.map_err(|e| e.to_string())?;

    let session = api.session.as_ref().ok_or("No session after login")?;

    // Store account
    let record = MegaAccountRecord {
        email: email.clone(),
        password: password.clone(),
        password_aes: megabasterd_core::util::i32a_to_bin(&session.password_aes)
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect(),
        user_hash: session.user_hash.clone(),
    };

    state.db.insert_mega_account(&record).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn remove_mega_account(
    state: State<'_, AppState>,
    email: String,
) -> Result<(), String> {
    state.db.delete_mega_account(&email).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_mega_accounts(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let accounts = state.db.select_mega_accounts().map_err(|e| e.to_string())?;
    Ok(accounts.into_keys().collect())
}
