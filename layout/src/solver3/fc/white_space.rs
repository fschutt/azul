//! White space processing, segment breaks and text transforms (CSS Text 3).

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontStyle, StyleFontWeight},
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            ColorU, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric,
        },
        layout::{
            LayoutBorderSpacing, LayoutClear, LayoutDisplay, LayoutFloat, LayoutHeight,
            LayoutJustifyContent, LayoutOverflow, LayoutPosition, LayoutTableLayout,
            LayoutTextJustify, LayoutWidth, LayoutWritingMode, ShapeInside, ShapeOutside,
            StyleBorderCollapse, StyleCaptionSide, StyleEmptyCells,
        },
        property::CssProperty,
        style::{
            BorderStyle, StyleDirection, StyleHyphens, StyleLineBreak, StyleListStylePosition,
            StyleListStyleType, StyleOverflowWrap, StyleTextAlign, StyleTextAlignLast,
            StyleTextBoxTrim, StyleTextCombineUpright, StyleTextOrientation, StyleUnicodeBidi,
            StyleVerticalAlign, StyleVisibility, StyleWhiteSpace, StyleWordBreak,
        },
    },
};
use rust_fontconfig::FcWeight;
use taffy::{AvailableSpace, LayoutInput, Line, Size as TaffySize};
#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    debug_ifc_layout, debug_info, debug_log, debug_table_layout, debug_warning,
    font_traits::{
        ContentIndex, FontLoaderTrait, ImageSource, InlineContent, InlineImage, InlineShape,
        LayoutFragment, ObjectFit, ParsedFontTrait, SegmentAlignment, ShapeBoundary,
        ShapeDefinition, ShapedItem, Size, StyleProperties, StyledRun, TextLayoutCache,
        UnifiedConstraints,
    },
    solver3::{
        geometry::{BoxProps, ContainingBlock as CBTY, EdgeSizes, IntrinsicSizes},
        getters::{
            get_clear, get_css_border_bottom_width, get_css_border_top_width, get_css_box_sizing,
            get_css_height, get_css_padding_bottom, get_css_padding_top, get_css_width,
            get_direction_property, get_display_property, get_element_font_size, get_float,
            get_list_style_position, get_list_style_type, get_overflow_x, get_overflow_y,
            get_parent_font_size, get_root_font_size, get_style_properties, get_text_align,
            get_text_box_edge_property, get_text_box_trim_property, get_text_orientation_property,
            get_unicode_bidi_property, get_vertical_align_property, get_visibility,
            get_white_space_property, get_writing_mode, MultiValue,
        },
        layout_tree::{
            AnonymousBoxType, CachedInlineLayout, LayoutNode, LayoutNodeCold, LayoutNodeHot,
            LayoutNodeId, LayoutNodeWarm, LayoutTree, PseudoElement,
        },
        positioning::get_position_type,
        scrollbar::{ScrollbarKind, ScrollbarRequirements},
        sizing::extract_text_from_node,
        taffy_bridge, LayoutContext, LayoutDebugMessage, LayoutError, Result,
    },
    text3::cache::{
        AvailableSpace as Text3AvailableSpace, BreakType, ClearType, InlineBreak,
        TextAlign as Text3TextAlign,
    },
};
#[allow(clippy::wildcard_imports)]
// the formatting contexts' items, re-exported from the sibling modules by mod.rs
use super::*;

