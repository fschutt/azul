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
    themes::{decl, OptionUiTheme, UiTheme},
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
        let (lo, hi) = ordered(self.from, self.to);
        value >= lo && value <= hi
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

    /// Name this gauge for assistive technology (default: its label).
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// The value shown.
    pub const fn set_value(&mut self, value: f64) {
        self.value = value;
    }

    /// [`Self::set_value`] for the builder chain.
    #[must_use]
    pub const fn with_value(mut self, value: f64) -> Self {
        self.set_value(value);
        self
    }

    /// The range, `min` to `max`.
    pub const fn set_range(&mut self, min: f64, max: f64) {
        self.min = min;
        self.max = max;
    }

    /// [`Self::set_range`] for the builder chain.
    #[must_use]
    pub const fn with_range(mut self, min: f64, max: f64) -> Self {
        self.set_range(min, max);
        self
    }

    /// How the range is drawn (the size and thickness stay as set).
    pub const fn set_kind(&mut self, kind: GaugeKind) {
        self.kind = kind;
    }

    /// [`Self::set_kind`] for the builder chain.
    #[must_use]
    pub const fn with_kind(mut self, kind: GaugeKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// Every band, in order (a later band wins where two overlap).
    pub fn set_bands(&mut self, bands: GaugeBandVec) {
        self.bands = bands;
    }

    /// [`Self::set_bands`] for the builder chain.
    #[must_use]
    pub fn with_bands(mut self, bands: GaugeBandVec) -> Self {
        self.set_bands(bands);
        self
    }

    /// One more band, after the others.
    pub fn add_band(&mut self, band: GaugeBand) {
        let mut v = self.bands.as_slice().to_vec();
        v.push(band);
        self.bands = GaugeBandVec::from_vec(v);
    }

    /// [`Self::add_band`] for the builder chain.
    #[must_use]
    pub fn with_band(mut self, band: GaugeBand) -> Self {
        self.add_band(band);
        self
    }

    /// What the gauge measures ("CPU").
    pub fn set_label(&mut self, label: AzString) {
        self.label = label;
    }

    /// [`Self::set_label`] for the builder chain.
    #[must_use]
    pub fn with_label(mut self, label: AzString) -> Self {
        self.set_label(label);
        self
    }

    /// The text shown as the value instead of the number and its unit.
    pub fn set_value_text(&mut self, text: AzString) {
        self.value_text = OptionString::Some(text);
    }

    /// [`Self::set_value_text`] for the builder chain.
    #[must_use]
    pub fn with_value_text(mut self, text: AzString) -> Self {
        self.set_value_text(text);
        self
    }

    /// The unit written after the value ("%", " GB").
    pub fn set_unit(&mut self, unit: AzString) {
        self.unit = unit;
    }

    /// [`Self::set_unit`] for the builder chain.
    #[must_use]
    pub fn with_unit(mut self, unit: AzString) -> Self {
        self.set_unit(unit);
        self
    }

    /// The diameter (radial) or width (linear) in px, at least
    /// [`MIN_SIZE`].
    pub const fn set_size(&mut self, size: f32) {
        self.size = size;
    }

    /// [`Self::set_size`] for the builder chain.
    #[must_use]
    pub const fn with_size(mut self, size: f32) -> Self {
        self.set_size(size);
        self
    }

    /// The ring's thickness (radial) or the bar's height (linear) in px.
    pub const fn set_thickness(&mut self, thickness: f32) {
        self.thickness = thickness;
    }

    /// [`Self::set_thickness`] for the builder chain.
    #[must_use]
    pub const fn with_thickness(mut self, thickness: f32) -> Self {
        self.set_thickness(thickness);
        self
    }

    /// The value's colour outside every band (default: the theme's accent).
    pub const fn set_accent(&mut self, accent: ChartColor) {
        self.accent = OptionChartColor::Some(accent);
    }

    /// [`Self::set_accent`] for the builder chain.
    #[must_use]
    pub const fn with_accent(mut self, accent: ChartColor) -> Self {
        self.set_accent(accent);
        self
    }

    /// Wash the bands over the track (default on).
    pub const fn set_show_bands(&mut self, show_bands: bool) {
        self.show_bands = show_bands;
    }

    /// [`Self::set_show_bands`] for the builder chain.
    #[must_use]
    pub const fn with_show_bands(mut self, show_bands: bool) -> Self {
        self.set_show_bands(show_bands);
        self
    }

    /// Pin the widget theme. Unset (`None`), the gauge follows the app
    /// theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty dial and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }
}

// ---- the geometry (the pure half) ----

