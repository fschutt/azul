//! An implementation of the Knuth-Plass line-breaking algorithm
//! for simple rectangular layouts.

#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Standard};
#[cfg(not(feature = "text_layout_hyphenation"))]
use crate::text3::cache::Standard;

use crate::text3::cache::{
    get_base_direction_from_logical, get_item_measure, is_word_separator, is_zero_width_space,
    AvailableSpace, BidiDirection, JustifyContent, LayoutError, LoadedFonts,
    LogicalItem, OverflowInfo, ParsedFontTrait, Point, PositionedItem,
    ShapedItem, TextAlign, UnifiedConstraints, UnifiedLayout,
};

const INFINITY_BADNESS: f32 = 10000.0;
const SPACE_STRETCH_RATIO: f32 = 0.5;
const SPACE_SHRINK_RATIO: f32 = 0.33;
const HYPHENATION_PENALTY: f32 = 50.0;
const BADNESS_MULTIPLIER: f32 = 100.0;

/// Represents the elements of a paragraph for the line-breaking algorithm.
#[derive(Debug, Clone)]
enum LayoutNode {
    /// A non-stretchable, non-shrinkable item (a glyph cluster or an object).
    Box(ShapedItem, f32), // Item and its width
    /// A flexible space.
    Glue {
        item: ShapedItem,
        /// Natural width of the space.
        width: f32,
        /// Maximum amount the space can grow beyond its natural width.
        stretch: f32,
        /// Maximum amount the space can shrink below its natural width.
        shrink: f32,
    },
    /// A point where a line break is allowed, with an associated cost.
    Penalty {
        /// Optional item associated with the penalty (e.g., a hyphen glyph).
        item: Option<ShapedItem>,
        width: f32,
        penalty: f32,
    },
}

/// Stores the result of the dynamic programming algorithm for a given point.
#[derive(Debug, Clone, Copy)]
struct Breakpoint {
    /// The total demerit score to reach this point.
    demerit: f32,
    /// The index of the previous breakpoint in the optimal path.
    previous: usize,
    /// The line number this breakpoint ends.
    line: usize,
}

/// Main entry point for the Knuth-Plass layout algorithm.
///
/// This implements optimal line-breaking as described in "Breaking Paragraphs into Lines"
/// (Knuth & Plass, 1981). Unlike greedy algorithms, it considers the entire paragraph
/// to find globally optimal break points.
///
/// # Use Cases
///
/// - `text-wrap: balance` - CSS property for balanced line lengths
/// - High-quality typesetting where line consistency matters
/// - Multi-line headings that should appear visually balanced
///
/// # Limitations
///
/// - Only supports horizontal text (vertical writing modes use greedy algorithm)
/// - Higher computational cost than greedy breaking
/// - May produce different results than browsers for edge cases
/// - Does not yet handle overflow-wrap: anywhere/break-word (handled in greedy path)
// overflow-wrap emergency breaks; the greedy break_one_line path handles this
pub(crate) fn kp_layout<T: ParsedFontTrait>(
    items: &[ShapedItem],
    logical_items: &[LogicalItem],
    constraints: &UnifiedConstraints,
    hyphenator: Option<&Standard>,
    fonts: &LoadedFonts<T>,
) -> UnifiedLayout {
    if items.is_empty() {
        return UnifiedLayout {
            items: Vec::new(),
            overflow: OverflowInfo::default(),
        };
    }

    // Convert ShapedItems into a sequence of Boxes, Glue, and Penalties
    let nodes = convert_items_to_nodes(items, hyphenator, fonts);

    // Dynamic Programming to find optimal breakpoints
    let breaks = find_optimal_breakpoints(&nodes, constraints);

    // Use breakpoints to build and position the final lines
    let final_layout: UnifiedLayout =
        position_lines_from_breaks(&nodes, &breaks, logical_items, constraints);

    final_layout
}

/// Converts a slice of `ShapedItems` into the Box/Glue/Penalty model.
// +spec:line-breaking:16e64c - soft wrap opportunity controls (word-break, overflow-wrap, line-break) threaded via UnifiedConstraints
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn convert_items_to_nodes<T: ParsedFontTrait>(
    items: &[ShapedItem],
    hyphenator: Option<&Standard>,
    fonts: &LoadedFonts<T>,
) -> Vec<LayoutNode> {
    let mut nodes = Vec::new();
    let is_vertical = false; // Knuth-Plass is horizontal-only for now
    let mut item_iter = items.iter().peekable();

    while let Some(item) = item_iter.next() {
        // +spec:line-breaking:f12241 - shaping across intra-word breaks: shaped clusters preserve joining forms
        // NOTE: word-break property is not yet threaded through to kp_layout.
        // Currently uses normal break behavior (spaces are break opportunities).
        // To fully support break-all/keep-all, UnifiedConstraints.word_break
        // would need to be passed here and used to insert additional Penalty
        // nodes between CJK clusters (normal) or between all clusters (break-all),
        // or suppress CJK inter-character penalties (keep-all).
        match item {
            item if is_zero_width_space(item) => {
                nodes.push(LayoutNode::Penalty {
                    item: None,
                    width: 0.0,
                    penalty: 0.0,
                });
            }
            item if is_word_separator(item) => {
                let width = get_item_measure(item, is_vertical);
                nodes.push(LayoutNode::Glue {
                    item: item.clone(),
                    width,
                    stretch: width * SPACE_STRETCH_RATIO,
                    shrink: width * SPACE_SHRINK_RATIO,
                });
                nodes.push(LayoutNode::Penalty {
                    item: None,
                    width: 0.0,
                    penalty: 0.0,
                });
            }
            ShapedItem::Cluster(cluster)
                if cluster.text.ends_with('\u{002D}')
                    || cluster.text.ends_with('\u{2010}') =>
            {
                let width = get_item_measure(item, is_vertical);
                nodes.push(LayoutNode::Box(item.clone(), width));
                // +spec:line-breaking:2d3674 - U+002D/U+2010 are soft wrap opportunities, not hyphenation opportunities (no extra glyph inserted)
                // Zero-width penalty: allows a line break after the visible
                // hyphen character without inserting an additional hyphen glyph.
                nodes.push(LayoutNode::Penalty {
                    item: None,
                    width: 0.0,
                    penalty: 0.0,
                });
            }
            ShapedItem::Cluster(cluster) => {
                // 1. Collect all adjacent clusters to form a full "word".
                let mut current_word_clusters = vec![cluster.clone()];
                while let Some(peeked_item) = item_iter.peek() {
                    if let ShapedItem::Cluster(next_cluster) = peeked_item {
                        // Stop collecting *before* any soft-wrap boundary so the outer
                        // loop can emit the correct node for it. Word separators are
                        // shaped as ordinary Clusters (text " "), so without these guards
                        // the greedy collection would absorb every space into one giant
                        // "word" and the paragraph could never break — it would collapse
                        // onto a single line. Boundaries handled by the outer loop:
                        //   * word separator     -> Glue + Penalty (soft wrap)
                        //   * zero-width space    -> Penalty (soft wrap)
                        //   * cluster ending '-'  -> Box + zero-width Penalty
                        //     (+spec:line-breaking:2d3674 — U+002D/U+2010 are UAX#14
                        //     class BA break opportunities AFTER the hyphen; a hyphen
                        //     occurs mid-word, e.g. "well-being").
                        if is_word_separator(peeked_item)
                            || is_zero_width_space(peeked_item)
                            || next_cluster.text.ends_with('\u{002D}')
                            || next_cluster.text.ends_with('\u{2010}')
                        {
                            break;
                        }
                        current_word_clusters.push(next_cluster.clone());
                        item_iter.next(); // Consume the peeked item
                    } else {
                        // Stop if we hit a non-cluster item (object, tab, break, etc.)
                        break;
                    }
                }

                // +spec:line-breaking:28a40b - Hyphenation is a rendering-only effect (no change to underlying content)
                // +spec:line-breaking:f23fe8 - UA may use language-tailored heuristics (delegated to hyphenation crate)
                // 2. Try to find all hyphenation opportunities for this word.
                // +spec:display-property:508895 - cross-direction hyphenation suppression (LTR in RTL / RTL in LTR) not yet implemented
                let hyphenation_breaks = hyphenator.and_then(|h| {
                    crate::text3::cache::find_all_hyphenation_breaks(
                        &current_word_clusters,
                        h,
                        is_vertical,
                        fonts,
                    )
                });

                if hyphenation_breaks.is_none() {
                    // No hyphenation possible, add the whole word as boxes.
                    for c in current_word_clusters {
                        nodes.push(LayoutNode::Box(ShapedItem::Cluster(c.clone()), c.advance));
                    }
                } else {
                    // 3. Convert word + hyphenation breaks into a sequence of Boxes and Penalties.
                    let breaks = hyphenation_breaks.unwrap();
                    let mut current_item_cursor = 0;

                    for b in &breaks {
                        // Add the items that form the next syllable (the part between the last
                        // break and this one)
                        let num_items_in_syllable = b.line_part.len() - current_item_cursor;
                        for item in b.line_part.iter().skip(current_item_cursor) {
                            nodes.push(LayoutNode::Box(
                                item.clone(),
                                get_item_measure(item, is_vertical),
                            ));
                        }
                        current_item_cursor += num_items_in_syllable;

                        let hyphen_measure = get_item_measure(&b.hyphen_item, is_vertical);
                        nodes.push(LayoutNode::Penalty {
                            item: Some(b.hyphen_item.clone()),
                            width: hyphen_measure,
                            penalty: HYPHENATION_PENALTY, // Standard penalty for hyphenation
                        });
                    }

                    // Add the final remainder of the word.
                    if let Some(last_break) = breaks.last() {
                        for remainder_item in &last_break.remainder_part {
                            nodes.push(LayoutNode::Box(
                                remainder_item.clone(),
                                get_item_measure(remainder_item, is_vertical),
                            ));
                        }
                    } else {
                        // This case happens if find_all_hyphenation_breaks returned an empty vec.
                        // Fallback to just adding the original word.
                        for c in current_word_clusters {
                            nodes.push(LayoutNode::Box(ShapedItem::Cluster(c.clone()), c.advance));
                        }
                    }
                }
            }
            // Per CSS Text 3 §5.1: "there is a soft wrap opportunity before and
            // after each replaced element or other atomic inline"
            ShapedItem::Object { .. } | ShapedItem::CombinedBlock { .. } => {
                // Soft wrap opportunity before the atomic inline
                nodes.push(LayoutNode::Penalty {
                    item: None,
                    width: 0.0,
                    penalty: 0.0,
                });
                nodes.push(LayoutNode::Box(
                    item.clone(),
                    get_item_measure(item, is_vertical),
                ));
                // Soft wrap opportunity after the atomic inline
                nodes.push(LayoutNode::Penalty {
                    item: None,
                    width: 0.0,
                    penalty: 0.0,
                });
            }
            ShapedItem::Tab { bounds, .. } => {
                nodes.push(LayoutNode::Glue {
                    item: item.clone(),
                    width: bounds.width,
                    stretch: bounds.width * SPACE_STRETCH_RATIO, // Treat like a space for flexibility
                    shrink: bounds.width * SPACE_SHRINK_RATIO,
                });
            }
            ShapedItem::Break { .. } => {
                nodes.push(LayoutNode::Penalty {
                    item: None,
                    width: 0.0,
                    penalty: -INFINITY_BADNESS,
                });
            }
        }
    }

    // Anchor the end of the paragraph. Standard Knuth-Plass appends a finishing
    // forced break so the final line is broken at the paragraph end. Without it,
    // a paragraph that ends in an ordinary word (its final nodes are Box, not a
    // Penalty) never sets breakpoints[n] in the DP, so the backtrack collapses
    // the entire paragraph onto a single line. A forced break at the end makes
    // n a legal breakpoint regardless of the last node's type; the forced-break
    // scoring in find_optimal_breakpoints already exempts a short last line from
    // any badness, so no finishing glue is required. The penalty carries no item,
    // so it contributes nothing to the positioned output.
    if !nodes.is_empty()
        && !matches!(
            nodes.last(),
            Some(LayoutNode::Penalty { penalty, .. }) if *penalty <= -INFINITY_BADNESS
        )
    {
        nodes.push(LayoutNode::Penalty {
            item: None,
            width: 0.0,
            penalty: -INFINITY_BADNESS,
        });
    }

    nodes
}

