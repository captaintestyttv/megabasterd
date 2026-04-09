/// Cryptographic operations, ported from CryptTools.java.
/// Must produce bit-identical output to the Java implementation for all MEGA operations.
use aes::Aes128;
use cbc::{Decryptor as CbcDecryptor, Encryptor as CbcEncryptor};
use cipher::{
    block_padding::{NoPadding, Pkcs7},
    BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit, StreamCipher,
};
use ctr::Ctr128BE;
use ecb::{Decryptor as EcbDecryptor, Encryptor as EcbEncryptor};
use hmac::Hmac;
use num_bigint::BigUint;
use pbkdf2::pbkdf2;
use sha2::{Sha256, Sha512};
use thiserror::Error;

use crate::util::{bin_to_i32a, i32a_to_bin, long_to_bytearray, url_base64_decode, url_base64_encode};

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength { expected: usize, got: usize },
    #[error("Invalid data length (not a multiple of block size)")]
    InvalidDataLength,
    #[error("Decryption failed")]
    DecryptionFailed,
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

type Result<T> = std::result::Result<T, CryptoError>;

// AES block size is always 16 bytes
const BLOCK_SIZE: usize = 16;

/// All-zero IV used in various MEGA operations.
pub const AES_ZERO_IV: [u8; 16] = [0u8; 16];

// ---------------------------------------------------------------------------
// AES-CBC (no padding)
// ---------------------------------------------------------------------------

pub fn aes_cbc_encrypt_nopadding(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
    if data.len() % BLOCK_SIZE != 0 {
        return Err(CryptoError::InvalidDataLength);
    }
    let mut buf = data.to_vec();
    CbcEncryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKeyLength { expected: 16, got: key.len() })?
        .encrypt_padded_mut::<NoPadding>(&mut buf, data.len())
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(buf)
}

pub fn aes_cbc_decrypt_nopadding(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
    if data.len() % BLOCK_SIZE != 0 {
        return Err(CryptoError::InvalidDataLength);
    }
    let mut buf = data.to_vec();
    CbcDecryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKeyLength { expected: 16, got: key.len() })?
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// AES-CBC (PKCS7 padding)
// ---------------------------------------------------------------------------

pub fn aes_cbc_encrypt_pkcs7(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
    let padded_len = (data.len() / BLOCK_SIZE + 1) * BLOCK_SIZE;
    let mut buf = vec![0u8; padded_len];
    buf[..data.len()].copy_from_slice(data);
    let result = CbcEncryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKeyLength { expected: 16, got: key.len() })?
        .encrypt_padded_mut::<Pkcs7>(&mut buf, data.len())
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(result.to_vec())
}

pub fn aes_cbc_decrypt_pkcs7(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
    let mut buf = data.to_vec();
    let result = CbcDecryptor::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKeyLength { expected: 16, got: key.len() })?
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(result.to_vec())
}

// ---------------------------------------------------------------------------
// AES-ECB (no padding)
// ---------------------------------------------------------------------------

pub fn aes_ecb_encrypt_nopadding(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    if data.len() % BLOCK_SIZE != 0 {
        return Err(CryptoError::InvalidDataLength);
    }
    if key.len() != 16 {
        return Err(CryptoError::InvalidKeyLength { expected: 16, got: key.len() });
    }
    let mut buf = data.to_vec();
    let cipher = EcbEncryptor::<Aes128>::new(key.into());
    cipher.encrypt_padded_mut::<NoPadding>(&mut buf, data.len())
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(buf)
}

