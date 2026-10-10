//! Level meter widget - how loud something is NOW: a microphone in a call, the music playing, a
//! track in a mixer. A trough filled from its start to the level, green, then amber past
//! [`WARM_FROM`], red past [`HOT_FROM`] - the colours a mixing desk uses - horizontal or vertical.
//!
//! The scale is DECIBELS, not amplitude: [`LEVEL_FLOOR_DB`] (-60 dB, silence for a meter) is the
//! empty trough, 0 dB (full scale) the full one, so a voice at a normal level fills it about two
//! thirds - an amplitude scale would leave every voice in the first tenth. [`rms_percent`] turns
//! samples into the level (AzMeet's meter, moved here), [`peak_percent`] a peak (what
//! `AudioPlayer::get_state` reports).
//!
//! A meter moves many times a second; rebuilding the app's DOM for each move would be waste. So
//! the fill is three segments (green, amber, red) whose sizes [`LevelMeter::update_level`] sets in
//! place on the built nodes, the accessibility value with them, and [`LevelMeterThrottle`] says
//! when a move is worth showing (at most ten times a second, by half a percent or more).
//!
//! Key types: [`LevelMeter`], [`LevelMeterThrottle`], [`LevelMeterOrientation`].

use alloc::vec::Vec;

use azul_core::{
    dom::{Dom, DomNodeId, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option,
    props::{
        basic::{length::FloatValue, pixel::PixelValue},
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink,
            LayoutHeight, LayoutMinHeight, LayoutMinWidth, LayoutOverflow, LayoutWidth,
        },
        property::{CssProperty, LayoutHeightValue, LayoutWidthValue},
        style::StyleUserSelect,
    },
    AzString, OptionF32, OptionString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::themes::{OptionUiTheme, UiTheme},
};

/// The level of an empty meter, in dB: quieter is silence as far as a meter is concerned.
pub const LEVEL_FLOOR_DB: f32 = -60.0;
/// The level (percent) the amber segment starts at (-18 dB).
pub const WARM_FROM: f32 = 70.0;
/// The level (percent) the red segment starts at (-6 dB): close to clipping.
pub const HOT_FROM: f32 = 90.0;

/// A level in dB as the meter's percent: [`LEVEL_FLOOR_DB`] and below is 0, 0 dB and above 100.
#[must_use]
pub fn db_percent(db: f32) -> f32 {
    if !db.is_finite() {
        // NaN is no level; +inf is past full scale.
        return if db > 0.0 { 100.0 } else { 0.0 };
    }
    ((db - LEVEL_FLOOR_DB) / -LEVEL_FLOOR_DB * 100.0).clamp(0.0, 100.0)
}

/// The level of `samples` (any channel layout): their RMS on the dB scale, as a percent.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn rms_percent(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    db_percent(20.0 * mean_square.sqrt().max(1e-6).log10())
}

/// A peak magnitude (`0.0..=1.0`, e.g. `AudioPlayerState::peak_left`) on the dB scale, as a
/// percent.
#[must_use]
pub fn peak_percent(peak: f32) -> f32 {
    if peak <= 0.0 || peak.is_nan() {
        return 0.0;
    }
    db_percent(20.0 * peak.log10())
}

/// The sizes (percent of the trough) of the green, amber and red segments at `level` percent.
#[must_use]
pub fn zone_widths(level: f32) -> [f32; 3] {
    let l = if level.is_finite() {
        level.clamp(0.0, 100.0)
    } else {
        0.0
    };
    [
        l.min(WARM_FROM),
        (l - WARM_FROM).clamp(0.0, HOT_FROM - WARM_FROM),
        (l - HOT_FROM).clamp(0.0, 100.0 - HOT_FROM),
    ]
}

/// Which way a meter fills.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LevelMeterOrientation {
    /// Left to right (a bar under a microphone picker).
    #[default]
    Horizontal,
    /// Bottom to top (a mixer's channel strip).
    Vertical,
}