/// Uses dynamic programming to find the optimal set of line breaks.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[allow(clippy::cognitive_complexity)] // cohesive Knuth-Plass DP: one branch per break class
fn find_optimal_breakpoints(nodes: &[LayoutNode], constraints: &UnifiedConstraints) -> Vec<usize> {
    // For MinContent (intrinsic min-content sizing), CSS wants the width of the
    // widest unbreakable unit (word). Break at EVERY legal opportunity so each
    // word lands on its own line; the widest resulting line then equals the
    // widest word. The optimizing DP below cannot express this: with an
    // effectively infinite width it puts the whole paragraph on one line, making
    // min-content == max-content. Every Penalty node is a legal break point.
    if matches!(constraints.available_width, AvailableSpace::MinContent) {
        let mut breaks = Vec::new();
        for (i, node) in nodes.iter().enumerate() {
            if matches!(node, LayoutNode::Penalty { .. }) {
                breaks.push(i + 1);
            }
        }
        // Ensure the final segment (which may end in Box nodes) forms a line.
        if breaks.last() != Some(&nodes.len()) {
            breaks.push(nodes.len());
        }
        return breaks;
    }

    // For Knuth-Plass, we need a definite line width.
    //
    // For MaxContent, use a very large value (no line breaks unless forced).
    // The actual min-content width is determined by the widest resulting line.

    let line_width = match constraints.available_width {
        AvailableSpace::Definite(w) => w,
        AvailableSpace::MaxContent => f32::MAX / 2.0,
        // MinContent is handled by the early return above; keep a large width as
        // a defensive fallback so this arm never breaks after every character.
        AvailableSpace::MinContent => f32::MAX / 2.0,
    };

    // (and lines after forced breaks when each-line is set). The hanging keyword would
    // invert this, indenting all lines EXCEPT the first.
    let text_indent = constraints.text_indent;
    let first_line_width = if constraints.text_indent_hanging {
        line_width
    } else {
        line_width - text_indent
    };
    let non_first_line_width = if constraints.text_indent_hanging {
        line_width - text_indent
    } else {
        line_width
    };

    // Prefix sums for O(1) range queries (eliminates O(n³) inner loop).
    let n = nodes.len();
    let mut prefix_width = vec![0.0f32; n + 1];
    let mut prefix_stretch = vec![0.0f32; n + 1];
    let mut prefix_shrink = vec![0.0f32; n + 1];
    for (k, node) in nodes.iter().enumerate() {
        let (w, st, sh) = match node {
            LayoutNode::Box(_, w) => (*w, 0.0, 0.0),
            LayoutNode::Glue {
                width,
                stretch,
                shrink,
                ..
            } => (*width, *stretch, *shrink),
            LayoutNode::Penalty { width, .. } => (*width, 0.0, 0.0),
        };
        prefix_width[k + 1] = prefix_width[k] + w;
        prefix_stretch[k + 1] = prefix_stretch[k] + st;
        prefix_shrink[k + 1] = prefix_shrink[k] + sh;
    }

    let mut breakpoints = vec![
        Breakpoint {
            demerit: INFINITY_BADNESS,
            previous: 0,
            line: 0
        };
        n + 1
    ];
    breakpoints[0] = Breakpoint {
        demerit: 0.0,
        previous: 0,
        line: 0,
    };

    for i in 0..n {
        // Optimization:
        //
        // A legal line break can only occur at a Penalty node. If the current node
        // is a Box or Glue, we can skip it as a potential breakpoint.

        if !matches!(nodes.get(i), Some(LayoutNode::Penalty { .. })) {
            continue;
        }

        for j in (0..=i).rev() {
            // Calculate the properties of a potential line from node `j` to `i`.
            // O(1) range sum via prefix sums: sum of nodes[j..=i]
            let current_width = prefix_width[i + 1] - prefix_width[j];
            let stretch = prefix_stretch[i + 1] - prefix_stretch[j];
            let shrink = prefix_shrink[i + 1] - prefix_shrink[j];

            let effective_line_width = if breakpoints[j].line == 0 {
                first_line_width
            } else if constraints.text_indent_hanging {
                non_first_line_width
            } else {
                line_width
            };

            // Calculate adjustment ratio. If the line is wider than the available width
            // but has no glue to shrink, it is an invalid candidate.
            let ratio = if current_width < effective_line_width {
                if stretch > 0.0 {
                    (effective_line_width - current_width) / stretch
                } else {
                    INFINITY_BADNESS // Cannot stretch
                }
            } else if current_width > effective_line_width {
                if shrink > 0.0 {
                    (effective_line_width - current_width) / shrink
                } else {
                    // Overfull with nothing to shrink: this line physically cannot
                    // fit, so it is INFEASIBLE — not merely "loose". Marking it with a
                    // large negative ratio makes the `ratio < -1.0` guard below reject
                    // it, exactly like an over-shrunk line. Using +INFINITY_BADNESS
                    // here (a positive ratio) was a bug: it let an overflowing line
                    // survive the feasibility guard and then be rewarded by the forced
                    // end-of-paragraph break, so the DP preferred one overflowing line
                    // over a legal break (e.g. after a mid-word hyphen). Overlong
                    // unbreakable content is intentionally left to the greedy path.
                    -INFINITY_BADNESS
                }
            } else {
                0.0 // Perfect fit
            };

            // Lines that must shrink too much (or that overflow with no shrink) are
            // invalid and cannot start an optimal path.
            if ratio < -1.0 {
                continue;
            }

            // Calculate badness
            let mut badness = BADNESS_MULTIPLIER * ratio.abs().powi(3);

            // Add penalty for the break point
            if let Some(LayoutNode::Penalty { penalty, .. }) = nodes.get(i) {
                if *penalty >= 0.0 {
                    badness += penalty;
                } else if *penalty <= -INFINITY_BADNESS {
                    badness = -INFINITY_BADNESS; // Forced break
                }
            }

            // TODO: Add demerits for consecutive lines with very different
            // ratios (fitness classes).
            //
            // For now, demerit is simply the cumulative badness.
            let demerit = badness + breakpoints[j].demerit;

            if demerit < breakpoints[i + 1].demerit {
                breakpoints[i + 1] = Breakpoint {
                    demerit,
                    previous: j,
                    line: breakpoints[j].line + 1,
                };
            }
        }
    }

    // Backtrack from the end to find the break points
    let mut breaks = Vec::new();
    let mut current = nodes.len();
    while current > 0 {
        breaks.push(current);
        let prev_idx = breakpoints[current].previous;
        current = prev_idx;
    }
    breaks.reverse();
    breaks
}

