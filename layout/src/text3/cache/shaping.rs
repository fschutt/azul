//! Stage 3: shaping visual items with font fallback, placeholders, ellipses and text orientation.

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

// --- Stage 3 Implementation ---

/// Shape visual items into `ShapedItems` using pre-loaded fonts.
///
/// This function does NOT load any fonts - all fonts must be pre-loaded and passed in.
/// If a required font is not in `loaded_fonts`, the text will be skipped with a warning.
///
/// **Optimization: Inline Run Coalescing**
///
/// // +spec:display-property:9c6d59 - text shaping not broken across inline box boundaries when no
/// effective formatting change // +spec:display-property:cf8917 - text shaping not broken across
/// inline box boundaries When consecutive text `VisualItem`s share the same layout-affecting
/// properties (font, size, spacing, etc.) but differ only in rendering properties (color,
/// background), they are coalesced into a single shaping call. This dramatically
/// reduces the number of `font.shape_text()` invocations for syntax-highlighted
/// code where hundreds of `<span>` elements use the same monospace font but
/// different colors. After shaping, the original per-span styles are restored
/// to each `ShapedCluster` based on byte-range mapping.
/// Shape visual items with per-item caching. For each item (or coalesced group),
/// compute a cache key from (text, `bidi_level`, script, `style_layout_hash`). On cache
/// hit, reuse the previously shaped clusters. On miss, shape and store.
///
/// This is the incremental shaping path: when one word changes in a paragraph,
/// only that word's item misses the per-item cache; all other items hit.
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
/// # Errors
///
/// Returns a `LayoutError` if shaping the visual items fails.
pub fn shape_visual_items_with_per_item_cache<T: ParsedFontTrait>(
    visual_items: &[VisualItem],
    per_item_cache: &mut HashMap<u64, Arc<PerItemShapedEntry>>,
    per_item_accessed: &mut HashSet<u64>,
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<ShapedItem>, LayoutError> {
    use std::hash::{Hash, Hasher};
    // Delegate to the existing shaping logic, but for each coalesce group,
    // check the per-item cache first.
    //
    // Strategy: Identify coalesce groups (adjacent items with same layout_hash,
    // bidi_level, script). For each group, compute a key from the concatenated
    // text + shared properties. Check cache. On miss, shape the group and cache it.
    let mut shaped = Vec::new();
    let mut idx = 0;

    while idx < visual_items.len() {
        let item = &visual_items[idx];

        // Determine coalesce group boundaries (same logic as shape_visual_items)
        let (layout_hash, bidi_level, script) = match &item.logical_source {
            LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                (style.layout_hash(), item.bidi_level, item.script)
            }
            _ => {
                // Non-text items: shape individually (no coalescing)
                let single = shape_visual_items(
                    &visual_items[idx..=idx],
                    font_chain_cache,
                    fc_cache,
                    loaded_fonts,
                    debug_messages,
                )?;
                shaped.extend(single);
                idx += 1;
                continue;
            }
        };

        let mut coalesce_end = idx + 1;
        while coalesce_end < visual_items.len() {
            let next = &visual_items[coalesce_end];
            let next_layout_hash = match &next.logical_source {
                LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                    Some(style.layout_hash())
                }
                _ => None,
            };
            if let Some(nlh) = next_layout_hash {
                if nlh == layout_hash && next.bidi_level == bidi_level && next.script == script {
                    coalesce_end += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Compute per-group cache key
        let mut hasher = DefaultHasher::new();
        for item in &visual_items[idx..coalesce_end] {
            item.text.hash(&mut hasher);
        }
        layout_hash.hash(&mut hasher);
        bidi_level.hash(&mut hasher);
        (script as u32).hash(&mut hasher);
        let group_key = hasher.finish();

        // Check per-item cache
        per_item_accessed.insert(group_key);
        if let Some(cached) = per_item_cache.get(&group_key) {
            // The key is `layout_hash`, which EXCLUDES paint-only properties
            // (colour, background, text-decoration) BY DESIGN — that is what
            // lets a hover recolour reuse the shaping instead of re-running
            // it. But the cached clusters carry a WHOLE `Arc<StyleProperties>`
            // per glyph, and `get_glyph_runs_simple` reads `glyph.style.color`
            // when it builds the display list. Handing back the cached glyphs
            // unchanged therefore hands back the colour of whichever run
            // happened to shape this text first.
            //
            // Same for identity: `source_node_id` rides along in the cluster,
            // and `DisplayListItem::Text.source_node_index` — which the damage
            // system attributes rects by — is taken from it. Two nodes with
            // the same text and the same layout_hash (three ribbon tab
            // headers, a column of identical labels) share one entry, so the
            // second one's text was reported as belonging to the first.
            //
            // Geometry is identical by construction (that IS the key), so
            // re-stamping the paint-and-identity fields from the CURRENT items
            // is sound and keeps the reuse. Clusters map back to their item
            // through `source_content_index`.
            let group = group_items(&visual_items[idx..coalesce_end]);
            // NEGATIVE-CONTROL KNOB (T2, plan §2.3): AZ_T2_SKIP_RESTAMP=1
            // hands back the cached entry UNMODIFIED — the exact defect
            // 8ec9f387d fixed. The identity gate
            // (tests/text3_shaping_cache_identity.rs) runs once with this
            // set and requires itself to FAIL; production never sets it.
            let t2_skip_restamp = env_flag!("AZ_T2_SKIP_RESTAMP");
            // (d7) Materialize from the compact store — the fat path
            // cloned every item here anyway, so this is cost-neutral.
            shaped.extend(cached.compact.expand().into_iter().map(|c| {
                if t2_skip_restamp {
                    return c;
                }
                let mut c = c;
                if let ShapedItem::Cluster(ref mut sc) = c {
                    // The item that shaped this cluster, by POSITION in the
                    // group: the cached group and the hitting one hold the same
                    // texts in the same order (that is the key), but a text
                    // shaped where it was run 0 is hit where it is run 1 - and
                    // matched by content index it matched nothing, so the
                    // cluster kept the run AND the node of the paragraph that
                    // shaped it first.
                    let position = cached
                        .items
                        .iter()
                        .position(|(source, _)| *source == sc.source_content_index);
                    if let Some(p) = position {
                        if let (Some(current), Some(&(_, cached_offset))) =
                            (group.get(p), cached.items.get(p))
                        {
                            // Cluster-level re-stamp: style no longer lives on
                            // glyphs, so a cache hit is a few writes per
                            // cluster instead of an Arc clone per glyph (T2
                            // pins this). `source_text` is deliberately NOT
                            // re-stamped: the cache key includes the text, so
                            // the cached Arc is content-equal to the hitting
                            // item's — keeping it SHARES one allocation across
                            // all equal-text nodes.
                            sc.source_content_index = current.source;
                            sc.source_cluster_id.source_run = current.source.run_index;
                            // The byte is relative to the item's LOGICAL run:
                            // move it from the cached item's offset in its run
                            // to the hitting item's.
                            let byte = i64::from(sc.source_cluster_id.start_byte_in_run)
                                - i64::try_from(cached_offset).unwrap_or(0)
                                + i64::try_from(current.run_byte_offset).unwrap_or(0);
                            sc.source_cluster_id.start_byte_in_run =
                                u32::try_from(byte.max(0)).unwrap_or(u32::MAX);
                            sc.source_node_id = current.source_node_id;
                            sc.style = current.style.clone();
                        }
                    }
                }
                c
            }));
        } else {
            // Cache miss — shape this group
            let deficit_before = thread_font_shape_deficit();
            let group_items = shape_visual_items(
                &visual_items[idx..coalesce_end],
                font_chain_cache,
                fc_cache,
                loaded_fonts,
                debug_messages,
            )?;
            // A group shaped short of a font (its face not loaded YET) is not
            // cached: the key is its text and layout style, not the loaded
            // faces, so the empty / partial result would be served after the
            // face arrives and the text would stay invisible. The next pass
            // shapes it again.
            if thread_font_shape_deficit() == deficit_before {
                let total_advance: f32 = group_items
                    .iter()
                    .map(|item| match item {
                        ShapedItem::Cluster(c) => c.advance,
                        _ => 0.0,
                    })
                    .sum();
                per_item_cache.insert(
                    group_key,
                    Arc::new(PerItemShapedEntry {
                        compact: CompactShapedEntry::build(&group_items),
                        total_advance,
                        items: self::group_items(&visual_items[idx..coalesce_end])
                            .into_iter()
                            .map(|it| (it.source, it.run_byte_offset))
                            .collect(),
                    }),
                );
            }
            shaped.extend(group_items);
        }

        idx = coalesce_end;
    }

    Ok(shaped)
}

/// Split text into segments where consecutive characters resolve to the same font
/// in the fallback chain. Returns Vec<(`byte_start`, `byte_end`, `FontId`)>.
///
/// Characters that can't be resolved to any font are skipped (gap in coverage).
pub(super) fn split_text_by_font_coverage<T: ParsedFontTrait>(
    text: &str,
    font_chain: &rust_fontconfig::FontFallbackChain,
    loaded_fonts: &LoadedFonts<T>,
) -> Vec<(usize, usize, FontId)> {
    let mut segments: Vec<(usize, usize, FontId)> = Vec::new();

    // Deterministic "last resort" face for characters no font covers: the lowest
    // FontId among the loaded fonts. Used so an uncovered codepoint still emits a
    // .notdef (tofu) segment instead of being silently dropped (zero glyphs/advance).
    let notdef_font_id = loaded_fonts.iter().map(|(id, _)| *id).min();

    // Per-character resolution is memoised for the duration of this call.
    // The chain walk scans every fallback group's unicode ranges linearly,
    // and the loop below runs it once per CHARACTER — so a paragraph paid it
    // ~200 times to answer ~40 distinct questions. A `perf` profile of a
    // steady-state frame put the cmap/range scanning at 7.8% of the whole
    // frame, third behind the pixel blend and the scanline sweep.
    //
    // The memo is call-scoped on purpose: `font_chain` and `loaded_fonts`
    // are both fixed for the duration, so a character's answer cannot
    // change within one call. A longer-lived cache would have to key on
    // both and is a different, riskier change.
    let mut resolved: alloc::collections::BTreeMap<char, Option<FontId>> =
        alloc::collections::BTreeMap::new();
    // Whether a char's covering face was not loaded (see below).
    let mut short_of_a_face = false;

    for (byte_idx, ch) in text.char_indices() {
        let char_end = byte_idx + ch.len_utf8();
        if let Some(&memo) = resolved.get(&ch) {
            if let Some(font_id) = memo {
                match segments.last_mut() {
                    Some(last) if last.2 == font_id && last.1 == byte_idx => {
                        last.1 = char_end;
                    }
                    _ => segments.push((byte_idx, char_end, font_id)),
                }
            }
            continue;
        }
        // Primary: the resolved fallback chain, BY COVERAGE (`covering_font`
        // leaves the last-resort tier out; it is consulted last, below). Its
        // coverage comes from rust-fontconfig's OS/2-derived
        // `unicode_ranges`, which can MISS codepoints a font actually has in
        // its cmap — e.g. Noto Sans CJK's JP face does not advertise the
        // Hangul OS/2 block, so 한국어 resolves to None here even though that
        // face's cmap covers it.
        //
        // A covering face that is NOT LOADED cannot draw: its chain was
        // resolved after the layout loaded its faces (a key the pre-pass
        // never saw, resolved on the miss). The char takes the next face
        // below that can draw it - shaped text, not nothing - and the call
        // counts as short of its font (below), so its result is not cached
        // and a pass with the face loaded shapes it again.
        let covering = covering_font(font_chain, ch);
        let covering_loaded = covering.filter(|id| loaded_fonts.get(id).is_some());
        short_of_a_face |= covering.is_some() && covering_loaded.is_none();
        let font_id = covering_loaded
            // The chain's OWN faces next, in chain order, by REAL cmap
            // coverage. The metadata behind the range walk is partial for a
            // face the fast probe found: it records only the codepoints the
            // DOM had at probe time, so the first 'ü' typed into an ASCII
            // paragraph misses there even though the paragraph's face has
            // it. Asking the paragraph's face first keeps that 'ü' in the
            // same font as the 'u' beside it instead of whichever OTHER
            // loaded face (the UI font, say) happens to sort first below.
            .or_else(|| {
                font_chain
                    .fonts()
                    .map(|fm| fm.id)
                    .find(|id| loaded_fonts.get(id).is_some_and(|f| f.has_glyph(ch as u32)))
            })
            // Fallback: probe the actually-loaded fonts by REAL glyph coverage
            // so OS/2-vs-cmap gaps render instead of being silently dropped.
            // The covering CJK face is already loaded (Han/Kana resolved to it),
            // so this reuses it for Hangul rather than mixing in another font.
            // Iterate in a STABLE order (lowest FontId first) so the chosen face is
            // deterministic across processes — a raw HashMap `.find` is seeded per
            // process and would pick different faces run-to-run.
            .or_else(|| {
                loaded_fonts
                    .iter()
                    .filter(|(_, font)| font.has_glyph(ch as u32))
                    .map(|(id, _)| *id)
                    .min()
            })
            // Last resort: no font advertises OR covers this codepoint. The
            // chain's own last-resort face first (the tier
            // `ensure_chains_nonempty` fills for a chain that matched
            // nothing), else the primary loaded face — so the shaper emits a
            // visible .notdef box and the byte range is preserved (following
            // text is not shifted).
            .or_else(|| {
                font_chain
                    .last_resort
                    .first()
                    .map(|m| m.id)
                    .filter(|id| loaded_fonts.get(id).is_some())
            })
            .or(notdef_font_id);
        resolved.insert(ch, font_id);
        if let Some(font_id) = font_id {
            match segments.last_mut() {
                Some(last) if last.2 == font_id && last.1 == byte_idx => {
                    // Extend current segment (same font, contiguous)
                    last.1 = char_end;
                }
                _ => {
                    // New segment (different font or gap)
                    segments.push((byte_idx, char_end, font_id));
                }
            }
        }
    }
    if short_of_a_face {
        note_font_shape_deficit();
    }

    segments
}

/// Measures the total inline advance (width in horizontal mode) of `text` shaped at
/// `style`, using the same font-resolution path as the main shaper. Returns `None` if the
/// font chain is not resolved / shaping fails, so callers can fall back to an estimate.
///
/// Used by ruby layout to size the base and annotation runs from REAL shaped advances
/// (instead of a `chars * font_size * magic_ratio` fudge).
pub(super) fn measure_run_advance<T: ParsedFontTrait>(
    text: &str,
    style: &Arc<StyleProperties>,
    script: Script,
    source: ContentIndex,
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }
    let language = script_to_language(script, text);
    match &style.font_stack {
        FontStack::Ref(font_ref) => {
            let glyphs = font_ref
                .shape_text(text, script, language, BidiDirection::Ltr, style.as_ref())
                .ok()?;
            Some(glyphs.iter().map(|g| g.advance + g.kerning).sum())
        }
        FontStack::Stack(selectors) => {
            let cache_key = FontChainKey::from_selectors(selectors);
            let font_chain = font_chain_cache.get(&cache_key)?;
            let clusters = shape_with_font_fallback(
                text,
                script,
                language,
                BidiDirection::Ltr,
                style,
                source,
                None,
                font_chain,
                fc_cache,
                loaded_fonts,
            )
            .ok()?;
            Some(clusters.iter().map(|c| c.advance).sum())
        }
    }
}

/// Count of shaping calls that produced ZERO clusters because the resolved
/// font was not loaded (or no font in the chain covered any character) —
/// the "success, zero glyphs" channel that made the first character typed
/// into a blank document invisible for three sessions. The pipeline stays
/// permissive at runtime (a transiently missing font must not abort the
/// whole layout), but the deficit is COUNTED and surfaced per layout pass in
/// `FrameReport::font_shape_deficit`, where a test can pin it to zero.
pub(crate) static FONT_SHAPE_DEFICIT: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);

