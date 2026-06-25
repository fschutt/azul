use alloc::string::String;

/// Known font file extensions (lowercase).
pub const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "woff", "woff2", "dfont"];

/// Size (in bytes) of the head/tail samples taken by
/// [`content_dedup_hash_u64`]. The full hash spans the file size plus
/// these two samples, so collisions are only possible for files that
/// agree on size *and* both head + tail windows — adequate for
/// deduping the same `.ttc` read under different paths without
/// incurring a full-file walk through mmapped pages.
pub const CONTENT_DEDUP_SAMPLE_BYTES: usize = 4096;

/// Deterministic 64-bit "cheap" content hash derived from
/// `(file_size, first 4 KiB, last 4 KiB)`.
///
/// Same guarantees as [`content_hash_u64`] — stable across process
/// runs, usable for the on-disk font cache — but avoids materialising
/// every page of a multi-megabyte `.ttc` into RSS just to compute a
/// dedup key. Callers typically have the scout's mmap open and have
/// already faulted-in the header tables anyway, so the head sample is
/// free; the tail sample costs at most one extra page fault.
pub fn content_dedup_hash_u64(bytes: &[u8]) -> u64 {
    let len = bytes.len();
    let head_len = len.min(CONTENT_DEDUP_SAMPLE_BYTES);
    let tail_len = (len - head_len).min(CONTENT_DEDUP_SAMPLE_BYTES);
    let tail_start = len - tail_len;
    // Mix size first so two equal head+tail samples with different
    // lengths produce different hashes.
    let mut seed_buf = [0u8; 8];
    seed_buf.copy_from_slice(&(len as u64).to_le_bytes());
    let seed = content_hash_u64(&seed_buf);
    let head = content_hash_u64(&bytes[..head_len]);
    let tail = content_hash_u64(&bytes[tail_start..tail_start + tail_len]);
    // Combine — wrapping_mul + xor avalanches the three sub-hashes
    // reasonably without needing a separate mixing function.
    const K: u64 = 0x9E3779B97F4A7C15;
    let mut h = seed;
    h ^= head;
    h = h.wrapping_mul(K);
    h ^= tail;
    h = h.wrapping_mul(K);
    h ^= h >> 33;
    h
}

/// Deterministic 64-bit content hash over an arbitrary byte slice.
///
/// Walks every byte — for large font files (`.ttc` can be tens of
/// MiB) this materialises the whole mmap into RSS, so production
/// callers that just want a dedup key should prefer the cheaper
/// [`content_dedup_hash_u64`]. This variant stays as a building
/// block and for tests that need strict equality.
///
/// Not cryptographic. Stable across process runs and across builds —
/// unlike `std::collections::hash_map::DefaultHasher`, which is
/// randomised per-process — so hashes can be persisted to the disk
/// cache. Processes 8 bytes per iteration, trivial no-dep impl.
pub fn content_hash_u64(bytes: &[u8]) -> u64 {
    // Golden-ratio multiplier; used by xxhash and others as a simple
    // avalanche-friendly constant.
    const K: u64 = 0x9E3779B97F4A7C15;

    let mut h: u64 = K ^ (bytes.len() as u64);
    let chunks = bytes.chunks_exact(8);
    let remainder = chunks.remainder();
    for chunk in chunks {
        let mut arr = [0u8; 8];
        arr.copy_from_slice(chunk);
        let v = u64::from_le_bytes(arr);
        h = h.wrapping_add(v).wrapping_mul(K);
        h ^= h >> 33;
    }
    // Fold in any 1..7 trailing bytes.
    let mut tail: u64 = 0;
    for (i, b) in remainder.iter().enumerate() {
        tail |= (*b as u64) << (i * 8);
    }
    h = h.wrapping_add(tail).wrapping_mul(K);
    h ^= h >> 33;
    h = h.wrapping_mul(K);
    h ^= h >> 33;
    h
}

/// Normalize a family/font name for comparison: lowercase, strip all non-alphanumeric characters.
///
/// This ensures consistent matching regardless of spaces, hyphens, underscores, or casing.
pub fn normalize_family_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Check if a file has a recognized font extension.
#[cfg(feature = "std")]
pub fn is_font_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            let lower = ext.to_lowercase();
            FONT_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_extensions_covers_common_formats() {
        for ext in &["ttf", "otf", "ttc", "woff", "woff2"] {
            assert!(FONT_EXTENSIONS.contains(ext), "missing extension: {}", ext);
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn is_font_file_recognizes_fonts() {
        use std::path::Path;
        assert!(is_font_file(Path::new("Arial.ttf")));
        assert!(is_font_file(Path::new("NotoSans.otf")));
        assert!(is_font_file(Path::new("Font.TTC"))); // case insensitive
        assert!(is_font_file(Path::new("web.woff2")));
    }

    #[cfg(feature = "std")]
    #[test]
    fn is_font_file_rejects_non_fonts() {
        use std::path::Path;
        assert!(!is_font_file(Path::new("readme.txt")));
        assert!(!is_font_file(Path::new("image.png")));
        assert!(!is_font_file(Path::new("no_extension")));
    }
}
