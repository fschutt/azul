//! The pipeline's items: logical items, visual items and shaped clusters and glyphs.

use std::{
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, HashMap},
        BTreeSet, HashSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage,
    props::{basic::ColorU, style::StyleBackgroundContent},
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{
    FcFontCache, FcPattern, FcStretch, FcWeight, FontId, PatternMatch, UnicodeRange,
};
use smallvec::{smallvec, SmallVec};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;
use crate::text3::script::{script_to_language, Language, Script};
#[allow(clippy::wildcard_imports)]
// the text layout cache's items, re-exported from the sibling modules by mod.rs
use super::*;

/// Glyph storage for a single shaped cluster.
///
/// Inline one glyph (the
/// common case for Latin text), spill to heap for ligatures / combining
/// marks / multi-glyph clusters. The `union` feature of smallvec packs
/// the inline buffer and the heap pointer into the same bytes, so sizeof
/// stays `sizeof(ShapedGlyph) + 2*usize` regardless of inline/heap state.
pub type ShapedGlyphVec = SmallVec<[ShapedGlyph; 1]>;

/// The kind of a glyph, used to distinguish characters from layout-inserted items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphKind {
    /// A standard glyph representing one or more characters from the source text.
    Character,
    /// A hyphen glyph inserted by the line breaking algorithm.
    Hyphen,
    /// A `.notdef` glyph, indicating a character that could not be found in any font.
    NotDef,
    /// A Kashida justification glyph, inserted to stretch Arabic text.
    Kashida {
        /// The target width of the kashida.
        width: f32,
    },
}

// --- Stage 1: Logical Representation ---

// [g117 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)) — same disc-mis-lift class as
// InlineContent above. LogicalItem is matched in measure Stage-2 (`if let LogicalItem::Text`) +
// reorder_logical_items; a repr(Rust) niche disc mis-lifts on the web. Explicit u8 tag at offset 0
// = a simple load the lift reads correctly. Internal to text3 (not FFI-exposed).
// LogicalItem::Object embeds InlineContent inline.
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum LogicalItem {
    Text {
        /// A stable ID pointing back to the original source character.
        source: ContentIndex,
        /// The text of this specific logical item. §3.2 3c: an `Arc` so
        /// shaped clusters can share it (`ShapedCluster::source_text`).
        /// For override-free runs this IS the `StyledRun`'s Arc; override
        /// / combine-upright segments mint one Arc per segment.
        text: Arc<str>,
        style: Arc<StyleProperties>,
        /// If this text is a list marker: whether it should be positioned outside
        /// (in the padding gutter) or inside (inline with content).
        /// None for non-marker content.
        marker_position_outside: Option<bool>,
        /// The DOM `NodeId` of the Text node this item originated from.
        /// None for generated content (list markers, `::before/::after`, etc.)
        source_node_id: Option<NodeId>,
    },
    // +spec:display-property:b1533f - text-combine-upright tate-chu-yoko horizontal-in-vertical
    // composition
    /// Tate-chu-yoko: Run of text to be laid out horizontally within a vertical context.
    CombinedText {
        source: ContentIndex,
        text: String,
        style: Arc<StyleProperties>,
    },
    Ruby {
        source: ContentIndex,
        // For the stub, we simplify to strings. A full implementation
        // would need to handle Vec<LogicalItem> for both.
        base_text: String,
        ruby_text: String,
        style: Arc<StyleProperties>,
    },
    Object {
        /// A stable ID pointing back to the original source object.
        source: ContentIndex,
        /// The original non-text object.
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        style: Arc<StyleProperties>,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl Hash for LogicalItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Text {
                source,
                text,
                style,
                marker_position_outside,
                source_node_id,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state); // Hash the content, not the Arc pointer
                marker_position_outside.hash(state);
                source_node_id.hash(state);
            }
            Self::CombinedText {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state);
            }
            Self::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                source.hash(state);
                base_text.hash(state);
                ruby_text.hash(state);
                style.as_ref().hash(state);
            }
            Self::Object { source, content } => {
                source.hash(state);
                content.hash(state);
            }
            Self::Tab { source, style } => {
                source.hash(state);
                style.as_ref().hash(state);
            }
            Self::Break { source, break_info } => {
                source.hash(state);
                break_info.hash(state);
            }
        }
    }
}