/// Drain [`FONT_SHAPE_DEFICIT`] (called once per layout pass by the funnel).
#[must_use]
pub fn take_font_shape_deficit() -> u32 {
    FONT_SHAPE_DEFICIT.swap(0, core::sync::atomic::Ordering::Relaxed)
}

std::thread_local! {
    /// This thread's running count of font-shape deficits. The global
    /// [`FONT_SHAPE_DEFICIT`] is shared by every thread and drained by the
    /// frame report, so it cannot tell whether ONE shaping call came up short;
    /// this one can (shaping is synchronous on its thread).
    static THREAD_FONT_SHAPE_DEFICIT: core::cell::Cell<u32> =
        const { core::cell::Cell::new(0) };
}

/// Count one shaping that came up short of a font (its face not loaded, or
/// no face for any of its characters): the frame report's counter and this
/// thread's.
pub(super) fn note_font_shape_deficit() {
    FONT_SHAPE_DEFICIT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    THREAD_FONT_SHAPE_DEFICIT.with(|count| count.set(count.get().wrapping_add(1)));
}

/// This thread's deficit count so far: unchanged across a shaping call means
/// that call had every face it needed.
pub(super) fn thread_font_shape_deficit() -> u32 {
    THREAD_FONT_SHAPE_DEFICIT.with(core::cell::Cell::get)
}

