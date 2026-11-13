//! Font fallback chains: their keys, resolving a chain, and the faces that cover a text.

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

// --- Core Data Structures for the New Architecture ---

/// Key for caching font chains - based only on CSS properties, not text content
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]

pub struct FontChainKey {
    pub font_families: Vec<String>,
    pub weight: FcWeight,
    pub italic: bool,
    pub oblique: bool,
    /// The optical size the chain's variable faces are drawn at: the used
    /// font size in whole CSS px (`font-optical-sizing: auto`, as Chrome
    /// and CoreText do it), 0 for "the face's default". See
    /// [`FontSelector::optical_size`].
    pub optical_size: u16,
}

/// Either a `FontChainKey` (resolved via fontconfig) or a direct `FontRef` hash.
///
/// This enum cleanly separates:
/// - `Chain`: Fonts resolved through fontconfig with fallback support
/// - `Ref`: Direct `FontRef` that bypasses fontconfig entirely (e.g., embedded icon fonts)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontChainKeyOrRef {
    /// Regular font chain resolved via fontconfig
    Chain(FontChainKey),
    /// Direct `FontRef` identified by pointer address (covers entire Unicode range, no fallbacks)
    Ref(usize),
}

impl FontChainKeyOrRef {
    /// Create from a `FontStack` enum
    #[must_use]
    pub fn from_font_stack(font_stack: &FontStack) -> Self {
        match font_stack {
            FontStack::Stack(selectors) => Self::Chain(FontChainKey::from_selectors(selectors)),
            FontStack::Ref(font_ref) => Self::Ref(font_ref.parsed as usize),
        }
    }

    /// Returns true if this is a direct `FontRef`
    #[must_use]
    pub const fn is_ref(&self) -> bool {
        matches!(self, Self::Ref(_))
    }

    /// Returns the `FontRef` pointer if this is a Ref variant
    #[must_use]
    pub const fn as_ref_ptr(&self) -> Option<usize> {
        match self {
            Self::Ref(ptr) => Some(*ptr),
            Self::Chain(_) => None,
        }
    }

    /// Returns the `FontChainKey` if this is a Chain variant
    #[must_use]
    pub const fn as_chain(&self) -> Option<&FontChainKey> {
        match self {
            Self::Chain(key) => Some(key),
            Self::Ref(_) => None,
        }
    }
}

impl FontChainKey {
    /// Create a `FontChainKey` from a slice of font selectors
    #[must_use]
    pub fn from_selectors(font_stack: &[FontSelector]) -> Self {
        // (2026-06-10) FIRST-WINS DEDUP: cascaded font stacks can carry duplicate
        // families (e.g. [serif, sans-serif, serif, monospace] when the UA fallback
        // list is appended to a stack already naming serif). The pre-resolve
        // collector dedupes its stacks, so without deduping HERE the shaping-time
        // key never matched the stored key (the g121/g122 chain-lookup misses).
        // This is THE canonical FontChainKey constructor — every key-build site
        // must go through it so lookups match by construction.
        let mut font_families: Vec<String> = Vec::new();
        for sel in font_stack {
            if sel.family.is_empty() || font_families.contains(&sel.family) {
                continue;
            }
            font_families.push(sel.family.clone());
        }

        let font_families = if font_families.is_empty() {
            vec!["serif".to_string()]
        } else {
            font_families
        };

        let weight = font_stack.first().map_or(FcWeight::Normal, |s| s.weight);
        let is_italic = font_stack
            .first()
            .is_some_and(|s| s.style == FontStyle::Italic);
        let is_oblique = font_stack
            .first()
            .is_some_and(|s| s.style == FontStyle::Oblique);
        let optical_size = font_stack.first().map_or(0, |s| s.optical_size);

        Self {
            font_families,
            weight,
            italic: is_italic,
            oblique: is_oblique,
            optical_size,
        }
    }
}