/// Where `value` sits in `min`..`max`, from 0 to 1: clamped to the range, an
/// inverted range read the right way round, a value that is not a number
/// (or an empty range) at 0.
#[must_use]
pub(crate) fn fraction(value: f64, min: f64, max: f64) -> f32 {
    let (lo, hi) = ordered(min, max);
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo || value.is_nan() {
        return 0.0;
    }
    ((value - lo) / (hi - lo)).clamp(0.0, 1.0) as f32
}

/// `(a, b)` smaller first; a NaN stays where the comparison leaves it (the
/// callers reject it).
fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// The angles a kind draws its range over: `(start, sweep)` in radians,
/// clockwise from twelve o'clock (`chart::wedge_ring`'s convention). The arc
/// starts at half past seven and sweeps 270 degrees; the ring starts at
/// twelve and goes all the way round; a bar has none.
#[must_use]
pub(crate) fn sweep_of(kind: GaugeKind) -> (f32, f32) {
    use core::f32::consts::PI;
    match kind {
        GaugeKind::Arc => (-0.75 * PI, 1.5 * PI),
        GaugeKind::Ring => (0.0, 2.0 * PI),
        GaugeKind::Linear => (0.0, 0.0),
    }
}

/// The angle of the point `fraction` along the range.
#[must_use]
pub(crate) fn angle_at(kind: GaugeKind, fraction: f32) -> f32 {
    let (start, sweep) = sweep_of(kind);
    sweep.mul_add(fraction.clamp(0.0, 1.0), start)
}

/// The band `value` is in: the LAST listed band holding it, so a narrower
/// band listed after a wide one wins.
#[must_use]
pub(crate) fn band_of(bands: &[GaugeBand], value: f64) -> Option<&GaugeBand> {
    bands.iter().rev().find(|b| b.contains(value))
}

/// The part of the range a band covers, as fractions `(from, to)` with
/// `from < to`, clipped to the range; `None` for a band outside it or of no
/// width.
#[must_use]
pub(crate) fn band_span(band: &GaugeBand, min: f64, max: f64) -> Option<(f32, f32)> {
    let (lo, hi) = ordered(min, max);
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo {
        return None;
    }
    let (a, b) = ordered(band.from, band.to);
    if a.is_nan() || b.is_nan() {
        return None;
    }
    let (a, b) = (a.max(lo), b.min(hi));
    if b <= a {
        return None;
    }
    Some((fraction(a, lo, hi), fraction(b, lo, hi)))
}

impl Gauge {
    /// The text that shows as the value: the app's, or the value written
    /// with its unit ("73%", "1,234.5 GB").
    #[must_use]
    pub fn shown_value(&self) -> AzString {
        match self.value_text.as_ref() {
            Some(text) => text.clone(),
            None => AzString::from(format!(
                "{}{}",
                crate::widgets::chart::format_value(self.value),
                self.unit.as_str()
            )),
        }
    }

    /// The gauge in words, for a screen reader: the shown value and the
    /// band's word ("73% (warning)").
    #[must_use]
    pub fn summary(&self) -> AzString {
        let shown = self.shown_value();
        match band_of(self.bands.as_slice(), self.value).and_then(|b| b.kind.word()) {
            Some(word) => AzString::from(format!("{} ({word})", shown.as_str())),
            None => shown,
        }
    }
}

// ==== the look ====

/// What one widget theme decides about a gauge: its inks, the track, the
/// band kinds' colours and the accent. Built by `themes::flat::gauge_skin`
/// and `themes::flora::gauge_skin`.
#[derive(Debug, Clone)]
pub(crate) struct GaugeSkin {
    /// The root: the face and the ink.
    pub(crate) root: Vec<CssPropertyWithConditions>,
    /// The value's text (its size follows the gauge's size).
    pub(crate) value_text: Vec<CssPropertyWithConditions>,
    /// The label's text.
    pub(crate) label: Vec<CssPropertyWithConditions>,
    /// The track: the whole range.
    pub(crate) track: ChartColor,
    /// [`GaugeBandKind::Ok`].
    pub(crate) ok: ChartColor,
    /// [`GaugeBandKind::Warn`].
    pub(crate) warn: ChartColor,
    /// [`GaugeBandKind::Bad`].
    pub(crate) bad: ChartColor,
    /// [`GaugeBandKind::Neutral`].
    pub(crate) neutral: ChartColor,
    /// The value outside every band.
    pub(crate) accent: ChartColor,
    /// The theme's marker class on the root, if it has one.
    pub(crate) marker: Option<&'static str>,
}

