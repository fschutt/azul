//! Declaration builders shared by the theme modules.
//!
//! A theme function is a list of decisions - this surface, that ink, a
//! hairline here, a ring on focus - and every one of them is a handful of
//! nested constructors when spelled out (`CssPropertyWithConditions::simple(
//! CssProperty::const_border_top_color(StyleBorderTopColor { inner: .. }))`,
//! four times, then again for the dark twin). These helpers are those
//! shapes, named, so the flat and flora sections read as the decisions they
//! make.
//!
//! Two rules they keep so a caller cannot get them wrong:
//!
//! * A dark twin always comes RIGHT AFTER its light value, in the same state
//!   (`widgets::theme_pairs` rejects anything else): every `themed_*` helper
//!   returns `[light, dark]` pairs in that order, edge by edge.
//! * A focus ring is either a border colour change ([`focus_ring`], only
//!   visible on a node that HAS a border) or a halo ([`focus_halo`], a spread
//!   shadow, for a node without one - it moves nothing).
//!
//! One name per meaning (this module absorbed `style_kit`'s twins,
//! `DEDUP_WIDGETS_API` F4): [`fill`] is a background paint (`themed_fill`,
//! `hover_fill`, `active_fill` its states), [`fill_box`] is `width/height:
//! 100%`. A shadow helper names its slot: the plain ones ([`shadow`],
//! [`themed_shadow`], [`focus_halo`], [`focus_halo_inset`]) share the bottom
//! slot, so a `:focus` halo REPLACES a resting shadow; the `_stacked` focus
//! halos sit in the left slot and [`themed_inset_shadow`] in the top one, so
//! they ADD to it.

use alloc::vec::Vec;

use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::{
            color::ColorU, pixel::PixelValue, pixel::PixelValueNoPercent, Direction,
            DirectionCorner, DirectionCorners, PercentageValue, StyleFontSize, StyleFontWeight,
        },
        layout::{
            LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight, LayoutMarginTop,
            LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop,
        },
        property::CssProperty,
        style::{
            BorderStyle, BoxShadowClipMode, ExtendMode, LayoutBorderBottomWidth, LayoutBorderLeftWidth,
            LayoutBorderRightWidth, LayoutBorderTopWidth, StyleBackgroundContent,
            StyleBackgroundContentVec, StyleBorderBottomColor, StyleBorderBottomLeftRadius,
            StyleBorderBottomRightRadius, StyleBorderBottomStyle, StyleBorderLeftColor,
            StyleBorderLeftStyle, StyleBorderRightColor, StyleBorderRightStyle,
            StyleBorderTopColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
            StyleBorderTopStyle, StyleBoxShadow, StyleLetterSpacing, StyleTextColor,
            StyleTextDecoration, LinearGradient, NormalizedLinearColorStop,
            NormalizedLinearColorStopVec,
        },
    },
};

/// `background: <color>`, one solid layer.
#[must_use]
pub(crate) fn fill(color: ColorU) -> CssProperty {
    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(alloc::vec![
        StyleBackgroundContent::Color(color),
    ]))
}

/// `background` built from layers, painted first to last (the base colour
/// goes FIRST - the reverse of a CSS comma list).
#[must_use]
pub(crate) fn layers(list: Vec<StyleBackgroundContent>) -> CssProperty {
    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(list))
}

/// `color: <color>`.
#[must_use]
pub(crate) const fn ink(color: ColorU) -> CssProperty {
    CssProperty::const_text_color(StyleTextColor { inner: color })
}

/// A resting surface with its dark twin: `[simple(light), dark_theme(dark)]`.
#[must_use]
pub(crate) fn themed_fill(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(fill(light), fill(dark))
}

/// A resting ink with its dark twin (`const`: usable in a `static` base).
#[must_use]
pub(crate) const fn themed_ink(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(ink(light)),
        CssPropertyWithConditions::dark_mode(ink(dark)),
    ]
}

/// A layered resting surface (a gradient face, a stone) with its dark twin.
#[must_use]
pub(crate) fn themed_layers(
    light: Vec<StyleBackgroundContent>,
    dark: Vec<StyleBackgroundContent>,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(layers(light), layers(dark))
}

/// The four border-colour properties, top / right / bottom / left.
#[must_use]
pub(crate) const fn border_colors(color: ColorU) -> [CssProperty; 4] {
    [
        CssProperty::const_border_top_color(StyleBorderTopColor { inner: color }),
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: color }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: color }),
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner: color }),
    ]
}

