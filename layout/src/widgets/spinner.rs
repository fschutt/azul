//! Spinner / activity indicator: the desktop's indeterminate "busy" indicator,
//! in the shape the desktop draws it.
//!
//! Two native shapes, measured in
//! `scripts/NATIVE_WIDGET_LOOK_REFERENCE_2026_09_28.md` (sections 3 and 5.2):
//!
//! * [`SpinnerStyle::Spokes`] - macOS 11-15 `NSProgressIndicator` (spinning) and iOS
//!   `UIActivityIndicatorView`: eight capsule spokes, D/8 wide, from 0.40 of the radius out to
//!   the rim. A wave of opacity travels clockwise, one revolution per 0.8 s: 0.55 at the head,
//!   0.07 less per spoke behind it, down to 0.06 just ahead of it.
//! * [`SpinnerStyle::Ring`] - the Windows 11 `ProgressRing`: a round-capped arc on a ring of
//!   centre-line radius 0.4375 D and stroke 0.09375 D, turning at 450 degrees a second. The native
//!   arc also grows and shrinks; a fixed 135-degree arc is what one clip shape can hold.
//!
//! Flat draws the ring and Flora the spokes ([`SpinnerStyle::Auto`]); either theme draws either
//! shape when asked.
//!
//! The shapes are real clip paths: each spoke, the arc and the optional track is a full-size
//! node painted in its colour and clipped to its outline (`Dom::with_svg_clip_path`), in the user
//! space the container declares (`SvgNodeData::ViewBox`, `0 0 D D`) - so the geometry scales
//! with the size, and hit-testing follows the shape.
//!
//! The motion is declared, not driven: every moving part carries
//! `-azul-animation-in: <keyframes> 800ms linear infinite`, and the container fades in and out
//! as it is shown and hidden (`-azul-animation-in` / `-azul-animation-out`). CSS would stagger
//! ONE spoke track with negative `animation-delay`s; a `CssDuration` holds no negative time, so
//! each spoke gets its own `@keyframes` block, phase-rotated so it starts at frame 0 of the wave.
//! Every animation is gated on `prefers-reduced-motion: no-preference`: with reduced motion the
//! indicator is the same picture, held still - the spokes at their frame-0 ramp, the arc where it
//! starts.
//!
//! Key types: [`Spinner`], [`SpinnerStyle`].

use alloc::vec::Vec;

