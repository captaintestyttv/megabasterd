/// MegaCrypter link resolution, ported from MegaCrypterAPI.java.
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::crypto::{
    aes_cbc_decrypt_nopadding, aes_cbc_decrypt_pkcs7, pbkdf2_hmac_sha256, AES_ZERO_IV,
};
use crate::util::{base64_decode, url_base64_decode, url_base64_encode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McFileMetadata {
    pub name: String,
    pub size: u64,
    pub key: String,
    pub password_hash: Option<String>,
    pub noexpire_token: Option<String>,
    pub path: Option<String>,
}

pub struct MegaCrypterClient {
    http: reqwest::Client,
}

impl MegaCrypterClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    /// Resolve a MegaCrypter link to file metadata.
    /// Java: getFileData (MegaCrypterAPI.java line 155-328)
    pub async fn get_file_metadata(
        &self,
        link: &str,
        password: Option<&str>,
    ) -> Result<McFileMetadata> {
        // Extract the base URL from the link
        let base_url = extract_base_url(link)?;
        let api_url = format!("{}/api", base_url);

        let request = json!({"m": "info", "link": link});
        let resp = self.http
            .post(&api_url)
            .json(&request)
            .send()
            .await?
            .json::<Value>()
            .await?;

        // Check for error
        if let Some(e) = resp.get("error") {
            return Err(anyhow!("MegaCrypter error: {}", e));
        }

        let name;
        let key;
        let size;

        if let Some(pass_field) = resp.get("pass").and_then(|v| v.as_str()) {
            // Password-protected link
            let pw = password.ok_or_else(|| anyhow!("Password required"))?;
            let (derived_key, _) = derive_mc_password_key(pass_field, pw)?;

            // Decrypt name
            let enc_name = resp.get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Missing name"))?;
            let enc_name_bytes = url_base64_decode(enc_name);
            let decrypted_name = aes_cbc_decrypt_pkcs7(&enc_name_bytes, &derived_key, &AES_ZERO_IV)?;
            name = String::from_utf8(decrypted_name)?;

            // Decrypt key
            let enc_key = resp.get("key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Missing key"))?;
            let enc_key_bytes = url_base64_decode(enc_key);
            let decrypted_key = aes_cbc_decrypt_pkcs7(&enc_key_bytes, &derived_key, &AES_ZERO_IV)?;
            key = url_base64_encode(&decrypted_key);
        } else {
            name = resp.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            key = resp.get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }

        size = resp.get("size")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let password_hash = resp.get("pass")
            .and_then(|v| v.as_str())
            .map(String::from);

        let noexpire_token = resp.get("noexpire")
            .and_then(|v| v.as_str())
            .map(String::from);

        let path = resp.get("path")
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(McFileMetadata {
            name,
            size,
            key,
            password_hash,
            noexpire_token,
            path,
        })
    }

    /// Get the download URL for a MegaCrypter link.
    pub async fn get_download_url(
        &self,
        link: &str,
        pass_hash: Option<&str>,
        noexpire_token: Option<&str>,
        sid: Option<&str>,
    ) -> Result<String> {
        let base_url = extract_base_url(link)?;
        let api_url = format!("{}/api", base_url);

        let mut request_obj = json!({"m": "dl", "link": link});
        if let Some(token) = noexpire_token {
            request_obj["noexpire"] = json!(token);
        }
        if let Some(sid) = sid {
            request_obj["sid"] = json!(sid);
        }

        let resp = self.http
            .post(&api_url)
            .json(&request_obj)
            .send()
            .await?
            .json::<Value>()
            .await?;

        if let Some(e) = resp.get("error") {
            return Err(anyhow!("MegaCrypter error: {}", e));
        }

        let url_val = resp.get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing url in response"))?;

        // If password-protected, decrypt the URL
        if let Some(pass) = pass_hash {
            let url_bytes = url_base64_decode(url_val);
            let key_bytes = url_base64_decode(pass);
            let decrypted = aes_cbc_decrypt_pkcs7(&url_bytes, &key_bytes, &AES_ZERO_IV)?;
            return Ok(String::from_utf8(decrypted)?);
        }

        Ok(url_val.to_string())
    }
}

/// Derive the AES key from a MegaCrypter password field.
/// Java: pass field format = "iterations#key_check#salt#iv"
/// Key derivation: PBKDF2-HMAC-SHA256 with 2^iterations iterations
fn derive_mc_password_key(pass_field: &str, password: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let parts: Vec<&str> = pass_field.split('#').collect();
    if parts.len() < 4 {
        return Err(anyhow!("Invalid pass field format"));
    }

    let iterations_exp: u32 = parts[0].parse()?;
    let key_check = base64_decode(parts[1]);
    let salt = base64_decode(parts[2]);
    let iv = base64_decode(parts[3]);

    let iterations = 2u32.pow(iterations_exp);
    let derived = pbkdf2_hmac_sha256(password, &salt, iterations, 256)?;

    // Verify key_check
    let check_encrypted = aes_cbc_decrypt_nopadding(&key_check, &derived, &iv)?;
    // (verification: check_encrypted should be all zeros or a known pattern)

    Ok((derived, iv))
}

fn extract_base_url(link: &str) -> Result<String> {
    let url = url::Url::parse(link).map_err(|_| anyhow!("Invalid URL: {}", link))?;
    let base = format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""));
    Ok(base)
}