/// Returns true if a character has Unicode line breaking class BK (mandatory break)
/// or NL (next line). Per CSS Text 3 §5.1, these must be treated as forced line
/// breaks regardless of the white-space property value.
#[inline]
pub(super) const fn is_bk_or_nl_class(c: char) -> bool {
    matches!(
        c,
        '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}

/// Splits text at all forced break points: newlines (\n, \r\n, \r) and BK/NL class chars.
/// Used for white-space modes that preserve segment breaks (pre, pre-wrap, pre-line, break-spaces).
// +spec:white-space-processing:af4e3f - each newline/segment break in text is treated as a segment
// break, interpreted per white-space property
pub(super) fn split_at_forced_breaks(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            segments.push(std::mem::take(&mut current));
        } else if c == '\r' {
            segments.push(std::mem::take(&mut current));
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
        } else if is_bk_or_nl_class(c) {
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    segments.push(current);
    segments
}

/// Splits text only at BK/NL class characters (not \n which is collapsed in normal/nowrap).
/// Used for white-space: normal/nowrap where \n is collapsed to space but BK/NL chars
/// still produce forced breaks per CSS Text 3 §5.1.
pub(super) fn split_at_bk_nl_chars(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if is_bk_or_nl_class(c) {
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    segments.push(current);
    segments
}

/// Returns true if the character is East Asian (CJK) for the purposes of
/// segment break transformation rules (CSS Text Level 3, §4.1.3).
pub(super) fn is_east_asian_wide(c: char) -> bool {
    let cp = c as u32;
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&cp)
    || (0x3400..=0x4DBF).contains(&cp)
    || (0x20000..=0x2A6DF).contains(&cp)
    || (0xF900..=0xFAFF).contains(&cp)
    // Hiragana
    || (0x3040..=0x309F).contains(&cp)
    // Katakana
    || (0x30A0..=0x30FF).contains(&cp)
    || (0x31F0..=0x31FF).contains(&cp)
    // CJK Radicals / Kangxi / Ideographic Description
    || (0x2E80..=0x2EFF).contains(&cp)
    || (0x2F00..=0x2FDF).contains(&cp)
    || (0x2FF0..=0x2FFF).contains(&cp)
    // CJK Symbols and Punctuation
    || (0x3000..=0x303F).contains(&cp)
    || (0x3200..=0x32FF).contains(&cp)
    || (0x3300..=0x33FF).contains(&cp)
    // Bopomofo
    || (0x3100..=0x312F).contains(&cp)
    // Hangul Syllables
    || (0xAC00..=0xD7AF).contains(&cp)
    // Fullwidth forms
    || (0xFF01..=0xFF60).contains(&cp)
    || (0xFFE0..=0xFFE6).contains(&cp)
}

// +spec:block-formatting-context:b78223 - fullwidth/wide chars treated as vertical script,
// halfwidth as horizontal per UAX#11
pub(super) fn is_east_asian_fullwidth_or_wide(ch: char) -> bool {
    let cp = ch as u32;
    // Exclude Hangul
    if (0x1100..=0x11FF).contains(&cp)
        || (0x3130..=0x318F).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0xA960..=0xA97F).contains(&cp)
        || (0xD7B0..=0xD7FF).contains(&cp)
    {
        return false;
    }
    is_east_asian_wide(ch)
        || (0xFF61..=0xFFDC).contains(&cp)
        || (0xFFE8..=0xFFEE).contains(&cp)
        || (0xA000..=0xA4CF).contains(&cp)
}

/// +spec:white-space-processing:159dbf - segment breaks converted to spaces (default transform)
/// +spec:white-space-processing:79891b - segment break transform: convert to space or remove
// +spec:white-space-processing:7e9529 - Segment break transformation rules (§4.1.3): collapse
// consecutive breaks, remove around ZWSP/CJK, else convert to space
/// Transforms segment breaks (newlines) in text according to CSS Text Level 3 §4.1.3.
/// - If adjacent to a zero-width space (U+200B), the segment break is removed.
/// - If both adjacent chars are East Asian F/W/H (not Hangul), removed entirely.
/// - Otherwise, converted to a single space.
pub(super) fn apply_segment_break_transform(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        if ch == '\n' || ch == '\r' {
            let break_end = if ch == '\r' && i + 1 < len && chars[i + 1] == '\n' {
                i + 2
            } else {
                i + 1
            };

            // +spec:white-space-processing:3c3680 - remove tabs/spaces around segment break before
            // transform §4.1.1: remove collapsible whitespace around segment breaks
            while result.ends_with(' ') || result.ends_with('\t') {
                result.pop();
            }

            let mut after_idx = break_end;
            while after_idx < len && (chars[after_idx] == ' ' || chars[after_idx] == '\t') {
                after_idx += 1;
            }

            let char_before = result.chars().last();
            let char_after = if after_idx < len {
                Some(chars[after_idx])
            } else {
                None
            };

            // Rule 1: adjacent to zero-width space → remove
            if char_before == Some('\u{200B}') || char_after == Some('\u{200B}') {
                // remove segment break
            }
            // Rule 2: both sides East Asian F/W/H (not Hangul) → remove
            else if let (Some(before), Some(after)) = (char_before, char_after) {
                if is_east_asian_fullwidth_or_wide(before) && is_east_asian_fullwidth_or_wide(after)
                {
                    // remove segment break
                } else {
                    result.push(' ');
                }
            } else {
                result.push(' ');
            }

            i = after_idx;
        } else {
            result.push(ch);
            i += 1;
        }
    }

    result
}

// ============================================================================
// +spec:white-space-processing:b64e38 - parser may normalize/collapse whitespace before CSS; CSS
// cannot restore