/// On-demand chain resolution for a shaping-time cache miss.
///
/// `font_chain_cache` is an OPTIMIZATION, not a gate: the pre-resolve
/// collector dedupes text nodes by compact-cache (family-hash, weight,
/// style) bits, and when those bits under-represent the real cascade
/// (e.g. every text node reads 0 and ONE representative decides the only
/// chain — its UA-bold h1 weight), whole runs asked for a key that was
/// never stored. The old behavior silently SKIPPED such runs: zero shaped
/// items, zero lines, zero height (miniword: multi-line paragraphs
/// measured 0.0 depending on which text node came first). Resolving at
/// miss time uses the same `fc_cache` query the pre-pass would have used,
/// so the result is identical — just later.
pub(crate) fn resolve_chain_on_miss(
    key: &FontChainKey,
    fc_cache: &FcFontCache,
) -> rust_fontconfig::FontFallbackChain {
    let mut trace = Vec::new();
    let mut chain = fc_cache.resolve_font_chain_with_scripts(
        &key.font_families,
        key.weight,
        if key.italic {
            PatternMatch::True
        } else {
            PatternMatch::False
        },
        if key.oblique {
            PatternMatch::True
        } else {
            PatternMatch::False
        },
        None,
        &mut trace,
    );
    // Same instance selection as the pre-pass, so a miss draws what a hit would.
    crate::solver3::getters::select_variable_instances(
        &mut chain,
        key.weight,
        key.optical_size,
        fc_cache,
    );
    chain
}

/// Whether `ch` needs a glyph of its own. Whitespace, controls and the
/// default-ignorable joiners/selectors (ZWJ, ZWNJ, variation selectors, BOM,
/// soft hyphen, bidi controls) are consumed by the shaper without drawing
/// anything, so asking a fallback font for them would pull in a face for
/// nothing — and for the ZWJ inside an emoji family it would pull in the
/// wrong one.
pub(crate) fn needs_own_glyph(ch: char) -> bool {
    let cp = ch as u32;
    !(ch.is_whitespace()
        || ch.is_control()
        || matches!(
            cp,
            0x00AD
                | 0x034F
                | 0x061C
                | 0x115F
                | 0x1160
                | 0x17B4
                | 0x17B5
                | 0x180B..=0x180F
                | 0x200B..=0x200F
                | 0x2028..=0x202E
                | 0x2060..=0x206F
                | 0x3164
                | 0xFE00..=0xFE0F
                | 0xFEFF
                | 0xFFA0
                | 0xFFF0..=0xFFF8
                | 0xE0000..=0xE0FFF
        ))
}

/// The script blocks to ask the resolver for so that `chars` get covered:
/// the well-known fallback block when a char sits in one (so the OS
/// expansion adds its script font, e.g. "Noto Sans Arabic"), the char's own
/// 128-slot block otherwise (so the coverage query still finds any face
/// whose OS/2 ranges include it).
pub(super) fn fallback_ranges_for(chars: &BTreeSet<char>) -> Vec<UnicodeRange> {
    const EXTRA_BLOCKS: &[UnicodeRange] = &[
        UnicodeRange {
            start: 0x0590,
            end: 0x05FF,
        }, // Hebrew
        UnicodeRange {
            start: 0x0E00,
            end: 0x0E7F,
        }, // Thai
    ];
    let mut out: Vec<UnicodeRange> = Vec::new();
    for &ch in chars {
        let cp = ch as u32;
        let block = rust_fontconfig::DEFAULT_UNICODE_FALLBACK_SCRIPTS
            .iter()
            .chain(EXTRA_BLOCKS)
            .find(|r| cp >= r.start && cp <= r.end)
            .copied()
            .unwrap_or(UnicodeRange {
                start: cp & !0x7F,
                end: (cp & !0x7F) + 0x7F,
            });
        if !out
            .iter()
            .any(|r| r.start == block.start && r.end == block.end)
        {
            out.push(block);
        }
    }
    out
}