/// `border: <width>px solid` on all four edges, no colour (pair it with
/// [`themed_border_color`]).
#[must_use]
pub(crate) const fn border(width_px: isize) -> [CssPropertyWithConditions; 8] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(width_px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
            LayoutBorderRightWidth::const_px(width_px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
            LayoutBorderBottomWidth::const_px(width_px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
            LayoutBorderLeftWidth::const_px(width_px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
            StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]
}

/// One solid edge: `border-<side>: <width>px solid`, no colour.
#[must_use]
pub(crate) const fn border_bottom(width_px: isize) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
            LayoutBorderBottomWidth::const_px(width_px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
            StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]
}

/// See [`border_bottom`].
#[must_use]
pub(crate) const fn border_left(width_px: isize) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
            LayoutBorderLeftWidth::const_px(width_px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]
}

/// All four border colours with their dark twins, each twin right after its
/// light value.
#[must_use]
pub(crate) fn themed_border_color(light: ColorU, dark: ColorU) -> Vec<CssPropertyWithConditions> {
    let mut out = Vec::with_capacity(8);
    for (l, d) in border_colors(light).into_iter().zip(border_colors(dark)) {
        out.extend(CssPropertyWithConditions::themed(l, d));
    }
    out
}

/// One edge's colour with its dark twin.
#[must_use]
pub(crate) fn themed_border_bottom_color(
    light: ColorU,
    dark: ColorU,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: light }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: dark }),
    )
}

/// See [`themed_border_bottom_color`].
#[must_use]
pub(crate) fn themed_border_top_color(
    light: ColorU,
    dark: ColorU,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(
        CssProperty::const_border_top_color(StyleBorderTopColor { inner: light }),
        CssProperty::const_border_top_color(StyleBorderTopColor { inner: dark }),
    )
}

/// See [`themed_border_bottom_color`].
#[must_use]
pub(crate) fn themed_border_left_color(
    light: ColorU,
    dark: ColorU,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light }),
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner: dark }),
    )
}

/// See [`themed_border_bottom_color`].
#[must_use]
pub(crate) fn themed_border_right_color(
    light: ColorU,
    dark: ColorU,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: light }),
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: dark }),
    )
}

/// The focus ring: all four border colours on `:focus`, each with its dark
/// twin right after it. Only visible on a node with a border (see the
/// module note) - pair it with [`ring_slot`] or the part's own border.
#[must_use]
pub(crate) const fn focus_ring(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 8] {
    type C = CssPropertyWithConditions;
    [
        C::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor { inner: light })),
        C::dark_on_focus(CssProperty::const_border_top_color(StyleBorderTopColor { inner: dark })),
        C::on_focus(CssProperty::const_border_right_color(StyleBorderRightColor { inner: light })),
        C::dark_on_focus(CssProperty::const_border_right_color(StyleBorderRightColor {
            inner: dark,
        })),
        C::on_focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: light,
        })),
        C::dark_on_focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: dark,
        })),
        C::on_focus(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light })),
        C::dark_on_focus(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: dark })),
    ]
}

/// All four border colours on `:hover`, each with its dark twin right after it.
#[must_use]
pub(crate) const fn hover_border_color(
    light: ColorU,
    dark: ColorU,
) -> [CssPropertyWithConditions; 8] {
    type C = CssPropertyWithConditions;
    [
        C::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor { inner: light })),
        C::dark_on_hover(CssProperty::const_border_top_color(StyleBorderTopColor { inner: dark })),
        C::on_hover(CssProperty::const_border_right_color(StyleBorderRightColor { inner: light })),
        C::dark_on_hover(CssProperty::const_border_right_color(StyleBorderRightColor {
            inner: dark,
        })),
        C::on_hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: light,
        })),
        C::dark_on_hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: dark,
        })),
        C::on_hover(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light })),
        C::dark_on_hover(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: dark })),
    ]
}

/// A surface on `:hover`, with its dark twin.
#[must_use]
pub(crate) fn hover_fill(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed_on_hover(fill(light), fill(dark))
}

/// A layered surface on `:hover`, with its dark twin.
#[must_use]
pub(crate) fn hover_layers(
    light: Vec<StyleBackgroundContent>,
    dark: Vec<StyleBackgroundContent>,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed_on_hover(layers(light), layers(dark))
}