impl GaugeSkin {
    /// A band kind's colour.
    #[must_use]
    pub(crate) const fn kind_color(&self, kind: GaugeBandKind) -> ChartColor {
        match kind {
            GaugeBandKind::Ok => self.ok,
            GaugeBandKind::Warn => self.warn,
            GaugeBandKind::Bad => self.bad,
            GaugeBandKind::Neutral => self.neutral,
        }
    }

    /// A band's colour: its own, or its kind's.
    #[must_use]
    pub(crate) fn band_color(&self, band: &GaugeBand) -> ChartColor {
        band.color
            .into_option()
            .unwrap_or_else(|| self.kind_color(band.kind))
    }
}

/// The colour of the value's arc or bar in `skin`: the band the value is in,
/// or the accent (the app's, else the theme's) outside every band.
#[must_use]
pub(crate) fn value_color(gauge: &Gauge, skin: &GaugeSkin) -> ChartColor {
    match band_of(gauge.bands.as_slice(), gauge.value) {
        Some(band) => skin.band_color(band),
        None => gauge.accent.into_option().unwrap_or(skin.accent),
    }
}

/// A solid fill in `color`, with its dark step.
fn solid(color: ChartColor) -> Vec<CssPropertyWithConditions> {
    decl::themed_fill(color.light, color.dark).to_vec()
}

/// A band's wash over the track: its colour at about a third (a little more
/// at night, where a third sinks into the dark track).
fn wash(color: ChartColor) -> Vec<CssPropertyWithConditions> {
    use azul_css::props::basic::color::ColorU;
    decl::themed_fill(
        ColorU {
            a: 90,
            ..color.light
        },
        ColorU {
            a: 120,
            ..color.dark
        },
    )
    .to_vec()
}

// ==== the DOM ====

/// One arc of the dial: a node over the dial with the ring segment between
/// two fractions of the range as its path.
fn arc_node(
    class: &'static str,
    kind: GaugeKind,
    ring: (f32, f32, f32, f32),
    span: (f32, f32),
    style: CssPropertyWithConditionsVec,
) -> Dom {
    use azul_core::{
        dom::SvgNodeData,
        svg::{SvgMultiPolygon, SvgPathVec},
    };
    let (cx, cy, r_out, r_in) = ring;
    let path = crate::widgets::chart::wedge_ring(
        cx,
        cy,
        r_out,
        r_in,
        angle_at(kind, span.0),
        angle_at(kind, span.1),
    );
    Dom::create_div()
        .with_ids_and_classes(decl::classes(&[class]))
        .with_css_props(style)
        .with_svg_data(SvgNodeData::Path(SvgMultiPolygon::create(
            SvgPathVec::from_vec(alloc::vec![path]),
        )))
}

/// A widget-owned text line with `class` and `style`.
fn text_line(text: AzString, class: &'static str, style: CssPropertyWithConditionsVec) -> Dom {
    crate::widgets::widget_p_with_text(text)
        .with_ids_and_classes(decl::classes(&[class]))
        .with_css_props(style)
}

/// `size` px scaled by `k`, rounded, held between `lo` and `hi`.
fn scaled(size: f32, k: f32, lo: f32, hi: f32) -> isize {
    (size * k).clamp(lo, hi).round() as isize
}

impl Gauge {
    /// The name the gauge is announced by: the app's, else its label.
    fn a11y_name(&self) -> Option<AzString> {
        match self.accessibility_name.as_ref() {
            Some(name) => Some(name.clone()),
            None if !self.label.as_str().is_empty() => Some(self.label.clone()),
            None => None,
        }
    }

    /// The root's accessibility: a meter (role `Indicator`) over the range,
    /// named, its value the summary.
    fn a11y_info(&self) -> azul_core::a11y::AccessibilityInfo {
        let name = self.a11y_name();
        crate::widgets::warn_widget_needs_a_name("Gauge", name.is_some());
        let mut info = azul_core::a11y::MeterAriaInfo::create(
            name.unwrap_or_else(|| AzString::from_const_str("Gauge")),
            self.value as f32,
            self.min as f32,
            self.max as f32,
        )
        .to_full_info();
        info.accessibility_value = OptionString::Some(self.summary());
        info
    }

