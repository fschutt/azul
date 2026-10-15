//! A pie marker: how far something has got (a sync, a drive's fullness) as
//! an 11px disc with a wedge, coloured by what the number means.
//!
//! The disc is the pie's ground in a faint ring; the wedge grows clockwise
//! from 12 o'clock by the fraction. The colour is the tone: the accent (by
//! night the accent's glow), green when done, amber when it waits or
//! nears a limit, red when something needs the user. A busy pie breathes
//! (its opacity 1 -> .72 -> 1 over 2.4 s), unless the reader asked for
//! less motion. The marker stands in a 15 x 25 hit area that lights under
//! the pointer and is a button: a click opens what the app shows about it,
//! and its label is what a screen reader says (the numbers in words).
//!
//! The shapes are polygons the node is clipped to and painted through
//! (`Dom::with_svg_clip_path`), in the user space the disc declares
//! (`SvgNodeData::ViewBox`, one unit per px).

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData},
    dom::{Dom, EventFilter, HoverEventFilter, IdOrClass::Class, IdOrClassVec, SvgNodeData, TabIndex},
    refany::RefAny,
    svg::{SvgMultiPolygon, SvgPathVec},
};
use azul_css::{
    css::{Css, KeyframeStop, KeyframeStopVec, Keyframes, KeyframesVec},
    dynamic_selector::{
        BoolCondition, CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelector,
    },
    props::{
        basic::{
            animation::{AnimationIterationCount, StyleAnimation, StyleAnimationVec, SvgPoint},
            color::ColorU,
            length::{FloatValue, PercentageValue},
            time::CssDuration,
        },
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutDisplay, LayoutFlexShrink, LayoutHeight,
            LayoutJustifyContent, LayoutLeft, LayoutPosition, LayoutTop, LayoutWidth,
        },
        property::{CssProperty, CssPropertyVec, StyleAnimationVecValue},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleCursor, StyleOpacity},
    },
    AzString,
};

use crate::widgets::{
    button::{ButtonOnClick, ButtonOnClickCallback, OptionButtonOnClick},
    spinner::{closed, polar},
    themes::{decl, flora},
};

/// What a pie's number means, which is its colour.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum PieTone {
    /// Just how far it is (a drive's fullness): the accent.
    #[default]
    Accent,
    /// Working (a sync in progress): the accent, breathing.
    Busy,
    /// Done (up to date): green.
    Good,
    /// Waiting or near a limit (paused, metered, more than 85% full): amber.
    Warning,
    /// Needs the user (a conflict, more than 97% full): red.
    Danger,
}

/// A pie marker ([`PieMarker::dom`]).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct PieMarker {
    /// How far it is, 0 to 1 (clamped when drawn): the wedge's share of the
    /// disc.
    pub fraction: f32,
    /// What the number means: the colour, and whether the pie breathes.
    pub tone: PieTone,
    /// What a screen reader says for the marker: the numbers in words
    /// ("Syncing, 12 of 40 files"). Empty: the marker has no name.
    pub label: AzString,
    /// What a click on the marker runs (the app opens its popover there).
    pub on_click: OptionButtonOnClick,
}

/// The marker's hit area (a button).
pub const PIE_MARKER_CLASS: &str = "__azul-pie-marker";
/// The 11px disc inside it, which breathes when busy.
pub const PIE_MARKER_DISC_CLASS: &str = "__azul-pie-marker-disc";
/// The wedge.
pub const PIE_MARKER_WEDGE_CLASS: &str = "__azul-pie-marker-wedge";

/// The disc's size, in px.
pub const PIE_SIZE: f32 = 11.0;
/// The hit area around it, in px.
pub const PIE_HIT_WIDTH: isize = 15;
/// The hit area's height, in px: a row of the navigation pane's foot.
pub const PIE_HIT_HEIGHT: isize = 25;
/// How long one breath of a busy pie takes, in ms.
pub const PIE_BREATH_MS: u32 = 2400;
/// Above this share of a drive full, its pie turns amber.
pub const SPACE_WARNING: f32 = 0.85;
/// Above this share of a drive full, its pie turns red.
pub const SPACE_DANGER: f32 = 0.97;