// --- Stage 2: Visual Representation ---

#[derive(Debug, Clone)]
pub struct VisualItem {
    /// A reference to the logical item this visual item originated from.
    /// A single `LogicalItem` can be split into multiple `VisualItems`.
    pub logical_source: LogicalItem,
    /// The Bidi embedding level for this item.
    pub bidi_level: BidiLevel,
    /// The script detected for this run, crucial for shaping.
    pub script: Script,
    /// The text content for this specific visual run.
    pub text: String,
    /// Byte offset of this visual run's `text` within its source logical run's
    /// text. When bidi splits one logical run into several visual runs, each
    /// shaped cluster's `start_byte_in_run` is produced relative to this visual
    /// run's `text`; adding `run_byte_offset` re-bases it to the logical run so
    /// cluster IDs stay unique and match caret/selection byte positions.
    pub run_byte_offset: usize,
}

// --- Stage 3: Shaped Representation ---

// [g118 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)) — same disc-mis-lift class as
// InlineContent
// + LogicalItem (g117). ShapedItem is matched in measure Stage-5 (`match item { ShapedItem::Cluster
//   ..}`)
// + cloned/matched throughout shaping; a repr(Rust) niche disc mis-lifts on the web. Explicit u8
//   tag at
// offset 0 = a simple load the lift reads correctly. Internal to text3 (not FFI-exposed).
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ShapedItem {
    Cluster(ShapedCluster),
    /// A block of combined text (tate-chu-yoko) that is laid out
    // as a single unbreakable object.
    CombinedBlock {
        source: ContentIndex,
        /// The glyphs to be rendered horizontally within the vertical line.
        glyphs: ShapedGlyphVec,
        /// Uniform style of the combined run (tate-chu-yoko is one style;
        /// glyphs no longer carry per-glyph style — see `ShapedGlyph`).
        style: Arc<StyleProperties>,
        bounds: Rect,
        baseline_offset: f32,
    },
    Object {
        source: ContentIndex,
        bounds: Rect,
        baseline_offset: f32,
        // Store original object for rendering
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        bounds: Rect,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl ShapedItem {
    #[must_use]
    pub const fn as_cluster(&self) -> Option<&ShapedCluster> {
        match self {
            Self::Cluster(c) => Some(c),
            _ => None,
        }
    }
    /// Returns the bounding box of the item, relative to its own origin.
    ///
    /// The origin of the returned `Rect` is `(0,0)`, representing the top-left corner
    /// of the item's layout space before final positioning. The size represents the
    /// item's total advance (width in horizontal mode) and its line height (ascent + descent).
    #[allow(clippy::match_same_arms)]
    // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that
    // can't merge)
    #[must_use]
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Cluster(cluster) => {
                // The width of a text cluster is its total advance.
                let width = cluster.advance;

                // The height is the sum of its ascent and descent, which defines its line box.
                // We use the existing helper function which correctly calculates this from font
                // metrics.
                let (ascent, descent) = get_item_vertical_metrics_approx(self);
                let height = ascent + descent;

                Rect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                }
            }
            // For atomic inline items like objects, combined blocks, and tabs,
            // their bounds have already been calculated during the shaping or measurement phase.
            Self::CombinedBlock { bounds, .. } => *bounds,
            Self::Object { bounds, .. } => *bounds,
            Self::Tab { bounds, .. } => *bounds,

            // Breaks are control characters and have no visual geometry.
            Self::Break { .. } => Rect::default(), // A zero-sized rectangle.
        }
    }
}

