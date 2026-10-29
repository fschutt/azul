//! Stage 4: breaking shaped items into lines, hyphenation, and positioning each line.

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

// Stub type when hyphenation is disabled
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct Standard;

#[cfg(not(feature = "text_layout_hyphenation"))]
impl Standard {
    /// Stub hyphenate method that returns no breaks
    pub fn hyphenate<'a>(&'a self, _word: &'a str) -> StubHyphenationBreaks {
        StubHyphenationBreaks { breaks: Vec::new() }
    }
}

/// Result of hyphenation (stub when feature is disabled)
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct StubHyphenationBreaks {
    pub breaks: Vec<usize>,
}

// +spec:text-alignment-spacing:25e82a - text-align shorthand resolves text-align-all /
// text-align-last
/// Resolve effective text alignment for a line, handling text-align-last per CSS Text §6.3.
/// For the last line (or lines before forced breaks), text-align-last overrides text-align.
/// When text-align-last is auto (default), justify falls back to start; others use text-align.
// +spec:text-alignment-spacing:bca77d - text-align-last auto falls back to text-align-all,
// justify→start +spec:line-breaking:9b10d2 - text-align-last applies to last line and lines before
// forced breaks
/// +spec:text-alignment-spacing:8d88ce - text-align-last overrides justify on last line/forced
/// break
pub(crate) fn resolve_effective_alignment(
    text_align: TextAlign,
    text_align_last: TextAlign,
    is_last_or_forced: bool,
) -> TextAlign {
    if is_last_or_forced {
        if text_align_last == TextAlign::default() {
            if text_align == TextAlign::Justify {
                TextAlign::Start
            } else {
                text_align
            }
        } else {
            text_align_last
        }
    } else {
        text_align
    }
}

/// How far below a fragment's bottom a line may end and still fit it: float
/// rounding, not overflow.
const LINE_FIT_EPSILON: f32 = 0.01;

/// A paragraph as the line breaker sees it: its shaped items turned for its
/// writing mode, with the constraints it is laid out in.
///
/// Every layout of a paragraph makes it here from the paragraph's shaped
/// items, its full layout, its intrinsic sizes and each relayout after an
/// edit alike, and breaks it from `cursor`, so none of them can see the
/// paragraph apart from the others.
#[derive(Debug)]
pub struct ParagraphToBreak<'c> {
    /// The shaped items, oriented for a vertical writing mode
    /// (`apply_text_orientation`): a cluster's advance runs along the line.
    pub items: Arc<Vec<ShapedItem>>,
    /// The constraints the paragraph is laid out in.
    constraints: &'c UnifiedConstraints,
}

impl<'c> ParagraphToBreak<'c> {
    /// The paragraph of `shaped_items` laid out in `constraints`.
    #[must_use]
    pub fn new(shaped_items: Arc<Vec<ShapedItem>>, constraints: &'c UnifiedConstraints) -> Self {
        Self {
            items: apply_text_orientation(shaped_items, constraints),
            constraints,
        }
    }

    /// The constraints the paragraph is laid out in.
    #[must_use]
    pub const fn constraints(&self) -> &'c UnifiedConstraints {
        self.constraints
    }

    /// A line breaker's cursor at the paragraph's start, which breaks it in
    /// the word-break, hyphens and line-break of its constraints.
    #[must_use]
    pub fn cursor(&self) -> BreakCursor<'_> {
        BreakCursor::for_constraints(&self.items, self.constraints)
    }
}