/// The design's pie, drawn in a 12-unit box shown 11px wide: the disc is
/// r 5.6 with a .8 ring on its edge, the wedge a 6-wide stroke on r 3 (so
/// it reaches the box's edge, over the ring).
const UNIT: f32 = PIE_SIZE / 12.0;
/// The disc's ground reaches the middle of its ring.
const DISC_R: f32 = 5.6 * UNIT;
/// The ring's inner and outer edge (its outer half lies at the box's edge).
const RING: (f32, f32) = ((5.6 - 0.4) * UNIT, 6.0 * UNIT);
/// The wedge's radius.
const WEDGE_R: f32 = 6.0 * UNIT;
/// Straight runs a whole circle is drawn in: at 11px a fiftieth of a px
/// off the arc.
const CIRCLE_STEPS: usize = 72;

/// The disc's ground, by day and by night (`--pie-bg`).
const GROUND: (ColorU, ColorU) = (ColorU::rgb(0xE2, 0xDF, 0xD8), ColorU::rgb(0x3A, 0x3A, 0x3A));
/// The faint ring on its edge (`--pie-ring`).
const RING_INK: (ColorU, ColorU) = (ColorU::new(60, 50, 30, 56), ColorU::new(0, 0, 0, 128));
/// Done.
const GOOD: ColorU = ColorU::rgb(0x4E, 0x9A, 0x5B);
/// Waiting, near a limit.
const WARNING: ColorU = ColorU::rgb(0xC9, 0x96, 0x2E);
/// Needs the user.
const DANGER: ColorU = ColorU::rgb(0xC2, 0x48, 0x3A);

/// The breath's track.
const BREATH_TRACK: &str = "__azul-pie-breathe";

impl PieTone {
    /// The wedge's colour by day and by night: the accent turns to its glow
    /// at night, the others keep their colour.
    #[must_use]
    pub const fn colors(self) -> (ColorU, ColorU) {
        match self {
            Self::Accent | Self::Busy => (flora::LIGHT_ACC, flora::DARK_GLOW),
            Self::Good => (GOOD, GOOD),
            Self::Warning => (WARNING, WARNING),
            Self::Danger => (DANGER, DANGER),
        }
    }

    /// The tone of a drive `used` full (0 to 1): the accent, amber above
    /// 85%, red above 97%.
    #[must_use]
    pub fn of_space(used: f32) -> Self {
        if used > SPACE_DANGER {
            Self::Danger
        } else if used > SPACE_WARNING {
            Self::Warning
        } else {
            Self::Accent
        }
    }
}

impl PieMarker {
    /// A pie `fraction` full (0 to 1) in `tone`, unnamed, doing nothing on a
    /// click.
    #[must_use]
    pub const fn create(fraction: f32, tone: PieTone) -> Self {
        Self {
            fraction,
            tone,
            label: AzString::from_const_str(""),
            on_click: OptionButtonOnClick::None,
        }
    }

    /// The pie of a drive `used` full (0 to 1): the accent, amber above
    /// 85%, red above 97%.
    #[must_use]
    pub fn create_space(used: f32) -> Self {
        Self::create(used, PieTone::of_space(used))
    }

    /// What a screen reader says for the marker.
    pub fn set_label(&mut self, label: AzString) {
        self.label = label;
    }

    /// [`Self::set_label`] for the builder chain.
    #[must_use]
    pub fn with_label(mut self, label: AzString) -> Self {
        self.set_label(label);
        self
    }

