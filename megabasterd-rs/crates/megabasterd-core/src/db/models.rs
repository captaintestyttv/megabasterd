use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub url: String,
    pub email: Option<String>,
    pub path: String,
    pub filename: String,
    pub filekey: String,
    pub filesize: u64,
    pub filepass: Option<String>,
    pub filenoexpire: Option<String>,
    pub custom_chunks_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MegaAccountRecord {
    pub email: String,
    pub password: String,
    pub password_aes: String,
    pub user_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MegaSessionRecord {
    pub email: String,
    pub data: Vec<u8>,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElcAccountRecord {
    pub host: String,
    pub user: String,
    pub apikey: String,
}