/// A level meter: a trough filled to the level, green, amber, red.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct LevelMeter {
    /// What the meter measures, for assistive technology ("Microphone level").
    pub accessibility_name: OptionString,
    /// The widget theme this meter is pinned to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// The level, `0.0..=100.0` percent of the dB scale.
    pub level: f32,
    /// Which way it fills.
    pub orientation: LevelMeterOrientation,
}

impl LevelMeter {
    /// A horizontal meter at `level` percent (see [`rms_percent`] / [`peak_percent`]).
    #[must_use]
    pub const fn create(level: f32) -> Self {
        Self {
            accessibility_name: OptionString::None,
            theme: OptionUiTheme::None,
            level,
            orientation: LevelMeterOrientation::Horizontal,
        }
    }

    /// The level, percent.
    pub const fn set_level(&mut self, level: f32) {
        self.level = level;
    }

    /// [`Self::set_level`] for the builder chain.
    #[must_use]
    pub const fn with_level(mut self, level: f32) -> Self {
        self.set_level(level);
        self
    }

    /// Which way the meter fills.
    pub const fn set_orientation(&mut self, orientation: LevelMeterOrientation) {
        self.orientation = orientation;
    }

    /// [`Self::set_orientation`] for the builder chain.
    #[must_use]
    pub const fn with_orientation(mut self, orientation: LevelMeterOrientation) -> Self {
        self.set_orientation(orientation);
        self
    }

    /// Name the meter for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// Pin the widget theme; unset, the meter follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty meter and returns the original.
    #[must_use]
    pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(0.0);
        core::mem::swap(&mut s, self);
        s
    }

    /// The meter's DOM: the pinned theme's look, or (unpinned) both themes' looks in their
    /// `@theme` blocks, the app theme picking.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks};
        match self.theme.into_option() {
            Some(UiTheme::Flat) => flat::level_meter(self),
            Some(UiTheme::Flora) => flora::level_meter(self),
            None => theme_blocks::follow_app_theme(self, flat::level_meter, flora::level_meter),
        }
    }

    /// Moves the meter built at `node` (the root of its `dom()`) to `level` percent in place: the
    /// three segments resize and the accessibility value follows, without a rebuild. False when
    /// `node` is not a meter's root.
    pub fn update_level(info: &mut CallbackInfo, node: DomNodeId, level: f32) -> bool {
        let Some(mut data) = info.get_dataset(node) else {
            return false;
        };
        let vertical = {
            let Some(d) = data.downcast_ref::<LevelMeterData>() else {
                return false;
            };

            d.vertical
        };
        let Some(track) = info.get_first_child(node) else {
            return false;
        };
        let Some(ok) = info.get_first_child(track) else {
            return false;
        };
        let Some(warm) = info.get_next_sibling(ok) else {
            return false;
        };
        let Some(hot) = info.get_next_sibling(warm) else {
            return false;
        };
        for (segment, size) in [ok, warm, hot].into_iter().zip(zone_widths(level)) {
            info.set_css_property(segment, segment_size(vertical, size));
        }
        info.set_accessibility_value(node, AzString::from(value_text(level)));
        true
    }

    /// The level of an audio frame's samples ([`rms_percent`]), for the API.
    #[must_use]
    pub fn level_of(frame: azul_core::audio::AudioFrame) -> f32 {
        rms_percent(frame.samples.as_ref())
    }

    /// A peak magnitude (`0.0..=1.0`, e.g. `AudioPlayerState::peak_left`) as a level
    /// ([`peak_percent`]), for the API.
    #[must_use]
    pub fn peak_level(peak: f32) -> f32 {
        peak_percent(peak)
    }
}

impl Default for LevelMeter {
    fn default() -> Self {
        Self::create(0.0)
    }
}

impl From<LevelMeter> for Dom {
    fn from(m: LevelMeter) -> Self {
        m.dom()
    }
}

