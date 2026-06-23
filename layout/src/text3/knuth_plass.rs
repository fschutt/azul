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
) -> Result<UnifiedLayout, LayoutError> {
    if items.is_empty() {
        return Ok(UnifiedLayout {
            items: Vec::new(),
            overflow: OverflowInfo::default(),
        });
    }

    // Convert ShapedItems into a sequence of Boxes, Glue, and Penalties
    let nodes = convert_items_to_nodes(items, hyphenator, fonts);

    // Dynamic Programming to find optimal breakpoints
    let breaks = find_optimal_breakpoints(&nodes, constraints);

    // Use breakpoints to build and position the final lines
    let final_layout: UnifiedLayout =
        position_lines_from_breaks(&nodes, &breaks, logical_items, constraints);

    Ok(final_layout)
}

/// Converts a slice of `ShapedItems` into the Box/Glue/Penalty model.
// +spec:line-breaking:16e64c - soft wrap opportunity controls (word-break, overflow-wrap, line-break) threaded via UnifiedConstraints
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
                        current_word_clusters.push(next_cluster.clone());
                        item_iter.next(); // Consume the peeked item
                    } else {
                        // Stop if we hit a non-cluster item (space, object, etc.)
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

    nodes
}

/// Uses dynamic programming to find the optimal set of line breaks.
fn find_optimal_breakpoints(nodes: &[LayoutNode], constraints: &UnifiedConstraints) -> Vec<usize> {
    // For Knuth-Plass, we need a definite line width.
    //
    // For MaxContent, use a very large value (no line breaks unless forced).
    // For MinContent, we also use a large value but will break at every word boundary.
    // The actual min-content width is determined by the widest resulting line.

    let line_width = match constraints.available_width {
        AvailableSpace::Definite(w) => w,
        AvailableSpace::MaxContent => f32::MAX / 2.0,
        // For MinContent: use a large width and let the greedy line breaker
        // break after each word. We DON'T use 0.0 because that breaks after
        // every character (including mid-word).
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
                    INFINITY_BADNESS // Cannot shrink
                }
            } else {
                0.0 // Perfect fit
            };

            // Lines that must shrink too much are invalid.
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

        let line_items: Vec<ShapedItem> = line_nodes
            .iter()
            .filter_map(|node| match node {
                LayoutNode::Box(item, _) => Some(item.clone()),
                LayoutNode::Glue { item, .. } => Some(item.clone()),
                LayoutNode::Penalty { item, .. } => item.clone(),
            })
            .collect();

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
            let is_indent_target = if constraints.text_indent_each_line {
                line_index == 0 // TODO: also detect lines after forced breaks in KP path
            } else {
                line_index == 0
            };
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