use azul_core::{
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, SvgNodeData},
    svg::{SvgLine, SvgMultiPolygon, SvgPath, SvgPathElement, SvgPathElementVec, SvgPathVec},
};
use azul_css::{
    css::{Css, KeyframeStop, KeyframeStopVec, Keyframes, KeyframesVec},
    dynamic_selector::{
        BoolCondition, CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelector,
        OptionCssPropertyWithConditionsVec,
    },
    props::{
        basic::{
            angle::AngleValue,
            animation::{
                AnimationIterationCount, AnimationTiming, StyleAnimation, StyleAnimationVec,
                SvgPoint,
            },
            color::{ColorU, OptionColorU},
            length::{FloatValue, PercentageValue},
            time::CssDuration,
        },
        layout::{
            LayoutAlignSelf, LayoutFlexGrow, LayoutFlexShrink, LayoutHeight, LayoutLeft,
            LayoutPosition, LayoutTop, LayoutWidth,
        },
        property::{CssProperty, CssPropertyVec, StyleAnimationVecValue},
        style::{
            StyleBackgroundContent, StyleBackgroundContentVec, StyleOpacity, StyleTransform,
            StyleTransformVec,
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

/// Default diameter, in logical px: macOS's regular spinner and the Windows
/// 11 `ProgressRing` are both 32 (small 16, mini 10).
const DEFAULT_SIZE: isize = 32;

/// One revolution, in ms: macOS's 0.8 s per turn for the spoke wave, and
/// Windows 11's 450 degrees a second for the ring.
const CYCLE_MS: u32 = 800;

/// Number of spokes (macOS 11+ and iOS; the 12-spoke look is pre-Big Sur).
const SPOKES: usize = 8;

/// How much of the ring the arc covers, in degrees.
const ARC_SWEEP_DEG: f32 = 135.0;

/// Segments per half-circle cap and per 45 degrees of arc: fine enough that
/// the mask, rasterised at 2x, shows no facets at any native size.
const CAP_STEPS: usize = 8;
const ARC_STEPS_PER_45_DEG: usize = 8;

/// Each spoke's `@keyframes` name. `&'static` because a keyframes block is
/// looked up by name, and spoke `k` always runs the same phase.
const SPOKE_TRACKS: [&str; SPOKES] = [
    "__azul-spinner-spoke-0",
    "__azul-spinner-spoke-1",
    "__azul-spinner-spoke-2",
    "__azul-spinner-spoke-3",
    "__azul-spinner-spoke-4",
    "__azul-spinner-spoke-5",
    "__azul-spinner-spoke-6",
    "__azul-spinner-spoke-7",
];
const SPIN_TRACK: &str = "__azul-spinner-spin";
const FADE_IN_TRACK: &str = "__azul-spinner-fade-in";
const FADE_OUT_TRACK: &str = "__azul-spinner-fade-out";

/// An indeterminate busy indicator. Stateless; see the module docs for the
/// two shapes it draws.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Spinner {
    /// The indicator's diameter, in logical px.
    pub size: isize,
    /// The container's CSS, or `None` for "no opinion" - in which case the
    /// container is sized from `size` and fades in and out.
    ///
    /// `None` and `Some(empty)` are different answers: the first means the
    /// widget picks, the second means the caller asked for no properties at all
    /// and gets none. The indicator inside is drawn either way.
    pub spinner_style: OptionCssPropertyWithConditionsVec,
    /// Which native indicator to draw; `Auto` lets the theme pick.
    pub indicator: SpinnerStyle,
    /// The widget theme, or `None` for the default
    /// (`crate::widgets::themes::UiTheme::default()`, Flat).
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// The indicator's ink - the spokes, or the ring's arc - or `None` for the
    /// native one: pure black / white spokes (flora: its ink), the desktop
    /// accent ring (flora: its accent stone). A colour given here is used in
    /// both the light and the dark theme.
    pub color: OptionColorU,
    /// The ring's track, drawn under the arc, or `None` for none (the Windows
    /// ring has none). The spokes have no track.
    pub track_color: OptionColorU,
}

/// What a theme decides about a spinner; [`build`] turns it and the widget's
/// state into the DOM. Built by `themes::flat::spinner` and
/// `themes::flora::spinner`.
pub(crate) struct SpinnerLook {
    /// What [`SpinnerStyle::Auto`] draws under this theme.
    pub auto: SpinnerStyle,
    /// The spokes' native ink: the light layer, and its dark twin if it has
    /// one.
    pub spoke_ink: (StyleBackgroundContent, Option<StyleBackgroundContent>),
    /// The ring's native arc: the light layer, and its dark twin if it has
    /// one.
    pub arc_ink: (StyleBackgroundContent, Option<StyleBackgroundContent>),
    /// How long the show / hide fade takes, in ms.
    pub fade_ms: u32,
    /// The theme's marker class on the root, if it has one.
    pub marker: Option<&'static str>,
}

/// The container: sized from `size`, never grown or stretched, and the
/// positioning context the full-size parts are laid over.
fn build_container_style(size: isize) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
        // Hug its own size inside a flex parent rather than stretch/grow.
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(size))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(size))),
    ])
}

impl Spinner {
    /// Creates a new spinner with the default size (32px) and the native ink.
    #[inline]
    #[must_use]
    pub const fn create() -> Self {
        Self::with_size(DEFAULT_SIZE)
    }

    /// Creates a new spinner with the given diameter (logical px) and the
    /// native ink.
    #[inline]
    #[must_use]
    pub const fn with_size(size: isize) -> Self {
        Self {
            size,
            spinner_style: OptionCssPropertyWithConditionsVec::None,
            indicator: SpinnerStyle::Auto,
            theme: crate::widgets::themes::OptionUiTheme::None,
            color: OptionColorU::None,
            track_color: OptionColorU::None,
        }
    }

