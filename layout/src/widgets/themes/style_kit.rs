//! Small style builders the widget skins in `flat` / `flora` share, plus the
//! theme marker class a themed widget carries on its root.
//!
//! Every colour helper here comes in the light+dark shape the theme modules
//! need: the light value first, its `dark_theme(..)` twin right after it, so
//! a skin assembled from these helpers cannot ship a dark twin without its
//! light half or push one before it (`widgets::theme_pairs`).
//!
//! A skin appends its STATE rules (`:hover`, `:active`, `:focus`) after every
//! resting declaration: a resting dark twin matches in every pseudo-state,
//! so one pushed after a state rule would shadow it.

use alloc::vec::Vec;

use azul_core::dom::IdOrClass;
#[allow(clippy::wildcard_imports)]
use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    AzString,
};

use super::UiTheme;

type P = CssPropertyWithConditions;

// ---------------------------------------------------------------------------
// The theme marker
// ---------------------------------------------------------------------------

/// The class on the root of a widget the flat theme rendered.
pub const FLAT_CLASS: &str = "__azul-theme-flat";
/// The class on the root of a widget the flora theme rendered.
pub const FLORA_CLASS: &str = "__azul-theme-flora";

/// The marker class for `theme`, as the root's `IdOrClass`.
#[must_use]
pub const fn marker(theme: UiTheme) -> IdOrClass {
    IdOrClass::Class(AzString::from_const_str(match theme {
        UiTheme::Flat => FLAT_CLASS,
        UiTheme::Flora => FLORA_CLASS,
    }))
}

/// Which theme a node was rendered in, read back from its classes - what a
/// callback that live-restyles a widget asks, so the colours it writes are
/// the theme's the widget was BUILT in. No marker: the default theme.
#[must_use]
pub fn theme_of_classes(classes: &[AzString]) -> UiTheme {
    if classes.iter().any(|c| c.as_str() == FLORA_CLASS) {
        UiTheme::Flora
    } else {
        UiTheme::Flat
    }
}

// ---------------------------------------------------------------------------
// Plain (unconditional) declarations
// ---------------------------------------------------------------------------

/// `background: <colour>` as a property.
#[must_use]
pub fn bg(color: ColorU) -> CssProperty {
    layers(alloc::vec![StyleBackgroundContent::Color(color)])
}

/// `background: <layers>`, painted first to last.
#[must_use]
pub fn layers(list: Vec<StyleBackgroundContent>) -> CssProperty {
    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(list))
}

/// `color: <colour>` as a property.
#[must_use]
pub const fn ink(color: ColorU) -> CssProperty {
    CssProperty::const_text_color(StyleTextColor { inner: color })
}

/// `padding: top right bottom left`, in px.
#[must_use]
pub const fn padding(top: isize, right: isize, bottom: isize, left: isize) -> [P; 4] {
    [
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(top))),
        P::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(right))),
        P::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(bottom))),
        P::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(left))),
    ]
}

/// `border-radius: <px>` on all four corners.
#[must_use]
pub const fn radius(px: isize) -> [P; 4] {
    [
        P::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(px),
        )),
        P::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(px),
        )),
        P::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(px),
        )),
        P::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(px),
        )),
    ]
}

/// `border-radius` per corner: top-left, top-right, bottom-right, bottom-left.
#[must_use]
pub const fn radius_corners(tl: isize, tr: isize, br: isize, bl: isize) -> [P; 4] {
    [
        P::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(tl),
        )),
        P::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(tr),
        )),
        P::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(br),
        )),
        P::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(bl),
        )),
    ]
}

/// `font-size: <px>`.
#[must_use]
pub const fn font_size(px: isize) -> P {
    P::simple(CssProperty::const_font_size(StyleFontSize::const_px(px)))
}

/// `font-weight`.
#[must_use]
pub const fn weight(w: StyleFontWeight) -> P {
    P::simple(CssProperty::font_weight(w))
}

