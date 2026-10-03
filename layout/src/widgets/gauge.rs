//! Gauge widget - one value between a minimum and a maximum, shown as an arc
//! (a dial), a ring or a bar, with ranges that say whether the value is fine:
//! a CPU load, a battery, a password's strength, a budget spent, a progress
//! towards a goal.
//!
//! ```text
//!        .-~~~~~-.            Arc: 270 degrees, open at the bottom
//!      .'  .---.  '.          the track, the bands washed over it (ok /
//!     /   /     \   \         warn / bad), the value's arc in the colour
//!    |   |  73%  |   |        of the band it is in
//!     \   \ CPU /   /         the value and the label in the middle
//!      '         '
//!
//!    CPU                73%   Linear: the label and the value over a bar
//!    [==========|====    ]
//! ```
//!
//! THE APP OWNS THE VALUE: a gauge shows what it is given ([`Gauge::create`],
//! [`Gauge::with_value`]) and takes no input - like HTML's `<meter>` it is
//! not a control and not a Tab stop. A changing value is a rebuild.
//!
//! BANDS: [`GaugeBand`]s cover parts of the range with a kind -
//! [`GaugeBandKind::Ok`], `Warn`, `Bad`, `Neutral` - or the app's own
//! colour; the track shows them washed, and the value's arc (or bar) takes
//! the colour of the band the value is in (the LAST band listed that holds
//! it), the accent outside every band. The colour is never the only signal:
//! the accessible value names the band ("73% (warning)").
//!
//! DRAWING: the dial is a box with an SVG user space of one unit per px and
//! every arc is ONE node with an SVG path FILL (the chart's
//! `chart::wedge_ring`, a ring segment between two angles): the engine's own
//! vector path, no second renderer. The bar is plain boxes.
//!
//! ACCESSIBILITY: role `Indicator` (accesskit `Meter`, via
//! [`azul_core::a11y::MeterAriaInfo`]), named by the accessibility name or
//! the label, its value the shown text and the band.
//!
//! Key types: [`Gauge`], [`GaugeKind`], [`GaugeBand`], [`GaugeBandKind`].

use alloc::{format, string::String, vec::Vec};

use azul_core::dom::{Dom, IdOrClass, IdOrClassVec};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, AzString, OptionString,
};

use crate::widgets::{
    chart::{ChartColor, OptionChartColor},
    themes::{OptionUiTheme, UiTheme},
};

// ---- classes ----

/// The widget's root.
pub const GAUGE_CLASS: &str = "__azul-native-gauge";
/// The dial of a radial gauge (the SVG user space).
pub const GAUGE_DIAL_CLASS: &str = "__azul-native-gauge-dial";
/// The bar of a linear gauge.
pub const GAUGE_BAR_CLASS: &str = "__azul-native-gauge-bar";
/// The track: the whole range.
pub const GAUGE_TRACK_CLASS: &str = "__azul-native-gauge-track";
/// One band, washed over the track.
pub const GAUGE_BAND_CLASS: &str = "__azul-native-gauge-band";
/// The value's arc or bar.
pub const GAUGE_VALUE_CLASS: &str = "__azul-native-gauge-value";
/// The value's text.
pub const GAUGE_VALUE_TEXT_CLASS: &str = "__azul-native-gauge-value-text";
/// The label's text.
pub const GAUGE_LABEL_CLASS: &str = "__azul-native-gauge-label";

// ---- metrics (logical px) ----

/// A radial gauge's diameter unless the app sets one.
pub const DEFAULT_SIZE: f32 = 120.0;
/// A linear gauge's width unless the app sets one.
pub const DEFAULT_LINEAR_WIDTH: f32 = 200.0;
/// A radial gauge's ring thickness unless the app sets one.
pub const DEFAULT_THICKNESS: f32 = 12.0;
/// A linear gauge's bar height unless the app sets one.
pub const DEFAULT_LINEAR_THICKNESS: f32 = 8.0;
/// The smallest diameter / width a gauge draws at.
pub const MIN_SIZE: f32 = 16.0;

// ---- the types the app sees ----

/// How a gauge draws its range.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum GaugeKind {
    /// A dial: an arc of 270 degrees, open at the bottom, the value and the
    /// label in the middle.
    #[default]
    Arc,
    /// A full ring, starting at twelve o'clock (an activity ring, a
    /// progress towards a goal).
    Ring,
    /// A horizontal bar under the label and the value.
    Linear,
}

impl GaugeKind {
    /// Arc or ring: drawn round.
    #[must_use]
    pub const fn is_radial(self) -> bool {
        matches!(self, Self::Arc | Self::Ring)
    }
}

/// What a band of the range means - its colour in every theme, and the word
/// the accessible value adds when the value is in it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum GaugeBandKind {
    /// Fine (green).
    #[default]
    Ok,
    /// Needs attention (amber).
    Warn,
    /// A problem (red).
    Bad,
    /// A plain range without a judgement (grey).
    Neutral,
}

impl GaugeBandKind {
    /// The word a screen reader hears after the value ("73% (warning)");
    /// `None` for a band without a judgement.
    #[must_use]
    pub const fn word(self) -> Option<&'static str> {
        match self {
            Self::Ok => Some("ok"),
            Self::Warn => Some("warning"),
            Self::Bad => Some("critical"),
            Self::Neutral => None,
        }
    }
}

/// A part of the range with a meaning: `from` to `to` (either way round,
/// both included).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct GaugeBand {
    /// One end of the band.
    pub from: f64,
    /// The other end.
    pub to: f64,
    /// What the band means.
    pub kind: GaugeBandKind,
    /// The band's own colour instead of its kind's.
    pub color: OptionChartColor,
}

