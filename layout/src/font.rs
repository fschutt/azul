//! Font parsing, metrics extraction, and subsetting.
//!
//! This module provides the core font infrastructure for text layout and PDF generation:
//! - `loading`: System font cache construction and font reload errors
//! - `mock`: Mock font implementation for testing without real font files
//! - `parsed`: Full font parsing via allsorts (outlines, metrics, shaping tables, subsetting)

#![cfg(feature = "font_loading")]

use azul_css::{AzString, U8Vec};
use rust_fontconfig::{FcFontCache, OwnedFontSource};

pub mod loading {
    #![cfg(feature = "std")]
    #![cfg(feature = "font_loading")]
    #![cfg_attr(not(feature = "std"), no_std)]

    use std::io::Error as IoError;

    use azul_css::{AzString, StringVec, U8Vec};
    use rust_fontconfig::FcFontCache;

    #[cfg(not(miri))]
    #[must_use] pub fn build_font_cache() -> FcFontCache {
        FcFontCache::build()
    }

    #[cfg(miri)]
    pub fn build_font_cache() -> FcFontCache {
        FcFontCache::default()
    }

    #[derive(Debug)]
    pub enum FontReloadError {
        Io(IoError, AzString),
        FontNotFound(AzString),
        FontLoadingNotActive(AzString),
    }

    impl Clone for FontReloadError {
        fn clone(&self) -> Self {
            use self::FontReloadError::{Io, FontNotFound, FontLoadingNotActive};
            match self {
                Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
                FontNotFound(id) => FontNotFound(id.clone()),
                FontLoadingNotActive(id) => FontLoadingNotActive(id.clone()),
            }
        }
    }

    azul_core::impl_display!(FontReloadError, {
        Io(err, path_buf) => format!("Could not load \"{}\" - IO error: {}", path_buf.as_str(), err),
        FontNotFound(id) => format!("Could not locate system font: \"{:?}\" found", id),
        FontLoadingNotActive(id) => format!("Could not load system font: \"{:?}\": crate was not compiled with --features=\"font_loading\"", id)
    });
}
pub mod mock {
    //! Mock font implementation for testing text layout.
    //!
    //! Provides a `MockFont` that simulates font behavior without requiring
    //! actual font files, useful for unit testing text layout functionality.

    use std::collections::BTreeMap;

    use crate::text3::cache::LayoutFontMetrics;

    /// A mock font implementation for testing text layout without real fonts.
    ///
    /// This allows testing text shaping, layout, and rendering code paths
    /// without needing to load actual TrueType/OpenType font files.
    #[derive(Debug, Clone)]
    pub struct MockFont {
        /// Font metrics (ascent, descent, etc.).
        pub font_metrics: LayoutFontMetrics,
        /// Width of the space character in font units.
        pub space_width: Option<usize>,
        /// Horizontal advance widths keyed by glyph ID.
        pub glyph_advances: BTreeMap<u16, u16>,
        /// Glyph bounding box sizes (width, height) keyed by glyph ID.
        pub glyph_sizes: BTreeMap<u16, (i32, i32)>,
        /// Unicode codepoint to glyph ID mapping.
        pub glyph_indices: BTreeMap<u32, u16>,
    }

    impl MockFont {
        /// Creates a new `MockFont` with the given font metrics.
        #[must_use] pub const fn new(font_metrics: LayoutFontMetrics) -> Self {
            Self {
                font_metrics,
                space_width: Some(10),
                glyph_advances: BTreeMap::new(),
                glyph_sizes: BTreeMap::new(),
                glyph_indices: BTreeMap::new(),
            }
        }

        /// Sets the space character width.
        #[must_use] pub const fn with_space_width(mut self, width: usize) -> Self {
            self.space_width = Some(width);
            self
        }

        /// Adds a horizontal advance value for a glyph.
        #[must_use] pub fn with_glyph_advance(mut self, glyph_index: u16, advance: u16) -> Self {
            self.glyph_advances.insert(glyph_index, advance);
            self
        }

        /// Adds a bounding box size for a glyph.
        #[must_use] pub fn with_glyph_size(mut self, glyph_index: u16, size: (i32, i32)) -> Self {
            self.glyph_sizes.insert(glyph_index, size);
            self
        }

        /// Adds a Unicode codepoint to glyph ID mapping.
        #[must_use] pub fn with_glyph_index(mut self, unicode: u32, index: u16) -> Self {
            self.glyph_indices.insert(unicode, index);
            self
        }
    }
}

pub mod parsed {
    use core::fmt;
    use std::{collections::BTreeMap, sync::Arc};

    use allsorts::{
        binary::read::ReadScope,
        font_data::FontData,
        layout::{GDEFTable, LayoutCache, LayoutCacheData, GPOS, GSUB},
        outline::{OutlineBuilder, OutlineSink},
        pathfinder_geometry::{line_segment::LineSegment2F, vector::Vector2F},
        subset::{subset as allsorts_subset, whole_font, CmapTarget, SubsetProfile},
        tables::{
            cmap::owned::CmapSubtable as OwnedCmapSubtable,
            glyf::{
                Glyph, GlyfVisitorContext, LocaGlyf, Point,
                VariableGlyfContext, VariableGlyfContextStore,
            },
            kern::owned::KernTable,
            FontTableProvider, HheaTable, MaxpTable,
        },
        tag,
    };
    use azul_core::resources::{
        GlyphOutline, GlyphOutlineOperation, OutlineCubicTo, OutlineLineTo, OutlineMoveTo,
        OutlineQuadTo, OwnedGlyphBoundingBox,
    };
    use azul_css::props::basic::FontMetrics as CssFontMetrics;

    // Mock font module for testing
    pub use crate::font::mock::MockFont;
    use crate::text3::cache::LayoutFontMetrics;

    /// Cached GSUB table for glyph substitution operations.
    pub type GsubCache = Arc<LayoutCacheData<GSUB>>;
    /// Cached GPOS table for glyph positioning operations.
    pub type GposCache = Arc<LayoutCacheData<GPOS>>;

