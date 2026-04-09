use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MegaSession {
    pub email: String,
    pub full_email: String,
    pub sid: Option<String>,
    pub master_key: Vec<i32>,
    pub password_aes: Vec<i32>,
    pub user_hash: String,
    pub root_id: Option<String>,
    pub inbox_id: Option<String>,
    pub trashbin_id: Option<String>,
    pub account_version: i32,
    pub salt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub name: String,
    pub size: u64,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderNode {
    pub handle: String,
    pub parent: String,
    pub name: Option<String>,
    pub node_type: i32, // 0=file, 1=folder, 2=root, 3=inbox, 4=trash
    pub size: Option<u64>,
    pub key: Option<String>,
}

/// RSA private key components extracted from MEGA's privk field.
#[derive(Debug, Clone)]
pub struct RsaPrivKey {
    pub p: BigUint,
    pub q: BigUint,
    pub d: BigUint,
    pub u: BigUint,
}