/// Performs layout for a single fragment, consuming items from a `BreakCursor`.
///
/// This function contains the core line-breaking and positioning logic, but is
/// designed to operate on a portion of a larger content stream and within the
/// constraints of a single geometric area (a fragment).
///
/// The loop terminates when either the fragment is filled (e.g., runs out of
/// vertical space) or the content stream managed by the `cursor` is exhausted.
///
/// # CSS Inline Layout Module Level 3 Implementation
///
/// This function implements the inline formatting context as described in:
/// <https://www.w3.org/TR/css-inline-3/#inline-formatting-context>
///
/// ## § 2.1 Layout of Line Boxes
/// "In general, the line-left edge of a line box touches the line-left edge of its
/// containing block and the line-right edge touches the line-right edge of its
/// containing block, and thus the logical width of a line box is equal to the inner
/// logical width of its containing block."
///
/// [ISSUE] `available_width` should be set to the containing block's inner width,
/// but is currently defaulting to 0.0 in `UnifiedConstraints::default()`.
/// This causes premature line breaking.
///
/// ## § 2.2 Layout Within Line Boxes
/// The layout process follows these steps:
/// 1. Baseline Alignment: All inline-level boxes are aligned by their baselines
/// 2. Content Size Contribution: Calculate layout bounds for each box
/// 3. Line Box Sizing: Size line box to fit aligned layout bounds
/// 4. Content Positioning: Position boxes within the line box
///
/// ## Missing Features:
/// - § 3 Baselines and Alignment Metrics: Only basic baseline alignment implemented
/// - § 4 Baseline Alignment: vertical-align property not fully supported
/// - § 5 Line Spacing: line-height implemented, but line-fit-edge missing
/// - § 6 Trimming Leading: text-box-trim not implemented
#[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if fragment layout fails.
pub fn perform_fragment_layout<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<'_>,
    logical_items: &[LogicalItem],
    fragment_constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> Result<UnifiedLayout, LayoutError> {
    const MAX_EMPTY_SEGMENTS: usize = 1000; // Maximum allowed consecutive empty segments
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering perform_fragment_layout ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Constraints: available_width={:?}, available_height={:?}, columns={}, text_wrap={:?}",
            fragment_constraints.available_width,
            fragment_constraints.available_height,
            fragment_constraints.columns,
            fragment_constraints.text_wrap
        )));
    }

    // For TextWrap::Balance, use Knuth-Plass algorithm for optimal line breaking
    // This produces more visually balanced lines at the cost of more computation.
    // It breaks against one measure, the full width: a room shapes or
    // exclusions cut, whose segments differ from line to line, is broken by
    // the greedy breaker below.
    if fragment_constraints.text_wrap == TextWrap::Balance
        && fragment_constraints.shape_boundaries.is_empty()
        && fragment_constraints.shape_exclusions.is_empty()
    {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Using Knuth-Plass algorithm for text-wrap: balance".to_string(),
            ));
        }

        // The paragraph starts in this fragment only when the cursor is at its
        // start - the greedy path's `is_first_formatted_line` below. Read it
        // before draining: a continuation fragment of a flow chain holds no
        // first formatted line, so `text-indent` does not apply to its line 0.
        let starts_paragraph = cursor.next_item_index == 0 && cursor.partial_remainder.is_empty();
        // Get the shaped items from the cursor
        let shaped_items: Vec<ShapedItem> = cursor.drain_remaining();

        // +spec:line-breaking:90c1bd - only auto-hyphenate when language is known and hyphenation
        // resource available
        let hyphenator = hyphenator_for(fragment_constraints);

        // Use the Knuth-Plass algorithm for optimal line breaking
        return Ok(crate::text3::knuth_plass::kp_layout(
            &shaped_items,
            logical_items,
            fragment_constraints,
            hyphenator.as_deref(),
            fonts,
            starts_paragraph,
        ));
    }

    // +spec:intrinsic-sizing:57e02d - hyphenation opportunities considered in min-content sizing
    let hyphenator = hyphenator_for(fragment_constraints);

    let mut positioned_items = Vec::new();
    let mut layout_bounds = Rect::default();

    let num_columns = fragment_constraints.columns.max(1);
    let total_column_gap = fragment_constraints.column_gap * (num_columns - 1) as f32;

    // CSS Inline Layout § 2.1: "the logical width of a line box is equal to the inner
    // logical width of its containing block"
    //
    // Handle the different available space modes:
    // - Definite(width): Use the specified width for column calculation
    // - MinContent: Force line breaks at word boundaries, return widest word width
    // - MaxContent: Use a large value to allow content to expand naturally
    //
    // IMPORTANT: For MinContent, we do NOT use 0.0 (which would break after every character).
    // Instead, we use a large width but track the is_min_content flag to force word-level
    // line breaks in the line breaker. The actual min-content width is the width of the
    // widest resulting line (typically the widest word).
    let is_min_content = matches!(
        fragment_constraints.available_width,
        AvailableSpace::MinContent
    );
    let is_max_content = matches!(
        fragment_constraints.available_width,
        AvailableSpace::MaxContent
    );

    let column_width = match fragment_constraints.available_width {
        AvailableSpace::Definite(width) => (width - total_column_gap) / num_columns as f32,
        AvailableSpace::MinContent | AvailableSpace::MaxContent => {
            // For intrinsic sizing, use a large width to measure actual content width.
            // The line breaker will handle MinContent specially by breaking after each word.
            f32::MAX / 2.0
        }
    };
    let mut current_column = 0;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Column width calculated: {column_width}"
        )));
    }

    // Use the CSS direction from constraints instead of auto-detecting from text
    // This ensures that mixed-direction text (e.g., "مرحبا - Hello") uses the
    // correct paragraph-level direction for alignment purposes.
    // With unicode-bidi: plaintext, direction is auto-detected from text content
    // per CSS Writing Modes §8.3.
    let base_direction = if fragment_constraints.unicode_bidi == UnicodeBidi::Plaintext {
        // Auto-detect from remaining shaped items' text content
        let remaining = &cursor.items[cursor.next_item_index..];
        let text: String = remaining
            .iter()
            .filter_map(|i| i.as_cluster())
            .map(ShapedCluster::text)
            .collect();
        match unicode_bidi::get_base_direction(text.as_str()) {
            unicode_bidi::Direction::Ltr => BidiDirection::Ltr,
            unicode_bidi::Direction::Rtl => BidiDirection::Rtl,
            // No strong character: fall back to containing block direction
            unicode_bidi::Direction::Mixed => {
                fragment_constraints.direction.unwrap_or(BidiDirection::Ltr)
            }
        }
    } else {
        fragment_constraints.direction.unwrap_or(BidiDirection::Ltr)
    };

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PFLayout] Base direction: {:?} (from CSS), Text align: {:?}",
            base_direction, fragment_constraints.text_align
        )));
    }

    // +spec:multi-column - column-fill:balance (the initial/default value): content is
    // balanced so the columns are as short and as equal in height as possible. The column loop
    // below only advances to the next column once a column reaches `available_height` — but a
    // block on a page is handed the whole page height as available space, which the short
    // content never reaches, so every line lands in column 0 (a single visual column). Fix:
    // measure the total line count up front (a cheap dry run of the line breaker over a CLONED
    // cursor at the column width) and give each column an equal share of lines; the balanced
    // budget (content_lines / N) is far below the page-height threshold so it takes precedence.
    // Gated on num_columns>1 with no shape boundaries and non-intrinsic sizing — exactly the
    // otherwise-broken case — so single-column and shaped/intrinsic layouts are untouched
    // (zero blast radius). column-fill:auto (fill-then-advance) is rare and not modelled here.
    // The paragraph's first formatted line is the first line of the fragment
    // whose cursor starts at the paragraph's start; a continuation fragment of
    // a flow starts mid-paragraph and gets no first-line `text-indent`.
    let mut is_first_formatted_line =
        cursor.next_item_index == 0 && cursor.partial_remainder.is_empty();

    let balanced_lines_per_column: Option<usize> = if num_columns > 1
        && fragment_constraints.shape_boundaries.is_empty()
        && !is_min_content
        && !is_max_content
    {
        let mut probe = cursor.clone();
        let mut probe_col_constraints = fragment_constraints.clone();
        probe_col_constraints.available_width = AvailableSpace::Definite(column_width);
        let probe_line_height = fragment_constraints.resolved_line_height();
        // A line consumes at least one shaped item, so the item count bounds the loop.
        let iter_cap = probe.items.len().saturating_mul(4).max(64);
        let mut total_lines = 0usize;
        let mut probe_y = 0.0_f32;
        let mut probe_guard = 0usize;
        let mut probe_first_line = is_first_formatted_line;
        let mut probe_after_forced_break = false;
        while !probe.is_done() && probe_guard < iter_cap {
            probe_guard += 1;
            let mut lc = get_line_constraints(
                probe_y,
                probe_line_height,
                &probe_col_constraints,
                &mut None,
            );
            if lc.segments.is_empty() {
                break;
            }
            indent_line_box(
                &mut lc,
                text_indent_of_line(
                    fragment_constraints,
                    probe_first_line,
                    probe_after_forced_break,
                ),
                base_direction,
            );
            let (probe_line, _) = break_one_line(
                &mut probe,
                &lc,
                false,
                hyphenator.as_deref(),
                fonts,
                fragment_constraints.line_break,
                fragment_constraints.white_space_mode,
                fragment_constraints.overflow_wrap,
            );
            if probe_line.is_empty() {
                break;
            }
            probe_first_line = false;
            probe_after_forced_break = probe_line
                .iter()
                .any(|item| matches!(item, ShapedItem::Break { .. }));
            total_lines += 1;
            probe_y += probe_line_height;
        }
        (total_lines > 0).then(|| total_lines.div_ceil(num_columns as usize).max(1))
    } else {
        None
    };

    // The bottom of the lowest line box that holds glyphs or NO item with a
    // height (a line a lone `<br>` ends), in any column, horizontal modes:
    // what the IFC's height is measured to, see below; and the top of the
    // topmost line box that holds glyphs. A line of only atomic inlines keeps
    // measuring by its items, as it always did.
    let mut line_box_extent = 0.0_f32;
    let mut glyph_line_box_top: Option<f32> = None;
    // +spec:multi-column - this context's share of a multi-column BLOCK
    // container's flow (`ColumnFlow`): a further column starts at each of
    // the given line indices, `advance` further along the inline axis, its
    // first line box at `column_top`. The lines are numbered across the whole
    // flow (a line keeps the index it has in the unsplit layout), and one
    // ending in a forced break still marks the next column's first line.
    // Only for a context without columns of its own, and never while
    // measuring intrinsic sizes.
    let column_flow = fragment_constraints
        .column_flow
        .as_ref()
        .filter(|_| num_columns == 1 && !is_min_content && !is_max_content);
    let column_count = column_flow.map_or(num_columns, |flow| {
        u32::try_from(flow.breaks.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1)
    });
    let mut flow_line_index = 0usize;
    let mut flow_after_forced_break = false;
    // The top of the highest and the bottom of the lowest line box that holds
    // only atomic inlines (no text cluster; an inline-block, an image), any
    // column, horizontal modes: such a line box holds the strut too, see
    // below.
    let mut atomic_line_box_top = f32::MAX;
    let mut atomic_line_box_bottom = f32::MIN;
    // Whether `line-clamp` stopped the lines with content left over: the
    // last line kept then ends in an ellipsis (`OverflowInfo::line_clamped`).
    let mut line_clamped = false;
    'column_loop: while current_column < column_count {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "\n-- Starting Column {current_column} --"
            )));
        }
        let column_start_x = match column_flow {
            Some(flow) => flow.advance * current_column as f32,
            None => (column_width + fragment_constraints.column_gap) * current_column as f32,
        };
        let mut line_top_y = match column_flow {
            Some(flow) if current_column > 0 => flow.column_top,
            _ => 0.0,
        };
        let mut line_index = if column_flow.is_some() {
            flow_line_index
        } else {
            0
        };
        let mut empty_segment_count = 0; // Failsafe counter for infinite loops
        // The lines this column holds so far: the first one stays even when
        // it is taller than the column.
        let mut lines_in_column = 0usize;
        let mut is_after_forced_break = column_flow.is_some() && flow_after_forced_break;
        // +spec:writing-modes:6e22a7 - vertical-rl advances columns (lines) right-to-left.
        // The positioner lays every line out at an increasing block-axis (x) offset from 0,
        // i.e. left-to-right. For vertical-rl we record each line's block band here so we can
        // mirror the block axis once the column's total extent is known (see after the loop).
        let column_item_start = positioned_items.len();
        let mut line_bands: Vec<(usize, f32, f32)> = Vec::new();

        // [g147 az-web-lift] Hard total-iteration cap on the line-build loop. On the remill lift,
        // `cursor.is_done()` (or the empty-segment failsafe) mis-lifts for the NESTED IFC
        // (content.len reads 0 → the cursor is starved but never reports done) → this
        // `while !cursor.is_done()` spins forever → solveLayoutReal HANGS inside
        // perform_fragment_layout. Cap total iterations so the loop always converges (the
        // harness can then read the markers). native is unaffected (far above real
        // line counts). The 0x60BC4 marker exposes the iteration count.
        #[allow(clippy::no_effect_underscore_binding)] // web_lift-gated debug iteration counter
        let mut _az_line_iters: usize = 0;
        // The inline boxes of the last cluster of the line before, for the
        // edges of a span the wrap splits (`position_one_line_between`).
        let mut before_chain: Option<Arc<[InlineBoxEdge]>> = None;
        while !cursor.is_done() {
            #[cfg(feature = "web_lift")]
            {
                _az_line_iters += 1;
                unsafe {
                    crate::az_mark(
                        (0x60BC4) as u32,
                        (_az_line_iters as u32 | 0xC0DE0000) as u32,
                    );
                }
                if _az_line_iters > 4096 {
                    break;
                }
            }
            if let Some(max_height) = fragment_constraints.available_height {
                // A line box is at least one line-height tall, so once less
                // than that is left no further line ends inside the column.
                let full = if lines_in_column == 0 {
                    line_top_y >= max_height
                } else {
                    line_top_y + fragment_constraints.resolved_line_height()
                        > max_height + LINE_FIT_EPSILON
                };
                if full {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "  Column full (pen {line_top_y}, height {max_height}), breaking to \
                             next column."
                        )));
                    }
                    break;
                }
            }

            if let Some(clamp) = fragment_constraints.line_clamp {
                if line_index >= clamp.get() {
                    line_clamped = true;
                    break;
                }
            }

            // +spec:multi-column - column-fill:balance: cap this column at its balanced share of
            // lines so content distributes across columns. The LAST column takes whatever remains
            // (so integer rounding of the per-column budget never drops content).
            if let Some(budget) = balanced_lines_per_column {
                if current_column + 1 < num_columns && line_index >= budget {
                    break;
                }
            }

            // A multi-column flow's next column starts at this line.
            if let Some(flow) = column_flow {
                if flow
                    .breaks
                    .get(current_column as usize)
                    .is_some_and(|&first_of_next| line_index >= first_of_next)
                {
                    break;
                }
            }

            // Create constraints specific to the current column for the line breaker.
            let mut column_constraints = fragment_constraints.clone();
            // For MinContent/MaxContent, preserve the semantic type so the line breaker
            // can handle word-level breaking correctly. Only use Definite for actual widths.
            if is_min_content {
                column_constraints.available_width = AvailableSpace::MinContent;
            } else if is_max_content {
                column_constraints.available_width = AvailableSpace::MaxContent;
            } else {
                column_constraints.available_width = AvailableSpace::Definite(column_width);
            }
            let mut line_constraints = get_line_constraints(
                line_top_y,
                fragment_constraints.resolved_line_height(),
                &column_constraints,
                debug_messages,
            );
            // Whether shapes or exclusions cut the room, read before the
            // indent: the indent is a margin on the line box, which a unit
            // too wide for what it leaves overflows (it never moves the line
            // down). Below every shape no cut is left to move past.
            let room_cut = !is_min_content
                && !is_max_content
                && room_is_cut(&line_constraints, column_width)
                && !below_every_shape(line_top_y, fragment_constraints);
            // CSS Text 3 8.1: the indent is a margin on the line box's start
            // edge - the line is broken, justified and aligned in what is left.
            indent_line_box(
                &mut line_constraints,
                text_indent_of_line(
                    fragment_constraints,
                    is_first_formatted_line,
                    is_after_forced_break,
                ),
                base_direction,
            );

            if line_constraints.segments.is_empty() {
                // No room at this height: a shape or an exclusion covers the
                // whole line. It moves down to where the room can change.
                empty_segment_count += 1;
                if empty_segment_count >= MAX_EMPTY_SEGMENTS
                    || below_every_shape(line_top_y, fragment_constraints)
                {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "  No room for a line at y={line_top_y} or below it: the shapes \
                             leave none. Ending the column."
                        )));
                    }
                    break;
                }
                line_top_y = next_band_top(line_top_y, fragment_constraints);
                continue;
            }

            // Reset counter when we find valid segments
            empty_segment_count = 0;

            // +spec:line-breaking:3bb032 - break-word not considered for min-content intrinsic
            // sizes +spec:overflow:b932c4 - overflow-wrap/word-wrap
            // (normal/break-word/anywhere) and hyphens interaction `anywhere`
            // introduces soft wrap opportunities (min-content = widest cluster),
            // but `break-word` does NOT (min-content = widest unbreakable word).
            let effective_overflow_wrap =
                if is_min_content && fragment_constraints.overflow_wrap == OverflowWrap::Anywhere {
                    OverflowWrap::Anywhere
                } else if is_min_content
                    && fragment_constraints.overflow_wrap == OverflowWrap::BreakWord
                {
                    OverflowWrap::Normal
                } else {
                    fragment_constraints.overflow_wrap
                };

            // CSS Text Module Level 3 § 5 Line Breaking and Word Boundaries
            // https://www.w3.org/TR/css-text-3/#line-breaking
            // +spec:display-property:2608cc - inline box splitting across line boxes, overflow for
            // unsplittable boxes +spec:display-property:ea615c - inline boxes split and
            // distributed across line boxes "When an inline box exceeds the logical
            // width of a line box, it is split into several fragments, which are
            // partitioned across multiple line boxes."
            let cursor_before_line = cursor.position();
            let pieces = match break_line_into_segments(
                cursor,
                &line_constraints,
                room_cut,
                base_direction,
                hyphenator.as_deref(),
                fonts,
                fragment_constraints,
                effective_overflow_wrap,
            ) {
                SegmentedLine::Pieces(pieces) => pieces,
                // The room the shapes and exclusions leave here holds not
                // even the next word: the line moves down to where the room
                // can change (CSS 2.2 9.5: until some content fits).
                SegmentedLine::NoRoom => {
                    line_top_y = next_band_top(line_top_y, fragment_constraints);
                    continue;
                }
                // White space the breaker strips at a line's edge leaves no
                // line box; the content after it starts a line here.
                SegmentedLine::Empty if !cursor.is_done() => continue,
                SegmentedLine::Empty => break,
                SegmentedLine::Stalled => {
                    // Defensive: the breaker always consumes an item while
                    // content is left; it must not loop forever.
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::warning(
                            "  Break returned no items and consumed none with content left. \
                             Ending column."
                                .to_string(),
                        ));
                    }
                    break;
                }
            };

            if let Some(msgs) = debug_messages {
                for piece in &pieces {
                    let text: String = piece
                        .line
                        .iter()
                        .filter_map(|i| i.as_cluster())
                        .map(ShapedCluster::text)
                        .collect();
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[PFLayout] Segment at x={} ({} wide), logical order: [{text}]",
                        piece.segment.start_x, piece.segment.width
                    )));
                }
            }

            // The spans the next line continues: those of this line's last
            // cluster in reading order, wherever the line puts it.
            let line_end_boxes = pieces
                .iter()
                .rev()
                .flat_map(|piece| piece.line.iter().rev())
                .find_map(ShapedItem::as_cluster)
                .map(|c| c.style.inline_boxes.clone());

            // +spec:line-breaking:c59944 - forced line breaks detected for bidi-aware alignment
            let line_ends_with_forced_break = pieces
                .iter()
                .any(|piece| piece.line.iter().any(|item| matches!(item, ShapedItem::Break { .. })));
            let last_piece = pieces.len() - 1;
            let ends_paragraph = cursor.is_done() && !pieces[last_piece].hyphenated;

            // Each segment is a line box of its own for alignment and
            // justification, its items put in visual order (UAX 9 rules L1
            // and L2) on their own; text-align-last takes the paragraph's
            // last one and one a forced break ends. They are positioned left
            // to right.
            let mut segment_lines: Vec<SegmentLine> = pieces
                .into_iter()
                .enumerate()
                .map(|(index, piece)| {
                    let forced = piece
                        .line
                        .iter()
                        .any(|item| matches!(item, ShapedItem::Break { .. }));
                    let is_last_line = index == last_piece && ends_paragraph;
                    SegmentLine {
                        segment: piece.segment,
                        items: piece.line.into_visual(base_direction),
                        text_align: resolve_effective_alignment(
                            fragment_constraints.text_align,
                            fragment_constraints.text_align_last,
                            is_last_line || forced,
                        ),
                        is_last_line,
                    }
                })
                .collect();
            segment_lines.sort_by(|a, b| {
                a.segment
                    .start_x
                    .partial_cmp(&b.segment.start_x)
                    .unwrap_or(Ordering::Equal)
            });

            // The spans on the lines around this one: a span the wrap
            // splits has no edge at the split.
            let after_chain = cursor
                .partial_remainder
                .iter()
                .chain(cursor.items.get(cursor.next_item_index..).unwrap_or(&[]))
                .find_map(ShapedItem::as_cluster)
                .map(|c| c.style.inline_boxes.clone());
            let (mut line_pos_items, line_height) = position_one_line_between(
                &segment_lines,
                before_chain.as_deref(),
                after_chain.as_deref(),
                line_top_y,
                line_index,
                base_direction,
                fragment_constraints,
                debug_messages,
                fonts,
            );

            // +spec:display-property:6c4978 - line-height on block container establishes minimum
            // line box height
            let band_height = line_height.max(fragment_constraints.resolved_line_height());

            // A line box is monolithic (CSS Fragmentation 3): one that ends
            // below the fragment goes to the next fragment or column, and
            // the line before it ends this one. The first line of an empty
            // fragment stays whatever its height, so the flow moves on.
            if let Some(max_height) = fragment_constraints.available_height {
                if lines_in_column > 0 && line_top_y + band_height > max_height + LINE_FIT_EPSILON
                {
                    cursor.restore(cursor_before_line);
                    break;
                }
            }
            lines_in_column += 1;

            before_chain = line_end_boxes.or(before_chain);

            // Track whether the next line follows a forced break
            is_after_forced_break = line_ends_with_forced_break;
            is_first_formatted_line = false;

            for item in &mut line_pos_items {
                item.position.x += column_start_x;
            }

            line_bands.push((line_index, line_top_y, band_height));
            let band_top = line_top_y;
            line_top_y += band_height;
            let holds_glyphs = line_pos_items
                .iter()
                .any(|item| matches!(&item.item, ShapedItem::Cluster(c) if !c.glyphs.is_empty()));
            if holds_glyphs {
                glyph_line_box_top = Some(glyph_line_box_top.map_or(band_top, |t| t.min(band_top)));
                line_box_extent = line_box_extent.max(line_top_y);
            } else if !line_pos_items
                .iter()
                .any(|item| item.item.bounds().height > 0.0)
            {
                line_box_extent = line_box_extent.max(line_top_y);
            }
            // A line of only atomic inlines (a box with a height, no text
            // cluster) is measured by its whole line box, strut included.
            if line_pos_items
                .iter()
                .any(|item| item.item.bounds().height > 0.0)
                && !line_pos_items
                    .iter()
                    .any(|item| matches!(item.item, ShapedItem::Cluster(_)))
            {
                atomic_line_box_top = atomic_line_box_top.min(line_top_y - band_height);
                atomic_line_box_bottom = atomic_line_box_bottom.max(line_top_y);
            }
            line_index += 1;
            positioned_items.extend(line_pos_items);
        }

        // +spec:writing-modes:6e22a7 - vertical-rl column order: mirror the block axis so the
        // FIRST line becomes the RIGHTMOST column and successive lines advance leftward. Each
        // line occupies the block band [t, t + h]; after mirroring within the column's total
        // block extent `block_extent` the band moves to [block_extent - t - h, block_extent - t],
        // which is x += block_extent - 2t - h for every item on that line. The inline (y) axis
        // and within-column glyph stacking are untouched. vertical-lr keeps left-to-right order.
        if fragment_constraints.writing_mode == Some(WritingMode::VerticalRl) {
            let block_extent = line_top_y;
            for item in &mut positioned_items[column_item_start..] {
                if let Some(&(_, t, h)) =
                    line_bands.iter().find(|(li, _, _)| *li == item.line_index)
                {
                    // delta = block_extent - 2t - h (written without a `2.0 * t`
                    // product so the flops lint stays quiet).
                    item.position.x += block_extent - t - t - h;
                }
            }
        }
        flow_line_index = line_index;
        flow_after_forced_break = is_after_forced_break;
        current_column += 1;
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting perform_fragment_layout, positioned {} items ---",
            positioned_items.len()
        )));
    }

    let mut layout = UnifiedLayout {
        items: positioned_items,
        overflow: OverflowInfo {
            line_clamped,
            ..OverflowInfo::default()
        },
    };

    // Calculate bounds on demand via the bounds() method
    let mut calculated_bounds = layout.bounds();

    // +spec:display-property:a0d0ab - an IFC is as tall as its LINE BOXES (CSS 2.2
    // 10.6.3: from the top of the topmost to the bottom of the bottommost). The
    // items' bounds miss two kinds of line box:
    // - one that holds no glyph: the line a lone `<br>` ends (`<div><br></div>`,
    //   Gmail's blank line) is one line-height tall in every browser (a line
    //   ending in a forced break is not a zero-height line box, 9.4.2), and
    //   measured 0 here because a break has no geometry;
    // - one of text smaller than its block: every line box holds the block's
    //   strut (10.8.1), so `<td><span style="font-size: 13px">` in a 16px
    //   document is one 16px line tall (Chrome 18px, the glyphs 15px - every
    //   row of a receipt 3px short, MAILREF8 group E).
    // Both reach down here (`line_box_extent`, from `glyph_line_box_top` for
    // text). A line of only atomic inlines is still measured by its items - an
    // `<svg>` alone on a line keeps its box's height (Chrome adds the strut's
    // descent below it; that moves every icon button and is left for a look
    // pass). The top is otherwise that of the items WITH a height; a break
    // positioned inside a `<span>` has none and sits at the baseline, and
    // measuring from it left `<div><span><br></span></div>` a quarter of a line
    // tall. The vertical modes stack their line boxes along x and keep the item
    // bounds.
    let horizontal = !matches!(
        fragment_constraints.writing_mode,
        Some(
            WritingMode::VerticalRl
                | WritingMode::VerticalLr
                | WritingMode::SidewaysRl
                | WritingMode::SidewaysLr
        )
    );
    if horizontal && line_box_extent > 0.0 {
        let items_top = layout
            .items
            .iter()
            .filter(|item| item.item.bounds().height > 0.0)
            .map(|item| item.position.y)
            .fold(None, |acc: Option<f32>, y| {
                Some(acc.map_or(y, |a| a.min(y)))
            });
        let top = match (items_top, glyph_line_box_top) {
            (Some(items), Some(lines)) => Some(items.min(lines)),
            (items, lines) => items.or(lines),
        };
        if let Some(top) = top {
            // A first line box above its (smaller) glyphs starts the IFC.
            if top < calculated_bounds.y {
                calculated_bounds.height += calculated_bounds.y - top;
                calculated_bounds.y = top;
            }
            calculated_bounds.height = calculated_bounds.height.max(line_box_extent - top);
        } else {
            calculated_bounds.y = 0.0;
            calculated_bounds.height = calculated_bounds.height.max(line_box_extent);
        }
    }

    // CSS 2.1 s10.8: every line box holds a STRUT, a zero-width inline box
    // with the block's font and line-height - a line of only atomic inlines
    // too. A 10px inline-block in a 16px Arial block makes an 18px line in
    // Chrome, and that is what the IFC measures (user ruling 2026-10-03:
    // Chrome is the reference). Its items alone measured 10: the box. An icon
    // that wants its box's height alone says so in CSS, as on the web
    // (`line-height: 0`, `display: block`, a flex container). This is the
    // look pass the line-box note above defers: where it says a line of only
    // atomic inlines keeps its items' height, this block supersedes it.
    if horizontal && atomic_line_box_bottom > atomic_line_box_top {
        let top = calculated_bounds.y.min(atomic_line_box_top);
        let bottom = (calculated_bounds.y + calculated_bounds.height).max(atomic_line_box_bottom);
        calculated_bounds.y = top;
        calculated_bounds.height = bottom - top;
    }

    // Record the unclipped content bounds. `overflow_items` stays empty by
    // design: this positioner places *every* item, so visual overflow is handled
    // at paint time via clipping rather than by dropping items here.
    // TODO(superplan): only populate `overflow_items` if a future positioning
    // path actually discards content that does not fit.
    layout.overflow.unclipped_bounds = calculated_bounds;

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Calculated bounds: width={}, height={} ---",
            calculated_bounds.width, calculated_bounds.height
        )));
    }

    Ok(layout)
}