// +spec:display-property:1389e3 - bidi control characters per UAX #9 for Unicode bidirectional
// algorithm +spec:display-property:aad99b - inline boxes can be split into fragments due to bidi
// text processing Bidi_Control property (UAX #9). These characters are ignored during white-space
// processing.
pub(super) const fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{200E}' | // LEFT-TO-RIGHT MARK
        '\u{200F}' | // RIGHT-TO-LEFT MARK
        '\u{202A}' | // LEFT-TO-RIGHT EMBEDDING
        '\u{202B}' | // RIGHT-TO-LEFT EMBEDDING
        '\u{202C}' | // POP DIRECTIONAL FORMATTING
        '\u{202D}' | // LEFT-TO-RIGHT OVERRIDE
        '\u{202E}' | // RIGHT-TO-LEFT OVERRIDE
        '\u{2066}' | // LEFT-TO-RIGHT ISOLATE
        '\u{2067}' | // RIGHT-TO-LEFT ISOLATE
        '\u{2068}' | // FIRST STRONG ISOLATE
        '\u{2069}' | // POP DIRECTIONAL ISOLATE
        '\u{061C}' // ARABIC LETTER MARK
    )
}

/// +spec:white-space-processing:1188f6 - only spaces, tabs, and segment breaks are document white
/// space Returns true if `c` is a CSS "document white space character" per CSS Text Level 3 §4.1.
/// Only spaces (U+0020), tabs (U+0009), and segment breaks (LF, CR, FF) qualify.
/// Other Unicode whitespace (e.g. U+00A0 non-breaking space) is NOT document white space.
#[inline]
pub(super) const fn is_css_document_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

// +spec:white-space-processing:efbece - white-space property controls collapsing/preserving of
// formatting characters for rendering +spec:writing-modes:b87688 - inlines laid out with bidi
// reordering and white-space wrapping +spec:writing-modes:cdd4f1 - white space trimming before bidi
// reordering preserves end-of-line spaces per UAX9 L1 white space characters are processed prior to
// line breaking and bidi reordering +spec:inline-block:381c0c - white-space property: collapsing,
// wrapping, and forced breaks per mode +spec:display-property:8acfaa - Phase I white-space
// collapsing for each inline in an IFC, ignoring bidi controls
/// Splits text content into `InlineContent` items based on white-space CSS property.
///
/// For `white-space: pre`, `pre-wrap`, and `pre-line`, newlines (`\n`) are treated as
/// forced line breaks per CSS Text Level 3 specification:
/// <https://www.w3.org/TR/css-text-3/#white-space-property>
///
/// Additionally, Unicode characters with BK or NL line breaking class (VT, FF, NEL, LS, PS)
/// are always treated as forced line breaks regardless of the white-space value.
///
/// This function:
/// 1. Checks the white-space property of the node (or its parent for text nodes)
/// 2. If `pre`, `pre-wrap`, or `pre-line`: splits text by `\n` and inserts
///    `InlineContent::LineBreak`
/// 3. Otherwise: returns the text as a single `InlineContent::Text`
/// 4. In ALL modes: BK/NL class chars (VT, FF, NEL, LS, PS) produce forced breaks
///
/// Returns a Vec of `InlineContent` items that correctly represent line breaks.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
pub fn split_text_for_whitespace(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    text: &str,
    style: &Arc<StyleProperties>,
) -> Vec<InlineContent> {
    let mut result = white_space_runs(styled_dom, dom_id, text, style);

    // +spec:white-space-processing:5e3f70 - text-transform applied after Phase I collapsing, before
    // Phase II trimming This means full-width only transforms spaces (U+0020) to U+3000
    // IDEOGRAPHIC SPACE within preserved white space, because non-preserved spaces were already
    // collapsed in Phase I above.
    let text_transform = style.text_transform;
    if text_transform != text3::cache::TextTransform::None {
        for item in &mut result {
            if let InlineContent::Text(run) = item {
                run.text = Arc::from(apply_text_transform(&run.text, text_transform).as_str());
            }
        }
    }

    result
}