pub fn aes_ecb_decrypt_nopadding(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    if data.len() % BLOCK_SIZE != 0 {
        return Err(CryptoError::InvalidDataLength);
    }
    if key.len() != 16 {
        return Err(CryptoError::InvalidKeyLength { expected: 16, got: key.len() });
    }
    let mut buf = data.to_vec();
    let cipher = EcbDecryptor::<Aes128>::new(key.into());
    cipher.decrypt_padded_mut::<NoPadding>(&mut buf)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// AES-CTR (for MEGA file content decryption)
// Java: AES/CTR/NoPadding — Ctr128BE matches Java's behaviour
// ---------------------------------------------------------------------------

pub fn aes_ctr_decrypt(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
    let mut buf = data.to_vec();
    let mut cipher = Ctr128BE::<Aes128>::new_from_slices(key, iv)
        .map_err(|_| CryptoError::InvalidKeyLength { expected: 16, got: key.len() })?;
    cipher.apply_keystream(&mut buf);
    Ok(buf)
}

/// AES-CTR encrypt is identical to decrypt (XOR-based stream cipher).
pub fn aes_ctr_encrypt(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
    aes_ctr_decrypt(data, key, iv)
}

// ---------------------------------------------------------------------------
// RSA decryption
// Java: rsaDecrypt(enc_data, p, q, d)
// Uses raw modular exponentiation: enc_data^d mod (p*q)
// ---------------------------------------------------------------------------

pub fn rsa_decrypt(enc_data: &BigUint, p: &BigUint, q: &BigUint, d: &BigUint) -> Vec<u8> {
    let n = p * q;
    let result = enc_data.modpow(d, &n);
    result.to_bytes_be()
}

// ---------------------------------------------------------------------------
// PBKDF2
// ---------------------------------------------------------------------------

/// PBKDF2-HMAC-SHA512 key derivation.
/// Java: PBKDF2HMACSHA512(password, salt, iterations, output_bit_length)
pub fn pbkdf2_hmac_sha512(
    password: &str,
    salt: &[u8],
    iterations: u32,
    output_bits: u32,
) -> Result<Vec<u8>> {
    let output_bytes = (output_bits / 8) as usize;
    let mut key = vec![0u8; output_bytes];
    pbkdf2::<Hmac<Sha512>>(password.as_bytes(), salt, iterations, &mut key)
        .map_err(|_| CryptoError::InvalidInput("PBKDF2-SHA512 failed".into()))?;
    Ok(key)
}

/// PBKDF2-HMAC-SHA256 key derivation (used by MegaCrypter).
pub fn pbkdf2_hmac_sha256(
    password: &str,
    salt: &[u8],
    iterations: u32,
    output_bits: u32,
) -> Result<Vec<u8>> {
    let output_bytes = (output_bits / 8) as usize;
    let mut key = vec![0u8; output_bytes];
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), salt, iterations, &mut key)
        .map_err(|_| CryptoError::InvalidInput("PBKDF2-SHA256 failed".into()))?;
    Ok(key)
}

// ---------------------------------------------------------------------------
// MEGA-specific key operations
// ---------------------------------------------------------------------------

/// Extract the 16-byte AES key from a MEGA file key string.
/// Java: initMEGALinkKey — XOR-folds 8 i32s into 4: k[i] = parts[i] ^ parts[i+4]
pub fn init_mega_link_key(key_string: &str) -> Vec<u8> {
    let raw = url_base64_decode(key_string);
    let int_key = bin_to_i32a(&raw);
    if int_key.len() < 8 {
        return raw[..16.min(raw.len())].to_vec();
    }
    let folded: Vec<i32> = (0..4).map(|i| int_key[i] ^ int_key[i + 4]).collect();
    i32a_to_bin(&folded)
}

/// Extract the 16-byte IV from a MEGA file key string.
/// Java: initMEGALinkKeyIV — takes elements [4] and [5], pads with two zeros
pub fn init_mega_link_key_iv(key_string: &str) -> Vec<u8> {
    let raw = url_base64_decode(key_string);
    let int_key = bin_to_i32a(&raw);
    if int_key.len() < 6 {
        return vec![0u8; 16];
    }
    let iv_ints = vec![int_key[4], int_key[5], 0i32, 0i32];
    i32a_to_bin(&iv_ints)
}

/// Forward the CTR IV by a given byte count (for resuming mid-file decryption).
/// Java: forwardMEGALinkKeyIV — copies first 8 bytes of IV, then appends counter as big-endian u64
/// Counter = bytes_downloaded / iv.length (where iv.length = 16)
pub fn forward_mega_link_key_iv(iv: &[u8], bytes_downloaded: u64) -> Vec<u8> {
    let mut new_iv = vec![0u8; 16];
    // Copy the first 8 bytes (nonce portion)
    let copy_len = 8.min(iv.len());
    new_iv[..copy_len].copy_from_slice(&iv[..copy_len]);
    // Set counter (bytes_downloaded / 16) in the last 8 bytes, big-endian
    let counter = bytes_downloaded / 16;
    let counter_bytes = long_to_bytearray(counter);
    new_iv[8..16].copy_from_slice(&counter_bytes);
    new_iv
}