/// Precomputed classification of a cluster's text — the flags word the
/// compact-record plan (§1.6) calls for. Computed ONCE at shaping, while
/// the cluster text is in hand; the line breaker's hot predicates then
/// read one bit instead of re-decoding UTF-8 on every probe (those scans
/// ran ~25x per cluster per line-break pass). Also the prerequisite for
/// deleting `ShapedCluster::text`: classification consumers stop needing
/// the bytes at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct ClusterFlags(pub u16);

impl ClusterFlags {
    /// Any `is_word_separator_char` (space family) in the text.
    pub const WORD_SEPARATOR: u16 = 1 << 0;
    /// NBSP / NNBSP / WORD JOINER / ZWNBSP anywhere (UAX#14 GL/WJ):
    /// word-spacing glue that must NOT offer a soft-wrap opportunity.
    pub const NO_BREAK_SPACE: u16 = 1 << 1;
    /// U+200B ZERO WIDTH SPACE anywhere: always a wrap opportunity.
    pub const ZERO_WIDTH_SPACE: u16 = 1 << 2;
    /// Text starts with U+00AD SOFT HYPHEN.
    pub const SOFT_HYPHEN_START: u16 = 1 << 3;
    /// Ends with '-', U+2010 HYPHEN or '/' (UAX#14 HY/BA/SY: break AFTER).
    pub const ENDS_BREAKABLE: u16 = 1 << 4;
    /// Any CJK character (implicit break opportunity in word-break:normal).
    pub const HAS_CJK: u16 = 1 << 5;
    /// Leading char is a grapheme extender (UAX#29): the cluster merges
    /// into the preceding base and is not a standalone caret stop.
    pub const GRAPHEME_CONTINUATION: u16 = 1 << 6;

    /// (d6h) Everything below is DENSE-SIDE ONLY: set by
    /// `DenseText::from_unified` to pack `ShapedCluster` fields the 16 B
    /// compact record has no room for; `classify()` never sets them and
    /// sparse↔dense flag comparisons must mask with
    /// [`Self::CLASSIFY_MASK`].
    pub const CLASSIFY_MASK: u16 = (1 << 7) - 1;
    /// `ShapedCluster::is_first_fragment` (dense packing).
    pub const DENSE_IS_FIRST_FRAGMENT: u16 = 1 << 7;
    /// `ShapedCluster::is_last_fragment` (dense packing).
    pub const DENSE_IS_LAST_FRAGMENT: u16 = 1 << 8;
    /// `ShapedCluster::marker_position_outside.is_some()` (dense packing).
    pub const DENSE_MARKER_SOME: u16 = 1 << 9;
    /// `marker_position_outside == Some(true)` (dense packing).
    pub const DENSE_MARKER_OUTSIDE: u16 = 1 << 10;

    #[must_use]
    pub fn classify(text: &str) -> Self {
        let mut bits = 0u16;
        for (i, ch) in text.chars().enumerate() {
            if is_word_separator_char(ch) {
                bits |= Self::WORD_SEPARATOR;
            }
            if matches!(ch, '\u{00A0}' | '\u{202F}' | '\u{2060}' | '\u{FEFF}') {
                bits |= Self::NO_BREAK_SPACE;
            }
            if ch == '\u{200B}' {
                bits |= Self::ZERO_WIDTH_SPACE;
            }
            if is_cjk_character(ch) {
                bits |= Self::HAS_CJK;
            }
            if i == 0 {
                if ch == '\u{00AD}' {
                    bits |= Self::SOFT_HYPHEN_START;
                }
                // Grapheme-extender probe: 'x' + ch collapsing to one
                // grapheme means ch extends the preceding base.
                let mut probe = String::with_capacity(1 + ch.len_utf8());
                probe.push('x');
                probe.push(ch);
                if probe.graphemes(true).count() == 1 {
                    bits |= Self::GRAPHEME_CONTINUATION;
                }
            }
        }
        if text.ends_with('\u{002D}') || text.ends_with('\u{2010}') || text.ends_with('\u{002F}') {
            bits |= Self::ENDS_BREAKABLE;
        }
        Self(bits)
    }

    #[must_use]
    pub const fn has(self, bit: u16) -> bool {
        self.0 & bit != 0
    }
}