/// Shape text with per-character font fallback.
///
/// Splits the text into segments by font coverage, shapes each segment with
/// its resolved font, and fixes byte offsets so they're relative to the
/// original `text` (not the segment substring).
#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
/// Shape a one-line PROMPT string (the engine `placeholder` attribute) with
/// the node's own style through the normal fallback chain. Returns a flat
/// glyph list; an unresolvable chain yields an empty Vec (no prompt beats a
/// panic in a paint path). `pub(crate)`: the display-list builder is the one
/// consumer.
pub(crate) fn shape_placeholder_text<T: ParsedFontTrait>(
    text: &str,
    style: &Arc<StyleProperties>,
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
) -> Vec<Glyph> {
    if text.is_empty() {
        return Vec::new();
    }
    let script = crate::text3::script::detect_script(text).unwrap_or(Script::Latin);
    let language = script_to_language(script, text);
    match &style.font_stack {
        FontStack::Ref(font_ref) => font_ref
            .shape_text(text, script, language, BidiDirection::Ltr, style)
            .unwrap_or_default(),
        FontStack::Stack(selectors) => {
            let cache_key = FontChainKey::from_selectors(selectors);
            // The chain cache is filled as a side effect of laying out real
            // text, and a prompt is deliberately NOT a DOM node — nothing
            // lays it out, so nothing guarantees its chain is in there. A
            // miss used to shape zero glyphs and the prompt simply vanished
            // for that frame: clicking anything that forces a relayout made
            // the placeholder blink out and come back (device report).
            // Resolve it the way `window.rs` does on the same miss.
            let resolved;
            let font_chain = if let Some(chain) = font_chain_cache.get(&cache_key) {
                chain
            } else {
                resolved = resolve_chain_on_miss(&cache_key, fc_cache);
                &resolved
            };
            // v1: the FIRST loaded face of the chain shapes the whole prompt
            // (a prompt is app-authored, single-script text; per-glyph
            // fallback can come later if a real prompt ever needs it).
            //
            // If none of the chain's faces is loaded YET, shape with whatever
            // face this frame does have. The prompt is not laid out like real
            // text — nothing loads fonts on its behalf — so on a frame that
            // resolves the chain before its faces arrive, insisting on the
            // chain paints NOTHING and the prompt blinks out for that frame.
            // One frame in a neighbouring face is invisible next to that.
            let Some(font) = font_chain
                .fonts()
                .find_map(|m| loaded_fonts.get(&m.id))
                .or_else(|| loaded_fonts.iter().next().map(|(_, f)| f))
            else {
                return Vec::new();
            };
            font.shape_text(text, script, language, BidiDirection::Ltr, style)
                .unwrap_or_default()
        }
    }
}

/// The ellipsis `text-overflow: ellipsis` ends a cut line with (CSS Overflow
/// 3 §3.1), shaped in `style` - the BLOCK's: the ellipsis "is styled and
/// baseline-aligned according to the block", whatever the text it follows.
/// U+2026, or three full stops where the block's face has no ellipsis (the
/// substitution the spec allows). Empty while no face is loaded
/// ([`shape_placeholder_text`], which shapes it the same way: a one-line
/// string no DOM node holds).
pub(crate) fn shape_ellipsis<T: ParsedFontTrait>(
    style: &Arc<StyleProperties>,
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
) -> Vec<Glyph> {
    let ellipsis = shape_placeholder_text(
        "\u{2026}",
        style,
        font_chain_cache,
        fc_cache,
        loaded_fonts,
    );
    if !ellipsis.is_empty() && ellipsis.iter().all(|g| g.glyph_id != 0) {
        return ellipsis;
    }
    shape_placeholder_text("...", style, font_chain_cache, fc_cache, loaded_fonts)
}