/// Compute the MEGA user hash from email bytes and password AES key.
/// Java: MEGAUserHash — XOR-folds email bytes into 4 ints, then AES-CBC encrypts 16384 times.
/// Returns url-base64 of [h32[0], h32[2]] (8 bytes).
pub fn mega_user_hash(email_bytes: &[u8], aes_key: &[i32]) -> Result<String> {
    let key_bytes = i32a_to_bin(aes_key);
    // XOR email bytes into 4-element i32 array (big-endian chunks)
    let mut h32 = [0i32; 4];
    for (i, &b) in email_bytes.iter().enumerate() {
        let idx = (i / 4) % 4;
        let shift = (3 - (i % 4)) * 8;
        h32[idx] ^= (b as i32) << shift;
    }
    let mut h_bytes = i32a_to_bin(&h32);
    // 16384 AES-CBC encryptions with zero IV
    for _ in 0..16384 {
        h_bytes = aes_cbc_encrypt_nopadding(&h_bytes, &key_bytes, &AES_ZERO_IV)?;
    }
    let h32_out = bin_to_i32a(&h_bytes);
    let result_bytes = i32a_to_bin(&[h32_out[0], h32_out[2]]);
    Ok(url_base64_encode(&result_bytes))
}

/// Prepare the master key from a password key (v1 auth).
/// Java: MEGAPrepareMasterKey — magic pkey, 65536 AES-CBC iterations
pub fn mega_prepare_master_key(password_key: &[i32]) -> Result<Vec<i32>> {
    let pkey = [0x93C467E3u32 as i32, 0x7DB0C7A4u32 as i32, 0xD1BE3F81u32 as i32, 0x0152CB56i32];
    let mut pkey_bytes = i32a_to_bin(&pkey);
    let pass_bytes = i32a_to_bin(password_key);
    let chunks = pass_bytes.chunks(16).count();
    // Use each 16-byte chunk of the password as the data to encrypt
    for _ in 0..65536 {
        for chunk in pass_bytes.chunks(16) {
            let padded = if chunk.len() < 16 {
                let mut p = vec![0u8; 16];
                p[..chunk.len()].copy_from_slice(chunk);
                p
            } else {
                chunk.to_vec()
            };
            let encrypted = aes_cbc_encrypt_nopadding(&pkey_bytes, &padded, &AES_ZERO_IV)?;
            pkey_bytes = encrypted;
        }
    }
    Ok(bin_to_i32a(&pkey_bytes))
}

/// Decrypt a key blob using AES-ECB (for decrypting MEGA file/master keys).
/// Java: decryptKey — simply AES-ECB decrypt in 16-byte blocks
pub fn decrypt_key(encrypted: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(encrypted.len());
    for chunk in encrypted.chunks(BLOCK_SIZE) {
        if chunk.len() == BLOCK_SIZE {
            let decrypted = aes_ecb_decrypt_nopadding(chunk, key)?;
            result.extend_from_slice(&decrypted);
        }
    }
    Ok(result)
}

/// Encrypt a key blob using AES-ECB.
pub fn encrypt_key(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(data.len());
    for chunk in data.chunks(BLOCK_SIZE) {
        if chunk.len() == BLOCK_SIZE {
            let encrypted = aes_ecb_encrypt_nopadding(chunk, key)?;
            result.extend_from_slice(&encrypted);
        }
    }
    Ok(result)
}

