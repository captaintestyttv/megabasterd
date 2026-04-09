/// Chunk offset and size calculation, ported from ChunkWriterManager.java.
/// These formulas must be bit-identical to the Java implementation.

/// Fixed offsets (in KB) for chunks 1-7.
/// Java: ChunkWriterManager.java lines 38-41 — 0-indexed, chunk N uses index N-1.
const CHUNK_OFFSETS_KB: [u64; 7] = [0, 128, 384, 768, 1280, 1920, 2688];

/// Calculate the byte offset for a given chunk ID.
/// chunk_id is 1-based.
/// Java: calculateChunkOffset
pub fn calculate_chunk_offset(chunk_id: u64, size_multi: u32) -> u64 {
    if chunk_id <= 7 {
        CHUNK_OFFSETS_KB[(chunk_id as usize) - 1] * 1024
    } else {
        // chunk 8+: 3584 KB + (chunk_id - 8) * 1024 KB * size_multi
        (3584 + (chunk_id - 8) * 1024 * size_multi as u64) * 1024
    }
}

/// Calculate the size in bytes of a given chunk.
/// chunk_id is 1-based.
/// Java: calculateChunkSize
pub fn calculate_chunk_size(chunk_id: u64, file_size: u64, size_multi: u32) -> u64 {
    let offset = calculate_chunk_offset(chunk_id, size_multi);
    if offset >= file_size {
        return 0;
    }
    let raw_size = if chunk_id <= 7 {
        chunk_id * 128 * 1024
    } else {
        1024 * 1024 * size_multi as u64
    };
    // Clamp to file_size
    raw_size.min(file_size - offset)
}

/// Generate the HTTP range URL for downloading a chunk.
/// Java: file_url + "/" + offset or file_url + "/" + offset + "-" + (offset+size-1)
pub fn gen_chunk_url(file_url: &str, offset: u64, chunk_size: u64) -> String {
    if chunk_size == 0 {
        format!("{}/{}", file_url, offset)
    } else {
        let end = offset + chunk_size - 1;
        format!("{}/{}-{}", file_url, offset, end)
    }
}

/// Determine how many chunks have been fully written based on current file size.
/// Java: calculateLastWrittenChunk
pub fn calculate_last_written_chunk(
    current_bytes: u64,
    size_multi: u32,
) -> u64 {
    if current_bytes == 0 {
        return 0;
    }
    let mut chunk_id = 1u64;
    loop {
        let offset = calculate_chunk_offset(chunk_id, size_multi);
        let size = calculate_chunk_size(chunk_id, u64::MAX, size_multi);
        if offset + size > current_bytes {
            // This chunk is only partially written; last fully written is chunk_id - 1
            return chunk_id.saturating_sub(1);
        }
        if offset + size == current_bytes {
            return chunk_id;
        }
        chunk_id += 1;
        if chunk_id > 100_000 {
            break; // safety
        }
    }
    0
}

/// Check whether a chunk_id is valid for a given file size.
pub fn is_valid_chunk_id(chunk_id: u64, file_size: u64, size_multi: u32) -> bool {
    let offset = calculate_chunk_offset(chunk_id, size_multi);
    offset < file_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_offsets() {
        // Java constants verification
        assert_eq!(calculate_chunk_offset(1, 20), 0);
        assert_eq!(calculate_chunk_offset(2, 20), 128 * 1024);
        assert_eq!(calculate_chunk_offset(3, 20), 384 * 1024);
        assert_eq!(calculate_chunk_offset(4, 20), 768 * 1024);
        assert_eq!(calculate_chunk_offset(5, 20), 1280 * 1024);
        assert_eq!(calculate_chunk_offset(6, 20), 1920 * 1024);
        assert_eq!(calculate_chunk_offset(7, 20), 2688 * 1024);
        // Chunk 8: 3584 KB + 0 * 20 MB = 3584 * 1024
        assert_eq!(calculate_chunk_offset(8, 20), 3584 * 1024);
        // Chunk 9: 3584 + 1 * 20*1024 KB = (3584 + 20480) * 1024
        assert_eq!(calculate_chunk_offset(9, 20), (3584 + 20 * 1024) * 1024);
    }

    #[test]
    fn test_chunk_sizes() {
        let file_size = 100 * 1024 * 1024; // 100 MB
        assert_eq!(calculate_chunk_size(1, file_size, 20), 128 * 1024);
        assert_eq!(calculate_chunk_size(2, file_size, 20), 256 * 1024);
        assert_eq!(calculate_chunk_size(7, file_size, 20), 7 * 128 * 1024);
        assert_eq!(calculate_chunk_size(8, file_size, 20), 20 * 1024 * 1024);
    }

    #[test]
    fn test_chunk_size_clamps_to_file_size() {
        // File is exactly 200 KB — chunk 2 would be 256 KB but is clamped
        let file_size = 200 * 1024;
        // Chunk 1: offset=0, size=128KB, fits
        assert_eq!(calculate_chunk_size(1, file_size, 20), 128 * 1024);
        // Chunk 2: offset=128KB, remaining=72KB
        assert_eq!(calculate_chunk_size(2, file_size, 20), 72 * 1024);
        // Chunk 3: offset=384KB, beyond file
        assert_eq!(calculate_chunk_size(3, file_size, 20), 0);
    }

    #[test]
    fn test_gen_chunk_url() {
        let url = "https://g.api.mega.co.nz/dl/ABCDEF";
        assert_eq!(gen_chunk_url(url, 0, 128 * 1024), format!("{}/0-{}", url, 128 * 1024 - 1));
        assert_eq!(gen_chunk_url(url, 128 * 1024, 256 * 1024),
            format!("{}/{}-{}", url, 128 * 1024, 128 * 1024 + 256 * 1024 - 1));
    }
}