/// Breaks a single line of items to fit within the given geometric constraints,
/// handling multi-segment lines and hyphenation.
/// Break a single line from the current cursor position.
///
/// # CSS Text Module Level 3 \u00a7 5 Line Breaking and Word Boundaries
/// <https://www.w3.org/TR/css-text-3/#line-breaking>
///
/// Implements the line breaking algorithm:
/// 1. "When an inline box exceeds the logical width of a line box, it is split into several
///    fragments, which are partitioned across multiple line boxes."
///
/// ## \u2705 Implemented Features:
/// - **Break Opportunities**: Identifies word boundaries and break points
/// - **Soft Wraps**: Wraps at spaces between words
/// - **Hard Breaks**: Handles explicit line breaks (\\n)
/// - **Overflow**: If a word is too long, places it anyway to avoid infinite loop
/// - **Hyphenation**: Tries to break long words at hyphenation points (\u00a7 5.4)
///
/// ## \u26a0\ufe0f Known Issues:
/// - If `line_constraints.total_available` is 0.0 (from `available_width: 0.0` bug), every word
///   will overflow, causing single-word lines
/// - This is the symptom visible in the PDF: "List items break extremely early"
///
/// ## \u00a7 5.2 Breaking Rules for Letters
/// \u2705 IMPLEMENTED: Uses Unicode line breaking algorithm
/// - Relies on UAX #14 for break opportunities
/// - Respects non-breaking spaces and zero-width joiners
///
/// ## \u00a7 5.3 Breaking Rules for Punctuation
/// \u26a0\ufe0f PARTIAL: Basic punctuation handling
/// - \u274c TODO: hanging-punctuation is declared in `UnifiedConstraints` but not used here
/// - \u274c TODO: Should implement punctuation trimming at line edges //
///   +spec:intrinsic-sizing:6085cf - hanging glyphs must be excluded from intrinsic size
///   computation
///
/// ## \u00a7 5.4 Hyphenation
/// \u2705 IMPLEMENTED: Automatic hyphenation with hyphenator library
/// - Tries to hyphenate words that overflow
/// - Inserts hyphen glyph at break point
/// - Carries remainder to next line
///
/// ## \u00a7 5.5 Overflow Wrapping
/// \u2705 IMPLEMENTED: Emergency breaking
/// - If line is empty and word doesn't fit, forces at least one item
/// - Prevents infinite loop
/// - This is "overflow-wrap: break-word" behavior
///
/// # Missing Features:
/// - word-break property (normal, break-all, keep-all) - IMPLEMENTED via `BreakCursor.word_break`
/// - \u26a0\ufe0f line-break property: anywhere implemented; loose/normal/strict CJK strictness
///   filtering added via `is_cjk_break_allowed_by_strictness` (§5.3)
/// - \u274c overflow-wrap: anywhere vs break-word distinction
/// - \u2705 white-space: break-spaces handling
// around every typographic character unit including preserved white spaces; with break-spaces
// it allows breaking before the first space of a sequence
// +spec:line-breaking:722f3b - wrapping only at soft wrap opportunities, minimizing overflow
#[allow(clippy::cognitive_complexity)] // cohesive line-break state machine: one branch per CSS line-break case
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if a break unit is unexpectedly empty (an internal invariant).
pub fn break_one_line<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<'_>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    hyphenator: Option<&Standard>,
    fonts: &LoadedFonts<T>,
    line_break: LineBreakStrictness,
    white_space_mode: WhiteSpaceMode,
    overflow_wrap: OverflowWrap,
) -> (LogicalLine, bool) {
    let mut line_items = Vec::new();
    let mut current_width = 0.0;

    if cursor.is_done() {
        return (LogicalLine::default(), false);
    }

    // +spec:white-space-processing:c83dbd - Phase II: collapsible spaces at line start removed,
    // trailing spaces removed, tab stops CSS Text Module Level 3 § 4.1.2: At the beginning of a
    // line, white space is collapsed away. Skip leading whitespace at line start.
    // https://www.w3.org/TR/css-text-3/#white-space-phase-2
    // Per CSS Text 3 §4.1.1/§4.1.2, leading white space at line start is collapsed
    // ONLY for the collapsing white-space modes. Pre / pre-wrap / break-spaces must
    // preserve leading indentation, so only strip for Normal / Nowrap / Pre-line.
    let strip_leading = matches!(
        white_space_mode,
        WhiteSpaceMode::Normal | WhiteSpaceMode::Nowrap | WhiteSpaceMode::PreLine
    );
    if strip_leading {
        while !cursor.is_done() {
            let next_unit = cursor.peek_next_unit();
            if next_unit.is_empty() {
                break;
            }
            if next_unit.len() == 1 && is_collapsible_whitespace(&next_unit[0]) {
                cursor.consume(1);
            } else {
                break;
            }
        }
    }

    // +spec:line-breaking:35817b - white-space: nowrap/pre prevent soft wrap opportunities
    // CSS Text Level 3 § 3: For nowrap and pre, wrapping is suppressed. All content
    // stays on a single line, overflowing if necessary.
    let no_wrap = matches!(
        white_space_mode,
        WhiteSpaceMode::Nowrap | WhiteSpaceMode::Pre
    );

    if no_wrap {
        // No soft wrapping — consume everything onto one line.
        // Only explicit <br>/newline breaks are honored.
        loop {
            let next_unit = cursor.peek_next_unit();
            if next_unit.is_empty() {
                break;
            }
            if let Some(ShapedItem::Break { .. }) = next_unit.first() {
                line_items.push(next_unit[0].clone());
                cursor.consume(1);
                return (LogicalLine::new(line_items), false);
            }
            line_items.extend_from_slice(&next_unit);
            cursor.consume(next_unit.len());
        }
    } else {
        loop {
            // typographic character unit as a soft wrap opportunity; hyphenation is not applied
            let next_unit = if line_break == LineBreakStrictness::Anywhere {
                cursor.peek_next_single_item()
            } else {
                cursor.peek_next_unit()
            };
            if next_unit.is_empty() {
                break; // End of content
            }

            if let Some(ShapedItem::Break { .. }) = next_unit.first() {
                line_items.push(next_unit[0].clone());
                cursor.consume(1);
                return (LogicalLine::new(line_items), false);
            }

            // Min-content: break at EVERY soft-wrap opportunity so each word forms its
            // own line (min-content = widest unbreakable unit). `total_available` is a
            // sentinel (f32::MAX/2) during intrinsic sizing and never overflows, so
            // without this the run would collapse onto one line and min-content would
            // wrongly equal max-content. Every unit ends at a soft wrap opportunity, so
            // once the line holds content the next unit starts a line of its own: after
            // a word, a hyphen ("well-" / "known") or a CJK ideograph, and before a space
            // or another break opportunity of its own, a leading space being stripped
            // at the next line's start (collapsing modes). A word after a space the line
            // keeps (pre-wrap) stays with it.
            if line_constraints.is_min_content
                && !line_items.is_empty()
                && ((next_unit.len() == 1
                    && is_break_opportunity_with_word_break(
                        &next_unit[0],
                        cursor.word_break,
                        cursor.hyphens,
                    ))
                    || !line_items.last().is_some_and(is_word_separator))
            {
                break;
            }
            // The min-content takes every hyphenation break too, a soft
            // hyphen's and the dictionary's: the unit's part up to its first
            // one ends the line, with the hyphen shown there, and the rest
            // keeps the unit's later breaks for the lines after it, as the
            // intrinsic scan measures the unit's fragments.
            if line_constraints.is_min_content {
                let breaks =
                    unit_hyphenation_breaks(&next_unit, cursor, hyphenator, is_vertical, fonts);
                if !breaks.is_empty() {
                    take_hyphenation_break(&mut line_items, cursor, next_unit.len(), breaks, 0);
                    return (LogicalLine::new(line_items), true);
                }
            }

            // Fold the unit onto the line item-by-item in document order - the
            // exact fold the intrinsic max-content scan uses - and compare on the
            // ADDITION side. A per-unit subtotal or a subtract-based test
            // (`unit <= available - current`) re-associates the f32 sum and can
            // wrap a line that fits its own measured max-content by 1 ULP; this
            // form is bit-identical to the measurement, so a box sized to its
            // measurement never wraps. See fold_line_width.
            let width_with_unit = next_unit.iter().fold(current_width, |w, item| {
                fold_line_width(w, item, is_vertical)
            });

            // 2. Can the whole unit fit on the current line?
            if width_with_unit <= line_constraints.total_available {
                line_items.extend_from_slice(&next_unit);
                current_width = width_with_unit;
                cursor.consume(next_unit.len());
            } else {
                let available_width = line_constraints.total_available - current_width;
                // 3. The unit overflows. Can we hyphenate it? It breaks at
                // its soft hyphens (hyphens: manual or auto) or, with none
                // and a dictionary, where the dictionary breaks it, with a
                // hyphen at the end of the line either way: at the last
                // break whose part fits.
                if line_break != LineBreakStrictness::Anywhere {
                    let breaks =
                        unit_hyphenation_breaks(&next_unit, cursor, hyphenator, is_vertical, fonts);
                    if let Some(chosen) = breaks
                        .iter()
                        .rposition(|b| b.width_on_line <= available_width)
                    {
                        take_hyphenation_break(
                            &mut line_items,
                            cursor,
                            next_unit.len(),
                            breaks,
                            chosen,
                        );
                        return (LogicalLine::new(line_items), true);
                    }
                }

                // an otherwise unbreakable sequence at an arbitrary point when no other
                // break points exist. Grapheme clusters stay together; no hyphen inserted.
                // 4. Cannot hyphenate or fit. The line is finished.
                // If the line is empty, we must force at least one item to avoid an infinite loop.
                // With overflow-wrap: anywhere or break-word, we break the unbreakable
                // unit at an arbitrary cluster boundary. With normal, we only force one
                // item to prevent infinite loops (content will overflow).
                if line_items.is_empty() {
                    match overflow_wrap {
                        OverflowWrap::Anywhere | OverflowWrap::BreakWord => {
                            // Emergency break: fit as many clusters as possible on
                            // this line.  Grapheme clusters stay together.
                            //
                            // Per CSS Text 3 §5.5: "an otherwise unbreakable sequence
                            // of characters may be broken at an arbitrary point" when
                            // overflow-wrap is anywhere/break-word.
                            let avail = line_constraints.total_available;
                            for item in &next_unit {
                                // Same fold as the fit test and the intrinsic scan.
                                let width_with_item =
                                    fold_line_width(current_width, item, is_vertical);
                                // Break BEFORE this item if adding it would overflow,
                                // but only if we already have at least one item on the
                                // line (must always make progress).
                                if !line_items.is_empty() && avail > 0.0 && width_with_item > avail
                                {
                                    break;
                                }
                                line_items.push(item.clone());
                                current_width = width_with_item;
                                // When the container is zero-width (avail <= 0), the
                                // break-before check above is skipped (it requires
                                // avail > 0), so every item lands on this one line —
                                // there's nowhere to break TO, content just overflows.
                                // This matches browser behavior for `width: 0`
                                // containers.
                            }
                            let consumed = line_items.len().max(1);
                            if line_items.is_empty() {
                                line_items.push(next_unit[0].clone());
                            }
                            cursor.consume(consumed);
                        }
                        OverflowWrap::Normal => {
                            // overflow-wrap:normal keeps an unbreakable word intact and
                            // lets it overflow the line box — it must NOT be shredded one
                            // grapheme per line. Place the whole unit on this (empty) line.
                            line_items.extend_from_slice(&next_unit);
                            cursor.consume(next_unit.len());
                        }
                    }
                }
                break;
            }
        }
    } // end !no_wrap

    // +spec:white-space-processing:fef250 - Phase II: trailing collapsible spaces and U+1680
    // removed at line end as well as any trailing U+1680 OGHAM SPACE MARK whose white-space is
    // normal/nowrap/pre-line. The line's end is its logical end: the line is still in
    // reading order here, its visual order is made after it is broken.
    let mut line = LogicalLine::new(line_items);
    line.trim_trailing_white_space(white_space_mode);
    (line, false)
}

