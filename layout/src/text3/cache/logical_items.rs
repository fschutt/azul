//! Stages 1 and 2: content to logical items, and bidi reordering into visual order.

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

// --- Stage 1 Implementation ---
#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if the scan cursor advances past the end of `text` (an internal invariant).
pub fn create_logical_items(
    content: &[InlineContent],
    style_overrides: &[StyleOverride],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<LogicalItem> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering create_logical_items (Refactored) ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input content length: {}",
            content.len()
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input overrides length: {}",
            style_overrides.len()
        )));
    }

    let mut items: Vec<LogicalItem> = Vec::new();
    let mut style_cache: HashMap<u64, Arc<StyleProperties>> = HashMap::new();

    // 1. Organize overrides for fast lookup per run.
    let mut run_overrides: HashMap<u32, HashMap<u32, &PartialStyleProperties>> = HashMap::new();
    for override_item in style_overrides {
        run_overrides
            .entry(override_item.target.run_index)
            .or_default()
            .insert(override_item.target.item_index, &override_item.style);
    }

    for (run_idx, inline_item) in content.iter().enumerate() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Processing content run #{run_idx}"
            )));
        }

        // Extract marker information if this is a marker
        let marker_position_outside = match inline_item {
            InlineContent::Marker {
                position_outside, ..
            } => Some(*position_outside),
            _ => None,
        };

        // [az-web-lift FIX 2026-06-06] Handle the common Text/Marker case via a STANDALONE `if let`
        // (a simple discriminant compare) instead of the first arm of the multi-way `match` below.
        // The remill lift mis-routes that multi-way InlineContent switch (LLVM's `subs/csel`-clamp
        // lowering): a Text(disc 0) variant lands in the `_`/Object arm → `inline_item.clone()` →
        // `<InlineContent as Clone>::clone` ALSO mis-routes to its Vec-clone arm → reads a heap ptr
        // as a Vec len → ×8 → ~789 MB alloc → BumpAlloc memset OOB. A standalone if-let lowers to a
        // single cmp/beq the lift handles correctly, so Text reaches its real body. Native
        // unaffected.
        if let InlineContent::Text(run) | InlineContent::Marker { run, .. } = inline_item {
            let text = &run.text;
            if text.is_empty() {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "  Run is empty, skipping.".to_string(),
                    ));
                }
                continue;
            }
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!("  Run text: '{text}'")));
            }

            let current_run_overrides = run_overrides.get(&(run_idx as u32));
            let mut boundaries = BTreeSet::new();
            boundaries.insert(0);
            boundaries.insert(text.len());

            // --- Stateful Boundary Generation ---
            // web-lift FIX + perf: this scan_cursor walk ONLY inserts boundaries for
            // per-char style overrides (Rule 2) or text-combine-upright digit runs (Rule 1).
            // For plain text (no overrides AND no combine-upright) it inserts NOTHING and just
            // walks char-by-char via `scan_cursor += current_char.len_utf8()` — which the web
            // lift mis-advances (overshoot → slice_start_index_len_fail OOB; stall → infinite
            // loop). Skip the whole walk in that common case so `boundaries` stays {0, len}.
            let needs_scan =
                current_run_overrides.is_some() || run.style.text_combine_upright.is_some();
            let mut scan_cursor = 0;
            while needs_scan && scan_cursor < text.len() {
                let style_at_cursor = current_run_overrides
                    .and_then(|o| o.get(&(scan_cursor as u32)))
                    .map_or_else(
                        || (*run.style).clone(),
                        |partial| run.style.apply_override(partial),
                    );

                let current_char = text[scan_cursor..].chars().next().unwrap();

                // +spec:containing-block:e4d9de - text-combine-upright digit run rules: digits
                // sharing an ancestor with same value form one sequence across box boundaries
                // +spec:inline-formatting-context:f65029 - text-combine-upright text run rules:
                // combine consecutive digits not interrupted by box boundary
                // Rule 1: Multi-character features take precedence.
                // +spec:containing-block:9a26bd - text-combine-upright digit runs scoped by
                // ancestor style boundaries
                if let Some(TextCombineUpright::Digits(max_digits)) =
                    style_at_cursor.text_combine_upright
                {
                    if max_digits > 0 && current_char.is_ascii_digit() {
                        let digit_chunk: String = text[scan_cursor..]
                            .chars()
                            .take(max_digits as usize)
                            .take_while(char::is_ascii_digit)
                            .collect();

                        let end_of_chunk = scan_cursor + digit_chunk.len();
                        boundaries.insert(scan_cursor);
                        boundaries.insert(end_of_chunk);
                        scan_cursor = end_of_chunk; // Jump past the entire sequence
                        continue;
                    }
                }

                // Rule 2: If no multi-char feature, check for a normal single-grapheme
                // override.
                if current_run_overrides
                    .and_then(|o| o.get(&(scan_cursor as u32)))
                    .is_some()
                {
                    let grapheme_len = text[scan_cursor..]
                        .graphemes(true)
                        .next()
                        .unwrap_or("")
                        .len();
                    boundaries.insert(scan_cursor);
                    boundaries.insert(scan_cursor + grapheme_len);
                    scan_cursor += grapheme_len;
                    continue;
                }

                // Rule 3: No special features or overrides at this point, just advance one
                // char.
                scan_cursor += current_char.len_utf8();
            }

            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    "  Boundaries: {boundaries:?}"
                )));
            }

            // --- Chunk Processing ---
            for (start, end) in boundaries.iter().zip(boundaries.iter().skip(1)) {
                let (start, end) = (*start, *end);
                if start >= end {
                    continue;
                }

                let text_slice = &text[start..end];
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  Processing chunk from {start} to {end}: '{text_slice}'"
                    )));
                }

                let style_to_use = current_run_overrides
                    .and_then(|o| o.get(&(start as u32)))
                    .map_or_else(
                        || run.style.clone(),
                        |partial_style| {
                            if let Some(msgs) = debug_messages {
                                msgs.push(LayoutDebugMessage::info(format!(
                                    "  -> Applying override at byte {start}"
                                )));
                            }
                            let mut hasher = DefaultHasher::new();
                            Arc::as_ptr(&run.style).hash(&mut hasher);
                            partial_style.hash(&mut hasher);
                            style_cache
                                .entry(hasher.finish())
                                .or_insert_with(|| {
                                    Arc::new(run.style.apply_override(partial_style))
                                })
                                .clone()
                        },
                    );

                // +spec:block-formatting-context:9e7c79 - text-combine-upright combines multiple
                // characters into 1em in vertical writing +spec:containing-block:
                // 2b399b - text-combine-upright digits: combine ASCII digit sequences within
                // max_digits limit; box boundaries implicitly prevent cross-box combination
                // +spec:display-contents:644c78 - text-combine-upright run boundary check:
                // if a combinable run boundary is due only to inline box boundaries,
                // and adjacent chars would form a longer combinable sequence, do not combine
                // +spec:white-space-processing:409d90 - text-combine-upright combined text: white
                // space at start/end processed as in inline-block
                let is_combinable_chunk = match &style_to_use.text_combine_upright {
                    Some(TextCombineUpright::All) => !text_slice.is_empty(),
                    Some(TextCombineUpright::Digits(max_digits)) => {
                        *max_digits > 0
                            && !text_slice.is_empty()
                            && text_slice.chars().all(|c| c.is_ascii_digit())
                            && text_slice.chars().count() <= *max_digits as usize
                    }
                    _ => false,
                };

                if is_combinable_chunk {
                    // Trim leading/trailing white space like an inline-block
                    let trimmed = text_slice.trim();
                    let combined_text = if trimmed.is_empty() {
                        text_slice.to_string()
                    } else {
                        trimmed.to_string()
                    };
                    items.push(LogicalItem::CombinedText {
                        source: ContentIndex {
                            run_index: run_idx as u32,
                            item_index: start as u32,
                        },
                        text: combined_text,
                        style: style_to_use,
                    });
                } else {
                    items.push(LogicalItem::Text {
                        source: ContentIndex {
                            run_index: run_idx as u32,
                            item_index: start as u32,
                        },
                        // §3.2 3c: the item text is an Arc so shaped
                        // clusters can SHARE it. The whole-run chunk
                        // (the overwhelmingly common case — overrides
                        // and combine-upright are the only splitters)
                        // aliases the StyledRun's own Arc: zero new
                        // allocations; override segments mint one Arc
                        // per segment, same cost as the old String.
                        text: if start == 0 && end == text.len() {
                            run.text.clone()
                        } else {
                            Arc::from(text_slice)
                        },
                        style: style_to_use,
                        marker_position_outside,
                        source_node_id: run.source_node_id,
                    });
                }
            }
        } else {
            match inline_item {
                // line breaking class characters must be treated as forced line breaks
                InlineContent::LineBreak(break_info) => {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "  LineBreak: {break_info:?}"
                        )));
                    }
                    items.push(LogicalItem::Break {
                        source: ContentIndex {
                            run_index: run_idx as u32,
                            item_index: 0,
                        },
                        break_info: *break_info,
                    });
                }
                // Handle tab characters
                InlineContent::Tab { style } => {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info("  Tab character".to_string()));
                    }
                    items.push(LogicalItem::Tab {
                        source: ContentIndex {
                            run_index: run_idx as u32,
                            item_index: 0,
                        },
                        style: style.clone(),
                    });
                }
                // Other cases (Image, Shape, Space, Ruby). Text/Marker are handled by the `if let`
                // above (so they never reach here at runtime); `_` keeps this inner match
                // exhaustive.
                _ => {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(
                            "  Run is not text, creating generic LogicalItem.".to_string(),
                        ));
                    }
                    items.push(LogicalItem::Object {
                        source: ContentIndex {
                            run_index: run_idx as u32,
                            item_index: 0,
                        },
                        content: inline_item.clone(),
                    });
                }
            }
        }
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting create_logical_items, created {} items ---",
            items.len()
        )));
    }
    items
}

