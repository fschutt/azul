//! On-disk font cache serialization and deserialization.
//!
//! This entire module is gated on `feature = "cache"`.

use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::{FcFontPath, FcPattern, FontId};
use crate::registry::FcFontRegistry;

/// Font cache manifest for on-disk serialization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FontManifest {
    /// Cache format version (bump on breaking changes)
    pub version: u32,
    /// Entries: path → cached font data
    pub entries: BTreeMap<String, FontCacheEntry>,
}

impl FontManifest {
    /// Bump on breaking changes. v2 adds `bytes_hash` per file entry
    /// for the Arc-shared-bytes deduplication added in rust-fontconfig 3.3.
    pub const CURRENT_VERSION: u32 = 2;
}

/// A single cached font file entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FontCacheEntry {
    /// File modification time (seconds since epoch)
    pub mtime_secs: u64,
    /// File size in bytes
    pub file_size: u64,
    /// 64-bit content hash of the whole file (see
    /// `crate::utils::content_hash_u64`). 0 = not computed.
    #[serde(default)]
    pub bytes_hash: u64,
    /// Parsed font data for each font index in the file
    pub font_indices: Vec<FontIndexEntry>,
}

/// A single font face within a font file, for disk cache serialization.
///
/// Font files (especially `.ttc` collections) can contain multiple faces.
/// Each entry pairs the parsed metadata with the face index so we can
/// reconstruct the full registry from the cache without re-parsing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FontIndexEntry {
    /// Parsed font metadata (name, family, weight, italic, unicode ranges, etc.)
    pub pattern: FcPattern,
    /// Zero-based index of this face within the font file (0 for single-face files)
    pub font_index: usize,
}

impl FcFontRegistry {
    /// Load font metadata from the on-disk cache.
    ///
    /// Reads and deserializes the bincode font manifest from the platform
    /// cache directory, then populates the inner `FcFontCache` with all cached
    /// patterns, font paths, and token indices. Marks all cached file paths as
    /// processed/completed so builder threads skip them.
    ///
    /// Returns `Some(())` on success, `None` if the cache is missing,
    /// unreadable, malformed, or has a version mismatch.
    /// On WASM this is a no-op that always returns `None`.
    #[cfg(not(target_family = "wasm"))]
    pub fn load_from_disk_cache(&self) -> Option<()> {
        let cache_path = get_font_cache_path()?;
        let data = std::fs::read(&cache_path).ok()?;
        let manifest: FontManifest = bincode::deserialize(&data).ok()?;

        if manifest.version != FontManifest::CURRENT_VERSION {
            return None;
        }

        let mut state = self.cache.state_write();
        let mut processed = self.processed_paths.lock().ok()?;
        let mut completed = self.completed_paths.lock().ok()?;

        manifest.entries.iter()
            .flat_map(|(path_str, entry)| {
                let pb = PathBuf::from(path_str);
                processed.insert(pb.clone());
                completed.insert(pb);
                let hash = entry.bytes_hash;
                entry.font_indices.iter().map(move |idx_entry| (path_str, hash, idx_entry))
            })
            .for_each(|(path_str, bytes_hash, idx_entry)| {
                let id = FontId::new();
                state.index_pattern_tokens(&idx_entry.pattern, id);
                state.patterns.insert(idx_entry.pattern.clone(), id);
                state.disk_fonts.insert(id, FcFontPath {
                    path: path_str.clone(),
                    font_index: idx_entry.font_index,
                    bytes_hash,
                });
                state.metadata.insert(id, idx_entry.pattern.clone());
            });

        drop(state);
        self.cache_loaded.store(true, Ordering::Release);

        Some(())
    }

    /// No-op on WASM — no filesystem access available.
    #[cfg(target_family = "wasm")]
    pub fn load_from_disk_cache(&self) -> Option<()> {
        None
    }

    /// Serialize the current registry state to the on-disk font cache.
    ///
    /// Collects all discovered font paths and their parsed metadata into a
    /// [`FontManifest`], then writes it as bincode to the platform cache
    /// directory (e.g. `~/.cache/rfc/fonts/manifest.bin` on Linux).
    ///
    /// Returns `None` if the cache path cannot be determined, the parent
    /// directory cannot be created, or serialization / writing fails.
    /// On WASM this is a no-op that always returns `None` (no filesystem access).
    #[cfg(not(target_family = "wasm"))]
    pub fn save_to_disk_cache(&self) -> Option<()> {
        let cache_path = get_font_cache_path()?;
        std::fs::create_dir_all(cache_path.parent()?).ok()?;

        let state = self.cache.state_read();

        let mut entries: BTreeMap<String, FontCacheEntry> = BTreeMap::new();

        state.disk_fonts.iter()
            .filter_map(|(id, font_path)| {
                state.metadata.get(id).map(|pattern| (font_path, pattern))
            })
            .for_each(|(font_path, pattern)| {
                entries
                    .entry(font_path.path.clone())
                    .or_insert_with(|| {
                        let (mtime_secs, file_size) = get_file_metadata(&font_path.path)
                            .unwrap_or((0, 0));
                        FontCacheEntry {
                            mtime_secs,
                            file_size,
                            bytes_hash: font_path.bytes_hash,
                            font_indices: Vec::new(),
                        }
                    })
                    .font_indices
                    .push(FontIndexEntry {
                        pattern: pattern.clone(),
                        font_index: font_path.font_index,
                    });
            });

        let manifest = FontManifest {
            version: FontManifest::CURRENT_VERSION,
            entries,
        };

        let data = bincode::serialize(&manifest).ok()?;
        std::fs::write(&cache_path, data).ok()?;

        Some(())
    }

    /// No-op on WASM — no filesystem access available.
    #[cfg(target_family = "wasm")]
    pub fn save_to_disk_cache(&self) -> Option<()> {
        None
    }
}

/// Get file mtime (seconds since epoch) and size in bytes.
pub fn get_file_metadata(path: &str) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some((mtime, meta.len()))
}

/// Get the path to the font cache manifest file.
pub fn get_font_cache_path() -> Option<PathBuf> {
    let base = get_cache_base_dir()?;
    Some(base.join("fonts").join("manifest.bin"))
}

/// Get the base cache directory for rust-fontconfig.
#[cfg(not(target_family = "wasm"))]
pub fn get_cache_base_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("rfc"))
}

/// Returns `None` on platforms without a conventional cache directory (e.g. WASM).
#[cfg(target_family = "wasm")]
pub fn get_cache_base_dir() -> Option<PathBuf> {
    None
}