impl GaugeBand {
    /// The band `from`..`to` of `kind`.
    #[must_use]
    pub const fn create(from: f64, to: f64, kind: GaugeBandKind) -> Self {
        Self {
            from,
            to,
            kind,
            color: OptionChartColor::None,
        }
    }

    /// The band in its own colour instead of its kind's.
    #[must_use]
    pub const fn with_color(mut self, color: ChartColor) -> Self {
        self.color = OptionChartColor::Some(color);
        self
    }

    /// Whether `value` is in the band (both ends included).
    #[must_use]
    pub fn contains(&self, value: f64) -> bool {
        let _ = value;
        false
    }
}

impl_option!(
    GaugeBand,
    OptionGaugeBand,
    [Debug, Clone, Copy, PartialEq, PartialOrd]
);
impl_vec!(
    GaugeBand,
    GaugeBandVec,
    GaugeBandVecDestructor,
    GaugeBandVecDestructorType,
    GaugeBandVecSlice,
    OptionGaugeBand
);
impl_vec_clone!(GaugeBand, GaugeBandVec, GaugeBandVecDestructor);
impl_vec_debug!(GaugeBand, GaugeBandVec);
impl_vec_mut!(GaugeBand, GaugeBandVec);

azul_css::impl_vec_partialeq!(GaugeBand, GaugeBandVec);

/// A value between a minimum and a maximum, shown as a dial, a ring or a
/// bar (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct Gauge {
    /// The value shown.
    pub value: f64,
    /// The start of the range.
    pub min: f64,
    /// The end of the range.
    pub max: f64,
    /// The bands, in order; a later band wins where two overlap.
    pub bands: GaugeBandVec,
    /// What the gauge measures ("CPU"), under the value (radial) or before
    /// it (linear); empty for none.
    pub label: AzString,
    /// What shows as the value; `None` writes the value with its unit.
    pub value_text: OptionString,
    /// The unit written after the value ("%", " GB"); empty for none.
    pub unit: AzString,
    /// What this gauge is CALLED, for assistive technology (`None`: the
    /// label).
    pub accessibility_name: OptionString,
    /// The diameter (radial) or the width (linear), in px.
    pub size: f32,
    /// The ring's thickness (radial) or the bar's height (linear), in px.
    pub thickness: f32,
    /// How the range is drawn.
    pub kind: GaugeKind,
    /// The widget theme, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// The value's colour outside every band (`None`: the theme's accent).
    pub accent: OptionChartColor,
    /// Wash the bands over the track (default on).
    pub show_bands: bool,
}

impl Default for Gauge {
    fn default() -> Self {
        Self::create(0.0, 0.0, 100.0)
    }
}

impl Gauge {
    /// A dial showing `value` between `min` and `max`.
    #[must_use]
    pub fn create(value: f64, min: f64, max: f64) -> Self {
        Self {
            value,
            min,
            max,
            bands: GaugeBandVec::from_const_slice(&[]),
            label: AzString::from_const_str(""),
            value_text: OptionString::None,
            unit: AzString::from_const_str(""),
            accessibility_name: OptionString::None,
            size: DEFAULT_SIZE,
            thickness: DEFAULT_THICKNESS,
            kind: GaugeKind::Arc,
            theme: OptionUiTheme::None,
            accent: OptionChartColor::None,
            show_bands: true,
        }
    }

    /// A bar showing `value` between `min` and `max`.
    #[must_use]
    pub fn create_linear(value: f64, min: f64, max: f64) -> Self {
        let mut g = Self::create(value, min, max);
        g.kind = GaugeKind::Linear;
        g.size = DEFAULT_LINEAR_WIDTH;
        g.thickness = DEFAULT_LINEAR_THICKNESS;
        g
    }
}

// ---- the geometry (the pure half) ----

/// Where `value` sits in `min`..`max`, from 0 to 1: clamped to the range, an
/// inverted range read the right way round, a value that is not a number
/// (or an empty range) at 0.
#[must_use]
pub(crate) fn fraction(value: f64, min: f64, max: f64) -> f32 {
    let _ = (value, min, max);
    0.0
}

/// The angles a kind draws its range over: `(start, sweep)` in radians,
/// clockwise from twelve o'clock (`chart::wedge_ring`'s convention). The arc
/// starts at half past seven and sweeps 270 degrees; the ring starts at
/// twelve and goes all the way round; a bar has none.
#[must_use]
pub(crate) fn sweep_of(kind: GaugeKind) -> (f32, f32) {
    let _ = kind;
    (0.0, 0.0)
}

/// The angle of the point `fraction` along the range.
#[must_use]
pub(crate) fn angle_at(kind: GaugeKind, fraction: f32) -> f32 {
    let _ = (kind, fraction);
    0.0
}

/// The band `value` is in: the LAST listed band holding it, so a narrower
/// band listed after a wide one wins.
#[must_use]
pub(crate) fn band_of(bands: &[GaugeBand], value: f64) -> Option<&GaugeBand> {
    let _ = (bands, value);
    None
}

/// The part of the range a band covers, as fractions `(from, to)` with
/// `from < to`, clipped to the range; `None` for a band outside it or of no
/// width.
#[must_use]
pub(crate) fn band_span(band: &GaugeBand, min: f64, max: f64) -> Option<(f32, f32)> {
    let _ = (band, min, max);
    None
}

impl Gauge {
    /// The text that shows as the value: the app's, or the value written
    /// with its unit ("73%", "1,234.5 GB").
    #[must_use]
    pub fn shown_value(&self) -> AzString {
        AzString::from_const_str("")
    }

    /// The gauge in words, for a screen reader: the shown value and the
    /// band's word ("73% (warning)").
    #[must_use]
    pub fn summary(&self) -> AzString {
        AzString::from_const_str("")
    }
}
