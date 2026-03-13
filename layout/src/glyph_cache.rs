//! Glyph path cache for CPU rendering.
//!
//! Caches built tiny-skia Path objects keyed by (font_hash, glyph_id) so that
//! repeated rendering of the same glyph avoids redundant path construction.
//! The Path is resolution-independent — scale and position are applied at render time.

use std::collections::HashMap;

use crate::font::parsed::{build_glyph_path, OwnedGlyph};

/// Cache key for a glyph path. Paths are resolution-independent (in font units),
/// so only font identity and glyph ID matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphPathKey {
    pub font_hash: u64,
    pub glyph_id: u16,
}

/// Cache of built glyph paths.
pub struct GlyphCache {
    paths: HashMap<GlyphPathKey, Option<tiny_skia::Path>>,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
        }
    }

    /// Get a cached path, or build it on cache miss.
    /// Returns `None` if the glyph has no outline (e.g. space character).
    pub fn get_or_build(
        &mut self,
        font_hash: u64,
        glyph_id: u16,
        glyph_data: &OwnedGlyph,
    ) -> Option<&tiny_skia::Path> {
        let key = GlyphPathKey { font_hash, glyph_id };
        self.paths
            .entry(key)
            .or_insert_with(|| build_glyph_path(glyph_data))
            .as_ref()
    }

    /// Evict all cached paths.
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.paths.len()
    }
}