    /// Renders the gauge: a dial or a ring (one SVG user space, an arc per
    /// node) or a bar, in the theme's skin - pinned, or both skins merged to
    /// follow the app theme.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks::skins_of};
        let skins = skins_of(self.theme, flat::gauge_skin, flora::gauge_skin);
        self.build(&skins)
    }

    /// The DOM in `skins` (`theme_blocks::skins_of`).
    pub(crate) fn build(&self, skins: &[GaugeSkin]) -> Dom {
        use crate::widgets::themes::theme_blocks::structure_skin;

        let marker = structure_skin(skins, self.theme).and_then(|s| s.marker);
        let body = if self.kind.is_radial() {
            self.build_dial(skins)
        } else {
            self.build_bar(skins)
        };
        let mut classes: Vec<IdOrClass> =
            alloc::vec![IdOrClass::Class(AzString::from_const_str(GAUGE_CLASS))];
        if let Some(marker) = marker {
            classes.push(IdOrClass::Class(AzString::from_const_str(marker)));
        }
        body.with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_accessibility_info(self.a11y_info())
    }

    /// The dial or the ring: the track, the bands, the value's arc and the
    /// text in the middle, over one user space of `size` x `size`.
    fn build_dial(&self, skins: &[GaugeSkin]) -> Dom {
        use azul_core::dom::SvgNodeData;
        use azul_css::props::layout::{
            LayoutAlignItems, LayoutFlexDirection, LayoutJustifyContent, LayoutPosition,
        };

        use crate::widgets::{
            chart::over_plot,
            themes::{decl, theme_blocks::part_of},
        };

        let size = self.size.max(MIN_SIZE);
        let r_out = size / 2.0 - 1.0;
        let thickness = self.thickness.clamp(1.0, r_out);
        let ring = (size / 2.0, size / 2.0, r_out, (r_out - thickness).max(0.0));
        let kind = self.kind;
        let on_dial = |paint: &dyn Fn(&GaugeSkin) -> Vec<CssPropertyWithConditions>| {
            part_of(skins, |s| {
                let mut v = over_plot();
                v.extend(paint(s));
                v
            })
        };

        let mut parts: Vec<Dom> = Vec::new();
        parts.push(arc_node(
            GAUGE_TRACK_CLASS,
            kind,
            ring,
            (0.0, 1.0),
            on_dial(&|s| solid(s.track)),
        ));
        if self.show_bands {
            for band in self.bands.as_slice() {
                if let Some(span) = band_span(band, self.min, self.max) {
                    parts.push(arc_node(
                        GAUGE_BAND_CLASS,
                        kind,
                        ring,
                        span,
                        on_dial(&|s| wash(s.band_color(band))),
                    ));
                }
            }
        }
        let f = fraction(self.value, self.min, self.max);
        if f > 0.0 {
            parts.push(arc_node(
                GAUGE_VALUE_CLASS,
                kind,
                ring,
                (0.0, f),
                on_dial(&|s| solid(value_color(self, s))),
            ));
        }

        // The middle: the value over the label, centred.
        let value_px = scaled(size, 0.2, 11.0, 40.0);
        let label_px = scaled(size, 0.1, 10.0, 14.0);
        let mut middle_kids = alloc::vec![text_line(
            self.shown_value(),
            GAUGE_VALUE_TEXT_CLASS,
            part_of(skins, |s| {
                let mut v = alloc::vec![decl::font_size(value_px)];
                v.extend(s.value_text.iter().cloned());
                v
            }),
        )];
        if !self.label.as_str().is_empty() {
            middle_kids.push(text_line(
                self.label.clone(),
                GAUGE_LABEL_CLASS,
                part_of(skins, |s| {
                    let mut v = alloc::vec![decl::font_size(label_px)];
                    v.extend(s.label.iter().cloned());
                    v
                }),
            ));
        }
        let mut middle_style = over_plot();
        middle_style.extend([
            decl::display_flex(),
            decl::flex_direction(LayoutFlexDirection::Column),
            decl::simple(azul_css::props::property::CssProperty::const_align_items(
                LayoutAlignItems::Center,
            )),
            decl::simple(
                azul_css::props::property::CssProperty::const_justify_content(
                    LayoutJustifyContent::Center,
                ),
            ),
        ]);
        parts.push(
            Dom::create_div()
                .with_css_props(CssPropertyWithConditionsVec::from_vec(middle_style))
                .with_children(middle_kids.into()),
        );

        let dial = Dom::create_div()
            .with_ids_and_classes(decl::classes(&[GAUGE_DIAL_CLASS]))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(alloc::vec![
                decl::position(LayoutPosition::Relative),
                decl::px_width(size),
                decl::px_height(size),
                decl::no_shrink(),
            ]))
            .with_svg_data(SvgNodeData::ViewBox {
                min_x: 0.0,
                min_y: 0.0,
                width: size,
                height: size,
            })
            .with_children(parts.into());

        Dom::create_div()
            .with_css_props(part_of(skins, |s| {
                let mut v = alloc::vec![
                    decl::display_flex(),
                    decl::flex_direction(LayoutFlexDirection::Column),
                    decl::simple(azul_css::props::property::CssProperty::const_align_items(
                        LayoutAlignItems::Center,
                    )),
                    decl::px_width(size),
                ];
                v.extend(s.root.iter().cloned());
                v
            }))
            .with_child(dial)
    }

    /// The bar: the label and the value over the track, the bands washed
    /// over it, the value's fill from the start.
    fn build_bar(&self, skins: &[GaugeSkin]) -> Dom {
        use azul_css::props::{
            basic::PixelValue,
            layout::{
                LayoutAlignItems, LayoutFlexDirection, LayoutJustifyContent, LayoutPosition,
                LayoutRowGap,
            },
            property::{CssProperty, LayoutRowGapValue},
        };

        use crate::widgets::themes::{decl, theme_blocks::part_of};

        let width = self.size.max(MIN_SIZE);
        let height = self.thickness.clamp(1.0, 64.0);
        let corner = (height / 2.0).round() as isize;
        let placed = |left: f32, w: f32| {
            alloc::vec![
                decl::position(LayoutPosition::Absolute),
                decl::px_left(left),
                decl::px_top(0.0),
                decl::px_width(w.max(0.0)),
                decl::px_height(height),
            ]
        };

        let mut bar_kids: Vec<Dom> = Vec::new();
        if self.show_bands {
            for band in self.bands.as_slice() {
                if let Some((f0, f1)) = band_span(band, self.min, self.max) {
                    bar_kids.push(
                        Dom::create_div()
                            .with_ids_and_classes(decl::classes(&[GAUGE_BAND_CLASS]))
                            .with_css_props(part_of(skins, |s| {
                                let mut v = placed(f0 * width, (f1 - f0) * width);
                                v.extend(wash(s.band_color(band)));
                                v
                            })),
                    );
                }
            }
        }
        let f = fraction(self.value, self.min, self.max);
        if f > 0.0 {
            bar_kids.push(
                Dom::create_div()
                    .with_ids_and_classes(decl::classes(&[GAUGE_VALUE_CLASS]))
                    .with_css_props(part_of(skins, |s| {
                        let mut v = placed(0.0, f * width);
                        v.extend(decl::radius(corner));
                        v.extend(solid(value_color(self, s)));
                        v
                    })),
            );
        }
        let bar = Dom::create_div()
            .with_ids_and_classes(decl::classes(&[GAUGE_BAR_CLASS, GAUGE_TRACK_CLASS]))
            .with_css_props(part_of(skins, |s| {
                let mut v = alloc::vec![
                    decl::position(LayoutPosition::Relative),
                    decl::px_width(width),
                    decl::px_height(height),
                    decl::overflow_x_hidden(),
                    decl::overflow_y_hidden(),
                ];
                v.extend(decl::radius(corner));
                v.extend(solid(s.track));
                v
            }))
            .with_children(bar_kids.into());

        let mut head_kids: Vec<Dom> = Vec::new();
        if !self.label.as_str().is_empty() {
            head_kids.push(text_line(
                self.label.clone(),
                GAUGE_LABEL_CLASS,
                part_of(skins, |s| s.label.clone()),
            ));
        }
        head_kids.push(text_line(
            self.shown_value(),
            GAUGE_VALUE_TEXT_CLASS,
            part_of(skins, |s| s.value_text.clone()),
        ));
        let head = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(alloc::vec![
                decl::display_flex(),
                decl::flex_direction(LayoutFlexDirection::Row),
                decl::simple(CssProperty::const_justify_content(
                    LayoutJustifyContent::SpaceBetween,
                )),
                decl::simple(CssProperty::const_align_items(LayoutAlignItems::End)),
            ]))
            .with_children(head_kids.into());

        Dom::create_div()
            .with_css_props(part_of(skins, |s| {
                let mut v = alloc::vec![
                    decl::display_flex(),
                    decl::flex_direction(LayoutFlexDirection::Column),
                    decl::simple(CssProperty::RowGap(LayoutRowGapValue::Exact(
                        LayoutRowGap {
                            inner: PixelValue::const_px(4),
                        }
                    ))),
                    decl::px_width(width),
                ];
                v.extend(s.root.iter().cloned());
                v
            }))
            .with_children(alloc::vec![head, bar].into())
    }
}

