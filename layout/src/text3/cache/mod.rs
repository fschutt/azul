//! Core types and layout pipeline for the text/inline formatting context.
//!
//! This module defines the central data structures (`UnifiedConstraints`,
//! `LayoutCache`, `FontManager`, `UnifiedLayout`, etc.) and implements the
//! 5-stage inline layout pipeline:
//!
//! 1. **Logical Analysis** — `InlineContent` → `LogicalItem`
//! 2. **`BiDi` Reordering** — `LogicalItem` → `VisualItem`
//! 3. **Shaping** — `VisualItem` → `ShapedItem`
//! 4. **Text Orientation** — vertical writing-mode transforms
//! 5. **Flow / Positioning** — line breaking + final `PositionedItem` placement
//!
//! The module also contains cursor movement helpers, caching infrastructure
//! (per-item and monolithic), and font management (`FontContext`, `FontManager`,
//! `LoadedFonts`).  Integration with the box layout solver lives in
//! `solver3/fc.rs`.

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

pub use azul_core::selection::{ContentIndex, GraphemeClusterId};
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

// Always import Language from script module
use crate::text3::script::{script_to_language, Language, Script};

// Re-export traits for backwards compatibility
pub use crate::font_traits::{ParsedFontTrait, ShallowClone};

// The modules of what was one file. Every item keeps the visibility it had there: a
// `pub(crate)` one in a private module is what clippy would rather call `pub`, which a
// `pub use` would publish.
mod metrics;
pub use metrics::*;
#[allow(clippy::redundant_pub_crate)]
mod font_chains;
pub use font_chains::*;
mod loaded_fonts;
pub use loaded_fonts::*;
mod font_manager;
pub use font_manager::*;
#[allow(clippy::redundant_pub_crate)]
mod constraints;
pub use constraints::*;
mod styles;
pub use styles::*;
#[allow(clippy::redundant_pub_crate)]
mod inline_items;
pub use inline_items::*;
mod shapes;
pub use shapes::*;
mod shaped_items;
pub use shaped_items::*;
mod unified_layout;
pub use unified_layout::*;
#[allow(clippy::redundant_pub_crate)]
mod shaping_cache;
pub use shaping_cache::*;
mod logical_items;
pub use logical_items::*;
#[allow(clippy::redundant_pub_crate)]
mod shaping;
pub use shaping::*;
mod line_metrics;
pub use line_metrics::*;
#[allow(clippy::redundant_pub_crate)]
mod line_layout;
pub use line_layout::*;
mod justification;
pub use justification::*;
#[allow(clippy::redundant_pub_crate)]
mod char_classes;
pub use char_classes::*;
mod measure;
pub use measure::*;
mod exclusions;
pub(crate) use exclusions::*;
#[allow(clippy::redundant_pub_crate)]
mod break_opportunities;
pub use break_opportunities::*;

#[cfg(test)]
mod shape_outside_and_ruby_tests;

#[cfg(test)]
mod font_cache_swap_tests;

/// Adversarial unit tests generated for `layout/src/text3/cache.rs`.
///
/// These probe the boundaries the production code never sees: NaN / ±inf floats,
/// `u16::MAX` units-per-em, empty slices, `usize::MAX` counts, degenerate geometry
/// and sentinel-value round trips. Where a function has a surprising-but-real
/// behaviour (e.g. `round_eq(NaN, 0.0) == true`), the test PINS that behaviour and
/// says so, rather than pretending it is safe.
#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::cast_precision_loss,
    clippy::similar_names
)]
mod autotest_generated;

/// TEXT7 (MAILENG6 "seen broken"): a run shaped while its face is not loaded
/// shapes to nothing (the font-shape deficit), and the per-item shaping cache
/// kept that nothing under the run's text and style - so the text stayed
/// invisible after the face arrived, until something changed the text.
#[cfg(test)]
mod a_run_shaped_before_its_font_loads;

#[cfg(test)]
mod a_caret_past_the_last_stop_tests;

