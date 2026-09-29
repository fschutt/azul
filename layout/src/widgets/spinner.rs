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
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, OptionCssPropertyWithConditionsVec,
    },
    props::{
        basic::{color::ColorU, *},
        layout::{LayoutAlignSelf, LayoutFlexGrow, LayoutHeight, LayoutWidth},
        property::{CssProperty, *},
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBorderBottomColor, StyleBorderBottomLeftRadius,
            StyleBorderBottomRightRadius, StyleBorderBottomStyle, StyleBorderLeftColor,
            StyleBorderLeftStyle, StyleBorderRightColor, StyleBorderRightStyle,
            StyleBorderTopColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
            StyleBorderTopStyle,
        },
    },
    AzString,
};

/// The class the spinner's root node carries, in every theme.
pub(crate) static SPINNER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-spinner"))];

/// Which native busy indicator a [`Spinner`] draws.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SpinnerStyle {
    /// The theme's own indicator: the ring under Flat, the spokes under
    /// Flora.
    #[default]
    Auto,
    /// Eight capsule spokes with an opacity wave travelling clockwise - the
    /// macOS and iOS activity indicator.
    Spokes,
    /// A round-capped arc spinning on a ring - the Windows 11 `ProgressRing`.
    Ring,
}

/// Default ring diameter, in logical px.
const DEFAULT_SIZE: isize = 24;
/// Faint "track" colour for the three inactive sides (#d0d4d9).
const DEFAULT_TRACK_COLOR: ColorU = ColorU {
    r: 208,
    g: 212,
    b: 217,
    a: 255,
};
/// Solid accent colour for the active (top) arc (#0d6efd, accent blue).
const DEFAULT_ACCENT_COLOR: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 255,
};

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
    /// The ring's CSS, or `None` for "no opinion" — in which case the style is
    /// derived from `size`, `color` and `track_color` at render time.
    ///
    /// `None` and `Some(empty)` are different answers: the first means the
    /// widget picks, the second means the caller asked for no properties at all
    /// and gets none.
    pub spinner_style: OptionCssPropertyWithConditionsVec,
    /// Which native indicator to draw; `Auto` lets the theme pick.
    pub indicator: SpinnerStyle,
    /// The widget theme, or `None` for the default
    /// (`crate::widgets::themes::UiTheme::default()`, Flat).
    pub theme: crate::widgets::themes::OptionUiTheme,
}

/// Builds the ring style for the given diameter and colours. All three are
/// instance-dependent, so the style is built at runtime per the recipe's
/// "runtime vec when param-dependent" path (see `badge::build_badge_style`).
fn build_spinner_style(
    size: isize,
    color: ColorU,
    track_color: ColorU,
) -> CssPropertyWithConditionsVec {
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
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
            StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        // top = accent (the visible "arc"); other three = faint track.
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(
            StyleBorderTopColor { inner: color }
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: track_color },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_color(
            StyleBorderLeftColor { inner: track_color }
        )),
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
    #[must_use]
    pub const fn create() -> Self {
        Self::with_size(DEFAULT_SIZE)
    }

    /// Creates a new spinner with the given diameter (logical px) and the
    /// default colours.
    #[inline]
    #[must_use]
    pub const fn with_size(size: isize) -> Self {
        Self {
            size,
            color: DEFAULT_ACCENT_COLOR,
            track_color: DEFAULT_TRACK_COLOR,
            spinner_style: OptionCssPropertyWithConditionsVec::None,
            indicator: SpinnerStyle::Auto,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// The ring CSS this spinner renders with.
    ///
    /// `None` means no opinion, so the geometry and colours decide — the same
    /// answer both themes give, asked in one place so they cannot drift. It is
    /// also what makes the three setters below plain field writes: there is no
    /// cached vec left for them to keep in step.
    #[must_use]
    pub fn resolved_spinner_style(&self) -> CssPropertyWithConditionsVec {
        self.spinner_style
            .clone()
            .into_option()
            .unwrap_or_else(|| build_spinner_style(self.size, self.color, self.track_color))
    }

    /// Sets the ring diameter (logical px).
    #[inline]
    pub const fn set_size(&mut self, size: isize) {
        self.size = size;
    }

    /// Builder-style setter for the ring diameter.
    #[inline]
    #[must_use]
    pub const fn with_spinner_size(mut self, size: isize) -> Self {
        self.set_size(size);
        self
    }

    /// Sets the active-arc colour.
    #[inline]
    pub const fn set_color(&mut self, color: ColorU) {
        self.color = color;
    }

    /// Builder-style setter for the active-arc colour.
    #[inline]
    #[must_use]
    pub const fn with_color(mut self, color: ColorU) -> Self {
        self.set_color(color);
        self
    }

    /// Sets the inactive "track" colour.
    #[inline]
    pub const fn set_track_color(&mut self, track_color: ColorU) {
        self.track_color = track_color;
    }

    /// Builder-style setter for the inactive "track" colour.
    #[inline]
    #[must_use]
    pub const fn with_track_color(mut self, track_color: ColorU) -> Self {
        self.set_track_color(track_color);
        self
    }

    /// Replaces `self` with a default spinner and returns the original.
    #[inline]
    #[must_use]
    pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Picks the native indicator to draw (`Auto`: the theme's own).
    #[inline]
    pub const fn set_indicator(&mut self, indicator: SpinnerStyle) {
        self.indicator = indicator;
    }

    /// Builder-style setter for the indicator.
    #[inline]
    #[must_use]
    pub const fn with_indicator(mut self, indicator: SpinnerStyle) -> Self {
        self.set_indicator(indicator);
        self
    }

    /// Pick the widget theme. Unset (`None`), the spinner renders in the
    /// default theme (`crate::widgets::themes::UiTheme::default()`).
    #[inline]
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[inline]
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Converts this spinner into its DOM, root classed
    /// `__azul-native-spinner`. The look comes from the theme module
    /// (`themes::flat::spinner` / `themes::flora::spinner`); `None` renders
    /// flat.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::spinner(self),
            Some(UiTheme::Flat) | None => crate::widgets::themes::flat::spinner(self),
        }
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

#[cfg(test)]
#[allow(
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp
)]
mod autotest_generated {
    use azul_core::dom::NodeType;

    use super::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// The number of declarations `build_spinner_style` is supposed to emit.
    const DECLARATIONS: usize = 20;

    /// Every property the ring declares, in source order. A missing side (or a
    /// side declared twice) is the difference between a ring and a solid box.
    const EXPECTED_ORDER: [CssPropertyType; DECLARATIONS] = [
        CssPropertyType::AlignSelf,
        CssPropertyType::FlexGrow,
        CssPropertyType::Width,
        CssPropertyType::Height,
        CssPropertyType::BorderTopWidth,
        CssPropertyType::BorderBottomWidth,
        CssPropertyType::BorderLeftWidth,
        CssPropertyType::BorderRightWidth,
        CssPropertyType::BorderTopStyle,
        CssPropertyType::BorderBottomStyle,
        CssPropertyType::BorderLeftStyle,
        CssPropertyType::BorderRightStyle,
        CssPropertyType::BorderTopColor,
        CssPropertyType::BorderBottomColor,
        CssPropertyType::BorderLeftColor,
        CssPropertyType::BorderRightColor,
        CssPropertyType::BorderTopLeftRadius,
        CssPropertyType::BorderTopRightRadius,
        CssPropertyType::BorderBottomLeftRadius,
        CssPropertyType::BorderBottomRightRadius,
    ];

