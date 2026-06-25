//! Spinner / activity-indicator widget — a small indeterminate "busy" ring. A
//! stateless single styled node (a near-clone of the leaf-node construction of
//! [`crate::widgets::badge::Badge`] / [`crate::widgets::progressbar::ProgressBar`]),
//! drawn as a circular ring whose three sides use a faint "track" colour and
//! whose top side uses a solid accent colour — the classic spinner look frozen
//! mid-rotation.
//!
//! ## PARTIAL — STATIC ONLY (no spin animation). See `TODO2` below.
//!
//! TODO2: this spinner is **static** — it shows the indeterminate ring shape but
//! does NOT rotate. Azul has no declarative CSS animation: there is no
//! `@keyframes` / `animation` / `transition` CSS property (`css/src/props` only
//! exposes one-shot `Transform`/`TransformOrigin` GPU props and the system-level
//! `AnimationMetrics` toggle; `props/basic/animation.rs` is SVG-curve
//! interpolation maths, not a style-driven keyframe engine). Producing real
//! motion would require a timer-driven `Update` loop that re-issues a rotating
//! `CssProperty::Transform` each tick (the same mechanism scroll-smoothing uses),
//! driven from the host app — there is no widget-local way to start such a timer
//! at DOM-build time. Rather than fake motion that cannot be produced, the ring
//! is rendered statically; a future revision can add the timer-driven rotation
//! once a widget-owned animation hook exists. (Compile-verified; not GUI-verified.)
//!
//! Key types: [`Spinner`].

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, *},
        layout::{LayoutAlignSelf, LayoutFlexGrow, LayoutWidth, LayoutHeight},
        property::{CssProperty, *},
        style::{LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius},
    },
    AzString,
};

static SPINNER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-spinner"))];

/// Default ring diameter, in logical px.
const DEFAULT_SIZE: isize = 24;
/// Faint "track" colour for the three inactive sides (#d0d4d9).
const DEFAULT_TRACK_COLOR: ColorU = ColorU { r: 208, g: 212, b: 217, a: 255 };
/// Solid accent colour for the active (top) arc (#0d6efd, accent blue).
const DEFAULT_ACCENT_COLOR: ColorU = ColorU { r: 13, g: 110, b: 253, a: 255 };

/// An indeterminate busy-indicator ring. Stateless; renders a single styled
/// node. **Static** — the ring shows the spinner shape but does not rotate
/// (see the module-level `TODO2`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Spinner {
    /// The ring diameter, in logical px.
    pub size: isize,
    /// Colour of the active (top) arc.
    pub color: ColorU,
    /// Colour of the three inactive ("track") sides.
    pub track_color: ColorU,
    /// The computed inline style for the ring.
    pub spinner_style: CssPropertyWithConditionsVec,
}

/// Builds the ring style for the given diameter and colours. All three are
/// instance-dependent, so the style is built at runtime per the recipe's
/// "runtime vec when param-dependent" path (see `badge::build_badge_style`).
fn build_spinner_style(size: isize, color: ColorU, track_color: ColorU) -> CssPropertyWithConditionsVec {
    // Ring thickness scales with the diameter (min 2px); radius = size/2 → circle.
    let border_width = (size / 8).max(2);
    let radius = size / 2;
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        // Hug its own size inside a flex parent rather than stretch/grow.
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(size))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(size))),
        // border: <border_width>px solid — three sides track, top accent.
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(border_width),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
            LayoutBorderBottomWidth::const_px(border_width),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
            LayoutBorderLeftWidth::const_px(border_width),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
            LayoutBorderRightWidth::const_px(border_width),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
            StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        // top = accent (the visible "arc"); other three = faint track.
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: color,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: track_color },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
            inner: track_color,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
            StyleBorderRightColor { inner: track_color },
        )),
        // border-radius: size/2 → a circle.
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(radius),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(radius),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(radius),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(radius),
        )),
    ])
}

impl Spinner {
    /// Creates a new spinner with the default size (24px) and accent colour.
    #[inline]
    #[must_use] pub fn create() -> Self {
        Self::with_size(DEFAULT_SIZE)
    }

    /// Creates a new spinner with the given diameter (logical px) and the
    /// default colours.
    #[inline]
    #[must_use] pub fn with_size(size: isize) -> Self {
        Self {
            size,
            color: DEFAULT_ACCENT_COLOR,
            track_color: DEFAULT_TRACK_COLOR,
            spinner_style: build_spinner_style(size, DEFAULT_ACCENT_COLOR, DEFAULT_TRACK_COLOR),
        }
    }

    /// Sets the ring diameter (logical px), recomputing the style.
    #[inline]
    pub fn set_size(&mut self, size: isize) {
        self.size = size;
        self.spinner_style = build_spinner_style(size, self.color, self.track_color);
    }

    /// Builder-style setter for the ring diameter.
    #[inline]
    #[must_use] pub fn with_spinner_size(mut self, size: isize) -> Self {
        self.set_size(size);
        self
    }

    /// Sets the active-arc colour, recomputing the style.
    #[inline]
    pub fn set_color(&mut self, color: ColorU) {
        self.color = color;
        self.spinner_style = build_spinner_style(self.size, color, self.track_color);
    }

    /// Builder-style setter for the active-arc colour.
    #[inline]
    #[must_use] pub fn with_color(mut self, color: ColorU) -> Self {
        self.set_color(color);
        self
    }

    /// Sets the inactive "track" colour, recomputing the style.
    #[inline]
    pub fn set_track_color(&mut self, track_color: ColorU) {
        self.track_color = track_color;
        self.spinner_style = build_spinner_style(self.size, self.color, track_color);
    }

    /// Builder-style setter for the inactive "track" colour.
    #[inline]
    #[must_use] pub fn with_track_color(mut self, track_color: ColorU) -> Self {
        self.set_track_color(track_color);
        self
    }

    /// Replaces `self` with a default spinner and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this spinner into a single DOM node with the
    /// `__azul-native-spinner` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(SPINNER_CLASS))
            .with_css_props(self.spinner_style)
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::create()
    }
}

impl From<Spinner> for Dom {
    fn from(s: Spinner) -> Self {
        s.dom()
    }
}