// --- Stage 2 Implementation ---

// +spec:inline-block:d47971 - unicode-bidi:plaintext uses P2/P3 heuristic for base direction
// (implemented via get_base_direction) +spec:writing-modes:287491 - BiDi reordering and base
// direction detection (Appendix A text processing order) when determining base direction,
// consistent with their neutral bidi treatment
#[must_use]
pub fn get_base_direction_from_logical(logical_items: &[LogicalItem]) -> BidiDirection {
    let first_strong = logical_items.iter().find_map(|item| {
        if let LogicalItem::Text { text, .. } = item {
            Some(unicode_bidi::get_base_direction(&**text))
        } else {
            None
        }
    });

    match first_strong {
        Some(unicode_bidi::Direction::Rtl) => BidiDirection::Rtl,
        _ => BidiDirection::Ltr,
    }
}

// +spec:containing-block:149255 - bidi reordering produces inline box fragments that may separate
// in wide containing blocks +spec:containing-block:c7c08f - bidi reordering produces inline box
// fragments that may be adjacent in narrow containing blocks +spec:containing-block:2936ae - bidi
// reordering splits inline boxes into visual fragments (CSS Writing Modes 4 §2.4.5)
// +spec:display-property:0cdbd3 - bidi reordering splits inline boxes into visual runs; each run is
// shaped/formatted independently +spec:display-property:0d62a2 - bidi reordering of inline content
// respects block direction and unicode-bidi embedding +spec:display-property:10f9cd - bidi
// reordering splits and reorders inline box fragments +spec:display-property:58b30a - bidi
// paragraph breaks within inline boxes: each IFC does independent bidi analysis, so splitting an
// inline box at a paragraph boundary naturally closes/reopens bidi embeddings
// +spec:display-property:ecd935 - inline boxes split and reordered for uniform bidi flow
// +spec:writing-modes:330b8f - text ordered according to Unicode bidi algorithm after white-space
// processing +spec:writing-modes:7a9e7d - bidi control translation: text passed to unicode_bidi for
// reordering +spec:writing-modes:8e7281 - unicode-bidi property: bidi control codes inserted via
// BidiInfo
#[allow(clippy::match_same_arms)]
// enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't
// merge)
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if bidi reordering fails.
pub fn reorder_logical_items(
    logical_items: &[LogicalItem],
    base_direction: BidiDirection,
    unicode_bidi: UnicodeBidi,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<VisualItem>, LayoutError> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering reorder_logical_items ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input logical items count: {}",
            logical_items.len()
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "Base direction: {base_direction:?}"
        )));
    }

    // +spec:writing-modes:809513 - bidi string built across inline element boundaries;
    // unicode-bidi:normal adds no extra embedding levels
    let mut bidi_str = String::new();
    let mut item_map = Vec::new();
    // Byte offset in `bidi_str` where each logical item's text begins, indexed
    // by logical item index. Used to re-base each visual run's byte offset to be
    // relative to its own logical run (see `run_byte_offset`).
    let mut logical_item_starts = Vec::with_capacity(logical_items.len());
    for (idx, item) in logical_items.iter().enumerate() {
        // +spec:containing-block:1fdc31 - inline boxes with unicode-bidi:normal are transparent to
        // bidi algorithm +spec:display-property:074abf - inline boxes transparent to bidi
        // when unicode-bidi:normal +spec:display-property:354966 - unicode-bidi control
        // code injection for inline boxes +spec:display-property:8409d3 - inline-level
        // elements with unicode-bidi:normal have no effect on bidi ordering; embed creates an
        // embedding +spec:display-property:89464a - inline boxes with unicode-bidi:normal
        // don't open embedding levels, so direction has no effect on bidi reordering
        // +spec:display-property:d47971 - bidi control codes should be injected at inline box
        // boundaries based on unicode-bidi + direction +spec:display-property:de657b - bidi
        // control codes injected for display:inline boxes per unicode-bidi value
        // +spec:display-property:f01a81 - bidi-override should prepend LRO/RLO and append PDF per
        // unicode-bidi CSS property (not yet implemented) are treated as neutral characters
        // in the bidi algorithm. Replaced elements with +spec:display-property:fcb011 -
        // unicode-bidi values on inline boxes insert bidi control codes
        // +spec:display-property:89095f - isolate/bidi-override/isolate-override/plaintext
        // semantics +spec:writing-modes:d490bf - direction only affects reordering when
        // unicode-bidi is embed/override (not yet enforced for inline elements)
        // display:inline are also neutral unless unicode-bidi != normal (not yet implemented).
        // +spec:display-property:b4756e - replaced inline elements treated as neutral bidi chars;
        // embed/bidi-override exception not yet implemented (would make them strong chars).
        // U+FFFC (OBJECT REPLACEMENT CHARACTER) is a neutral bidi character.
        // +spec:display-property:df11ef - atomic inlines treated as neutral bidi characters
        // (U+FFFC) Replaced elements with display:inline are also neutral unless
        // unicode-bidi != normal.
        let text = match item {
            LogicalItem::Text { text, .. } => text,
            LogicalItem::CombinedText { text, .. } => text.as_str(),
            _ => "\u{FFFC}",
        };
        let start_byte = bidi_str.len();
        logical_item_starts.push(start_byte);
        bidi_str.push_str(text);
        for _ in start_byte..bidi_str.len() {
            item_map.push(idx);
        }
    }

    if bidi_str.is_empty() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Bidi string is empty, returning.".to_string(),
            ));
        }
        return Ok(Vec::new());
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Constructed bidi string: '{bidi_str}'"
        )));
    }

    // +spec:display-property:1a6075 - paragraph embedding level set from direction property per
    // UAX9 HL1 +spec:containing-block:0d4914 - unicode-bidi: plaintext exception
    // When the containing block has unicode-bidi: plaintext, use None so the
    // Unicode bidi algorithm applies P2/P3 heuristics instead of the HL1 override
    let bidi_level = if unicode_bidi == UnicodeBidi::Plaintext {
        None
    } else if base_direction == BidiDirection::Rtl {
        Some(Level::rtl())
    } else {
        Some(Level::ltr())
    };
    // +spec:writing-modes:15bf17 - bidi isolation handled by unicode_bidi UAX #9 implementation
    let bidi_info = BidiInfo::new(&bidi_str, bidi_level);
    let para = &bidi_info.paragraphs[0];
    let (levels, visual_runs) = bidi_info.visual_runs(para, para.range.clone());

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "Bidi visual runs generated:".to_string(),
        ));
        for (i, run_range) in visual_runs.iter().enumerate() {
            let level = levels[run_range.start].number();
            let slice = &bidi_str[run_range.start..run_range.end];
            msgs.push(LayoutDebugMessage::info(format!(
                "  Run {i}: range={run_range:?}, level={level}, text='{slice}'"
            )));
        }
    }

    // TODO(text3-review): RTL glyph-level visual reversal is NOT applied.
    // `visual_runs` orders the RUNS visually (left-to-right), but the loop below
    // emits each run's content in LOGICAL byte order, and shaping/positioning then
    // place clusters left-to-right in that logical order. For an RTL run this is
    // wrong: the first logical character must land at the LARGEST visual x. The
    // shaped clusters of each RTL run therefore need to be reversed (UBA rule L2,
    // applied per run at the glyph level AFTER shaping — a single logical Text item
    // shapes into multiple clusters, so it cannot be reversed here at the item
    // level). This must compose with the run-level ordering already done here
    // (naively re-running full L2 on top would double-reverse RTL-base paragraphs),
    // and `UnifiedLayout::get_selection_rects` must additionally split a selection
    // into one visual rect per directional segment. Deferred as a coherent
    // cross-cutting change; see failing tests text3_brutal_shaping::
    // {hebrew_run_is_rtl_reversed_and_33px_wide, bidi_mixed_run_is_80px_and_reverses_hebrew}
    // and text3_brutal_selection::bidi_selection_over_rtl_run_splits_into_multiple_rects.
    let mut visual_items = Vec::new();
    for run_range in visual_runs {
        let bidi_level = BidiLevel::new(levels[run_range.start].number());
        let mut sub_run_start = run_range.start;

        for i in (run_range.start + 1)..run_range.end {
            if item_map[i] != item_map[sub_run_start] {
                let logical_idx = item_map[sub_run_start];
                let logical_item = &logical_items[logical_idx];
                let text_slice = &bidi_str[sub_run_start..i];
                visual_items.push(VisualItem {
                    logical_source: logical_item.clone(),
                    bidi_level,
                    script: crate::text3::script::detect_script(text_slice)
                        .unwrap_or(Script::Latin),
                    text: text_slice.to_string(),
                    run_byte_offset: sub_run_start - logical_item_starts[logical_idx],
                });
                sub_run_start = i;
            }
        }

        let logical_idx = item_map[sub_run_start];
        let logical_item = &logical_items[logical_idx];
        let text_slice = &bidi_str[sub_run_start..run_range.end];
        visual_items.push(VisualItem {
            logical_source: logical_item.clone(),
            bidi_level,
            script: crate::text3::script::detect_script(text_slice).unwrap_or(Script::Latin),
            text: text_slice.to_string(),
            run_byte_offset: sub_run_start - logical_item_starts[logical_idx],
        });
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "Final visual items produced:".to_string(),
        ));
        for (i, item) in visual_items.iter().enumerate() {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Item {}: level={}, text='{}'",
                i,
                item.bidi_level.level(),
                item.text
            )));
        }
        msgs.push(LayoutDebugMessage::info(
            "--- Exiting reorder_logical_items ---".to_string(),
        ));
    }
    Ok(visual_items)
}