impl From<Gauge> for Dom {
    fn from(g: Gauge) -> Self {
        g.dom()
    }
}

#[cfg(test)]
mod geometry_tests {
    use core::f32::consts::PI;

    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_value_sits_at_its_fraction_of_the_range() {
        assert!(close(fraction(50.0, 0.0, 100.0), 0.5));
        assert!(close(fraction(0.0, 0.0, 100.0), 0.0));
        assert!(close(fraction(100.0, 0.0, 100.0), 1.0));
        assert!(close(fraction(-5.0, -10.0, 10.0), 0.25));
    }

    #[test]
    fn a_value_outside_the_range_holds_at_its_end() {
        assert!(close(fraction(150.0, 0.0, 100.0), 1.0));
        assert!(close(fraction(-1.0, 0.0, 100.0), 0.0));
        assert!(close(fraction(f64::INFINITY, 0.0, 100.0), 1.0));
        assert!(close(fraction(f64::NEG_INFINITY, 0.0, 100.0), 0.0));
    }

    #[test]
    fn a_broken_value_or_range_draws_nothing_and_an_inverted_range_reads_right() {
        assert!(close(fraction(f64::NAN, 0.0, 100.0), 0.0));
        assert!(close(fraction(5.0, 5.0, 5.0), 0.0));
        assert!(close(fraction(5.0, f64::NAN, 10.0), 0.0));
        assert!(close(fraction(25.0, 100.0, 0.0), 0.25));
    }