    /// The container CSS this spinner renders with.
    ///
    /// `None` means no opinion, so the size decides - the same answer both
    /// themes give, asked in one place so they cannot drift.
    #[must_use]
    pub fn resolved_spinner_style(&self) -> CssPropertyWithConditionsVec {
        self.spinner_style
            .clone()
            .into_option()
            .unwrap_or_else(|| build_container_style(self.size))
    }

    /// Sets the diameter (logical px).
    #[inline]
    pub const fn set_size(&mut self, size: isize) {
        self.size = size;
    }

    /// Builder-style setter for the diameter.
    #[inline]
    #[must_use]
    pub const fn with_spinner_size(mut self, size: isize) -> Self {
        self.set_size(size);
        self
    }

    /// Sets the indicator's ink (the spokes, or the ring's arc), in both the
    /// light and the dark theme.
    #[inline]
    pub const fn set_color(&mut self, color: ColorU) {
        self.color = OptionColorU::Some(color);
    }

    /// Builder-style setter for the indicator's ink.
    #[inline]
    #[must_use]
    pub const fn with_color(mut self, color: ColorU) -> Self {
        self.set_color(color);
        self
    }

    /// Sets the ring's track colour (drawn under the arc).
    #[inline]
    pub const fn set_track_color(&mut self, track_color: ColorU) {
        self.track_color = OptionColorU::Some(track_color);
    }

    /// Builder-style setter for the ring's track colour.
    #[inline]
    #[must_use]
    pub const fn with_track_color(mut self, track_color: ColorU) -> Self {
        self.set_track_color(track_color);
        self
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

    /// Replaces `self` with a default spinner and returns the original.
    #[inline]
    #[must_use]
    pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
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

// ---------------------------------------------------------------------------
// The build: one function, two looks
// ---------------------------------------------------------------------------

/// The spinner's DOM in `look`: the container (user space, fade, keyframes)
/// and the indicator's parts inside it.
pub(crate) fn build(s: Spinner, look: &SpinnerLook) -> Dom {
    let style = match s.indicator {
        SpinnerStyle::Auto => look.auto,
        chosen => chosen,
    };
    // A look never answers `Auto` with `Auto`; if one did, the ring is the
    // indicator every platform family can show.
    let style = if style == SpinnerStyle::Auto {
        SpinnerStyle::Ring
    } else {
        style
    };
    // Geometry of a negative size is an empty box; the CSS keeps the value.
    #[allow(clippy::cast_precision_loss)] // a spinner is far below 2^24 px
    let d = s.size.max(0) as f32;

    let owns_container = s.spinner_style.as_ref().is_none();
    let mut container: Vec<CssPropertyWithConditions> =
        s.resolved_spinner_style().as_slice().to_vec();
    if owns_container {
        container.push(motion(CssProperty::AnimationIn(animation(
            FADE_IN_TRACK,
            look.fade_ms,
            AnimationIterationCount::Count(1),
            AnimationTiming::EaseOut,
        ))));
        container.push(motion(CssProperty::AnimationOut(animation(
            FADE_OUT_TRACK,
            look.fade_ms,
            AnimationIterationCount::Count(1),
            AnimationTiming::EaseIn,
        ))));
    }

    let mut tracks = alloc::vec![
        fade_track(FADE_IN_TRACK, 0.0, 1.0),
        fade_track(FADE_OUT_TRACK, 1.0, 0.0),
    ];
    let mut parts: Vec<Dom> = Vec::new();
    let shape_class = match style {
        SpinnerStyle::Spokes => {
            let ink = match s.color.into_option() {
                Some(c) => (StyleBackgroundContent::Color(c), None),
                None => look.spoke_ink.clone(),
            };
            for k in 0..SPOKES {
                tracks.push(spoke_track(k));
                parts.push(part(
                    "__azul-spinner-spoke",
                    s.size,
                    ink.clone(),
                    Some(spoke_opacity(k, 0)),
                    Some(SPOKE_TRACKS[k]),
                    spoke_shape(d, k),
                ));
            }
            "__azul-spinner-spokes"
        }
        SpinnerStyle::Ring | SpinnerStyle::Auto => {
            tracks.push(spin_track());
            if let Some(track) = s.track_color.into_option() {
                parts.push(part(
                    "__azul-spinner-track",
                    s.size,
                    (StyleBackgroundContent::Color(track), None),
                    None,
                    None,
                    track_shape(d),
                ));
            }
            let ink = match s.color.into_option() {
                Some(c) => (StyleBackgroundContent::Color(c), None),
                None => look.arc_ink.clone(),
            };
            parts.push(part(
                "__azul-spinner-arc",
                s.size,
                ink,
                None,
                Some(SPIN_TRACK),
                arc_shape(d),
            ));
            "__azul-spinner-ring"
        }
    };

    let mut classes = alloc::vec![
        Class(AzString::from_const_str("__azul-native-spinner")),
        Class(AzString::from_const_str(shape_class)),
    ];
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }

    let mut keyframes = Css::empty();
    keyframes.keyframes = KeyframesVec::from_vec(tracks);

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container))
        // The user space every part's clip path is drawn in: one unit per px.
        .with_svg_data(SvgNodeData::ViewBox {
            min_x: 0.0,
            min_y: 0.0,
            width: d,
            height: d,
        })
        .with_component_css(keyframes)
        .with_children(parts.into())
}