pub(super) fn shape_with_font_fallback<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: Language,
    direction: BidiDirection,
    style: &Arc<StyleProperties>,
    source_index: ContentIndex,
    source_node_id: Option<NodeId>,
    font_chain: &rust_fontconfig::FontFallbackChain,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
) -> Result<Vec<ShapedCluster>, LayoutError> {
    // Cache the debug flag in a `OnceLock<bool>` — reading it per-shape
    // (this function fires once per text segment, ~hundreds of times
    // per render of a real DOM) costs ~100 ns per `std::env::var_os`
    // call on macOS (env-lock + hashmap lookup), and even before the
    // lookup finishes the `eprintln!` machinery takes a stderr lock
    // and allocates the formatted string. Both are invisible in
    // release unless `AZ_FONT_FALLBACK_DEBUG=1` is set.
    static FONT_FB_DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let dbg = *FONT_FB_DEBUG.get_or_init(|| std::env::var_os("AZ_FONT_FALLBACK_DEBUG").is_some());

    let segments = split_text_by_font_coverage(text, font_chain, loaded_fonts);

    if dbg && segments.len() > 1 {
        eprintln!(
            "[FONT FALLBACK] text needs {} font segments for '{}' ({}..{} bytes)",
            segments.len(),
            text.chars().take(40).collect::<String>(),
            0,
            text.len()
        );
    }

    unsafe {
        crate::az_mark(0x60850_u32, segments.len() as u32);
    } // [g123] segments count (split_text_by_font_coverage)
    if segments.len() <= 1 {
        // Fast path: all characters use the same font (common case)
        let (seg_start, seg_end, font_id) = if let Some(s) = segments.first() {
            unsafe {
                crate::az_mark(0x60854_u32, 0x0000_0001_u32);
            }
            s
        } else {
            unsafe {
                crate::az_mark(0x60854_u32, 0x0000_00EE_u32);
            } // [g123] split→0 segments (resolve_char failed all)
            if dbg {
                eprintln!(
                    "[FONT FALLBACK] no font could render any char in '{}'",
                    text.chars().take(20).collect::<String>()
                );
            }
            note_font_shape_deficit();
            return Ok(Vec::new());
        };
        let font = if let Some(f) = loaded_fonts.get(font_id) {
            unsafe {
                crate::az_mark(0x60858_u32, 0x0000_0001_u32);
            }
            f
        } else {
            unsafe {
                crate::az_mark(0x60858_u32, 0x0000_00EE_u32);
            } // [g123] loaded_fonts.get MISS
            if dbg {
                eprintln!(
                    "[FONT FALLBACK] font {:?} not in loaded_fonts for '{}'",
                    font_id,
                    text.chars().take(20).collect::<String>()
                );
            }
            note_font_shape_deficit();
            return Ok(Vec::new());
        };
        // If segment covers the full text (overwhelmingly common), skip substr+fixup
        if *seg_start == 0 && *seg_end == text.len() {
            unsafe {
                crate::az_mark(0x60860_u32, 0xC0DE_0860_u32);
            } // [g123] reached shape_text_correctly (full-text)
            return shape_text_correctly(
                text,
                script,
                language,
                direction,
                font,
                style,
                source_index,
                source_node_id,
            );
        }
        let mut clusters = shape_text_correctly(
            &text[*seg_start..*seg_end],
            script,
            language,
            direction,
            font,
            style,
            source_index,
            source_node_id,
        )?;
        if *seg_start > 0 {
            for cluster in &mut clusters {
                cluster.source_cluster_id.start_byte_in_run += *seg_start as u32;
            }
        }
        return Ok(clusters);
    }

    // Multiple fonts needed — shape each segment separately
    let mut all_clusters = Vec::new();
    for (seg_start, seg_end, font_id) in &segments {
        let Some(font) = loaded_fonts.get(font_id) else {
            if dbg {
                eprintln!(
                    "[FONT FALLBACK] font {font_id:?} NOT loaded, skipping segment bytes \
                     {seg_start}..{seg_end}"
                );
            }
            // The segment's glyphs are missing: as much a deficit as a whole
            // run shaped to nothing (it was skipped silently).
            note_font_shape_deficit();
            continue;
        };
        let segment_text = &text[*seg_start..*seg_end];
        if dbg {
            eprintln!(
                "[FONT FALLBACK] text='{segment_text}' uses font {font_id:?} (bytes \
                 {seg_start}..{seg_end})"
            );
        }
        let mut seg_clusters = shape_text_correctly(
            segment_text,
            script,
            language,
            direction,
            font,
            style,
            source_index,
            source_node_id,
        )?;
        // Fix byte offsets: shape_text_correctly produces offsets relative to
        // segment_text, but callers expect offsets relative to the full text.
        if *seg_start > 0 {
            for cluster in &mut seg_clusters {
                cluster.source_cluster_id.start_byte_in_run += *seg_start as u32;
            }
        }
        all_clusters.extend(seg_clusters);
    }
    Ok(all_clusters)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
#[allow(clippy::match_same_arms)]
// enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't
// merge)
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if shaping the visual items fails.
pub fn shape_visual_items<T: ParsedFontTrait>(
    visual_items: &[VisualItem],
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<ShapedItem>, LayoutError> {
    let mut shaped = Vec::new();
    let mut idx = 0;
    let mut _coalesced_runs = 0usize;
    let mut _total_runs = 0usize;
    let mut _shape_calls = 0usize;

    // Log count of visual items for debugging coalescing

    while idx < visual_items.len() {
        let item = &visual_items[idx];
        match &item.logical_source {
            LogicalItem::Text {
                style,
                source,
                marker_position_outside,
                source_node_id,
                ..
            } => {
                let layout_hash = style.layout_hash();
                let bidi_level = item.bidi_level;
                let script = item.script;

                // +spec:display-property:ca95f6 - text shaping breaks at inline box boundaries when
                // layout-affecting properties differ when layout-affecting
                // properties (font weight, family, size, etc.) change
                // across element boundaries, preventing ligatures from forming across such changes.
                // Look ahead: find consecutive text items with the same layout-affecting
                // properties (font, size, spacing) that can be shaped as one merged run.
                let mut coalesce_end = idx + 1;
                while coalesce_end < visual_items.len() {
                    let next = &visual_items[coalesce_end];
                    if let LogicalItem::Text {
                        style: next_style, ..
                    } = &next.logical_source
                    {
                        if next_style.layout_hash() == layout_hash
                            && next.bidi_level == bidi_level
                            && next.script == script
                        {
                            coalesce_end += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                let coalesce_count = coalesce_end - idx;

                if coalesce_count > 1 {
                    _coalesced_runs += coalesce_count;
                    _shape_calls += 1;
                    // ── COALESCED PATH ──
                    // Merge N text items into one shaping call, then split results
                    // back per original run to preserve per-span rendering styles.

                    // Build merged text and record byte ranges → original style
                    let total_text_len: usize = visual_items[idx..coalesce_end]
                        .iter()
                        .map(|v| v.text.len())
                        .sum();
                    let mut merged_text = String::with_capacity(total_text_len);
                    // (byte_start, byte_end, style, source, source_node_id, marker_outside,
                    //  run_byte_offset, item_text — the logical item's shared Arc,
                    //  stamped onto each re-attributed cluster as `source_text`)
                    let mut byte_ranges: Vec<(
                        usize,
                        usize,
                        Arc<StyleProperties>,
                        ContentIndex,
                        Option<NodeId>,
                        Option<bool>,
                        usize,
                        Arc<str>,
                    )> = Vec::with_capacity(coalesce_count);

                    for item in &visual_items[idx..coalesce_end] {
                        let start = merged_text.len();
                        merged_text.push_str(&item.text);
                        let end = merged_text.len();
                        if let LogicalItem::Text {
                            style: s,
                            source: src,
                            source_node_id: nid,
                            marker_position_outside: mpo,
                            text: itext,
                            ..
                        } = &item.logical_source
                        {
                            byte_ranges.push((
                                start,
                                end,
                                s.clone(),
                                *src,
                                *nid,
                                *mpo,
                                item.run_byte_offset,
                                itext.clone(),
                            ));
                        }
                    }

                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "[TextLayout] Coalescing {} text runs ({} bytes) into single shaping \
                             call",
                            coalesce_count,
                            merged_text.len()
                        )));
                    }

                    let direction = if bidi_level.is_rtl() {
                        BidiDirection::Rtl
                    } else {
                        BidiDirection::Ltr
                    };
                    let language = script_to_language(script, &merged_text);

                    // Shape the merged text using the first item's font (layout is identical
                    // for all coalesced items since layout_hash matches).
                    let shaped_clusters_result: Result<Vec<ShapedCluster>, LayoutError> =
                        match &style.font_stack {
                            FontStack::Ref(font_ref) => shape_text_correctly(
                                &merged_text,
                                script,
                                language,
                                direction,
                                font_ref,
                                style,
                                *source,
                                *source_node_id,
                            ),
                            FontStack::Stack(selectors) => {
                                let cache_key = FontChainKey::from_selectors(selectors);
                                let resolved_on_miss;
                                let font_chain = if let Some(c) = font_chain_cache.get(&cache_key) {
                                    c
                                } else {
                                    resolved_on_miss = resolve_chain_on_miss(&cache_key, fc_cache);
                                    &resolved_on_miss
                                };
                                // Per-character font fallback: split text by font coverage
                                shape_with_font_fallback(
                                    &merged_text,
                                    script,
                                    language,
                                    direction,
                                    style,
                                    *source,
                                    *source_node_id,
                                    font_chain,
                                    fc_cache,
                                    loaded_fonts,
                                )
                            }
                        };

                    let shaped_clusters = shaped_clusters_result?;

                    // Restore original per-span styles to each cluster based on byte position.
                    // Each ShapedCluster's source_cluster_id.start_byte_in_run is the byte
                    // offset within the merged text — we use byte_ranges to find which
                    // original run it belongs to and reassign its style, source info, etc.
                    for cluster in shaped_clusters {
                        let byte_pos = cluster.source_cluster_id.start_byte_in_run as usize;
                        // Find the original run this cluster's first byte falls into
                        let orig = byte_ranges
                            .iter()
                            .find(|(start, end, ..)| byte_pos >= *start && byte_pos < *end);
                        let mut cluster = cluster;
                        if let Some((
                            range_start,
                            _,
                            orig_style,
                            orig_source,
                            orig_nid,
                            orig_mpo,
                            orig_run_offset,
                            orig_text,
                        )) = orig
                        {
                            // Reassign rendering-affecting style (color, background, etc.)
                            cluster.style = orig_style.clone();
                            cluster.source_content_index = *orig_source;
                            cluster.source_node_id = *orig_nid;
                            // Fix the byte offset to be relative to the original logical run:
                            // (position within the merged text - this run's start in the merge)
                            // + this visual run's offset within its logical run (bidi split).
                            cluster.source_cluster_id.source_run = orig_source.run_index;
                            cluster.source_cluster_id.start_byte_in_run =
                                (byte_pos - range_start + *orig_run_offset) as u32;
                            cluster.style = orig_style.clone();
                            // §3.2 3c: the finalized offset is item-relative, so
                            // stamp the item's shared text Arc it slices into.
                            cluster.source_text = orig_text.clone();
                            if let Some(is_outside) = orig_mpo {
                                cluster.marker_position_outside = Some(*is_outside);
                            }
                        }
                        shaped.push(ShapedItem::Cluster(cluster));
                    }

                    idx = coalesce_end;
                    continue;
                }

                // ── SINGLE ITEM PATH (no coalescing) ──
                _total_runs += 1;
                _shape_calls += 1;
                let direction = if item.bidi_level.is_rtl() {
                    BidiDirection::Rtl
                } else {
                    BidiDirection::Ltr
                };

                let language = script_to_language(item.script, &item.text);

                // Shape text using either FontRef directly or fontconfig-resolved font
                let shaped_clusters_result: Result<Vec<ShapedCluster>, LayoutError> =
                    match &style.font_stack {
                        FontStack::Ref(font_ref) => {
                            unsafe {
                                crate::az_mark(0x60820_u32, 0x0000_0001_u32);
                            } // [g121] Ref arm
                              // For FontRef, use the font directly without fontconfig
                            if let Some(msgs) = debug_messages {
                                msgs.push(LayoutDebugMessage::info(format!(
                                    "[TextLayout] Using direct FontRef for text: '{}'",
                                    item.text.chars().take(30).collect::<String>()
                                )));
                            }
                            shape_text_correctly(
                                &item.text,
                                item.script,
                                language,
                                direction,
                                font_ref,
                                style,
                                *source,
                                *source_node_id,
                            )
                        }
                        FontStack::Stack(selectors) => {
                            unsafe {
                                crate::az_mark(0x60820_u32, 0x0000_0002_u32);
                            } // [g121] Stack arm
                              // Build FontChainKey and resolve through fontconfig
                            let cache_key = FontChainKey::from_selectors(selectors);
                            unsafe {
                                crate::az_mark(0x60824_u32, font_chain_cache.len() as u32);
                            } // [g121] chain map len

                            // Look up the pre-resolved font chain. (2026-06-10: the g122
                            // by_find/by_only fallback chain is GONE — the historic miss was a
                            // KEY-CONSTRUCTION divergence (duplicated families on the query side,
                            // deduped on the store side), fixed by routing every key build through
                            // FontChainKey::from_selectors. Verified lifted: lookup path = get.)
                            let resolved_on_miss;
                            let font_chain = if let Some(c) = font_chain_cache.get(&cache_key) {
                                c
                            } else {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::info(format!(
                                        "[TextLayout] Font chain not pre-resolved for {:?} - \
                                         resolving on demand",
                                        cache_key.font_families
                                    )));
                                }
                                resolved_on_miss = resolve_chain_on_miss(&cache_key, fc_cache);
                                &resolved_on_miss
                            };

                            // Per-character font fallback: split text by font coverage
                            shape_with_font_fallback(
                                &item.text,
                                item.script,
                                language,
                                direction,
                                style,
                                *source,
                                *source_node_id,
                                font_chain,
                                fc_cache,
                                loaded_fonts,
                            )
                        }
                    };

                let mut shaped_clusters = shaped_clusters_result?;

                // Re-base cluster byte offsets to the logical run. Shaping produced
                // `start_byte_in_run` relative to this visual run's `text`; when bidi
                // split the logical run into several visual runs, add the visual run's
                // offset so every cluster ID is unique + matches caret byte positions.
                let run_byte_offset = item.run_byte_offset as u32;
                if run_byte_offset != 0 {
                    for cluster in &mut shaped_clusters {
                        cluster.source_cluster_id.start_byte_in_run = cluster
                            .source_cluster_id
                            .start_byte_in_run
                            .saturating_add(run_byte_offset);
                    }
                }

                // §3.2 3c: offsets are item-relative from here on — stamp
                // the logical item's shared text Arc that `text()` slices.
                if let LogicalItem::Text {
                    text: item_text, ..
                } = &item.logical_source
                {
                    for cluster in &mut shaped_clusters {
                        cluster.source_text = item_text.clone();
                    }
                }

                // Set marker flag on all clusters if this is a marker
                if let Some(is_outside) = marker_position_outside {
                    for cluster in &mut shaped_clusters {
                        cluster.marker_position_outside = Some(*is_outside);
                    }
                }

                shaped.extend(shaped_clusters.into_iter().map(ShapedItem::Cluster));
            }
            // +spec:display-property:df076b - tab-size rendering and inline-level line breaking
            // "If the tab size is zero, preserved tabs are not rendered."
            // "Otherwise, each preserved tab is rendered as a horizontal shift that lines up
            //  the start edge of the next glyph with the next tab stop."
            // "Tab stops occur at points that are multiples of the tab size from the starting
            //  content edge of the preserved tab's nearest block container ancestor."
            LogicalItem::Tab { source, style } => {
                if style.tab_size == 0.0 {
                    // Tab size zero: tab is not rendered (zero width)
                    shaped.push(ShapedItem::Tab {
                        source: *source,
                        bounds: Rect {
                            x: 0.0,
                            y: 0.0,
                            width: 0.0,
                            height: 0.0,
                        },
                    });
                } else {
                    // TODO: use actual font's space_width via ParsedFontTrait::get_space_width()
                    // once we thread font resolution into the shaping phase for tab stops.
                    // For now, approximate space advance as 0.5 * font_size (typical for Latin
                    // fonts).
                    let space_advance_approx = style.font_size_px * SPACE_WIDTH_RATIO;
                    // +spec:text-alignment-spacing:5a5efd - tab-size includes letter-spacing and
                    // word-spacing
                    let ls = style.letter_spacing.resolve_px(style.font_size_px);
                    let ws = style.word_spacing.resolve_px(style.font_size_px);
                    // Tab stop interval: tab_size * (space advance + letter-spacing + word-spacing)
                    let tab_interval = style.tab_size * (space_advance_approx + ls + ws);
                    // Calculate current advance to find next tab stop
                    let current_advance: f32 = shaped
                        .iter()
                        .map(|item| match item {
                            ShapedItem::Cluster(c) => c.advance,
                            ShapedItem::Tab { bounds, .. } => bounds.width,
                            ShapedItem::Object { bounds, .. } => bounds.width,
                            _ => 0.0,
                        })
                        .sum();
                    // Next tab stop = next multiple of tab_interval from content edge
                    let next_tab_stop =
                        ((current_advance / tab_interval).floor() + 1.0) * tab_interval;
                    let mut tab_width = next_tab_stop - current_advance;
                    // "If this distance is less than 0.5ch, then the subsequent tab stop is used
                    // instead."
                    let half_ch = space_advance_approx * 0.5;
                    if tab_width < half_ch {
                        tab_width += tab_interval;
                    }
                    shaped.push(ShapedItem::Tab {
                        source: *source,
                        bounds: Rect {
                            x: 0.0,
                            y: 0.0,
                            width: tab_width,
                            height: 0.0,
                        },
                    });
                }
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                // CSS Ruby Layout (§3): the annotation (ruby-text) is laid out at its used
                // `font-size` — the UA default is `RUBY_ANNOTATION_FONT_SCALE` of the base —
                // and centered over the base, with the ruby box reserving the WIDER of the
                // two inline-sizes and stacking the annotation line above the base line.
                //
                // Both the base and the annotation are shaped to obtain their REAL inline
                // advances (no `chars * font_size * 0.6` fudge). The annotation is shaped at
                // the scaled style so its width reflects the smaller glyphs.
                let base_font_size = style.font_size_px;
                let annotation_font_size = base_font_size * RUBY_ANNOTATION_FONT_SCALE;

                let mut annotation_props = (**style).clone();
                annotation_props.font_size_px = annotation_font_size;
                let annotation_style = Arc::new(annotation_props);

                // Fallback estimate (only when shaping fails / no font chain): 1em per char
                // is a closer CJK approximation than the old 0.6 ratio.
                let base_width = measure_run_advance(
                    base_text,
                    style,
                    item.script,
                    *source,
                    font_chain_cache,
                    fc_cache,
                    loaded_fonts,
                )
                .unwrap_or_else(|| base_text.chars().count() as f32 * base_font_size);
                let annotation_width = measure_run_advance(
                    ruby_text,
                    &annotation_style,
                    item.script,
                    *source,
                    font_chain_cache,
                    fc_cache,
                    loaded_fonts,
                )
                .unwrap_or_else(|| ruby_text.chars().count() as f32 * annotation_font_size);

                let base_line_height = style.line_height.resolve(base_font_size, 0.0, 0.0, 0.0, 0);
                let annotation_line_height =
                    annotation_style
                        .line_height
                        .resolve(annotation_font_size, 0.0, 0.0, 0.0, 0);
                // The ruby box reserves the wider inline-size, and stacks the annotation
                // line (at its smaller font-size) above the base line.
                let (reserved_width, reserved_height) = ruby_reserved_box(
                    base_width,
                    annotation_width,
                    base_line_height,
                    annotation_line_height,
                );

                // TODO2: the annotation glyphs are now correctly sized + reserve vertical
                // space above the base, but are not yet emitted as a separately positioned
                // (centered) run — `ShapedItem::Object` carries only the base `StyledRun`.
                // Rendering the centered annotation needs a ruby-aware `ShapedItem` variant
                // (rendering-structural change); deferred to keep this change layout-safe.
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: reserved_width,
                        height: reserved_height,
                    },
                    baseline_offset: 0.0,
                    content: InlineContent::Text(StyledRun {
                        text: Arc::from(base_text.as_str()),
                        style: style.clone(),
                        logical_start_byte: 0,
                        source_node_id: None,
                    }),
                });
            }
            LogicalItem::CombinedText {
                style,
                source,
                text,
            } => {
                let language = script_to_language(item.script, &item.text);

                // +spec:width-calculation:657f75 - convert full-width chars to non-full-width
                // before compression +spec:width-calculation:d0a295 - full-width
                // digit conversion example (e.g. "23" stays narrow) When combined
                // text has more than one typographic character unit, full-width
                // characters (U+FF01..U+FF5E) are converted to their
                // ASCII equivalents (U+0021..U+007E) before compression.
                let text = if text.chars().count() > 1 {
                    let converted: String = text
                        .chars()
                        .map(|c| {
                            let cp = c as u32;
                            if (0xFF01..=0xFF5E).contains(&cp) {
                                // Reverse of text-transform: full-width
                                char::from_u32(cp - 0xFF01 + 0x0021).unwrap_or(c)
                            } else {
                                c
                            }
                        })
                        .collect();
                    converted
                } else {
                    text.clone()
                };

                // +spec:width-calculation:1ed84d - OpenType compression (half-width/third-width
                // substitution) is delegated to the font shaping layer via
                // shape_text()

                // Shape CombinedText using either FontRef directly or fontconfig-resolved font
                let glyphs: Vec<Glyph> = match &style.font_stack {
                    FontStack::Ref(font_ref) => {
                        // For FontRef, use the font directly without fontconfig
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[TextLayout] Using direct FontRef for CombinedText: '{}'",
                                text.chars().take(30).collect::<String>()
                            )));
                        }
                        font_ref.shape_text(
                            &text,
                            item.script,
                            language,
                            BidiDirection::Ltr,
                            style.as_ref(),
                        )?
                    }
                    FontStack::Stack(selectors) => {
                        // Build FontChainKey and resolve through fontconfig
                        let cache_key = FontChainKey::from_selectors(selectors);

                        let resolved_on_miss;
                        let font_chain = if let Some(c) = font_chain_cache.get(&cache_key) {
                            c
                        } else {
                            resolved_on_miss = resolve_chain_on_miss(&cache_key, fc_cache);
                            &resolved_on_miss
                        };

                        // Per-character font fallback for CombinedText
                        let segments = split_text_by_font_coverage(&text, font_chain, loaded_fonts);
                        let mut all_glyphs = Vec::new();
                        for (seg_start, seg_end, font_id) in &segments {
                            let Some(font) = loaded_fonts.get(font_id) else {
                                continue;
                            };
                            let segment_text = &text[*seg_start..*seg_end];
                            let mut seg_glyphs = font.shape_text(
                                segment_text,
                                item.script,
                                language,
                                BidiDirection::Ltr,
                                style.as_ref(),
                            )?;
                            // Fix byte offsets for glyphs
                            if *seg_start > 0 {
                                for g in &mut seg_glyphs {
                                    g.logical_byte_index += *seg_start;
                                    g.cluster += *seg_start as u32;
                                }
                            }
                            all_glyphs.extend(seg_glyphs);
                        }
                        if all_glyphs.is_empty() {
                            idx += 1;
                            continue;
                        }
                        all_glyphs
                    }
                };

                let shaped_glyphs: ShapedGlyphVec = glyphs
                    .into_iter()
                    .map(|g| ShapedGlyph {
                        kind: GlyphKind::Character,
                        glyph_id: g.glyph_id,
                        script: g.script,
                        font_hash: g.font_hash,
                        font_metrics: g.font_metrics,
                        cluster_offset: 0,
                        advance: g.advance,
                        kerning: g.kerning,
                        offset: g.offset,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                    })
                    .collect();

                // +spec:block-formatting-context:dc4549 - text-combine-upright compression: UA may
                // scale composition to match 水 advance height
                let total_width: f32 = shaped_glyphs.iter().map(|g| g.advance + g.kerning).sum();
                // +spec:inline-formatting-context:8c5969 - text-combine-upright baseline centering
                // The composition forms a 1em square. Per spec, its baseline must be
                // chosen so the square is centered between the text-over and text-under
                // baselines of the parent inline box. We approximate by using font_size
                // as the square height and centering it (baseline_offset = em_size / 2).
                let em_size = style.font_size_px;
                let bounds = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: total_width,
                    height: em_size,
                };

                shaped.push(ShapedItem::CombinedBlock {
                    style: style.clone(),
                    source: *source,
                    glyphs: shaped_glyphs,
                    bounds,
                    baseline_offset: em_size / 2.0,
                });
            }
            LogicalItem::Object {
                content, source, ..
            } => {
                let (bounds, baseline) = measure_inline_object(content)?;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds,
                    baseline_offset: baseline,
                    content: content.clone(),
                });
            }
            LogicalItem::Break { source, break_info } => {
                shaped.push(ShapedItem::Break {
                    source: *source,
                    break_info: *break_info,
                });
            }
        }
        idx += 1;
    }

    Ok(shaped)
}

