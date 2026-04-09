/// Application configuration and constants, ported from MainPanel.java and Transference.java.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::Database;

// ---------------------------------------------------------------------------
// Constants (from Transference.java and MainPanel.java)
// ---------------------------------------------------------------------------

pub const APP_VERSION: &str = "1.0.0";
pub const MIN_WORKERS: u32 = 1;
pub const MAX_WORKERS: u32 = 20;
pub const WORKERS_DEFAULT: u32 = 6;
pub const SIM_TRANSFERENCES_DEFAULT: u32 = 4;
pub const MAX_SIM_TRANSFERENCES: u32 = 50;
pub const CHUNK_SIZE_MULTI: u32 = 20; // 20 MB for chunks 8+
pub const THROTTLE_SLICE_SIZE: usize = 16 * 1024; // 16 KB
pub const DEFAULT_BYTE_BUFFER_SIZE: usize = 16 * 1024;
pub const CLIPBOARD_CHECK_MS: u64 = 250;
pub const PROGRESS_WATCHDOG_TIMEOUT_S: u64 = 600;
pub const MAX_WAIT_WORKERS_SHUTDOWN_S: u64 = 15;
pub const HTTP_CONNECT_TIMEOUT_MS: u64 = 60_000;
pub const HTTP_READ_TIMEOUT_MS: u64 = 60_000;
pub const PROXY_CONNECT_TIMEOUT_MS: u64 = 5_000;
pub const MAX_CHUNK_ERRORS: u32 = 50;
pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:61.0) Gecko/20100101 Firefox/61.0";

// MEGA API error codes — fatal, do not retry
pub const FATAL_API_ERROR_CODES: &[i32] = &[-2, -4, -8, -14, -15, -16, -17, 22, 23, 24];
// MEGA API error codes — no exception needed
pub const MEGA_ERROR_NO_EXCEPTION_CODES: &[i32] = &[-1, -3];

// Smart proxy defaults
pub const SMART_PROXY_BAN_TIME_S: u32 = 300;
pub const SMART_PROXY_TIMEOUT_MS: u32 = 5_000;
pub const SMART_PROXY_AUTOREFRESH_MIN: u32 = 60;

// ---------------------------------------------------------------------------
// AppConfig struct — serializable settings stored in the SQLite settings table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // Paths
    pub home_dir: PathBuf,
    pub default_download_path: PathBuf,
    pub custom_chunks_dir: Option<PathBuf>,
    pub use_custom_chunks_dir: bool,

    // Transfer limits
    pub max_downloads: u32,
    pub default_slots: u32,
    pub use_slots: bool,
    pub chunk_size_multi: u32,
    pub limit_download_speed: bool,
    pub max_download_speed_kbps: u32,

    // Proxy
    pub use_proxy: bool,
    pub proxy_host: Option<String>,
    pub proxy_port: u16,
    pub proxy_user: Option<String>,
    pub proxy_pass: Option<String>,

    // Smart proxy
    pub use_smart_proxy: bool,
    pub smart_proxy_url: Option<String>,
    pub smart_proxy_ban_time_s: u32,
    pub smart_proxy_timeout_ms: u32,
    pub smart_proxy_autorefresh_min: u32,
    pub smart_proxy_random_select: bool,
    pub smart_proxy_reset_slot: bool,
    pub force_smart_proxy: bool,

    // MEGA accounts
    pub use_mega_account_down: bool,
    pub mega_account_down_email: Option<String>,

    // UI
    pub dark_mode: bool,
    pub language: String,

    // Behaviour
    pub init_paused: bool,
    pub monitor_clipboard: bool,
    pub verify_cbc_mac: bool,
    pub verify_download_file: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home_dir = dirs_home();
        let default_download_path = home_dir.join("Downloads");
        Self {
            home_dir,
            default_download_path,
            custom_chunks_dir: None,
            use_custom_chunks_dir: false,
            max_downloads: SIM_TRANSFERENCES_DEFAULT,
            default_slots: WORKERS_DEFAULT,
            use_slots: true,
            chunk_size_multi: CHUNK_SIZE_MULTI,
            limit_download_speed: false,
            max_download_speed_kbps: 0,
            use_proxy: false,
            proxy_host: None,
            proxy_port: 8080,
            proxy_user: None,
            proxy_pass: None,
            use_smart_proxy: false,
            smart_proxy_url: None,
            smart_proxy_ban_time_s: SMART_PROXY_BAN_TIME_S,
            smart_proxy_timeout_ms: SMART_PROXY_TIMEOUT_MS,
            smart_proxy_autorefresh_min: SMART_PROXY_AUTOREFRESH_MIN,
            smart_proxy_random_select: true,
            smart_proxy_reset_slot: true,
            force_smart_proxy: false,
            use_mega_account_down: false,
            mega_account_down_email: None,
            dark_mode: false,
            language: "EN".to_string(),
            init_paused: false,
            monitor_clipboard: true,
            verify_cbc_mac: false,
            verify_download_file: false,
        }
    }
}