/// A surface while pressed, with its dark twin.
#[must_use]
pub(crate) fn active_fill(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed_on_active(fill(light), fill(dark))
}

/// A layered surface while pressed, with its dark twin.
#[must_use]
pub(crate) fn active_layers(
    light: Vec<StyleBackgroundContent>,
    dark: Vec<StyleBackgroundContent>,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed_on_active(layers(light), layers(dark))
}

/// An ink on `:hover`, with its dark twin.
#[must_use]
pub(crate) const fn hover_ink(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::on_hover(ink(light)),
        CssPropertyWithConditions::dark_on_hover(ink(dark)),
    ]
}

/// [`focus_halo`] drawn INSIDE the node: for a node whose parent clips
/// (`overflow: hidden`), where an outer halo would be cut off at the edge -
/// an accordion header inside its rounded panel, a menu item in its bar.
#[must_use]
pub(crate) fn focus_halo_inset(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed_on_focus(
        shadow(0, 0, 2, light, true),
        shadow(0, 0, 2, dark, true),
    )
}

/// A link's underline under the pointer. An underline has no colour of its
/// own, so the dark twin repeats it - emitted anyway so every state rule has
/// its twin (the rule `widgets::theme_pairs` checks).
#[must_use]
pub(crate) fn hover_underline() -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed_on_hover(
        CssProperty::text_decoration(StyleTextDecoration::Underline),
        CssProperty::text_decoration(StyleTextDecoration::Underline),
    )
}

/// `border-radius: <px>` on all four corners.
#[must_use]
pub(crate) const fn radius(px: isize) -> [CssPropertyWithConditions; 4] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(px),
        )),
    ]
}

/// `padding: <top> <right> <bottom> <left>`, in px.
#[must_use]
pub(crate) const fn padding(
    top: isize,
    right: isize,
    bottom: isize,
    left: isize,
) -> [CssPropertyWithConditions; 4] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(top),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(right),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(bottom),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(left),
        )),
    ]
}

/// `margin: <top> <right> <bottom> <left>`, in px.
#[must_use]
pub(crate) const fn margin(
    top: isize,
    right: isize,
    bottom: isize,
    left: isize,
) -> [CssPropertyWithConditions; 4] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_margin_top(
            LayoutMarginTop::const_px(top),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_margin_right(
            LayoutMarginRight::const_px(right),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_margin_bottom(
            LayoutMarginBottom::const_px(bottom),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_margin_left(
            LayoutMarginLeft::const_px(left),
        )),
    ]
}

/// `box-shadow: 0 <offset_y>px <blur>px <spread>px <color> [inset]`, as ONE
/// declaration: the bottom one of a node's four shadow slots.
///
/// azul keeps a node's shadows in four slots (`-azul-box-shadow-left/right/
/// top/bottom`) and paints each DISTINCT one once (`getters::get_box_shadows`),
/// so this one slot paints exactly what the `box-shadow` shorthand (the same
/// shadow in all four) paints. Every shadow of this module sits in the same
/// slot, so a `:focus` halo replaces the resting shadow, as a CSS `:focus {
/// box-shadow: .. }` does.
#[must_use]
pub(crate) fn shadow(
    offset_y: isize,
    blur: isize,
    spread: isize,
    color: ColorU,
    inset: bool,
) -> CssProperty {
    CssProperty::box_shadow_bottom(box_shadow(offset_y, blur, spread, color, inset))
}

/// The one `StyleBoxShadow` the shadow helpers build, whatever slot carries
/// it: `0 <offset_y>px <blur>px <spread>px <color> [inset]`.
const fn box_shadow(
    offset_y: isize,
    blur: isize,
    spread: isize,
    color: ColorU,
    inset: bool,
) -> StyleBoxShadow {
    StyleBoxShadow {
        offset_x: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        offset_y: PixelValueNoPercent {
            inner: PixelValue::const_px(offset_y),
        },
        blur_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(blur),
        },
        spread_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(spread),
        },
        clip_mode: if inset {
            BoxShadowClipMode::Inset
        } else {
            BoxShadowClipMode::Outset
        },
        color,
    }
}

/// A drop shadow with its dark twin (flora's shadows are warm in light mode
/// and near-black at night).
#[must_use]
pub(crate) fn themed_shadow(
    offset_y: isize,
    blur: isize,
    light: ColorU,
    dark: ColorU,
) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(
        shadow(offset_y, blur, 0, light, false),
        shadow(offset_y, blur, 0, dark, false),
    )
}