impl_option!(
    LevelMeter,
    OptionLevelMeter,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// When a meter's move is worth showing: at most every `interval_ms`, and by `min_step` percent
/// or more (each move repaints the meter; fifty a second, one per audio chunk, is waste).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelMeterThrottle {
    /// The shortest time between two moves.
    pub interval_ms: u64,
    /// When the meter last moved (`moved` false: never).
    pub last_ms: u64,
    /// The level shown.
    pub level: f32,
    /// The smallest move worth showing, percent.
    pub min_step: f32,
    /// The meter has moved at least once.
    pub moved: bool,
}

impl LevelMeterThrottle {
    /// A throttle that lets a move through at most every `interval_ms` (100 reads as live), by
    /// half a percent or more.
    #[must_use]
    pub const fn create(interval_ms: u64) -> Self {
        Self {
            interval_ms,
            last_ms: 0,
            level: 0.0,
            min_step: 0.5,
            moved: false,
        }
    }

    /// The level to show at `now_ms` for a measured `level`, or `None` when the meter should stay
    /// where it is (too soon, or too small a move).
    pub fn next(&mut self, level: f32, now_ms: u64) -> OptionF32 {
        if self.moved {
            if (self.level - level).abs() < self.min_step {
                return OptionF32::None;
            }
            if now_ms.saturating_sub(self.last_ms) < self.interval_ms {
                return OptionF32::None;
            }
        }
        self.moved = true;
        self.last_ms = now_ms;
        self.level = level;
        OptionF32::Some(level)
    }
}

impl Default for LevelMeterThrottle {
    fn default() -> Self {
        Self::create(100)
    }
}

/// What a theme decides about a meter: the SKIN of each part.
#[derive(Debug, Clone, Default)]
pub(crate) struct LevelMeterLook {
    /// The trough.
    pub track: Vec<CssPropertyWithConditions>,
    /// The green, amber and red segments.
    pub ok: Vec<CssPropertyWithConditions>,
    pub warm: Vec<CssPropertyWithConditions>,
    pub hot: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the root, if it has one.
    pub marker: Option<&'static str>,
}

// ==== The DOM ====

static ROOT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-level-meter"))];
static TRACK_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-level-meter-track",
))];
static OK_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-level-meter-ok",
))];
static WARM_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-level-meter-warm",
))];
static HOT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-level-meter-hot",
))];

/// What `update_level` needs to know of a built meter.
struct LevelMeterData {
    vertical: bool,
}

/// The thickness of the trough, px.
const THICKNESS: isize = 8;

const fn simple(p: CssProperty) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(p)
}

/// A horizontal meter: one row, the trough across it, at least 40 px long.
pub(crate) static LEVEL_METER_ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(40))),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A vertical meter: one column, the trough down it, at least 40 px tall.
pub(crate) static LEVEL_METER_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(40))),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The horizontal trough: the meter's length, `THICKNESS` thick, the segments left to right.
pub(crate) static LEVEL_TRACK_ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_height(LayoutHeight::const_px(THICKNESS))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

/// The vertical trough: the meter's height, `THICKNESS` wide, the segments bottom to top.
pub(crate) static LEVEL_TRACK_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::ColumnReverse,
    )),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_width(LayoutWidth::const_px(THICKNESS))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

/// A segment keeps the size it is given.
pub(crate) static LEVEL_SEGMENT_BASE: &[CssPropertyWithConditions] =
    &[simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    }))];

/// A segment's size along the trough: a width across a horizontal meter, a height up a vertical
/// one, percent of the trough.
fn segment_size(vertical: bool, percent: f32) -> CssProperty {
    if vertical {
        CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight::Px(
            PixelValue::percent(percent),
        )))
    } else {
        CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth::Px(
            PixelValue::percent(percent),
        )))
    }
}

