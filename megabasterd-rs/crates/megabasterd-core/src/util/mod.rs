/// Utility functions ported from MiscTools.java.
/// All byte/encoding helpers that the rest of the codebase depends on.
use base64::{engine::general_purpose, Engine};
use byteorder::{BigEndian, ByteOrder};
use num_bigint::BigUint;
use rand::distributions::Alphanumeric;
use rand::Rng;
use regex::Regex;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// i32 array ↔ byte array (big-endian, matching Java's ByteBuffer.order(BIG_ENDIAN))
// ---------------------------------------------------------------------------

/// Convert a big-endian byte slice to a Vec of i32.
/// Java: bin2i32a
pub fn bin_to_i32a(bin: &[u8]) -> Vec<i32> {
    let len = bin.len() / 4;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        out.push(BigEndian::read_i32(&bin[i * 4..(i + 1) * 4]));
    }
    out
}

/// Convert a slice of i32 to a big-endian byte Vec.
/// Java: i32a2bin
pub fn i32a_to_bin(values: &[i32]) -> Vec<u8> {
    let mut out = vec![0u8; values.len() * 4];
    for (i, &v) in values.iter().enumerate() {
        BigEndian::write_i32(&mut out[i * 4..(i + 1) * 4], v);
    }
    out
}

// ---------------------------------------------------------------------------
// Base64 (URL-safe, no padding — MEGA's format)
// ---------------------------------------------------------------------------

/// Decode a MEGA-style URL-safe base64 string to bytes.
/// Java: UrlBASE642Bin
pub fn url_base64_decode(data: &str) -> Vec<u8> {
    // MEGA uses URL-safe base64 without padding
    general_purpose::URL_SAFE_NO_PAD
        .decode(data)
        .unwrap_or_default()
}

/// Encode bytes to MEGA-style URL-safe base64 (no padding).
/// Java: Bin2UrlBASE64
pub fn url_base64_encode(data: &[u8]) -> String {
    general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Standard base64 decode.
pub fn base64_decode(data: &str) -> Vec<u8> {
    general_purpose::STANDARD
        .decode(data)
        .unwrap_or_default()
}

/// Standard base64 encode.
pub fn base64_encode(data: &[u8]) -> String {
    general_purpose::STANDARD.encode(data)
}

// ---------------------------------------------------------------------------
// Hex encoding
// ---------------------------------------------------------------------------

pub fn hex_to_bin(s: &str) -> Vec<u8> {
    hex::decode(s).unwrap_or_default()
}

pub fn bin_to_hex(data: &[u8]) -> String {
    hex::encode(data)
}

// ---------------------------------------------------------------------------
// MPI (multi-precision integer) → BigUint
// Java: mpi2big
// First 2 bytes = bit length; remaining bytes = big-endian value
// ---------------------------------------------------------------------------
pub fn mpi_to_big(s: &[u8]) -> BigUint {
    if s.len() < 2 {
        return BigUint::from(0u32);
    }
    let bit_len = (BigEndian::read_u16(s) as usize + 7) / 8;
    let start = 2;
    let end = (start + bit_len).min(s.len());
    BigUint::from_bytes_be(&s[start..end])
}

// ---------------------------------------------------------------------------
// Long ↔ byte array (big-endian, 8 bytes)
// Java: long2bytearray
// ---------------------------------------------------------------------------
pub fn long_to_bytearray(val: u64) -> Vec<u8> {
    let mut out = vec![0u8; 8];
    BigEndian::write_u64(&mut out, val);
    out
}

// ---------------------------------------------------------------------------
// Random ID generation
// Java: genID
// ---------------------------------------------------------------------------
pub fn gen_id(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

// ---------------------------------------------------------------------------
// Exponential backoff
// Java: getWaitTimeExpBackOff
// base=2, start=1s, max=8s
// ---------------------------------------------------------------------------
pub const EXP_BACKOFF_BASE: u64 = 2;
pub const EXP_BACKOFF_SECS_RETRY: u64 = 1;
pub const EXP_BACKOFF_MAX_WAIT_TIME: u64 = 8;

pub fn wait_time_exp_backoff(retry_count: u32) -> u64 {
    let wait = EXP_BACKOFF_SECS_RETRY * EXP_BACKOFF_BASE.pow(retry_count);
    wait.min(EXP_BACKOFF_MAX_WAIT_TIME)
}

// ---------------------------------------------------------------------------
// Regex helpers
// Java: findFirstRegex / findAllRegex
// ---------------------------------------------------------------------------

pub fn find_first_regex(pattern: &str, data: &str, group: usize) -> Option<String> {
    let re = Regex::new(pattern).ok()?;
    let caps = re.captures(data)?;
    caps.get(group).map(|m| m.as_str().to_string())
}

pub fn find_all_regex(pattern: &str, data: &str, group: usize) -> Vec<String> {
    let re = match Regex::new(pattern) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    re.captures_iter(data)
        .filter_map(|c| c.get(group).map(|m| m.as_str().to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// SHA-1 hash of a string (used for temp dir naming)
// ---------------------------------------------------------------------------
pub fn sha1_hex(data: &str) -> String {
    // Using SHA-256 truncated to 40 hex chars as a stable content hash
    // (Xuggler-style SHA-1 would require the sha1 crate; SHA-256 is available)
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)[..40].to_string()
}

// ---------------------------------------------------------------------------
// Byte formatting for UI display
// ---------------------------------------------------------------------------
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx < UNITS.len() - 1 {
        value /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.2} {}", value, UNITS[idx])
    }
}

/// Format a duration in seconds to human-readable string.
pub fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Sanitize a filename by removing characters not safe for the filesystem.
pub fn clean_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Array reversal helper (used in some crypto operations)
// Java: recReverseArray
// ---------------------------------------------------------------------------
pub fn rec_reverse_array(arr: &mut [u8], start: usize, end: usize) {
    if start < end {
        arr.swap(start, end);
        if start + 1 < end {
            rec_reverse_array(arr, start + 1, end - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i32a_roundtrip() {
        let original = vec![1i32, -1, 0x12345678, i32::MAX, i32::MIN];
        let bytes = i32a_to_bin(&original);
        let recovered = bin_to_i32a(&bytes);
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_url_base64_roundtrip() {
        let data = b"hello world MEGA test \x00\xFF\xAB";
        let encoded = url_base64_encode(data);
        let decoded = url_base64_decode(&encoded);
        assert_eq!(data.to_vec(), decoded);
    }

    #[test]
    fn test_mpi_to_big() {
        // 2-byte bit length of 8 = 1 byte, value 0x05
        let mpi = vec![0x00, 0x08, 0x05];
        let big = mpi_to_big(&mpi);
        assert_eq!(big, BigUint::from(5u32));
    }

    #[test]
    fn test_exp_backoff() {
        assert_eq!(wait_time_exp_backoff(0), 1); // 1 * 2^0 = 1
        assert_eq!(wait_time_exp_backoff(1), 2); // 1 * 2^1 = 2
        assert_eq!(wait_time_exp_backoff(2), 4); // 1 * 2^2 = 4
        assert_eq!(wait_time_exp_backoff(3), 8); // capped at 8
        assert_eq!(wait_time_exp_backoff(4), 8); // still 8
    }

    #[test]
    fn test_find_first_regex() {
        let result = find_first_regex(r"foo(\d+)", "bar foo42 baz", 1);
        assert_eq!(result, Some("42".to_string()));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn test_sha1_hex() {
        // Returns first 40 chars of SHA-256 hex
        let result = sha1_hex("test");
        assert_eq!(result.len(), 40);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