/// Faces that `content` needs and its resolved chains cannot provide, per
/// chain key — the fix for text typed in a script the document did not
/// contain when its chains were resolved.
///
/// Chain resolution is scoped to the text the DOM CONTAINS at layout time
/// (`collect_used_codepoints_all` → `request_fonts_fast` stops at the first
/// family covering it; the legacy path prunes to the used chars). A character
/// typed afterwards in a script the document did not have yet — Arabic into
/// a Latin paragraph — has no covering face in the chain, and unless some
/// OTHER chain happened to load one, `split_text_by_font_coverage` falls
/// through to its .notdef last resort: the user sees boxes for as long as the
/// resolved chains stay in force. (The full layout of a re-materialized DOM
/// does not help, since the typed text lives in the content overlay, not in
/// the DOM the resolver scans.)
///
/// A char counts as covered when the chain resolves it by OS/2 ranges
/// ([`covering_font`], which does not count the last-resort face) OR any
/// already-loaded face has it in its cmap — exactly the two checks the shaper
/// makes before giving up. The rest go through [`faces_covering`].
///
/// `chain_for` supplies the chain currently in force for a key (`None` =
/// the run is skipped: there is nothing to extend). Returns the new faces
/// per key.
#[allow(clippy::implicit_hasher)] // internal; matches the chain caches' default hasher
pub fn missing_coverage_faces<T: ParsedFontTrait>(
    content: &[InlineContent],
    chain_for: &mut dyn FnMut(&FontChainKey) -> Option<rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    registry: Option<&rust_fontconfig::registry::FcFontRegistry>,
    loaded: &LoadedFonts<T>,
) -> Vec<(FontChainKey, Vec<rust_fontconfig::FontMatch>)> {
    // Per key: the chain in force and the chars it cannot draw.
    let mut chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain> = HashMap::new();
    let mut missing: HashMap<FontChainKey, BTreeSet<char>> = HashMap::new();
    let mut checked: HashMap<FontChainKey, BTreeSet<char>> = HashMap::new();
    for item in content {
        let InlineContent::Text(run) = item else {
            continue;
        };
        let FontStack::Stack(selectors) = &run.style.font_stack else {
            continue;
        };
        let key = FontChainKey::from_selectors(selectors);
        if !chains.contains_key(&key) {
            match chain_for(&key) {
                Some(chain) => {
                    chains.insert(key.clone(), chain);
                }
                None => continue,
            }
        }
        let chain = &chains[&key];
        let seen = checked.entry(key.clone()).or_default();
        for ch in run.text.chars() {
            if !needs_own_glyph(ch) || !seen.insert(ch) {
                continue;
            }
            let covered = covering_font(chain, ch).is_some()
                || loaded.iter().any(|(_, font)| font.has_glyph(ch as u32));
            if !covered {
                missing.entry(key.clone()).or_default().insert(ch);
            }
        }
    }

    let glyph_check = |id: FontId, ch: char| loaded.get(&id).map(|font| font.has_glyph(ch as u32));
    let mut out = Vec::new();
    for (key, uncovered) in missing {
        let added = faces_covering(
            &key,
            &chains[&key],
            uncovered,
            fc_cache,
            registry,
            &glyph_check,
        );
        if !added.is_empty() {
            out.push((key, added));
        }
    }
    out
}