    /// What a click on the marker runs.
    pub fn set_on_click<C: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, on_click: C) {
        self.on_click = Some(ButtonOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    /// [`Self::set_on_click`] for the builder chain.
    #[must_use]
    pub fn with_on_click<C: Into<ButtonOnClickCallback>>(mut self, data: RefAny, on_click: C) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    /// The marker: the hit area holding the disc, its ring and its wedge.
    #[must_use]
    pub fn dom(self) -> Dom {
        let fraction = if self.fraction.is_finite() {
            self.fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut parts = alloc::vec![
            layer(GROUND, &disc_shape(DISC_R)),
            layer(RING_INK, &ring_shape(RING.0, RING.1)),
        ];
        if let Some(wedge) = wedge_shape(fraction) {
            parts.push(layer(self.tone.colors(), &wedge).with_ids_and_classes(
                IdOrClassVec::from_vec(alloc::vec![Class(AzString::from_const_str(
                    PIE_MARKER_WEDGE_CLASS
                ))]),
            ));
        }
        let mut disc_style = alloc::vec![
            decl::position(LayoutPosition::Relative),
            decl::px_width(PIE_SIZE),
            decl::px_height(PIE_SIZE),
            decl::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
                inner: FloatValue::const_new(0),
            })),
        ];
        let busy = self.tone == PieTone::Busy;
        if busy {
            disc_style.push(CssPropertyWithConditions::with_single_condition(
                CssProperty::AnimationIn(StyleAnimationVecValue::Exact(StyleAnimationVec::from_vec(
                    alloc::vec![StyleAnimation {
                        name: AzString::from_const_str(BREATH_TRACK),
                        duration: CssDuration::from_millis(PIE_BREATH_MS),
                        delay: CssDuration::from_millis(0),
                        iterations: AnimationIterationCount::Infinite,
                        timing: flora::FL_EASE,
                        clip: true,
                    }],
                ))),
                NO_REDUCED_MOTION,
            ));
        }
        let mut disc = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
                AzString::from_const_str(PIE_MARKER_DISC_CLASS)
            )]))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(disc_style))
            // The user space the layers' clip paths are drawn in: one unit per px from the
            // disc's corner. Without it a clip path is read in window coordinates, and every
            // part landed beside its disc.
            .with_svg_data(SvgNodeData::ViewBox {
                min_x: 0.0,
                min_y: 0.0,
                width: PIE_SIZE,
                height: PIE_SIZE,
            })
            .with_children(parts.into());
        if busy {
            disc = disc.with_component_css(breath());
        }

        let mut hit = alloc::vec![
            decl::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
            decl::simple(CssProperty::const_display(LayoutDisplay::Flex)),
            decl::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
            decl::simple(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
            decl::simple(CssProperty::const_width(LayoutWidth::const_px(PIE_HIT_WIDTH))),
            decl::simple(CssProperty::const_height(LayoutHeight::const_px(PIE_HIT_HEIGHT))),
            decl::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
                inner: FloatValue::const_new(0),
            })),
            decl::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        ];
        hit.extend(decl::radius(3));
        hit.extend(decl::hover_fill(flora::LIGHT_HB, flora::DARK_HB));
        hit.extend(decl::focus_halo_inset(flora::LIGHT_ACC, flora::DARK_GLOW));
        let mut a11y = azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::PushButton,
            ..Default::default()
        };
        if !self.label.as_str().is_empty() {
            a11y.accessibility_name = azul_css::OptionString::Some(self.label);
        }
        let mut marker = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
                AzString::from_const_str(PIE_MARKER_CLASS)
            )]))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(hit))
            .with_accessibility_info(a11y)
            .with_tab_index(TabIndex::Auto)
            .with_child(disc);
        if let Some(click) = self.on_click.into_option() {
            marker = marker.with_callbacks(
                alloc::vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    callback: CoreCallback {
                        cb: click.callback.cb as *const () as usize,
                        ctx: click.callback.ctx,
                    },
                    refany: click.refany,
                }]
                .into(),
            );
        }
        marker
    }
}

/// Only where the reader has not asked for less motion.
const NO_REDUCED_MOTION: &[DynamicSelector] =
    &[DynamicSelector::PrefersReducedMotion(BoolCondition::False)];

/// One full-size layer of the disc: painted in `ink` (with its night
/// twin), clipped to `shape`.
fn layer(ink: (ColorU, ColorU), shape: &SvgMultiPolygon) -> Dom {
    let fill = |c: ColorU| {
        CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(alloc::vec![
            StyleBackgroundContent::Color(c)
        ]))
    };
    let mut style = alloc::vec![
        decl::position(LayoutPosition::Absolute),
        decl::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        decl::simple(CssProperty::const_top(LayoutTop::const_px(0))),
        decl::px_width(PIE_SIZE),
        decl::px_height(PIE_SIZE),
    ];
    style.extend(CssPropertyWithConditions::themed(fill(ink.0), fill(ink.1)));
    Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
        .with_svg_clip_path(shape.clone())
}

/// The centre of the disc, both axes.
const CENTRE: f32 = PIE_SIZE / 2.0;

