/// MEGA REST API client, ported from MegaAPI.java.
/// Handles authentication (v1 and v2), session management, file metadata, and download URLs.
pub mod errors;
pub mod types;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use num_bigint::BigUint;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::config::{
    DEFAULT_USER_AGENT, FATAL_API_ERROR_CODES, HTTP_CONNECT_TIMEOUT_MS,
    HTTP_READ_TIMEOUT_MS, MEGA_ERROR_NO_EXCEPTION_CODES,
};
use crate::crypto::{
    aes_cbc_decrypt_nopadding, decrypt_key, init_mega_link_key, mega_prepare_master_key,
    mega_user_hash, pbkdf2_hmac_sha512, rsa_decrypt, AES_ZERO_IV,
};
use crate::proxy::{ProxyConfig, SmartProxyManager};
use crate::util::{
    bin_to_i32a, find_first_regex, gen_id, i32a_to_bin, mpi_to_big, url_base64_decode,
    url_base64_encode, wait_time_exp_backoff,
};

use errors::{is_fatal_error, is_no_exception_code, MegaApiError};
pub use types::{FileMetadata, FolderNode, MegaSession, RsaPrivKey};

/// API endpoint — MegaAPI.java line 51
pub const API_URL: &str = "https://g.api.mega.co.nz";
pub const REQ_ID_LENGTH: usize = 10;
pub const PBKDF2_ITERATIONS: u32 = 100_000;
pub const PBKDF2_OUTPUT_BIT_LENGTH: u32 = 256;

pub struct MegaApiClient {
    http: Client,
    seqno: AtomicU64,
    req_id: String,
    pub session: Option<MegaSession>,
    proxy_manager: Option<Arc<SmartProxyManager>>,
    manual_proxy: Option<ProxyConfig>,
}

impl MegaApiClient {
    pub fn new() -> Result<Self> {
        let client = Self::build_client(None)?;
        Ok(Self {
            http: client,
            seqno: AtomicU64::new(rand::random::<u32>() as u64),
            req_id: gen_id(REQ_ID_LENGTH),
            session: None,
            proxy_manager: None,
            manual_proxy: None,
        })
    }

    pub fn with_proxy(proxy: ProxyConfig) -> Result<Self> {
        let client = Self::build_client(Some(&proxy))?;
        Ok(Self {
            http: client,
            seqno: AtomicU64::new(rand::random::<u32>() as u64),
            req_id: gen_id(REQ_ID_LENGTH),
            session: None,
            proxy_manager: None,
            manual_proxy: Some(proxy),
        })
    }

    fn build_client(proxy: Option<&ProxyConfig>) -> Result<Client> {
        let mut builder = Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .connect_timeout(Duration::from_millis(HTTP_CONNECT_TIMEOUT_MS))
            .timeout(Duration::from_millis(HTTP_READ_TIMEOUT_MS))
            .gzip(true);

        if let Some(p) = proxy {
            builder = builder.proxy(
                SmartProxyManager::build_reqwest_proxy(&format!("{}:{}", p.host, p.port), &p.proxy_type)?
            );
        }

        Ok(builder.build()?)
    }

    fn next_seqno(&self) -> u64 {
        self.seqno.fetch_add(1, Ordering::Relaxed)
    }

    fn build_api_url(&self, include_sid: bool) -> String {
        let base = format!("{}/cs?id={}&app=megabasterd", API_URL, self.next_seqno());
        if include_sid {
            if let Some(sid) = self.session.as_ref().and_then(|s| s.sid.as_ref()) {
                return format!("{}&sid={}", base, sid);
            }
        }
        base
    }

    // ---------------------------------------------------------------------------
    // Raw HTTP request with retry logic (port of RAW_REQUEST in MegaAPI.java)
    // ---------------------------------------------------------------------------