/// The white-space processing half of [`split_text_for_whitespace`] (CSS
/// Text 3 Phase I - collapsing, forced breaks, tabs), without the
/// `text-transform` it then applies: the text as the layout's carets count
/// its bytes, in the characters the DOM holds.
///
/// What the edit model (`LayoutWindow::get_text_before_textinput`) reads a
/// `white-space: normal` / `nowrap` text node as: the layout collapses "a   b"
/// to "a b" before it shapes it, so a caret after the 'b' is byte 3 - of the
/// collapsed text, not of the raw one. The case of the letters is
/// presentation, and a stored value never takes it on.
pub fn white_space_runs(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    text: &str,
    style: &Arc<StyleProperties>,
) -> Vec<InlineContent> {
    // (characters with the Bidi_Control property) as if they were not there"
    // Strip bidi control characters before white-space processing so they don't
    // interfere with collapsing (e.g. a bidi mark between two spaces).
    let text_owned;
    let text: &str = if text.chars().any(is_bidi_control) {
        text_owned = text
            .chars()
            .filter(|c| !is_bidi_control(*c))
            .collect::<String>();
        &text_owned
    } else {
        text
    };

    // Get the white-space property - TEXT NODES inherit from parent!
    // We need to check the parent element's white-space, not the text node itself
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let parent_id = node_hierarchy[dom_id].parent_id();

    // Try parent first, then fall back to the node itself
    let white_space = parent_id.map_or(StyleWhiteSpace::Normal, |parent| {
        let styled_nodes = styled_dom.styled_nodes.as_container();
        let parent_state = styled_nodes
            .get(parent)
            .map(|n| n.styled_node_state)
            .unwrap_or_default();

        match get_white_space_property(styled_dom, parent, &parent_state) {
            MultiValue::Exact(ws) => ws,
            _ => StyleWhiteSpace::Normal,
        }
    });

    let mut result = Vec::new();

    // +spec:white-space-processing:3a0f58 - HTML newlines normalized to U+000A, each treated as
    // segment break +spec:white-space-processing:6eb1a2 - CR (U+000D) not treated as segment
    // break by HTML; handle if inserted via DOM HTML parsers convert \r to \n during
    // preprocessing, but \r can survive via escape sequences (e.g. &#x0d;). Any remaining
    // U+000D must be treated identically to U+000A (line feed).
    let text_cr;
    let text: &str = if text.contains('\r') {
        text_cr = text.replace("\r\n", "\n").replace('\r', "\n");
        &text_cr
    } else {
        text
    };

    // +spec:white-space-processing:bd11da - white-space property: new lines, spaces/tabs, wrapping
    // per value table +spec:white-space-processing:b166c5 - segment breaks preserved as forced
    // line feeds for pre/pre-wrap/break-spaces/pre-line For `pre`, `pre-wrap`, `pre-line`, and
    // `break-spaces`, newlines must be preserved as forced breaks CSS Text Level 3: "Newlines
    // in the source will be honored as forced line breaks."
    match white_space {
        StyleWhiteSpace::Pre | StyleWhiteSpace::PreWrap | StyleWhiteSpace::BreakSpaces => {
            // Pre, pre-wrap, break-spaces: preserve whitespace and honor newlines
            // Split by newlines and BK/NL class chars, insert LineBreak between parts
            // Also handle tab characters (\t) by inserting InlineContent::Tab
            let segments = split_at_forced_breaks(text);
            let segment_count = segments.len();
            let mut content_index = 0;

            for (seg_idx, segment) in segments.into_iter().enumerate() {
                // Split the segment by tab characters and insert Tab elements
                let mut tab_parts = segment.split('\t').peekable();
                while let Some(part) = tab_parts.next() {
                    if !part.is_empty() {
                        result.push(InlineContent::Text(StyledRun {
                            text: Arc::from(part),
                            style: Arc::clone(style),
                            logical_start_byte: 0,
                            source_node_id: Some(dom_id),
                        }));
                    }

                    if tab_parts.peek().is_some() {
                        result.push(InlineContent::Tab {
                            style: Arc::clone(style),
                        });
                    }
                }

                if seg_idx + 1 < segment_count {
                    result.push(InlineContent::LineBreak(InlineBreak {
                        break_type: BreakType::Hard,
                        clear: ClearType::None,
                        content_index,
                    }));
                    content_index += 1;
                }
            }
        }
        StyleWhiteSpace::PreLine => {
            // Pre-line: collapse whitespace but honor newlines and BK/NL class chars
            let segments = split_at_forced_breaks(text);
            let segment_count = segments.len();
            let mut content_index = 0;

            for (seg_idx, segment) in segments.into_iter().enumerate() {
                // Collapse only CSS document white space within the line (not all Unicode
                // whitespace)
                let collapsed: String = segment
                    .split(|c: char| is_css_document_whitespace(c))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");

                if !collapsed.is_empty() {
                    result.push(InlineContent::Text(StyledRun {
                        text: Arc::from(collapsed.as_str()),
                        style: Arc::clone(style),
                        logical_start_byte: 0,
                        source_node_id: Some(dom_id),
                    }));
                }

                if seg_idx + 1 < segment_count {
                    result.push(InlineContent::LineBreak(InlineBreak {
                        break_type: BreakType::Hard,
                        clear: ClearType::None,
                        content_index,
                    }));
                    content_index += 1;
                }
            }
        }
        StyleWhiteSpace::Normal | StyleWhiteSpace::Nowrap => {
            // +spec:white-space-processing:adbebb - Phase I collapsing for normal/nowrap modes
            // CSS Text Level 3, Section 4.1.1 - Phase I: Collapsing and Transformation
            // https://www.w3.org/TR/css-text-3/#white-space-phase-1
            //
            // For `white-space: normal` and `nowrap`:
            // 1. Segment breaks are transformed per §4.1.3
            // 2. Any sequence of consecutive spaces/tabs is collapsed to a single space
            // 3. Leading/trailing spaces at line boundaries are handled during line layout
            //
            // are forced breaks regardless of white-space value. Split on them first,
            // then collapse whitespace within each segment.
            let segments = split_at_bk_nl_chars(text);
            let segment_count = segments.len();
            let mut content_index = 0;

            for (seg_idx, segment) in segments.into_iter().enumerate() {
                let after_segment_breaks = apply_segment_break_transform(&segment);

                // Collapse document white space within this segment (normal/nowrap rules)
                let collapsed: String = after_segment_breaks
                    .chars()
                    .map(|c| {
                        if is_css_document_whitespace(c) {
                            ' '
                        } else {
                            c
                        }
                    })
                    .collect::<String>()
                    .split(' ')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");

                let final_text = if collapsed.is_empty() && !segment.is_empty() {
                    " ".to_string()
                } else if !collapsed.is_empty() {
                    // Check if original had leading/trailing document whitespace
                    let had_leading = segment
                        .chars()
                        .next()
                        .is_some_and(is_css_document_whitespace);
                    let had_trailing = segment
                        .chars()
                        .last()
                        .is_some_and(is_css_document_whitespace);

                    let mut r = String::new();
                    if had_leading {
                        r.push(' ');
                    }
                    r.push_str(&collapsed);
                    if had_trailing && !had_leading {
                        r.push(' ');
                    } else if had_trailing && had_leading && collapsed.is_empty() { /* already have one space */
                    } else if had_trailing {
                        r.push(' ');
                    }
                    r
                } else {
                    collapsed
                };

                if !final_text.is_empty() {
                    result.push(InlineContent::Text(StyledRun {
                        text: Arc::from(final_text.as_str()),
                        style: Arc::clone(style),
                        logical_start_byte: 0,
                        source_node_id: Some(dom_id),
                    }));
                }

                // Insert forced break between segments (for BK/NL chars)
                if seg_idx + 1 < segment_count {
                    result.push(InlineContent::LineBreak(InlineBreak {
                        break_type: BreakType::Hard,
                        clear: ClearType::None,
                        content_index,
                    }));
                    content_index += 1;
                }
            }
        }
    }

    result
}