    /// `FloatValue` stores `value * FP_PRECISION_MULTIPLIER` as an `isize`, so a
    /// whole-pixel size only survives while `|size| <= isize::MAX / 1000`.
    const FP_SCALE: isize = 1000;

    /// The largest / smallest diameters that still fit the fixed-point encoding.
    /// `isize::MIN / 1000` truncates toward zero, so it scales back to
    /// `-9223372036854775000`, one step inside `isize::MIN`.
    const MAX_ENCODABLE_SIZE: isize = isize::MAX / FP_SCALE;
    const MIN_ENCODABLE_SIZE: isize = isize::MIN / FP_SCALE;

    /// Diameters that must all build a style without panicking: the degenerate
    /// small ones (where the 2px floor is thicker than the box), the ordinary
    /// ones, negatives (nothing in the widget rejects them), and both ends of
    /// the encodable range.
    const SAFE_SIZES: [isize; 16] = [
        0,
        1,
        2,
        3,
        4,
        7,
        8,
        15,
        16,
        24,
        1_000,
        -1,
        -3,
        -24,
        MAX_ENCODABLE_SIZE,
        MIN_ENCODABLE_SIZE,
    ];

    /// Colours that are trivially distinguishable in a failure message, plus the
    /// fully transparent one (alpha is carried untouched, so it must survive).
    const RED: ColorU = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    const GREEN: ColorU = ColorU {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    };
    const GHOST: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// The declared properties of a style vec, in declaration order.
    fn props(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_slice().iter().map(|p| p.property.clone()).collect()
    }

    /// The first property matching `f`, or `None` if the style never declares it.
    fn find<T>(
        v: &CssPropertyWithConditionsVec,
        f: impl Fn(&CssProperty) -> Option<T>,
    ) -> Option<T> {
        v.as_slice().iter().find_map(|p| f(&p.property))
    }

    /// The raw fixed-point encoding of a length — the value that actually
    /// survives, without a second lossy round trip through `get()`.
    fn raw(pv: PixelValue) -> isize {
        pv.number.number()
    }