/// A group of glyphs that corresponds to one or more source characters (a cluster).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedCluster {
    /// §3.2 step 3c: the text this cluster was shaped FROM, as a shared
    /// `Arc` of the whole LOGICAL ITEM's text (for override-free runs
    /// that is the `StyledRun`'s own Arc — zero extra allocations). The
    /// per-cluster `String` copy this replaces was the single largest
    /// retained-text duplication (one heap alloc per cluster); the
    /// cluster's own text is the `source_byte_len`-long slice at
    /// `source_cluster_id.start_byte_in_run` — see [`Self::text`].
    ///
    /// NOTE `start_byte_in_run` is relative to the LOGICAL ITEM (the
    /// bidi re-base adds only the visual fragment's offset within the
    /// item; a style-override segment's run offset lives in
    /// `source_content_index.item_index`) — which is exactly why this
    /// field holds the ITEM text, not unconditionally the run text.
    pub source_text: Arc<str>,
    /// Byte length of this cluster's slice in `source_text`. Stored, not
    /// re-derived: ligature-fused clusters span MULTIPLE graphemes, so
    /// "next grapheme boundary" cannot reconstruct the slice in general.
    pub source_byte_len: u16,
    /// The ID of the grapheme cluster this glyph cluster represents.
    pub source_cluster_id: GraphemeClusterId,
    /// The source `ContentIndex` for mapping back to logical items.
    pub source_content_index: ContentIndex,
    /// The DOM `NodeId` of the Text node this cluster originated from.
    /// None for generated content (list markers, `::before/::after`, etc.)
    pub source_node_id: Option<NodeId>,
    /// The glyphs that make up this cluster. `SmallVec<[T; 1]>` — inline
    /// single-glyph clusters (the common case for Latin text), spill to
    /// heap only for ligatures / combining marks.
    pub glyphs: ShapedGlyphVec,
    /// Precomputed text classification — see [`ClusterFlags`].
    pub flags: ClusterFlags,
    /// The total advance width (horizontal) or height (vertical) of the cluster.
    pub advance: f32,
    /// The direction of this cluster, inherited from its `VisualItem`.
    pub direction: BidiDirection,
    /// Font style of this cluster
    pub style: Arc<StyleProperties>,
    /// If this cluster is a list marker: whether it should be positioned outside
    /// (in the padding gutter) or inside (inline with content).
    /// None for non-marker content.
    pub marker_position_outside: Option<bool>,
    /// True if this is the first visual fragment of its inline box.
    /// Used for `box-decoration-break` and split inline border/padding.
    /// When an inline element wraps across lines, only the first fragment
    /// gets the start-edge border/padding.
    pub is_first_fragment: bool,
    /// True if this is the last visual fragment of its inline box.
    /// Only the last fragment gets the end-edge border/padding.
    pub is_last_fragment: bool,
}

impl ShapedCluster {
    /// The cluster's source text: the `source_byte_len`-long slice of the
    /// shared item text at `start_byte_in_run`. Replaces the deleted
    /// per-cluster `String` (§3.2 step 3c); T1 pins slice==shaped-text.
    /// Out-of-range indices (defensive; ids are shaper-produced) yield "".
    #[must_use]
    pub fn text(&self) -> &str {
        if self.source_cluster_id.start_byte_in_run == u32::MAX {
            // Synthesized clusters (hyphenation hyphen, kashida) carry the
            // sentinel id and their own tiny Arc — the whole buffer IS the
            // cluster text.
            return &self.source_text;
        }
        let start = self.source_cluster_id.start_byte_in_run as usize;
        self.source_text
            .get(start..start + self.source_byte_len as usize)
            .unwrap_or("")
    }
}

/// Shared empty `Arc<str>`: the pre-stamp placeholder for
/// `ShapedCluster::source_text`. `shape_text_correctly` cannot see the
/// logical item's Arc (it receives a visual-fragment `&str`), so it
/// stamps this and the shaping loop overwrites it with the real item Arc
/// right after the bidi re-base — the same site that finalizes
/// `start_byte_in_run`, which `text()` slices with.
#[must_use]
pub fn empty_arc_str() -> Arc<str> {
    static EMPTY: std::sync::OnceLock<Arc<str>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| Arc::from("")).clone()
}