impl AppConfig {
    /// Load config from the SQLite settings table.
    pub fn load_from_db(db: &Database, home_dir: PathBuf) -> Result<Self> {
        let settings = db.get_all_settings()?;
        let mut cfg = Self::default();
        cfg.home_dir = home_dir.clone();

        macro_rules! load_str {
            ($key:expr, $field:expr) => {
                if let Some(v) = settings.get($key) {
                    $field = v.clone();
                }
            };
        }
        macro_rules! load_bool {
            ($key:expr, $field:expr) => {
                if let Some(v) = settings.get($key) {
                    $field = v == "yes" || v == "true" || v == "1";
                }
            };
        }
        macro_rules! load_opt_str {
            ($key:expr, $field:expr) => {
                if let Some(v) = settings.get($key) {
                    $field = if v.is_empty() { None } else { Some(v.clone()) };
                }
            };
        }
        macro_rules! load_u32 {
            ($key:expr, $field:expr) => {
                if let Some(v) = settings.get($key) {
                    if let Ok(n) = v.parse() {
                        $field = n;
                    }
                }
            };
        }
        macro_rules! load_u16 {
            ($key:expr, $field:expr) => {
                if let Some(v) = settings.get($key) {
                    if let Ok(n) = v.parse() {
                        $field = n;
                    }
                }
            };
        }

        // Download path
        if let Some(v) = settings.get("download_path") {
            if !v.is_empty() {
                cfg.default_download_path = PathBuf::from(v);
            }
        }

        // Custom chunks dir
        if let Some(v) = settings.get("custom_chunks_dir") {
            if !v.is_empty() {
                cfg.custom_chunks_dir = Some(PathBuf::from(v));
            }
        }
        load_bool!("use_custom_chunks_dir", cfg.use_custom_chunks_dir);

        load_u32!("max_downloads", cfg.max_downloads);
        load_u32!("default_slots", cfg.default_slots);
        load_bool!("use_slots", cfg.use_slots);
        load_u32!("chunk_size_multi", cfg.chunk_size_multi);
        load_bool!("limit_download_speed", cfg.limit_download_speed);
        load_u32!("max_download_speed_kbps", cfg.max_download_speed_kbps);

        load_bool!("use_proxy", cfg.use_proxy);
        load_opt_str!("proxy_host", cfg.proxy_host);
        load_u16!("proxy_port", cfg.proxy_port);
        load_opt_str!("proxy_user", cfg.proxy_user);
        load_opt_str!("proxy_pass", cfg.proxy_pass);

        load_bool!("use_smart_proxy", cfg.use_smart_proxy);
        load_opt_str!("smart_proxy_url", cfg.smart_proxy_url);
        load_u32!("smartproxy_ban_time", cfg.smart_proxy_ban_time_s);
        load_u32!("smartproxy_timeout", cfg.smart_proxy_timeout_ms);
        load_u32!("smartproxy_autorefresh_time", cfg.smart_proxy_autorefresh_min);
        load_bool!("random_proxy", cfg.smart_proxy_random_select);
        load_bool!("reset_slot_proxy", cfg.smart_proxy_reset_slot);
        load_bool!("force_smart_proxy", cfg.force_smart_proxy);

        load_bool!("use_mega_account_down", cfg.use_mega_account_down);
        load_opt_str!("mega_account_down_email", cfg.mega_account_down_email);

        load_bool!("dark_mode", cfg.dark_mode);
        load_str!("language", cfg.language);

        load_bool!("init_paused", cfg.init_paused);
        load_bool!("clipboardspy", cfg.monitor_clipboard);
        load_bool!("verify_cbc_mac", cfg.verify_cbc_mac);
        load_bool!("verify_download_file", cfg.verify_download_file);

        Ok(cfg)
    }

