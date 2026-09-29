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

use alloc::vec::Vec;

use azul_css::{
    css::BoxOrStatic,
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::{color::ColorU, pixel::PixelValue, pixel::PixelValueNoPercent, StyleFontWeight},
        layout::{
            LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight, LayoutMarginTop,
            LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop,
        },
        property::{CssProperty, StyleBoxShadowValue},
        style::{
            BorderStyle, BoxShadowClipMode, LayoutBorderBottomWidth, LayoutBorderLeftWidth,
            LayoutBorderRightWidth, LayoutBorderTopWidth, StyleBackgroundContent,
            StyleBackgroundContentVec, StyleBorderBottomColor, StyleBorderBottomLeftRadius,
            StyleBorderBottomRightRadius, StyleBorderBottomStyle, StyleBorderLeftColor,
            StyleBorderLeftStyle, StyleBorderRightColor, StyleBorderRightStyle,
            StyleBorderTopColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
            StyleBorderTopStyle, StyleBoxShadow, StyleLetterSpacing, StyleTextColor,
            StyleTextDecoration,
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

/// A resting ink with its dark twin.
#[must_use]
pub(crate) fn themed_ink(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed(ink(light), ink(dark))
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
/// twin. Only visible on a node with a border (see the module note).
#[must_use]
pub(crate) fn focus_ring(light: ColorU, dark: ColorU) -> Vec<CssPropertyWithConditions> {
    let mut out = Vec::with_capacity(8);
    for (l, d) in border_colors(light).into_iter().zip(border_colors(dark)) {
        out.extend(CssPropertyWithConditions::themed_on_focus(l, d));
    }
    out
}

/// All four border colours on `:hover`, each with its dark twin.
#[must_use]
pub(crate) fn hover_border_color(light: ColorU, dark: ColorU) -> Vec<CssPropertyWithConditions> {
    let mut out = Vec::with_capacity(8);
    for (l, d) in border_colors(light).into_iter().zip(border_colors(dark)) {
        out.extend(CssPropertyWithConditions::themed_on_hover(l, d));
    }
    out
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
pub(crate) fn hover_ink(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    CssPropertyWithConditions::themed_on_hover(ink(light), ink(dark))
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
/// declaration.
///
/// azul stores a shadow per side, and the painter draws every side's shadow
/// as a whole box shadow (`display_list.rs`, "Check all four sides"): the
/// four copies the `box-shadow` shorthand expands to overlap, and a
/// translucent shadow comes out four times as dark. One side's slot carries
/// exactly one shadow, which is what a theme means.
#[must_use]
pub(crate) fn shadow(
    offset_y: isize,
    blur: isize,
    spread: isize,
    color: ColorU,
    inset: bool,
) -> CssProperty {
    CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(BoxOrStatic::heap(
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
        },
    )))
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
    CssPropertyWithConditions::simple(CssProperty::font_weight(StyleFontWeight::Bold))
}

/// `font-weight: 600`.
#[must_use]
pub(crate) const fn semibold() -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::font_weight(StyleFontWeight::W600))
}

/// `letter-spacing: <em>em` - flora tracks its small labels out.
#[must_use]
pub(crate) fn letter_spacing_em(em: f32) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::const_letter_spacing(StyleLetterSpacing {
        inner: PixelValue::em(em),
    }))
}