/// Returns true if `c` is a hanging punctuation stop or comma per CSS Text 3 §8.2.1.
// +spec:hanging-punctuation - full stop/comma character list per CSS Text 3 §8.2.1
pub(super) const fn is_hanging_punctuation_char(c: char) -> bool {
    matches!(
        c,
        ','      | // U+002C COMMA
        '.'      | // U+002E FULL STOP
        '\u{060C}' | // ARABIC COMMA
        '\u{06D4}' | // ARABIC FULL STOP
        '\u{3001}' | // IDEOGRAPHIC COMMA
        '\u{3002}' | // IDEOGRAPHIC FULL STOP
        '\u{FF0C}' | // FULLWIDTH COMMA
        '\u{FF0E}' | // FULLWIDTH FULL STOP
        '\u{FE50}' | // SMALL COMMA
        '\u{FE51}' | // SMALL IDEOGRAPHIC COMMA
        '\u{FE52}' | // SMALL FULL STOP
        '\u{FF61}' | // HALFWIDTH IDEOGRAPHIC FULL STOP
        '\u{FF64}' // HALFWIDTH IDEOGRAPHIC COMMA
    )
}

/// Helper to check if a cluster contains only hanging punctuation.
// +spec:box-model:8bbcd1 - non-zero inline-axis borders/padding between hangable glyph and line
// edge prevent hanging
/// +spec:inline-formatting-context:135be2 - hanging punctuation placed outside the line box
/// +spec:intrinsic-sizing:407d8b - hanging glyphs not counted in intrinsic size computation
pub(super) fn is_hanging_punctuation(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        if c.glyphs.len() == 1 {
            c.text()
                .chars()
                .next()
                .is_some_and(is_hanging_punctuation_char)
        } else {
            false
        }
    } else {
        false
    }
}