/// Faces to append to `chain` (the chain resolved for `key`) so that every
/// char in `uncovered` has one that can draw it — greedy, in lookup order: a
/// face is kept only if it covers a char no earlier face did, and a face the
/// chain already holds is never returned twice.
///
/// Two lookups, in order:
///
/// 1. the registry's cmap probe over the stack's OS expansion for the missing scripts
///    (`request_fonts_fast` returns only faces that cover at least one of the requested chars — the
///    same resolver the full layout uses; the script ranges are what make the expansion include
///    "Noto Sans Arabic" and its kin, which the plain sans-serif list does not);
/// 2. the coverage-based resolver (`resolve_font_chain_with_scripts`) over the `FcFontCache` for
///    whatever is still uncovered — this is the lookup that sees memory fonts
///    (`register_named_font`) and a registry-less cache.
///
/// `glyph_check(id, ch)` reports the cmap truth for a LOADED face
/// (`None` = not loaded, judge by the OS/2 ranges the match carries). Real
/// cmap knowledge overrides the OS/2 claim in both directions: a loaded face
/// whose cmap lacks every missing char is never returned however loudly its
/// OS/2 bits claim the block.
pub fn faces_covering(
    key: &FontChainKey,
    chain: &rust_fontconfig::FontFallbackChain,
    mut uncovered: BTreeSet<char>,
    fc_cache: &FcFontCache,
    registry: Option<&rust_fontconfig::registry::FcFontRegistry>,
    glyph_check: &dyn Fn(FontId, char) -> Option<bool>,
) -> Vec<rust_fontconfig::FontMatch> {
    use rust_fontconfig::FontMatch;

    fn fm_covers(fm: &FontMatch, cp: u32) -> bool {
        fm.unicode_ranges
            .iter()
            .any(|r| cp >= r.start && cp <= r.end)
    }

    if uncovered.is_empty() {
        return Vec::new();
    }
    let mut known: HashSet<FontId> = chain.fonts().map(|f| f.id).collect();
    let ranges = fallback_ranges_for(&uncovered);
    let italic = if key.italic {
        PatternMatch::True
    } else {
        PatternMatch::False
    };
    let oblique = if key.oblique {
        PatternMatch::True
    } else {
        PatternMatch::False
    };
    let mut added: Vec<FontMatch> = Vec::new();

    let mut consider = |fm: &FontMatch, uncovered: &mut BTreeSet<char>| {
        if uncovered.is_empty() || known.contains(&fm.id) {
            return;
        }
        let covers: Vec<char> = uncovered
            .iter()
            .copied()
            .filter(|c| glyph_check(fm.id, *c).unwrap_or_else(|| fm_covers(fm, *c as u32)))
            .collect();
        if covers.is_empty() {
            return;
        }
        for c in covers {
            uncovered.remove(&c);
        }
        known.insert(fm.id);
        added.push(fm.clone());
    };

    if let Some(registry) = registry {
        // The stack expanded for the MISSING scripts (the probe itself only
        // expands for the default seven): the generic's families, the
        // script candidates for `ranges` (fonts.conf aliases merged over the
        // OS tables) and the config's last resort.
        let stack = fc_cache
            .fallback_config()
            .candidate_families(&key.font_families, &ranges);
        let probed = registry.request_fonts_fast(&[(stack, uncovered.clone())], key.weight, italic);
        for fm in probed
            .iter()
            .flat_map(|c| c.css_fallbacks.iter())
            .flat_map(|g| g.fonts.iter())
        {
            consider(fm, &mut uncovered);
        }
    }

    if !uncovered.is_empty() {
        let mut trace = Vec::new();
        let resolved = fc_cache.resolve_font_chain_with_scripts(
            &key.font_families,
            key.weight,
            italic,
            oblique,
            Some(&ranges),
            &mut trace,
        );
        for fm in resolved.fonts() {
            consider(fm, &mut uncovered);
        }
    }

    added
}

/// The font `chain` resolves `ch` to BY COVERAGE - `None` when nothing in
/// the chain claims the codepoint.
///
/// This is the coverage question; `FontFallbackChain::resolve_codepoint` is
/// not. Once a chain has a last-resort face (`ensure_chains_nonempty` gives
/// one to every chain that matched nothing), `resolve_codepoint` hands that
/// face out for EVERY codepoint, tagged `LAST_RESORT_SOURCE` - so asking it
/// "is this char covered" would say yes for a char no font on the machine
/// can draw, and `missing_coverage_faces` would never go looking for one.
/// Every coverage decision goes through here; the shaper consults the last
/// resort separately, after the cmap probe, as the tier it is.
#[must_use]
pub fn covering_font(chain: &rust_fontconfig::FontFallbackChain, ch: char) -> Option<FontId> {
    match chain.resolve_codepoint(ch as u32) {
        Some((id, source)) if source != rust_fontconfig::fallback::LAST_RESORT_SOURCE => Some(id),
        _ => None,
    }
}

/// Append faces found by [`faces_covering`] to `chain` as one more
/// unicode-fallback group.
///
/// The group's range is the whole codepoint space: these faces were chosen
/// by the cmap/OS-2 coverage of the specific chars that were missing, and
/// the resolver already checks a face's own `unicode_ranges` before using
/// it, so a narrower range would only restate what the `FontMatch` carries.
pub fn append_coverage_faces(
    chain: &mut rust_fontconfig::FontFallbackChain,
    faces: Vec<rust_fontconfig::FontMatch>,
) {
    if faces.is_empty() {
        return;
    }
    chain
        .unicode_fallbacks
        .push(rust_fontconfig::ScriptFallbackGroup {
            range: UnicodeRange {
                start: 0,
                end: 0x0010_FFFF,
            },
            fonts: faces,
        });
}