// ---------------------------------------------------------------------------
// Light + dark pairs (resting)
// ---------------------------------------------------------------------------

/// A resting background, light then its dark twin.
#[must_use]
pub fn themed_bg(light: ColorU, dark: ColorU) -> [P; 2] {
    [P::simple(bg(light)), P::dark_theme(bg(dark))]
}

/// A resting layered background (a gradient face), light then dark.
#[must_use]
pub fn themed_layers(light: Vec<StyleBackgroundContent>, dark: Vec<StyleBackgroundContent>) -> [P; 2] {
    [P::simple(layers(light)), P::dark_theme(layers(dark))]
}

/// A resting text colour, light then its dark twin.
#[must_use]
pub const fn themed_ink(light: ColorU, dark: ColorU) -> [P; 2] {
    [P::simple(ink(light)), P::dark_theme(ink(dark))]
}

/// Which edges a border helper draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edges {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl Edges {
    /// All four edges.
    pub const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };
    /// The bottom edge only (a rule under a heading).
    pub const BOTTOM: Self = Self {
        top: false,
        right: false,
        bottom: true,
        left: false,
    };
    /// The top edge only (a rule over a footer).
    pub const TOP: Self = Self {
        top: true,
        right: false,
        bottom: false,
        left: false,
    };
}

/// A solid border of `width` px on `edges`: widths, styles, the light colour
/// and - right after it - the dark twin of the colour.
#[must_use]
pub fn border(edges: Edges, width: isize, light: ColorU, dark: ColorU) -> Vec<P> {
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
        v.push(P::dark_theme(CssProperty::const_border_top_color(StyleBorderTopColor {
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
        v.push(P::dark_theme(CssProperty::const_border_right_color(
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
        v.push(P::dark_theme(CssProperty::const_border_bottom_color(
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
        v.push(P::dark_theme(CssProperty::const_border_left_color(
            StyleBorderLeftColor { inner: dark },
        )));
    }
    v
}

/// A 1px border that is invisible at rest in both modes: the box a focus
/// ring colours in, so the ring costs no layout when it appears.
#[must_use]
pub fn ring_slot() -> Vec<P> {
    border(Edges::ALL, 1, ColorU::TRANSPARENT, ColorU::TRANSPARENT)
}

/// A drop shadow under a floating surface, light then dark. One edge slot
/// carries it: each edge's shadow paints as a whole-box shadow, so four
/// copies would stack.
#[must_use]
pub fn drop_shadow(offset_y: isize, blur: isize, light: ColorU, dark: ColorU) -> [P; 2] {
    let shadow = |color: ColorU| StyleBoxShadow {
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
            inner: PixelValue::const_px(0),
        },
        clip_mode: BoxShadowClipMode::Outset,
        color,
    };
    [
        P::simple(CssProperty::box_shadow_bottom(shadow(light))),
        P::dark_theme(CssProperty::box_shadow_bottom(shadow(dark))),
    ]
}

/// An inset shadow along the top edge (a sunken well), light then dark.
#[must_use]
pub fn inset_shadow(offset_y: isize, blur: isize, light: ColorU, dark: ColorU) -> [P; 2] {
    let shadow = |color: ColorU| StyleBoxShadow {
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
            inner: PixelValue::const_px(0),
        },
        clip_mode: BoxShadowClipMode::Inset,
        color,
    };
    [
        P::simple(CssProperty::box_shadow_top(shadow(light))),
        P::dark_theme(CssProperty::box_shadow_top(shadow(dark))),
    ]
}

// ---------------------------------------------------------------------------
// State pairs (append AFTER every resting declaration)
// ---------------------------------------------------------------------------

/// The focus ring: all four border edges take `light` on `:focus`, `dark`
/// on `:focus` in the dark theme. Needs a border to colour - pair it with
/// [`ring_slot`] (or the part's own border).
#[must_use]
pub const fn focus_ring(light: ColorU, dark: ColorU) -> [P; 8] {
    [
        P::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor { inner: light })),
        P::on_focus(CssProperty::const_border_right_color(StyleBorderRightColor {
            inner: light,
        })),
        P::on_focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: light,
        })),
        P::on_focus(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light })),
        P::dark_on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: dark,
        })),
        P::dark_on_focus(CssProperty::const_border_right_color(StyleBorderRightColor {
            inner: dark,
        })),
        P::dark_on_focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: dark,
        })),
        P::dark_on_focus(CssProperty::const_border_left_color(StyleBorderLeftColor {
            inner: dark,
        })),
    ]
}

