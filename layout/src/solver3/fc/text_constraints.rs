//! From CSS to text3: the constraints, fonts and languages an inline formatting context lays out with.

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

/// Helper: Convert `StyleFontStyle` to `text3::cache::FontStyle`
#[must_use]
pub const fn convert_font_style(style: StyleFontStyle) -> crate::font_traits::FontStyle {
    match style {
        StyleFontStyle::Normal => crate::font_traits::FontStyle::Normal,
        StyleFontStyle::Italic => crate::font_traits::FontStyle::Italic,
        StyleFontStyle::Oblique => crate::font_traits::FontStyle::Oblique,
    }
}

/// Helper: Convert `StyleFontWeight` to `FcWeight`
#[must_use]
pub const fn convert_font_weight(weight: StyleFontWeight) -> FcWeight {
    match weight {
        StyleFontWeight::W100 => FcWeight::Thin,
        StyleFontWeight::W200 => FcWeight::ExtraLight,
        StyleFontWeight::W300 | StyleFontWeight::Lighter => FcWeight::Light,
        StyleFontWeight::Normal => FcWeight::Normal,
        StyleFontWeight::W500 => FcWeight::Medium,
        StyleFontWeight::W600 => FcWeight::SemiBold,
        StyleFontWeight::Bold => FcWeight::Bold,
        StyleFontWeight::W800 => FcWeight::ExtraBold,
        StyleFontWeight::W900 | StyleFontWeight::Bolder => FcWeight::Black,
    }
}

/// The content language of DOM node `node` (HTML's "language of a node"):
/// the nearest `lang` attribute on it or an ancestor (`xml:lang` lands as the
/// same attribute, after `lang`, so the node's LAST one is read). `None`
/// when no element states one, or the nearest says `lang=""` ("unknown").
pub(super) fn content_language(styled_dom: &StyledDom, node: NodeId) -> Option<&str> {
    use azul_core::dom::{AttributeType, NodeData};

    let hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data: &[NodeData] = styled_dom.node_data.as_ref();
    let mut current = Some(node);
    while let Some(id) = current {
        let lang = node_data
            .get(id.index())?
            .attributes()
            .as_ref()
            .iter()
            .rev()
            .find_map(|a| match a {
                AttributeType::Lang(tag) => Some(tag.as_str().trim()),
                _ => None,
            });
        if let Some(tag) = lang {
            return (!tag.is_empty()).then_some(tag);
        }
        current = hierarchy.get(id).and_then(|h| h.parent_id());
    }
    None
}

/// The hyphenation resource for BCP 47 language tag `tag` (`en`, `en-US`,
/// `DE`): the tag's own, else its primary language subtag's (`en-AU` ->
/// `en`, RFC 4647 lookup). Tags compare without case. `None` for a language
/// without one (and in a build without `text_layout_hyphenation`). The one
/// reading of a language tag, for `-azul-hyphenation-language` and `lang`.
pub(super) fn hyphenation_language_of_tag(tag: &str) -> Option<crate::text3::script::Language> {
    #[cfg(feature = "text_layout_hyphenation")]
    {
        use hyphenation::Language;
        let of = |tag: &str| match tag {
            "en-us" | "en" => Some(Language::EnglishUS),
            "en-gb" => Some(Language::EnglishGB),
            "de-de" | "de" => Some(Language::German1996),
            "fr-fr" | "fr" => Some(Language::French),
            "es-es" | "es" => Some(Language::Spanish),
            "it-it" | "it" => Some(Language::Italian),
            "pt-pt" | "pt" => Some(Language::Portuguese),
            "nl-nl" | "nl" => Some(Language::Dutch),
            "pl-pl" | "pl" => Some(Language::Polish),
            "ru-ru" | "ru" => Some(Language::Russian),
            "zh-cn" | "zh" => Some(Language::Chinese),
            _ => None,
        };
        let tag = tag.trim().to_ascii_lowercase();
        of(tag.as_str()).or_else(|| tag.split('-').next().and_then(of))
    }
    #[cfg(not(feature = "text_layout_hyphenation"))]
    {
        let _ = tag;
        None
    }
}

/// Translates solver3 layout constraints into the text3 engine's unified constraints.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose
                                                               // layout/render/parse routine (one
                                                               // branch per case)