    /// Save config to the SQLite settings table.
    pub fn save_to_db(&self, db: &Database) -> Result<()> {
        let mut settings: HashMap<String, String> = HashMap::new();

        settings.insert(
            "download_path".to_string(),
            self.default_download_path.to_string_lossy().to_string(),
        );
        settings.insert(
            "custom_chunks_dir".to_string(),
            self.custom_chunks_dir
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        );

        macro_rules! save_bool {
            ($key:expr, $val:expr) => {
                settings.insert($key.to_string(), if $val { "yes" } else { "no" }.to_string());
            };
        }

        save_bool!("use_custom_chunks_dir", self.use_custom_chunks_dir);
        settings.insert("max_downloads".to_string(), self.max_downloads.to_string());
        settings.insert("default_slots".to_string(), self.default_slots.to_string());
        save_bool!("use_slots", self.use_slots);
        settings.insert("chunk_size_multi".to_string(), self.chunk_size_multi.to_string());
        save_bool!("limit_download_speed", self.limit_download_speed);
        settings.insert("max_download_speed_kbps".to_string(), self.max_download_speed_kbps.to_string());
        save_bool!("use_proxy", self.use_proxy);
        settings.insert("proxy_host".to_string(), self.proxy_host.clone().unwrap_or_default());
        settings.insert("proxy_port".to_string(), self.proxy_port.to_string());
        settings.insert("proxy_user".to_string(), self.proxy_user.clone().unwrap_or_default());
        settings.insert("proxy_pass".to_string(), self.proxy_pass.clone().unwrap_or_default());
        save_bool!("use_smart_proxy", self.use_smart_proxy);
        settings.insert("smart_proxy_url".to_string(), self.smart_proxy_url.clone().unwrap_or_default());
        settings.insert("smartproxy_ban_time".to_string(), self.smart_proxy_ban_time_s.to_string());
        settings.insert("smartproxy_timeout".to_string(), self.smart_proxy_timeout_ms.to_string());
        settings.insert("smartproxy_autorefresh_time".to_string(), self.smart_proxy_autorefresh_min.to_string());
        save_bool!("random_proxy", self.smart_proxy_random_select);
        save_bool!("reset_slot_proxy", self.smart_proxy_reset_slot);
        save_bool!("force_smart_proxy", self.force_smart_proxy);
        save_bool!("use_mega_account_down", self.use_mega_account_down);
        settings.insert(
            "mega_account_down_email".to_string(),
            self.mega_account_down_email.clone().unwrap_or_default(),
        );
        save_bool!("dark_mode", self.dark_mode);
        settings.insert("language".to_string(), self.language.clone());
        save_bool!("init_paused", self.init_paused);
        save_bool!("clipboardspy", self.monitor_clipboard);
        save_bool!("verify_cbc_mac", self.verify_cbc_mac);
        save_bool!("verify_download_file", self.verify_download_file);

        db.set_all_settings(&settings)?;
        Ok(())
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Return the MegaBasterd data directory path.
pub fn megabasterd_dir(home_dir: &Path) -> PathBuf {
    home_dir.join(".megabasterd1.0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn test_defaults_match_constants() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.max_downloads, SIM_TRANSFERENCES_DEFAULT);
        assert_eq!(cfg.default_slots, WORKERS_DEFAULT);
        assert_eq!(cfg.chunk_size_multi, CHUNK_SIZE_MULTI);
        assert!(cfg.monitor_clipboard);
        assert!(!cfg.verify_cbc_mac);
        assert_eq!(cfg.language, "EN");
    }

    #[test]
    fn test_save_load_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let home = PathBuf::from("/tmp/test_home");

        let mut cfg = AppConfig::default();
        cfg.home_dir = home.clone();
        cfg.dark_mode = true;
        cfg.max_downloads = 8;
        cfg.proxy_host = Some("proxy.example.com".to_string());
        cfg.proxy_port = 3128;

        cfg.save_to_db(&db).unwrap();

        let loaded = AppConfig::load_from_db(&db, home).unwrap();
        assert!(loaded.dark_mode);
        assert_eq!(loaded.max_downloads, 8);
        assert_eq!(loaded.proxy_host, Some("proxy.example.com".to_string()));
        assert_eq!(loaded.proxy_port, 3128);
    }
}