/// The focus ring for a node WITHOUT a border: a 2px halo on `:focus` (a
/// spread shadow, so it follows the corner radius and moves nothing), with
/// its dark twin. The keyboard-only `outline: 2px solid var(--focus-color)`
/// of flora.css, in the one shape azul can draw it.
#[must_use]
pub(crate) fn focus_halo(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed_on_focus(
        shadow(0, 0, 2, light, false),
        shadow(0, 0, 2, dark, false),
    )
}

/// `font-weight: bold`.
#[must_use]
pub(crate) const fn bold() -> CssPropertyWithConditions {
    weight(StyleFontWeight::Bold)
}

/// `font-weight: 600`.
#[must_use]
pub(crate) const fn semibold() -> CssPropertyWithConditions {
    weight(StyleFontWeight::W600)
}

/// `font-weight: <w>`.
#[must_use]
pub(crate) const fn weight(w: StyleFontWeight) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::font_weight(w))
}

/// `font-size: <px>px`.
#[must_use]
pub(crate) const fn font_size(px: isize) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(px)))
}

/// `letter-spacing: <em>em` - flora tracks its small labels out.
#[must_use]
pub(crate) fn letter_spacing_em(em: f32) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::const_letter_spacing(StyleLetterSpacing {
        inner: PixelValue::em(em),
    }))
}

// ==== R5-A: a part's base, then its skin ====

/// A widget part's declarations: its BASE - the structure, the same in every
/// theme, declared once in the widget's own file - then a theme's SKIN (its
/// paint and metrics). Base first: the structure both looks share then leads
/// both orders, and the merge (`theme_blocks`) declares it once, outside
/// every `@theme` block, so it also holds under a theme no widget knows.
///
/// For the parts of ONE list, before the merge. Stacking two parts a merge
/// already went through (a base part and a state part) is
/// `theme_blocks::stack_parts`'s job.
#[must_use]
pub(crate) fn on_base(
    base: &[CssPropertyWithConditions],
    skin: &[CssPropertyWithConditions],
) -> Vec<CssPropertyWithConditions> {
    let mut part = Vec::with_capacity(base.len() + skin.len());
    part.extend_from_slice(base);
    part.extend_from_slice(skin);
    part
}

// ==== mail widgets: a top rule ====

/// `border-top: <width>px solid`, no colour (pair it with
/// [`themed_border_top_color`]): the hairline over a footer or a button
/// row - [`border_bottom`]'s twin for the other edge.
#[must_use]
pub(crate) const fn border_top(width_px: isize) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(width_px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]
}

// ==== cell_grid: a right rule ====

/// `border-right: <width>px solid`, no colour (pair it with
/// [`themed_border_right_color`]): [`border_left`]'s twin for the other
/// edge - a grid line between two cells, a header's divider.
#[must_use]
pub(crate) const fn border_right(width_px: isize) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
            LayoutBorderRightWidth::const_px(width_px),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]
}

// ==== layout: one-property declarations (DEDUP_WIDGETS_API F4) ====
//
// The single structural declarations a widget's BASE parts are built from.
// timeline.rs, cell_grid.rs, dialog_kit.rs and shells/mod.rs each carried
// private copies of these; they are the shared ones now.

use azul_css::props::{
    basic::length::FloatValue,
    layout::{
        LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink, LayoutHeight,
        LayoutInsetBottom, LayoutLeft, LayoutMinWidth, LayoutOverflow, LayoutPosition, LayoutTop,
        LayoutWidth,
    },
    property::StyleWhiteSpaceValue,
    style::StyleWhiteSpace,
};

/// An unconditional declaration - `CssPropertyWithConditions::simple`, for
/// the `static` base lists that are written as one call per property.
#[must_use]
pub(crate) const fn simple(property: CssProperty) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(property)
}

/// `display: flex`.
#[must_use]
pub(crate) const fn display_flex() -> CssPropertyWithConditions {
    simple(CssProperty::const_display(LayoutDisplay::Flex))
}

/// `flex-direction: <direction>`.
#[must_use]
pub(crate) const fn flex_direction(direction: LayoutFlexDirection) -> CssPropertyWithConditions {
    simple(CssProperty::const_flex_direction(direction))
}

/// `flex-grow: <n>`.
#[must_use]
pub(crate) const fn grow(n: isize) -> CssPropertyWithConditions {
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(n)))
}