/// The level as a screen reader says it ("42%"; an unknown level reads 0).
fn value_text(level: f32) -> String {
    let l = if level.is_finite() {
        level.clamp(0.0, 100.0)
    } else {
        0.0
    };
    alloc::format!("{l:.0}%")
}

/// The meter's DOM in `look`: root > trough > green, amber, red.
pub(crate) fn build(meter: LevelMeter, look: &LevelMeterLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let vertical = meter.orientation == LevelMeterOrientation::Vertical;
    let sizes = zone_widths(meter.level);
    let segment = |class: &'static [IdOrClass], skin: &[CssPropertyWithConditions], size: f32| {
        let mut props = crate::widgets::themes::decl::on_base(LEVEL_SEGMENT_BASE, skin);
        props.push(simple(segment_size(vertical, size)));
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(class))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(props))
    };
    let track_base = if vertical {
        LEVEL_TRACK_COLUMN_BASE
    } else {
        LEVEL_TRACK_ROW_BASE
    };
    let track = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(TRACK_CLASS))
        .with_css_props(part(track_base, &look.track))
        .with_children(DomVec::from_vec(alloc::vec![
            segment(OK_CLASS, &look.ok, sizes[0]),
            segment(WARM_CLASS, &look.warm, sizes[1]),
            segment(HOT_CLASS, &look.hot, sizes[2]),
        ]));
    let mut classes: Vec<IdOrClass> = ROOT_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    let root_base = if vertical {
        LEVEL_METER_COLUMN_BASE
    } else {
        LEVEL_METER_ROW_BASE
    };
    let data = RefAny::new(LevelMeterData { vertical });
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(root_base, &[]))
        .with_dataset(Some(data).into())
        // A meter is a VALUE: "42%" says what the coloured boxes show.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::ProgressBar,
            accessibility_name: meter.accessibility_name.clone(),
            accessibility_value: Some(AzString::from(value_text(meter.level))).into(),
            ..Default::default()
        })
        .with_child(track)
}