pub(super) fn apply_text_transform(text: &str, transform: text3::cache::TextTransform) -> String {
    use crate::text3::cache::TextTransform;
    match transform {
        TextTransform::None => text.to_string(),
        TextTransform::Uppercase => text.to_uppercase(),
        TextTransform::Lowercase => text.to_lowercase(),
        TextTransform::Capitalize => {
            let mut result = String::with_capacity(text.len());
            let mut prev_is_word_boundary = true;
            for c in text.chars() {
                if prev_is_word_boundary && c.is_alphabetic() {
                    for uc in c.to_uppercase() {
                        result.push(uc);
                    }
                    prev_is_word_boundary = false;
                } else {
                    result.push(c);
                    prev_is_word_boundary = c.is_whitespace() || c.is_ascii_punctuation();
                }
            }
            result
        }
        TextTransform::FullWidth => {
            // Full-width transforms ASCII characters to their full-width equivalents.
            // Spaces (U+0020) become U+3000 IDEOGRAPHIC SPACE — but only those that
            // survived Phase I collapsing (i.e. preserved white space).
            text.chars()
                .map(|c| match c {
                    ' ' => '\u{3000}', // U+0020 SPACE -> U+3000 IDEOGRAPHIC SPACE
                    '!'..='~' => {
                        // ASCII printable range U+0021..U+007E -> fullwidth U+FF01..U+FF5E
                        char::from_u32(c as u32 - 0x0021 + 0xFF01).unwrap_or(c)
                    }
                    _ => c,
                })
                .collect()
        }
    }
}