pub(super) fn translate_to_text3_constraints<'a, T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    constraints: &'a LayoutConstraints<'a>,
    styled_dom: &StyledDom,
    dom_id: NodeId,
    // The IFC root is an anonymous block box and `dom_id` the element it
    // borrows its style from: the element's columns are not its own.
    anonymous: bool,
) -> UnifiedConstraints {
    use azul_css::compact_cache::{
        DOM_HAS_EXCLUSION_MARGIN, DOM_HAS_HANGING_PUNCTUATION, DOM_HAS_HYPHENATION_LANGUAGE,
        DOM_HAS_HYPHENS, DOM_HAS_INITIAL_LETTER, DOM_HAS_INITIAL_LETTER_ALIGN, DOM_HAS_LINE_BREAK,
        DOM_HAS_LINE_CLAMP, DOM_HAS_OVERFLOW_WRAP, DOM_HAS_SHAPE_INSIDE,
        DOM_HAS_SHAPE_MARGIN, DOM_HAS_SHAPE_OUTSIDE, DOM_HAS_TEXT_ALIGN_LAST,
        DOM_HAS_TEXT_COMBINE_UPRIGHT, DOM_HAS_TEXT_JUSTIFY, DOM_HAS_UNICODE_BIDI,
        DOM_HAS_WORD_BREAK,
    };
    unsafe {
        crate::az_mark(0x60704_u32, (0x30u32));
    }
    // DOM-level declared flags: if a bit is clear, no node in this DOM
    // declared the corresponding property → cascade walks always return
    // None, and we use the default value directly. All flags default to
    // "set" when there is no compact cache (paranoid fallback).
    let dom_declared = styled_dom
        .css_property_cache
        .ptr
        .compact_cache
        .as_ref()
        .map_or(!0u32, |cc| cc.dom_declared_flags);

    // Convert floats into exclusion zones for text3 to flow around.
    let mut shape_exclusions = if let Some(ref bfc_state) = constraints.bfc_state {
        debug_info!(
            ctx,
            "[translate_to_text3] dom_id={:?}, converting {} floats to exclusions",
            dom_id,
            bfc_state.floats.floats.len()
        );
        bfc_state
            .floats
            .floats
            .iter()
            .enumerate()
            .map(|(i, float_box)| {
                let rect = text3::cache::Rect {
                    x: float_box.rect.origin.x,
                    y: float_box.rect.origin.y,
                    width: float_box.rect.size.width,
                    height: float_box.rect.size.height,
                };
                debug_info!(
                    ctx,
                    "[translate_to_text3]   Exclusion #{}: {:?} at ({}, {}) size {}x{}",
                    i,
                    float_box.kind,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height
                );
                ShapeBoundary::Rectangle(rect)
            })
            .collect()
    } else {
        debug_info!(
            ctx,
            "[translate_to_text3] dom_id={:?}, NO bfc_state - no float exclusions",
            dom_id
        );
        Vec::new()
    };

    debug_info!(
        ctx,
        "[translate_to_text3] dom_id={:?}, available_size={}x{}, shape_exclusions.len()={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        shape_exclusions.len()
    );

    // Map text-align and justify-content from CSS to text3 enums.
    let id = dom_id;
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;

    // Read CSS Shapes properties
    // For reference box, use the element's CSS height if available, otherwise available_size
    // This is important because available_size.height might be infinite during auto height
    // calculation
    let ref_box_height = if constraints.available_size.height.is_finite() {
        constraints.available_size.height
    } else {
        // Try to get explicit CSS height
        // NOTE: If height is infinite, we can't properly resolve % heights
        // This is a limitation - shape-inside with % heights requires finite containing block
        styled_dom
            .css_property_cache
            .ptr
            .get_height(node_data, &id, node_state)
            .and_then(|v| v.get_property())
            .and_then(|h| match h {
                LayoutHeight::Px(v) => {
                    // Only accept absolute units (px, pt, in, cm, mm) - no %, em, rem
                    // since we can't resolve relative units without proper context
                    match v.metric {
                        SizeMetric::Px => Some(v.number.get()),
                        SizeMetric::Pt => Some(v.number.get() * PT_TO_PX),
                        SizeMetric::In => Some(v.number.get() * super::super::calc::PX_PER_INCH),
                        SizeMetric::Cm => Some(
                            v.number.get() * super::super::calc::PX_PER_INCH / super::super::calc::CM_PER_INCH,
                        ),
                        SizeMetric::Mm => Some(
                            v.number.get() * super::super::calc::PX_PER_INCH / super::super::calc::MM_PER_INCH,
                        ),
                        _ => None, // Ignore %, em, rem
                    }
                }
                _ => None,
            })
            .unwrap_or(constraints.available_size.width) // Fallback: use width as height (square)
    };

    let reference_box = text3::cache::Rect {
        x: 0.0,
        y: 0.0,
        width: constraints.available_size.width,
        height: ref_box_height,
    };

    // shape-inside: Text flows within the shape boundary
    debug_info!(ctx, "Checking shape-inside for node {:?}", id);
    debug_info!(
        ctx,
        "Reference box: {:?} (available_size height was: {})",
        reference_box,
        constraints.available_size.height
    );

    let shape_boundaries = if dom_declared & DOM_HAS_SHAPE_INSIDE != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_shape_inside(node_data, &id, node_state)
            .and_then(|v| {
                debug_info!(ctx, "Got shape-inside value: {:?}", v);
                v.get_property()
            })
            .and_then(|shape_inside| {
                debug_info!(ctx, "shape-inside property: {:?}", shape_inside);
                if let ShapeInside::Shape(css_shape) = shape_inside {
                    debug_info!(
                        ctx,
                        "Converting CSS shape to ShapeBoundary: {:?}",
                        css_shape
                    );
                    let boundary =
                        ShapeBoundary::from_css_shape(css_shape, reference_box, ctx.debug_messages);
                    debug_info!(ctx, "Created ShapeBoundary: {:?}", boundary);
                    Some(vec![boundary])
                } else {
                    debug_info!(ctx, "shape-inside is None");
                    None
                }
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    debug_info!(
        ctx,
        "Final shape_boundaries count: {}",
        shape_boundaries.len()
    );

    // shape-outside: Text wraps around the shape (adds to exclusions)
    debug_info!(ctx, "Checking shape-outside for node {:?}", id);
    if dom_declared & DOM_HAS_SHAPE_OUTSIDE != 0 {
        if let Some(shape_outside_value) = styled_dom
            .css_property_cache
            .ptr
            .get_shape_outside(node_data, &id, node_state)
        {
            debug_info!(ctx, "Got shape-outside value: {:?}", shape_outside_value);
            if let Some(shape_outside) = shape_outside_value.get_property() {
                debug_info!(ctx, "shape-outside property: {:?}", shape_outside);
                if let ShapeOutside::Shape(css_shape) = shape_outside {
                    debug_info!(
                        ctx,
                        "Converting CSS shape-outside to ShapeBoundary: {:?}",
                        css_shape
                    );
                    let boundary =
                        ShapeBoundary::from_css_shape(css_shape, reference_box, ctx.debug_messages);
                    debug_info!(ctx, "Created ShapeBoundary (exclusion): {:?}", boundary);
                    shape_exclusions.push(boundary);
                }
            }
        } else {
            debug_info!(ctx, "No shape-outside value found");
        }
    }

    // TODO: clip-path will be used for rendering clipping (not text layout)

    let writing_mode = get_writing_mode(styled_dom, id, node_state).unwrap_or_default();

    let text_align = get_text_align(styled_dom, id, node_state).unwrap_or_default();

    let text_justify = if dom_declared & DOM_HAS_TEXT_JUSTIFY != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_text_justify(node_data, &id, node_state)
            .and_then(|s| s.get_property().copied())
            .unwrap_or_default()
    } else {
        LayoutTextJustify::default()
    };

    // Get font-size for resolving line-height
    // Use helper function which checks dependency chain first
    let font_size = get_element_font_size(styled_dom, id, node_state);

    // The IFC root's own computed style: the very `StyleProperties` its text
    // runs are built from (memoized per layout). The line-height and the
    // strut's font below come from it, so the strut sits around the baseline
    // exactly like the glyphs of the same font and line-height do. This used
    // to re-read `line-height` from the cascade on its own, and the two
    // readers disagreed: a node that declared none came out `1.2em` here but
    // `normal` on its runs.
    let root_style = crate::solver3::getters::get_style_properties_cached(
        &mut ctx.style_cache,
        styled_dom,
        id,
        ctx.system_style.as_ref(),
        PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
    );
    // CSS 2.2 §10.8.1: the strut has the ascent and descent of the block
    // container's FIRST AVAILABLE FONT. A synthetic 0.8em / 0.2em split
    // stood in for it, and the strut is part of EVERY line box (text3's
    // `position_one_line` unions it with the line's content): for any font
    // with another split (Times: 0.891em / 0.216em) the strut and the text
    // sat at different heights around the same baseline, and every line box
    // came out taller than its line-height by |(A - D) / 2 - 0.3em| -
    // `line-height: 19px` on 11pt text pitched its lines 19.55px apart. The
    // ascent and descent are the face's ROUNDED pixel metrics, exactly as a
    // glyph's (`LayoutFontMetrics::line_metrics_px`, Chrome's rounding), so
    // both boxes coincide for the same font. Until the face is loaded the
    // approximation stays.
    let strut_face = ctx
        .font_manager
        .first_available_font_metrics(&root_style.font_stack)
        .filter(|m| m.units_per_em > 0);
    let strut_font = strut_face.and_then(|m| m.line_metrics_px(root_style.font_size_px));
    // The same face's OS/2 x-height and cap height (`vertical-align:
    // middle`, `text-box-edge: ex / cap`); 0.5em / 0.7em where the face has
    // none or is not loaded yet.
    let strut_scale = strut_face.map(|m| root_style.font_size_px / f32::from(m.units_per_em));
    let strut_x_height = strut_face
        .and_then(|m| m.x_height)
        .zip(strut_scale)
        .map_or(font_size * 0.5, |(x_height, scale)| x_height * scale);
    let strut_cap_height = strut_face
        .and_then(|m| m.cap_height)
        .zip(strut_scale)
        .map_or(font_size * 0.7, |(cap_height, scale)| cap_height * scale);
    let (strut_ascent, strut_descent) =
        strut_font.map_or((font_size * 0.8, font_size * 0.2), |(a, d, _)| (a, d));
    // The root's `line-height: normal` IS that face's A + D + line gap: the
    // strut of a line holding nothing else (`<div><br></div>`, every blank
    // line Gmail writes) is as tall as a line of text in that font, as in a
    // browser, and its leading is shared like a glyph's
    // (`text3::cache::split_leading`). Without a loaded face `normal` stays
    // (the strut's 1em).
    let root_line_height = match (root_style.line_height, strut_font) {
        (text3::cache::LineHeight::Normal, Some((a, d, gap))) => {
            text3::cache::LineHeight::Px(a + d + gap)
        }
        (line_height, _) => line_height,
    };
    // The used line-height as a length, for the readers that need one
    // (`vertical-align: <percentage>`, `initial-letter`); `normal` without a
    // loaded face stands in as 1.2em there, as it always did.
    let line_height_px = match root_line_height {
        text3::cache::LineHeight::Px(px) => px,
        text3::cache::LineHeight::Normal => font_size * 1.2,
    };

    let hyphenation = if dom_declared & DOM_HAS_HYPHENS != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_hyphens(node_data, &id, node_state)
            .and_then(|s| s.get_property().copied())
            .unwrap_or_default()
    } else {
        StyleHyphens::default()
    };

    let word_break_css = if dom_declared & DOM_HAS_WORD_BREAK != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_word_break(node_data, &id, node_state)
            .and_then(|s| s.get_property().copied())
            .unwrap_or_default()
    } else {
        StyleWordBreak::default()
    };

    let overflow_wrap_css = if dom_declared & DOM_HAS_OVERFLOW_WRAP != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_overflow_wrap(node_data, &id, node_state)
            .and_then(|s| s.get_property().copied())
            .unwrap_or_default()
    } else {
        StyleOverflowWrap::default()
    };

    let line_break_css = if dom_declared & DOM_HAS_LINE_BREAK != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_line_break(node_data, &id, node_state)
            .and_then(|s| s.get_property().copied())
            .unwrap_or_default()
    } else {
        StyleLineBreak::default()
    };

    let text_align_last_css = if dom_declared & DOM_HAS_TEXT_ALIGN_LAST != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_text_align_last(node_data, &id, node_state)
            .and_then(|s| s.get_property().copied())
            .unwrap_or_default()
    } else {
        StyleTextAlignLast::default()
    };

    let overflow_behaviour = get_overflow_x(styled_dom, id, node_state).unwrap_or_default();

    // +spec:display-property:21f728 - vertical-align shorthand resolves inline-level box alignment
    // +spec:display-property:98fa8e - alignment-baseline values for inline-level boxes in IFC
    // (implemented via vertical-align shorthand) +spec:display-property:1f71ad - baseline-shift
    // + alignment-baseline longhands mapped through vertical-align +spec:display-property:
    // 89dd7b - line-relative shift values (top/center/bottom) and aligned subtree alignment
    // +spec:inline-formatting-context:21da06 - vertical-align uses line-over/line-under sides via
    // writing_mode logical mapping +spec:inline-formatting-context:295603 - baseline alignment:
    // vertical-align determines how inline boxes align (baseline, super, sub, etc.)
    // +spec:inline-formatting-context:7351bf - default alignment baseline is alphabetic in
    // horizontal typographic mode +spec:inline-formatting-context:85de3d - vertical-align
    // shorthand: alignment within line box +spec:inline-formatting-context:aa8af0 - alignment
    // baseline chosen by vertical-align, defaults to parent's dominant baseline
    // +spec:inline-formatting-context:e475d2 - baseline and vertical-align control transverse
    // alignment of inline content on line boxes +spec:overflow:d44eac - vertical-align inline
    // box alignment (CSS 2.2 model covers
    // baseline/top/middle/bottom/sub/super/text-top/text-bottom) +spec:writing-modes:313575 -
    // alignment-baseline: inline-level boxes align baselines within parent inline box's alignment
    // context along inline axis +spec:writing-modes:60ad67 - inline layout aligns boxes in
    // block axis via baselines +spec:writing-modes:0127e5 - line-relative directions:
    // line-over/under map to vertical-align top/bottom Get vertical-align from CSS property
    // cache (defaults to Baseline per CSS spec) +spec:inline-formatting-context:686f8b -
    // vertical-align shorthand: alignment-baseline + baseline-shift for inline boxes
    // +spec:inline-formatting-context:e579b6 - vertical-align / baseline alignment in inline
    // context +spec:inline-formatting-context:a01a75 - dominant baseline alignment for atomic
    // inlines
    //
    // CSS 2.2 section 10.8.1: vertical-align applies to INLINE-LEVEL boxes
    // and TABLE CELLS only. `id` here is the IFC ROOT (a block container:
    // div, td, th, ...) — its own vertical-align must NOT become the line
    // alignment of its anonymous inline content, which always starts from
    // the initial value (baseline). Inline spans and atomic inlines inside
    // the IFC carry their alignment per-item. A table cell's vertical-align
    // is consumed by position_table_cells (cell content block alignment),
    // and letting it leak in here double-applied it as an inline shift:
    // the UA `th { vertical-align: middle }` pushed every header glyph in
    // table-basic-001 ~9px below the padding box.
    let vertical_align = StyleVerticalAlign::Baseline;

    // +spec:display-property:c03a6b - baseline-shift (sub/super/length/percentage) and
    // line-relative (top/center/bottom) shifts handled via vertical-align
    let vertical_align = match vertical_align {
        StyleVerticalAlign::Baseline => text3::cache::VerticalAlign::Baseline,
        StyleVerticalAlign::Top => text3::cache::VerticalAlign::Top,
        StyleVerticalAlign::Middle => text3::cache::VerticalAlign::Middle,
        StyleVerticalAlign::Bottom => text3::cache::VerticalAlign::Bottom,
        StyleVerticalAlign::Sub => text3::cache::VerticalAlign::Sub,
        // +spec:inline-formatting-context:fe563c - vertical-align: super shifts inline to
        // superscript position +spec:inline-formatting-context:fe563c -
        // vertical-align:super shifts child to superscript position
        StyleVerticalAlign::Superscript => text3::cache::VerticalAlign::Super,
        StyleVerticalAlign::TextTop => text3::cache::VerticalAlign::TextTop,
        StyleVerticalAlign::TextBottom => text3::cache::VerticalAlign::TextBottom,
        // §10.8.1: <percentage> refers to line-height of the element itself
        StyleVerticalAlign::Percentage(p) => {
            text3::cache::VerticalAlign::Offset(p.normalized() * line_height_px)
        }
        // §10.8.1: <length> is absolute offset from baseline
        StyleVerticalAlign::Length(l) => {
            // Resolve viewport units (vw/vh/vmin/vmax) against the real viewport
            // instead of falling through `resolve_pixel_value`'s "treat 50vw as 50px".
            let offset = super::super::calc::resolve_pixel_value_with_viewport(
                &l,
                0.0,
                font_size,
                font_size,
                ctx.viewport_size.width,
                ctx.viewport_size.height,
            );
            text3::cache::VerticalAlign::Offset(offset)
        }
    };
    // +spec:block-formatting-context:987746 - text-orientation property (mixed/upright/sideways)
    // for vertical writing modes +spec:inline-formatting-context:cbe738 - text-orientation
    // (mixed/upright/sideways) bi-orientational transform for vertical text
    // +spec:writing-modes:09a1bb - vertical typesetting orientation (upright/sideways) for
    // vertical-rl/vertical-lr +spec:writing-modes:2eb1b2 - text-orientation
    // (mixed/upright/sideways) applied to vertical text layout
    let text_orientation = match get_text_orientation_property(styled_dom, id, node_state) {
        MultiValue::Exact(o) => match o {
            StyleTextOrientation::Mixed => text3::cache::TextOrientation::Mixed,
            StyleTextOrientation::Upright => text3::cache::TextOrientation::Upright,
            // +spec:block-formatting-context:a606e6 - sideways text typeset rotated 90° CW in
            // vertical modes
            StyleTextOrientation::Sideways => text3::cache::TextOrientation::Sideways,
        },
        _ => text3::cache::TextOrientation::default(),
    };

    // +spec:display-property:8364c0 - direction property (ltr/rtl) sets paragraph embedding level
    // for bidi algorithm +spec:text-alignment-spacing:97b93a - direction property affects
    // text-align:justify last-line alignment +spec:writing-modes:73aaff - block elements
    // inherit base direction from parent via CSS direction property +spec:writing-modes:8a888b
    // - line box inline base direction from containing block's direction Get the direction
    // property from the CSS cache (defaults to LTR if not set) +spec:display-property:da3b59 -
    // direction property specifies inline base direction for ordering inline-level content
    // +spec:inline-formatting-context:97af40 - direction property sets inline base direction for
    // bidi, text alignment, overflow +spec:writing-modes:2deb38 - bidirectional reordering via
    // CSS direction property +spec:writing-modes:fbb332 - in vertical writing modes,
    // text-orientation:upright forces used direction to ltr
    let direction = match constraints.writing_mode {
        LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr
            if matches!(text_orientation, text3::cache::TextOrientation::Upright) =>
        {
            Some(text3::cache::BidiDirection::Ltr)
        }
        _ => match get_direction_property(styled_dom, id, node_state) {
            MultiValue::Exact(d) => Some(match d {
                StyleDirection::Ltr => text3::cache::BidiDirection::Ltr,
                StyleDirection::Rtl => text3::cache::BidiDirection::Rtl,
            }),
            _ => None,
        },
    };

    // Get unicode-bidi property for bidi algorithm configuration
    // +spec:containing-block:0d4914 - unicode-bidi: plaintext causes P2/P3 heuristics instead of
    // HL1 override
    let unicode_bidi_val = if dom_declared & DOM_HAS_UNICODE_BIDI != 0 {
        match get_unicode_bidi_property(styled_dom, id, node_state) {
            MultiValue::Exact(u) => match u {
                StyleUnicodeBidi::Normal => text3::cache::UnicodeBidi::Normal,
                StyleUnicodeBidi::Embed => text3::cache::UnicodeBidi::Embed,
                StyleUnicodeBidi::Isolate => text3::cache::UnicodeBidi::Isolate,
                StyleUnicodeBidi::BidiOverride => text3::cache::UnicodeBidi::BidiOverride,
                StyleUnicodeBidi::IsolateOverride => text3::cache::UnicodeBidi::IsolateOverride,
                StyleUnicodeBidi::Plaintext => text3::cache::UnicodeBidi::Plaintext,
            },
            _ => text3::cache::UnicodeBidi::Normal,
        }
    } else {
        text3::cache::UnicodeBidi::Normal
    };

    debug_info!(
        ctx,
        "dom_id={:?}, available_size={}x{}, setting available_width={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        constraints.available_size.width
    );

    // +spec:box-model:8113d7 - text-indent treated as margin on start edge of line box
    // +spec:display-contents:5f95ac - text-indent: percentage=0 for intrinsic sizing, each-line and
    // hanging keywords +spec:floats:17c74a - text-indent applied to first line (5em indentation
    // with no floats) +spec:positioning:1e32b1 - text-indent with hanging/each-line keywords
    // resolved and passed to text layout
    // +spec:intrinsic-sizing:0e8625 - percentage text-indent treated as 0 for intrinsic size
    // contributions (`getters::resolve_text_indent`, shared with the intrinsic scan)
    let is_intrinsic_sizing = matches!(
        constraints.available_width_type,
        Text3AvailableSpace::MinContent | Text3AvailableSpace::MaxContent
    );
    let (text_indent, text_indent_each_line, text_indent_hanging) =
        crate::solver3::getters::resolve_text_indent(
            styled_dom,
            id,
            node_state,
            constraints.available_size.width,
            ctx.viewport_size,
            is_intrinsic_sizing,
        );

    // Multi-column: THE reader of the column declarations and THE column
    // resolution (`multicol::column_style` / `ColumnStyle::geometry`, CSS
    // Multicol 1 §3.4), shared with the multi-column block layout. text3
    // splits this inline formatting context's lines over the columns; the
    // gap only matters between columns. The column properties are not
    // inherited, so an anonymous box (laid out inside the multi-column
    // container, `id` being the container's) has none.
    let column_geometry = if anonymous {
        None
    } else {
        crate::solver3::multicol::column_style(styled_dom, id, node_state, ctx.viewport_size)
            .map(|style| style.geometry(constraints.available_size.width))
    };
    let columns = column_geometry.map_or(1, |g| g.count);
    let column_gap = column_geometry.map_or(0.0, |g| g.gap);

    // +spec:line-breaking:b4928e - white-space values mapped to wrap/whitespace processing rules
    // Map white-space CSS property to TextWrap
    let resolved_ws = match get_white_space_property(styled_dom, id, node_state) {
        MultiValue::Exact(ws) => ws,
        _ => StyleWhiteSpace::Normal,
    };
    let text_wrap = match resolved_ws {
        StyleWhiteSpace::Normal
        | StyleWhiteSpace::PreWrap
        | StyleWhiteSpace::PreLine
        | StyleWhiteSpace::BreakSpaces => text3::cache::TextWrap::Wrap,
        StyleWhiteSpace::Nowrap | StyleWhiteSpace::Pre => text3::cache::TextWrap::NoWrap,
    };
    let white_space_mode = match resolved_ws {
        StyleWhiteSpace::Normal => text3::cache::WhiteSpaceMode::Normal,
        StyleWhiteSpace::Nowrap => text3::cache::WhiteSpaceMode::Nowrap,
        StyleWhiteSpace::Pre => text3::cache::WhiteSpaceMode::Pre,
        StyleWhiteSpace::PreWrap => text3::cache::WhiteSpaceMode::PreWrap,
        StyleWhiteSpace::PreLine => text3::cache::WhiteSpaceMode::PreLine,
        StyleWhiteSpace::BreakSpaces => text3::cache::WhiteSpaceMode::BreakSpaces,
    };

    // +spec:block-formatting-context:fd60a8 - initial letter box is in-flow in its BFC, originating
    // line box +spec:block-formatting-context:c5ba02 - initial letter inline flow layout
    // (alignment, white space collapsing) +spec:block-formatting-context:83f8a7 - initial
    // letter wrapping modes (none, all, first) +spec:block-formatting-context:fef28d - initial
    // letter box is in-flow in its BFC, part of originating line box +spec:box-model:c3ce58 -
    // initial letter block-start margin edge must be below containing block content edge
    // +spec:display-contents:568fe2 - initial letter participates in same IFC as its line
    // +spec:display-property:a89adb - initial letter boxes from non-replaced inline boxes and
    // atomic inlines +spec:display-property:4b59ce - initial-letter applies to inline-level
    // boxes at start of first line +spec:display-property:756cad - initial-letter sizing:
    // drop/raise/sunken initial computation +spec:display-property:8b08f4 - initial-letter
    // applied to first inline-level child of block container +spec:display-property:8c1dce -
    // initial-letter property: size/sink for drop caps on inline-level boxes
    // +spec:display-property:b453a3 - initial-letter applies to inline-level boxes in IFC
    // +spec:display-property:b5e149 - initial letters are in-flow inline-level content, not floats
    // +spec:display-property:fa044e - initial-letter applies to first-child inline-level boxes
    // +spec:line-height:306d87 - initial-letter sizing must use containing block's line-height, not
    // spanned lines' heights +spec:writing-modes:903310 - atomic initial letters use normal
    // sizing; only positioning is special Get initial-letter for drop caps
    // +spec:display-property:4c69bf - read initial-letter-align for alignment points
    let initial_letter_align = if dom_declared & DOM_HAS_INITIAL_LETTER_ALIGN != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_initial_letter_align(node_data, &id, node_state)
            .and_then(|s| s.get_property())
            .map_or(text3::cache::InitialLetterAlign::Auto, |a| match a {
                azul_css::props::style::text::StyleInitialLetterAlign::Auto => {
                    text3::cache::InitialLetterAlign::Auto
                }
                azul_css::props::style::text::StyleInitialLetterAlign::Alphabetic => {
                    text3::cache::InitialLetterAlign::Alphabetic
                }
                azul_css::props::style::text::StyleInitialLetterAlign::Hanging => {
                    text3::cache::InitialLetterAlign::Hanging
                }
                azul_css::props::style::text::StyleInitialLetterAlign::Ideographic => {
                    text3::cache::InitialLetterAlign::Ideographic
                }
            })
    } else {
        text3::cache::InitialLetterAlign::Auto
    };
    // +spec:display-property:5af252 - initial-letter on inline-level box not at line start uses
    // normal +spec:text-alignment-spacing:a17609 - sunken initial letters suppress
    // letter-spacing and justification (not word-spacing) with adjacent content
    // +spec:display-property:68ab22 - initial-letter only applies in IFC (inline-level);
    // float!=none or position!=static causes display to compute to block (BFC), so
    // initial-letter naturally does not apply to those elements
    // +spec:writing-modes:c89d19 - initial-letter block-axis positioning: sink determines block
    // offset +spec:display-property:b67500 - initial-letter size/sink: values other than normal
    // make box an initial letter box (inline-level, in-flow) +spec:display-property:416f27 -
    // initial-letter sink defaults to "drop" (sink = size floored) when omitted
    let initial_letter = if dom_declared & DOM_HAS_INITIAL_LETTER != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_initial_letter(node_data, &id, node_state)
            .and_then(|s| s.get_property())
            .map(|il| {
                use std::num::NonZeroUsize;
                let sink = match il.sink {
                    azul_css::corety::OptionU32::Some(s) => s,
                    azul_css::corety::OptionU32::None => il.size, // "drop" assumed: sink = size
                };
                text3::cache::InitialLetter {
                    size: il.size as f32,
                    sink,
                    count: NonZeroUsize::new(1).unwrap(),
                    align: initial_letter_align,
                }
            })
    } else {
        None
    };

    // If initial-letter is set, compute the drop cap exclusion area and add it
    // to the shape exclusions so that text wraps around the enlarged letter.
    // +spec:box-model:d4adf6 - ancestor inline boundaries excluded via geometric exclusion
    // +spec:floats:c5e23f - floats in subsequent lines adjacent to a sunk initial letter must clear
    // it
    if let Some(ref il) = initial_letter {
        let (letter_w, letter_h) = layout_initial_letter(
            il.size,
            il.sink,
            constraints.available_size.width,
            line_height_px,
        );
        if letter_w > 0.0 && letter_h > 0.0 {
            // Place the exclusion at the inline-start (x=0, y=0 relative to the IFC).
            // This creates a rectangular exclusion that text flows around.
            shape_exclusions.push(ShapeBoundary::Rectangle(text3::cache::Rect {
                x: 0.0,
                y: 0.0,
                width: letter_w,
                height: letter_h,
            }));
        }
    }

    // Get line-clamp for limiting visible lines
    let line_clamp = if dom_declared & DOM_HAS_LINE_CLAMP != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_line_clamp(node_data, &id, node_state)
            .and_then(|s| s.get_property())
            .and_then(|lc| std::num::NonZeroUsize::new(lc.max_lines))
    } else {
        None
    };

    // Get hanging-punctuation for hanging punctuation marks
    let hanging_punctuation = if dom_declared & DOM_HAS_HANGING_PUNCTUATION != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_hanging_punctuation(node_data, &id, node_state)
            .and_then(|s| s.get_property())
            .is_some_and(azul_css::props::style::StyleHangingPunctuation::is_enabled)
    } else {
        false
    };

    // Get text-combine-upright for vertical text combination
    // +spec:line-breaking:9f150a - text-combine-upright:all composes glyphs horizontally, ignoring
    // letter-spacing and forced line breaks +spec:line-breaking:1b88cd -
    // text-combine-upright:all layout: inline-block with 1em square, ignoring forced line breaks
    // +spec:inline-formatting-context:c8d8d9 - text-combine-upright compression passed to text
    // shaping engine +spec:inline-formatting-context:f4ef7d - text-combine-upright layout rules
    // (1em square composition)
    let text_combine_upright = if dom_declared & DOM_HAS_TEXT_COMBINE_UPRIGHT != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_text_combine_upright(node_data, &id, node_state)
            .and_then(|s| s.get_property())
            // +spec:display-property:6f174d - text-combine-upright horizontal-in-vertical composition
            .map(|tcu| match tcu {
                StyleTextCombineUpright::None => text3::cache::TextCombineUpright::None,
                StyleTextCombineUpright::All => text3::cache::TextCombineUpright::All,
                StyleTextCombineUpright::Digits(n) => text3::cache::TextCombineUpright::Digits(*n),
            })
    } else {
        None
    };

    // Get exclusion-margin (CSS Exclusions L1) and shape-margin (CSS Shapes L1)
    // for shape exclusions. We sum both into a single margin knob — strictly,
    // they apply to different sources (exclusion-margin → CSS Exclusions,
    // shape-margin → shape-outside), but the layout solver currently keeps
    // a single per-IFC margin value, so the two get added.
    let exclusion_margin_base = if dom_declared & DOM_HAS_EXCLUSION_MARGIN != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_exclusion_margin(node_data, &id, node_state)
            .and_then(|s| s.get_property())
            .map_or(0.0, |em| em.inner.get())
    } else {
        0.0
    };

    let shape_margin = if dom_declared & DOM_HAS_SHAPE_MARGIN != 0 {
        styled_dom
            .css_property_cache
            .ptr
            .get_shape_margin(node_data, &id, node_state)
            .and_then(|s| s.get_property())
            .map_or(0.0, |sm| sm.inner.number.get())
    } else {
        0.0
    };

    let exclusion_margin = exclusion_margin_base + shape_margin;

    // The hyphenation language (CSS Text 3 5.4): azul's own
    // `-azul-hyphenation-language` where it is set - an override - else the
    // CONTENT LANGUAGE, the nearest `lang` attribute. It was the property
    // alone: `<div lang="en" style="hyphens: auto">` was never hyphenated
    // (pdfocr engine issue 3). Read only under `hyphens: auto`, the one use
    // of it (text3's hyphenator), so a document that never asks for
    // automatic hyphenation walks no ancestors and reads no property.
    let hyphenation_language = if hyphenation == StyleHyphens::Auto {
        let from_property = if dom_declared & DOM_HAS_HYPHENATION_LANGUAGE != 0 {
            styled_dom
                .css_property_cache
                .ptr
                .get_hyphenation_language(node_data, &id, node_state)
                .and_then(|s| s.get_property())
                .map(|hl| hyphenation_language_of_tag(hl.inner.as_str()))
        } else {
            None
        };
        from_property.unwrap_or_else(|| {
            content_language(styled_dom, id).and_then(hyphenation_language_of_tag)
        })
    } else {
        None
    };

    UnifiedConstraints {
        exclusion_margin,
        hyphenation_language,
        text_indent,
        text_indent_each_line,
        text_indent_hanging,
        initial_letter,
        line_clamp,
        columns,
        column_gap,
        // One piece of a multi-column block container's flow - unless this
        // context has columns of its own, which split it instead.
        column_flow: if columns == 1 {
            constraints.column_flow.clone()
        } else {
            None
        },
        hanging_punctuation,
        text_wrap,
        white_space_mode,
        text_combine_upright,
        segment_alignment: SegmentAlignment::Total,
        overflow: match overflow_behaviour {
            LayoutOverflow::Visible => text3::cache::OverflowBehavior::Visible,
            LayoutOverflow::Hidden | LayoutOverflow::Clip => text3::cache::OverflowBehavior::Hidden,
            LayoutOverflow::Scroll => text3::cache::OverflowBehavior::Scroll,
            LayoutOverflow::Auto => text3::cache::OverflowBehavior::Auto,
        },
        // Use the semantic available_width_type directly instead of converting from float.
        // This preserves MinContent/MaxContent semantics for intrinsic sizing.
        available_width: constraints.available_width_type,
        // Height only constrains line PRODUCTION where it is semantically
        // load-bearing: multi-column balancing ("column full → next column").
        // Everywhere else the continuous IFC lays out ALL its lines and the
        // true content height flows into overflow_size — in CSS, inline
        // content is never truncated by available height at layout time
        // (overflow is a paint/scroll concern, and nothing consumes
        // text3's remaining_items on this path, so truncated lines were
        // silently LOST). The old `Some(available_size.height)` arm let any
        // 0-height measure pass produce a ZERO-LINE layout that the
        // width+content-keyed caches then served to the real pass — the
        // miniword estimator measured multi-line paragraphs as 0.0 px
        // depending on one leading whitespace character shifting which
        // pass primed the cache.
        available_height: if columns > 1 {
            Some(constraints.available_size.height)
        } else {
            None
        },
        shape_boundaries, // CSS shape-inside: text flows within shape
        shape_exclusions, // CSS shape-outside + floats: text wraps around shapes
        writing_mode: Some(match writing_mode {
            LayoutWritingMode::HorizontalTb => text3::cache::WritingMode::HorizontalTb,
            LayoutWritingMode::VerticalRl => text3::cache::WritingMode::VerticalRl,
            LayoutWritingMode::VerticalLr => text3::cache::WritingMode::VerticalLr,
        }),
        direction, // Use the CSS direction property (currently defaulting to LTR)
        unicode_bidi: unicode_bidi_val,
        // +spec:overflow:7ff7d1 - hyphens property: none/manual/auto hyphenation control
        hyphenation: match hyphenation {
            StyleHyphens::None => text3::cache::Hyphens::None,
            StyleHyphens::Manual => text3::cache::Hyphens::Manual,
            StyleHyphens::Auto => text3::cache::Hyphens::Auto,
        },
        text_orientation,
        // +spec:text-alignment-spacing:6cb965 - text-align shorthand sets text-align-all (mapped
        // here from computed value) +spec:text-alignment-spacing:838967 - map text-align
        // values (start/end/left/right/center/justify) to inline alignment
        // +spec:text-alignment-spacing:d9ea45 - property index: text-align, text-justify,
        // letter-spacing mapped to layout +spec:text-alignment-spacing:600fda - text-align
        // values (left/right/center/justify) mapped per CSS Text §6.1
        text_align: match text_align {
            StyleTextAlign::Start => text3::cache::TextAlign::Start,
            StyleTextAlign::End => text3::cache::TextAlign::End,
            StyleTextAlign::Left => text3::cache::TextAlign::Left,
            StyleTextAlign::Right => text3::cache::TextAlign::Right,
            StyleTextAlign::Center => text3::cache::TextAlign::Center,
            StyleTextAlign::Justify => text3::cache::TextAlign::Justify,
        },
        // +spec:text-alignment-spacing:0ea31d - text-justify inter-word/inter-character/distribute
        // mapped per §6.4 +spec:text-alignment-spacing:01244f - text-justify: none disables
        // justification, auto uses inter-word as universal default
        text_justify: match text_justify {
            LayoutTextJustify::None => text3::cache::JustifyContent::None,
            LayoutTextJustify::Auto | LayoutTextJustify::InterWord => {
                text3::cache::JustifyContent::InterWord
            }
            // distribute computes to inter-character
            LayoutTextJustify::InterCharacter | LayoutTextJustify::Distribute => {
                text3::cache::JustifyContent::InterCharacter
            }
        },
        // +spec:line-height:79f3aa - line-height resolved: `normal` uses the font's real
        // metrics (ascent - descent + line_gap), <number>/<percentage> × font-size.
        // When line-height is NOT declared the computed value is `normal`: the
        // ROOT's (the strut's) is its first available face's rounded A + D +
        // gap (`root_line_height` above); each run still resolves its own
        // `normal` against its own glyphs' faces (CoreText/Chrome parity)
        // instead of a synthetic 1.2 ratio. The value is the root style's,
        // the one its runs carry (see `root_style`).
        line_height: root_line_height,
        // The strut's ascent, descent, x-height and cap height: the
        // container's first available font's (see `strut_ascent` above; the
        // x-height falls back to 0.5em per CSS Inline 3 Appendix A, the cap
        // height to the typical Latin 0.7em - Appendix A.2's formal fallback,
        // the ascent, would make cap-edge trimming a no-op).
        // TODO(superplan): `ch_width` from `get_space_width` / the "0" glyph.
        strut_ascent,
        strut_descent,
        strut_x_height,
        strut_cap_height,
        // The parent font size `vertical-align: sub` / `super` shift by
        // (text3 `baseline_shift`): the container's, as its runs carry it.
        strut_font_size: root_style.font_size_px,
        ch_width: font_size * 0.5,
        vertical_align,
        // +spec:inline-formatting-context:48ce44 - overflow-wrap property: break at otherwise
        // disallowed points to prevent overflow +spec:line-breaking:bbb5f7 - overflow-wrap:
        // anywhere vs break-word distinction for min-content
        overflow_wrap: if word_break_css == StyleWordBreak::BreakWord {
            // +spec:line-breaking:815882 - break-word forces overflow-wrap: anywhere
            text3::cache::OverflowWrap::Anywhere
        } else {
            match overflow_wrap_css {
                StyleOverflowWrap::Normal => text3::cache::OverflowWrap::Normal,
                StyleOverflowWrap::Anywhere => text3::cache::OverflowWrap::Anywhere,
                StyleOverflowWrap::BreakWord => text3::cache::OverflowWrap::BreakWord,
            }
        },
        text_align_last: match text_align_last_css {
            StyleTextAlignLast::Auto => text3::cache::TextAlign::default(),
            StyleTextAlignLast::Start => text3::cache::TextAlign::Start,
            StyleTextAlignLast::End => text3::cache::TextAlign::End,
            StyleTextAlignLast::Left => text3::cache::TextAlign::Left,
            StyleTextAlignLast::Right => text3::cache::TextAlign::Right,
            StyleTextAlignLast::Center => text3::cache::TextAlign::Center,
            StyleTextAlignLast::Justify => text3::cache::TextAlign::Justify,
        },
        // +spec:line-breaking:815882 - word-break: break-word => normal + overflow-wrap: anywhere
        word_break: match word_break_css {
            StyleWordBreak::Normal | StyleWordBreak::BreakWord => text3::cache::WordBreak::Normal,
            StyleWordBreak::BreakAll => text3::cache::WordBreak::BreakAll,
            StyleWordBreak::KeepAll => text3::cache::WordBreak::KeepAll,
        },
        // +spec:white-space-processing:bc5f7b - line-break with break-spaces allows breaking before
        // first space CSS Text Level 3 §5.3: The line-break property affects preserved
        // white space behavior:
        // - normal/pre-line: preserved white space at end/start of line is discarded
        // - nowrap/pre: wrapping is forbidden altogether
        // - pre-wrap: preserved white space hangs
        // - break-spaces: allows breaking before first space of a sequence
        // break-spaces allows wrapping preserved spaces to next line; for other white-space values,
        // preserved spaces at line ends are either discarded (normal, pre-line), wrapping is
        // forbidden (nowrap, pre), or they hang (pre-wrap).
        line_break: match line_break_css {
            StyleLineBreak::Auto => text3::cache::LineBreakStrictness::Auto,
            StyleLineBreak::Loose => text3::cache::LineBreakStrictness::Loose,
            StyleLineBreak::Normal => text3::cache::LineBreakStrictness::Normal,
            StyleLineBreak::Strict => text3::cache::LineBreakStrictness::Strict,
            StyleLineBreak::Anywhere => text3::cache::LineBreakStrictness::Anywhere,
        },
    }
}