/// A circle of radius `r` around the disc's centre.
fn circle(r: f32) -> Vec<SvgPoint> {
    #[allow(clippy::cast_precision_loss)] // 72 steps
    (0..CIRCLE_STEPS)
        .map(|i| polar(CENTRE, r, 360.0 * i as f32 / CIRCLE_STEPS as f32))
        .collect()
}

/// The disc's ground.
pub(crate) fn disc_shape(r: f32) -> SvgMultiPolygon {
    SvgMultiPolygon::create(SvgPathVec::from_vec(alloc::vec![closed(&circle(r))]))
}

/// The ring between `inner` and `outer`: one outline that runs round the
/// outer edge and back round the inner one, so it fills as a ring under
/// either fill rule.
pub(crate) fn ring_shape(inner: f32, outer: f32) -> SvgMultiPolygon {
    let mut points = circle(outer);
    points.push(polar(CENTRE, outer, 0.0));
    points.push(polar(CENTRE, inner, 0.0));
    let mut back = circle(inner);
    back.reverse();
    points.extend(back);
    SvgMultiPolygon::create(SvgPathVec::from_vec(alloc::vec![closed(&points)]))
}

/// The wedge of `fraction` (0 to 1): from the centre up to 12 o'clock and
/// clockwise round by the fraction of a turn. `None` for an empty pie; a
/// full one is the whole disc.
pub(crate) fn wedge_shape(fraction: f32) -> Option<SvgMultiPolygon> {
    if fraction <= 0.0 {
        return None;
    }
    if fraction >= 1.0 {
        return Some(disc_shape(WEDGE_R));
    }
    let sweep = 360.0 * fraction;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )] // at most 72 steps
    let steps = ((CIRCLE_STEPS as f32 * fraction).ceil() as usize).max(1);
    let mut points = Vec::with_capacity(steps + 2);
    points.push(SvgPoint {
        x: CENTRE,
        y: CENTRE,
    });
    for i in 0..=steps {
        #[allow(clippy::cast_precision_loss)]
        let deg = sweep * i as f32 / steps as f32;
        points.push(polar(CENTRE, WEDGE_R, deg));
    }
    Some(SvgMultiPolygon::create(SvgPathVec::from_vec(alloc::vec![closed(&points)])))
}

/// A busy pie's breath: opacity 1 -> .72 at half time -> 1.
fn breath() -> Css {
    let stop = |permille: u16, o: f32| KeyframeStop {
        permille,
        props: CssPropertyVec::from_vec(alloc::vec![CssProperty::const_opacity(StyleOpacity {
            inner: PercentageValue::new(o * 100.0),
        })]),
    };
    let mut css = Css::empty();
    css.keyframes = KeyframesVec::from_vec(alloc::vec![Keyframes {
        name: AzString::from_const_str(BREATH_TRACK),
        stops: KeyframeStopVec::from_vec(alloc::vec![
            stop(0, 1.0),
            stop(500, 0.72),
            stop(1000, 1.0),
        ]),
    }]);
    css
}

#[cfg(test)]
mod tests {
    //! The pie against the design: an 11px disc in a 15 x 25 hit area, a
    //! wedge clockwise from 12 o'clock, the tone's colours, the space
    //! thresholds and the breath.

    use super::*;