    #[test]
    fn the_arc_sweeps_270_degrees_from_half_past_seven() {
        let (start, sweep) = sweep_of(GaugeKind::Arc);
        assert!(close(start, -0.75 * PI));
        assert!(close(sweep, 1.5 * PI));
        assert!(close(angle_at(GaugeKind::Arc, 0.0), -0.75 * PI));
        assert!(
            close(angle_at(GaugeKind::Arc, 0.5), 0.0),
            "the middle is at twelve"
        );
        assert!(close(angle_at(GaugeKind::Arc, 1.0), 0.75 * PI));
    }

    #[test]
    fn the_ring_goes_all_the_way_round_from_twelve() {
        let (start, sweep) = sweep_of(GaugeKind::Ring);
        assert!(close(start, 0.0));
        assert!(close(sweep, 2.0 * PI));
        assert!(
            close(angle_at(GaugeKind::Ring, 0.25), 0.5 * PI),
            "a quarter is at three"
        );
    }

    #[test]
    fn a_bar_has_no_angles() {
        assert_eq!(sweep_of(GaugeKind::Linear), (0.0, 0.0));
    }

    #[test]
    fn a_band_holds_both_its_ends_either_way_round() {
        let b = GaugeBand::create(60.0, 80.0, GaugeBandKind::Warn);
        assert!(b.contains(60.0) && b.contains(70.0) && b.contains(80.0));
        assert!(!b.contains(59.9) && !b.contains(80.1) && !b.contains(f64::NAN));
        let reversed = GaugeBand::create(80.0, 60.0, GaugeBandKind::Warn);
        assert!(reversed.contains(70.0));
    }

    #[test]
    fn the_value_takes_the_last_band_that_holds_it() {
        let bands = [
            GaugeBand::create(0.0, 100.0, GaugeBandKind::Ok),
            GaugeBand::create(70.0, 90.0, GaugeBandKind::Warn),
            GaugeBand::create(90.0, 100.0, GaugeBandKind::Bad),
        ];
        assert_eq!(
            band_of(&bands, 50.0).map(|b| b.kind),
            Some(GaugeBandKind::Ok)
        );
        assert_eq!(
            band_of(&bands, 75.0).map(|b| b.kind),
            Some(GaugeBandKind::Warn)
        );
        assert_eq!(
            band_of(&bands, 90.0).map(|b| b.kind),
            Some(GaugeBandKind::Bad)
        );
        assert_eq!(band_of(&bands, 120.0).map(|b| b.kind), None);
        assert_eq!(band_of(&[], 50.0).map(|b| b.kind), None);
    }

    #[test]
    fn a_band_spans_its_part_of_the_range_clipped_to_it() {
        let span =
            |from, to| band_span(&GaugeBand::create(from, to, GaugeBandKind::Ok), 0.0, 100.0);
        assert_eq!(span(25.0, 75.0), Some((0.25, 0.75)));
        assert_eq!(span(75.0, 25.0), Some((0.25, 0.75)));
        assert_eq!(span(-50.0, 50.0), Some((0.0, 0.5)));
        assert_eq!(span(90.0, 150.0), Some((0.9, 1.0)));
        assert_eq!(span(120.0, 150.0), None, "outside the range");
        assert_eq!(span(40.0, 40.0), None, "no width");
        assert_eq!(span(f64::NAN, 40.0), None);
    }

    #[test]
    fn the_shown_value_is_the_number_with_its_unit_or_the_apps_text() {
        let g = Gauge::create(73.0, 0.0, 100.0).with_unit(AzString::from_const_str("%"));
        assert_eq!(g.shown_value().as_str(), "73%");
        let g = Gauge::create(1234.5, 0.0, 2000.0).with_unit(AzString::from_const_str(" GB"));
        assert_eq!(g.shown_value().as_str(), "1,234.5 GB");
        let g = Gauge::create(0.5, 0.0, 1.0).with_value_text(AzString::from_const_str("Half full"));
        assert_eq!(g.shown_value().as_str(), "Half full");
    }