/// Only where the reader has not asked for less motion.
const NO_REDUCED_MOTION: &[DynamicSelector] =
    &[DynamicSelector::PrefersReducedMotion(BoolCondition::False)];

/// A motion declaration, gated on `prefers-reduced-motion: no-preference`.
fn motion(property: CssProperty) -> CssPropertyWithConditions {
    CssPropertyWithConditions::with_single_condition(property, NO_REDUCED_MOTION)
}

/// One entry of `-azul-animation-in` / `-out`.
fn animation(
    track: &'static str,
    millis: u32,
    iterations: AnimationIterationCount,
    timing: AnimationTiming,
) -> StyleAnimationVecValue {
    StyleAnimationVecValue::Exact(StyleAnimationVec::from_vec(alloc::vec![StyleAnimation {
        name: AzString::from_const_str(track),
        duration: CssDuration::from_millis(millis),
        delay: CssDuration::from_millis(0),
        iterations,
        timing,
        clip: true,
    }]))
}

/// One full-size part of the indicator: absolutely placed over the
/// container, painted in `ink` (with its dark twin), clipped to `shape`, at a
/// resting `opacity` and running `track` (both optional).
fn part(
    class: &'static str,
    size: isize,
    ink: (StyleBackgroundContent, Option<StyleBackgroundContent>),
    opacity: Option<f32>,
    track: Option<&'static str>,
    shape: SvgMultiPolygon,
) -> Dom {
    let fill = |layer: StyleBackgroundContent| {
        CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(alloc::vec![
            layer
        ]))
    };
    let mut style = alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
        CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(size))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(size))),
        CssPropertyWithConditions::simple(fill(ink.0)),
    ];
    if let Some(dark) = ink.1 {
        style.push(CssPropertyWithConditions::dark_theme(fill(dark)));
    }
    if let Some(o) = opacity {
        style.push(CssPropertyWithConditions::simple(opacity_property(o)));
    }
    if let Some(track) = track {
        style.push(motion(CssProperty::AnimationIn(animation(
            track,
            CYCLE_MS,
            AnimationIterationCount::Infinite,
            AnimationTiming::Linear,
        ))));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
            AzString::from_const_str(class)
        )]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
        .with_svg_clip_path(shape)
}

// ---------------------------------------------------------------------------
// Motion: the spoke wave, the spin, the fades
// ---------------------------------------------------------------------------

/// Spoke `k`'s opacity at `permille` of the cycle.
///
/// The spoke is the head at `k / 8` of the cycle, at 0.55; it fades linearly
/// to 0.06 over the next 7/8 of a cycle, then rises back to 0.55 in the last
/// eighth as the head comes round again (reference section 3.1, "per-spoke
/// opacity curve"). In thousandths, so the stops are exact.
const fn spoke_opacity_milli(k: usize, permille: u32) -> u32 {
    #[allow(clippy::cast_possible_truncation)] // k < 8
    let phase = (k as u32) * 1000 / (SPOKES as u32);
    let x = (permille % 1000 + 1000 - phase) % 1000;
    if x <= 875 {
        550 - 490 * x / 875
    } else {
        60 + 490 * (x - 875) / 125
    }
}