/// Decrypt MEGA Downloader encrypted links (mega://enc?... and mega://enc2?...).
/// Java: decryptMegaDownloaderLink — two hardcoded AES keys, AES-CBC decrypt
pub fn decrypt_mega_downloader_link(link: &str) -> Result<String> {
    // Hardcoded keys from CryptTools.java line 248
    let key1 = b"the\x7fW\x9f\xbc\x0c\xc9\x86\xf1\xdeam\x8c\x82p\xc5";
    let key2 = b"\x93\xc4g\xe3}\xb0\xc7\xa4\xd1\xbe?\x81\x01R\xcbV";

    let encrypted_part = if link.contains("enc2?") {
        link.split("enc2?").nth(1).unwrap_or("")
    } else if link.contains("enc?") {
        link.split("enc?").nth(1).unwrap_or("")
    } else {
        return Err(CryptoError::InvalidInput("Not a mega://enc link".into()));
    };

    let data = url_base64_decode(encrypted_part);
    if data.is_empty() {
        return Err(CryptoError::InvalidInput("Empty encrypted data".into()));
    }

    // Try key2 first (enc2), then key1 (enc)
    let (key, iv_key) = if link.contains("enc2?") {
        (key2.as_ref(), key1.as_ref())
    } else {
        (key1.as_ref(), key2.as_ref())
    };

    let decrypted = aes_cbc_decrypt_nopadding(&data, key, &AES_ZERO_IV)
        .or_else(|_| aes_cbc_decrypt_nopadding(&data, iv_key, &AES_ZERO_IV))?;

    // Strip null bytes from end
    let end = decrypted.iter().rposition(|&b| b != 0).map(|i| i + 1).unwrap_or(0);
    String::from_utf8(decrypted[..end].to_vec())
        .map_err(|_| CryptoError::InvalidInput("Decrypted data is not valid UTF-8".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_cbc_nopadding_roundtrip() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let data = b"0123456789abcdef"; // exactly 16 bytes
        let encrypted = aes_cbc_encrypt_nopadding(data, &key, &iv).unwrap();
        let decrypted = aes_cbc_decrypt_nopadding(&encrypted, &key, &iv).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_aes_ecb_nopadding_roundtrip() {
        let key = [0x42u8; 16];
        let data = b"abcdefghijklmnop"; // 16 bytes
        let encrypted = aes_ecb_encrypt_nopadding(data, &key).unwrap();
        let decrypted = aes_ecb_decrypt_nopadding(&encrypted, &key).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_aes_ctr_roundtrip() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let data = b"hello world this is a test message";
        let encrypted = aes_ctr_encrypt(data, &key, &iv).unwrap();
        let decrypted = aes_ctr_decrypt(&encrypted, &key, &iv).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_forward_mega_link_key_iv() {
        let iv = vec![0xAAu8, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0, 0, 0, 0, 0, 0, 0, 0];
        // Forward by 3584*1024 bytes (start of chunk 8)
        let bytes = 3584u64 * 1024;
        let new_iv = forward_mega_link_key_iv(&iv, bytes);
        // First 8 bytes preserved
        assert_eq!(&new_iv[..8], &iv[..8]);
        // Counter = bytes / 16 = 229376 = 0x0038000
        let counter = bytes / 16;
        let expected_counter = counter.to_be_bytes();
        assert_eq!(&new_iv[8..], &expected_counter);
    }

    #[test]
    fn test_init_mega_link_key() {
        // 8 i32s (32 bytes) encoded as url-base64
        // parts[0..4] XOR parts[4..8] = key
        use crate::util::url_base64_encode;
        let parts: [i32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        let bytes = crate::util::i32a_to_bin(&parts);
        let encoded = url_base64_encode(&bytes);
        let key = init_mega_link_key(&encoded);
        let key_ints = crate::util::bin_to_i32a(&key);
        assert_eq!(key_ints[0], 1 ^ 5);
        assert_eq!(key_ints[1], 2 ^ 6);
        assert_eq!(key_ints[2], 3 ^ 7);
        assert_eq!(key_ints[3], 4 ^ 8);
    }

    #[test]
    fn test_init_mega_link_key_iv() {
        use crate::util::url_base64_encode;
        let parts: [i32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        let bytes = crate::util::i32a_to_bin(&parts);
        let encoded = url_base64_encode(&bytes);
        let iv = init_mega_link_key_iv(&encoded);
        let iv_ints = crate::util::bin_to_i32a(&iv);
        assert_eq!(iv_ints[0], 5); // parts[4]
        assert_eq!(iv_ints[1], 6); // parts[5]
        assert_eq!(iv_ints[2], 0);
        assert_eq!(iv_ints[3], 0);
    }

    #[test]
    fn test_decrypt_key_roundtrip() {
        let key = [0x13u8; 16];
        let data = b"ABCDEFGHIJKLMNOP"; // 16 bytes
        let encrypted = encrypt_key(data, &key).unwrap();
        let decrypted = decrypt_key(&encrypted, &key).unwrap();
        assert_eq!(decrypted, data);
    }
}