/// Unicode Bidi Algorithm rule L2, applied at the glyph/cluster level for one line.
///
/// `reorder_logical_items` already placed the level RUNS of the paragraph in
/// visual order (rule L2 at the run level, via `unicode_bidi::visual_runs`), but
/// left the clusters *within* each run in LOGICAL order. To finish L2 the clusters
/// of every RTL (odd-level) run must be reversed so the run reads right-to-left.
///
/// We reverse each maximal contiguous run of clusters that share the same
/// direction, flipping only the RTL ones. Re-running full L2 over the whole line
/// instead would double-reverse the run order that is already correct. Grouping
/// by direction is exact here: under implicit bidi (no explicit embedding
/// controls, which azul does not inject) two runs of the same direction are never
/// visually adjacent — a higher even level nests inside its odd parent and a lower
/// level separates two same-parity runs — so a "same-direction" group is always a
/// single real level run. An atomic inline (an `Object`, U+FFFC - a neutral)
/// takes its direction from the clusters around it, rules N1 / N2: the
/// direction of both neighbours where they agree, the paragraph's (`base`)
/// otherwise and at the line's edges - so two inline-blocks of an RTL
/// paragraph are reversed like its letters (Cerberus's `<td dir="rtl">`
/// columns; they were run boundaries and stayed in logical order). Breaks and
/// tabs act as run boundaries. Applied per line, so a wrapped RTL run reorders
/// correctly per line.
pub(super) fn apply_l2_visual_reversal(line_items: &mut [ShapedItem], base: BidiDirection) {
    let cluster_dir = |it: &ShapedItem| it.as_cluster().map(|c| c.direction);
    let directions: Vec<Option<BidiDirection>> = (0..line_items.len())
        .map(|k| match &line_items[k] {
            ShapedItem::Cluster(c) => Some(c.direction),
            ShapedItem::Object { .. } => {
                let before = line_items[..k].iter().rev().find_map(cluster_dir);
                let after = line_items[k + 1..].iter().find_map(cluster_dir);
                match (before.unwrap_or(base), after.unwrap_or(base)) {
                    (b, a) if b == a => Some(b),
                    _ => Some(base),
                }
            }
            _ => None,
        })
        .collect();
    let mut i = 0;
    while i < line_items.len() {
        let Some(dir) = directions[i] else {
            i += 1;
            continue;
        };
        let mut j = i + 1;
        while j < line_items.len() && directions[j] == Some(dir) {
            j += 1;
        }
        if dir == BidiDirection::Rtl {
            line_items[i..j].reverse();
        }
        i = j;
    }
}