/// A single, shaped glyph with its essential metrics.
// Deliberately NOT `Copy`: this is ~60 bytes on the hottest path in the
// engine, and an implicit copy is exactly the kind of silent cost the
// memory campaign spent weeks removing. Callers clone explicitly.
#[allow(missing_copy_implementations)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    /// The kind of glyph this is (character, hyphen, etc.).
    pub kind: GlyphKind,
    /// Glyph ID inside of the font
    pub glyph_id: u16,
    /// The byte offset of this glyph's source character(s) within its cluster text.
    pub cluster_offset: u32,
    /// The horizontal advance for this glyph (for horizontal text) - this is the BASE advance
    /// from the font metrics, WITHOUT kerning applied
    pub advance: f32,
    /// The kerning adjustment for this glyph (positive = more space, negative = less space)
    /// This is separate from advance so we can position glyphs absolutely
    pub kerning: f32,
    /// The horizontal offset/bearing for this glyph
    pub offset: Point,
    /// The vertical advance for this glyph (for vertical text).
    pub vertical_advance: f32,
    /// The vertical offset/bearing for this glyph.
    pub vertical_offset: Point,
    pub script: Script,
    // NOTE (§3.3 field disposition, 2026-08-10): `style` was REMOVED from
    // the per-glyph record — style is uniform within a cluster by
    // construction (shaping runs are style-coalesced), so the enclosing
    // `ShapedCluster::style` is the single source. This deletes one
    // Arc<StyleProperties> per glyph (~94k retained across the holders),
    // kills the per-glyph Arc-clone loop in the shaping-cache hit
    // re-stamp, and unblocks sharing whole glyph arrays behind Arc.
    /// Hash of the font - use `LoadedFonts` to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
}

impl ShapedGlyph {
    #[must_use]
    pub fn into_glyph_instance<T: ParsedFontTrait>(
        &self,
        style: &StyleProperties,
        writing_mode: WritingMode,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, style.font_size_px))
            .unwrap_or_default();

        let position = if writing_mode.is_advance_horizontal() {
            LogicalPosition {
                x: self.offset.x,
                y: self.offset.y,
            }
        } else {
            LogicalPosition {
                x: self.vertical_offset.x,
                y: self.vertical_offset.y,
            }
        };

        GlyphInstance {
            index: u32::from(self.glyph_id),
            point: position,
            size,
        }
    }

    /// Convert this `ShapedGlyph` into a `GlyphInstance` with an absolute position.
    /// This is used for display list generation where glyphs need their final page coordinates.
    #[must_use]
    pub fn into_glyph_instance_at<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        absolute_position: LogicalPosition,
        style: &StyleProperties,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, style.font_size_px))
            .unwrap_or_default();

        GlyphInstance {
            index: u32::from(self.glyph_id),
            point: absolute_position,
            size,
        }
    }

    /// Convert this `ShapedGlyph` into a `GlyphInstance` with an absolute position.
    /// This version doesn't require fonts - it uses a default size.
    /// Use this when you don't need precise glyph bounds (e.g., display list generation).
    #[must_use]
    pub fn into_glyph_instance_at_simple(
        &self,
        _writing_mode: WritingMode,
        absolute_position: LogicalPosition,
    ) -> GlyphInstance {
        // Use font metrics to estimate size, or default to zero
        // The actual rendering will use the font directly
        GlyphInstance {
            index: u32::from(self.glyph_id),
            point: absolute_position,
            size: LogicalSize::default(),
        }
    }
}

// --- Stage 4: Positioned Representation (Final Layout) ---

#[derive(Debug, Clone, PartialEq)]
pub struct PositionedItem {
    pub item: ShapedItem,
    pub position: Point,
    pub line_index: usize,
}