/// How narrow, in ems of the paragraph's font, a segment the shapes and
/// exclusions cut off may be before it holds no text: a sliver beside a hole
/// would hold a letter or two, cut off from the rest of the line.
const MIN_SEGMENT_EM: f32 = 1.0;

/// One segment of a line as the breaker filled it.
#[derive(Debug)]
pub(super) struct LinePiece {
    /// The segment's room.
    pub(super) segment: LineSegment,
    /// What the breaker put in it, in reading order.
    pub(super) line: LogicalLine,
    /// Whether its last word was hyphenated.
    pub(super) hyphenated: bool,
}

/// What breaking a line into the segments of its room came to.
#[derive(Debug)]
pub(super) enum SegmentedLine {
    /// The segments that hold content, in reading order.
    Pieces(Vec<LinePiece>),
    /// No segment the shapes and exclusions leave holds the next unit; the
    /// cursor stands where it stood.
    NoRoom,
    /// The content ended in white space, which a line's edge strips.
    Empty,
    /// Content is left and nothing was taken (never expected).
    Stalled,
}

/// One segment of a line ready to be positioned: its room and its items in
/// visual order, with how its line box is aligned.
#[derive(Debug, Clone)]
pub struct SegmentLine {
    /// The segment's room.
    pub segment: LineSegment,
    /// Its items, in visual order.
    pub items: VisualLine,
    /// Its alignment, `text-align-last` already taken where it applies.
    pub text_align: TextAlign,
    /// Whether it holds the end of its paragraph (it is not justified).
    pub is_last_line: bool,
}

/// Breaks the next line into the segments of `line_constraints`, each one in
/// turn, in reading order, as a line of its own: no word and no hyphenation
/// crosses from one segment into the next, as the room between them is not
/// the line's.
///
/// Where the room is cut by shapes or exclusions (`room_cut`), a segment
/// narrower than [`MIN_SEGMENT_EM`] holds nothing, and neither does one too
/// narrow for its next unit: the unit goes on to the next segment. A room
/// the shapes do not cut is one segment, and a unit too wide for it
/// overflows it. A forced break ends the whole line.
#[allow(clippy::too_many_arguments)]
pub(super) fn break_line_into_segments<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<'_>,
    line_constraints: &LineConstraints,
    room_cut: bool,
    base_direction: BidiDirection,
    hyphenator: Option<&Standard>,
    fonts: &LoadedFonts<T>,
    constraints: &UnifiedConstraints,
    overflow_wrap: OverflowWrap,
) -> SegmentedLine {
    let start = cursor.position();
    let mut in_reading_order = line_constraints.segments.clone();
    if base_direction == BidiDirection::Rtl {
        in_reading_order.reverse();
    }
    let min_width = MIN_SEGMENT_EM * constraints.strut_font_size;
    let mut pieces = Vec::new();
    'segments: for segment in in_reading_order {
        if room_cut && segment.width + 0.01 < min_width {
            continue;
        }
        let room = LineConstraints {
            segments: vec![segment],
            total_available: segment.width,
            is_min_content: line_constraints.is_min_content,
        };
        // White space a segment's edge strips leaves it empty: it is read
        // again, from the content after the white space.
        while !cursor.is_done() {
            let before = cursor.position();
            let (line, hyphenated) = break_one_line(
                cursor,
                &room,
                false,
                hyphenator,
                fonts,
                constraints.line_break,
                constraints.white_space_mode,
                overflow_wrap,
            );
            if line.is_empty() {
                if cursor.position() == before {
                    return SegmentedLine::Stalled;
                }
                continue;
            }
            let width = line
                .iter()
                .fold(0.0, |width, item| fold_line_width(width, item, false));
            if room_cut && width > segment.width + 0.01 {
                cursor.restore(before);
                continue 'segments;
            }
            let forced = line
                .iter()
                .any(|item| matches!(item, ShapedItem::Break { .. }));
            pieces.push(LinePiece {
                segment,
                line,
                hyphenated,
            });
            if forced {
                break 'segments;
            }
            continue 'segments;
        }
        break;
    }
    if !pieces.is_empty() {
        return SegmentedLine::Pieces(pieces);
    }
    if cursor.is_done() {
        return SegmentedLine::Empty;
    }
    cursor.restore(start);
    if room_cut {
        SegmentedLine::NoRoom
    } else {
        SegmentedLine::Stalled
    }
}

/// Represents a single valid hyphenation point within a word.
#[derive(Debug, Clone)]
pub struct HyphenationBreak {
    /// The number of characters of the word on the line, before the hyphen.
    /// The engine's line breakers do not read it; it is for other callers.
    pub char_len_on_line: usize,
    /// The width the line breaker counts for the line part and the hyphen:
    /// their advances with kerning and the letter- and word-spacing the pen
    /// adds after each (`fold_line_width`), so a part that fits by this width
    /// also fits where the line is positioned.
    pub width_on_line: f32,
    /// The cluster(s) that will remain on the current line.
    pub line_part: Vec<ShapedItem>,
    /// The cluster that represents the hyphen character itself.
    pub hyphen_item: ShapedItem,
    /// The cluster(s) that will be carried over to the next line.
    pub remainder_part: Vec<ShapedItem>,
}