/// Takes the optimal break points and performs the final positioning.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/counter/fixed-point cast
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn position_lines_from_breaks(
    nodes: &[LayoutNode],
    breaks: &[usize],
    logical_items: &[LogicalItem],
    constraints: &UnifiedConstraints,
) -> UnifiedLayout {
    let mut positioned_items = Vec::new();
    let mut start_node = 0;
    let mut cross_axis_pen = 0.0;
    let base_direction = get_base_direction_from_logical(logical_items);
    for (line_index, &end_node) in breaks.iter().enumerate() {
        let line_nodes = &nodes[start_node..end_node];
        let is_last_line = line_index == breaks.len() - 1;

        let mut line_items: Vec<ShapedItem> = line_nodes
            .iter()
            .filter_map(|node| match node {
                LayoutNode::Box(item, _) => Some(item.clone()),
                LayoutNode::Glue { item, .. } => Some(item.clone()),
                LayoutNode::Penalty { item, .. } => item.clone(),
            })
            .collect();

        // +spec CSS Text 3 §4.1.2: a line's trailing (line-terminating) spaces
        // "hang" — they are removed before measuring the line and are not counted
        // as justification opportunities. The break index sits just past the
        // Penalty following the trailing Glue, so that space is the last item
        // here. Drop trailing word separators so line_width, the justification
        // space count, and positioning all exclude them (matching the greedy
        // break_one_line path, which trims trailing spaces).
        while line_items.last().is_some_and(is_word_separator) {
            line_items.pop();
        }

        // Note: Calculate spacing, do not mutate items
        let mut extra_per_space = 0.0;
        let line_width: f32 = line_items.iter().map(|i| get_item_measure(i, false)).sum();

        // the last line and lines ending with a forced break
        let ends_with_forced_break = line_nodes.iter().any(|n| matches!(
            n, LayoutNode::Penalty { penalty, .. } if *penalty <= -INFINITY_BADNESS
        ));
        let effective_align = super::cache::resolve_effective_alignment(
            constraints.text_align,
            constraints.text_align_last,
            is_last_line || ends_with_forced_break,
        );

        // +spec:display-contents:858337 - text-align justification: last line start-aligned, justify-all forces last line justify
        // +spec:display-property:50e074 - justify stretches spaces/words in inline boxes, not inline-table/inline-block
        // +spec:display-property:ce8d54 - text-justify selects justification method, inherited from block containers to root inline box
        let should_justify = constraints.text_justify != JustifyContent::None
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll
                || effective_align == TextAlign::Justify || effective_align == TextAlign::JustifyAll);

        // Get the available width as f32 for calculations
        // For MinContent/MaxContent, we use the actual computed line_width
        // since there's no "available" space to justify into.
        let available_width_f32 = match constraints.available_width {
            AvailableSpace::Definite(w) => w,
            AvailableSpace::MaxContent => line_width,
            AvailableSpace::MinContent => line_width,
        };

        if should_justify {
            let space_to_add = available_width_f32 - line_width;
            if space_to_add > 0.0 {
                let space_count = line_items
                    .iter()
                    .filter(|item| is_word_separator(item))
                    .count();
                if space_count > 0 {
                    extra_per_space = space_to_add / space_count as f32;
                }
            }
        }

        // Alignment & Positioning
        let total_width: f32 = line_items
            .iter()
            .map(|item| get_item_measure(item, false))
            .sum();

        // For MaxContent, don't apply alignment (treat as left-aligned)
        let is_indefinite = matches!(
            constraints.available_width,
            AvailableSpace::MaxContent | AvailableSpace::MinContent
        );
        let remaining_space = if is_indefinite {
            0.0
        } else {
            available_width_f32
                - (total_width
                    + extra_per_space
                        * line_items
                            .iter()
                            .filter(|item| is_word_separator(item))
                            .count() as f32)
        };

        // +spec:writing-modes:155a06 - resolve start/end edges of line box per bidi direction
        let physical_align = match (effective_align, base_direction) {
            (TextAlign::Start, BidiDirection::Ltr) => TextAlign::Left,
            (TextAlign::Start, BidiDirection::Rtl) => TextAlign::Right,
            (TextAlign::End, BidiDirection::Ltr) => TextAlign::Right,
            (TextAlign::End, BidiDirection::Rtl) => TextAlign::Left,
            (other, _) => other,
        };

        // +spec:display-contents:5a1b30 - overflowing lines are start-aligned (overflow off end edge)
        let mut main_axis_pen = if remaining_space < 0.0 {
            0.0
        } else {
            match physical_align {
                TextAlign::Center => remaining_space / 2.0,
                TextAlign::Right => remaining_space,
                _ => 0.0,
            }
        };

        // +spec:display-contents:21b27a - text-indent applies to initial letter's originating line as usual
        // +spec:line-breaking:bc389d - text-indent with each-line/hanging keywords
        if constraints.text_indent != 0.0 {
            // TODO: with text-indent-each-line, also detect lines after forced breaks in the KP path
            let is_indent_target = line_index == 0;
            let should_indent = if constraints.text_indent_hanging {
                !is_indent_target
            } else {
                is_indent_target
            };
            if should_indent {
                main_axis_pen += constraints.text_indent;
            }
        }

        for item in line_items {
            let item_advance = get_item_measure(&item, false);

            let draw_pos = match &item {
                ShapedItem::Cluster(c) if !c.glyphs.is_empty() => {
                    let glyph = &c.glyphs[0];
                    Point {
                        x: main_axis_pen + glyph.offset.x,
                        y: cross_axis_pen - glyph.offset.y, // Use - for GPOS offset
                    }
                }
                _ => Point {
                    x: main_axis_pen,
                    y: cross_axis_pen,
                },
            };

            positioned_items.push(PositionedItem {
                item: item.clone(),
                position: draw_pos,
                line_index,
            });

            main_axis_pen += item_advance;

            // Apply extra spacing to the pen
            if is_word_separator(&item) {
                main_axis_pen += extra_per_space;
            }
        }

        // +spec:box-model:96f5a7 - line box height uses line-height only; inline margins/borders/padding do not enter calculation
        cross_axis_pen += constraints.resolved_line_height();
        start_node = end_node;
    }

    let mut layout = UnifiedLayout {
        items: positioned_items,
        overflow: OverflowInfo::default(),
    };
    // Record the unclipped content bounds. `overflow_items` stays empty by
    // design: every item is positioned (visual overflow is clipped at paint
    // time), so nothing is dropped here. TODO(superplan): populate
    // `overflow_items` only if a path actually discards content that doesn't fit.
    let bounds = layout.bounds();
    layout.overflow.unclipped_bounds = bounds;
    layout
}