#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
pub(super) fn shape_text_correctly<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: Language,
    direction: BidiDirection,
    font: &T, // Changed from &Arc<T>
    style: &Arc<StyleProperties>,
    source_index: ContentIndex,
    source_node_id: Option<NodeId>,
) -> Result<Vec<ShapedCluster>, LayoutError> {
    unsafe {
        crate::az_mark(0x60864_u32, 0xC0DE_0864_u32);
    } // [g123] shape_text_correctly ENTERED
    let glyphs = font.shape_text(text, script, language, direction, style.as_ref())?;
    unsafe {
        crate::az_mark(0x60868_u32, (glyphs.len() as u32) | 0x8000_0000_u32);
    } // [g123] font.shape_text returned (high bit set); low bits = glyph count

    if glyphs.is_empty() {
        return Ok(Vec::new());
    }

    let mut clusters = Vec::new();

    // Group glyphs by cluster ID from the shaper.
    let mut current_cluster_glyphs = Vec::new();
    let mut cluster_id = glyphs[0].cluster;
    let mut cluster_start_byte_in_text = glyphs[0].logical_byte_index;

    for glyph in glyphs {
        if glyph.cluster != cluster_id {
            // Finalize previous cluster
            let advance = current_cluster_glyphs
                .iter()
                .map(|g: &Glyph| g.advance)
                .sum();

            // Safely extract cluster text - handle cases where byte indices may be out of order
            // (can happen with RTL text or complex GSUB reordering)
            let (start, end) = if cluster_start_byte_in_text <= glyph.logical_byte_index {
                (cluster_start_byte_in_text, glyph.logical_byte_index)
            } else {
                (glyph.logical_byte_index, cluster_start_byte_in_text)
            };
            let cluster_text = text.get(start..end).unwrap_or("");

            clusters.push(ShapedCluster {
                flags: ClusterFlags::classify(cluster_text),
                // §3.2 3c: placeholder — the shaping loop stamps the real
                // item Arc after the bidi re-base; the LENGTH is final now.
                source_text: empty_arc_str(),
                source_byte_len: u16::try_from(cluster_text.len()).unwrap_or(u16::MAX),
                source_cluster_id: GraphemeClusterId {
                    source_run: source_index.run_index,
                    start_byte_in_run: cluster_id,
                },
                source_content_index: source_index,
                source_node_id,
                glyphs: current_cluster_glyphs
                    .iter()
                    .map(|g| {
                        // Calculate cluster_offset safely
                        let cluster_offset = if g.logical_byte_index >= cluster_start_byte_in_text {
                            (g.logical_byte_index - cluster_start_byte_in_text) as u32
                        } else {
                            0
                        };
                        ShapedGlyph {
                            kind: if g.glyph_id == 0 {
                                GlyphKind::NotDef
                            } else {
                                GlyphKind::Character
                            },
                            glyph_id: g.glyph_id,
                            script: g.script,
                            font_hash: g.font_hash,
                            font_metrics: g.font_metrics,
                            cluster_offset,
                            advance: g.advance,
                            kerning: g.kerning,
                            vertical_advance: g.vertical_advance,
                            vertical_offset: g.vertical_bearing,
                            offset: g.offset,
                        }
                    })
                    .collect(),
                advance,
                direction,
                style: style.clone(),
                marker_position_outside: None,
                is_first_fragment: true,
                is_last_fragment: true,
            });
            current_cluster_glyphs.clear();
            cluster_id = glyph.cluster;
            cluster_start_byte_in_text = glyph.logical_byte_index;
        }
        current_cluster_glyphs.push(glyph);
    }

    // Finalize the last cluster
    if !current_cluster_glyphs.is_empty() {
        let advance = current_cluster_glyphs
            .iter()
            .map(|g: &Glyph| g.advance)
            .sum();
        let cluster_text = text.get(cluster_start_byte_in_text..).unwrap_or("");
        clusters.push(ShapedCluster {
            flags: ClusterFlags::classify(cluster_text),
            // §3.2 3c: placeholder — stamped by the shaping loop (see above).
            source_text: empty_arc_str(),
            source_byte_len: u16::try_from(cluster_text.len()).unwrap_or(u16::MAX),
            source_cluster_id: GraphemeClusterId {
                source_run: source_index.run_index,
                start_byte_in_run: cluster_id,
            },
            source_content_index: source_index,
            source_node_id,
            glyphs: current_cluster_glyphs
                .iter()
                .map(|g| {
                    // Calculate cluster_offset safely
                    let cluster_offset = if g.logical_byte_index >= cluster_start_byte_in_text {
                        (g.logical_byte_index - cluster_start_byte_in_text) as u32
                    } else {
                        0
                    };
                    ShapedGlyph {
                        kind: if g.glyph_id == 0 {
                            GlyphKind::NotDef
                        } else {
                            GlyphKind::Character
                        },
                        glyph_id: g.glyph_id,
                        font_hash: g.font_hash,
                        font_metrics: g.font_metrics,
                        script: g.script,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                        cluster_offset,
                        advance: g.advance,
                        kerning: g.kerning,
                        offset: g.offset,
                    }
                })
                .collect(),
            advance,
            direction,
            style: style.clone(),
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        });
    }

    Ok(clusters)
}

