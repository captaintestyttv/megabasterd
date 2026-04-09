/// MEGA link detection and decryption, ported from ClipboardSpy.java and CryptTools.java.
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::crypto::decrypt_mega_downloader_link;

// ---------------------------------------------------------------------------
// Link types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MegaLink {
    /// Public file link: mega.nz/file/ID#KEY or mega.nz/#!ID!KEY
    PublicFile {
        url: String,
        file_id: String,
        key: String,
    },
    /// Public folder link: mega.nz/folder/ID#KEY or mega.nz/#F!ID!KEY
    PublicFolder {
        url: String,
        folder_id: String,
        key: String,
    },
    /// File inside a folder link: mega.nz/folder/FID#KEY/file/NID
    FolderFile {
        url: String,
        folder_id: String,
        folder_key: String,
        file_node_id: String,
    },
    /// MegaCrypter wrapped link
    MegaCrypter { url: String },
    /// Encrypted mega:// link (enc/enc2)
    EncryptedLink { url: String },
}

// ---------------------------------------------------------------------------
// Regex patterns for all MEGA link formats
// ---------------------------------------------------------------------------

static RE_FILE_NEW: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"https?://mega\.nz/file/([A-Za-z0-9_-]+)#([A-Za-z0-9_-]+)").unwrap()
});

static RE_FILE_OLD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"https?://mega\.nz/#!([A-Za-z0-9_-]+)!([A-Za-z0-9_-]+)").unwrap()
});

static RE_FOLDER_NEW: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"https?://mega\.nz/folder/([A-Za-z0-9_-]+)#([A-Za-z0-9_-]+)(?:/file/([A-Za-z0-9_-]+))?").unwrap()
});

static RE_FOLDER_OLD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"https?://mega\.nz/#F!([A-Za-z0-9_-]+)!([A-Za-z0-9_-]+)(?:!([A-Za-z0-9_-]+))?").unwrap()
});

static RE_ENCRYPTED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"mega://enc2?\?[A-Za-z0-9_=-]+").unwrap()
});

static RE_MEGACRYPTER: Lazy<Regex> = Lazy::new(|| {
    // MegaCrypter links look like https://megacrypter.com/... or similar
    Regex::new(r"https?://[a-zA-Z0-9.-]+/[a-zA-Z0-9_-]{20,}").unwrap()
});

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Detect all MEGA links in a block of text.
pub fn detect_mega_links(text: &str) -> Vec<MegaLink> {
    let mut links = Vec::new();

    // Folder links (new format, with optional file node)
    for caps in RE_FOLDER_NEW.captures_iter(text) {
        let url = caps[0].to_string();
        let folder_id = caps[1].to_string();
        let folder_key = caps[2].to_string();
        if let Some(file_node) = caps.get(3) {
            links.push(MegaLink::FolderFile {
                url,
                folder_id,
                folder_key,
                file_node_id: file_node.as_str().to_string(),
            });
        } else {
            links.push(MegaLink::PublicFolder {
                url,
                folder_id,
                key: folder_key,
            });
        }
    }

    // Folder links (old format)
    for caps in RE_FOLDER_OLD.captures_iter(text) {
        let url = caps[0].to_string();
        let folder_id = caps[1].to_string();
        let folder_key = caps[2].to_string();
        if let Some(file_node) = caps.get(3) {
            links.push(MegaLink::FolderFile {
                url,
                folder_id,
                folder_key,
                file_node_id: file_node.as_str().to_string(),
            });
        } else {
            links.push(MegaLink::PublicFolder {
                url,
                folder_id,
                key: folder_key,
            });
        }
    }

    // Public file links (new format)
    for caps in RE_FILE_NEW.captures_iter(text) {
        links.push(MegaLink::PublicFile {
            url: caps[0].to_string(),
            file_id: caps[1].to_string(),
            key: caps[2].to_string(),
        });
    }

    // Public file links (old format)
    for caps in RE_FILE_OLD.captures_iter(text) {
        links.push(MegaLink::PublicFile {
            url: caps[0].to_string(),
            file_id: caps[1].to_string(),
            key: caps[2].to_string(),
        });
    }

    // Encrypted mega:// links
    for m in RE_ENCRYPTED.find_iter(text) {
        links.push(MegaLink::EncryptedLink {
            url: m.as_str().to_string(),
        });
    }

    // Deduplicate by URL
    links.dedup_by(|a, b| link_url(a) == link_url(b));
    links
}