/// Every place the word made of `word_clusters` may be hyphenated, as the
/// clusters on each side and the line part's width.
///
/// A word that holds a soft hyphen breaks before its soft hyphens and
/// nowhere else. Any other word is broken by the `hyphenator`'s dictionary,
/// if there is one, which is handed the word's letters only
/// (`letter_runs`): the comma of "Month," or the quote of "‘ahead" would
/// hide where the word ends or starts from its patterns, and count among
/// the letters a break must leave on either side. Each line part ends in
/// the hyphen `hyphen_after` makes, a soft hyphen's and the dictionary's
/// alike.
///
/// The dictionary names its breaks as byte offsets into the word's text.
/// A break is taken where a cluster starts at that byte; one inside a
/// cluster (between the letters of a ligature, or inside a grapheme) has no
/// cluster boundary to split at and is skipped. Bytes, characters and
/// clusters only coincide for ASCII text with one character per cluster.
#[must_use]
pub fn find_all_hyphenation_breaks<T: ParsedFontTrait>(
    word_clusters: &[ShapedCluster],
    hyphenator: Option<&Standard>,
    is_vertical: bool,
    fonts: &LoadedFonts<T>,
) -> Option<Vec<HyphenationBreak>> {
    if word_clusters.is_empty() {
        return None;
    }

    // The word's text, and the byte at which each of its clusters starts.
    let mut word_string = String::new();
    let mut cluster_starts = Vec::with_capacity(word_clusters.len());
    for cluster in word_clusters {
        cluster_starts.push(word_string.len());
        word_string.push_str(cluster.text());
    }

    // +spec:line-breaking:d7ed93 - language-specific hyphenation rules apply to both auto and
    // explicit (soft hyphen) opportunities
    // A word that holds a soft hyphen breaks only before its soft hyphens.
    // Any other word is broken by the dictionary, each run of letters a word
    // of its own to it, and the breaks are moved back to bytes of the unit.
    let soft_hyphens: Vec<usize> = word_clusters
        .iter()
        .zip(&cluster_starts)
        .filter(|(cluster, _)| is_soft_hyphen(cluster))
        .map(|(_, &start)| start)
        .collect();
    let break_bytes: Vec<usize> = match hyphenator {
        Some(hyphenator) if soft_hyphens.is_empty() => {
            let unit_end = word_string.len();
            letter_runs(word_clusters, apostrophe_is_a_letter(hyphenator))
                .into_iter()
                .flat_map(|run| {
                    let start = cluster_starts[run.start];
                    let end = cluster_starts.get(run.end).copied().unwrap_or(unit_end);
                    hyphenator
                        .hyphenate(&word_string[start..end])
                        .breaks
                        .into_iter()
                        .map(move |byte| start + byte)
                })
                .collect()
        }
        _ => soft_hyphens,
    };
    // The break is before the cluster that starts at its byte.
    let splits: Vec<usize> = break_bytes
        .iter()
        .filter_map(|break_byte| cluster_starts.binary_search(break_byte).ok())
        .collect();
    let possible_breaks = hyphenation_breaks_at(word_clusters, &splits, is_vertical, fonts);
    if possible_breaks.is_empty() {
        None
    } else {
        Some(possible_breaks)
    }
}

/// The hyphenation breaks of the word made of `word_clusters` before the
/// clusters at `splits`, in their order: the clusters on each side, the line
/// part's width and the hyphen ending it.
///
/// A split at the word's start or end breaks nothing, and none is made
/// where `hyphen_after` finds no hyphen.
fn hyphenation_breaks_at<T: ParsedFontTrait>(
    word_clusters: &[ShapedCluster],
    splits: &[usize],
    is_vertical: bool,
    fonts: &LoadedFonts<T>,
) -> Vec<HyphenationBreak> {
    splits
        .iter()
        .filter_map(|&split| {
            if split == 0 || split >= word_clusters.len() {
                return None;
            }
            let hyphen_item = hyphen_after(&word_clusters[split - 1], fonts)?;
            let line_part: Vec<ShapedItem> = word_clusters[..split]
                .iter()
                .map(|c| ShapedItem::Cluster(c.clone()))
                .collect();
            let remainder_part: Vec<ShapedItem> = word_clusters[split..]
                .iter()
                .map(|c| ShapedItem::Cluster(c.clone()))
                .collect();
            let width_on_line = line_part
                .iter()
                .chain(std::iter::once(&hyphen_item))
                .fold(0.0, |width, item| fold_line_width(width, item, is_vertical));
            Some(HyphenationBreak {
                char_len_on_line: word_clusters[..split]
                    .iter()
                    .map(|c| c.text().chars().count())
                    .sum(),
                width_on_line,
                line_part,
                hyphen_item,
                remainder_part,
            })
        })
        .collect()
}

/// The clusters of a unit the line breakers hyphenate, or none.
///
/// None is hyphenated with `hyphens: none`, nor one that ends at a break
/// opportunity of its own (after a hyphen: the line takes it whole up to
/// there), nor one that holds anything but text: an inline-block inside a
/// word would be lost from the line and remainder parts, made of clusters
/// only.
pub(super) fn hyphenatable_clusters(
    unit: &[ShapedItem],
    hyphens: Hyphens,
) -> Option<Vec<ShapedCluster>> {
    if hyphens == Hyphens::None || unit.last().is_none_or(is_break_opportunity) {
        return None;
    }
    unit.iter().map(|item| item.as_cluster().cloned()).collect()
}

/// Where the greedy breaker may hyphenate `unit`, the next unit of `cursor`:
/// the one source of the breaks of its lines and of its min-content, as of
/// the intrinsic scan's.
///
/// The rest of a word carried from the line before keeps the word's own
/// breaks (`BreakCursor::partial_remainder_breaks`); any other unit is
/// broken by `find_all_hyphenation_breaks`, as the intrinsic scan breaks it.
fn unit_hyphenation_breaks<T: ParsedFontTrait>(
    unit: &[ShapedItem],
    cursor: &BreakCursor<'_>,
    hyphenator: Option<&Standard>,
    is_vertical: bool,
    fonts: &LoadedFonts<T>,
) -> Vec<HyphenationBreak> {
    let Some(clusters) = hyphenatable_clusters(unit, cursor.hyphens) else {
        return Vec::new();
    };
    match &cursor.partial_remainder_breaks {
        Some(splits) if unit.len() == cursor.partial_remainder.len() => {
            hyphenation_breaks_at(&clusters, splits, is_vertical, fonts)
        }
        _ => find_all_hyphenation_breaks(&clusters, hyphenator, is_vertical, fonts)
            .unwrap_or_default(),
    }
}

/// How wide the min-content's widest line of the unit made of `unit` is,
/// with the width of each of its items in `advances`.
///
/// The line breakers break a unit at its hyphenation breaks there
/// (`hyphenatable_clusters`, `find_all_hyphenation_breaks`, as
/// `unit_hyphenation_breaks`), each line part ending in its hyphen: the
/// widest of those parts, or the whole unit. A space a line removes at a
/// break is no part of it.
pub(super) fn widest_min_content_piece<T: ParsedFontTrait>(
    unit: &[ShapedItem],
    advances: &[f32],
    hyphens: Hyphens,
    hyphenator: Option<&Standard>,
    is_vertical: bool,
    fonts: &LoadedFonts<T>,
) -> f32 {
    if unit
        .iter()
        .all(|item| is_word_separator(item) && !is_no_break_space(item))
    {
        return 0.0;
    }
    let width_of = |range: core::ops::Range<usize>| -> f32 {
        advances.get(range).map_or(0.0, |part| part.iter().sum())
    };
    let breaks = hyphenatable_clusters(unit, hyphens)
        .and_then(|clusters| {
            find_all_hyphenation_breaks(&clusters, hyphenator, is_vertical, fonts)
        })
        .unwrap_or_default();
    let mut widest = 0.0_f32;
    let mut start = 0;
    for hyphenation in &breaks {
        let split = hyphenation.line_part.len();
        let hyphen = get_item_measure_with_spacing(&hyphenation.hyphen_item, is_vertical).max(0.0);
        widest = widest.max(width_of(start..split) + hyphen);
        start = split;
    }
    widest.max(width_of(start..advances.len()))
}

/// Puts the line part of `breaks[chosen]` on the line, its hyphen ending it,
/// and carries the rest of the unit, `unit_len` items long, to the next
/// line with the unit's later breaks.
fn take_hyphenation_break(
    line_items: &mut Vec<ShapedItem>,
    cursor: &mut BreakCursor<'_>,
    unit_len: usize,
    breaks: Vec<HyphenationBreak>,
    chosen: usize,
) {
    let split = breaks[chosen].line_part.len();
    let later: Vec<usize> = breaks[chosen + 1..]
        .iter()
        .map(|b| b.line_part.len() - split)
        .collect();
    let Some(taken) = breaks.into_iter().nth(chosen) else {
        return;
    };
    line_items.extend(taken.line_part);
    line_items.push(taken.hyphen_item);
    cursor.consume(unit_len);
    cursor.partial_remainder = taken.remainder_part;
    cursor.partial_remainder_breaks = Some(later);
}

/// The hyphen a line breaker puts at the end of a line it breaks inside a
/// word, after `cluster`, the line's last one.
///
/// It is the font's hyphen glyph in the font and style of that cluster;
/// there is none when the cluster has no glyph or its font no hyphen.
pub(super) fn hyphen_after<T: ParsedFontTrait>(
    cluster: &ShapedCluster,
    fonts: &LoadedFonts<T>,
) -> Option<ShapedItem> {
    let glyph = cluster.glyphs.last()?;
    let font = fonts.get_by_hash(glyph.font_hash)?;
    let (hyphen_glyph_id, hyphen_advance) =
        font.get_hyphen_glyph_and_advance(cluster.style.font_size_px)?;
    Some(ShapedItem::Cluster(ShapedCluster {
        flags: ClusterFlags::classify("-"),
        source_text: Arc::from("-"),
        source_byte_len: 1,
        source_cluster_id: GraphemeClusterId {
            source_run: u32::MAX,
            start_byte_in_run: u32::MAX,
        },
        source_content_index: ContentIndex {
            run_index: u32::MAX,
            item_index: u32::MAX,
        },
        source_node_id: None, // Hyphen is generated, not from DOM
        glyphs: smallvec![ShapedGlyph {
            kind: GlyphKind::Hyphen,
            glyph_id: hyphen_glyph_id,
            font_hash: glyph.font_hash,
            font_metrics: glyph.font_metrics,
            cluster_offset: 0,
            script: Script::Latin,
            advance: hyphen_advance,
            kerning: 0.0,
            offset: Point::default(),
            vertical_advance: hyphen_advance,
            vertical_offset: Point::default(),
        }],
        advance: hyphen_advance,
        // The hyphen ends the word it breaks and is reordered with it.
        direction: cluster.direction,
        bidi_level: cluster.bidi_level,
        style: cluster.style.clone(),
        marker_position_outside: None,
        is_first_fragment: true,
        is_last_fragment: true,
    }))
}

/// The runs of letters in a line breaker's unit, as ranges of its clusters.
///
/// These are the words the hyphenation dictionary breaks. A cluster is a
/// letter when its first character is alphabetic, in any script; a cluster
/// that continues the grapheme before it (a combining mark shaped apart from
/// its letter) goes with that grapheme. Every other cluster ends a word: the
/// punctuation around "Month," or "(Egypt", a digit, a dash, and an
/// apostrophe unless the language spells words with it, so "deliver'd" is
/// broken as "deliver".
fn letter_runs(
    word_clusters: &[ShapedCluster],
    apostrophe_in_words: bool,
) -> Vec<core::ops::Range<usize>> {
    let mut runs = Vec::new();
    // The open run's first cluster, and the cluster after its last letter.
    let mut open: Option<(usize, usize)> = None;
    let mut previous_is_letter = false;
    for (index, cluster) in word_clusters.iter().enumerate() {
        let text = cluster.text();
        let is_letter = if cluster.flags.has(ClusterFlags::GRAPHEME_CONTINUATION) {
            previous_is_letter
        } else {
            text.chars().next().is_some_and(char::is_alphabetic)
        };
        previous_is_letter = is_letter;
        let stays_in_word = apostrophe_in_words && matches!(text, "'" | "\u{2019}");
        if is_letter {
            let start = open.map_or(index, |(start, _)| start);
            open = Some((start, index + 1));
        } else if !stays_in_word {
            if let Some((start, end)) = open.take() {
                runs.push(start..end);
            }
        }
    }
    if let Some((start, end)) = open {
        runs.push(start..end);
    }
    runs
}

