/// Sequential chunk writer with AES-CTR decryption, ported from ChunkWriterManager.java.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::config::CHUNK_SIZE_MULTI;
use crate::crypto::{aes_ctr_decrypt, forward_mega_link_key_iv};
use crate::download::chunk::{calculate_chunk_offset, calculate_chunk_size};

/// Writes chunks sequentially to the output file, decrypting each with AES-CTR.
/// Java: ChunkWriterManager (ChunkWriterManager.java lines 179-280)
pub async fn chunk_writer_worker(
    file_key: Vec<u8>,
    file_iv: Vec<u8>,
    file_size: u64,
    chunks_dir: PathBuf,
    output_path: PathBuf,
    start_chunk: u64,
    size_multi: u32,
    cancel: CancellationToken,
    notify: Arc<Notify>,
    on_progress: impl Fn(u64) + Send + Sync + 'static,
) -> Result<u64> {
    // Open output file (append mode for resume)
    let mut output_file = if start_chunk > 1 {
        fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(&output_path)
            .await?
    } else {
        fs::File::create(&output_path).await?
    };

    let mut chunk_id = start_chunk.max(1);
    let mut bytes_written = calculate_chunk_offset(chunk_id, size_multi);

    loop {
        if cancel.is_cancelled() {
            break;
        }

        let chunk_path = chunks_dir.join(format!(".chunk{}", chunk_id));

        if !chunk_path.exists() {
            // Wait for the downloader to produce this chunk
            tokio::select! {
                _ = notify.notified() => {
                    // Check again
                    if !chunk_path.exists() {
                        continue;
                    }
                }
                _ = cancel.cancelled() => {
                    break;
                }
                _ = sleep(Duration::from_millis(200)) => {
                    // Periodic check
                    continue;
                }
            }
        }

        let chunk_size = calculate_chunk_size(chunk_id, file_size, size_multi);
        if chunk_size == 0 {
            break; // Done
        }

        // Read chunk data
        let encrypted_data = fs::read(&chunk_path).await?;

        // Forward IV to the current byte offset (CTR counter)
        let iv = forward_mega_link_key_iv(&file_iv, bytes_written);

        // Decrypt with AES-CTR
        let decrypted = aes_ctr_decrypt(&encrypted_data, &file_key, &iv)?;

        // Write to output file
        output_file.write_all(&decrypted).await?;
        bytes_written += decrypted.len() as u64;

        debug!("Wrote chunk {} ({} bytes, total {})", chunk_id, decrypted.len(), bytes_written);

        on_progress(bytes_written);

        // Remove the chunk temp file
        if let Err(e) = fs::remove_file(&chunk_path).await {
            warn!("Failed to remove chunk {}: {}", chunk_id, e);
        }

        if bytes_written >= file_size {
            break; // Complete
        }

        chunk_id += 1;
    }

    output_file.flush().await?;
    Ok(bytes_written)
}