    pub async fn raw_request(&self, request: &str, url: &str) -> Result<String, MegaApiError> {
        let mut retry_count = 0u32;

        loop {
            let response = self.http
                .post(url)
                .header("Content-Type", "application/json")
                .body(request.to_string())
                .send()
                .await;

            match response {
                Err(e) => {
                    warn!("Network error (retry {}): {}", retry_count, e);
                    if retry_count >= 10 {
                        return Err(MegaApiError::NetworkError(e));
                    }
                    let wait = wait_time_exp_backoff(retry_count);
                    sleep(Duration::from_secs(wait)).await;
                    retry_count += 1;
                    continue;
                }
                Ok(resp) => {
                    let status = resp.status().as_u16();

                    match status {
                        509 => return Err(MegaApiError::BandwidthLimitExceeded),
                        429 => {
                            if retry_count >= 5 {
                                return Err(MegaApiError::TooManyRequests);
                            }
                            let wait = wait_time_exp_backoff(retry_count);
                            sleep(Duration::from_secs(wait)).await;
                            retry_count += 1;
                            continue;
                        }
                        403 => return Err(MegaApiError::Forbidden),
                        200 | 201 => {}
                        code => {
                            warn!("Unexpected HTTP status: {}", code);
                            if retry_count >= 5 {
                                return Err(MegaApiError::HttpError(status));
                            }
                            retry_count += 1;
                            continue;
                        }
                    }

                    let body = resp.text().await.map_err(MegaApiError::NetworkError)?;

                    if body.is_empty() {
                        warn!("Empty response (retry {})", retry_count);
                        if retry_count >= 5 {
                            return Err(MegaApiError::InvalidResponse("Empty body".into()));
                        }
                        retry_count += 1;
                        continue;
                    }

                    // Check for MEGA API error code: response is just a number like [-3] or -3
                    let trimmed = body.trim().trim_start_matches('[').trim_end_matches(']');
                    if let Ok(code) = trimmed.parse::<i32>() {
                        if code < 0 {
                            if is_fatal_error(code) {
                                return Err(MegaApiError::FatalApiError(code));
                            }
                            if is_no_exception_code(code) {
                                // -1 = EAGAIN — retry
                                let wait = wait_time_exp_backoff(retry_count);
                                sleep(Duration::from_secs(wait)).await;
                                retry_count += 1;
                                continue;
                            }
                            return Err(MegaApiError::ApiError(code));
                        }
                    }

                    return Ok(body);
                }
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Authentication
    // ---------------------------------------------------------------------------

    /// Check account version and salt (us0 request).
    async fn read_account_version_and_salt(&self, email: &str) -> Result<(i32, Option<String>), MegaApiError> {
        let request = json!([{"a": "us0", "user": email}]).to_string();
        let url = format!("{}/cs?id={}", API_URL, self.next_seqno());
        let body = self.raw_request(&request, &url).await?;
        let parsed: Vec<Value> = serde_json::from_str(&body)?;
        let obj = parsed.first().ok_or(MegaApiError::InvalidResponse("Empty us0 response".into()))?;
        let version = obj.get("v").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
        let salt = obj.get("s").and_then(|v| v.as_str()).map(String::from);
        Ok((version, salt))
    }

    pub async fn check_2fa(&self, email: &str) -> Result<bool, MegaApiError> {
        let request = json!([{"a": "mfag", "e": email}]).to_string();
        let url = format!("{}/cs?id={}", API_URL, self.next_seqno());
        let body = self.raw_request(&request, &url).await?;
        let parsed: Vec<i64> = serde_json::from_str(&body)?;
        Ok(parsed.first().copied().unwrap_or(0) == 1)
    }

    /// Full login — sets session on self.
    pub async fn login(
        &mut self,
        email: &str,
        password: &str,
        pincode: Option<&str>,
    ) -> Result<(), MegaApiError> {
        let email_clean = email.split('#').next().unwrap_or(email).to_lowercase();

        let (version, salt) = self.read_account_version_and_salt(&email_clean).await?;

        let (password_aes, user_hash) = if version == 1 {
            // v1: legacy key derivation
            let pass_bytes: Vec<i32> = password.bytes()
                .collect::<Vec<u8>>()
                .chunks(4)
                .map(|c| {
                    let mut buf = [0i32; 1];
                    for (i, &b) in c.iter().enumerate() {
                        buf[0] |= (b as i32) << ((3 - i) * 8);
                    }
                    buf[0]
                })
                .collect();
            let key = mega_prepare_master_key(&pass_bytes)
                .map_err(|e| MegaApiError::CryptoError(e.to_string()))?;
            let hash = mega_user_hash(email_clean.as_bytes(), &key)
                .map_err(|e| MegaApiError::CryptoError(e.to_string()))?;
            (key, hash)
        } else {
            // v2: PBKDF2-HMAC-SHA512
            let salt_bytes = url_base64_decode(salt.as_deref().unwrap_or(""));
            let derived = pbkdf2_hmac_sha512(password, &salt_bytes, PBKDF2_ITERATIONS, PBKDF2_OUTPUT_BIT_LENGTH)
                .map_err(|e| MegaApiError::CryptoError(e.to_string()))?;
            let pass_aes = bin_to_i32a(&derived[..16]);
            let user_hash = url_base64_encode(&derived[16..32]);
            (pass_aes, user_hash)
        };

        self._real_login(email, &email_clean, password_aes, user_hash, version, salt, pincode).await
    }

    async fn _real_login(
        &mut self,
        full_email: &str,
        email: &str,
        password_aes: Vec<i32>,
        user_hash: String,
        version: i32,
        salt: Option<String>,
        pincode: Option<&str>,
    ) -> Result<(), MegaApiError> {
        let request = if let Some(pin) = pincode {
            json!([{"a": "us", "mfa": pin, "user": email, "uh": user_hash}])
        } else {
            json!([{"a": "us", "user": email, "uh": user_hash}])
        };

        let url = format!("{}/cs?id={}", API_URL, self.next_seqno());
        let body = self.raw_request(&request.to_string(), &url).await?;
        let parsed: Vec<Value> = serde_json::from_str(&body)?;
        let obj = parsed.first().ok_or(MegaApiError::InvalidResponse("Empty us response".into()))?;

        // Decrypt master key: k field is AES-ECB encrypted with password_aes
        let k_b64 = obj.get("k").and_then(|v| v.as_str())
            .ok_or(MegaApiError::InvalidResponse("Missing k field".into()))?;
        let k_bytes = url_base64_decode(k_b64);
        let pass_key_bytes = i32a_to_bin(&password_aes);
        let master_key_bytes = decrypt_key(&k_bytes, &pass_key_bytes)
            .map_err(|e| MegaApiError::CryptoError(e.to_string()))?;
        let master_key = bin_to_i32a(&master_key_bytes);

        // Extract session ID from csid (RSA encrypted)
        let sid = if let Some(csid) = obj.get("csid").and_then(|v| v.as_str()) {
            let privk = obj.get("privk").and_then(|v| v.as_str())
                .ok_or(MegaApiError::InvalidResponse("Missing privk".into()))?;

            let enc_privk_bytes = url_base64_decode(privk);
            let enc_privk_i32 = bin_to_i32a(&enc_privk_bytes);
            let master_key_bytes = i32a_to_bin(&master_key);
            let privk_bytes = decrypt_key(&i32a_to_bin(&enc_privk_i32), &master_key_bytes)
                .map_err(|e| MegaApiError::CryptoError(e.to_string()))?;

            let rsa_priv_key = extract_rsa_priv_key(&privk_bytes);
            let csid_bytes = url_base64_decode(csid);
            let csid_big = mpi_to_big(&csid_bytes);
            let raw_sid = rsa_decrypt(&csid_big, &rsa_priv_key.p, &rsa_priv_key.q, &rsa_priv_key.d);

            // First 43 bytes as url-base64
            let sid_len = 43.min(raw_sid.len());
            Some(url_base64_encode(&raw_sid[..sid_len]))
        } else {
            None
        };

        self.session = Some(MegaSession {
            email: email.to_string(),
            full_email: full_email.to_string(),
            sid,
            master_key,
            password_aes,
            user_hash,
            root_id: None,
            inbox_id: None,
            trashbin_id: None,
            account_version: version,
            salt,
        });

        // Fetch nodes to populate root/inbox/trash IDs
        if let Err(e) = self.fetch_nodes().await {
            warn!("fetch_nodes failed after login: {}", e);
        }

        Ok(())
    }

    async fn fetch_nodes(&mut self) -> Result<(), MegaApiError> {
        let request = json!([{"a": "f", "c": 1}]).to_string();
        let url = self.build_api_url(true);
        let body = self.raw_request(&request, &url).await?;
        let parsed: Vec<Value> = serde_json::from_str(&body)?;
        if let Some(obj) = parsed.first() {
            if let Some(nodes) = obj.get("f").and_then(|v| v.as_array()) {
                if let Some(session) = &mut self.session {
                    for node in nodes {
                        let t = node.get("t").and_then(|v| v.as_i64()).unwrap_or(-1);
                        let h = node.get("h").and_then(|v| v.as_str()).map(String::from);
                        match t {
                            2 => session.root_id = h,
                            3 => session.inbox_id = h,
                            4 => session.trashbin_id = h,
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // File metadata and download URL
    // ---------------------------------------------------------------------------

    /// Get file metadata AND download URL in a single API call (more efficient).
    /// Uses `{"a":"g","g":1,"p":file_id}` which returns size, attributes, and URL together.
    pub async fn get_file_info_and_url(
        &self,
        file_id: &str,
        file_key: &str,
    ) -> Result<(FileMetadata, String), MegaApiError> {
        let request = json!([{"a": "g", "g": 1, "p": file_id}]).to_string();
        let url = self.build_api_url(self.session.is_some());
        let body = self.raw_request(&request, &url).await?;
        let parsed: Vec<Value> = serde_json::from_str(&body)?;
        let obj = parsed.first().ok_or(MegaApiError::InvalidResponse("Empty g response".into()))?;

        let size = obj.get("s")
            .and_then(|v| v.as_u64())
            .ok_or(MegaApiError::InvalidResponse("Missing size in g response".into()))?;

        let name = if let Some(at) = obj.get("at").and_then(|v| v.as_str()) {
            decrypt_file_attributes(at, file_key).unwrap_or_else(|_| "unknown".to_string())
        } else {
            "unknown".to_string()
        };

        let download_url = obj.get("g")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or(MegaApiError::InvalidResponse("Missing download URL in g response".into()))?;

        Ok((FileMetadata { name, size, key: file_key.to_string() }, download_url))
    }

    /// Get file metadata (name, size) for a public MEGA link.
    pub async fn get_file_metadata(
        &self,
        file_id: &str,
        file_key: &str,
    ) -> Result<FileMetadata, MegaApiError> {
        let request = json!([{"a": "g", "p": file_id}]).to_string();
        let url = self.build_api_url(self.session.is_some());
        let body = self.raw_request(&request, &url).await?;
        let parsed: Vec<Value> = serde_json::from_str(&body)?;
        let obj = parsed.first().ok_or(MegaApiError::InvalidResponse("Empty g response".into()))?;

        let size = obj.get("s")
            .and_then(|v| v.as_u64())
            .ok_or(MegaApiError::InvalidResponse("Missing size".into()))?;

        // Decrypt file attributes to get the name
        let name = if let Some(at) = obj.get("at").and_then(|v| v.as_str()) {
            decrypt_file_attributes(at, file_key)
                .unwrap_or_else(|_| "unknown".to_string())
        } else {
            "unknown".to_string()
        };

        Ok(FileMetadata {
            name,
            size,
            key: file_key.to_string(),
        })
    }

    /// Get the download URL for a file.
    pub async fn get_download_url(&self, file_id: &str) -> Result<String, MegaApiError> {
        let request = json!([{"a": "g", "g": 1, "p": file_id}]).to_string();
        let url = self.build_api_url(self.session.is_some());
        let body = self.raw_request(&request, &url).await?;
        let parsed: Vec<Value> = serde_json::from_str(&body)?;
        let obj = parsed.first().ok_or(MegaApiError::InvalidResponse("Empty g response".into()))?;

        obj.get("g")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or(MegaApiError::InvalidResponse("Missing download URL".into()))
    }

    /// Get download URL for a file inside a folder.
    pub async fn get_folder_file_download_url(
        &self,
        folder_id: &str,
        file_node_id: &str,
        folder_key: &str,
    ) -> Result<String, MegaApiError> {
        let request = json!([{"a": "g", "g": 1, "n": file_node_id}]).to_string();
        let url = format!("{}/cs?id={}&n={}", API_URL, self.next_seqno(), folder_id);
        let body = self.raw_request(&request, &url).await?;
        let parsed: Vec<Value> = serde_json::from_str(&body)?;
        let obj = parsed.first().ok_or(MegaApiError::InvalidResponse("Empty g response".into()))?;

        obj.get("g")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or(MegaApiError::InvalidResponse("Missing download URL".into()))
    }

    /// Get all nodes in a public folder.
    pub async fn get_folder_nodes(
        &self,
        folder_id: &str,
        folder_key: &str,
    ) -> Result<Vec<FolderNode>, MegaApiError> {
        let request = json!([{"a": "f", "c": 1, "r": 1}]).to_string();
        let url = format!("{}/cs?id={}&n={}", API_URL, self.next_seqno(), folder_id);
        let body = self.raw_request(&request, &url).await?;
        let parsed: Vec<Value> = serde_json::from_str(&body)?;
        let obj = parsed.first().ok_or(MegaApiError::InvalidResponse("Empty folder response".into()))?;

        let mut nodes = Vec::new();
        let folder_key_bytes = init_mega_link_key(folder_key);

        if let Some(arr) = obj.get("f").and_then(|v| v.as_array()) {
            for node in arr {
                let handle = node.get("h").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let parent = node.get("p").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let node_type = node.get("t").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let size = node.get("s").and_then(|v| v.as_u64());

                let (name, key) = if let Some(k) = node.get("k").and_then(|v| v.as_str()) {
                    // Key is "handle:encrypted_key" — decrypt with folder key
                    let node_key = k.split(':').last().unwrap_or(k);
                    let name = if let Some(at) = node.get("a").and_then(|v| v.as_str()) {
                        decrypt_folder_node_attributes(at, node_key, &folder_key_bytes).ok()
                    } else {
                        None
                    };
                    (name, Some(node_key.to_string()))
                } else {
                    (None, None)
                };

                nodes.push(FolderNode {
                    handle,
                    parent,
                    name,
                    node_type,
                    size,
                    key,
                });
            }
        }

        Ok(nodes)
    }
}

// ---------------------------------------------------------------------------
// RSA private key extraction
// Java: _extractRSAPrivKey — reads MPI-encoded (p, q, d, u) from privk_byte
// ---------------------------------------------------------------------------

fn extract_rsa_priv_key(privk_bytes: &[u8]) -> RsaPrivKey {
    let mut offset = 0;

    let read_mpi = |data: &[u8], pos: &mut usize| -> BigUint {
        if *pos + 2 > data.len() {
            return BigUint::from(0u32);
        }
        let bit_len = ((data[*pos] as usize) << 8) | (data[*pos + 1] as usize);
        let byte_len = (bit_len + 7) / 8;
        *pos += 2;
        if *pos + byte_len > data.len() {
            return BigUint::from(0u32);
        }
        let val = BigUint::from_bytes_be(&data[*pos..*pos + byte_len]);
        *pos += byte_len;
        val
    };

    let p = read_mpi(privk_bytes, &mut offset);
    let q = read_mpi(privk_bytes, &mut offset);
    let d = read_mpi(privk_bytes, &mut offset);
    let u = read_mpi(privk_bytes, &mut offset);

    RsaPrivKey { p, q, d, u }
}

// ---------------------------------------------------------------------------
// File attribute decryption
// Java: _decAttr — AES-CBC decrypt, strip null + "MEGA" prefix, parse JSON
// ---------------------------------------------------------------------------

fn decrypt_file_attributes(at_b64: &str, file_key: &str) -> Result<String, Box<dyn std::error::Error>> {
    let key = init_mega_link_key(file_key);
    let at_bytes = url_base64_decode(at_b64);

    // Pad to 16-byte boundary
    let padded_len = ((at_bytes.len() + 15) / 16) * 16;
    let mut padded = at_bytes.clone();
    padded.resize(padded_len, 0);

    let decrypted = aes_cbc_decrypt_nopadding(&padded, &key, &AES_ZERO_IV)?;

    // Strip null bytes
    let end = decrypted.iter().rposition(|&b| b != 0).map(|i| i + 1).unwrap_or(decrypted.len());
    let text = std::str::from_utf8(&decrypted[..end])?;

    // Strip "MEGA" prefix
    let json_str = text.trim_start_matches("MEGA").trim();
    let obj: Value = serde_json::from_str(json_str)?;

    Ok(obj.get("n")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string())
}

fn decrypt_folder_node_attributes(
    at_b64: &str,
    node_key: &str,
    folder_key_bytes: &[u8],
) -> Result<String, Box<dyn std::error::Error>> {
    let node_key_bytes = {
        let raw = url_base64_decode(node_key);
        let ints = bin_to_i32a(&raw);
        if ints.len() >= 8 {
            // XOR-fold
            let folded: Vec<i32> = (0..4).map(|i| ints[i] ^ ints[i + 4]).collect();
            i32a_to_bin(&folded)
        } else {
            raw[..16.min(raw.len())].to_vec()
        }
    };

    let at_bytes = url_base64_decode(at_b64);
    let padded_len = ((at_bytes.len() + 15) / 16) * 16;
    let mut padded = at_bytes;
    padded.resize(padded_len, 0);

    let decrypted = aes_cbc_decrypt_nopadding(&padded, &node_key_bytes, &AES_ZERO_IV)?;
    let end = decrypted.iter().rposition(|&b| b != 0).map(|i| i + 1).unwrap_or(decrypted.len());
    let text = std::str::from_utf8(&decrypted[..end])?;
    let json_str = text.trim_start_matches("MEGA").trim();
    let obj: Value = serde_json::from_str(json_str)?;

    Ok(obj.get("n")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string())
}