/// `flex-shrink: 0`.
#[must_use]
pub(crate) const fn no_shrink() -> CssPropertyWithConditions {
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    }))
}

/// `position: <position>`.
#[must_use]
pub(crate) const fn position(position: LayoutPosition) -> CssPropertyWithConditions {
    simple(CssProperty::const_position(position))
}

/// `overflow-x: hidden`.
#[must_use]
pub(crate) const fn overflow_x_hidden() -> CssPropertyWithConditions {
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden))
}

/// `overflow-y: hidden`.
#[must_use]
pub(crate) const fn overflow_y_hidden() -> CssPropertyWithConditions {
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden))
}

/// `white-space: nowrap`.
#[must_use]
pub(crate) const fn nowrap() -> CssPropertyWithConditions {
    simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Nowrap,
    )))
}

/// `width: <px>px`, fractional.
#[must_use]
pub(crate) fn px_width(px: f32) -> CssPropertyWithConditions {
    simple(CssProperty::const_width(LayoutWidth::px(px)))
}

/// `height: <px>px`, fractional.
#[must_use]
pub(crate) fn px_height(px: f32) -> CssPropertyWithConditions {
    simple(CssProperty::const_height(LayoutHeight::px(px)))
}

/// `min-width: <px>px`, fractional.
#[must_use]
pub(crate) fn px_min_width(px: f32) -> CssPropertyWithConditions {
    simple(CssProperty::const_min_width(LayoutMinWidth::px(px)))
}

/// `left: <px>px`, fractional.
#[must_use]
pub(crate) fn px_left(px: f32) -> CssPropertyWithConditions {
    simple(CssProperty::const_left(LayoutLeft::px(px)))
}

/// `top: <px>px`, fractional.
#[must_use]
pub(crate) fn px_top(px: f32) -> CssPropertyWithConditions {
    simple(CssProperty::const_top(LayoutTop::px(px)))
}

/// `bottom: <px>px`, fractional.
#[must_use]
pub(crate) fn px_bottom(px: f32) -> CssPropertyWithConditions {
    simple(CssProperty::const_bottom(LayoutInsetBottom::px(px)))
}

// ==== from style_kit: the builders that had no twin here (DEDUP_WIDGETS_API F4) ====

/// `border-radius` per corner: top-left, top-right, bottom-right, bottom-left.
#[must_use]
pub(crate) const fn radius_corners(
    tl: isize,
    tr: isize,
    br: isize,
    bl: isize,
) -> [CssPropertyWithConditions; 4] {
    [
        simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(tl),
        )),
        simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(tr),
        )),
        simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(br),
        )),
        simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(bl),
        )),
    ]
}

/// `width: 100%; height: 100%` - a part that fills its box. Not a paint:
/// a background is [`fill`].
#[must_use]
pub(crate) fn fill_box() -> [CssPropertyWithConditions; 2] {
    [
        simple(CssProperty::const_width(LayoutWidth::Px(PixelValue::percent(100.0)))),
        simple(CssProperty::const_height(LayoutHeight::Px(PixelValue::percent(100.0)))),
    ]
}

/// Which edges [`themed_border`] draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Edges {
    pub(crate) top: bool,
    pub(crate) right: bool,
    pub(crate) bottom: bool,
    pub(crate) left: bool,
}

impl Edges {
    /// All four edges.
    pub(crate) const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };
    /// The bottom edge only (a rule under a heading).
    pub(crate) const BOTTOM: Self = Self {
        top: false,
        right: false,
        bottom: true,
        left: false,
    };
    /// The top edge only (a rule over a footer).
    pub(crate) const TOP: Self = Self {
        top: true,
        right: false,
        bottom: false,
        left: false,
    };
}