/// Measures a non-text object, returning its bounds and baseline offset.
pub(super) fn measure_inline_object(item: &InlineContent) -> Result<(Rect, f32), LayoutError> {
    match item {
        InlineContent::Image(img) => {
            let size = img.display_size.unwrap_or(img.intrinsic_size);
            Ok((
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                img.baseline_offset,
            ))
        }
        InlineContent::Shape(shape) => Ok({
            let size = shape.shape_def.get_size();
            (
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                shape.baseline_offset,
            )
        }),
        InlineContent::Space(space) => Ok((
            Rect {
                x: 0.0,
                y: 0.0,
                width: space.width,
                height: 0.0,
            },
            0.0,
        )),
        InlineContent::Marker { .. } => {
            // Markers are treated as text content, not measurable objects
            Err(LayoutError::InvalidText(
                "Marker is text content, not a measurable object".into(),
            ))
        }
        _ => Err(LayoutError::InvalidText("Not a measurable object".into())),
    }
}

// --- Stage 4 Implementation: Vertical Text ---

/// Applies orientation and vertical metrics to glyphs if the writing mode is vertical.
// +spec:block-formatting-context:227171 - vertical glyph orientation with fallback vertical metrics
// +spec:block-formatting-context:df20a5 - mixed vertical orientation dispatch
// (TextOrientation::Mixed)
pub(super) fn apply_text_orientation(
    items: Arc<Vec<ShapedItem>>,
    constraints: &UnifiedConstraints,
) -> Arc<Vec<ShapedItem>> {
    if !constraints.is_vertical() {
        return items;
    }

    let mut oriented_items = Vec::with_capacity(items.len());
    let writing_mode = constraints.writing_mode.unwrap_or_default();

    for item in items.iter() {
        match item {
            ShapedItem::Cluster(cluster) => {
                let mut new_cluster = cluster.clone();
                let mut total_vertical_advance = 0.0;

                for glyph in &mut new_cluster.glyphs {
                    // Use the vertical metrics already computed during shaping
                    // If they're zero, use fallback values
                    if glyph.vertical_advance > 0.0 {
                        total_vertical_advance += glyph.vertical_advance;
                    } else {
                        // Fallback: use line height for vertical advance
                        let fallback_advance = cluster
                            .style
                            .line_height
                            .resolve_with_metrics(cluster.style.font_size_px, &glyph.font_metrics);
                        glyph.vertical_advance = fallback_advance;
                        // Center the glyph horizontally as a fallback
                        glyph.vertical_offset = Point {
                            x: -glyph.advance / 2.0,
                            y: 0.0,
                        };
                        total_vertical_advance += fallback_advance;
                    }
                }
                // The cluster's `advance` now represents vertical advance.
                new_cluster.advance = total_vertical_advance;
                oriented_items.push(ShapedItem::Cluster(new_cluster));
            }
            // Non-text objects also need their advance axis swapped.
            ShapedItem::Object {
                source,
                bounds,
                baseline_offset,
                content,
            } => {
                let mut new_bounds = *bounds;
                std::mem::swap(&mut new_bounds.width, &mut new_bounds.height);
                oriented_items.push(ShapedItem::Object {
                    source: *source,
                    bounds: new_bounds,
                    baseline_offset: *baseline_offset,
                    content: content.clone(),
                });
            }
            _ => oriented_items.push(item.clone()),
        }
    }

    Arc::new(oriented_items)
}