    /// Monotonic-clock nanos since process start. Used to timestamp
    /// `ParsedFont.last_used` for LRU eviction. Cheap (single
    /// `Instant::now`); resolution is plenty fine for "did this
    /// face get touched in the last N seconds" decisions. Exposed
    /// `pub(crate)` so `FontManager::evict_unused` reads from the
    /// same clock as `last_used` writes.
    #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    pub(crate) fn monotonic_now_nanos() -> u64 {
        // Safe: `Instant::elapsed` against the same launch instant is
        // monotonic and never overflows in any realistic process
        // lifetime (>500 years).
        use std::sync::OnceLock;
        use std::time::Instant;
        static LAUNCH: OnceLock<Instant> = OnceLock::new();
        let start = LAUNCH.get_or_init(Instant::now);
        start.elapsed().as_nanos() as u64
    }

    /// Glyph-outline decoder state. See the
    /// [`ParsedFont::loca_glyf`] field docs for the full description.
    #[derive(Clone)]
    pub(crate) enum LocaGlyfState {
        /// Ready to decode immediately, or known to have no outline
        /// data. `None` covers both CFF fonts and fonts where the
        /// loca+glyf parse failed.
        ///
        /// This variant *cannot* be evicted by
        /// [`crate::text3::cache::FontManager::evict_unused`]: there
        /// are no source bytes retained to re-decode from. The eager
        /// `from_bytes` path (tests, `with_source_bytes` PDF callers)
        /// produces this variant.
        Loaded(Option<Arc<std::sync::Mutex<LocaGlyf>>>),
        /// Font bytes retained for lazy `LocaGlyf` construction.
        ///
        /// `loaded` is `Mutex<Option<…>>` (not `OnceLock`) so an
        /// idle eviction can clear it back to `None`; the next
        /// `get_or_decode_glyph` will re-parse from `bytes`. Two-step
        /// double-check pattern in `resolve_loca_glyf` keeps the
        /// expensive `LocaGlyf::load` outside the critical section.
        Deferred {
            bytes: Arc<rust_fontconfig::FontBytes>,
            font_index: usize,
            loaded: Arc<std::sync::Mutex<Option<Arc<std::sync::Mutex<LocaGlyf>>>>>,
        },
    }

    /// Adapter that collects allsorts outline commands into our `GlyphOutline` format.
    ///
    /// Implements `OutlineSink` so it can be passed to `GlyfVisitorContext::visit()`.
    /// This handles composite glyph resolution, transforms, and variable font
    /// deltas automatically via allsorts internals.
    struct GlyphOutlineCollector {
        contours: Vec<GlyphOutline>,
        current_contour: Vec<GlyphOutlineOperation>,
    }

    impl GlyphOutlineCollector {
        const fn new() -> Self {
            Self {
                contours: Vec::new(),
                current_contour: Vec::new(),
            }
        }

        fn into_outlines(mut self) -> Vec<GlyphOutline> {
            if !self.current_contour.is_empty() {
                self.contours.push(GlyphOutline {
                    operations: std::mem::take(&mut self.current_contour).into(),
                });
            }
            self.contours
        }
    }

    impl OutlineSink for GlyphOutlineCollector {
        #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        fn move_to(&mut self, to: Vector2F) {
            if !self.current_contour.is_empty() {
                self.contours.push(GlyphOutline {
                    operations: std::mem::take(&mut self.current_contour).into(),
                });
            }
            self.current_contour.push(GlyphOutlineOperation::MoveTo(OutlineMoveTo {
                x: to.x() as i16,
                y: to.y() as i16,
            }));
        }

        #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        fn line_to(&mut self, to: Vector2F) {
            self.current_contour.push(GlyphOutlineOperation::LineTo(OutlineLineTo {
                x: to.x() as i16,
                y: to.y() as i16,
            }));
        }

        #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        fn quadratic_curve_to(&mut self, ctrl: Vector2F, to: Vector2F) {
            self.current_contour.push(GlyphOutlineOperation::QuadraticCurveTo(
                OutlineQuadTo {
                    ctrl_1_x: ctrl.x() as i16,
                    ctrl_1_y: ctrl.y() as i16,
                    end_x: to.x() as i16,
                    end_y: to.y() as i16,
                },
            ));
        }

        #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        fn cubic_curve_to(&mut self, ctrl: LineSegment2F, to: Vector2F) {
            self.current_contour.push(GlyphOutlineOperation::CubicCurveTo(
                OutlineCubicTo {
                    ctrl_1_x: ctrl.from_x() as i16,
                    ctrl_1_y: ctrl.from_y() as i16,
                    ctrl_2_x: ctrl.to_x() as i16,
                    ctrl_2_y: ctrl.to_y() as i16,
                    end_x: to.x() as i16,
                    end_y: to.y() as i16,
                },
            ));
        }

        fn close(&mut self) {
            self.current_contour.push(GlyphOutlineOperation::ClosePath);
            self.contours.push(GlyphOutline {
                operations: std::mem::take(&mut self.current_contour).into(),
            });
        }
    }

    /// Parsed font data with all required tables for text layout and PDF generation.
    ///
    /// This struct holds the parsed representation of a TrueType/OpenType font,
    /// including glyph outlines, metrics, and shaping tables. It's used for:
    /// - Text layout (via GSUB/GPOS tables)
    /// - Glyph rendering (via glyf/CFF outlines)
    /// - PDF font embedding (via font metrics and subsetting)
    pub struct ParsedFont {
        /// Hash of the font bytes for caching and equality checks.
        pub hash: u64,
        /// Layout-specific font metrics (ascent, descent, line gap).
        pub font_metrics: LayoutFontMetrics,
        /// PDF-specific detailed font metrics from HEAD, HHEA, OS/2 tables.
        pub pdf_font_metrics: PdfFontMetrics,
        /// Total number of glyphs in the font (from maxp table).
        pub num_glyphs: u16,
        /// Horizontal header table (hhea) containing global horizontal metrics.
        pub hhea_table: HheaTable,
        /// Offset+length into `original_bytes` for hmtx table (lazy: no copy).
        pub hmtx_range: (usize, usize),
        /// Offset+length into `original_bytes` for vmtx table (lazy: no copy).
        pub vmtx_range: (usize, usize),
        /// Vertical header table (vhea), same format as hhea. None if font has no vertical metrics.
        pub vhea_table: Option<HheaTable>,
        /// Maximum profile table (maxp) containing glyph count and memory hints.
        pub maxp_table: MaxpTable,
        /// Raw GSUB table bytes, kept as a `Vec<u8>` (tens to low-hundreds
        /// of KiB) so the parsed `GsubCache` can be built on first shape
        /// call instead of up-front. Access via [`ParsedFont::gsub`] —
        /// that getter populates `gsub_cache_lazy` via `OnceLock` and
        /// returns a borrow.
        pub(crate) gsub_bytes: Option<Vec<u8>>,
        /// Lazy GSUB cache: populated on first [`ParsedFont::gsub`] call.
        /// `None` means "font has no GSUB table" *after* init attempt;
        /// the `OnceLock` wrapper distinguishes "not yet initialised"
        /// from "initialised to None".
        pub(crate) gsub_cache_lazy: std::sync::OnceLock<Option<GsubCache>>,
        /// Raw GPOS table bytes. Same lazy-parse arrangement as
        /// `gsub_bytes` — see [`ParsedFont::gpos`].
        pub(crate) gpos_bytes: Option<Vec<u8>>,
        /// Lazy GPOS cache, populated on first [`ParsedFont::gpos`] call.
        pub(crate) gpos_cache_lazy: std::sync::OnceLock<Option<GposCache>>,
        /// Glyph definition table (GDEF) for glyph classification.
        pub opt_gdef_table: Option<Arc<GDEFTable>>,
        /// Legacy kerning table (kern) for fonts without GPOS.
        pub opt_kern_table: Option<Arc<KernTable>>,
        /// Monotonic-clock nanos at the most recent
        /// [`ParsedFont::get_or_decode_glyph`] / `gsub()` / `gpos()`
        /// call. `0` means "never touched". Used by
        /// [`crate::text3::cache::FontManager::evict_unused`] to
        /// decide which `LocaGlyfState::Deferred` faces to release.
        pub(crate) last_used: Arc<std::sync::atomic::AtomicU64>,
        /// `true` if this font is a variable font (carries a `gvar`
        /// table). Cached at parse time so [`decode_glyph_inner`]
        /// can short-circuit the variable-context construction for
        /// the common non-variable case. Variable-glyph delta
        /// application requires the source bytes to be retained,
        /// so it only fires on the `LocaGlyfState::Deferred` path.
        pub(crate) is_variable_font: bool,
        /// Lazy outline cache. Populated on first
        /// [`ParsedFont::get_or_decode_glyph`] call per `gid`; entries
        /// are wrapped in `Arc` so callers can hold them without
        /// keeping the lock. The space glyph (and `.notdef` when
        /// present) are pre-inserted by `from_bytes_internal` so the
        /// shaper's cmap-miss path has something to render without
        /// racing with a decode.
        ///
        /// Tests that previously walked the public `glyph_records_decoded`
        /// `BTreeMap` field now call
        /// [`ParsedFont::prime_glyph_cache`] (decodes every glyph into
        /// this cache) followed by
        /// [`ParsedFont::for_each_decoded_glyph`] /
        /// [`ParsedFont::glyph_cache_snapshot`] to walk the result.
        // [az-web-lift] queue RwLock spins in lock_contended in single-threaded lifted wasm
        // (only the pure-Rust queue RwLock is lifted; Mutex is Leaf-stubbed). Reuse
        // rust_fontconfig::StLock (no-atomic single-threaded bypass). One of the 3 RwLocks total.
        pub(crate) glyph_cache: Arc<rust_fontconfig::StLock<BTreeMap<u16, Arc<OwnedGlyph>>>>,
        /// Glyph outline decoder state.
        ///
        /// - `Loaded(Some(arc))`: `LocaGlyf` is already loaded (owning
        ///   its own `Box<[u8]>` copy of the loca+glyf tables) and
        ///   ready to decode glyphs. Produced by the eager `from_bytes`
        ///   constructor path (tests).
        /// - `Loaded(None)`: the font has no usable loca+glyf (CFF, or
        ///   a parse failure). Glyph outlines won't decode; the hmtx
        ///   advance fallback fills in the blanks.
        /// - `Deferred`: we retain an `Arc<[u8]>` to the full font file
        ///   and the `font_index`; the first `get_or_decode_glyph` call
        ///   parses a fresh `FontData` / `TableProvider` from those
        ///   bytes and loads `LocaGlyf`, storing the result in the
        ///   `OnceLock`. Fonts that get resolved into a chain but are
        ///   never actually rasterized pay zero decode cost — this is
        ///   the big win for pages like `excel.html` where 20+ fallback
        ///   faces load but only a handful are touched.
        pub(crate) loca_glyf: LocaGlyfState,
        /// Cached width of the space character in font units.
        pub space_width: Option<usize>,
        /// Character-to-glyph mapping (cmap subtable).
        pub cmap_subtable: Option<OwnedCmapSubtable>,
        /// Mock font data for testing (replaces real font behavior).
        pub mock: Option<Box<MockFont>>,
        /// Reverse mapping: `glyph_id` -> cluster text (handles ligatures like "fi").
        pub reverse_glyph_cache: BTreeMap<u16, String>,
        /// Original font bytes — only retained for callers that need to
        /// reconstruct or subset the font (PDF export). Layout / shaping /
        /// raster never read this, so `ParsedFont::from_bytes` leaves it
        /// as `None` by default and callers opt in via
        /// [`ParsedFont::with_source_bytes`]. Shared across faces of the
        /// same `.ttc` via the `Arc<FontBytes>` that
        /// [`rust_fontconfig::FcFontCache::get_font_bytes`] returns —
        /// for disk fonts the backing is an mmap so untouched pages
        /// don't count toward RSS.
        pub original_bytes: Option<Arc<rust_fontconfig::FontBytes>>,
        /// Font index within collection (0 for single-font files).
        pub original_index: usize,
        /// GID to CID mapping for CFF fonts (required for PDF embedding).
        pub index_to_cid: BTreeMap<u16, u16>,
        /// Font type (TrueType outlines or OpenType CFF).
        pub font_type: FontType,
        /// PostScript font name from the NAME table.
        pub font_name: Option<String>,
        /// TrueType bytecode hinting instance (mutable interpreter state).
        /// Wrapped in Mutex because hinting mutates internal state.
        /// None for CFF fonts or fonts without hinting data.
        pub hint_instance: Option<std::sync::Mutex<allsorts::hinting::HintInstance>>,
    }

    impl Clone for ParsedFont {
        fn clone(&self) -> Self {
            Self {
                hash: self.hash,
                font_metrics: self.font_metrics.clone(),
                pdf_font_metrics: self.pdf_font_metrics,
                num_glyphs: self.num_glyphs,
                hhea_table: self.hhea_table.clone(),
                hmtx_range: self.hmtx_range,
                vmtx_range: self.vmtx_range,
                vhea_table: self.vhea_table.clone(),
                maxp_table: self.maxp_table.clone(),
                // OnceLock<T: Clone>: Clone preserves the init state, so
                // a clone of a parsed cache skips re-parse on first
                // access. The raw bytes we keep around for lazy init
                // are cloned too.
                gsub_bytes: self.gsub_bytes.clone(),
                gsub_cache_lazy: self.gsub_cache_lazy.clone(),
                gpos_bytes: self.gpos_bytes.clone(),
                gpos_cache_lazy: self.gpos_cache_lazy.clone(),
                opt_gdef_table: self.opt_gdef_table.clone(),
                opt_kern_table: self.opt_kern_table.clone(),
                // Share the lazy cache and loca_glyf across clones: cheap
                // Arc bump, amortises glyph decode across clones of the
                // same face.
                last_used: Arc::clone(&self.last_used),
                is_variable_font: self.is_variable_font,
                glyph_cache: Arc::clone(&self.glyph_cache),
                // `LocaGlyfState` is `Clone` — for `Loaded` this is an
                // `Arc::clone`; for `Deferred` it's an `Arc::clone` of
                // the bytes + the `OnceLock`, so a clone of a face
                // that's already decoded glyphs carries the decode.
                loca_glyf: self.loca_glyf.clone(),
                space_width: self.space_width,
                cmap_subtable: self.cmap_subtable.clone(),
                mock: self.mock.clone(),
                reverse_glyph_cache: self.reverse_glyph_cache.clone(),
                // Arc clone — O(1), just bumps refcount; no byte copy.
                original_bytes: self.original_bytes.clone(),
                original_index: self.original_index,
                index_to_cid: self.index_to_cid.clone(),
                font_type: self.font_type.clone(),
                font_name: self.font_name.clone(),
                // HintInstance has mutable interpreter state and is not Clone.
                // Clones are used for PDF/serialization where hinting isn't needed.
                hint_instance: None,
            }
        }
    }

    /// Distinguishes TrueType fonts from OpenType CFF fonts.
    ///
    /// This affects how glyph outlines are extracted and how the font
    /// is embedded in PDF documents.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum FontType {
        /// TrueType font with quadratic Bézier outlines in glyf table.
        TrueType,
        /// OpenType font with cubic Bézier outlines in CFF table.
        /// Contains the serialized CFF data for PDF embedding.
        OpenTypeCFF(Vec<u8>),
    }

    /// PDF-specific font metrics from HEAD, HHEA, and OS/2 tables.
    ///
    /// These metrics are used for PDF font descriptors and accurate
    /// text positioning in generated PDF documents.
    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(C)]
    pub struct PdfFontMetrics {
        // -- HEAD table fields --
        /// Font units per em-square (typically 1000 or 2048).
        pub units_per_em: u16,
        /// Font flags (italic, bold, fixed-pitch, etc.).
        pub font_flags: u16,
        /// Minimum x-coordinate across all glyphs.
        pub x_min: i16,
        /// Minimum y-coordinate across all glyphs.
        pub y_min: i16,
        /// Maximum x-coordinate across all glyphs.
        pub x_max: i16,
        /// Maximum y-coordinate across all glyphs.
        pub y_max: i16,

        // -- HHEA table fields --
        /// Typographic ascender (distance above baseline).
        pub ascender: i16,
        /// Typographic descender (distance below baseline, usually negative).
        pub descender: i16,
        /// Recommended line gap between lines of text.
        pub line_gap: i16,
        /// Maximum horizontal advance width across all glyphs.
        pub advance_width_max: u16,
        /// Caret slope rise for italic angle calculation.
        pub caret_slope_rise: i16,
        /// Caret slope run for italic angle calculation.
        pub caret_slope_run: i16,

        // -- OS/2 table fields (0 if table not present) --
        /// Average width of lowercase letters.
        pub x_avg_char_width: i16,
        /// Visual weight class (100-900, 400=normal, 700=bold).
        pub us_weight_class: u16,
        /// Visual width class (1-9, 5=normal).
        pub us_width_class: u16,
        /// Thickness of strikeout stroke in font units.
        pub y_strikeout_size: i16,
        /// Vertical position of strikeout stroke.
        pub y_strikeout_position: i16,
    }

    impl Default for PdfFontMetrics {
        fn default() -> Self {
            Self::zero()
        }
    }

    impl PdfFontMetrics {
        /// Returns zeroed metrics with `units_per_em` set to 1000 (standard PostScript default)
        /// to avoid division-by-zero in scaling calculations.
        #[must_use] pub const fn zero() -> Self {
            Self {
                units_per_em: 1000,
                font_flags: 0,
                x_min: 0,
                y_min: 0,
                x_max: 0,
                y_max: 0,
                ascender: 0,
                descender: 0,
                line_gap: 0,
                advance_width_max: 0,
                caret_slope_rise: 0,
                caret_slope_run: 0,
                x_avg_char_width: 0,
                us_weight_class: 0,
                us_width_class: 0,
                y_strikeout_size: 0,
                y_strikeout_position: 0,
            }
        }
    }

    /// Result of font subsetting operation.
    ///
    /// Contains the subsetted font bytes and a mapping from original
    /// glyph IDs to new glyph IDs in the subset.
    #[derive(Debug, Clone)]
    pub struct SubsetFont {
        /// The subsetted font file bytes (smaller than original).
        pub bytes: Vec<u8>,
        /// Mapping: original glyph ID -> (new subset glyph ID, source character).
        pub glyph_mapping: BTreeMap<u16, (u16, char)>,
    }

    impl SubsetFont {
        /// Return the changed text so that when rendering with the subset font (instead of the
        /// original) the renderer will end up at the same glyph IDs as if we used the original text
        /// on the original font
        #[must_use] pub fn subset_text(&self, text: &str) -> String {
            text.chars()
                .filter_map(|c| {
                    self.glyph_mapping.values().find_map(|(ngid, ch)| {
                        if *ch == c {
                            char::from_u32(u32::from(*ngid))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        }
    }

    /// Hash-based equality: two fonts are considered equal if their content hash matches.
    /// This is a performance optimization — hash collisions are possible but vanishingly
    /// unlikely (~1/2^64).
    impl PartialEq for ParsedFont {
        fn eq(&self, other: &Self) -> bool {
            self.hash == other.hash
        }
    }

    impl Eq for ParsedFont {}

    const FONT_B64_START: &str = "data:font/ttf;base64,";

    impl serde::Serialize for ParsedFont {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use base64::Engine;
            let s = format!(
                "{FONT_B64_START}{}",
                base64::prelude::BASE64_STANDARD.encode(self.to_bytes(None).unwrap_or_default())
            );
            s.serialize(serializer)
        }
    }

    impl<'de> serde::Deserialize<'de> for ParsedFont {
        fn deserialize<D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Self, D::Error> {
            use base64::Engine;
            let s = String::deserialize(deserializer)?;
            let b64 = s.strip_prefix(FONT_B64_START).and_then(|b| base64::prelude::BASE64_STANDARD.decode(b).ok());

            let mut warnings = Vec::new();
            Self::from_bytes(&b64.unwrap_or_default(), 0, &mut warnings).ok_or_else(|| {
                serde::de::Error::custom(format!("Font deserialization error: {warnings:?}"))
            })
        }
    }

    impl fmt::Debug for ParsedFont {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ParsedFont")
                .field("hash", &self.hash)
                .field("font_metrics", &self.font_metrics)
                .field("num_glyphs", &self.num_glyphs)
                .field("hhea_table", &self.hhea_table)
                .field(
                    "hmtx_range",
                    &format_args!("<{} bytes>", self.hmtx_range.1),
                )
                .field("maxp_table", &self.maxp_table)
                .field(
                    "glyph_cache",
                    &format_args!(
                        "{} entries (lazy)",
                        self.glyph_cache.read().map(|m| m.len()).unwrap_or(0),
                    ),
                )
                .field("space_width", &self.space_width)
                .field("cmap_subtable", &self.cmap_subtable)
                .finish_non_exhaustive()
        }
    }

    /// Warning or error message generated during font parsing.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FontParseWarning {
        /// Severity level of this warning.
        pub severity: FontParseWarningSeverity,
        /// Human-readable description of the issue.
        pub message: String,
    }

    /// Severity level for font parsing warnings.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FontParseWarningSeverity {
        /// Informational message (not an error).
        Info,
        /// Warning that may affect font rendering.
        Warning,
        /// Error that prevents proper font usage.
        Error,
    }

    impl FontParseWarning {
        /// Creates an info-level message.
        #[must_use] pub const fn info(message: String) -> Self {
            Self {
                severity: FontParseWarningSeverity::Info,
                message,
            }
        }

        /// Creates a warning-level message.
        #[must_use] pub const fn warning(message: String) -> Self {
            Self {
                severity: FontParseWarningSeverity::Warning,
                message,
            }
        }

        /// Creates an error-level message.
        #[must_use] pub const fn error(message: String) -> Self {
            Self {
                severity: FontParseWarningSeverity::Error,
                message,
            }
        }
    }

    // WEB-LIFT FIX (2026-06-02): a `FontTableProvider` that scans the sfnt table directory
    // by hand from the raw font bytes. allsorts' `OffsetTableFontProvider` produces garbage
    // on the remill/web backend: (1) `ReadArray::read_item`'s nested-tuple `TableRecord` read
    // returns `table_tag = 0` for EVERY record (proven: tags[7]=0x0000 while the bytes there
    // are 0x68656164 'head'); (2) even a hand-rolled scan added to the *allsorts crate* sees a
    // bad `self.scope.data()` (the ReadScope fat-pointer mis-lifts through provider
    // construction, or allsorts-crate code lifts differently). This provider lives in
    // azul-layout — whose identical byte reads PROVABLY work (the `from_provider` probe read
    // num_tables=15 from these same `font_bytes`) — and reads the slice directly. KEEP.
    #[inline]
    fn manual_be16(d: &[u8], o: usize) -> u32 {
        (u32::from(d[o]) << 8) | u32::from(d[o + 1])
    }
    #[inline]
    fn manual_be32(d: &[u8], o: usize) -> u32 {
        (u32::from(d[o]) << 24)
            | (u32::from(d[o + 1]) << 16)
            | (u32::from(d[o + 2]) << 8)
            | u32::from(d[o + 3])
    }

    struct ManualTableProvider<'a> {
        data: &'a [u8],
        dir: usize, // byte offset of the first table record (offset-table base + 12)
        num: usize, // number of table records
    }

    impl<'a> ManualTableProvider<'a> {
        fn new(data: &'a [u8], font_index: usize) -> Option<Self> {
            if data.len() < 12 {
                return None;
            }
            let base = if manual_be32(data, 0) == 0x7474_6366 {
                // 'ttcf' (TrueType Collection): the font_index'th offset-table offset.
                let num_fonts = manual_be32(data, 8) as usize;
                if font_index >= num_fonts || 12 + font_index * 4 + 4 > data.len() {
                    return None;
                }
                manual_be32(data, 12 + font_index * 4) as usize
            } else {
                0 // single font: offset table at the start
            };
            if base + 12 > data.len() {
                return None;
            }
            Some(ManualTableProvider {
                data,
                dir: base + 12,
                num: manual_be16(data, base + 4) as usize,
            })
        }
    }

    impl FontTableProvider for ManualTableProvider<'_> {
        fn table_data(
            &self,
            tag: u32,
        ) -> Result<Option<std::borrow::Cow<'_, [u8]>>, allsorts::error::ParseError> {
            let mut i = 0;
            while i < self.num {
                let r = self.dir + i * 16;
                if r + 16 > self.data.len() {
                    break;
                }
                if manual_be32(self.data, r) == tag {
                    let off = manual_be32(self.data, r + 8) as usize;
                    let len = manual_be32(self.data, r + 12) as usize;
                    return Ok(off
                        .checked_add(len)
                        .filter(|&e| e <= self.data.len())
                        .map(|e| std::borrow::Cow::Borrowed(&self.data[off..e])));
                }
                i += 1;
            }
            Ok(None)
        }

        fn has_table(&self, tag: u32) -> bool {
            self.table_data(tag).ok().flatten().is_some()
        }

        fn table_tags(&self) -> Option<Vec<u32>> {
            // DIAG (REVERT): sentinel 0xFADE as tags[0] proves THIS provider ran; then
            // self.num pushes let me see if the usize field survived; then the real reads
            // show if self.data (slice field) survived the struct move through generics.
            let mut tags = Vec::with_capacity(self.num + 1);
            tags.push(0x0000_FADE);
            let mut i = 0;
            while i < self.num {
                let r = self.dir + i * 16;
                if r + 4 > self.data.len() {
                    break;
                }
                tags.push(manual_be32(self.data, r));
                i += 1;
            }
            Some(tags)
        }
    }

    impl allsorts::tables::SfntVersion for ManualTableProvider<'_> {
        fn sfnt_version(&self) -> u32 {
            let base = self.dir.saturating_sub(12);
            if base + 4 <= self.data.len() {
                manual_be32(self.data, base)
            } else {
                0
            }
        }
    }

    impl ParsedFont {
        /// Parse a font from bytes using allsorts
        ///
        /// # Arguments
        /// * `font_bytes` - The font file data
        /// * `font_index` - Index of the font in a font collection (0 for single fonts)
        /// * `warnings` - Optional vector to collect parsing warnings
        ///
        /// # Returns
        /// `Some(ParsedFont)` if parsing succeeds, `None` otherwise
        ///
        /// Note: Outlines are decoded lazily by `get_or_decode_glyph`;
        /// `LocaGlyf::load` runs eagerly here. Use `from_bytes_shared`
        /// for the lazy-LocaGlyf production path.
        pub fn from_bytes(
            font_bytes: &[u8],
            font_index: usize,
            warnings: &mut Vec<FontParseWarning>,
        ) -> Option<Self> {
            // `from_bytes` keeps the eager-LocaGlyf behaviour for the
            // small number of callers (mainly tests) that don't have
            // an `Arc<[u8]>` to keep alive for the lazy path.
            let mut font = Self::from_bytes_internal(font_bytes, font_index, warnings, false)?;
            // Retain an owned copy of the source bytes so the face can later be
            // subset/embedded (PDF export, save->parse roundtrips). Callers pass a
            // borrowed slice that may not outlive us, so we own it here. Mirrors
            // `from_bytes_shared`, which retains the caller's `Arc<FontBytes>`.
            if font.original_bytes.is_none() {
                font.original_bytes = Some(Arc::new(
                    rust_fontconfig::FontBytes::Owned(Arc::from(font_bytes.to_vec())),
                ));
            }
            Some(font)
        }

        /// Shared implementation of `from_bytes` / `from_bytes_shared`.
        ///
        /// `defer_loca_glyf = true` skips the `LocaGlyf::load` call
        /// here so the caller (`from_bytes_shared`) can install a
        /// `LocaGlyfState::Deferred` slot that re-parses on first
        /// glyph decode. Saves the load-then-drop cycle the previous
        /// arrangement paid (`from_bytes_shared` used to call
        /// `from_bytes` and immediately replace the loaded `LocaGlyf`
        /// with a Deferred slot, throwing away ~hundreds of KiB of
        /// loca+glyf bytes per face for fonts in the chain that get
        /// loaded but never rasterized).
        fn from_bytes_internal(
            font_bytes: &[u8],
            font_index: usize,
            warnings: &mut Vec<FontParseWarning>,
            defer_loca_glyf: bool,
        ) -> Option<Self> {
            use allsorts::{binary::read::ReadScope, font_data::FontData};
            fn provider_err(font_index: usize, e: impl fmt::Display) -> FontParseWarning {
                FontParseWarning::error(format!(
                    "Failed to get table provider for font index {font_index}: {e}"
                ))
            }

            let scope = ReadScope::new(font_bytes);
            let font_file = match scope.read::<FontData<'_>>() {
                Ok(ff) => ff,
                Err(e) => {
                    warnings.push(FontParseWarning::error(format!(
                        "Failed to read font data: {e}"
                    )));
                    return None;
                }
            };
            // FIX (2026-06-02): route OpenType fonts through the CONCRETE provider
            // (`OffsetTableFontProvider`) instead of `FontData::table_provider`'s
            // `Box<dyn FontTableProvider>`. On the lifted/web backend the trait-object
            // VTABLE dispatch (allsorts font_data.rs:45 `self.provider.table_data(tag)`)
            // mis-lifts: the vtable's fn-pointers are untranslated native addresses, so the
            // indirect-call dispatcher routes the dyn call to the WRONG `table_data` impl,
            // which returns a `Cow::Owned` garbage buffer → `HeadTable::read` errors → font
            // parse returns None → text measures height 0. A concrete provider makes every
            // `table_data` a DIRECT (monomorphized) call, which lifts correctly. Woff/Woff2
            // keep the dyn path (they're not used on the web backend's embedded TTF).
            match font_file {
                FontData::OpenType(otf) => {
                    // Prefer the hand-rolled provider (reads font_bytes directly) over
                    // allsorts' OffsetTableFontProvider, whose lifted table reads are garbage
                    // on the web backend. Fall back to allsorts only if the manual layout
                    // parse can't recognise the sfnt (e.g. an unusual TTC).
                    if let Some(mp) = ManualTableProvider::new(font_bytes, font_index) {
                        Self::from_provider(mp, font_bytes, font_index, warnings, defer_loca_glyf)
                    } else {
                        match otf.table_provider(font_index) {
                            Ok(p) => Self::from_provider(
                                p,
                                font_bytes,
                                font_index,
                                warnings,
                                defer_loca_glyf,
                            ),
                            Err(e) => {
                                warnings.push(provider_err(font_index, e));
                                None
                            }
                        }
                    }
                }
                other => match other.table_provider(font_index) {
                    Ok(p) => {
                        Self::from_provider(p, font_bytes, font_index, warnings, defer_loca_glyf)
                    }
                    Err(e) => {
                        warnings.push(provider_err(font_index, e));
                        None
                    }
                },
            }
        }

        /// Build a `ParsedFont` from a concrete [`FontTableProvider`]. Split out of
        /// `from_bytes_internal` (2026-06-02) so OpenType fonts use the concrete
        /// `OffsetTableFontProvider` (direct `table_data` calls that lift correctly on
        /// the web backend) rather than `FontData::table_provider`'s `Box<dyn>`, whose
        /// trait-object vtable dispatch mis-lifts (wrong impl → Owned garbage → parse fail).
        #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
        fn from_provider<P: FontTableProvider>(
            provider: P,
            font_bytes: &[u8],
            font_index: usize,
            warnings: &mut Vec<FontParseWarning>,
            defer_loca_glyf: bool,
        ) -> Option<Self> {
            use std::{
                collections::hash_map::DefaultHasher,
                hash::{Hash, Hasher},
            };

            use allsorts::{
                binary::read::ReadScope,
                tables::{
                    cmap::{owned::CmapSubtable as OwnedCmapSubtable, CmapSubtable},
                    FontTableProvider, HeadTable, HheaTable, MaxpTable,
                },
                tag,
            };

            // Extract font name from NAME table early (before provider is moved).
            // WEB-LIFT FIX (2026-06-02): NameTable::string_for_id decodes the NAME strings via
            // `encoding_rs` (Mac Roman / UTF-16 charset state machines), whose jump-tables
            // are NOT devirt'd by the remill lift → MISSING_BLOCK trap (proven: trap in
            // encoding_rs::Decoder::decode_to_utf8). font_name is OPTIONAL metadata (NOT used
            // for layout/metrics/shaping — those are binary head/hhea/maxp/cmap/glyf), so skip
            // the NAME-string decode on the web backend to avoid encoding_rs entirely.
            #[cfg(feature = "web_lift")]
            let font_name: Option<String> = None;
            #[cfg(not(feature = "web_lift"))]
            let font_name = provider.table_data(tag::NAME).ok().and_then(|name_data| {
                ReadScope::new(&name_data?)
                    .read::<allsorts::tables::NameTable<'_>>()
                    .ok()
                    .and_then(|name_table| {
                        name_table.string_for_id(allsorts::tables::NameTable::POSTSCRIPT_NAME)
                    })
            });

            // DIAG (2026-06-02, REVERT): pinpoint the web font-parse-fails root — does HEAD
            // fail because table_data can't find/return the table (directory mis-lift) or
            // because HeadTable::read errors (table-read mis-lift)? Surfaced via warnings.
            let head_table = match provider.table_data(tag::HEAD) {
                Ok(Some(head_cow)) => {
                    // DIAG: is the HEAD table data CORRECT (magicNumber 0x5F0F3CF5 @ off 12 →
                    // HeadTable::read mis-lifts) or WRONG bytes (directory offset mis-lift)?
                    let bb = head_cow.as_ref();
                    let magic = if bb.len() >= 16 {
                        (u32::from(bb[12]) << 24) | (u32::from(bb[13]) << 16)
                            | (u32::from(bb[14]) << 8) | u32::from(bb[15])
                    } else { 0 };
                    if let Ok(h) = ReadScope::new(&head_cow).read::<HeadTable>() { h } else {
                        // DIAG: surface the sliced offset (how wrong) as hex — "HO" + 8 hex
                        // of (head_cow.ptr - font_bytes.ptr). garbage→offset-read mis-lift;
                        // 00000000→base; plausible-but-wrong→record mapping. "RF"=bytes-OK.
                        let m = if magic == 0x5F0F_3CF5 {
                            "RF000000".to_string()
                        } else {
                            let off = (head_cow.as_ref().as_ptr() as usize)
                                .wrapping_sub(font_bytes.as_ptr() as usize);
                            let mut msg = String::new();
                            // B=Borrowed(slice of font_bytes, ptr-arith/base mis-lift) vs
                            // O=Owned(decompressed/copied Vec — wrong path for plain TTF).
                            msg.push(if matches!(head_cow, std::borrow::Cow::Borrowed(_)) { 'B' } else { 'O' });
                            msg.push_str("HO");
                            let mut sh: i32 = 28;
                            while sh >= 0 {
                                let d = ((off >> sh) & 0xf) as u8;
                                msg.push((if d < 10 { b'0' + d } else { b'a' + d - 10 }) as char);
                                sh -= 4;
                            }
                            msg
                        };
                        warnings.push(FontParseWarning::error(m));
                        return None;
                    }
                }
                Ok(None) => {
                    // DIAG (REVERT): bytes+len+read_item-count+dir all proved OK (N0fr0fc0fg1)
                    // yet find_table_record(HEAD)=None though 'head' is rec[7] on disk. So
                    // either read_item's table_tag FIELD is garbage, or tag::HEAD mis-lifts, or
                    // the u32 == mis-lifts. t7 = tags[7] (should be 0x68656164 'head' low16
                    // =6164); H = tag::HEAD low16 (should be 6164); f = ANY tag==HEAD via an
                    // indexed compare loop (NOT .iter().any). "T<4h t7>H<4h HEAD>f<0|1>".
                    //   T6164 H6164 f1 → values+compare OK (won't reach here — HEAD found)
                    //   T6164 H6164 f0 → the u32 == comparison mis-lifts
                    //   T!=6164        → read_item table_tag FIELD garbage (tuple read mis-lift)
                    //   H!=6164        → tag::HEAD const mis-lifts
                    #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
                    fn hx(m: &mut String, val: u32, nibbles: i32) {
                        let mut sh = (nibbles - 1) * 4;
                        while sh >= 0 {
                            let d = ((val >> sh) & 0xf) as u8;
                            m.push((if d < 10 { b'0' + d } else { b'a' + d - 10 }) as char);
                            sh -= 4;
                        }
                    }
                    // DECISIVE: tags[8] (head, file off 124) reads 0 but tags[2] (off 28) is OK.
                    // Read the SAME offsets from from_provider's LOCAL font_bytes param (proven
                    // correct at off 4/12). If local@124 = 0x6865 'he' but provider tags[8]=0 ⇒
                    // STORED-SLICE issue (provider self.data fat-ptr mis-lifts) → read locally.
                    // If local@124 = 0 ⇒ the font CONST is only PARTIALLY MIRRORED into the wasm
                    // (deep data-mirror gap) → table data is simply absent. local@93596 (=0x16f9c,
                    // head TABLE data start) further maps the mirror: 'he'/nonzero vs 0.
                    let loc124 = if font_bytes.len() >= 126 {
                        (u32::from(font_bytes[124]) << 8) | u32::from(font_bytes[125])
                    } else {
                        0xEEEE
                    };
                    let loc_head = if font_bytes.len() >= 93598 {
                        (u32::from(font_bytes[93596]) << 8) | u32::from(font_bytes[93597])
                    } else {
                        0xEEEE
                    };
                    let mut m = String::from("L"); // local font_bytes[124..126] (head dir record):
                    hx(&mut m, loc124 & 0xffff, 4); // 6865 'he' = mirrored; 0000 = not
                    m.push('H'); // local font_bytes[93596..] (head TABLE data, deep):
                    hx(&mut m, loc_head & 0xffff, 4);
                    warnings.push(FontParseWarning::error(m));
                    return None;
                }
                Err(_) => {
                    warnings.push(FontParseWarning::error("HEAD_DATAERR".to_string()));
                    return None;
                }
            };

            let maxp_table = provider
                .table_data(tag::MAXP)
                .ok()
                .and_then(|maxp_data| ReadScope::new(&maxp_data?).read::<MaxpTable>().ok())
                .unwrap_or(MaxpTable {
                    num_glyphs: 0,
                    version1_sub_table: None,
                });

            let num_glyphs = maxp_table.num_glyphs as usize;

            // Compute byte offset+length into font_bytes for hmtx/vmtx
            // instead of copying the table data. The provider returns a
            // borrowed slice for OpenType fonts, so we can derive the
            // offset via pointer arithmetic.
            let hmtx_range = provider
                .table_data(tag::HMTX)
                .ok()
                .and_then(|cow_opt| {
                    let cow = cow_opt?;
                    match cow {
                        std::borrow::Cow::Borrowed(slice) => {
                            let base = font_bytes.as_ptr() as usize;
                            let ptr = slice.as_ptr() as usize;
                            let offset = ptr.checked_sub(base)?;
                            if offset + slice.len() <= font_bytes.len() {
                                Some((offset, slice.len()))
                            } else {
                                None
                            }
                        }
                        std::borrow::Cow::Owned(_) => None,
                    }
                })
                .unwrap_or((0, 0));

            let vmtx_range = provider
                .table_data(tag::VMTX)
                .ok()
                .and_then(|s| {
                    let slice = s?;
                    let base = font_bytes.as_ptr() as usize;
                    let ptr = slice.as_ptr() as usize;
                    let offset = ptr.checked_sub(base)?;
                    if offset + slice.len() <= font_bytes.len() {
                        Some((offset, slice.len()))
                    } else {
                        None
                    }
                })
                .unwrap_or((0, 0));

            // Parse vhea table (same format as hhea, used for vertical metrics)
            let vhea_table = provider
                .table_data(tag::VHEA)
                .ok()
                .and_then(|vhea_data| ReadScope::new(&vhea_data?).read::<HheaTable>().ok());

            // hhea is required per the OpenType spec; return None if missing
            let hhea_table = provider
                .table_data(tag::HHEA)
                .ok()
                .and_then(|hhea_data| ReadScope::new(&hhea_data?).read::<HheaTable>().ok())?;

            // Build layout-specific font metrics
            let font_metrics = LayoutFontMetrics {
                units_per_em: if head_table.units_per_em == 0 {
                    1000
                } else {
                    head_table.units_per_em
                },
                ascent: f32::from(hhea_table.ascender),
                descent: f32::from(hhea_table.descender),
                line_gap: f32::from(hhea_table.line_gap),
                x_height: None, // will be populated from OS/2 table via from_font_metrics if available
                cap_height: None,
            };

            // Build PDF-specific font metrics
            let pdf_font_metrics =
                Self::parse_pdf_font_metrics(font_bytes, font_index, &head_table, &hhea_table);

            // Use allsorts LocaGlyf for on-demand outline extraction. We
            // *load* LocaGlyf eagerly (it owns ~tens of KiB of loca +
            // ~hundreds of KiB of glyf bytes) but we *don't* decode any
            // glyph outlines up front — that's the big RSS win. Glyphs
            // are decoded by `ParsedFont::get_or_decode_glyph` on first
            // access from the CPU/GPU rasterizer.
            //
            // When `defer_loca_glyf` is set (production lazy path via
            // `from_bytes_shared`), we skip `LocaGlyf::load` here too —
            // the caller will overwrite the slot with
            // `LocaGlyfState::Deferred` carrying the source bytes
            // `Arc<[u8]>`, and the load happens on the first
            // `get_or_decode_glyph` call. This avoids parsing
            // ~hundreds of KiB per face for fonts that get resolved
            // into a chain but never actually rasterized (typical
            // for fallback fonts in CSS chains).
            let has_glyf = provider.has_table(tag::GLYF) && provider.has_table(tag::LOCA);
            // Cache `has_gvar` before `provider` gets moved into
            // `allsorts::font::Font::new(provider)` further down —
            // it's the cheapest way to detect a variable font and
            // avoids the borrow-after-move that a later
            // `provider.has_table(tag::GVAR)` would incur.
            let has_gvar = provider.has_table(tag::GVAR);
            let loca_glyf_opt: Option<Arc<std::sync::Mutex<LocaGlyf>>> = if has_glyf
                && !defer_loca_glyf
            {
                match LocaGlyf::load(&provider) {
                    Ok(lg) => Some(Arc::new(std::sync::Mutex::new(lg))),
                    Err(e) => {
                        warnings.push(FontParseWarning::warning(format!(
                            "Failed to load LocaGlyf: {e} — falling back to hmtx-only"
                        )));
                        None
                    }
                }
            } else {
                None
            };

            // Lazy `glyph_cache` starts empty; the space-glyph stub
            // below pre-inserts gid 0 / space so the shaper's
            // cmap-miss fallback has something to render without
            // racing with a decode.

            let mut font_data_impl = allsorts::font::Font::new(provider).ok()?;

            // Create TrueType hinting instance from font tables.
            // [az-web-lift] Skip on the web build. The lifted layout never grid-fits glyphs to a
            // pixel raster (it measures + ships a display list to JS), so hinting is never used.
            // Building it (HintInstance::new) runs the allsorts bytecode Interpreter
            // (Interpreter::new + ::dispatch — a large un-devirt'd opcode jump table the remill
            // lift can't resolve, plus ~700 op_* fns of closure bloat). This is INDEPENDENT of the
            // lift's jump-table devirt: even with a perfect lift, web has no use for hinting, and
            // hinted advances are lower-quality output than the plain scaled advance. Native keeps
            // real hinting unchanged.
            #[cfg(feature = "web_lift")]
            let hint_instance: Option<std::sync::Mutex<allsorts::hinting::HintInstance>> = None;
            #[cfg(not(feature = "web_lift"))]
            let hint_instance = allsorts::hinting::HintInstance::new(
                &font_data_impl.font_table_provider
            ).ok().flatten().map(std::sync::Mutex::new);

            // Stash raw GSUB/GPOS bytes for lazy parse. Typical fonts
            // have ~tens of KiB of GSUB + a few-to-tens of KiB of GPOS —
            // dwarfed by glyph outlines — so we keep the bytes around
            // and only spend `LayoutTable::read` + `new_layout_cache`
            // cycles when the shaper actually needs them (via
            // `ParsedFont::gsub` / `::gpos`). For an ASCII run where no
            // substitution / kerning is required, we skip both entirely.
            let gsub_bytes = font_data_impl
                .font_table_provider
                .table_data(tag::GSUB)
                .ok()
                .flatten()
                .map(std::borrow::Cow::into_owned);
            let gpos_bytes = font_data_impl
                .font_table_provider
                .table_data(tag::GPOS)
                .ok()
                .flatten()
                .map(std::borrow::Cow::into_owned);
            let opt_gdef_table = font_data_impl.gdef_table().ok().and_then(|o| o);
            let num_glyphs = font_data_impl.num_glyphs();

            let opt_kern_table = font_data_impl
                .kern_table()
                .ok()
                .and_then(|s| s);

            let cmap_data = font_data_impl.cmap_subtable_data();
            let cmap_subtable = ReadScope::new(cmap_data);
            let cmap_subtable = cmap_subtable
                .read::<CmapSubtable<'_>>()
                .ok()
                .and_then(|s| s.to_owned());

            // Font identity hash — used by `PartialEq` for ParsedFont.
            //
            // Previously we did `font_bytes.hash(&mut hasher)` over
            // the full mmap. That touched every page of the file
            // (a 40 MiB `.ttc` walked byte-for-byte) so the "lazy
            // mmap" ended up *fully resident* the moment we built
            // a `ParsedFont`. Cold RSS jumped ~40 MiB from this
            // single line.
            //
            // The hash doesn't need to be cryptographic — it just
            // has to disambiguate two `ParsedFont`s. `(len, first
            // 4 KiB, last 4 KiB, font_index)` is plenty unique and
            // only faults in the two header / trailer pages, which
            // shaping is going to need anyway.
            let mut hasher = DefaultHasher::new();
            (font_bytes.len() as u64).hash(&mut hasher);
            let head_len = font_bytes.len().min(4096);
            font_bytes[..head_len].hash(&mut hasher);
            let tail_start = font_bytes.len().saturating_sub(4096);
            font_bytes[tail_start..].hash(&mut hasher);
            font_index.hash(&mut hasher);
            let hash = hasher.finish();

            let mut font = Self {
                hash,
                font_metrics,
                pdf_font_metrics,
                num_glyphs,
                hhea_table,
                hmtx_range,
                vmtx_range,
                vhea_table,
                maxp_table,
                gsub_bytes,
                gsub_cache_lazy: std::sync::OnceLock::new(),
                gpos_bytes,
                gpos_cache_lazy: std::sync::OnceLock::new(),
                opt_gdef_table,
                opt_kern_table,
                cmap_subtable,
                last_used: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                is_variable_font: has_gvar,
                glyph_cache: Arc::new(rust_fontconfig::StLock::new(BTreeMap::new())),
                // Eager path: `from_bytes` loaded LocaGlyf immediately
                // (or set None if the font has no loca+glyf). Lazy
                // callers use `from_bytes_shared` which replaces this
                // with `LocaGlyfState::Deferred` before returning.
                loca_glyf: LocaGlyfState::Loaded(loca_glyf_opt),
                space_width: None,
                mock: None,
                reverse_glyph_cache: BTreeMap::new(),
                // Don't retain the source bytes by default — layout and
                // raster don't need them. PDF subsetting / `to_bytes`
                // callers opt in via `with_source_bytes`.
                original_bytes: None,
                original_index: font_index,
                index_to_cid: BTreeMap::new(), // Will be filled for CFF fonts
                font_type: FontType::TrueType, // Default, will be updated if CFF
                font_name,
                hint_instance,
            };

            // Calculate space width
            let space_width = font.get_space_width_internal();

            // Pre-decode the space glyph straight into the lazy
            // `glyph_cache`. Space typically has no outline, so the
            // decoder's outline visitor returns nothing useful and
            // we'd spin re-decoding it every shape — short-circuit
            // here with a hand-rolled record carrying the hmtx
            // advance.
            let _ = (|| {
                let space_gid = font.lookup_glyph_index(' ' as u32)?;
                if let Ok(cache) = font.glyph_cache.read() {
                    if cache.contains_key(&space_gid) {
                        return None;
                    }
                }
                let space_width_val = space_width?;
                let space_record = OwnedGlyph {
                    bounding_box: OwnedGlyphBoundingBox {
                        max_x: 0,
                        max_y: 0,
                        min_x: 0,
                        min_y: 0,
                    },
                    horz_advance: space_width_val as u16,
                    outline: Vec::new(),
                    phantom_points: None,
                    raw_points: None,
                    raw_on_curve: None,
                    raw_contour_ends: None,
                    instructions: None,
                };
                if let Ok(mut cache) = font.glyph_cache.write() {
                    cache.insert(space_gid, Arc::new(space_record));
                }
                Some(())
            })();

            font.space_width = space_width;

            Some(font)
        }

        /// Attach the source font bytes to this `ParsedFont`, enabling
        /// [`ParsedFont::to_bytes`] and [`ParsedFont::subset`] (both of
        /// which the layout / shaping path never calls).
        ///
        /// Takes an `Arc<FontBytes>` so the same file's bytes can be
        /// shared across every face of a `.ttc` at zero extra cost —
        /// pair with [`rust_fontconfig::FcFontCache::get_font_bytes`].
        /// For ad-hoc PDF callers that have raw heap bytes, wrap them
        /// via `Arc::new(FontBytes::Owned(Arc::from(vec)))`.
        #[must_use]
        pub fn with_source_bytes(mut self, bytes: Arc<rust_fontconfig::FontBytes>) -> Self {
            self.original_bytes = Some(bytes);
            self
        }

        /// Lazy-friendly constructor — identical to
        /// [`ParsedFont::from_bytes`] except that `LocaGlyf` is
        /// **not** loaded during the call. Instead, the supplied
        /// `Arc<[u8]>` is retained and `LocaGlyf::load` runs the first
        /// time [`get_or_decode_glyph`] needs glyph outlines for this
        /// face.
        ///
        /// Fonts that get resolved into a CSS fallback chain but are
        /// never actually rasterized (common on desktop — e.g. every
        /// face of HelveticaNeue.ttc loads, but only one or two are
        /// shaped) then pay zero loca/glyf cost.
        ///
        /// Production callers (the reftest harness, `LayoutWindow`,
        /// `cpurender`) should prefer this constructor. Tests that
        /// inspect `glyph_records_decoded` directly and don't want
        /// a lazy path keep using `from_bytes`.
        pub fn from_bytes_shared(
            bytes: Arc<rust_fontconfig::FontBytes>,
            font_index: usize,
            warnings: &mut Vec<FontParseWarning>,
        ) -> Option<Self> {
            // Skip the eager LocaGlyf::load via `defer_loca_glyf=true`
            // — saves the load-then-drop cycle the prior arrangement
            // paid (when this called `from_bytes`, allocated
            // ~hundreds of KiB of loca+glyf bytes, then immediately
            // replaced the slot with `Deferred` and dropped them).
            // `bytes.as_ref()` derefs FontBytes → &[u8] (mmap or owned
            // — same code path).
            let mut font = Self::from_bytes_internal(bytes.as_ref(), font_index, warnings, true)?;
            font.original_bytes = Some(bytes.clone());
            font.loca_glyf = LocaGlyfState::Deferred {
                bytes,
                font_index,
                loaded: Arc::new(std::sync::Mutex::new(None)),
            };
            Some(font)
        }

        /// Resolve the current face's `LocaGlyf`, loading it lazily
        /// on first call when `loca_glyf` is `Deferred`. Returns
        /// `None` when the font has no usable loca+glyf (CFF fonts
        /// or parse failures).
        fn resolve_loca_glyf(&self) -> Option<Arc<std::sync::Mutex<LocaGlyf>>> {
            use allsorts::{
                binary::read::ReadScope,
                font_data::FontData,
                tables::FontTableProvider,
            };
            match &self.loca_glyf {
                LocaGlyfState::Loaded(inner) => inner.clone(),
                LocaGlyfState::Deferred { bytes, font_index, loaded } => {
                    // Fast path: cached LocaGlyf is present.
                    if let Ok(guard) = loaded.lock() {
                        if let Some(arc) = guard.as_ref() {
                            return Some(Arc::clone(arc));
                        }
                    }
                    let _p = crate::probe::Probe::span("resolve_loca_glyf");

                    // Slow path: parse provider + load LocaGlyf without
                    // holding the slot's lock (allsorts can take a
                    // millisecond or two on a fresh load). Re-check
                    // after acquiring the write lock so a parallel
                    // decoder doesn't double-load.
                    let scope = ReadScope::new(bytes.as_slice());
                    let font_data = scope.read::<FontData<'_>>().ok()?;
                    let provider = font_data.table_provider(*font_index).ok()?;
                    // Gate on table presence to match the `from_bytes`
                    // has_glyf check; avoids a spurious warning on
                    // CFF fonts that sneak into the Deferred path.
                    if !provider.has_table(tag::GLYF) || !provider.has_table(tag::LOCA) {
                        return None;
                    }
                    let new_arc = LocaGlyf::load(&provider)
                        .ok()
                        .map(|lg| Arc::new(std::sync::Mutex::new(lg)))?;

                    if let Ok(mut guard) = loaded.lock() {
                        if let Some(existing) = guard.as_ref() {
                            return Some(Arc::clone(existing));
                        }
                        *guard = Some(Arc::clone(&new_arc));
                    }
                    Some(new_arc)
                }
            }
        }

        /// Source bytes for PDF subsetting / table extraction.
        ///
        /// Looks in two places:
        /// - `original_bytes` (set by [`ParsedFont::with_source_bytes`]
        ///   for legacy PDF-first construction).
        /// - `LocaGlyfState::Deferred.bytes` (set by
        ///   [`ParsedFont::from_bytes_shared`] — the production lazy
        ///   path, which already retains an `Arc<[u8]>` for the lazy
        ///   loca/glyf loader).
        ///
        /// Returns `None` only for `ParsedFont`s built via the eager
        /// `from_bytes` path without an explicit `with_source_bytes`
        /// call — i.e. unit tests that load a font and don't touch
        /// PDF.
        pub fn source_bytes_for_subset(&self) -> Option<Arc<rust_fontconfig::FontBytes>> {
            if let Some(bytes) = &self.original_bytes {
                return Some(Arc::clone(bytes));
            }
            if let LocaGlyfState::Deferred { bytes, .. } = &self.loca_glyf {
                return Some(Arc::clone(bytes));
            }
            None
        }

        /// Read the monotonic-clock nanos timestamp of the most
        /// recent [`get_or_decode_glyph`] call on this face, or `0`
        /// if it's never been touched.
        pub fn last_used_nanos(&self) -> u64 {
            self.last_used.load(std::sync::atomic::Ordering::Relaxed)
        }

        /// Drop the cached `LocaGlyf` for this face if it's
        /// `Deferred`-with-bytes-retained — so the next
        /// [`get_or_decode_glyph`] re-parses from `bytes`. No-op for
        /// `Loaded` faces (no source bytes to fall back to).
        ///
        /// Used by [`crate::text3::cache::FontManager::evict_unused`]
        /// and exposed publicly so embedders can free memory under
        /// pressure on fonts they no longer need to render.
        pub fn evict_loca_glyf(&self) -> bool {
            match &self.loca_glyf {
                LocaGlyfState::Deferred { loaded, .. } => {
                    if let Ok(mut guard) = loaded.lock() {
                        if guard.is_some() {
                            *guard = None;
                            return true;
                        }
                    }
                    false
                }
                LocaGlyfState::Loaded(_) => false,
            }
        }

        /// Fetch the parsed GSUB cache if this font has one, parsing
        /// it from the retained `gsub_bytes` on first access.
        ///
        /// Moved out of the eager `from_bytes` path because most text
        /// runs never trigger GSUB — plain ASCII without ligatures is
        /// handled entirely by the cmap + hmtx fast path. Building
        /// `LayoutCacheData<GSUB>` up front reserved ~0.5–2 MiB per
        /// face just to throw it away on pages that don't shape
        /// complex scripts.
        pub fn gsub(&self) -> Option<&GsubCache> {
            self.gsub_cache_lazy
                .get_or_init(|| {
                    use allsorts::{
                        binary::read::ReadScope,
                        layout::{new_layout_cache, LayoutTable, GSUB},
                    };
                    let bytes = self.gsub_bytes.as_ref()?;
                    ReadScope::new(bytes)
                        .read::<LayoutTable<GSUB>>()
                        .ok()
                        .map(new_layout_cache)
                })
                .as_ref()
        }

        /// Fetch the parsed GPOS cache if this font has one, parsing
        /// it from the retained `gpos_bytes` on first access. See
        /// [`ParsedFont::gsub`] for the motivation.
        pub fn gpos(&self) -> Option<&GposCache> {
            self.gpos_cache_lazy
                .get_or_init(|| {
                    use allsorts::{
                        binary::read::ReadScope,
                        layout::{new_layout_cache, LayoutTable, GPOS},
                    };
                    let bytes = self.gpos_bytes.as_ref()?;
                    ReadScope::new(bytes)
                        .read::<LayoutTable<GPOS>>()
                        .ok()
                        .map(new_layout_cache)
                })
                .as_ref()
        }

        /// Fetch an `OwnedGlyph` for `gid`, decoding it on first access.
        ///
        /// Cached in the `Arc<RwLock<…>>` `glyph_cache` so subsequent
        /// calls (including across clones of this `ParsedFont`) hit the
        /// cache. Returns `None` when `gid >= num_glyphs` or the font
        /// has no loca+glyf and no hmtx entry for the glyph. For CFF
        /// fonts the returned record has an empty outline and an advance
        /// pulled from hmtx — matching the pre-lazy behaviour.
        ///
        /// Called on the rasterizer hot path; performance budget is a
        /// few µs per unique glyph (first hit) and an Arc bump + `BTreeMap`
        /// lookup (cache hits). The write lock is held only across the
        /// decode, not across the caller's use of the returned Arc.
        pub fn get_or_decode_glyph(&self, gid: u16) -> Option<Arc<OwnedGlyph>> {
            use std::sync::Arc;
            if usize::from(gid) >= self.num_glyphs as usize {
                return None;
            }
            // Bump the LRU timestamp so `FontManager::evict_unused`
            // can tell this face is still in use. Cheap atomic store
            // (Relaxed — eviction reads the same atomic and tolerates
            // a slightly stale value, which only causes "evict, then
            // re-load on next access" — never an incorrect render).
            self.last_used
                .store(monotonic_now_nanos(), std::sync::atomic::Ordering::Relaxed);

            // Fast path: cache hit.
            if let Ok(cache) = self.glyph_cache.read() {
                if let Some(existing) = cache.get(&gid) {
                    return Some(Arc::clone(existing));
                }
            }

            // Miss: decode. We drop the read lock before taking the
            // write lock to avoid deadlock, and we re-check on the way
            // in because another thread may have decoded the same glyph
            // in between.
            let record = self.decode_glyph_inner(gid);
            let arc = Arc::new(record);
            if let Ok(mut cache) = self.glyph_cache.write() {
                cache
                    .entry(gid)
                    .or_insert_with(|| Arc::clone(&arc));
                // If another thread beat us to the insert, return theirs
                // so all callers observe the same Arc.
                if let Some(winner) = cache.get(&gid) {
                    return Some(Arc::clone(winner));
                }
            }
            Some(arc)
        }

        /// Eagerly decode every glyph into the lazy `glyph_cache`,
        /// restoring the pre-lazy "every glyph is materialised at
        /// construction time" behaviour. Used by tests that iterate
        /// or compare against reference tooling, and by embedders
        /// that want a walkable view without driving every shape
        /// through `get_or_decode_glyph`.
        ///
        /// After `prime_glyph_cache`, callers can use
        /// [`ParsedFont::for_each_decoded_glyph`] or
        /// [`ParsedFont::glyph_cache_snapshot`] to observe the
        /// populated cache.
        #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        pub fn prime_glyph_cache(&mut self) {
            let n = self.num_glyphs as usize;
            for glyph_index in 0..n {
                let gid = glyph_index as u16;
                drop(self.get_or_decode_glyph(gid));
            }
        }

        /// Walk every entry currently in the lazy `glyph_cache`,
        /// invoking `f(gid, &OwnedGlyph)` for each. Holds a read
        /// lock for the duration; do not call back into the font
        /// from `f`. The cache is populated on demand by
        /// [`ParsedFont::get_or_decode_glyph`] (and bulk-prefilled
        /// by [`ParsedFont::prime_glyph_cache`]).
        pub fn for_each_decoded_glyph<F: FnMut(u16, &OwnedGlyph)>(&self, mut f: F) {
            if let Ok(cache) = self.glyph_cache.read() {
                for (gid, glyph) in cache.iter() {
                    f(*gid, glyph.as_ref());
                }
            }
        }

        /// Snapshot of the currently-decoded glyphs as a
        /// `BTreeMap<u16, Arc<OwnedGlyph>>`. Cheap (clones the
        /// Arcs, not the records). Used by callers that want to
        /// hand the map off across an API boundary; for in-place
        /// iteration prefer [`ParsedFont::for_each_decoded_glyph`].
        pub fn glyph_cache_snapshot(&self) -> BTreeMap<u16, Arc<OwnedGlyph>> {
            self.glyph_cache
                .read()
                .map(|c| c.clone())
                .unwrap_or_default()
        }

        /// Core decode routine: produces one `OwnedGlyph` for `gid` by
        /// locking `loca_glyf` and running allsorts' outline visitor +
        /// raw-simple-glyph extraction. Factored out so both
        /// [`get_or_decode_glyph`] and [`prime_glyph_cache`] share it.
        ///
        /// Always returns an `OwnedGlyph` — if anything in the decode
        /// chain fails, falls back to an empty-outline record with the
        /// `hmtx` advance. This mirrors the pre-lazy behaviour where
        /// every gid ended up in `glyph_records_decoded`.
        fn hmtx_bytes(&self) -> &[u8] {
            let (off, len) = self.hmtx_range;
            if len == 0 { return &[]; }
            self.original_bytes.as_ref()
                .map_or(&[], |b| &b.as_ref()[off..off+len])
        }

        fn vmtx_bytes(&self) -> &[u8] {
            let (off, len) = self.vmtx_range;
            if len == 0 { return &[]; }
            self.original_bytes.as_ref()
                .map_or(&[], |b| &b.as_ref()[off..off+len])
        }

        #[allow(clippy::cast_possible_wrap)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        fn decode_glyph_inner(&self, gid: u16) -> OwnedGlyph {
            let _p = crate::probe::Probe::span("decode_glyph");
            // [az-web-lift] use get_horizontal_advance (reads hmtx directly on the web build)
            // instead of allsorts::glyph_info::advance, whose lifted ReadArray parse has an
            // un-devirt'd jump table → MISSING_BLOCK → OOB during measure.
            let horz_advance = self.get_horizontal_advance(gid);

            let mut record = OwnedGlyph {
                horz_advance,
                bounding_box: OwnedGlyphBoundingBox {
                    min_x: 0,
                    min_y: 0,
                    max_x: horz_advance as i16,
                    max_y: 0,
                },
                outline: Vec::new(),
                phantom_points: None,
                raw_points: None,
                raw_on_curve: None,
                raw_contour_ends: None,
                instructions: None,
            };

            // Resolve the `LocaGlyf` for this face. For `Loaded` that's
            // a cheap `Arc::clone`; for `Deferred` this is where the
            // actual `LocaGlyf::load` happens on first access, paid once
            // per face that ever decodes a glyph.
            let Some(loca_glyf_arc) = self.resolve_loca_glyf() else {
                // No usable loca+glyf → CFF / OpenType-PostScript font
                // (Noto Sans/Serif CJK and most .otf). Decode the glyph
                // from the `CFF ` table instead; the TrueType-only glyf
                // path below can't see these, which left every CFF glyph
                // blank on the cpurender/headless path (CJK rendered as
                // empty space with the hmtx advance still reserved).
                self.decode_cff_glyph_into(gid, &mut record);
                return record;
            };
            let Ok(mut loca_glyf) = loca_glyf_arc.lock() else {
                return record;
            };

            // Visit the outline. If this is a variable font (gvar
            // table present) AND we still have source bytes (only
            // the `LocaGlyfState::Deferred` path retains them), we
            // re-derive a `VariableGlyfContext` here so default-
            // instance vs designed-instance differences land in
            // the decoded outline. The chained `if let` pattern
            // keeps `provider` and `store` in scope for the
            // visit, which the borrow checker requires (the
            // store's `Cow::Borrowed(&[u8])` tables tie its
            // lifetime to the provider).
            //
            // Eager-`from_bytes` faces (no retained bytes) and
            // non-variable fonts skip the var-context machinery
            // and decode the default instance — same behaviour as
            // before R4.
            // [az-web-lift] The lifted web layout NEVER rasterizes (it measures + positions, then
            // ships a display list to JS) — so glyph OUTLINES + TrueType hinting raw-points are
            // never needed in wasm. Decoding them (allsorts GlyfVisitorContext::visit +
            // GlyphOutlineCollector::into_outlines, whose GlyphOutlineOperation match is a 5-arm
            // jump table the remill lift doesn't devirtualize → MISSING_BLOCK → OOB) crashes the
            // measure pass. Skip BOTH decode passes on the web build; the record keeps its hmtx
            // advance/metrics (set above) which is all text measurement needs.
            if !cfg!(feature = "web_lift") {
            let mut outline_done = false;
            if self.is_variable_font {
                if let LocaGlyfState::Deferred { bytes, .. } = &self.loca_glyf {
                    let scope = ReadScope::new(bytes);
                    if let Ok(font_data) =
                        scope.read::<FontData<'_>>()
                    {
                        if let Ok(provider) = font_data.table_provider(self.original_index) {
                            if let Ok(store) = VariableGlyfContextStore::read(&provider) {
                                if let Ok(var_ctx) = VariableGlyfContext::new(&store) {
                                    let mut visitor = GlyfVisitorContext::new(
                                        &mut loca_glyf,
                                        Some(var_ctx),
                                    );
                                    let mut collector = GlyphOutlineCollector::new();
                                    if visitor.visit(gid, None, &mut collector).is_ok() {
                                        record.outline = collector.into_outlines();
                                        let (min_x, min_y, max_x, max_y) =
                                            compute_outline_bbox(&record.outline);
                                        record.bounding_box = OwnedGlyphBoundingBox {
                                            min_x,
                                            min_y,
                                            max_x,
                                            max_y,
                                        };
                                        outline_done = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !outline_done {
                let mut visitor =
                    GlyfVisitorContext::new(&mut loca_glyf, None);
                let mut collector = GlyphOutlineCollector::new();
                if visitor.visit(gid, None, &mut collector).is_ok() {
                    record.outline = collector.into_outlines();
                    let (min_x, min_y, max_x, max_y) =
                        compute_outline_bbox(&record.outline);
                    record.bounding_box = OwnedGlyphBoundingBox {
                        min_x,
                        min_y,
                        max_x,
                        max_y,
                    };
                }
            }

            // Second pass: pull raw SimpleGlyph data for TrueType
            // bytecode hinting. LocaGlyf caches the `Arc<Glyph>`
            // internally so this lookup is cheap after the first call.
            if let Ok(glyph_arc) = loca_glyf.glyph(gid) {
                if let allsorts::tables::glyf::Glyph::Simple(sg) = glyph_arc.as_ref() {
                    record.raw_points = Some(
                        sg.coordinates.iter().map(|(_, pt)| (pt.0, pt.1)).collect(),
                    );
                    record.raw_on_curve = Some(
                        sg.coordinates.iter().map(|(f, _)| f.is_on_curve()).collect(),
                    );
                    record.raw_contour_ends = Some(sg.end_pts_of_contours.clone());
                    record.instructions = Some(sg.instructions.to_vec());
                }
            }
            } // [az-web-lift] end skip glyph outline/hinting decode on web

            record
        }

        /// Decode a single glyph outline from the `CFF ` (OpenType
        /// PostScript) table into `record`. Used for fonts with no `glyf`
        /// table — `decode_glyph_inner`'s TrueType path returns an empty
        /// outline for them, so without this every CFF glyph rasterised as
        /// blank on the CPU renderer. Notably this hit ALL CJK text: the
        /// installed Noto Sans/Serif CJK fonts are CID-keyed CFF. allsorts'
        /// `CFFOutlines` feeds the same `GlyphOutlineCollector` the glyf
        /// path uses and resolves CID-keyed local subrs internally.
        fn decode_cff_glyph_into(&self, gid: u16, record: &mut OwnedGlyph) {
            use allsorts::cff::{outline::CFFOutlines, CFF};

            let Some(ref original) = self.original_bytes else {
                return;
            };
            let bytes: &[u8] = original.as_slice();
            let Ok(font_data) = ReadScope::new(bytes).read::<FontData<'_>>() else {
                return;
            };
            let Ok(provider) = font_data.table_provider(self.original_index) else {
                return;
            };
            let Ok(Some(cff_data)) = provider.table_data(tag::CFF) else {
                return;
            };
            let Ok(cff) = ReadScope::new(&cff_data).read::<CFF<'_>>() else {
                return;
            };
            let mut outlines = CFFOutlines { table: &cff };
            let mut collector = GlyphOutlineCollector::new();
            if outlines.visit(gid, None, &mut collector).is_ok() {
                record.outline = collector.into_outlines();
                let (min_x, min_y, max_x, max_y) = compute_outline_bbox(&record.outline);
                record.bounding_box = OwnedGlyphBoundingBox {
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                };
            }
        }

        /// Parse PDF-specific font metrics from HEAD, HHEA, and OS/2 tables
        fn parse_pdf_font_metrics(
            font_bytes: &[u8],
            font_index: usize,
            head_table: &allsorts::tables::HeadTable,
            hhea_table: &HheaTable,
        ) -> PdfFontMetrics {
            use allsorts::{
                binary::read::ReadScope,
                font_data::FontData,
                tables::{os2::Os2, FontTableProvider},
                tag,
            };

            let scope = ReadScope::new(font_bytes);
            let font_file = scope.read::<FontData<'_>>().ok();
            let provider = font_file
                .as_ref()
                .and_then(|ff| ff.table_provider(font_index).ok());

            let os2_table = provider
                .as_ref()
                .and_then(|p| p.table_data(tag::OS_2).ok())
                .and_then(|os2_data| {
                    let data = os2_data?;
                    let scope = ReadScope::new(&data);
                    scope.read_dep::<Os2>(data.len()).ok()
                });

            // Base metrics from HEAD and HHEA (always present)
            let base = PdfFontMetrics {
                units_per_em: head_table.units_per_em,
                font_flags: head_table.flags,
                x_min: head_table.x_min,
                y_min: head_table.y_min,
                x_max: head_table.x_max,
                y_max: head_table.y_max,
                ascender: hhea_table.ascender,
                descender: hhea_table.descender,
                line_gap: hhea_table.line_gap,
                advance_width_max: hhea_table.advance_width_max,
                caret_slope_rise: hhea_table.caret_slope_rise,
                caret_slope_run: hhea_table.caret_slope_run,
                ..PdfFontMetrics::zero()
            };

            // Add OS/2 metrics if available
            os2_table
                .map_or(base, |os2| PdfFontMetrics {
                    x_avg_char_width: os2.x_avg_char_width,
                    us_weight_class: os2.us_weight_class,
                    us_width_class: os2.us_width_class,
                    y_strikeout_size: os2.y_strikeout_size,
                    y_strikeout_position: os2.y_strikeout_position,
                    ..base
                })
        }

        /// Returns the width of the space character in font units.
        ///
        /// This is used internally for text layout calculations.
        /// Returns `None` if the font has no space glyph or its width cannot be determined.
        fn get_space_width_internal(&self) -> Option<usize> {
            if let Some(mock) = self.mock.as_ref() {
                return mock.space_width;
            }
            let glyph_index = self.lookup_glyph_index(' ' as u32)?;

            // [az-web-lift] use get_horizontal_advance (direct hmtx on web) instead of
            // allsorts::glyph_info::advance (un-devirt'd jump table → OOB).
            Some(self.get_horizontal_advance(glyph_index) as usize)
        }

        /// Look up the glyph index for a Unicode codepoint
        pub fn lookup_glyph_index(&self, codepoint: u32) -> Option<u16> {
            let cmap = self.cmap_subtable.as_ref()?;
            cmap.map_glyph(codepoint).ok().flatten()
        }

        /// Get the horizontal advance width for a glyph in font units.
        ///
        /// Pulled straight from the `hmtx` table — no glyph-outline
        /// decode. Called once per shaped glyph per layout pass, so
        /// avoiding the lazy decode here is a meaningful win over
        /// routing through `get_or_decode_glyph`.
        pub fn get_horizontal_advance(&self, glyph_index: u16) -> u16 {
            if let Some(mock) = self.mock.as_ref() {
                return mock.glyph_advances.get(&glyph_index).copied().unwrap_or(0);
            }
            // [az-web-lift] Read the hmtx advance DIRECTLY (a plain longHorMetric table lookup)
            // instead of allsorts::glyph_info::advance, whose lifted binary `ReadArray` parse has
            // an un-devirt'd jump table → MISSING_BLOCK → OOB during text measure. Identical result
            // for non-variable fonts (the web fallback font is non-variable); native keeps the
            // allsorts path (variable-font deltas etc.).
            #[cfg(feature = "web_lift")]
            {
                let hmtx = self.hmtx_bytes();
                let num = usize::from(self.hhea_table.num_h_metrics);
                if num == 0 {
                    return 0;
                }
                let idx = (glyph_index as usize).min(num - 1);
                let off = idx * 4;
                return if off + 2 <= hmtx.len() {
                    ((hmtx[off] as u16) << 8) | (hmtx[off + 1] as u16)
                } else {
                    0
                };
            }
            #[cfg(not(feature = "web_lift"))]
            {
                allsorts::glyph_info::advance(
                    &self.maxp_table,
                    &self.hhea_table,
                    self.hmtx_bytes(),
                    glyph_index,
                )
                .unwrap_or_default()
            }
        }

        /// Get the hinted advance width in pixels for a glyph at the given ppem.
        ///
        /// For glyphs with outlines, runs TrueType bytecode hinting to get the
        /// grid-fitted advance from phantom points. For glyphs without outlines
        /// (e.g. space), rounds the scaled advance to the pixel grid, matching
        /// `FreeType`'s behavior.
        ///
        /// Returns `None` if hinting is not available or fails.
        #[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        pub fn get_hinted_advance_px(&self, glyph_index: u16, ppem: u16) -> Option<f32> {
            // [az-web-lift] No pixel grid-fitting on the web (measure-only): return None so the
            // caller falls back to the plain scaled advance. Hard-cfg (not a runtime `if cfg!`)
            // so the whole hinting body — get_or_decode_glyph's outline path AND set_ppem →
            // allsorts Interpreter::dispatch (opcode jump table → OOB) — is removed from the lift
            // closure entirely. SEPARATE concern from the transpiler's jump-table devirt: web has
            // no use for hinted advances regardless of lift quality. Native is unchanged.
            #[cfg(feature = "web_lift")]
            {
                let _ = (glyph_index, ppem);
                None
            }
            #[cfg(not(feature = "web_lift"))]
            {
            use allsorts::hinting::f26dot6::{compute_scale, F26Dot6};
            let glyph = self.get_or_decode_glyph(glyph_index)?;

            let upem = self.font_metrics.units_per_em;
            if upem == 0 || ppem == 0 {
                return None;
            }

            // Check if we even have a hint instance
            let hint_mutex = self.hint_instance.as_ref()?;

            let scale = compute_scale(ppem, upem);
            let adv_f26dot6 = F26Dot6::from_funits(i32::from(glyph.horz_advance), scale);

            // For glyphs with outline data, run bytecode hinting
            if let (Some(raw_points), Some(raw_on_curve), Some(raw_contour_ends)) = (
                glyph.raw_points.as_ref(),
                glyph.raw_on_curve.as_ref(),
                glyph.raw_contour_ends.as_ref(),
            ) {
                let instructions = glyph.instructions.as_deref().unwrap_or(&[]);
                let mut hint = hint_mutex.lock().ok()?;
                hint.set_ppem(ppem, f64::from(ppem)).ok()?;
                drop(hint);

                let points_f26dot6: Vec<(i32, i32)> = raw_points
                    .iter()
                    .map(|&(x, y)| {
                        let sx = F26Dot6::from_funits(i32::from(x), scale);
                        let sy = F26Dot6::from_funits(i32::from(y), scale);
                        (sx.to_bits(), sy.to_bits())
                    })
                    .collect();
            }

            // Use the scaled advance rounded to pixel grid, NOT the hinted
            // phantom point.  Some glyph programs apply ClearType-specific SHPIX
            // adjustments to the advance phantom point that are wrong for
            // non-ClearType rendering.  The rounded scaled advance matches
            // FreeType's DEFAULT mode advance output (and, for glyphs without an
            // outline such as space, FreeType's phantom-point pre-rounding).
            let rounded = (adv_f26dot6.to_bits() + 32) & !63;
            Some(rounded as f32 / 64.0)
            } // [az-web-lift] end #[cfg(not(web_lift))] hinting body
        }

        /// Get the number of glyphs in this font
        pub const fn num_glyphs(&self) -> u16 {
            self.num_glyphs
        }

        /// Check if this font has a glyph for the given codepoint
        pub fn has_glyph(&self, codepoint: u32) -> bool {
            self.lookup_glyph_index(codepoint).is_some()
        }

        /// Get vertical metrics for a glyph (for vertical text layout).
        ///
        /// Uses vhea+vmtx tables (same binary format as hhea+hmtx).
        /// Returns None if font has no vertical metrics tables.
        #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
        pub fn get_vertical_metrics(
            &self,
            glyph_id: u16,
        ) -> Option<crate::text3::cache::VerticalMetrics> {
            let vhea = self.vhea_table.as_ref()?;
            if self.vmtx_range.1 == 0 {
                return None;
            }
            let vert_advance = f32::from(allsorts::glyph_info::advance(
                &self.maxp_table, vhea, self.vmtx_bytes(), glyph_id,
            ).ok()?);

            let units_per_em = f32::from(self.font_metrics.units_per_em);
            let scale = if units_per_em > 0.0 { 1.0 / units_per_em } else { 0.001 };

            // Vertical bearing: approximate from glyph bbox if available
            let (bearing_x, bearing_y) = self.get_or_decode_glyph(glyph_id)
                .map_or((0.0, 0.0), |g| {
                    let bbox = &g.bounding_box;
                    // tsb (top side bearing): origin_y - max_y
                    // lsb for vertical: center the glyph horizontally
                    let width = f32::from(bbox.max_x - bbox.min_x);
                    (-(width / 2.0) * scale, (vert_advance * scale) - (f32::from(bbox.max_y) * scale))
                });

            Some(crate::text3::cache::VerticalMetrics {
                advance: vert_advance * scale,
                bearing_x,
                bearing_y,
                origin_y: self.font_metrics.ascent * scale,
            })
        }

        /// Get layout-specific font metrics
        #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
        pub fn get_font_metrics(&self) -> LayoutFontMetrics {
            // Ensure descent is positive (OpenType may have negative descent)
            let descent = if self.font_metrics.descent > 0.0 {
                self.font_metrics.descent
            } else {
                -self.font_metrics.descent
            };

            LayoutFontMetrics {
                ascent: self.font_metrics.ascent,
                descent,
                line_gap: self.font_metrics.line_gap,
                units_per_em: self.font_metrics.units_per_em,
                x_height: self.font_metrics.x_height,
                cap_height: self.font_metrics.cap_height,
            }
        }

        /// Convert the `ParsedFont` back to bytes using `allsorts::whole_font`
        /// This reconstructs the entire font from the parsed data
        ///
        /// Source bytes come from either the explicit
        /// [`ParsedFont::with_source_bytes`] handle (PDF-first
        /// construction) *or* the `LocaGlyfState::Deferred` slot
        /// installed by [`ParsedFont::from_bytes_shared`]. The
        /// production lazy path retains bytes for the lazy `LocaGlyf`
        /// loader, so PDF subsetting Just Works without an extra
        /// `with_source_bytes` call.
        ///
        /// # Arguments
        /// * `tags` - Optional list of specific table tags to include (None = all tables)
        /// # Errors
        ///
        /// Returns an error string if serializing the font fails.
        pub fn to_bytes(&self, tags: Option<&[u32]>) -> Result<Vec<u8>, String> {
            let source = self.source_bytes_for_subset().ok_or_else(|| {
                "ParsedFont::to_bytes requires source bytes; construct via \
                 ParsedFont::from_bytes_shared (production lazy path) or \
                 attach via ParsedFont::with_source_bytes"
                    .to_string()
            })?;
            let scope = ReadScope::new(source.as_slice());
            let font_file = scope.read::<FontData<'_>>().map_err(|e| e.to_string())?;
            let provider = font_file
                .table_provider(self.original_index)
                .map_err(|e| e.to_string())?;

            let tags_to_use = tags.unwrap_or(&[
                tag::CMAP,
                tag::HEAD,
                tag::HHEA,
                tag::HMTX,
                tag::MAXP,
                tag::NAME,
                tag::OS_2,
                tag::POST,
                tag::GLYF,
                tag::LOCA,
            ]);

            whole_font(&provider, tags_to_use).map_err(|e| e.to_string())
        }

        /// Create a subset font containing only the specified glyph IDs
        /// Returns the subset font bytes and a mapping from old to new glyph IDs
        ///
        /// # Arguments
        /// * `glyph_ids` - The glyph IDs to include in the subset (glyph 0/.notdef is always
        ///   included)
        /// * `cmap_target` - Target cmap format (Unicode for web, `MacRoman` for compatibility)
        ///
        /// # Returns
        /// A tuple of (`subset_font_bytes`, `glyph_mapping`) where `glyph_mapping` maps
        /// `original_glyph_id` -> (`new_glyph_id`, `original_char`)
        #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
        /// # Errors
        ///
        /// Returns an error string if subsetting the font fails.
        pub fn subset(
            &self,
            glyph_ids: &[(u16, char)],
            cmap_target: CmapTarget,
        ) -> Result<(Vec<u8>, BTreeMap<u16, (u16, char)>), String> {
            let source = self.source_bytes_for_subset().ok_or_else(|| {
                "ParsedFont::subset requires source bytes; construct via \
                 ParsedFont::from_bytes_shared (production lazy path) or \
                 attach via ParsedFont::with_source_bytes"
                    .to_string()
            })?;
            let scope = ReadScope::new(source.as_slice());
            let font_file = scope.read::<FontData<'_>>().map_err(|e| e.to_string())?;
            let provider = font_file
                .table_provider(self.original_index)
                .map_err(|e| e.to_string())?;

            // Build glyph mapping: original_id -> (new_id, char)
            let glyph_mapping: BTreeMap<u16, (u16, char)> = glyph_ids
                .iter()
                .enumerate()
                .map(|(new_id, &(original_id, ch))| (original_id, (new_id as u16, ch)))
                .collect();

            // Extract just the glyph IDs for subsetting
            let ids: Vec<u16> = glyph_ids.iter().map(|(id, _)| *id).collect();

            // Use PDF profile for embedding fonts in PDFs
            let font_bytes = allsorts_subset(&provider, &ids, &SubsetProfile::Pdf, cmap_target)
                .map_err(|e| format!("Subset error: {e:?}"))?;

            Ok((font_bytes, glyph_mapping))
        }

        /// Get the width of a glyph in font units (internal, unscaled)
        pub fn get_glyph_width_internal(&self, glyph_index: u16) -> Option<usize> {
            allsorts::glyph_info::advance(
                &self.maxp_table,
                &self.hhea_table,
                self.hmtx_bytes(),
                glyph_index,
            )
            .ok()
            .map(|s| s as usize)
        }

        /// Get the width of the space character (unscaled font units)
        #[inline]
        pub const fn get_space_width(&self) -> Option<usize> {
            self.space_width
        }

        /// Add glyph-to-text mapping to reverse cache
        /// This should be called during text shaping when we know both the source text and
        /// resulting glyphs
        pub fn cache_glyph_mapping(&mut self, glyph_id: u16, cluster_text: &str) {
            self.reverse_glyph_cache
                .insert(glyph_id, cluster_text.to_string());
        }

        /// Get the cluster text that produced a specific glyph ID
        /// Returns the original text that was shaped into this glyph (handles ligatures correctly)
        pub fn get_glyph_cluster_text(&self, glyph_id: u16) -> Option<&str> {
            self.reverse_glyph_cache.get(&glyph_id).map(String::as_str)
        }

        /// Get the first character from the cluster text for a glyph ID
        /// This is useful for PDF `ToUnicode` `CMap` generation which requires single character
        /// mappings
        pub fn get_glyph_primary_char(&self, glyph_id: u16) -> Option<char> {
            self.reverse_glyph_cache
                .get(&glyph_id)
                .and_then(|text| text.chars().next())
        }

        /// Clear the reverse glyph cache (useful for memory management)
        pub fn clear_glyph_cache(&mut self) {
            self.reverse_glyph_cache.clear();
        }

        /// Get the bounding box size of a glyph (unscaled units) - for PDF
        /// Returns (width, height) in font units
        pub fn get_glyph_bbox_size(&self, glyph_index: u16) -> Option<(i32, i32)> {
            let g = self.get_or_decode_glyph(glyph_index)?;
            let glyph_width = i32::from(g.horz_advance);
            let glyph_height = i32::from(g.bounding_box.max_y) - i32::from(g.bounding_box.min_y);
            Some((glyph_width, glyph_height))
        }
    }

    /// Compute the bounding box from collected glyph outlines.
    fn compute_outline_bbox(outlines: &[GlyphOutline]) -> (i16, i16, i16, i16) {
        let mut min_x = i16::MAX;
        let mut min_y = i16::MAX;
        let mut max_x = i16::MIN;
        let mut max_y = i16::MIN;
        let mut has_points = false;

        for outline in outlines {
            for op in outline.operations.as_slice() {
                let points: &[(i16, i16)] = match op {
                    GlyphOutlineOperation::MoveTo(m) => &[(m.x, m.y)],
                    GlyphOutlineOperation::LineTo(l) => &[(l.x, l.y)],
                    GlyphOutlineOperation::QuadraticCurveTo(q) => {
                        // Check both control and end point for bbox
                        min_x = min_x.min(q.ctrl_1_x).min(q.end_x);
                        min_y = min_y.min(q.ctrl_1_y).min(q.end_y);
                        max_x = max_x.max(q.ctrl_1_x).max(q.end_x);
                        max_y = max_y.max(q.ctrl_1_y).max(q.end_y);
                        has_points = true;
                        continue;
                    }
                    GlyphOutlineOperation::CubicCurveTo(c) => {
                        min_x = min_x.min(c.ctrl_1_x).min(c.ctrl_2_x).min(c.end_x);
                        min_y = min_y.min(c.ctrl_1_y).min(c.ctrl_2_y).min(c.end_y);
                        max_x = max_x.max(c.ctrl_1_x).max(c.ctrl_2_x).max(c.end_x);
                        max_y = max_y.max(c.ctrl_1_y).max(c.ctrl_2_y).max(c.end_y);
                        has_points = true;
                        continue;
                    }
                    GlyphOutlineOperation::ClosePath => continue,
                };
                for &(x, y) in points {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                    has_points = true;
                }
            }
        }

        if has_points {
            (min_x, min_y, max_x, max_y)
        } else {
            (0, 0, 0, 0)
        }
    }

    #[derive(Debug, Clone)]
    pub struct OwnedGlyph {
        pub bounding_box: OwnedGlyphBoundingBox,
        pub horz_advance: u16,
        pub outline: Vec<GlyphOutline>,
        pub phantom_points: Option<[Point; 4]>,
        /// Raw TrueType points in font units (for hinting). None for composite/CFF glyphs.
        pub raw_points: Option<Vec<(i16, i16)>>,
        /// On-curve flags for each raw point.
        pub raw_on_curve: Option<Vec<bool>>,
        /// Contour end-point indices (TrueType).
        pub raw_contour_ends: Option<Vec<u16>>,
        /// Per-glyph TrueType hinting instructions.
        pub instructions: Option<Vec<u8>>,
    }

    // --- ParsedFontTrait Implementation for ParsedFont ---

    impl crate::text3::cache::ShallowClone for ParsedFont {
        fn shallow_clone(&self) -> Self {
            self.clone() // ParsedFont::clone uses Arc internally, so it's shallow
        }
    }

    impl crate::text3::cache::ParsedFontTrait for ParsedFont {
        fn shape_text(
            &self,
            text: &str,
            script: crate::font_traits::Script,
            language: crate::font_traits::Language,
            direction: crate::font_traits::BidiDirection,
            style: &crate::font_traits::StyleProperties,
        ) -> Result<Vec<crate::font_traits::Glyph>, crate::font_traits::LayoutError> {
            // Call the existing shape_text_for_parsed_font method (defined in default.rs)
            crate::text3::default::shape_text_for_parsed_font(
                self, text, script, language, direction, style,
            )
        }

        fn get_hash(&self) -> u64 {
            self.hash
        }

        fn get_glyph_size(
            &self,
            glyph_id: u16,
            font_size_px: f32,
        ) -> Option<azul_core::geom::LogicalSize> {
            self.get_or_decode_glyph(glyph_id).map(|record| {
                let units_per_em = f32::from(self.font_metrics.units_per_em);
                let scale_factor = if units_per_em > 0.0 {
                    font_size_px / units_per_em
                } else {
                    0.01
                };
                let bbox = &record.bounding_box;
                azul_core::geom::LogicalSize {
                    width: f32::from(bbox.max_x - bbox.min_x) * scale_factor,
                    height: f32::from(bbox.max_y - bbox.min_y) * scale_factor,
                }
            })
        }

        fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
            let glyph_id = self.lookup_glyph_index('-' as u32)?;
            let advance_units = self.get_horizontal_advance(glyph_id);
            let scale_factor = if self.font_metrics.units_per_em > 0 {
                font_size / f32::from(self.font_metrics.units_per_em)
            } else {
                return None;
            };
            let scaled_advance = f32::from(advance_units) * scale_factor;
            Some((glyph_id, scaled_advance))
        }

        fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
            let glyph_id = self.lookup_glyph_index('\u{0640}' as u32)?;
            let advance_units = self.get_horizontal_advance(glyph_id);
            let scale_factor = if self.font_metrics.units_per_em > 0 {
                font_size / f32::from(self.font_metrics.units_per_em)
            } else {
                return None;
            };
            let scaled_advance = f32::from(advance_units) * scale_factor;
            Some((glyph_id, scaled_advance))
        }

        fn has_glyph(&self, codepoint: u32) -> bool {
            self.lookup_glyph_index(codepoint).is_some()
        }

        fn get_vertical_metrics(
            &self,
            glyph_id: u16,
        ) -> Option<crate::text3::cache::VerticalMetrics> {
            self.get_vertical_metrics(glyph_id)
        }

        fn get_font_metrics(&self) -> LayoutFontMetrics {
            self.font_metrics.clone()
        }

        fn num_glyphs(&self) -> u16 {
            self.num_glyphs
        }

        fn get_space_width(&self) -> Option<usize> {
            self.space_width
        }
    }

    /// Build an agg-rust `PathStorage` from an `OwnedGlyph` outline (in font units, Y-up → Y-down).
    ///
    /// Returns `None` if the glyph has no outline operations (e.g. space).
    /// The caller is responsible for applying scale and translation transforms.
    #[cfg(feature = "cpurender")]
    #[must_use] pub fn build_glyph_path(glyph: &OwnedGlyph) -> Option<agg_rust::path_storage::PathStorage> {
        use agg_rust::{basics::PATH_FLAGS_NONE, path_storage::PathStorage};

        let mut path = PathStorage::new();
        let mut has_ops = false;
        for outline in &glyph.outline {
            for op in outline.operations.as_slice() {
                has_ops = true;
                match op {
                    GlyphOutlineOperation::MoveTo(OutlineMoveTo { x, y }) => {
                        path.move_to(f64::from(*x), -f64::from(*y));
                    }
                    GlyphOutlineOperation::LineTo(OutlineLineTo { x, y }) => {
                        path.line_to(f64::from(*x), -f64::from(*y));
                    }
                    GlyphOutlineOperation::QuadraticCurveTo(OutlineQuadTo {
                        ctrl_1_x, ctrl_1_y, end_x, end_y,
                    }) => {
                        path.curve3(
                            f64::from(*ctrl_1_x), -f64::from(*ctrl_1_y),
                            f64::from(*end_x), -f64::from(*end_y),
                        );
                    }
                    GlyphOutlineOperation::CubicCurveTo(OutlineCubicTo {
                        ctrl_1_x, ctrl_1_y, ctrl_2_x, ctrl_2_y, end_x, end_y,
                    }) => {
                        path.curve4(
                            f64::from(*ctrl_1_x), -f64::from(*ctrl_1_y),
                            f64::from(*ctrl_2_x), -f64::from(*ctrl_2_y),
                            f64::from(*end_x), -f64::from(*end_y),
                        );
                    }
                    GlyphOutlineOperation::ClosePath => {
                        path.close_polygon(PATH_FLAGS_NONE);
                    }
                }
            }
        }
        if !has_ops {
            return None;
        }
        Some(path)
    }
}