/// The focus ring of an item in a joined bar (a pagination button, a
/// segment), whose inner items share their side borders so a border ring
/// would miss an edge: a 2px inset shadow ring on `:focus`, light then dark.
/// One edge slot carries it (each edge's shadow paints the whole box): the
/// LEFT one, so it adds to a resting drop / inset shadow ([`drop_shadow`]
/// uses the bottom slot, [`inset_shadow`] the top) instead of replacing it.
#[must_use]
pub fn focus_shadow_ring(light: ColorU, dark: ColorU) -> [P; 2] {
    let ring = |color: ColorU| StyleBoxShadow {
        offset_x: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        offset_y: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        blur_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        spread_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(2),
        },
        clip_mode: BoxShadowClipMode::Inset,
        color,
    };
    [
        P::on_focus(CssProperty::box_shadow_left(ring(light))),
        P::dark_on_focus(CssProperty::box_shadow_left(ring(dark))),
    ]
}

/// A hover fill, light then dark.
#[must_use]
pub fn hover_bg(light: ColorU, dark: ColorU) -> [P; 2] {
    [P::on_hover(bg(light)), P::dark_on_hover(bg(dark))]
}

/// A hover face of layers (a gradient), light then dark.
#[must_use]
pub fn hover_layers(light: Vec<StyleBackgroundContent>, dark: Vec<StyleBackgroundContent>) -> [P; 2] {
    [P::on_hover(layers(light)), P::dark_on_hover(layers(dark))]
}

/// A pressed fill, light then dark.
#[must_use]
pub fn active_bg(light: ColorU, dark: ColorU) -> [P; 2] {
    [P::on_active(bg(light)), P::dark_on_active(bg(dark))]
}

/// A pressed face of layers (a gradient), light then dark.
#[must_use]
pub fn active_layers(light: Vec<StyleBackgroundContent>, dark: Vec<StyleBackgroundContent>) -> [P; 2] {
    [P::on_active(layers(light)), P::dark_on_active(layers(dark))]
}

/// A hover text colour, light then dark.
#[must_use]
pub const fn hover_ink(light: ColorU, dark: ColorU) -> [P; 2] {
    [P::on_hover(ink(light)), P::dark_on_hover(ink(dark))]
}

/// A hover border colour on all four edges, light then dark.
#[must_use]
pub const fn hover_border(light: ColorU, dark: ColorU) -> [P; 8] {
    [
        P::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor { inner: light })),
        P::on_hover(CssProperty::const_border_right_color(StyleBorderRightColor {
            inner: light,
        })),
        P::on_hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: light,
        })),
        P::on_hover(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light })),
        P::dark_on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: dark,
        })),
        P::dark_on_hover(CssProperty::const_border_right_color(StyleBorderRightColor {
            inner: dark,
        })),
        P::dark_on_hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: dark,
        })),
        P::dark_on_hover(CssProperty::const_border_left_color(StyleBorderLeftColor {
            inner: dark,
        })),
    ]
}

/// A two-stop vertical gradient face (`linear-gradient(top, bottom)`).
#[must_use]
pub fn face(top: ColorU, bottom: ColorU) -> StyleBackgroundContent {
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
pub fn push<const N: usize>(v: &mut Vec<P>, items: [P; N]) {
    v.extend(items);
}