/// Return true if the text contains at least one MEGA link.
pub fn is_mega_link(text: &str) -> bool {
    RE_FILE_NEW.is_match(text)
        || RE_FILE_OLD.is_match(text)
        || RE_FOLDER_NEW.is_match(text)
        || RE_FOLDER_OLD.is_match(text)
        || RE_ENCRYPTED.is_match(text)
}

fn link_url(link: &MegaLink) -> &str {
    match link {
        MegaLink::PublicFile { url, .. } => url,
        MegaLink::PublicFolder { url, .. } => url,
        MegaLink::FolderFile { url, .. } => url,
        MegaLink::MegaCrypter { url } => url,
        MegaLink::EncryptedLink { url } => url,
    }
}

/// Try to decrypt an encrypted mega:// link and return the underlying MEGA URL(s).
pub fn try_decrypt_encrypted_link(link: &MegaLink) -> Option<Vec<MegaLink>> {
    if let MegaLink::EncryptedLink { url } = link {
        match decrypt_mega_downloader_link(url) {
            Ok(decrypted) => {
                let links = detect_mega_links(&decrypted);
                if !links.is_empty() {
                    return Some(links);
                }
            }
            Err(_) => {}
        }
    }
    None
}

/// Serializable info about a detected link, for IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInfo {
    pub url: String,
    pub link_type: String,
    pub file_id: Option<String>,
    pub key: Option<String>,
}

impl From<&MegaLink> for LinkInfo {
    fn from(link: &MegaLink) -> Self {
        match link {
            MegaLink::PublicFile { url, file_id, key } => LinkInfo {
                url: url.clone(),
                link_type: "file".to_string(),
                file_id: Some(file_id.clone()),
                key: Some(key.clone()),
            },
            MegaLink::PublicFolder { url, folder_id, key } => LinkInfo {
                url: url.clone(),
                link_type: "folder".to_string(),
                file_id: Some(folder_id.clone()),
                key: Some(key.clone()),
            },
            MegaLink::FolderFile { url, folder_id, folder_key, file_node_id } => LinkInfo {
                url: url.clone(),
                link_type: "folder_file".to_string(),
                file_id: Some(format!("{}:{}", folder_id, file_node_id)),
                key: Some(folder_key.clone()),
            },
            MegaLink::MegaCrypter { url } => LinkInfo {
                url: url.clone(),
                link_type: "megacrypter".to_string(),
                file_id: None,
                key: None,
            },
            MegaLink::EncryptedLink { url } => LinkInfo {
                url: url.clone(),
                link_type: "encrypted".to_string(),
                file_id: None,
                key: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_new_file_link() {
        let text = "Download from https://mega.nz/file/ABCDE12345#key_here_base64url ok";
        let links = detect_mega_links(text);
        assert_eq!(links.len(), 1);
        assert!(matches!(&links[0], MegaLink::PublicFile { file_id, key, .. }
            if file_id == "ABCDE12345" && key == "key_here_base64url"));
    }

    #[test]
    fn test_detect_old_file_link() {
        let text = "https://mega.nz/#!fileID123!keyABCDEFG456";
        let links = detect_mega_links(text);
        assert_eq!(links.len(), 1);
        assert!(matches!(&links[0], MegaLink::PublicFile { file_id, key, .. }
            if file_id == "fileID123" && key == "keyABCDEFG456"));
    }

    #[test]
    fn test_detect_folder_link() {
        let text = "https://mega.nz/folder/folderXYZ123#folderKeyABC";
        let links = detect_mega_links(text);
        assert_eq!(links.len(), 1);
        assert!(matches!(&links[0], MegaLink::PublicFolder { folder_id, .. }
            if folder_id == "folderXYZ123"));
    }

    #[test]
    fn test_detect_folder_file_link() {
        let text = "https://mega.nz/folder/folderXYZ123#folderKeyABC/file/fileNodeID";
        let links = detect_mega_links(text);
        assert_eq!(links.len(), 1);
        assert!(matches!(&links[0], MegaLink::FolderFile { file_node_id, .. }
            if file_node_id == "fileNodeID"));
    }

    #[test]
    fn test_is_mega_link() {
        assert!(is_mega_link("check https://mega.nz/file/ID123#KEY456 out"));
        assert!(!is_mega_link("nothing here"));
    }

    #[test]
    fn test_detect_encrypted_link() {
        let text = "mega://enc?AABBCCDD1122";
        let links = detect_mega_links(text);
        assert_eq!(links.len(), 1);
        assert!(matches!(&links[0], MegaLink::EncryptedLink { url }
            if url.starts_with("mega://enc?")));
    }

    #[test]
    fn test_no_false_positives() {
        let text = "Visit https://example.com/notamegalink for info";
        assert!(!is_mega_link(text));
    }
}