/// Whether the hyphenation patterns of `hyphenator`'s language spell words
/// with an apostrophe.
///
/// French and Italian elide into the next word ("l'homme"), Ukrainian and
/// Belarusian write the apostrophe inside words, and their patterns say
/// where to break around it; so do those listed here, the languages whose
/// pattern files hold an apostrophe. The patterns of every other language
/// hold none, and the typesetting tradition they come from ends a word at
/// an apostrophe, so an English "deliver'd" is the word "deliver" and an
/// ending.
#[cfg(feature = "text_layout_hyphenation")]
fn apostrophe_is_a_letter(hyphenator: &Standard) -> bool {
    matches!(
        hyphenator.language(),
        HyphenationLanguage::Afrikaans
            | HyphenationLanguage::Albanian
            | HyphenationLanguage::Belarusian
            | HyphenationLanguage::French
            | HyphenationLanguage::Friulan
            | HyphenationLanguage::GreekAncient
            | HyphenationLanguage::GreekMono
            | HyphenationLanguage::GreekPoly
            | HyphenationLanguage::Italian
            | HyphenationLanguage::Occitan
            | HyphenationLanguage::Piedmontese
            | HyphenationLanguage::Romansh
            | HyphenationLanguage::Ukrainian
    )
}

/// Without dictionaries no word is hyphenated, and an apostrophe ends one.
#[cfg(not(feature = "text_layout_hyphenation"))]
const fn apostrophe_is_a_letter(_hyphenator: &Standard) -> bool {
    false
}

/// Positions a single line of items, handling alignment and justification within segments.
///
/// This function is architecturally critical for cache safety. It does not mutate the
/// `advance` or `bounds` of the input `ShapedItem`s. Instead, it applies justification
/// spacing by adjusting the drawing pen's position (`main_axis_pen`).
///
/// # Returns
/// A tuple containing the `Vec` of positioned items and the calculated height of the line box.
/// Position items on a single line after breaking.
///
/// # CSS Inline Layout Module Level 3 \u00a7 2.2 Layout Within Line Boxes
/// <https://www.w3.org/TR/css-inline-3/#layout-within-line-boxes>
///
/// Implements the positioning algorithm:
/// 1. "All inline-level boxes are aligned by their baselines"
/// 2. "Calculate layout bounds for each inline box"
/// 3. "Size the line box to fit the aligned layout bounds"
/// 4. "Position all inline boxes within the line box"
///
/// ## \u2705 Implemented Features:
///
/// ### \u00a7 4 Baseline Alignment (vertical-align)
/// \u26a0\ufe0f PARTIAL IMPLEMENTATION:
/// - \u2705 `baseline`: Aligns box baseline with parent baseline (default)
/// - \u2705 `top`: Aligns top of box with top of line box
/// - \u2705 `middle`: Centers box within line box
/// - \u2705 `bottom`: Aligns bottom of box with bottom of line box
/// - \u274c MISSING: `text-top`, `text-bottom`, `sub`, `super`
/// - \u274c MISSING: `<length>`, `<percentage>` values for custom offset
///
/// ### \u00a7 2.2.1 Text Alignment (text-align)
/// +spec:containing-block:8d5146 - text-align aligns within line box, not viewport/containing block
/// \u2705 IMPLEMENTED:
/// - `left`, `right`, `center`: Physical alignment
/// - `start`, `end`: Logical alignment (respects direction: ltr/rtl)
/// - `justify`: Distributes space between words/characters
/// - `justify-all`: Justifies last line too
///
/// ### \u00a7 7.3 Text Justification (text-justify)
/// \u2705 IMPLEMENTED:
/// - `inter-word`: Adds space between words
/// - `inter-character`: Adds space between characters
/// - `kashida`: Arabic kashida elongation
/// - \u274c MISSING: `distribute` (CJK justification)
///
/// ### CSS Text \u00a7 8.1 Text Indentation (text-indent)
/// \u2705 IMPLEMENTED by the caller: `indent_line_box` takes the indent off the
/// start-side segment of `line_constraints` before the line is broken, so the
/// segment's `start_x` / `width` already hold it here.
///
/// ### CSS Text \u00a7 4.1 Word Spacing (word-spacing)
/// \u2705 IMPLEMENTED: Additional space between words
///
/// ### CSS Text \u00a7 4.2 Letter Spacing (letter-spacing)
/// \u2705 IMPLEMENTED: Additional space between characters
///
/// ## Segment-Aware Layout:
/// \u2705 Handles CSS Shapes and multi-column layouts
/// - Breaks line into segments (for shape boundaries)
/// - Calculates justification per segment
/// - Applies alignment within each segment's bounds
///
/// ## Known Issues:
/// - \u26a0\ufe0f If segment.width is infinite (from intrinsic sizing), sets `alignment_offset=0`
///   to avoid infinite positioning. This is correct for measurement but documented for clarity.
///
/// # Missing Features:
/// - \u274c \u00a7 6 Trimming Leading (text-box-trim, text-box-edge)
/// - \u274c \u00a7 3.3 Initial Letters (drop caps) // +spec:display-property:265c04 - initial
///   letter exclusion area must continue into subsequent blocks when paragraph is shorter than drop
///   cap
/// - \u274c Full vertical-align support (sub, super, lengths, percentages)
/// - \u274c white-space: break-spaces alignment behavior
// +spec:text-alignment-spacing:c8a926 - order of operations: shaping → letter/word-spacing →
// justification → alignment
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::match_same_arms)]
// enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't
// merge)
#[allow(clippy::too_many_arguments)]
pub fn position_one_line<T: ParsedFontTrait>(
    segment_lines: &[SegmentLine],
    line_top_y: f32,
    line_index: usize,
    base_direction: BidiDirection,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> (Vec<PositionedItem>, f32) {
    position_one_line_between(
        segment_lines,
        None,
        None,
        line_top_y,
        line_index,
        base_direction,
        constraints,
        debug_messages,
        fonts,
    )
}

/// [`position_one_line`] for a line between others: `before` / `after` are
/// the inline boxes of the nearest clusters on the lines before and after
/// it, so a span split by the line wrap gets no edge at the split
/// ([`inline_box_edge_advances_between`]).
///
/// The line is one line box: its height and baseline come from the items of
/// all its segments. Each segment is aligned and justified in its own room
/// (`segment_lines` left to right, each holding the items the breaker put in
/// it).
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::match_same_arms)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose
                                                               // layout/render/parse routine (one
                                                               // branch per case)