/// [`spoke_opacity_milli`] as a fraction.
#[allow(clippy::cast_precision_loss)] // at most 550
fn spoke_opacity(k: usize, permille: u32) -> f32 {
    spoke_opacity_milli(k, permille) as f32 / 1000.0
}

fn opacity_property(o: f32) -> CssProperty {
    CssProperty::const_opacity(StyleOpacity {
        inner: PercentageValue::new(o * 100.0),
    })
}

fn stop(permille: u16, props: Vec<CssProperty>) -> KeyframeStop {
    KeyframeStop {
        permille,
        props: CssPropertyVec::from_vec(props),
    }
}

/// Spoke `k`'s phase of the wave: stops at the cycle's ends, the moment it
/// is the head and the moment it bottoms out - the curve is linear between.
fn spoke_track(k: usize) -> Keyframes {
    #[allow(clippy::cast_possible_truncation)] // k < 8
    let head = (k as u32) * 1000 / (SPOKES as u32);
    let trough = (head + 875) % 1000;
    let mut at: Vec<u32> = alloc::vec![0, head, trough, 1000];
    at.sort_unstable();
    at.dedup();
    Keyframes {
        name: AzString::from_const_str(SPOKE_TRACKS[k]),
        stops: KeyframeStopVec::from_vec(
            at.into_iter()
                .map(|p| {
                    #[allow(clippy::cast_possible_truncation)] // p <= 1000
                    let permille = p as u16;
                    stop(permille, alloc::vec![opacity_property(spoke_opacity(k, p))])
                })
                .collect(),
        ),
    }
}

/// One clockwise turn.
fn spin_track() -> Keyframes {
    let rotate = |deg: isize| {
        CssProperty::const_transform(StyleTransformVec::from_vec(alloc::vec![
            StyleTransform::Rotate(AngleValue::const_deg(deg))
        ]))
    };
    Keyframes {
        name: AzString::from_const_str(SPIN_TRACK),
        stops: KeyframeStopVec::from_vec(alloc::vec![
            stop(0, alloc::vec![rotate(0)]),
            stop(1000, alloc::vec![rotate(360)]),
        ]),
    }
}

/// An opacity ramp from `from` to `to`.
fn fade_track(name: &'static str, from: f32, to: f32) -> Keyframes {
    Keyframes {
        name: AzString::from_const_str(name),
        stops: KeyframeStopVec::from_vec(alloc::vec![
            stop(0, alloc::vec![opacity_property(from)]),
            stop(1000, alloc::vec![opacity_property(to)]),
        ]),
    }
}

// ---------------------------------------------------------------------------
// Shapes, in the container's user space (one unit per px, origin top left)
// ---------------------------------------------------------------------------

/// The point `r` from the centre `c` (both axes), `deg` degrees clockwise from
/// 12 o'clock.
fn polar(c: f32, r: f32, deg: f32) -> SvgPoint {
    let (s, co) = deg.to_radians().sin_cos();
    SvgPoint {
        x: c + r * s,
        y: c - r * co,
    }
}

/// A closed polygon through `points`.
fn closed(points: &[SvgPoint]) -> SvgPath {
    let n = points.len();
    let items: Vec<SvgPathElement> = (0..n)
        .map(|i| SvgPathElement::Line(SvgLine::new(points[i], points[(i + 1) % n])))
        .collect();
    SvgPath::create(SvgPathElementVec::from_vec(items))
}

/// Half a circle of radius `h` around `centre`, from `centre + h*a` through
/// `centre + h*b` to `centre - h*a`, endpoints included.
fn half_circle(centre: SvgPoint, h: f32, a: (f32, f32), b: (f32, f32), out: &mut Vec<SvgPoint>) {
    for i in 0..=CAP_STEPS {
        #[allow(clippy::cast_precision_loss)] // i <= 8
        let phi = core::f32::consts::PI * i as f32 / CAP_STEPS as f32;
        let (sp, cp) = phi.sin_cos();
        out.push(SvgPoint {
            x: centre.x + h * (cp * a.0 + sp * b.0),
            y: centre.y + h * (cp * a.1 + sp * b.1),
        });
    }
}