    #[test]
    fn the_summary_names_the_band_the_value_is_in() {
        let g = Gauge::create(73.0, 0.0, 100.0)
            .with_unit(AzString::from_const_str("%"))
            .with_band(GaugeBand::create(0.0, 70.0, GaugeBandKind::Ok))
            .with_band(GaugeBand::create(70.0, 90.0, GaugeBandKind::Warn))
            .with_band(GaugeBand::create(90.0, 100.0, GaugeBandKind::Bad));
        assert_eq!(g.summary().as_str(), "73% (warning)");
        let g = g.with_value(95.0);
        assert_eq!(g.summary().as_str(), "95% (critical)");
        let plain = Gauge::create(10.0, 0.0, 100.0).with_unit(AzString::from_const_str("%"));
        assert_eq!(plain.summary().as_str(), "10%");
        let neutral = plain.with_band(GaugeBand::create(0.0, 50.0, GaugeBandKind::Neutral));
        assert_eq!(neutral.summary().as_str(), "10%");
    }
}

// ==== fixtures (the widget manifest's sample) ====

/// Samples for the widget manifest (`widgets::label_convention`) and the
/// tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// The three bands of a load meter: ok to 70, warning to 90, critical.
    pub(crate) fn load_bands() -> GaugeBandVec {
        GaugeBandVec::from_vec(alloc::vec![
            GaugeBand::create(0.0, 70.0, GaugeBandKind::Ok),
            GaugeBand::create(70.0, 90.0, GaugeBandKind::Warn),
            GaugeBand::create(90.0, 100.0, GaugeBandKind::Bad),
        ])
    }

    /// A CPU dial at 73% (in the warning band).
    pub(crate) fn sample() -> Gauge {
        Gauge::create(73.0, 0.0, 100.0)
            .with_label(AzString::from_const_str("CPU"))
            .with_unit(AzString::from_const_str("%"))
            .with_bands(load_bands())
    }

    /// The same load as a bar, 200 px wide.
    pub(crate) fn linear() -> Gauge {
        Gauge::create_linear(73.0, 0.0, 100.0)
            .with_label(AzString::from_const_str("CPU"))
            .with_unit(AzString::from_const_str("%"))
            .with_bands(load_bands())
    }
}

#[cfg(test)]
mod dom_tests {
    use azul_core::{
        a11y::AccessibilityRole,
        dom::{NodeType, SvgNodeData},
    };
    use azul_css::props::{
        layout::LayoutWidth,
        property::{CssProperty, CssPropertyType},
    };

    use super::{fixtures::*, *};
    use crate::widgets::themes::{flat, theme_blocks::checks, theme_checks as tc};