pub fn position_one_line_between<T: ParsedFontTrait>(
    segment_lines: &[SegmentLine],
    before: Option<&[InlineBoxEdge]>,
    after: Option<&[InlineBoxEdge]>,
    line_top_y: f32,
    line_index: usize,
    base_direction: BidiDirection,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> (Vec<PositionedItem>, f32) {
    // The line's items left to right, every segment's after the one before.
    let line_items: Vec<ShapedItem> = segment_lines
        .iter()
        .flat_map(|piece| piece.items.iter().cloned())
        .collect();
    if let Some(msgs) = debug_messages {
        let line_text: String = line_items
            .iter()
            .filter_map(|i| i.as_cluster())
            .map(ShapedCluster::text)
            .collect();
        msgs.push(LayoutDebugMessage::info(format!(
            "\n--- Entering position_one_line for line: [{line_text}] ---"
        )));
    }

    // +spec:box-model:847003 - Phantom line boxes: empty lines treated as zero-height
    // +spec:box-model:d781f3 - empty line boxes (no text, no preserved whitespace, no inline
    // elements with non-zero margins/padding/borders, no in-flow content) are treated as
    // zero-height +spec:display-property:90d782 - Phantom line boxes (containing only empty
    // inline boxes, out-of-flow items, or collapsed whitespace) are ignored
    if line_items.is_empty() {
        return (Vec::new(), 0.0);
    }
    let mut positioned = Vec::new();
    let is_vertical = constraints.is_vertical();

    // +spec:line-height:9ca9d9 - line box height = distance from uppermost box top to lowermost box
    // bottom, including strut The line box is calculated once for all items on the line,
    // regardless of segment. Per CSS 2.2 §10.8, top/bottom aligned items are handled in a
    // second pass to minimize line box height; baseline-aligned items determine the initial
    // height.
    // +spec:box-model:e99f7d - strut: each line box starts with zero-width inline box with block
    // container's font/line-height +spec:line-height:29c478 - strut: zero-width inline box with
    // block container's font/line-height inline box with the block container's font and
    // line-height. The strut has A (ascent) and D (descent) from the block container's first
    // available font. Half-leading L/2 is applied: L = line-height - (A + D), strut_above = A +
    // L/2, strut_below = D + L/2. +spec:height-calculation:8e91b2 - specified line-height used
    // in line box height calculation
    // The leading is shared exactly as a glyph's (`split_leading`), so the
    // strut and the text of the same face coincide. The strut is part of the
    // baseline-aligned pass (`calculate_line_metrics`), before top / bottom.
    let strut = split_leading(
        constraints.resolved_line_height(),
        constraints.strut_ascent,
        constraints.strut_descent,
    );
    let (line_ascent, line_descent) =
        calculate_line_metrics(&line_items, constraints.vertical_align, constraints, strut);
    let line_box_height = line_ascent + line_descent;

    // The baseline for the entire line is determined by its tallest item.
    let line_baseline_y = line_top_y + line_ascent;

    // --- Segment-Aware Positioning ---
    // The index in `line_items` of the current segment's first item.
    let mut item_cursor = 0;

    for piece in segment_lines {
        let segment = &piece.segment;
        let is_last_line = piece.is_last_line;
        // +spec:text-alignment-spacing:13b72d - line box start/end determined by inline base
        // direction +spec:text-alignment-spacing:d497af - line box inline base direction
        // affects text-align resolution +spec:text-alignment-spacing:68332e - bidi direction
        // determines start/end to left/right mapping
        let physical_align = physical_text_align(piece.text_align, base_direction);
        let segment_start = item_cursor;
        let segment_items: Vec<ShapedItem> = piece.items.to_vec();
        item_cursor += segment_items.len();

        if segment_items.is_empty() {
            continue;
        }

        // 2. Kashida justification needs to be segment-aware if used.
        let justified_segment_items = if constraints.text_justify == JustifyContent::Kashida
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
        {
            let segment_line_constraints = LineConstraints {
                segments: vec![*segment],
                total_available: segment.width,
                is_min_content: false,
            };
            justify_kashida_and_rebuild(
                segment_items,
                &segment_line_constraints,
                is_vertical,
                debug_messages,
                fonts,
            )
        } else {
            segment_items
        };

        // Recalculate width in case kashida changed the item list. The width
        // the line is positioned at: each cluster's advance AND the letter- /
        // word-spacing `position_one_line` adds after it (an outside marker
        // advances nothing), the width the line breaker and the max-content
        // scan measured (`get_item_measure_with_spacing`). With the bare
        // advances, `text-align: center` put a letter-spaced line half its
        // spacing right of centre (`right`: all of it, past the box's edge).
        //
        // An inline box's start and end margin, border and padding move the
        // pen too ([`inline_box_edge_advances`]), so they belong to the width
        // the line is aligned by: a centred `<span style="margin-right:
        // -0.08em">` (a tracked label giving its last tracking back) sat
        // where the margin was not, and a padded span half its padding off.
        // The boxes on either side of this segment: the line's other
        // segments, then the lines around it.
        let segment_before = line_items[..segment_start]
            .iter()
            .rev()
            .find_map(ShapedItem::as_cluster)
            .map(|c| &*c.style.inline_boxes)
            .or(before);
        let segment_after = line_items[item_cursor..]
            .iter()
            .find_map(ShapedItem::as_cluster)
            .map(|c| &*c.style.inline_boxes)
            .or(after);
        let inline_offsets: Vec<(f32, f32)> = (0..justified_segment_items.len())
            .map(|idx| {
                inline_box_edge_advances_between(
                    &justified_segment_items,
                    idx,
                    segment_before,
                    segment_after,
                )
            })
            .collect();
        let final_segment_width: f32 = justified_segment_items
            .iter()
            .map(|item| match item {
                ShapedItem::Cluster(c) if c.marker_position_outside == Some(true) => {
                    get_item_measure(item, is_vertical)
                }
                _ => get_item_measure_with_spacing(item, is_vertical),
            })
            .sum::<f32>()
            + inline_offsets.iter().map(|(l, r)| l + r).sum::<f32>();

        // +spec:line-breaking:155a96 - pre-wrap hanging spaces: unconditionally hang without forced
        // break, conditionally hang with forced break +spec:white-space-processing:68af09 -
        // Phase II: trailing whitespace hanging/conditional hanging per white-space mode
        // +spec:white-space-processing:75d91e - preserved white space hangs at line end, affecting
        // intrinsic sizing +spec:overflow:a68394 - Hanging trailing whitespace:
        // unconditionally hang (not considered during alignment, may overflow) for lines
        // without forced break; conditionally hang for lines ending with forced break (only
        // hang if would overflow). For normal/nowrap/pre-line: unconditionally hang
        // trailing WS. For pre-wrap: unconditionally hang, unless before forced break (then
        // conditionally hang). For break-spaces: trailing spaces cannot hang.
        // For pre: no hanging (whitespace preserved as-is).
        // +spec:intrinsic-sizing:1db683 - conditionally hanging glyphs excluded from min-content,
        // included in max-content
        let trailing_ws_width = match constraints.white_space_mode {
            WhiteSpaceMode::BreakSpaces | WhiteSpaceMode::Pre => 0.0,
            WhiteSpaceMode::Normal | WhiteSpaceMode::Nowrap | WhiteSpaceMode::PreLine => {
                measure_trailing_whitespace(&justified_segment_items, is_vertical, base_direction)
            }
            // +spec:line-breaking:8aa426 - space before forced break does not hang if it doesn't
            // overflow
            WhiteSpaceMode::PreWrap => {
                // The line ends where its paragraph reads to (rule L1 keeps
                // its break and its trailing white space there).
                let has_forced_break = from_line_end(&justified_segment_items, base_direction)
                    .next()
                    .is_some_and(|item| matches!(item, ShapedItem::Break { .. }));
                let ws_width = measure_trailing_whitespace(
                    &justified_segment_items,
                    is_vertical,
                    base_direction,
                );
                if has_forced_break {
                    // +spec:display-contents:2704a2 - conditionally hanging chars not considered
                    // when measuring line fit Conditionally hang: only hang if
                    // it would overflow
                    let content_width = final_segment_width - ws_width;
                    if content_width + ws_width > segment.width {
                        ws_width
                    } else {
                        0.0
                    }
                } else {
                    ws_width // unconditionally hang
                }
            }
        };
        let effective_segment_width = final_segment_width - trailing_ws_width;

        // +spec:text-alignment-spacing:b9d88e - justify stretches inline boxes via text-justify;
        // non-collapsible WS may skip justification
        // 3. Justification spacing *for this segment only*: the room the
        // segment leaves, measured by the width the line is aligned by (the
        // spacing the pen adds after every cluster and the inline box edges
        // included), so a justified line ends on its segment's edge whatever
        // its letter- and word-spacing. Kashida justification inserts glyphs
        // instead and adds no spacing.
        // +spec:text-alignment-spacing:30d322 - justify lines with justification opportunities when
        // text-align is justify CSS Text 3 §6: text-justify controls HOW to justify, but
        // only applies when text-align is justify/justify-all. Without this check, ALL text
        // gets justified because text-justify defaults to auto (→ InterWord).
        let (extra_word_spacing, extra_char_spacing) = if (constraints.text_align
            == TextAlign::Justify
            || constraints.text_align == TextAlign::JustifyAll)
            && constraints.text_justify != JustifyContent::None
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
            && constraints.text_justify != JustifyContent::Kashida
        {
            calculate_justification_spacing(
                &justified_segment_items,
                effective_segment_width,
                segment.width,
                constraints.text_justify,
            )
        } else {
            (0.0, 0.0)
        };

        // +spec:text-alignment-spacing:287316 - overflow content is start-aligned; alignment offset
        // within line box
        // 4. Calculate alignment offset *within this segment*.
        let remaining_space = segment.width - effective_segment_width;

        // Handle MaxContent/indefinite width: when available_width is MaxContent (for intrinsic
        // sizing), segment.width will be f32::MAX / 2.0. Alignment calculations would
        // produce huge offsets. In this case, treat as left-aligned (offset = 0) since
        // we're measuring natural content width. We check for both infinite AND very large
        // values (> 1e30) to catch the MaxContent case.
        let is_indefinite_width = segment.width.is_infinite() || segment.width > 1e30;
        // +spec:text-alignment-spacing:ab1d4f - unexpandable justify text aligns as center
        let alignment_offset = if is_indefinite_width {
            0.0 // No alignment offset for indefinite width
        } else {
            let align = match physical_align {
                // CSS Text §6.4.3: If text cannot be stretched to full width
                // and text-align-last is justify, align as center.
                TextAlign::Justify | TextAlign::JustifyAll
                    if remaining_space > 0.0
                        && extra_word_spacing == 0.0
                        && extra_char_spacing == 0.0 =>
                {
                    TextAlign::Center
                }
                other => other,
            };
            // An overflowing line is start-aligned (CSS Text 3 7.1).
            line_alignment_offset(align, remaining_space, base_direction)
        };

        let mut main_axis_pen = segment.start_x + alignment_offset;
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[Pos1Line] Segment width: {}, Item width: {}, Remaining space: {}, Initial pen: \
                 {}",
                segment.width, final_segment_width, remaining_space, main_axis_pen
            )));
        }

        // `text-indent` is already in the segment: `indent_line_box` took it off
        // the line box's start edge before the line was broken.

        // Calculate total marker width for proper outside marker positioning
        // We need to position all marker clusters together in the padding gutter
        let total_marker_width: f32 = justified_segment_items
            .iter()
            .filter_map(|item| {
                if let ShapedItem::Cluster(c) = item {
                    if c.marker_position_outside == Some(true) {
                        return Some(get_item_measure(item, is_vertical));
                    }
                }
                None
            })
            .sum();

        // Track marker pen separately - starts at negative position for outside markers
        let marker_spacing = 4.0; // Small gap between marker and content
        let mut marker_pen = if total_marker_width > 0.0 {
            -(total_marker_width + marker_spacing)
        } else {
            0.0
        };

        // 5. Position the items belonging to this segment.
        //
        // +spec:inline-formatting-context:267438 - Content positioning: position aligned subtree
        // and baseline-shift values within line box
        //
        // Vertical alignment positioning (CSS vertical-align)
        //
        // +spec:font-metrics:cae541 - dominant baseline used for inline alignment
        // Per CSS Inline Layout Level 3 § 4 (Baseline Alignment), each inline
        // element can specify its own `vertical-align`. For Object items
        // (inline-blocks, images), we use their per-item alignment stored in
        // `InlineContent::Shape.alignment` or `InlineContent::Image.alignment`.
        // For text clusters or items without a per-item override, we fall back
        // to the global `constraints.vertical_align` from the containing block.
        //
        // +spec:font-metrics:f29b61 - baseline alignment matches corresponding baseline types (only
        // alphabetic implemented) Reference: https://www.w3.org/TR/css-inline-3/#baseline-alignment
        // +spec:block-formatting-context:26b535 - In vertical typographic mode, central baseline is
        // dominant when text-orientation is mixed/upright; otherwise alphabetic
        // +spec:inline-formatting-context:eb735b - alignment-baseline: inline-level boxes aligned
        // to parent's baseline via vertical-align +spec:inline-formatting-context:da3f34 -
        // baseline alignment of in-flow inline-level boxes in block axis per
        // dominant-baseline/vertical-align +spec:line-height:e2253a - vertical-align
        // positioning within line boxes

        // The inline margin/border/padding offsets at span boundaries
        // (`inline_offsets` above, the rule the intrinsic scan shares) move
        // the pen as the items are placed.
        let segment_item_count = justified_segment_items.len();
        for (inline_offset_idx, item) in justified_segment_items.into_iter().enumerate() {
            let (item_ascent, item_descent) = get_item_vertical_metrics(&item, constraints);
            // Use per-item alignment if available, otherwise fall back to global
            let effective_align =
                get_item_vertical_align(&item).unwrap_or(constraints.vertical_align);
            // +spec:display-property:328cfc - baseline-shift / aligned subtree vertical alignment
            // (sub, super, top, bottom, center) §10.8.1 vertical-align positioning
            // +spec:line-height:0fcfab - vertical-align property values (baseline, top, middle,
            // bottom, sub, super, text-top, text-bottom, percentage, length)
            let item_baseline_pos = match effective_align {
                // +spec:display-property:8e018d - aligned subtree edges used for top/bottom line
                // box alignment +spec:inline-formatting-context:495672 -
                // line-relative vertical-align (top/center/bottom) and aligned subtree positioning
                // top: align top of aligned subtree with top of line box
                VerticalAlign::Top => line_top_y + item_ascent,
                // bottom: align bottom of aligned subtree with bottom of line box
                VerticalAlign::Bottom => line_top_y + line_box_height - item_descent,
                // +spec:font-metrics:70000d - middle: the box's midpoint at the parent's
                // baseline raised by half its x-height; text-top / text-bottom: against the
                // parent's content area (s10.6.1); <length> / <percentage>: raise or lower;
                // +spec:display-property:3b0e76 - sub / super: the parent font size / 5 + 1
                // down, / 3 + 1 up;
                // +spec:display-property:8bf37e +spec:font-metrics:96bbd3 - baseline: the
                // box's alphabetic baseline on the parent's. ONE rule with the line box
                // (`baseline_shift`, also read by `calculate_line_metrics`).
                VerticalAlign::Sub
                | VerticalAlign::Super
                | VerticalAlign::Middle
                | VerticalAlign::TextTop
                | VerticalAlign::TextBottom
                | VerticalAlign::Offset(_)
                | VerticalAlign::Baseline => {
                    line_baseline_y
                        + baseline_shift(effective_align, item_ascent, item_descent, constraints)
                            .unwrap_or(0.0)
                }
            };

            // Calculate item measure (needed for both positioning and pen advance)
            let item_measure = get_item_measure(&item, is_vertical);

            // Advance pen by inline left_inset at span entry (before positioning glyphs)
            let (left_inset, right_inset) = if inline_offset_idx < inline_offsets.len() {
                inline_offsets[inline_offset_idx]
            } else {
                (0.0, 0.0)
            };
            main_axis_pen += left_inset;

            let position = if is_vertical {
                Point {
                    x: item_baseline_pos - item_ascent,
                    y: main_axis_pen,
                }
            } else {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[Pos1Line] is_vertical=false, main_axis_pen={main_axis_pen}, \
                         item_baseline_pos={item_baseline_pos}, item_ascent={item_ascent}"
                    )));
                }

                // Check if this is an outside marker - if so, position it in the padding gutter
                let x_position = if let ShapedItem::Cluster(cluster) = &item {
                    if cluster.marker_position_outside == Some(true) {
                        // Use marker_pen for sequential marker positioning
                        let marker_width = item_measure;
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[Pos1Line] Outside marker detected! width={marker_width}, \
                                 positioning at marker_pen={marker_pen}"
                            )));
                        }
                        let pos = marker_pen;
                        marker_pen += marker_width; // Advance marker pen for next marker cluster
                        pos
                    } else {
                        main_axis_pen
                    }
                } else {
                    main_axis_pen
                };

                Point {
                    y: item_baseline_pos - item_ascent,
                    x: x_position,
                }
            };

            // item_measure is calculated above for marker positioning
            let item_text = item.as_cluster().map_or("[OBJ]", |c| c.text());
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[Pos1Line] Positioning item '{item_text}' at pen_x={main_axis_pen}"
                )));
            }
            positioned.push(PositionedItem {
                item: item.clone(),
                position,
                line_index,
            });

            // Outside markers don't advance the pen - they're positioned in the padding gutter
            let is_outside_marker = if let ShapedItem::Cluster(c) = &item {
                c.marker_position_outside == Some(true)
            } else {
                false
            };

            if !is_outside_marker {
                main_axis_pen += item_measure;
                // Advance pen by inline right_inset at span exit (after glyph advance)
                main_axis_pen += right_inset;
            }

            // +spec:text-alignment-spacing:e09bd1 - justification space added on top of
            // letter-spacing/word-spacing +spec:text-alignment-spacing:456643 - cursive
            // scripts don't admit inter-character gaps
            // The space goes between letters: none after the segment's last
            // one, which ends on the segment's edge.
            if extra_char_spacing > 0.0
                && inline_offset_idx + 1 < segment_item_count
                && takes_inter_character_space(&item)
            {
                main_axis_pen += extra_char_spacing;
            }
            // +spec:display-property:3a833c - consecutive atomic inlines treated as single unit for
            // letter-spacing +spec:display-property:49f04f - letter-spacing applied per
            // innermost inline element +spec:text-alignment-spacing:22bea4 -
            // letter-spacing applied after bidi reordering, additive with kerning and word-spacing;
            // justification may further adjust
            if let ShapedItem::Cluster(c) = &item {
                if !is_outside_marker {
                    // +spec:display-property:756454 - letter-spacing applied between typographic
                    // character units +spec:overflow:e63bc0 - letter-spacing
                    // ignores zero-width formatting chars (Cf); handled by shaper merging them into
                    // clusters +spec:text-alignment-spacing:80f9ec -
                    // letter-spacing applied per-cluster using innermost element's style
                    // (UA-allowed attachment) +spec:text-alignment-spacing:
                    // bdd704 - letter-spacing applied after each cluster, not at line start
                    // +spec:text-alignment-spacing:d3ef6e - single-char element: only trailing
                    // space, no inter-char effect +spec:text-alignment-spacing:
                    // d668fc - letter-spacing only affects characters within the element
                    // (per-cluster style) +spec:text-alignment-spacing:8dbb78 -
                    // zero letter-spacing behaves as normal (Px(0) adds no spacing)
                    // +spec:text-alignment-spacing:456643 - skip letter-spacing for cursive scripts
                    if !is_cursive_script_cluster(c) {
                        let letter_spacing_px =
                            c.style.letter_spacing.resolve_px(c.style.font_size_px);
                        main_axis_pen += letter_spacing_px;
                    }
                    // +spec:width-calculation:9447d1 - word-spacing only applied to word
                    // separators; zero-width chars like U+200B are excluded
                    if is_word_separator(&item) {
                        let word_spacing_px = c.style.word_spacing.resolve_px(c.style.font_size_px);
                        main_axis_pen += word_spacing_px;
                        main_axis_pen += extra_word_spacing;
                    }
                }
            }
        }
    }

    (positioned, line_box_height)
}