#[cfg(test)]
mod kp_fix_tests {
    use super::*;
    use crate::text3::cache::{ShapedCluster, StyleProperties, UnifiedConstraints};
    use azul_core::selection::{ContentIndex, GraphemeClusterId};
    use azul_css::props::basic::FontRef;
    use std::sync::Arc;

    fn cl(text: &str, advance: f32) -> ShapedItem {
        ShapedItem::Cluster(ShapedCluster {
            text: text.to_string(),
            source_cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: 0 },
            source_content_index: ContentIndex { run_index: 0, item_index: 0 },
            source_node_id: None,
            glyphs: smallvec::SmallVec::new(),
            advance,
            direction: BidiDirection::Ltr,
            style: Arc::new(StyleProperties::default()),
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        })
    }

    fn nodes_for(text: &str) -> Vec<LayoutNode> {
        // Build per-grapheme clusters like the real shaper. 12px letters, 6px '-', 5px space.
        let items: Vec<ShapedItem> = text
            .chars()
            .map(|c| {
                let s = c.to_string();
                let adv = match c {
                    ' ' => 5.0,
                    '-' => 6.0,
                    _ => 12.0,
                };
                cl(&s, adv)
            })
            .collect();
        let fonts: LoadedFonts<FontRef> = LoadedFonts::new();
        convert_items_to_nodes(&items, None, &fonts)
    }

    #[test]
    fn bug1_terminal_break_wraps_word_ending_paragraph() {
        // "aaaa aaaa" (ends in a Box, no trailing space) must wrap at width 60.
        let nodes = nodes_for("aaaa aaaa");
        assert!(matches!(nodes.last(), Some(LayoutNode::Penalty { penalty, .. }) if *penalty <= -INFINITY_BADNESS),
            "a terminal forced break must be appended");
        let c = UnifiedConstraints { available_width: AvailableSpace::Definite(60.0), ..Default::default() };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        assert!(breaks.len() >= 2, "must break into >=2 lines, got {breaks:?}");
        assert_eq!(*breaks.last().unwrap(), nodes.len(), "final break at n");
    }

    #[test]
    fn bug2_hyphen_is_break_opportunity() {
        // "aaaa-aaaa": a zero-width penalty must follow the '-' Box.
        let nodes = nodes_for("aaaa-aaaa");
        // find the '-' box and assert the next node is a zero-width penalty
        let mut found = false;
        for (i, n) in nodes.iter().enumerate() {
            if let LayoutNode::Box(ShapedItem::Cluster(cc), _) = n {
                if cc.text == "-" {
                    match nodes.get(i + 1) {
                        Some(LayoutNode::Penalty { penalty, width, .. }) => {
                            assert!(*width == 0.0 && *penalty > -INFINITY_BADNESS,
                                "hyphen must be followed by a zero-width soft penalty");
                            found = true;
                        }
                        other => panic!("expected penalty after hyphen, got {other:?}"),
                    }
                }
            }
        }
        assert!(found, "hyphen box must exist");
        // and it must actually enable a wrap at width 60
        let c = UnifiedConstraints { available_width: AvailableSpace::Definite(60.0), ..Default::default() };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        assert!(breaks.len() >= 2, "hyphenated token must wrap, got {breaks:?}");
    }

    #[test]
    fn bug4_min_content_breaks_every_word() {
        // "aaaa aaaa" min-content: two words => two content lines (widest = one word).
        let nodes = nodes_for("aaaa aaaa");
        let c = UnifiedConstraints { available_width: AvailableSpace::MinContent, ..Default::default() };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        // there must be a break after the first word's trailing space penalty,
        // i.e. more than one break -> not a single spanning line.
        assert!(breaks.len() >= 2, "min-content must break per word, got {breaks:?}");
    }

    #[test]
    fn bug3_trailing_space_trimmed_from_line() {
        // "aaaa aaaa" @60 wraps to [.., 6, 11]; line 0 = "aaaa" + trailing space.
        let nodes = nodes_for("aaaa aaaa");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::Definite(60.0),
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        // The line-terminating space must not be positioned on line 0.
        let line0_spaces = layout
            .items
            .iter()
            .filter(|it| it.line_index == 0 && is_word_separator(&it.item))
            .count();
        assert_eq!(line0_spaces, 0, "trailing space must be trimmed from line 0");
        // Rightmost cluster edge on line 0 == 4*12 = 48px (not 53 incl. the space).
        let max_x = layout
            .items
            .iter()
            .filter(|it| it.line_index == 0)
            .filter_map(|it| it.item.as_cluster().map(|cc| it.position.x + cc.advance))
            .fold(0.0f32, f32::max);
        assert!((max_x - 48.0).abs() < 0.01, "line 0 right edge {max_x} should be 48px");
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact geometry: every advance below is an exact f32
#[allow(clippy::cast_precision_loss)] // small line counters
mod autotest_generated {
    use std::sync::Arc;

    use azul_core::selection::{ContentIndex, GraphemeClusterId};
    use azul_css::props::basic::FontRef;

    use super::*;
    use crate::text3::cache::{
        BreakType, ClearType, InlineBreak, InlineContent, LineHeight, Rect, ShapedCluster,
        StyleProperties,
    };

    // ---------------------------------------------------------------------
    // Builders. Every item is a single-grapheme cluster with an explicit
    // advance, mirroring what the shaper produces: 12px letters, 5px space,
    // 6px hyphen. No glyphs -> get_item_measure() == advance exactly, so all
    // expected coordinates below are exact integers.
    // ---------------------------------------------------------------------

    fn cl(text: &str, advance: f32) -> ShapedItem {
        ShapedItem::Cluster(ShapedCluster {
            text: text.to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            source_content_index: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            source_node_id: None,
            glyphs: smallvec::SmallVec::new(),
            advance,
            direction: BidiDirection::Ltr,
            style: Arc::new(StyleProperties::default()),
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        })
    }

    fn advance_of(c: char) -> f32 {
        match c {
            ' ' => 5.0,
            '-' => 6.0,
            _ => 12.0,
        }
    }

    /// One cluster per `char`, like the real shaper hands to `kp_layout`.
    fn items_of(text: &str) -> Vec<ShapedItem> {
        text.chars()
            .map(|c| cl(&c.to_string(), advance_of(c)))
            .collect()
    }

    fn no_fonts() -> LoadedFonts<FontRef> {
        LoadedFonts::new()
    }

    /// `convert_items_to_nodes` with hyphenation disabled (the hyphenator is a
    /// feature-gated type and needs a real dictionary; `None` is the path the
    /// engine takes whenever `hyphens: none`).
    fn nodes_of(items: &[ShapedItem]) -> Vec<LayoutNode> {
        convert_items_to_nodes(items, None, &no_fonts())
    }

    fn nodes_for(text: &str) -> Vec<LayoutNode> {
        nodes_of(&items_of(text))
    }

    fn definite(width: f32) -> UnifiedConstraints {
        UnifiedConstraints {
            available_width: AvailableSpace::Definite(width),
            ..Default::default()
        }
    }

    fn object(width: f32) -> ShapedItem {
        ShapedItem::Object {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height: 10.0,
            },
            baseline_offset: 0.0,
            content: InlineContent::Tab {
                style: Arc::new(StyleProperties::default()),
            },
        }
    }

    fn tab(width: f32) -> ShapedItem {
        ShapedItem::Tab {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height: 10.0,
            },
        }
    }

    fn hard_break() -> ShapedItem {
        ShapedItem::Break {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            break_info: InlineBreak {
                break_type: BreakType::Hard,
                clear: ClearType::None,
                content_index: 0,
            },
        }
    }

    fn rtl_logical() -> Vec<LogicalItem> {
        vec![LogicalItem::Text {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            text: "\u{05E9}\u{05DC}\u{05D5}\u{05DD}".to_string(), // "שלום"
            style: Arc::new(StyleProperties::default()),
            marker_position_outside: None,
            source_node_id: None,
        }]
    }

    // ---------------------------------------------------------------------
    // Inspectors
    // ---------------------------------------------------------------------

    fn is_forced(node: &LayoutNode) -> bool {
        matches!(node, LayoutNode::Penalty { penalty, .. } if *penalty <= -INFINITY_BADNESS)
    }

    fn penalty_count(nodes: &[LayoutNode]) -> usize {
        nodes
            .iter()
            .filter(|n| matches!(n, LayoutNode::Penalty { .. }))
            .count()
    }

    fn line_text(layout: &UnifiedLayout, line: usize) -> String {
        layout
            .items
            .iter()
            .filter(|it| it.line_index == line)
            .filter_map(|it| it.item.as_cluster().map(|c| c.text.clone()))
            .collect()
    }

    fn line_count(layout: &UnifiedLayout) -> usize {
        layout
            .items
            .iter()
            .map(|it| it.line_index + 1)
            .max()
            .unwrap_or(0)
    }

    fn line_left(layout: &UnifiedLayout, line: usize) -> f32 {
        layout
            .items
            .iter()
            .filter(|it| it.line_index == line)
            .map(|it| it.position.x)
            .fold(f32::INFINITY, f32::min)
    }

    fn line_right(layout: &UnifiedLayout, line: usize) -> f32 {
        layout
            .items
            .iter()
            .filter(|it| it.line_index == line)
            .map(|it| it.position.x + get_item_measure(&it.item, false))
            .fold(f32::NEG_INFINITY, f32::max)
    }

    /// Structural contract of `find_optimal_breakpoints`: the returned indices
    /// are strictly increasing, in range, and the paragraph ends at `n`.
    fn assert_breaks_well_formed(nodes: &[LayoutNode], breaks: &[usize], what: &str) {
        for w in breaks.windows(2) {
            assert!(
                w[0] < w[1],
                "{what}: breaks must be strictly increasing, got {breaks:?}"
            );
        }
        for &b in breaks {
            assert!(
                b <= nodes.len(),
                "{what}: break {b} out of range (n = {})",
                nodes.len()
            );
        }
        if !breaks.is_empty() {
            assert_eq!(
                *breaks.last().unwrap(),
                nodes.len(),
                "{what}: the paragraph must end at the last node"
            );
        }
    }

    // =====================================================================
    // convert_items_to_nodes: structure of the Box/Glue/Penalty stream
    // =====================================================================

    #[test]
    fn convert_empty_items_yields_no_nodes() {
        // The terminal-forced-break append is guarded on !nodes.is_empty(),
        // so an empty paragraph must stay empty (not gain a lone penalty).
        assert!(nodes_of(&[]).is_empty());
    }

    #[test]
    fn convert_appends_exactly_one_terminal_forced_break() {
        let nodes = nodes_for("ab cd");
        assert!(is_forced(nodes.last().unwrap()), "paragraph must be anchored");
        assert_eq!(
            nodes.iter().filter(|n| is_forced(n)).count(),
            1,
            "exactly one forced break for a paragraph with no explicit breaks"
        );
    }

    #[test]
    fn convert_does_not_duplicate_terminal_break_after_an_explicit_break() {
        let items = vec![cl("a", 12.0), hard_break()];
        let nodes = nodes_of(&items);
        assert_eq!(nodes.len(), 2, "Box + the Break's own forced Penalty, no more");
        assert!(is_forced(&nodes[1]));
        assert_eq!(nodes.iter().filter(|n| is_forced(n)).count(), 1);
    }

    #[test]
    fn convert_space_glue_uses_the_documented_stretch_shrink_ratios() {
        let nodes = nodes_for("a b");
        let glue = nodes
            .iter()
            .find(|n| matches!(n, LayoutNode::Glue { .. }))
            .expect("a space must become Glue");
        match glue {
            LayoutNode::Glue {
                width,
                stretch,
                shrink,
                ..
            } => {
                assert_eq!(*width, 5.0);
                assert_eq!(*stretch, 5.0 * SPACE_STRETCH_RATIO);
                assert_eq!(*shrink, 5.0 * SPACE_SHRINK_RATIO);
                assert!(
                    *shrink < *width,
                    "a space may never shrink past zero width"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn convert_zero_width_space_becomes_an_itemless_penalty() {
        // U+200B is a wrap opportunity with no glyph: it must produce a
        // zero-width, zero-cost Penalty and contribute no Box.
        let items = vec![cl("a", 12.0), cl("\u{200B}", 0.0), cl("b", 12.0)];
        let nodes = nodes_of(&items);
        let zwsp = match &nodes[1] {
            LayoutNode::Penalty {
                item,
                width,
                penalty,
            } => (item.is_none(), *width, *penalty),
            other => panic!("expected a Penalty for U+200B, got {other:?}"),
        };
        assert_eq!(zwsp, (true, 0.0, 0.0));
        // ...and it really is a break opportunity at a narrow width.
        let breaks = find_optimal_breakpoints(&nodes, &definite(12.0));
        assert_breaks_well_formed(&nodes, &breaks, "zwsp");
    }

    #[test]
    fn convert_both_hyphen_codepoints_are_soft_wrap_opportunities() {
        // U+002D HYPHEN-MINUS and U+2010 HYPHEN are UAX#14 class BA: a break is
        // allowed AFTER them, and no extra hyphen glyph is inserted.
        for hyphen in ['\u{002D}', '\u{2010}'] {
            let items = vec![cl("a", 12.0), cl(&hyphen.to_string(), 6.0), cl("b", 12.0)];
            let nodes = nodes_of(&items);
            match (&nodes[1], &nodes[2]) {
                (
                    LayoutNode::Box(ShapedItem::Cluster(c), w),
                    LayoutNode::Penalty {
                        item,
                        width,
                        penalty,
                    },
                ) => {
                    assert_eq!(c.text, hyphen.to_string());
                    assert_eq!(*w, 6.0);
                    assert!(item.is_none(), "no extra hyphen glyph may be inserted");
                    assert_eq!(*width, 0.0);
                    assert!(
                        *penalty > -INFINITY_BADNESS,
                        "the hyphen break is optional, not forced"
                    );
                }
                other => panic!("expected Box('{hyphen}') + zero-width Penalty, got {other:?}"),
            }
        }
    }

    #[test]
    fn convert_atomic_inline_is_wrapped_in_wrap_opportunities() {
        // CSS Text 3 §5.1: a soft wrap opportunity exists before AND after each
        // replaced element / atomic inline.
        let nodes = nodes_of(&[object(40.0)]);
        assert!(matches!(nodes[0], LayoutNode::Penalty { .. }));
        assert!(matches!(nodes[1], LayoutNode::Box(_, w) if w == 40.0));
        assert!(matches!(nodes[2], LayoutNode::Penalty { .. }));
    }

    #[test]
    fn convert_tab_is_glue_and_not_a_wrap_opportunity() {
        // A tab is stretchable like a space but is NOT followed by a Penalty,
        // so no line may break at it.
        let nodes = nodes_of(&[cl("a", 12.0), tab(30.0), cl("b", 12.0)]);
        match &nodes[1] {
            LayoutNode::Glue {
                width,
                stretch,
                shrink,
                ..
            } => {
                assert_eq!(*width, 30.0);
                assert_eq!(*stretch, 30.0 * SPACE_STRETCH_RATIO);
                assert_eq!(*shrink, 30.0 * SPACE_SHRINK_RATIO);
            }
            other => panic!("a tab must become Glue, got {other:?}"),
        }
        assert_eq!(
            penalty_count(&nodes),
            1,
            "only the terminal forced break; a tab offers no wrap opportunity"
        );
    }

    #[test]
    fn nbsp_must_not_be_a_soft_wrap_opportunity() {
        // RED (expected to fail): UAX#14 class GL. cache.rs suppresses NBSP /
        // NNBSP / WJ / ZWNBSP as break opportunities in the greedy path
        // (`is_break_opportunity_with_word_break`, "otherwise 10\u{00A0}km
        // wrongly wraps"), but convert_items_to_nodes keys off
        // `is_word_separator`, which reports NBSP as a separator -- so the
        // Knuth-Plass path emits Glue + Penalty and "10\u{00A0}km" CAN wrap at
        // the no-break space. See the report accompanying this test batch.
        for nbsp in ['\u{00A0}', '\u{202F}'] {
            let items = vec![cl("1", 12.0), cl(&nbsp.to_string(), 5.0), cl("k", 12.0)];
            let nodes = nodes_of(&items);
            let optional_breaks = nodes
                .iter()
                .filter(|n| matches!(n, LayoutNode::Penalty { .. }) && !is_forced(n))
                .count();
            assert_eq!(
                optional_breaks, 0,
                "U+{:04X} is a no-break space: it must not offer a wrap opportunity, got {nodes:?}",
                nbsp as u32
            );
        }
    }

    #[test]
    fn cjk_ideographic_space_currently_offers_no_wrap_opportunity() {
        // Characterization: U+3000 is deliberately NOT a word separator (CSS
        // Text §7.1), and word_break is not threaded into kp_layout yet, so the
        // whole run collapses into one unbreakable word. Locks in today's
        // behavior; see the `word_break` TODO in convert_items_to_nodes.
        let items = vec![cl("\u{4E00}", 16.0), cl("\u{3000}", 16.0), cl("\u{4E8C}", 16.0)];
        let nodes = nodes_of(&items);
        assert_eq!(
            penalty_count(&nodes),
            1,
            "only the terminal forced break exists in the CJK run"
        );
        let breaks = find_optimal_breakpoints(&nodes, &definite(20.0));
        assert_breaks_well_formed(&nodes, &breaks, "cjk");
    }

    #[test]
    fn convert_survives_hostile_unicode_without_panicking() {
        // Combining marks, an emoji ZWJ sequence, RTL text, a lone surrogate is
        // impossible in Rust, so use the next-worst thing: unpaired combining
        // marks, an unassigned plane-15 codepoint, and a zero-advance cluster.
        let items = vec![
            cl("e\u{0301}", 12.0),
            cl("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", 48.0),
            cl("\u{05D0}", 12.0),
            cl("\u{FFFD}", 12.0),
            cl("\u{F0000}", 0.0),
            cl("", 0.0),
        ];
        let nodes = nodes_of(&items);
        // Nothing is a separator or a hyphen, so every cluster stays a Box.
        assert_eq!(
            nodes.iter().filter(|n| matches!(n, LayoutNode::Box(..))).count(),
            items.len()
        );
        let layout = kp_layout(&items, &[], &definite(30.0), None, &no_fonts());
        assert_eq!(
            layout.items.len(),
            items.len(),
            "no cluster may be dropped by the layout"
        );
    }

    // =====================================================================
    // find_optimal_breakpoints: numeric limits & structural invariants
    // =====================================================================

    #[test]
    fn breakpoints_of_an_empty_paragraph_are_empty() {
        assert!(find_optimal_breakpoints(&[], &definite(100.0)).is_empty());
        assert!(find_optimal_breakpoints(&[], &UnifiedConstraints::default()).is_empty());
    }

    #[test]
    fn breakpoints_of_an_empty_paragraph_under_min_content_stay_in_range() {
        // The MinContent fast path appends nodes.len() unconditionally, so an
        // empty node list yields [0]. That must still be a safe input to the
        // positioner (kp_layout short-circuits before this, but the DP is
        // callable on its own).
        let breaks = find_optimal_breakpoints(
            &[],
            &UnifiedConstraints {
                available_width: AvailableSpace::MinContent,
                ..Default::default()
            },
        );
        assert_breaks_well_formed(&[], &breaks, "empty min-content");
        let layout = position_lines_from_breaks(
            &[],
            &breaks,
            &[],
            &UnifiedConstraints {
                available_width: AvailableSpace::MinContent,
                ..Default::default()
            },
        );
        assert!(layout.items.is_empty());
    }

    #[test]
    fn breaks_stay_well_formed_across_pathological_widths() {
        let nodes = nodes_for("aa bb cccc");
        let widths = [
            AvailableSpace::Definite(0.0),
            AvailableSpace::Definite(1.0),
            AvailableSpace::Definite(-100.0),
            AvailableSpace::Definite(f32::MIN),
            AvailableSpace::Definite(f32::MAX),
            AvailableSpace::Definite(f32::INFINITY),
            AvailableSpace::Definite(f32::NEG_INFINITY),
            AvailableSpace::Definite(f32::NAN),
            AvailableSpace::Definite(f32::EPSILON),
            AvailableSpace::MinContent,
            AvailableSpace::MaxContent,
        ];
        for w in widths {
            let c = UnifiedConstraints {
                available_width: w,
                ..Default::default()
            };
            let breaks = find_optimal_breakpoints(&nodes, &c);
            assert_breaks_well_formed(&nodes, &breaks, &format!("{w:?}"));
            // The positioner must survive whatever the DP produced.
            let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
            assert!(
                layout.items.len() <= nodes.len(),
                "{w:?}: cannot position more items than there are nodes"
            );
        }
    }

    #[test]
    fn breaks_land_only_after_penalty_nodes_in_a_feasible_paragraph() {
        // Knuth-Plass may only break at a Penalty: `breaks[k]` is the index one
        // past the last node of a line, so nodes[breaks[k] - 1] must be one.
        let nodes = nodes_for("aa bb cccc");
        let breaks = find_optimal_breakpoints(&nodes, &definite(60.0));
        assert!(breaks.len() >= 2, "must wrap at 60px, got {breaks:?}");
        for &b in &breaks {
            assert!(
                matches!(nodes[b - 1], LayoutNode::Penalty { .. }),
                "break at {b} follows {:?}, which is not a legal break point",
                nodes[b - 1]
            );
        }
    }

    #[test]
    fn nan_advances_do_not_panic_or_hang() {
        let items = vec![
            cl("a", f32::NAN),
            cl(" ", f32::NAN),
            cl("b", f32::NAN),
            cl("c", 12.0),
        ];
        let nodes = nodes_of(&items);
        let c = definite(50.0);
        let breaks = find_optimal_breakpoints(&nodes, &c);
        assert_breaks_well_formed(&nodes, &breaks, "NaN advances");
        // Positioning NaN geometry may yield NaN coordinates, but must not panic
        // and must not lose content.
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        assert_eq!(
            layout
                .items
                .iter()
                .filter(|it| it.item.as_cluster().is_some_and(|cc| cc.text != " "))
                .count(),
            3
        );
    }

    #[test]
    fn infinite_advance_collapses_to_one_overfull_line_without_panicking() {
        let items = vec![cl("a", f32::INFINITY)];
        let nodes = nodes_of(&items);
        let c = definite(100.0);
        let breaks = find_optimal_breakpoints(&nodes, &c);
        assert_breaks_well_formed(&nodes, &breaks, "infinite advance");
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        assert_eq!(layout.items.len(), 1);
        // Overflowing lines are start-aligned, so the pen never moves off zero.
        assert_eq!(layout.items[0].position.x, 0.0);
    }

    #[test]
    fn extreme_text_indent_does_not_panic() {
        let nodes = nodes_for("aa bb cccc");
        for indent in [f32::MAX, f32::MIN, -1000.0, f32::NAN, f32::INFINITY] {
            for hanging in [false, true] {
                let c = UnifiedConstraints {
                    available_width: AvailableSpace::Definite(100.0),
                    text_indent: indent,
                    text_indent_hanging: hanging,
                    ..Default::default()
                };
                let breaks = find_optimal_breakpoints(&nodes, &c);
                assert_breaks_well_formed(&nodes, &breaks, &format!("indent {indent}"));
                let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
                assert!(layout.items.len() <= nodes.len());
            }
        }
    }

    #[test]
    fn zero_width_container_keeps_all_content() {
        // width: 0px is a genuinely zero-width container, not "unresolved":
        // every line overflows, but nothing may be dropped.
        let nodes = nodes_for("aa bb");
        let c = definite(0.0);
        let breaks = find_optimal_breakpoints(&nodes, &c);
        assert_breaks_well_formed(&nodes, &breaks, "zero width");
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        let letters = layout
            .items
            .iter()
            .filter(|it| it.item.as_cluster().is_some_and(|cc| cc.text != " "))
            .count();
        assert_eq!(letters, 4, "all four letters must still be positioned");
    }

    #[test]
    fn min_content_breaks_at_every_penalty() {
        let nodes = nodes_for("aa bb cccc");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::MinContent,
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        assert_breaks_well_formed(&nodes, &breaks, "min-content");
        // One break per Penalty node (the terminal penalty's break IS nodes.len()).
        assert_eq!(breaks.len(), penalty_count(&nodes));
        for &b in &breaks {
            assert!(matches!(nodes[b - 1], LayoutNode::Penalty { .. }));
        }
    }

    #[test]
    fn max_content_puts_an_unforced_paragraph_on_a_single_line() {
        let nodes = nodes_for("aa bb cccc");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        assert_eq!(
            breaks,
            vec![nodes.len()],
            "max-content must not wrap without a forced break"
        );
    }

    #[test]
    fn forced_break_item_starts_a_new_line_even_under_max_content() {
        let items = vec![cl("a", 12.0), hard_break(), cl("b", 12.0)];
        let c = UnifiedConstraints {
            available_width: AvailableSpace::MaxContent,
            ..Default::default()
        };
        let layout = kp_layout(&items, &[], &c, None, &no_fonts());
        assert_eq!(line_count(&layout), 2, "a Break must force a second line");
        assert_eq!(line_text(&layout, 0), "a");
        assert_eq!(line_text(&layout, 1), "b");
    }

    #[test]
    fn dp_terminates_on_a_paragraph_that_is_all_break_opportunities() {
        // 400 zero-width spaces: every node is a Penalty, i.e. the DP's worst
        // case (O(n^2) candidate pairs). It must terminate and produce a
        // well-formed, in-range break list.
        let items: Vec<ShapedItem> = (0..400).map(|_| cl("\u{200B}", 0.0)).collect();
        let nodes = nodes_of(&items);
        assert_eq!(penalty_count(&nodes), nodes.len());
        let breaks = find_optimal_breakpoints(&nodes, &definite(100.0));
        assert_breaks_well_formed(&nodes, &breaks, "all-penalty");
    }

    #[test]
    fn large_paragraph_keeps_every_glyph() {
        let text = "aaa ".repeat(300);
        let items = items_of(&text);
        let layout = kp_layout(&items, &[], &definite(100.0), None, &no_fonts());
        let letters = layout
            .items
            .iter()
            .filter(|it| it.item.as_cluster().is_some_and(|c| c.text == "a"))
            .count();
        assert_eq!(letters, 900, "no glyph may be lost while wrapping");
        // line_index is emitted in reading order.
        assert!(layout
            .items
            .windows(2)
            .all(|w| w[0].line_index <= w[1].line_index));
    }

    #[test]
    fn a_multi_line_paragraph_must_not_collapse_onto_one_overfull_line() {
        // RED (expected to fail): `breakpoints[].demerit` is seeded with
        // INFINITY_BADNESS, a *finite* 10_000 -- but demerits ACCUMULATE across
        // lines (`demerit = badness + breakpoints[j].demerit`). Once the optimal
        // path's cumulative demerit passes 10_000, no candidate can ever satisfy
        // `demerit < breakpoints[i + 1].demerit` again, so no further breakpoint
        // is recorded and the backtrack falls through the default
        // `previous: 0` -- putting the entire paragraph on one overfull line.
        //
        // Here each 2-word line has ratio 3.6 -> badness ~4_666, so the third
        // line (cumulative ~13_997) is already unrepresentable. Any paragraph of
        // loose lines hits this; even perfectly-fitting lines hit it once they
        // are numerous enough. Fix: seed `demerit` with f32::INFINITY instead of
        // reusing the badness constant as the sentinel.
        let text = "aaa ".repeat(8); // 8 words, 36px each + 5px spaces
        let items = items_of(&text);
        let layout = kp_layout(&items, &[], &definite(100.0), None, &no_fonts());
        let lines = line_count(&layout);
        assert!(
            lines >= 4,
            "at most 2 of these 41px words fit per 100px line, so 8 words need \
             >= 4 lines; got {lines}"
        );
        for line in 0..lines {
            assert!(
                line_right(&layout, line) <= 100.5,
                "line {line} runs to {}px in a 100px container",
                line_right(&layout, line)
            );
        }
    }

    // =====================================================================
    // position_lines_from_breaks: geometry, alignment, justification
    // =====================================================================

    #[test]
    fn position_with_no_breaks_yields_an_empty_layout() {
        let nodes = nodes_for("ab");
        let layout = position_lines_from_breaks(&nodes, &[], &[], &definite(100.0));
        assert!(layout.items.is_empty());
        assert_eq!(layout.overflow.unclipped_bounds.width, 0.0);
    }

    #[test]
    fn position_tolerates_a_repeated_break_index() {
        // A degenerate empty line (start == end) must not panic or shift content.
        let nodes = nodes_for("aa bb cccc");
        let n = nodes.len();
        let layout = position_lines_from_breaks(&nodes, &[8, 8, n], &[], &definite(100.0));
        assert_eq!(line_text(&layout, 0), "aa bb");
        assert_eq!(line_text(&layout, 1), "", "the empty line holds nothing");
        assert_eq!(line_text(&layout, 2), "cccc");
    }

    #[test]
    fn lines_advance_by_exactly_the_resolved_line_height() {
        let nodes = nodes_for("aa bb cccc");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::Definite(60.0),
            line_height: LineHeight::Px(20.0),
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        assert_eq!(c.resolved_line_height(), 20.0);
        for it in &layout.items {
            assert_eq!(
                it.position.y,
                20.0 * it.line_index as f32,
                "line {} must sit at {}px",
                it.line_index,
                20.0 * it.line_index as f32
            );
        }
    }

    #[test]
    fn left_aligned_pen_starts_at_zero_and_never_moves_backwards() {
        let nodes = nodes_for("aa bb cccc");
        let c = definite(60.0);
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        for line in 0..line_count(&layout) {
            let xs: Vec<f32> = layout
                .items
                .iter()
                .filter(|it| it.line_index == line)
                .map(|it| it.position.x)
                .collect();
            assert_eq!(xs[0], 0.0, "line {line} must start at the left edge");
            assert!(
                xs.windows(2).all(|w| w[0] <= w[1]),
                "the pen must advance monotonically on line {line}: {xs:?}"
            );
        }
    }

    #[test]
    fn center_and_right_alignment_use_the_exact_remaining_space() {
        // "aaaa" = 48px in a 100px box -> 52px of slack.
        let nodes = nodes_for("aaaa");
        for (align, expected_left) in [
            (TextAlign::Left, 0.0f32),
            (TextAlign::Center, 26.0),
            (TextAlign::Right, 52.0),
            (TextAlign::Start, 0.0),
            (TextAlign::End, 52.0),
        ] {
            let c = UnifiedConstraints {
                available_width: AvailableSpace::Definite(100.0),
                text_align: align,
                ..Default::default()
            };
            let breaks = find_optimal_breakpoints(&nodes, &c);
            let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
            assert_eq!(
                line_left(&layout, 0),
                expected_left,
                "{align:?} must place the line at {expected_left}px"
            );
            assert_eq!(line_right(&layout, 0), expected_left + 48.0);
        }
    }

    #[test]
    fn an_overflowing_line_is_start_aligned_even_when_right_aligned() {
        // +spec: overflowing lines overflow the END edge, so the pen stays at 0
        // instead of going negative.
        let nodes = nodes_for("aaaaaaaa"); // 96px
        for align in [TextAlign::Right, TextAlign::Center, TextAlign::End] {
            let c = UnifiedConstraints {
                available_width: AvailableSpace::Definite(50.0),
                text_align: align,
                ..Default::default()
            };
            let breaks = find_optimal_breakpoints(&nodes, &c);
            let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
            assert_eq!(
                line_left(&layout, 0),
                0.0,
                "{align:?}: an overfull line must not be pushed to a negative x"
            );
            assert_eq!(line_text(&layout, 0), "aaaaaaaa", "no glyph may be dropped");
        }
    }

    #[test]
    fn rtl_base_direction_flips_logical_start_and_end() {
        // 3 clusters = 36px in a 100px box -> 64px of slack.
        let nodes = nodes_for("abc");
        let c_start = UnifiedConstraints {
            available_width: AvailableSpace::Definite(100.0),
            text_align: TextAlign::Start,
            ..Default::default()
        };
        let c_end = UnifiedConstraints {
            text_align: TextAlign::End,
            ..c_start.clone()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c_start);

        // LTR paragraph: start = left, end = right.
        assert_eq!(
            line_left(&position_lines_from_breaks(&nodes, &breaks, &[], &c_start), 0),
            0.0
        );
        assert_eq!(
            line_left(&position_lines_from_breaks(&nodes, &breaks, &[], &c_end), 0),
            64.0
        );

        // RTL paragraph (Hebrew logical items): start = right, end = left.
        let rtl = rtl_logical();
        assert_eq!(get_base_direction_from_logical(&rtl), BidiDirection::Rtl);
        assert_eq!(
            line_left(
                &position_lines_from_breaks(&nodes, &breaks, &rtl, &c_start),
                0
            ),
            64.0
        );
        assert_eq!(
            line_left(&position_lines_from_breaks(&nodes, &breaks, &rtl, &c_end), 0),
            0.0
        );
    }

    #[test]
    fn justification_stretches_inner_lines_and_leaves_the_last_line_alone() {
        // "aa bb cccc" @60px wraps to ["aa bb ", "cccc"]. Line 0 is 53px wide
        // after trimming its hanging space, so its single interior space must
        // absorb all 7px of slack; the last line must stay at its natural width.
        let nodes = nodes_for("aa bb cccc");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::Definite(60.0),
            text_align: TextAlign::Justify,
            text_justify: JustifyContent::InterWord,
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        assert_eq!(line_count(&layout), 2, "breaks: {breaks:?}");
        assert_eq!(line_text(&layout, 0), "aa bb");
        assert_eq!(line_text(&layout, 1), "cccc");
        assert_eq!(line_right(&layout, 0), 60.0, "inner line must be justified");
        assert_eq!(
            line_right(&layout, 1),
            48.0,
            "text-align: justify must not stretch the last line"
        );
    }

    #[test]
    fn justification_of_a_space_less_line_does_not_divide_by_zero() {
        // The only space is the line-terminating one, which is trimmed before
        // the space count is taken: extra_per_space must stay 0, never NaN/inf.
        let nodes = nodes_for("aaaa aaaa");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::Definite(60.0),
            text_align: TextAlign::Justify,
            text_justify: JustifyContent::InterWord,
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        assert!(!layout.items.is_empty());
        for it in &layout.items {
            assert!(
                it.position.x.is_finite() && it.position.y.is_finite(),
                "no coordinate may be NaN or infinite: {:?}",
                it.position
            );
        }
        assert_eq!(line_right(&layout, 0), 48.0, "nothing to stretch, no stretch");
    }

    #[test]
    fn text_indent_offsets_only_the_first_line() {
        let nodes = nodes_for("aa bb cccc");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::Definite(80.0),
            text_indent: 10.0,
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        assert_eq!(line_count(&layout), 2, "breaks: {breaks:?}");
        assert_eq!(line_left(&layout, 0), 10.0, "first line is indented");
        assert_eq!(line_left(&layout, 1), 0.0, "later lines are not");
    }

    #[test]
    fn hanging_text_indent_offsets_every_line_but_the_first() {
        let nodes = nodes_for("aa bb cccc");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::Definite(80.0),
            text_indent: 10.0,
            text_indent_hanging: true,
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        assert_eq!(line_count(&layout), 2, "breaks: {breaks:?}");
        assert_eq!(line_left(&layout, 0), 0.0, "hanging: first line is flush");
        assert_eq!(line_left(&layout, 1), 10.0, "hanging: later lines indent");
    }

    #[test]
    fn every_line_trims_its_hanging_space() {
        // CSS Text 3 §4.1.2: line-terminating spaces hang; they are not measured
        // and are not justification opportunities.
        let nodes = nodes_for("aaaa aaaa aaaa");
        let c = definite(60.0);
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        assert!(line_count(&layout) >= 2, "breaks: {breaks:?}");
        for line in 0..line_count(&layout) {
            let text = line_text(&layout, line);
            assert!(
                !text.ends_with(' '),
                "line {line} kept its hanging space: {text:?}"
            );
            assert!(!text.is_empty());
        }
    }

    #[test]
    fn unclipped_bounds_enclose_every_positioned_item() {
        let nodes = nodes_for("aa bb cccc");
        let c = UnifiedConstraints {
            available_width: AvailableSpace::Definite(60.0),
            line_height: LineHeight::Px(20.0),
            ..Default::default()
        };
        let breaks = find_optimal_breakpoints(&nodes, &c);
        let layout = position_lines_from_breaks(&nodes, &breaks, &[], &c);
        let b = layout.overflow.unclipped_bounds;
        assert!(layout.overflow.overflow_items.is_empty(), "nothing is dropped");
        for it in &layout.items {
            assert!(
                it.position.x >= b.x - 0.01 && it.position.x <= b.x + b.width + 0.01,
                "item at {:?} escapes the recorded bounds {b:?}",
                it.position
            );
            assert!(it.position.y >= b.y - 0.01 && it.position.y <= b.y + b.height + 0.01);
        }
    }

    // =====================================================================
    // kp_layout: end-to-end smoke over extreme inputs
    // =====================================================================

    #[test]
    fn kp_layout_of_an_empty_paragraph_is_empty() {
        let layout = kp_layout(&[], &[], &definite(100.0), None, &no_fonts());
        assert!(layout.items.is_empty());
        assert!(layout.overflow.overflow_items.is_empty());
        assert_eq!(layout.overflow.unclipped_bounds.width, 0.0);
        assert_eq!(layout.overflow.unclipped_bounds.height, 0.0);
    }

    #[test]
    fn kp_layout_round_trips_the_paragraph_text_minus_hanging_spaces() {
        let items = items_of("aa bb cccc");
        let layout = kp_layout(&items, &[], &definite(60.0), None, &no_fonts());
        let round_tripped: String = (0..line_count(&layout))
            .map(|l| line_text(&layout, l))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            round_tripped, "aa bb cccc",
            "re-joining the lines with their trimmed break spaces must \
             reproduce the source text"
        );
    }

    #[test]
    fn kp_layout_no_panic_smoke_over_extreme_inputs() {
        let inputs: Vec<Vec<ShapedItem>> = vec![
            Vec::new(),
            items_of(" "),
            items_of("   "),
            items_of("-"),
            items_of("a-b"),
            items_of("\u{200B}"),
            vec![object(0.0)],
            vec![object(f32::MAX)],
            vec![tab(0.0), tab(f32::INFINITY)],
            vec![hard_break(), hard_break()],
            vec![cl("a", -50.0), cl(" ", -5.0), cl("b", -50.0)],
            vec![cl("a", f32::MAX), cl(" ", f32::MAX), cl("b", f32::MAX)],
            vec![cl("x", f32::NAN), tab(f32::NAN), object(f32::NAN)],
        ];
        let constraints = [
            definite(0.0),
            definite(-10.0),
            definite(f32::NAN),
            definite(f32::MAX),
            UnifiedConstraints {
                available_width: AvailableSpace::MinContent,
                text_align: TextAlign::JustifyAll,
                text_justify: JustifyContent::Distribute,
                ..Default::default()
            },
            UnifiedConstraints {
                available_width: AvailableSpace::MaxContent,
                text_align: TextAlign::End,
                text_align_last: TextAlign::Center,
                text_indent: -25.0,
                ..Default::default()
            },
        ];
        for items in &inputs {
            for c in &constraints {
                let layout = kp_layout(items, &[], c, None, &no_fonts());
                assert!(
                    layout.items.len() <= items.len() + 2,
                    "no item may be duplicated: {} positioned from {} shaped",
                    layout.items.len(),
                    items.len()
                );
                assert!(layout.overflow.overflow_items.is_empty());
            }
        }
    }
}