/// Spoke `k` of a `d`-wide spinner: a capsule D/8 wide from 13/64 D (0.40 of
/// the radius) to the rim, `k * 45` degrees clockwise from 12 o'clock.
pub(crate) fn spoke_shape(d: f32, k: usize) -> SvgMultiPolygon {
    #[allow(clippy::cast_precision_loss)] // k < 8
    let deg = k as f32 * 360.0 / SPOKES as f32;
    let c = d / 2.0;
    let h = d / 16.0;
    let (r_in, r_out) = (d * 13.0 / 64.0, d / 2.0);
    let (s, co) = deg.to_radians().sin_cos();
    let out = (s, -co); // away from the centre
    let side = (co, s); // clockwise across the spoke
    let inner_cap = polar(c, r_in + h, deg);
    let outer_cap = polar(c, r_out - h, deg);
    let mut points = Vec::with_capacity(2 * (CAP_STEPS + 1));
    half_circle(outer_cap, h, side, out, &mut points);
    half_circle(inner_cap, h, (-side.0, -side.1), (-out.0, -out.1), &mut points);
    SvgMultiPolygon::create(SvgPathVec::from_vec(alloc::vec![closed(&points)]))
}

/// The Windows ring's centre-line radius and half its stroke, for diameter `d`.
fn ring_metrics(d: f32) -> (f32, f32) {
    (d * 0.4375, d * 0.093_75 / 2.0)
}

/// The arc: [`ARC_SWEEP_DEG`] of the ring from 12 o'clock clockwise, with
/// round caps at both ends.
pub(crate) fn arc_shape(d: f32) -> SvgMultiPolygon {
    let c = d / 2.0;
    let (rc, h) = ring_metrics(d);
    let steps = ARC_STEPS_PER_45_DEG * 3;
    #[allow(clippy::cast_precision_loss)] // small counts
    let angle = |i: usize| ARC_SWEEP_DEG * i as f32 / steps as f32;
    let mut points = Vec::with_capacity(2 * (steps + CAP_STEPS + 2));
    for i in 0..=steps {
        points.push(polar(c, rc + h, angle(i)));
    }
    // The leading cap: from the outer edge, forward, to the inner edge.
    let end = ARC_SWEEP_DEG.to_radians();
    let (se, ce) = end.sin_cos();
    half_circle(polar(c, rc, ARC_SWEEP_DEG), h, (se, -ce), (ce, se), &mut points);
    for i in (0..=steps).rev() {
        points.push(polar(c, rc - h, angle(i)));
    }
    // The trailing cap at 12 o'clock: from the inner edge, back, to the outer.
    half_circle(polar(c, rc, 0.0), h, (0.0, 1.0), (-1.0, 0.0), &mut points);
    SvgMultiPolygon::create(SvgPathVec::from_vec(alloc::vec![closed(&points)]))
}

