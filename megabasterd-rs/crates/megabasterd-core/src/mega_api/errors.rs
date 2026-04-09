use thiserror::Error;

#[derive(Debug, Error)]
pub enum MegaApiError {
    #[error("MEGA API error code: {0}")]
    ApiError(i32),
    #[error("HTTP error: {0}")]
    HttpError(u16),
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Crypto error: {0}")]
    CryptoError(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Rate limited (509)")]
    BandwidthLimitExceeded,
    #[error("Too many requests (429)")]
    TooManyRequests,
    #[error("Forbidden (403)")]
    Forbidden,
    #[error("Fatal API error: {0}")]
    FatalApiError(i32),
}

/// Check if an API error code is fatal (should not be retried).
pub fn is_fatal_error(code: i32) -> bool {
    matches!(code, -2 | -8 | -14 | -15 | -16 | -17 | 22 | 23 | 24)
}

/// Check if an error code means "no exception needed" (silently ignore).
pub fn is_no_exception_code(code: i32) -> bool {
    matches!(code, -1 | -3)
}