    fn width(v: &CssPropertyWithConditionsVec) -> PixelValue {
        find(v, |p| match p {
            CssProperty::Width(x) => match x.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(*pv),
                other => panic!("the ring must size in absolute lengths, got {other:?}"),
            },
            _ => None,
        })
        .expect("the ring must declare a width")
    }

    fn height(v: &CssPropertyWithConditionsVec) -> PixelValue {
        find(v, |p| match p {
            CssProperty::Height(x) => match x.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(*pv),
                other => panic!("the ring must size in absolute lengths, got {other:?}"),
            },
            _ => None,
        })
        .expect("the ring must declare a height")
    }

    /// Border widths in `[top, right, bottom, left]` order.
    fn border_widths(v: &CssPropertyWithConditionsVec) -> [PixelValue; 4] {
        [
            find(v, |p| match p {
                CssProperty::BorderTopWidth(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderRightWidth(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderBottomWidth(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderLeftWidth(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
        ]
        .map(|o| o.expect("the ring must declare all four border widths"))
    }

    /// Border colours in `[top, right, bottom, left]` order.
    fn border_colors(v: &CssPropertyWithConditionsVec) -> [ColorU; 4] {
        [
            find(v, |p| match p {
                CssProperty::BorderTopColor(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderRightColor(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderBottomColor(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderLeftColor(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
        ]
        .map(|o| o.expect("the ring must declare all four border colours"))
    }

    /// Border styles in `[top, right, bottom, left]` order.
    fn border_styles(v: &CssPropertyWithConditionsVec) -> [BorderStyle; 4] {
        [
            find(v, |p| match p {
                CssProperty::BorderTopStyle(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderRightStyle(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderBottomStyle(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderLeftStyle(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
        ]
        .map(|o| o.expect("the ring must declare all four border styles"))
    }

    /// Corner radii in `[top-left, top-right, bottom-left, bottom-right]` order.
    fn radii(v: &CssPropertyWithConditionsVec) -> [PixelValue; 4] {
        [
            find(v, |p| match p {
                CssProperty::BorderTopLeftRadius(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderTopRightRadius(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderBottomLeftRadius(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::BorderBottomRightRadius(x) => x.get_property().map(|x| x.inner),
                _ => None,
            }),
        ]
        .map(|o| o.expect("the ring must declare all four corner radii"))
    }

    /// Every absolute length the style declares — width, height, the four border
    /// widths and the four radii.
    fn lengths(v: &CssPropertyWithConditionsVec) -> Vec<PixelValue> {
        let mut out = vec![width(v), height(v)];
        out.extend(border_widths(v));
        out.extend(radii(v));
        out
    }

    /// The properties a built DOM node carries inline, in declaration order.
    fn dom_props(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    fn dom_classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    // ==================================================================
    // build_spinner_style — shape of the emitted style
    // ==================================================================

    #[test]
    fn build_spinner_style_declares_every_side_exactly_once() {
        // A ring is four independently-declared sides. A dropped or duplicated
        // declaration silently turns the spinner into a box (or a solid disc).
        for size in SAFE_SIZES {
            let style = build_spinner_style(size, RED, GREEN);
            let types: Vec<CssPropertyType> =
                props(&style).iter().map(CssProperty::get_type).collect();

            assert_eq!(
                style.len(),
                DECLARATIONS,
                "declaration count changed for {size}"
            );
            assert_eq!(
                types,
                EXPECTED_ORDER.to_vec(),
                "declaration order changed for {size}"
            );

            let mut sorted = types.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                DECLARATIONS,
                "a property is declared twice for {size}"
            );
        }
    }

    #[test]
    fn build_spinner_style_declarations_are_all_unconditional() {
        // `simple()` means "no @media/@os/:hover guard". A condition sneaking in
        // would make the ring vanish on some platforms only.
        let style = build_spinner_style(24, RED, GREEN);
        for p in style.as_slice() {
            assert!(
                p.apply_if.is_empty(),
                "{:?} became conditional: {:?}",
                p.property.get_type(),
                p.apply_if,
            );
        }
    }

    #[test]
    fn build_spinner_style_is_deterministic() {
        for size in SAFE_SIZES {
            assert_eq!(
                build_spinner_style(size, RED, GREEN),
                build_spinner_style(size, RED, GREEN),
                "two identical calls produced different styles for {size}",
            );
        }
    }

    // ==================================================================
    // build_spinner_style — numeric edges
    // ==================================================================

    #[test]
    fn build_spinner_style_at_zero_size() {
        // 0 / 8 = 0, so the `.max(2)` floor is what keeps the border declarable;
        // the radius collapses to 0 and the box to 0x0.
        let style = build_spinner_style(0, RED, GREEN);

        assert_eq!(raw(width(&style)), 0);
        assert_eq!(raw(height(&style)), 0);
        for bw in border_widths(&style) {
            assert_eq!(
                raw(bw),
                2 * FP_SCALE,
                "the 2px border floor stopped applying at size 0"
            );
        }
        for r in radii(&style) {
            assert_eq!(raw(r), 0, "a zero-diameter ring must have a zero radius");
        }
    }

    #[test]
    fn border_width_is_an_eighth_of_the_size_with_a_two_px_floor() {
        // `(size / 8).max(2)`: integer division truncates toward zero, and every
        // negative eighth is swallowed by the floor.
        for size in (-256_isize..=256).chain(SAFE_SIZES) {
            let style = build_spinner_style(size, RED, GREEN);
            let expected = (size / 8).max(2);

            for bw in border_widths(&style) {
                assert_eq!(
                    raw(bw),
                    expected * FP_SCALE,
                    "border width wrong for size {size}",
                );
                assert!(
                    raw(bw) >= 2 * FP_SCALE,
                    "border width dropped below 2px for {size}"
                );
            }
        }
    }

    #[test]
    fn radius_is_half_the_size_truncated_toward_zero() {
        // `size / 2` truncates, so odd diameters get a radius half a pixel short
        // of a perfect circle — deterministic, and identical on all four corners.
        for size in (-256_isize..=256).chain(SAFE_SIZES) {
            let style = build_spinner_style(size, RED, GREEN);
            let corners = radii(&style);

            for r in corners {
                assert_eq!(
                    raw(r),
                    (size / 2) * FP_SCALE,
                    "radius wrong for size {size}"
                );
            }
            assert!(
                corners.iter().all(|r| *r == corners[0]),
                "the four corners disagree for size {size}: {corners:?}",
            );
        }
    }

    #[test]
    fn negative_sizes_pass_straight_through_unclamped() {
        // Nothing rejects or clamps a negative diameter: the box and the radius
        // both go negative while the border keeps its 2px floor. Pinned because
        // it is the *only* documented behaviour — if the widget ever starts
        // clamping to 0, this flips loudly rather than silently changing layout.
        let style = build_spinner_style(-24, RED, GREEN);

        assert_eq!(raw(width(&style)), -24 * FP_SCALE);
        assert_eq!(raw(height(&style)), -24 * FP_SCALE);
        assert_eq!(raw(border_widths(&style)[0]), 2 * FP_SCALE);
        assert_eq!(raw(radii(&style)[0]), -12 * FP_SCALE);
    }

    #[test]
    fn the_ring_is_thicker_than_its_box_only_below_four_px() {
        // Below 4px the 2px-per-side floor eats more than the whole diameter, so
        // the "ring" degenerates into a filled blob. Above it, `size / 8` keeps
        // the two borders at a quarter of the box at most.
        for size in 0_isize..=64 {
            let style = build_spinner_style(size, RED, GREEN);
            let total = 2 * (raw(border_widths(&style)[0]) / FP_SCALE);

            if size >= 4 {
                assert!(
                    total <= size,
                    "size {size}: borders ({total}px) overflow the box"
                );
            } else {
                assert!(
                    total > size,
                    "size {size}: expected the degenerate 2px-floor ring"
                );
            }
        }
    }

    #[test]
    fn every_length_is_an_absolute_pixel() {
        // A relative unit here would resolve against the parent font or box and
        // either vanish or blow up — the ring must be self-contained.
        for size in SAFE_SIZES {
            let style = build_spinner_style(size, RED, GREEN);
            for length in lengths(&style) {
                assert_eq!(
                    length.metric,
                    SizeMetric::Px,
                    "size {size} produced a relative length: {length:?}",
                );
            }
        }
    }

    #[test]
    fn size_round_trips_through_the_fixed_point_encoding() {
        // encode == decode: the diameter goes in as an `isize` and must come back
        // out of the style unchanged, including at both ends of the range.
        for size in SAFE_SIZES {
            let style = build_spinner_style(size, RED, GREEN);

            assert_eq!(
                raw(width(&style)),
                size * FP_SCALE,
                "width encoding lost {size}"
            );
            assert_eq!(
                raw(width(&style)) / FP_SCALE,
                size,
                "width did not round-trip for {size}"
            );
            assert_eq!(
                raw(height(&style)) / FP_SCALE,
                size,
                "height did not round-trip for {size}"
            );

            // The `f32` view is only exact for values a float can hold.
            if size.abs() <= 1_000 {
                assert_eq!(
                    width(&style).number.get(),
                    size as f32,
                    "float view wrong for {size}",
                );
            }
        }
    }

    #[test]
    fn the_edges_of_the_encodable_range_do_not_overflow() {
        // `isize::MAX / 1000` is the largest whole-pixel diameter `const_px` can
        // scale without wrapping; one more is the overflow pinned below.
        for size in [MAX_ENCODABLE_SIZE, MIN_ENCODABLE_SIZE] {
            let style = build_spinner_style(size, RED, GREEN);

            assert_eq!(style.len(), DECLARATIONS);
            assert_eq!(raw(width(&style)), size * FP_SCALE);
            assert_eq!(raw(radii(&style)[0]), (size / 2) * FP_SCALE);
            assert_eq!(raw(border_widths(&style)[0]), (size / 8).max(2) * FP_SCALE);
        }
    }

    #[cfg(panic = "unwind")]
    #[test]
    fn sizes_beyond_the_encodable_range_are_not_saturated() {
        use std::{
            hint::black_box,
            panic::{catch_unwind, AssertUnwindSafe},
        };

        // LATENT BUG, pinned: `PixelValue::const_px` multiplies by 1000 with a
        // plain `*`, so any diameter above `isize::MAX / 1000` (~9.2e15) either
        // panics (overflow checks on: `Spinner::with_size(isize::MAX)` kills a
        // debug build) or wraps to a garbage length (checks off) — it never
        // saturates. Asserted against a probe of the *current* profile so the
        // test is profile-independent; adding saturation flips it loudly.
        let profile_traps_overflow = catch_unwind(AssertUnwindSafe(|| {
            let big = black_box(isize::MAX);
            let _ = black_box(big * FP_SCALE);
        }))
        .is_err();

        for size in [
            isize::MAX,
            isize::MIN,
            MAX_ENCODABLE_SIZE + 1,
            MIN_ENCODABLE_SIZE - 1,
        ] {
            let widget_panicked = catch_unwind(AssertUnwindSafe(|| {
                drop(build_spinner_style(size, RED, GREEN))
            }))
            .is_err();

            assert_eq!(
                widget_panicked, profile_traps_overflow,
                "size {size}: the fixed-point encoding no longer behaves like a raw multiply \
                 (expected panic == {profile_traps_overflow})",
            );
        }
    }

    // ==================================================================
    // build_spinner_style — colour placement
    // ==================================================================

    #[test]
    fn only_the_top_side_gets_the_accent_colour() {
        // The whole spinner illusion is "one lit side, three faint ones". Swapping
        // a side would render a static ring with no visible arc.
        for (color, track) in [
            (RED, GREEN),
            (GHOST, RED),
            (RED, GHOST),
            (RED, RED),
            (
                ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
                ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
            ),
        ] {
            let style = build_spinner_style(24, color, track);
            let [top, right, bottom, left] = border_colors(&style);

            assert_eq!(top, color, "the top side lost the accent colour");
            assert_eq!(
                [right, bottom, left],
                [track; 3],
                "a track side lost its colour"
            );
        }
    }

    #[test]
    fn colour_channels_survive_untouched() {
        // Alpha in particular: a fully transparent accent must stay transparent
        // rather than being normalised to opaque somewhere in the pipeline.
        for a in [0_u8, 1, 127, 255] {
            let color = ColorU {
                r: 1,
                g: 2,
                b: 3,
                a,
            };
            let track = ColorU {
                r: 253,
                g: 254,
                b: 255,
                a: 255 - a,
            };
            let style = build_spinner_style(24, color, track);
            let [top, right, bottom, left] = border_colors(&style);

            assert_eq!((top.r, top.g, top.b, top.a), (1, 2, 3, a));
            for side in [right, bottom, left] {
                assert_eq!((side.r, side.g, side.b, side.a), (253, 254, 255, 255 - a));
            }
        }
    }

    #[test]
    fn all_four_sides_are_solid() {
        // `BorderStyle::None` on any side would delete that quarter of the ring.
        for size in SAFE_SIZES {
            assert_eq!(
                border_styles(&build_spinner_style(size, RED, GREEN)),
                [BorderStyle::Solid; 4],
                "a side stopped being solid at size {size}",
            );
        }
    }

    #[test]
    fn the_ring_neither_grows_nor_stretches() {
        // `align-self: start` + `flex-grow: 0` are what stop a flex parent from
        // stretching the ring into an ellipse.
        let style = build_spinner_style(24, RED, GREEN);

        let align = find(&style, |p| match p {
            CssProperty::AlignSelf(x) => x.get_property().copied(),
            _ => None,
        });
        let grow = find(&style, |p| match p {
            CssProperty::FlexGrow(x) => x.get_property().map(|x| x.inner),
            _ => None,
        });

        assert_eq!(align, Some(LayoutAlignSelf::Start));
        assert_eq!(grow.map(|g| g.number()), Some(0));
    }

    // ==================================================================
    // create / with_size / Default — construction invariants
    // ==================================================================

    #[test]
    fn create_matches_the_documented_defaults() {
        let s = Spinner::create();

        assert_eq!(s.size, DEFAULT_SIZE);
        assert_eq!(s.size, 24, "the documented default diameter changed");
        assert_eq!(
            s.color,
            ColorU {
                r: 13,
                g: 110,
                b: 253,
                a: 255
            }
        );
        assert_eq!(
            s.track_color,
            ColorU {
                r: 208,
                g: 212,
                b: 217,
                a: 255
            }
        );
        assert_eq!(
            s.resolved_spinner_style(),
            build_spinner_style(DEFAULT_SIZE, DEFAULT_ACCENT_COLOR, DEFAULT_TRACK_COLOR),
        );
        // 24px → 3px border, 12px radius.
        assert_eq!(
            raw(border_widths(&s.resolved_spinner_style())[0]),
            3 * FP_SCALE
        );
        assert_eq!(raw(radii(&s.resolved_spinner_style())[0]), 12 * FP_SCALE);
    }

    #[test]
    fn default_is_create() {
        assert_eq!(Spinner::default(), Spinner::create());
    }

    #[test]
    fn with_size_records_the_size_and_rebuilds_the_style() {
        for size in SAFE_SIZES {
            let s = Spinner::with_size(size);

            assert_eq!(s.size, size, "the size field does not match the argument");
            assert_eq!(s.color, DEFAULT_ACCENT_COLOR);
            assert_eq!(s.track_color, DEFAULT_TRACK_COLOR);
            assert_eq!(s.resolved_spinner_style().len(), DECLARATIONS);
            assert_eq!(
                s.resolved_spinner_style(),
                build_spinner_style(size, DEFAULT_ACCENT_COLOR, DEFAULT_TRACK_COLOR),
            );
            assert_eq!(raw(width(&s.resolved_spinner_style())) / FP_SCALE, size);
        }
    }

    // ==================================================================
    // set_size / set_color / set_track_color — no cross-clobbering
    // ==================================================================

    #[test]
    fn set_size_keeps_the_custom_colours() {
        // `set_size` rebuilds the whole style, so it has to feed the *current*
        // colours back in — a regression here silently resets the palette.
        let mut s = Spinner::create().with_color(RED).with_track_color(GREEN);
        s.set_size(64);

        assert_eq!(s.size, 64);
        assert_eq!(s.color, RED);
        assert_eq!(s.track_color, GREEN);
        assert_eq!(
            border_colors(&s.resolved_spinner_style()),
            [RED, GREEN, GREEN, GREEN]
        );
        assert_eq!(raw(width(&s.resolved_spinner_style())), 64 * FP_SCALE);
    }

    #[test]
    fn set_color_touches_only_the_accent() {
        let mut s = Spinner::with_size(48).with_track_color(GREEN);
        s.set_color(RED);

        assert_eq!(s.size, 48, "set_color moved the diameter");
        assert_eq!(s.track_color, GREEN, "set_color clobbered the track colour");
        assert_eq!(
            border_colors(&s.resolved_spinner_style()),
            [RED, GREEN, GREEN, GREEN]
        );
        assert_eq!(raw(width(&s.resolved_spinner_style())), 48 * FP_SCALE);
    }

    #[test]
    fn set_track_color_touches_only_the_track() {
        let mut s = Spinner::with_size(48).with_color(RED);
        s.set_track_color(GREEN);

        assert_eq!(s.size, 48, "set_track_color moved the diameter");
        assert_eq!(s.color, RED, "set_track_color clobbered the accent colour");
        assert_eq!(
            border_colors(&s.resolved_spinner_style()),
            [RED, GREEN, GREEN, GREEN]
        );
    }

    #[test]
    fn setters_are_idempotent_and_never_grow_the_style() {
        // The style is *replaced*, not appended to: a hundred rounds of setters
        // must leave exactly the same 20 declarations as one round.
        let mut s = Spinner::create();
        for _ in 0..100 {
            s.set_size(24);
            s.set_color(RED);
            s.set_track_color(GREEN);
        }

        let once = Spinner::with_size(24)
            .with_color(RED)
            .with_track_color(GREEN);
        assert_eq!(
            s.resolved_spinner_style().len(),
            DECLARATIONS,
            "the style vec grew"
        );
        assert_eq!(
            s, once,
            "repeated setters diverged from a single application"
        );
    }

    #[test]
    fn set_size_survives_every_encodable_diameter() {
        // The same spinner walked across the whole safe range: each step must
        // leave a fully-formed style, with no state left over from the previous.
        let mut s = Spinner::create().with_color(RED).with_track_color(GREEN);
        for size in SAFE_SIZES {
            s.set_size(size);

            assert_eq!(s.size, size);
            assert_eq!(s.resolved_spinner_style().len(), DECLARATIONS);
            assert_eq!(
                s,
                Spinner::with_size(size)
                    .with_color(RED)
                    .with_track_color(GREEN)
            );
        }
    }

    // ==================================================================
    // Builder setters mirror the mutating ones
    // ==================================================================

    #[test]
    fn builder_setters_match_the_mutating_setters() {
        for size in SAFE_SIZES {
            let mut mutated = Spinner::create();
            mutated.set_size(size);
            assert_eq!(
                Spinner::create().with_spinner_size(size),
                mutated,
                "size {size}"
            );
        }

        let mut mutated = Spinner::create();
        mutated.set_color(RED);
        assert_eq!(Spinner::create().with_color(RED), mutated);

        let mut mutated = Spinner::create();
        mutated.set_track_color(GHOST);
        assert_eq!(Spinner::create().with_track_color(GHOST), mutated);
    }

    #[test]
    fn the_builder_chain_is_order_independent() {
        // Each setter rebuilds from all three fields, so the final spinner must
        // not depend on the order the fields were set in.
        let a = Spinner::create()
            .with_spinner_size(40)
            .with_color(RED)
            .with_track_color(GREEN);
        let b = Spinner::create()
            .with_track_color(GREEN)
            .with_color(RED)
            .with_spinner_size(40);
        let c = Spinner::create()
            .with_color(RED)
            .with_spinner_size(40)
            .with_track_color(GREEN);

        assert_eq!(a, b, "setting the size last changed the result");
        assert_eq!(a, c, "interleaving the setters changed the result");
    }

    #[test]
    fn equality_distinguishes_every_field() {
        let base = Spinner::create();

        assert_ne!(base, Spinner::create().with_spinner_size(25));
        assert_ne!(base, Spinner::create().with_color(RED));
        assert_ne!(base, Spinner::create().with_track_color(RED));
        assert_eq!(base, Spinner::create().with_spinner_size(DEFAULT_SIZE));
    }

    // ==================================================================
    // swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_original_and_installs_a_default() {
        let mut s = Spinner::with_size(96)
            .with_color(RED)
            .with_track_color(GREEN);
        let expected = s.clone();

        let taken = s.swap_with_default();

        assert_eq!(taken, expected, "the returned spinner is not the original");
        assert_eq!(s, Spinner::create(), "the receiver is not a fresh default");
        // The returned value must own a live style, not a moved-out husk.
        assert_eq!(taken.resolved_spinner_style().len(), DECLARATIONS);
        assert_eq!(
            border_colors(&taken.resolved_spinner_style()),
            [RED, GREEN, GREEN, GREEN]
        );
    }

    #[test]
    fn swapping_twice_leaves_a_default_both_times() {
        let mut s = Spinner::with_size(MAX_ENCODABLE_SIZE);

        let first = s.swap_with_default();
        let second = s.swap_with_default();

        assert_eq!(first.size, MAX_ENCODABLE_SIZE);
        assert_eq!(
            second,
            Spinner::create(),
            "the second swap did not return the default"
        );
        assert_eq!(s, Spinner::create());
    }

    #[test]
    fn swap_on_a_default_is_observationally_a_no_op() {
        let mut s = Spinner::create();
        let taken = s.swap_with_default();

        assert_eq!(taken, Spinner::create());
        assert_eq!(s, Spinner::create());
    }

    // ==================================================================
    // Clone / ownership — the style vec is heap memory behind a C ABI
    // ==================================================================

    #[test]
    fn clone_deep_copies_the_style_buffer() {
        // `CssPropertyWithConditionsVec` is a raw-pointer FFI vec: a shallow clone
        // would alias one allocation into two owners and double-free it.
        // Only a STORED style owns a buffer — a resolved one is built fresh per
        // call and has nothing to alias — so this asks with an explicit style.
        let mut original = Spinner::with_size(32).with_color(RED);
        original.spinner_style =
            OptionCssPropertyWithConditionsVec::Some(build_spinner_style(32, RED, GREEN));
        let copy = original.clone();

        assert_eq!(copy, original);
        let buffer = |s: &Spinner| {
            s.spinner_style
                .as_ref()
                .expect("the fixture stores a style")
                .as_ptr()
        };
        assert_ne!(
            buffer(&original),
            buffer(&copy),
            "the clone shares the original's style buffer",
        );
    }

    #[test]
    fn mutating_a_clone_leaves_the_original_alone() {
        let original = Spinner::with_size(32).with_color(RED);
        let mut copy = original.clone();

        copy.set_size(8);
        copy.set_track_color(GHOST);

        assert_eq!(
            original.size, 32,
            "mutating the clone moved the original's size"
        );
        assert_eq!(original.track_color, DEFAULT_TRACK_COLOR);
        assert_eq!(
            raw(width(&original.resolved_spinner_style())),
            32 * FP_SCALE
        );
        assert_eq!(border_colors(&original.resolved_spinner_style())[0], RED);
    }

    // ==================================================================
    // dom()
    // ==================================================================

    #[test]
    fn dom_is_a_single_classed_div() {
        let dom = Spinner::create().dom();

        assert_eq!(*dom.root.get_node_type(), NodeType::Div);
        assert!(
            dom.children.as_ref().is_empty(),
            "the spinner must be a leaf node"
        );
        assert_eq!(dom.estimated_total_children, 0);
        assert_eq!(dom_classes(&dom), vec!["__azul-native-spinner".to_string()]);
    }

    #[test]
    fn dom_carries_exactly_the_spinner_style() {
        // `with_css_props` turns the vec into one inline rule per declaration;
        // nothing may be dropped, reordered or made conditional on the way.
        for size in SAFE_SIZES {
            let s = Spinner::with_size(size)
                .with_color(RED)
                .with_track_color(GREEN);
            let expected = props(&s.resolved_spinner_style());
            let dom = s.dom();

            assert_eq!(
                dom_props(&dom),
                expected,
                "the DOM lost declarations for size {size}"
            );
            assert!(
                dom.root
                    .style
                    .iter_inline_properties()
                    .all(|(_, c)| c.is_empty()),
                "size {size}: an inline declaration became conditional",
            );
        }
    }

    #[test]
    fn from_impl_matches_the_dom_method() {
        let s = Spinner::with_size(17).with_color(GHOST);
        assert_eq!(Dom::from(s.clone()), s.dom());
    }

    #[test]
    fn dom_is_deterministic_for_equal_inputs() {
        let a = Spinner::with_size(13)
            .with_color(RED)
            .with_track_color(GHOST)
            .dom();
        let b = Spinner::with_size(13)
            .with_color(RED)
            .with_track_color(GHOST)
            .dom();

        assert_eq!(a, b, "two identically-built spinners rendered differently");
    }

    #[test]
    fn dom_survives_every_encodable_size_and_colour() {
        for size in SAFE_SIZES {
            for (color, track) in [(RED, GREEN), (GHOST, GHOST)] {
                let dom = Spinner::with_size(size)
                    .with_color(color)
                    .with_track_color(track)
                    .dom();

                assert_eq!(
                    dom_props(&dom).len(),
                    DECLARATIONS,
                    "shape changed for {size}"
                );
                assert!(dom.children.as_ref().is_empty());
                assert_eq!(dom_classes(&dom).len(), 1);
            }
        }
    }
}

/// The makeover: a native busy indicator, per theme and per style.
///
/// Numbers are the ones `scripts/NATIVE_WIDGET_LOOK_REFERENCE_2026_09_28.md`
/// measured: macOS 11-15 (section 3.1) for the spokes, Windows 11's
/// `ProgressRing` (section 3.2) for the ring.
#[cfg(test)]
mod makeover_tests {
    use azul_core::{
        dom::{Dom, SvgNodeData},
        svg::SvgMultiPolygon,
    };
    use azul_css::{
        dynamic_selector::{
            BoolCondition, CssPropertyWithConditions, DynamicSelectorContext, ThemeCondition,
        },
        props::{
            basic::animation::{AnimationIterationCount, AnimationTiming, StyleAnimation},
            basic::color::SystemColorRef,
            property::CssProperty,
            style::{StyleBackgroundContent, StyleTransform},
        },
    };

    use super::*;
    use crate::widgets::themes::{flora, OptionUiTheme, UiTheme};

    /// macOS's opacity ramp at frame 0: the head at 12 o'clock, then 0.07
    /// less per spoke going counter-clockwise, down to 0.06 just clockwise
    /// of the head. Indexed by spoke, clockwise from 12 o'clock.
    const RAMP: [f32; 8] = [0.55, 0.06, 0.13, 0.20, 0.27, 0.34, 0.41, 0.48];

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    fn has_class(dom: &Dom, name: &str) -> bool {
        classes(dom).iter().any(|c| c == name)
    }

    /// Every node of the tree, depth first, root included.
    fn all_nodes(dom: &Dom) -> Vec<&Dom> {
        let mut out = vec![dom];
        for child in dom.children.as_ref() {
            out.extend(all_nodes(child));
        }
        out
    }

    fn with_class<'a>(dom: &'a Dom, name: &str) -> Vec<&'a Dom> {
        all_nodes(dom)
            .into_iter()
            .filter(|n| has_class(n, name))
            .collect()
    }

    fn clip(node: &Dom) -> Option<&SvgMultiPolygon> {
        match node.root.get_svg_data() {
            Some(SvgNodeData::Path(p)) => Some(p),
            _ => None,
        }
    }

    fn declarations(node: &Dom) -> Vec<CssPropertyWithConditions> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, c)| CssPropertyWithConditions {
                property: p.clone(),
                apply_if: c.clone(),
            })
            .collect()
    }

    /// The declarations that apply under `ctx`, in order.
    fn applying(node: &Dom, ctx: &DynamicSelectorContext) -> Vec<CssProperty> {
        declarations(node)
            .into_iter()
            .filter(|d| d.matches(ctx))
            .map(|d| d.property)
            .collect()
    }

    fn ctx(theme: ThemeCondition, reduced_motion: bool) -> DynamicSelectorContext {
        let mut c = DynamicSelectorContext::default();
        c.theme = theme;
        c.prefers_reduced_motion = if reduced_motion {
            BoolCondition::True
        } else {
            BoolCondition::False
        };
        c
    }

    fn light() -> DynamicSelectorContext {
        ctx(ThemeCondition::Light, false)
    }

    fn dark() -> DynamicSelectorContext {
        ctx(ThemeCondition::Dark, false)
    }

    fn last_fill(props: &[CssProperty]) -> Option<Vec<StyleBackgroundContent>> {
        props.iter().rev().find_map(|p| match p {
            CssProperty::BackgroundContent(v) => v.get_property().map(|v| v.as_ref().to_vec()),
            _ => None,
        })
    }

    fn opacity(props: &[CssProperty]) -> Option<f32> {
        props.iter().rev().find_map(|p| match p {
            CssProperty::Opacity(v) => v.get_property().map(|o| o.inner.normalized()),
            _ => None,
        })
    }

    fn animation_in(props: &[CssProperty]) -> Option<StyleAnimation> {
        props.iter().rev().find_map(|p| match p {
            CssProperty::AnimationIn(v) => v
                .get_property()
                .and_then(|list| list.as_ref().first().cloned()),
            _ => None,
        })
    }

    fn animation_out(props: &[CssProperty]) -> Option<StyleAnimation> {
        props.iter().rev().find_map(|p| match p {
            CssProperty::AnimationOut(v) => v
                .get_property()
                .and_then(|list| list.as_ref().first().cloned()),
            _ => None,
        })
    }

    /// The `@keyframes` block named `name` among the root's stylesheets.
    fn keyframes<'a>(dom: &'a Dom, name: &str) -> Option<&'a azul_css::css::Keyframes> {
        dom.css
            .as_ref()
            .iter()
            .flat_map(|css| css.keyframes.as_ref().iter())
            .find(|k| k.name.as_str() == name)
    }

    /// `(permille, opacity)` of every stop of a keyframes block that sets one.
    fn opacity_stops(kf: &azul_css::css::Keyframes) -> Vec<(u16, f32)> {
        kf.stops
            .as_ref()
            .iter()
            .filter_map(|s| {
                s.props.as_ref().iter().find_map(|p| match p {
                    CssProperty::Opacity(v) => {
                        v.get_property().map(|o| (s.permille, o.inner.normalized()))
                    }
                    _ => None,
                })
            })
            .collect()
    }

    /// `(permille, degrees)` of every stop that rotates.
    fn rotation_stops(kf: &azul_css::css::Keyframes) -> Vec<(u16, f32)> {
        kf.stops
            .as_ref()
            .iter()
            .filter_map(|s| {
                s.props.as_ref().iter().find_map(|p| match p {
                    CssProperty::Transform(v) => v.get_property().and_then(|list| {
                        list.as_ref().iter().find_map(|t| match t {
                            StyleTransform::Rotate(a) => Some((s.permille, a.to_degrees())),
                            _ => None,
                        })
                    }),
                    _ => None,
                })
            })
            .collect()
    }

    /// The point `r` px from the centre of a `size` box, `deg` degrees
    /// clockwise from 12 o'clock, in the box's own coordinates.
    fn polar(size: f32, deg: f32, r: f32) -> (f32, f32) {
        let (s, c) = deg.to_radians().sin_cos();
        (size / 2.0 + r * s, size / 2.0 - r * c)
    }

    fn inside(p: &SvgMultiPolygon, point: (f32, f32)) -> bool {
        p.contains_point(point.0, point.1)
    }

    fn spinner(theme: UiTheme, style: SpinnerStyle) -> Dom {
        Spinner::create()
            .with_theme(theme)
            .with_indicator(style)
            .dom()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.006
    }

    const RED: ColorU = ColorU {
        r: 200,
        g: 20,
        b: 20,
        a: 255,
    };

    // ------------------------------------------------------------------
    // Options and defaults
    // ------------------------------------------------------------------

    #[test]
    fn the_default_spinner_is_the_native_regular_size() {
        // macOS regular = 32pt, Windows 11 ProgressRing default = 32px. The old
        // 24 matched nothing.
        let s = Spinner::create();
        assert_eq!(s.size, 32);
        assert_eq!(s.indicator, SpinnerStyle::Auto, "the theme picks");
        assert_eq!(s.theme, OptionUiTheme::None, "no theme opinion");
    }

    #[test]
    fn a_flat_spinner_draws_the_windows_ring_by_default() {
        let dom = Spinner::create().dom();
        assert!(has_class(&dom, "__azul-native-spinner"), "{:?}", classes(&dom));
        assert!(has_class(&dom, "__azul-spinner-ring"), "{:?}", classes(&dom));
        assert_eq!(with_class(&dom, "__azul-spinner-arc").len(), 1, "one arc");
        assert!(with_class(&dom, "__azul-spinner-spoke").is_empty());
    }

    #[test]
    fn a_flora_spinner_draws_the_macos_spokes_by_default() {
        let dom = Spinner::create().with_theme(UiTheme::Flora).dom();
        assert!(has_class(&dom, "__azul-spinner-spokes"), "{:?}", classes(&dom));
        assert!(has_class(&dom, "__azul-theme-flora"), "{:?}", classes(&dom));
        assert_eq!(
            with_class(&dom, "__azul-spinner-spoke").len(),
            8,
            "8 capsule spokes, one every 45 degrees (the 12-spoke look is pre-Big Sur)"
        );
    }

    #[test]
    fn the_indicator_option_overrides_the_themes_pick() {
        let spokes = spinner(UiTheme::Flat, SpinnerStyle::Spokes);
        assert_eq!(with_class(&spokes, "__azul-spinner-spoke").len(), 8);
        let ring = spinner(UiTheme::Flora, SpinnerStyle::Ring);
        assert_eq!(with_class(&ring, "__azul-spinner-arc").len(), 1);
        assert!(with_class(&ring, "__azul-spinner-spoke").is_empty());
    }

    // ------------------------------------------------------------------
    // Shapes: real clip paths in the container's user space
    // ------------------------------------------------------------------

    #[test]
    fn the_container_sets_up_a_user_space_the_size_of_the_spinner() {
        for style in [SpinnerStyle::Spokes, SpinnerStyle::Ring] {
            let dom = Spinner::with_size(48).with_indicator(style).dom();
            match dom.root.get_svg_data() {
                Some(SvgNodeData::ViewBox {
                    min_x,
                    min_y,
                    width,
                    height,
                }) => {
                    assert_eq!((*min_x, *min_y, *width, *height), (0.0, 0.0, 48.0, 48.0));
                }
                other => panic!("{style:?}: no viewBox on the container: {other:?}"),
            }
        }
    }

    #[test]
    fn each_spoke_is_a_capsule_from_two_fifths_of_the_radius_to_the_rim() {
        // At 32: 4 wide, from r = 6.5 to r = 16, one every 45 degrees clockwise.
        let dom = spinner(UiTheme::Flat, SpinnerStyle::Spokes);
        let spokes = with_class(&dom, "__azul-spinner-spoke");
        assert_eq!(spokes.len(), 8);
        for (k, spoke) in spokes.iter().enumerate() {
            let deg = k as f32 * 45.0;
            let shape = clip(spoke).unwrap_or_else(|| panic!("spoke {k} has no clip shape"));
            assert!(inside(shape, polar(32.0, deg, 11.0)), "spoke {k}: its middle");
            assert!(inside(shape, polar(32.0, deg, 15.5)), "spoke {k}: near the rim");
            assert!(!inside(shape, polar(32.0, deg, 5.0)), "spoke {k}: the hole");
            assert!(
                !inside(shape, polar(32.0, deg + 22.5, 11.0)),
                "spoke {k}: the gap to its neighbour"
            );
            assert!(!inside(shape, (16.0, 16.0)), "spoke {k}: the centre");
        }
    }

    #[test]
    fn the_ring_is_a_round_capped_arc_on_the_windows_ring() {
        // At 32: centre-line radius 0.4375 x 32 = 14, stroke 0.09375 x 32 = 3.
        let dom = spinner(UiTheme::Flat, SpinnerStyle::Ring);
        let arc = with_class(&dom, "__azul-spinner-arc");
        let shape = clip(arc[0]).expect("the arc is a clip shape");
        assert!(inside(shape, polar(32.0, 60.0, 14.0)), "on the arc");
        assert!(!inside(shape, polar(32.0, 60.0, 11.0)), "inside the ring");
        assert!(!inside(shape, polar(32.0, 60.0, 16.0)), "outside the ring");
        assert!(!inside(shape, polar(32.0, 225.0, 14.0)), "the gap in the arc");
        assert!(!inside(shape, (16.0, 16.0)), "the centre");
    }

    #[test]
    fn a_ring_track_is_drawn_only_when_asked_for() {
        let bare = spinner(UiTheme::Flat, SpinnerStyle::Ring);
        assert!(
            with_class(&bare, "__azul-spinner-track").is_empty(),
            "the Windows ring has no track by default"
        );
        let tracked = Spinner::create()
            .with_indicator(SpinnerStyle::Ring)
            .with_track_color(RED)
            .dom();
        let track = with_class(&tracked, "__azul-spinner-track");
        assert_eq!(track.len(), 1);
        let shape = clip(track[0]).expect("the track is a clip shape");
        for deg in [0.0, 90.0, 225.0, 300.0] {
            assert!(inside(shape, polar(32.0, deg, 14.0)), "a full ring, at {deg}");
        }
        assert!(!inside(shape, (16.0, 16.0)), "a ring, not a disc");
        assert_eq!(
            last_fill(&applying(track[0], &light())),
            Some(vec![StyleBackgroundContent::Color(RED)])
        );
        // Under the arc: the track is painted first.
        let order: Vec<bool> = tracked.children.as_ref()
            .iter()
            .map(|c| has_class(c, "__azul-spinner-track"))
            .collect();
        assert_eq!(order.first(), Some(&true), "the track lies under the arc");
    }

    // ------------------------------------------------------------------
    // Motion
    // ------------------------------------------------------------------

    #[test]
    fn the_spokes_hold_the_macos_opacity_ramp() {
        let dom = spinner(UiTheme::Flat, SpinnerStyle::Spokes);
        for (k, spoke) in with_class(&dom, "__azul-spinner-spoke").iter().enumerate() {
            let o = opacity(&applying(spoke, &light())).expect("a spoke has an opacity");
            assert!(close(o, RAMP[k]), "spoke {k}: {o}, want {}", RAMP[k]);
        }
    }

    #[test]
    fn every_spoke_runs_its_own_phase_of_the_wave_once_per_800_ms() {
        let dom = spinner(UiTheme::Flat, SpinnerStyle::Spokes);
        let spokes = with_class(&dom, "__azul-spinner-spoke");
        let mut names = Vec::new();
        for (k, spoke) in spokes.iter().enumerate() {
            let anim = animation_in(&applying(spoke, &light()))
                .unwrap_or_else(|| panic!("spoke {k} declares no animation"));
            assert_eq!(anim.duration.millis(), 800, "spoke {k}: one revolution per 0.8 s");
            assert_eq!(anim.iterations, AnimationIterationCount::Infinite, "spoke {k}");
            assert_eq!(anim.timing, AnimationTiming::Linear, "spoke {k}");
            let kf = keyframes(&dom, anim.name.as_str())
                .unwrap_or_else(|| panic!("spoke {k}: @keyframes {} is missing", anim.name.as_str()));
            let stops = opacity_stops(kf);
            let at = |permille: u16| stops.iter().find(|(p, _)| *p == permille).map(|(_, o)| *o);
            // It starts where the static ramp holds it, so the first frame
            // and the reduced-motion picture agree.
            assert!(at(0).is_some_and(|o| close(o, RAMP[k])), "spoke {k}: {stops:?}");
            // Its peak is the head passing it: spoke k is the head k/8 in.
            let peak = if k == 0 { 0 } else { 125 * k as u16 };
            assert!(at(peak).is_some_and(|o| close(o, 0.55)), "spoke {k}: {stops:?}");
            let low = stops.iter().map(|(_, o)| *o).fold(1.0_f32, f32::min);
            assert!(close(low, 0.06), "spoke {k}: the trough is 0.06, {stops:?}");
            names.push(anim.name.as_str().to_string());
        }
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 8, "one phase-shifted track per spoke");
    }

    #[test]
    fn the_ring_spins_clockwise_at_450_degrees_a_second() {
        let dom = spinner(UiTheme::Flat, SpinnerStyle::Ring);
        let arc = with_class(&dom, "__azul-spinner-arc");
        let anim = animation_in(&applying(arc[0], &light())).expect("the arc spins");
        assert_eq!(anim.duration.millis(), 800);
        assert_eq!(anim.iterations, AnimationIterationCount::Infinite);
        assert_eq!(anim.timing, AnimationTiming::Linear);
        let turns = rotation_stops(keyframes(&dom, anim.name.as_str()).expect("@keyframes"));
        assert_eq!(turns.first().map(|t| t.0), Some(0));
        assert_eq!(turns.last().map(|t| t.0), Some(1000));
        let sweep = turns.last().map_or(0.0, |t| t.1) - turns.first().map_or(0.0, |t| t.1);
        assert!(close(sweep, 360.0), "a full clockwise turn per cycle, got {sweep}");
    }

    #[test]
    fn the_spinner_fades_in_when_shown_and_out_when_hidden() {
        for style in [SpinnerStyle::Spokes, SpinnerStyle::Ring] {
            let dom = spinner(UiTheme::Flat, style);
            let root = applying(&dom, &light());
            let fade_in = animation_in(&root).expect("a fade in");
            let fade_out = animation_out(&root).expect("a fade out");
            let rise = opacity_stops(keyframes(&dom, fade_in.name.as_str()).expect("@keyframes"));
            let fall = opacity_stops(keyframes(&dom, fade_out.name.as_str()).expect("@keyframes"));
            assert_eq!(rise.first().map(|s| s.1), Some(0.0), "{style:?}: {rise:?}");
            assert_eq!(rise.last().map(|s| s.1), Some(1.0), "{style:?}: {rise:?}");
            assert_eq!(fall.first().map(|s| s.1), Some(1.0), "{style:?}: {fall:?}");
            assert_eq!(fall.last().map(|s| s.1), Some(0.0), "{style:?}: {fall:?}");
            assert_ne!(
                fade_in.iterations,
                AnimationIterationCount::Infinite,
                "a fade runs once"
            );
        }
    }

    #[test]
    fn the_spinner_holds_still_under_reduced_motion() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            for style in [SpinnerStyle::Spokes, SpinnerStyle::Ring] {
                let dom = spinner(theme, style);
                let still = ctx(ThemeCondition::Light, true);
                for node in all_nodes(&dom) {
                    let props = applying(node, &still);
                    assert!(
                        animation_in(&props).is_none() && animation_out(&props).is_none(),
                        "{theme:?} {style:?}: {:?} animates under reduced motion",
                        classes(node)
                    );
                }
                // ...and it is still an indicator: the same shapes, held.
                let moving = all_nodes(&dom)
                    .into_iter()
                    .filter(|n| animation_in(&applying(n, &light())).is_some())
                    .count();
                assert!(moving > 1, "{theme:?} {style:?}: nothing declared motion");
                assert!(
                    all_nodes(&dom).iter().filter(|n| clip(n).is_some()).count() >= 1,
                    "{theme:?} {style:?}: the static picture lost its shapes"
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // Colour: the native ink per theme, or the caller's
    // ------------------------------------------------------------------

    #[test]
    fn flat_spokes_are_black_by_day_and_white_by_night() {
        // The macOS sprite is pure ink; only the alpha varies.
        let dom = spinner(UiTheme::Flat, SpinnerStyle::Spokes);
        let spoke = with_class(&dom, "__azul-spinner-spoke")[0];
        assert_eq!(
            last_fill(&applying(spoke, &light())),
            Some(vec![StyleBackgroundContent::Color(ColorU::BLACK)])
        );
        assert_eq!(
            last_fill(&applying(spoke, &dark())),
            Some(vec![StyleBackgroundContent::Color(ColorU::WHITE)])
        );
    }

    #[test]
    fn flora_spokes_are_flora_s_ink_by_day_and_night() {
        let dom = spinner(UiTheme::Flora, SpinnerStyle::Spokes);
        let spoke = with_class(&dom, "__azul-spinner-spoke")[0];
        assert_eq!(
            last_fill(&applying(spoke, &light())),
            Some(vec![StyleBackgroundContent::Color(flora::LIGHT_INK)])
        );
        assert_eq!(
            last_fill(&applying(spoke, &dark())),
            Some(vec![StyleBackgroundContent::Color(flora::DARK_INK)])
        );
    }

    #[test]
    fn the_flat_ring_is_the_desktop_accent() {
        let dom = spinner(UiTheme::Flat, SpinnerStyle::Ring);
        let arc = with_class(&dom, "__azul-spinner-arc")[0];
        let accent = Some(vec![StyleBackgroundContent::SystemColor(
            SystemColorRef::Accent,
        )]);
        assert_eq!(last_fill(&applying(arc, &light())), accent);
        assert_eq!(last_fill(&applying(arc, &dark())), accent, "resolved per theme");
    }

    #[test]
    fn the_flora_ring_is_the_accent_stone_lifted_to_its_glow_at_night() {
        let dom = spinner(UiTheme::Flora, SpinnerStyle::Ring);
        let arc = with_class(&dom, "__azul-spinner-arc")[0];
        assert_eq!(
            last_fill(&applying(arc, &light())),
            Some(vec![StyleBackgroundContent::Color(flora::LIGHT_ACC)])
        );
        assert_eq!(
            last_fill(&applying(arc, &dark())),
            Some(vec![StyleBackgroundContent::Color(flora::DARK_GLOW)])
        );
    }

    #[test]
    fn a_chosen_colour_paints_the_indicator_in_both_modes() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            for (style, part) in [
                (SpinnerStyle::Spokes, "__azul-spinner-spoke"),
                (SpinnerStyle::Ring, "__azul-spinner-arc"),
            ] {
                let dom = Spinner::create()
                    .with_theme(theme)
                    .with_indicator(style)
                    .with_color(RED)
                    .dom();
                let node = with_class(&dom, part)[0];
                for mode in [light(), dark()] {
                    assert_eq!(
                        last_fill(&applying(node, &mode)),
                        Some(vec![StyleBackgroundContent::Color(RED)]),
                        "{theme:?} {style:?}"
                    );
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // The container
    // ------------------------------------------------------------------

    #[test]
    fn a_callers_spinner_style_replaces_the_container_css() {
        let custom = CssPropertyWithConditionsVec::from_vec(alloc::vec![
            CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(
                77
            )))
        ]);
        let mut s = Spinner::create();
        s.spinner_style = OptionCssPropertyWithConditionsVec::Some(custom.clone());
        let dom = s.dom();
        let got: Vec<CssProperty> = declarations(&dom).into_iter().map(|d| d.property).collect();
        let want: Vec<CssProperty> = custom.as_ref().iter().map(|p| p.property.clone()).collect();
        assert_eq!(got, want, "the caller chose every container property");
        assert_eq!(
            with_class(&dom, "__azul-spinner-arc").len(),
            1,
            "the indicator is still drawn inside it"
        );
    }

    #[test]
    fn the_spinner_is_decoration_to_the_keyboard() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            for style in [SpinnerStyle::Spokes, SpinnerStyle::Ring] {
                let dom = spinner(theme, style);
                assert!(
                    all_nodes(&dom).iter().all(|n| n.root.get_tab_index().is_none()),
                    "{theme:?} {style:?}: a busy indicator takes no focus"
                );
            }
        }
    }
}