/// The track: the whole ring, the outer edge clockwise and the inner edge
/// counter-clockwise so the centre is a hole under the nonzero rule.
pub(crate) fn track_shape(d: f32) -> SvgMultiPolygon {
    let c = d / 2.0;
    let (rc, h) = ring_metrics(d);
    let n = ARC_STEPS_PER_45_DEG * 8;
    #[allow(clippy::cast_precision_loss)] // small counts
    let angle = |i: usize| 360.0 * i as f32 / n as f32;
    let outer: Vec<SvgPoint> = (0..n).map(|i| polar(c, rc + h, angle(i))).collect();
    let inner: Vec<SvgPoint> = (0..n).rev().map(|i| polar(c, rc - h, angle(i))).collect();
    SvgMultiPolygon::create(SvgPathVec::from_vec(alloc::vec![
        closed(&outer),
        closed(&inner),
    ]))
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use crate::widgets::themes::{OptionUiTheme, UiTheme};

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

    #[test]
    fn create_is_the_documented_default() {
        let s = Spinner::create();
        assert_eq!(s.size, DEFAULT_SIZE);
        assert_eq!(s.color, OptionColorU::None, "the native ink");
        assert_eq!(s.track_color, OptionColorU::None, "no track");
        assert_eq!(s.indicator, SpinnerStyle::Auto);
        assert_eq!(s.theme, OptionUiTheme::None);
        assert_eq!(s.spinner_style, OptionCssPropertyWithConditionsVec::None);
        assert_eq!(Spinner::default(), s);
    }

    #[test]
    fn every_setter_touches_only_its_own_field() {
        let base = Spinner::create();
        let mut s = base.clone();
        s.set_color(RED);
        assert_eq!(s.color, OptionColorU::Some(RED));
        assert_eq!(s.track_color, base.track_color);
        s.set_track_color(GREEN);
        assert_eq!(s.track_color, OptionColorU::Some(GREEN));
        assert_eq!(s.color, OptionColorU::Some(RED), "set_track_color clobbered the ink");
        s.set_size(48);
        s.set_indicator(SpinnerStyle::Spokes);
        s.set_theme(UiTheme::Flora);
        assert_eq!(
            s,
            Spinner::with_size(48)
                .with_color(RED)
                .with_track_color(GREEN)
                .with_indicator(SpinnerStyle::Spokes)
                .with_theme(UiTheme::Flora),
            "the builders and the setters agree"
        );
    }

    #[test]
    fn equality_distinguishes_every_field() {
        let base = Spinner::create();
        assert_ne!(base, Spinner::create().with_spinner_size(25));
        assert_ne!(base, Spinner::create().with_color(RED));
        assert_ne!(base, Spinner::create().with_track_color(RED));
        assert_ne!(base, Spinner::create().with_indicator(SpinnerStyle::Ring));
        assert_ne!(base, Spinner::create().with_theme(UiTheme::Flat));
        assert_eq!(base, Spinner::create().with_spinner_size(DEFAULT_SIZE));
    }

    #[test]
    fn swap_with_default_returns_the_original_and_installs_a_default() {
        let mut s = Spinner::with_size(96).with_color(RED);
        let expected = s.clone();
        assert_eq!(s.swap_with_default(), expected);
        assert_eq!(s, Spinner::create());
    }

    #[test]
    fn the_container_is_sized_and_never_grows() {
        let style = Spinner::with_size(20).resolved_spinner_style();
        let props: Vec<&CssProperty> = style.as_ref().iter().map(|p| &p.property).collect();
        assert!(props.contains(&&CssProperty::const_width(LayoutWidth::const_px(20))));
        assert!(props.contains(&&CssProperty::const_height(LayoutHeight::const_px(20))));
        assert!(props.contains(&&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))));
        assert!(
            props.contains(&&CssProperty::const_position(LayoutPosition::Relative)),
            "the parts are laid over it"
        );
    }

    #[test]
    fn the_wave_is_continuous_across_the_cycle_boundary() {
        for k in 0..SPOKES {
            assert_eq!(
                spoke_opacity_milli(k, 0),
                spoke_opacity_milli(k, 1000),
                "spoke {k} jumps when the cycle repeats"
            );
            let values: Vec<u32> = (0..=1000).step_by(25).map(|p| spoke_opacity_milli(k, p)).collect();
            assert_eq!(values.iter().max(), Some(&550), "spoke {k}");
            assert_eq!(values.iter().min(), Some(&60), "spoke {k}");
        }
    }

    #[test]
    fn a_negative_or_zero_size_builds_without_panicking() {
        for size in [0, 1, -24] {
            for style in [SpinnerStyle::Spokes, SpinnerStyle::Ring] {
                let _ = Spinner::with_size(size)
                    .with_indicator(style)
                    .with_track_color(RED)
                    .dom();
            }
        }
    }

    #[test]
    fn the_shapes_stay_inside_the_box() {
        let d = 32.0_f32;
        let mut shapes: Vec<SvgMultiPolygon> = (0..SPOKES).map(|k| spoke_shape(d, k)).collect();
        shapes.push(arc_shape(d));
        shapes.push(track_shape(d));
        for (i, shape) in shapes.iter().enumerate() {
            let b = shape.get_bounds();
            assert!(
                b.x >= -0.01 && b.y >= -0.01 && b.x + b.width <= d + 0.01 && b.y + b.height <= d + 0.01,
                "shape {i} leaves the box: {b:?}"
            );
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