    fn class_of(dom: &Dom, class: &str) -> bool {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| c.as_class() == Some(class))
    }

    fn find<'a>(dom: &'a Dom, class: &str) -> Option<&'a Dom> {
        if class_of(dom, class) {
            return Some(dom);
        }
        dom.children.as_ref().iter().find_map(|c| find(c, class))
    }

    fn points(shape: &SvgMultiPolygon) -> Vec<SvgPoint> {
        shape.rings.as_ref()[0]
            .items
            .as_ref()
            .iter()
            .map(|e| e.get_start())
            .collect()
    }

    #[test]
    fn the_wedge_starts_at_12_o_clock_and_turns_clockwise_by_the_fraction() {
        let wedge = wedge_shape(0.25).expect("a quarter");
        let p = points(&wedge);
        assert!((p[0].x - CENTRE).abs() < 1e-4 && (p[0].y - CENTRE).abs() < 1e-4, "from the centre");
        assert!((p[1].x - CENTRE).abs() < 1e-3 && (p[1].y - (CENTRE - WEDGE_R)).abs() < 1e-3, "up to 12");
        let last = p[p.len() - 1];
        assert!(
            (last.x - (CENTRE + WEDGE_R)).abs() < 1e-3 && (last.y - CENTRE).abs() < 1e-3,
            "a quarter turn clockwise ends at 3 o'clock: {last:?}"
        );
    }

    #[test]
    fn an_empty_pie_has_no_wedge_and_a_full_one_is_the_whole_disc() {
        assert!(wedge_shape(0.0).is_none());
        let empty = PieMarker::create(0.0, PieTone::Accent).dom();
        assert!(find(&empty, PIE_MARKER_WEDGE_CLASS).is_none());
        let full = wedge_shape(1.0).expect("full");
        assert_eq!(points(&full).len(), CIRCLE_STEPS);
    }

    #[test]
    fn a_drive_s_pie_is_the_accent_amber_above_85_and_red_above_97_percent() {
        assert_eq!(PieTone::of_space(0.85), PieTone::Accent);
        assert_eq!(PieTone::of_space(0.86), PieTone::Warning);
        assert_eq!(PieTone::of_space(0.97), PieTone::Warning);
        assert_eq!(PieTone::of_space(0.98), PieTone::Danger);
        assert_eq!(PieMarker::create_space(0.5).tone, PieTone::Accent);
    }

    #[test]
    fn the_tones_are_the_design_s_colours_and_the_accent_glows_at_night() {
        assert_eq!(PieTone::Good.colors(), (ColorU::rgb(0x4E, 0x9A, 0x5B), ColorU::rgb(0x4E, 0x9A, 0x5B)));
        assert_eq!(PieTone::Warning.colors().0, ColorU::rgb(0xC9, 0x96, 0x2E));
        assert_eq!(PieTone::Danger.colors().0, ColorU::rgb(0xC2, 0x48, 0x3A));
        assert_eq!(PieTone::Busy.colors(), (flora::LIGHT_ACC, flora::DARK_GLOW));
    }

    #[test]
    fn only_a_busy_pie_breathes() {
        let busy = PieMarker::create(0.4, PieTone::Busy).dom();
        let disc = find(&busy, PIE_MARKER_DISC_CLASS).expect("the disc");
        let breathes = disc
            .root
            .style
            .iter_inline_properties()
            .any(|(p, _)| matches!(p, CssProperty::AnimationIn(_)));
        assert!(breathes, "a busy pie breathes");
        let calm = PieMarker::create(0.4, PieTone::Good).dom();
        let disc = find(&calm, PIE_MARKER_DISC_CLASS).expect("the disc");
        assert!(
            !disc.root.style.iter_inline_properties().any(|(p, _)| matches!(p, CssProperty::AnimationIn(_))),
            "a done pie is still"
        );
    }

    #[test]
    fn the_marker_is_a_named_button_15_by_25_around_an_11px_disc() {
        let dom = PieMarker::create(0.3, PieTone::Accent)
            .with_label(AzString::from("Syncing, 12 of 40 files"))
            .dom();
        assert!(class_of(&dom, PIE_MARKER_CLASS));
        let a11y = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(a11y.role, azul_core::a11y::AccessibilityRole::PushButton);
        let px = |dom: &Dom, pick: fn(&CssProperty) -> Option<f32>| {
            dom.root.style.iter_inline_properties().filter_map(|(p, _)| pick(p)).last()
        };
        let width = |p: &CssProperty| match p {
            CssProperty::Width(v) => v.get_property().and_then(|w| match w {
                LayoutWidth::Px(px) => Some(px.number.get()),
                _ => None,
            }),
            _ => None,
        };
        let height = |p: &CssProperty| match p {
            CssProperty::Height(v) => v.get_property().and_then(|h| match h {
                LayoutHeight::Px(px) => Some(px.number.get()),
                _ => None,
            }),
            _ => None,
        };
        assert_eq!(px(&dom, width), Some(15.0));
        assert_eq!(px(&dom, height), Some(25.0));
        let disc = find(&dom, PIE_MARKER_DISC_CLASS).expect("the disc");
        assert_eq!(px(disc, width), Some(11.0));
        assert_eq!(px(disc, height), Some(11.0));
    }
}