    fn texts(dom: &Dom) -> Vec<String> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| match n.root.get_node_type() {
                NodeType::Text(s) => Some(s.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    fn declares(node: &Dom, property: &CssProperty) -> bool {
        node.root
            .style
            .iter_inline_properties()
            .any(|(p, _)| p == property)
    }

    #[test]
    fn a_dial_is_one_user_space_of_its_size() {
        let dom = sample().with_size(140.0).with_theme(UiTheme::Flat).dom();
        let dial = tc::find(&dom, GAUGE_DIAL_CLASS).expect("the dial");
        match dial.root.get_svg_data() {
            Some(SvgNodeData::ViewBox {
                min_x,
                min_y,
                width,
                height,
            }) => {
                assert_eq!((*min_x, *min_y, *width, *height), (0.0, 0.0, 140.0, 140.0));
            }
            other => panic!("the dial carries no viewBox: {other:?}"),
        }
    }

    #[test]
    fn the_track_every_band_and_the_value_are_one_path_node_each() {
        let dom = sample().with_theme(UiTheme::Flat).dom();
        let track = tc::find_all(&dom, GAUGE_TRACK_CLASS);
        let bands = tc::find_all(&dom, GAUGE_BAND_CLASS);
        let value = tc::find_all(&dom, GAUGE_VALUE_CLASS);
        assert_eq!((track.len(), bands.len(), value.len()), (1, 3, 1));
        for node in track.iter().chain(&bands).chain(&value) {
            assert!(matches!(
                node.root.get_svg_data(),
                Some(SvgNodeData::Path(_))
            ));
            assert!(tc::background(node, false).is_some(), "an arc is its fill");
        }
    }

    #[test]
    fn without_bands_shown_the_track_is_bare() {
        let dom = sample().with_show_bands(false).dom();
        assert!(tc::find_all(&dom, GAUGE_BAND_CLASS).is_empty());
    }

    #[test]
    fn a_value_at_the_start_of_the_range_draws_no_arc() {
        let dom = sample().with_value(0.0).dom();
        assert!(tc::find(&dom, GAUGE_VALUE_CLASS).is_none());
        assert!(
            tc::find(&dom, GAUGE_TRACK_CLASS).is_some(),
            "the track stays"
        );
    }

    #[test]
    fn the_value_wears_the_colour_of_its_band_or_the_accent() {
        let skin = flat::gauge_skin();
        assert_eq!(value_color(&sample(), &skin), skin.warn);
        assert_eq!(value_color(&sample().with_value(20.0), &skin), skin.ok);
        assert_eq!(value_color(&sample().with_value(95.0), &skin), skin.bad);
        let unbanded = Gauge::create(50.0, 0.0, 100.0);
        assert_eq!(value_color(&unbanded, &skin), skin.accent);
        let own = ChartColor::same(azul_css::props::basic::color::ColorU::rgb(1, 2, 3));
        assert_eq!(value_color(&unbanded.clone().with_accent(own), &skin), own);
        let custom =
            unbanded.with_band(GaugeBand::create(0.0, 100.0, GaugeBandKind::Ok).with_color(own));
        assert_eq!(
            value_color(&custom, &skin),
            own,
            "a band's own colour wins over its kind's"
        );
    }

    #[test]
    fn the_value_and_the_label_are_written_in_the_middle() {
        let all = texts(&sample().dom());
        assert!(all.iter().any(|t| t == "73%"), "{all:?}");
        assert!(all.iter().any(|t| t == "CPU"), "{all:?}");
    }

    #[test]
    fn a_gauge_is_a_meter_named_by_its_label_with_its_summary_as_value() {
        let dom = sample().dom();
        let info = dom
            .root
            .get_accessibility_info()
            .expect("the root is the meter");
        assert_eq!(info.role, AccessibilityRole::Indicator);
        assert_eq!(
            info.accessibility_name
                .as_ref()
                .map(|n| n.as_str().to_string()),
            Some(String::from("CPU"))
        );
        assert_eq!(
            info.accessibility_value
                .as_ref()
                .map(|n| n.as_str().to_string()),
            Some(String::from("73% (warning)"))
        );
        let named = sample().with_accessibility_name("Processor load").dom();
        let info = named.root.get_accessibility_info().expect("the meter");
        assert_eq!(
            info.accessibility_name
                .as_ref()
                .map(|n| n.as_str().to_string()),
            Some(String::from("Processor load"))
        );
    }

    #[test]
    fn a_gauge_is_not_a_tab_stop() {
        for dom in [sample().dom(), linear().dom()] {
            assert!(tc::focusable(&dom).is_empty(), "a meter takes no focus");
        }
    }

    #[test]
    fn a_linear_gauge_fills_its_bar_as_far_as_the_value() {
        let dom = linear().with_value(25.0).with_size(200.0).dom();
        let bar = tc::find(&dom, GAUGE_BAR_CLASS).expect("the bar");
        assert!(declares(
            bar,
            &CssProperty::const_width(LayoutWidth::px(200.0))
        ));
        let value = tc::find(&dom, GAUGE_VALUE_CLASS).expect("the value's fill");
        assert!(declares(
            value,
            &CssProperty::const_width(LayoutWidth::px(50.0))
        ));
        assert_eq!(tc::find_all(&dom, GAUGE_BAND_CLASS).len(), 3);
        let all = texts(&dom);
        assert_eq!(all, vec![String::from("CPU"), String::from("25%")]);
        assert!(
            !value
                .root
                .style
                .iter_inline_properties()
                .any(|(p, _)| p.get_type() == CssPropertyType::Width
                    && *p == CssProperty::const_width(LayoutWidth::px(200.0))),
            "the fill is not the whole bar"
        );
    }

    #[test]
    fn a_gauge_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "gauge",
            || sample().dom(),
            |theme| sample().with_theme(theme).dom(),
        );
        checks::assert_follows_the_app_theme(
            "gauge (linear)",
            || linear().dom(),
            |theme| linear().with_theme(theme).dom(),
        );
    }

    #[test]
    fn a_pinned_gauge_keeps_its_theme_invariants() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            tc::assert_theme_invariants("gauge", &sample().with_theme(theme).dom());
            tc::assert_theme_invariants("gauge (linear)", &linear().with_theme(theme).dom());
        }
    }
}