/// `text-align: start` / `end` as the physical side of a line whose inline
/// base direction is `base_direction` (start = left in a left-to-right line,
/// right in a right-to-left one); physical alignments are returned as they
/// are. The one mapping of both line positioners.
pub(crate) const fn physical_text_align(
    text_align: TextAlign,
    base_direction: BidiDirection,
) -> TextAlign {
    match (text_align, base_direction) {
        (TextAlign::Start, BidiDirection::Ltr) | (TextAlign::End, BidiDirection::Rtl) => {
            TextAlign::Left
        }
        (TextAlign::Start, BidiDirection::Rtl) | (TextAlign::End, BidiDirection::Ltr) => {
            TextAlign::Right
        }
        (other, _) => other,
    }
}

/// Where a line's content starts in its line box: the offset from the box's
/// left edge for the PHYSICAL alignment `physical_align` (start / end already
/// resolved against `base_direction`), with `remaining_space` = the box's
/// width - the content's.
///
/// CSS Text 3 section 7.1: "If ... the inline contents of a line box are too
/// long to fit within it, then the contents are start-aligned: any content
/// that doesn't fit overflows the line box's end edge" - whatever
/// `text-align` says, as in Chrome ("wide lines spill out of the block based
/// off direction"). In a left-to-right line that is offset 0 (overflow on the
/// right); in a right-to-left one the content's right edge stays on the box's
/// (offset = the negative `remaining_space`, overflow on the left). Applying a
/// right / center alignment to the negative space cut off the line's START
/// (`AzCalculator`'s long results).
///
/// The ONE alignment rule of both line positioners: `position_one_line` and
/// the Knuth-Plass path (`knuth_plass::position_lines_from_breaks`).
pub(crate) fn line_alignment_offset(
    physical_align: TextAlign,
    remaining_space: f32,
    base_direction: BidiDirection,
) -> f32 {
    let align = if remaining_space < 0.0 {
        match base_direction {
            BidiDirection::Ltr => TextAlign::Left,
            BidiDirection::Rtl => TextAlign::Right,
        }
    } else {
        physical_align
    };
    match align {
        TextAlign::Center => remaining_space / 2.0,
        TextAlign::Right => remaining_space,
        _ => 0.0, // Left, and Justify (a justified line fills its box)
    }
}

/// The `text-indent` of one line box (CSS Text 3 section 8.1): the first
/// formatted line of the block container is indented - with `each-line`
/// every line after a forced line break too, never a line after a soft wrap -
/// and `hanging` inverts which lines are. 0 for the others.
///
/// `is_first_formatted_line` is the paragraph's first line, not a fragment's:
/// a continuation fragment of a flow starts mid-paragraph. The ONE choice of
/// the greedy breaker, the Knuth-Plass path and the intrinsic-size scan.
pub(crate) const fn text_indent_of_line(
    constraints: &UnifiedConstraints,
    is_first_formatted_line: bool,
    is_after_forced_break: bool,
) -> f32 {
    let picked =
        is_first_formatted_line || (constraints.text_indent_each_line && is_after_forced_break);
    if picked == constraints.text_indent_hanging {
        0.0
    } else {
        constraints.text_indent
    }
}

/// Takes a line's `text-indent` off the start edge of its line box. CSS Text 3
/// section 8.1: the indent "is treated as a margin applied to the start edge of
/// the line box" - the line box is that much narrower (a negative indent:
/// wider), so the breaker fills, `justify` spreads over and the alignment
/// places the line in what is left. The start-side segment is the leftmost one
/// of a left-to-right line (and of a vertical one: its top) and the rightmost
/// one of a right-to-left line, whose start edge is its right edge.
///
/// Shifting the finished line by the indent instead (as both positioners
/// did) broke the first line against the full width: it ended `indent` past
/// the paragraph (pdfocr, 2026-10-02).
pub(crate) fn indent_line_box(
    line_constraints: &mut LineConstraints,
    indent: f32,
    base_direction: BidiDirection,
) {
    if indent == 0.0 || !indent.is_finite() {
        return;
    }
    let start_segment = match base_direction {
        BidiDirection::Ltr => line_constraints.segments.first_mut(),
        BidiDirection::Rtl => line_constraints.segments.last_mut(),
    };
    let Some(segment) = start_segment else {
        return;
    };
    if base_direction == BidiDirection::Ltr {
        segment.start_x += indent;
    }
    segment.width -= indent;
    line_constraints.total_available -= indent;
}

/// The room a line has at block offset `line_y`, `line_height` tall: the
/// spans of the content area left to it, left to right.
///
/// The room starts as the available width. Inside the content area a
/// `shape-inside` keeps only what every one of its shapes covers over the
/// whole line box (CSS Shapes 2 takes the area outside the shape away); a
/// line below that area keeps the whole width, as overflow continues outside
/// the element. Each exclusion then takes whatever of the line box it
/// reaches, at any height of it. One span is one segment of the line.
pub(super) fn get_line_constraints(
    line_y: f32,
    line_height: f32,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LineConstraints {
    // The width is the available width when it is definite, and a large one
    // while measuring the intrinsic sizes: the min-content breaker breaks at
    // every word on its own, and `white-space: nowrap` overflows whatever the
    // width (both in `break_one_line`).
    let width = match constraints.available_width {
        AvailableSpace::Definite(w) => w,
        AvailableSpace::MaxContent | AvailableSpace::MinContent => f32::MAX / 2.0,
    };
    let is_min_content = matches!(constraints.available_width, AvailableSpace::MinContent);
    if constraints.shape_boundaries.is_empty() && constraints.shape_exclusions.is_empty() {
        // The whole width, even an empty one: a zero-width box still lays
        // its content out, on lines that overflow it.
        return LineConstraints {
            segments: vec![LineSegment {
                start_x: 0.0,
                width,
                priority: 0,
            }],
            total_available: width,
            is_min_content,
        };
    }
    let mut spans = vec![(0.0, width)];

    let area_bottom = constraints.shape_area_height.unwrap_or(f32::INFINITY);
    let band_bottom_in_area = (line_y + line_height).min(area_bottom);
    if band_bottom_in_area > line_y {
        for boundary in &constraints.shape_boundaries {
            let inside = shape_spans_in_band(
                boundary,
                line_y,
                band_bottom_in_area - line_y,
                BandCover::Whole,
            );
            spans = intersect_spans(&spans, &inside);
        }
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[get_line_constraints] y={line_y} h={line_height}: room inside the shapes {spans:?}"
        )));
    }

    for exclusion in &constraints.shape_exclusions {
        let taken = get_shape_horizontal_spans(exclusion, line_y, line_height);
        if !taken.is_empty() {
            spans = subtract_spans(&spans, &taken);
        }
    }

    let segments: Vec<LineSegment> = spans
        .into_iter()
        .filter(|(start, end)| end > start)
        .map(|(start, end)| LineSegment {
            start_x: start,
            width: end - start,
            priority: 0,
        })
        .collect();
    let total_available = segments.iter().map(|s| s.width).sum();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[get_line_constraints] segments {segments:?}, {total_available} wide in all"
        )));
    }

    LineConstraints {
        segments,
        total_available,
        is_min_content,
    }
}

/// Whether the room of a line is cut by a shape or an exclusion: anything but
/// the one span of the full `width` from the line's start.
fn room_is_cut(line_constraints: &LineConstraints, width: f32) -> bool {
    match line_constraints.segments.as_slice() {
        [only] => only.start_x.abs() > 0.01 || only.width < width - 0.01,
        _ => true,
    }
}

/// Where a line that found no room at `line_top` tries next: the next height
/// below it at which a shape or an exclusion starts, ends or turns (where the
/// room can change), or one line further down, whichever comes first.
///
/// CSS 2.2 9.5 moves a line box that is too short for its content down until
/// some content fits; the room only changes at those heights, so no height in
/// between needs a try.
fn next_band_top(line_top: f32, constraints: &UnifiedConstraints) -> f32 {
    let mut next = line_top + constraints.resolved_line_height().max(1.0);
    let mut consider = |y: f32| {
        if y > line_top + 0.01 && y < next {
            next = y;
        }
    };
    for shape in constraints
        .shape_boundaries
        .iter()
        .chain(&constraints.shape_exclusions)
    {
        shape.for_each_turning_height(&mut consider);
    }
    if let Some(area) = constraints.shape_area_height {
        consider(area);
    }
    next
}

/// Whether a line at `line_top` lies below every height where a shape or an
/// exclusion could still change its room: moving it further down gains
/// nothing.
fn below_every_shape(line_top: f32, constraints: &UnifiedConstraints) -> bool {
    let mut lowest = constraints.shape_area_height.unwrap_or(f32::MIN);
    for shape in constraints
        .shape_boundaries
        .iter()
        .chain(&constraints.shape_exclusions)
    {
        shape.for_each_turning_height(&mut |y| lowest = lowest.max(y));
    }
    line_top >= lowest
}

/// The hyphenation dictionary of `language`.
///
/// Every paragraph in a language breaks its words with the same patterns,
/// and the line breakers and the intrinsic sizes of each ask for them: they
/// are read from the embedded dictionary once and shared after that.
#[cfg(feature = "text_layout_hyphenation")]
pub(super) fn get_hyphenator(language: HyphenationLanguage) -> Result<Arc<Standard>, LayoutError> {
    static DICTIONARIES: std::sync::OnceLock<Mutex<HashMap<HyphenationLanguage, Arc<Standard>>>> =
        std::sync::OnceLock::new();
    let mut dictionaries = DICTIONARIES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dictionary = if let Some(dictionary) = dictionaries.get(&language) {
        Arc::clone(dictionary)
    } else {
        let dictionary = Arc::new(
            Standard::from_embedded(language)
                .map_err(|e| LayoutError::HyphenationError(e.to_string()))?,
        );
        dictionaries.insert(language, Arc::clone(&dictionary));
        dictionary
    };
    drop(dictionaries);
    Ok(dictionary)
}

/// Stub when hyphenation is disabled - always returns an error
#[cfg(not(feature = "text_layout_hyphenation"))]
pub(super) fn get_hyphenator(_language: Language) -> Result<Arc<Standard>, LayoutError> {
    Err(LayoutError::HyphenationError(
        "Hyphenation feature not enabled".to_string(),
    ))
}

/// The dictionary `hyphens: auto` breaks the words of a paragraph with
/// `constraints` by: that of its language, and none for `manual` and
/// `none` or a language without patterns.
///
/// The line breakers and the intrinsic scan all take theirs from here, so
/// a paragraph's lines and its min-content break its words alike.
pub(super) fn hyphenator_for(constraints: &UnifiedConstraints) -> Option<Arc<Standard>> {
    if constraints.hyphenation == Hyphens::Auto {
        constraints
            .hyphenation_language
            .and_then(|lang| get_hyphenator(lang).ok())
    } else {
        None
    }
}