#[cfg(test)]
mod level_meter_tests {
    use azul_core::dom::NodeType;

    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, theme_checks};

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.05
    }

    #[test]
    fn the_scale_is_decibels_from_minus_sixty_to_full_scale() {
        assert!(close(db_percent(-60.0), 0.0));
        assert!(close(db_percent(-90.0), 0.0), "below the floor is empty");
        assert!(close(db_percent(-30.0), 50.0));
        assert!(close(db_percent(0.0), 100.0));
        assert!(close(db_percent(6.0), 100.0), "above full scale is full");
        assert!(close(peak_percent(1.0), 100.0));
        assert!(close(peak_percent(0.001), 0.0), "-60 dB");
        assert!(close(peak_percent(0.0), 0.0), "silence");
        assert!(close(peak_percent(10f32.powf(-30.0 / 20.0)), 50.0));
    }

    #[test]
    fn the_level_of_samples_is_their_rms_on_the_db_scale() {
        assert!(close(rms_percent(&[]), 0.0));
        assert!(close(rms_percent(&[0.0; 480]), 0.0));
        // A full-scale square wave: RMS 1.0 = 0 dB.
        let square: Vec<f32> = (0..480)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        assert!(close(rms_percent(&square), 100.0));
        // RMS 0.1 = -20 dB = two thirds of the trough.
        let tenth: Vec<f32> = (0..480)
            .map(|i| if i % 2 == 0 { 0.1 } else { -0.1 })
            .collect();
        assert!((rms_percent(&tenth) - 200.0 / 3.0).abs() < 0.1);
    }

    #[test]
    fn green_fills_first_then_amber_then_red() {
        assert_eq!(zone_widths(0.0), [0.0, 0.0, 0.0]);
        assert_eq!(zone_widths(50.0), [50.0, 0.0, 0.0]);
        assert_eq!(zone_widths(80.0), [70.0, 10.0, 0.0]);
        assert_eq!(zone_widths(95.0), [70.0, 20.0, 5.0]);
        assert_eq!(
            zone_widths(140.0),
            [70.0, 20.0, 10.0],
            "clamped to the trough"
        );
        assert_eq!(
            zone_widths(f32::NAN),
            [0.0, 0.0, 0.0],
            "an unknown level is empty"
        );
    }

    #[test]
    fn a_meter_moves_at_most_every_interval_and_by_half_a_percent_or_more() {
        let mut t = LevelMeterThrottle::create(100);
        assert_eq!(
            t.next(50.0, 1_000).into_option(),
            Some(50.0),
            "the first move shows"
        );
        assert_eq!(t.next(60.0, 1_050).into_option(), None, "too soon");
        assert_eq!(t.next(60.0, 1_100).into_option(), Some(60.0));
        assert_eq!(t.next(60.3, 1_300).into_option(), None, "too small a move");
        assert_eq!(t.next(20.0, 1_400).into_option(), Some(20.0));
        assert!(close(t.level, 20.0));
    }

    /// The root, the trough and the three segments of a built meter.
    fn segments(dom: &Dom) -> (&Dom, [&Dom; 3]) {
        let track = &dom.children.as_ref()[0];
        let s = track.children.as_ref();
        (track, [&s[0], &s[1], &s[2]])
    }

    #[test]
    fn the_meter_is_a_trough_of_three_segments_sized_by_the_level() {
        for theme in checks::BOTH {
            let dom = LevelMeter::create(95.0).with_theme(theme).dom();
            assert!(theme_checks::has_class(&dom, "__azul-native-level-meter"));
            let (track, [ok, warm, hot]) = segments(&dom);
            assert!(theme_checks::has_class(
                track,
                "__azul-native-level-meter-track"
            ));
            assert!(theme_checks::has_class(ok, "__azul-native-level-meter-ok"));
            assert!(theme_checks::has_class(
                warm,
                "__azul-native-level-meter-warm"
            ));
            assert!(theme_checks::has_class(
                hot,
                "__azul-native-level-meter-hot"
            ));
            for (seg, want) in [(ok, "70"), (warm, "20"), (hot, "5")] {
                let style = format!("{:?}", seg.root.get_style());
                assert!(
                    style.contains(&format!("Px({want}%)")),
                    "{}: a segment of {want}% in {style}",
                    theme.name()
                );
            }
        }
    }

    #[test]
    fn the_meter_says_its_level_as_a_progress_value_and_takes_no_focus() {
        let dom = LevelMeter::create(42.4)
            .with_accessibility_name("Microphone level")
            .with_theme(UiTheme::Flat)
            .dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::ProgressBar);
        assert_eq!(
            info.accessibility_value.as_ref().map(|v| v.as_str()),
            Some("42%")
        );
        assert_eq!(
            info.accessibility_name.as_ref().map(|v| v.as_str()),
            Some("Microphone level")
        );
        assert!(theme_checks::focusable(&dom).is_empty());
        // No text: a meter is drawn, not written.
        assert!(theme_checks::nodes(&dom)
            .iter()
            .all(|(_, n)| !matches!(n.root.get_node_type(), NodeType::Text(_))));
    }

    #[test]
    fn a_vertical_meter_fills_from_the_bottom() {
        let dom = LevelMeter::create(50.0)
            .with_orientation(LevelMeterOrientation::Vertical)
            .with_theme(UiTheme::Flat)
            .dom();
        let (track, [ok, _, _]) = segments(&dom);
        let track_style = format!("{:?}", track.root.get_style());
        assert!(track_style.contains("ColumnReverse"), "{track_style}");
        let ok_style = format!("{:?}", ok.root.get_style());
        assert!(
            ok_style.contains("Height"),
            "the segments size by height: {ok_style}"
        );
    }

    #[test]
    fn a_meter_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "level_meter",
            || LevelMeter::create(80.0).dom(),
            |t: UiTheme| LevelMeter::create(80.0).with_theme(t).dom(),
        );
    }
}