/// A solid border of `width` px on `edges`, colour included: per edge its
/// width, its style, the light colour and - right after it - the colour's
/// dark twin. Without a colour it is [`border`] (+ [`themed_border_color`]).
#[must_use]
pub(crate) fn themed_border(
    edges: Edges,
    width: isize,
    light: ColorU,
    dark: ColorU,
) -> Vec<CssPropertyWithConditions> {
    type P = CssPropertyWithConditions;
    let solid = BorderStyle::Solid;
    let mut v = Vec::with_capacity(16);
    if edges.top {
        v.push(P::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(width),
        )));
        v.push(P::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
            inner: solid,
        })));
        v.push(P::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: light,
        })));
        v.push(P::dark_mode(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: dark,
        })));
    }
    if edges.right {
        v.push(P::simple(CssProperty::const_border_right_width(
            LayoutBorderRightWidth::const_px(width),
        )));
        v.push(P::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle { inner: solid },
        )));
        v.push(P::simple(CssProperty::const_border_right_color(
            StyleBorderRightColor { inner: light },
        )));
        v.push(P::dark_mode(CssProperty::const_border_right_color(
            StyleBorderRightColor { inner: dark },
        )));
    }
    if edges.bottom {
        v.push(P::simple(CssProperty::const_border_bottom_width(
            LayoutBorderBottomWidth::const_px(width),
        )));
        v.push(P::simple(CssProperty::const_border_bottom_style(
            StyleBorderBottomStyle { inner: solid },
        )));
        v.push(P::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: light },
        )));
        v.push(P::dark_mode(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: dark },
        )));
    }
    if edges.left {
        v.push(P::simple(CssProperty::const_border_left_width(
            LayoutBorderLeftWidth::const_px(width),
        )));
        v.push(P::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
            inner: solid,
        })));
        v.push(P::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
            inner: light,
        })));
        v.push(P::dark_mode(CssProperty::const_border_left_color(
            StyleBorderLeftColor { inner: dark },
        )));
    }
    v
}

/// A 1px border that is invisible at rest in both modes: the box a
/// [`focus_ring`] colours in, so the ring costs no layout when it appears.
#[must_use]
pub(crate) fn ring_slot() -> Vec<CssPropertyWithConditions> {
    themed_border(Edges::ALL, 1, ColorU::TRANSPARENT, ColorU::TRANSPARENT)
}

/// An inset shadow along the top edge (a sunken well), light then dark. In
/// the TOP slot, so it adds to a resting [`themed_shadow`] (bottom slot).
#[must_use]
pub(crate) fn themed_inset_shadow(
    offset_y: isize,
    blur: isize,
    light: ColorU,
    dark: ColorU,
) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::simple(CssProperty::box_shadow_top(box_shadow(
            offset_y, blur, 0, light, true,
        ))),
        CssPropertyWithConditions::dark_mode(CssProperty::box_shadow_top(box_shadow(
            offset_y, blur, 0, dark, true,
        ))),
    ]
}

/// [`focus_halo_inset`] in the LEFT shadow slot: the focus ring of an item
/// in a joined bar (a pagination button, a segment) whose inner items share
/// their side borders, so a border ring would miss an edge. In its own slot
/// it ADDS to a resting [`themed_shadow`] / [`themed_inset_shadow`] instead
/// of replacing it.
#[must_use]
pub(crate) fn focus_halo_inset_stacked(
    light: ColorU,
    dark: ColorU,
) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::on_focus(CssProperty::box_shadow_left(box_shadow(
            0, 0, 2, light, true,
        ))),
        CssPropertyWithConditions::dark_on_focus(CssProperty::box_shadow_left(box_shadow(
            0, 0, 2, dark, true,
        ))),
    ]
}

/// [`focus_halo`] in the LEFT shadow slot: the focus ring of a bare glyph
/// button (a "x" with no box of its own to ring and no border to colour),
/// a 2px halo just outside its box on `:focus`, declared for the state only.
/// In its own slot it ADDS to a resting shadow instead of replacing it.
#[must_use]
pub(crate) fn focus_halo_stacked(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::on_focus(CssProperty::box_shadow_left(box_shadow(
            0, 0, 2, light, false,
        ))),
        CssPropertyWithConditions::dark_on_focus(CssProperty::box_shadow_left(box_shadow(
            0, 0, 2, dark, false,
        ))),
    ]
}

/// A two-stop vertical gradient face (`linear-gradient(top, bottom)`), one
/// layer for [`layers`] / [`themed_layers`].
#[must_use]
pub(crate) fn face(top: ColorU, bottom: ColorU) -> StyleBackgroundContent {
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_vec(alloc::vec![
            NormalizedLinearColorStop::new(PercentageValue::const_new(0), top),
            NormalizedLinearColorStop::new(PercentageValue::const_new(100), bottom),
        ]),
    })
}

/// Appends `items` to `v` - `Vec::extend` for the fixed-size pairs above.
pub(crate) fn push<const N: usize>(v: &mut Vec<CssPropertyWithConditions>, items: [CssPropertyWithConditions; N]) {
    v.extend(items);
}