pub(super) fn perform_bidi_analysis<'a>(
    styled_runs: &'a [TextRunInfo<'_>],
    full_text: &'a str,
    force_lang: Option<Language>,
) -> (Vec<VisualRun<'a>>, BidiDirection) {
    if full_text.is_empty() {
        return (Vec::new(), BidiDirection::Ltr);
    }

    let bidi_info = BidiInfo::new(full_text, None);
    let para = &bidi_info.paragraphs[0];
    let base_direction = if para.level.is_rtl() {
        BidiDirection::Rtl
    } else {
        BidiDirection::Ltr
    };

    // Create a map from each byte index to its original styled run.
    let mut byte_to_run_index: Vec<usize> = vec![0; full_text.len()];
    for (run_idx, run) in styled_runs.iter().enumerate() {
        let start = run.logical_start;
        let end = start + run.text.len();
        for slot in &mut byte_to_run_index[start..end] {
            *slot = run_idx;
        }
    }

    let mut final_visual_runs = Vec::new();
    let (levels, visual_run_ranges) = bidi_info.visual_runs(para, para.range.clone());

    for range in visual_run_ranges {
        let bidi_level = levels[range.start];
        let mut sub_run_start = range.start;

        // Iterate through the bytes of the visual run to detect style changes.
        for i in (range.start + 1)..range.end {
            if byte_to_run_index[i] != byte_to_run_index[sub_run_start] {
                // Style boundary found. Finalize the previous sub-run.
                let original_run_idx = byte_to_run_index[sub_run_start];
                let script = crate::text3::script::detect_script(&full_text[sub_run_start..i])
                    .unwrap_or(Script::Latin);
                final_visual_runs.push(VisualRun {
                    text_slice: &full_text[sub_run_start..i],
                    style: styled_runs[original_run_idx].style.clone(),
                    logical_start_byte: sub_run_start,
                    bidi_level: BidiLevel::new(bidi_level.number()),
                    language: force_lang.unwrap_or_else(|| {
                        script_to_language(script, &full_text[sub_run_start..i])
                    }),
                    script,
                });
                // Start a new sub-run.
                sub_run_start = i;
            }
        }

        // Add the last sub-run (or the only one if no style change occurred).
        let original_run_idx = byte_to_run_index[sub_run_start];
        let script = crate::text3::script::detect_script(&full_text[sub_run_start..range.end])
            .unwrap_or(Script::Latin);

        final_visual_runs.push(VisualRun {
            text_slice: &full_text[sub_run_start..range.end],
            style: styled_runs[original_run_idx].style.clone(),
            logical_start_byte: sub_run_start,
            bidi_level: BidiLevel::new(bidi_level.number()),
            script,
            language: force_lang.unwrap_or_else(|| {
                script_to_language(script, &full_text[sub_run_start..range.end])
            }),
        });
    }

    (final_visual_runs, base_direction)
}
