//! Chart widget - line, area, bar (grouped / stacked), scatter, pie and donut
//! charts over series of numbers, drawn with the engine's own vector path.
//!
//! ```text
//!   Revenue by month                                   the title
//!   1.5K |          .--.                               gridlines at nice ticks
//!   1.0K |   .------'  '---.         +-------------+   (1-2-5 steps)
//!    500 |---'             '--       | Mar   1,234 |   the tooltip under the pointer
//!      0 +-----------------------    +-------------+
//!         Jan   Feb   Mar   Apr                        category / number ticks
//!   o North  o South                                   the legend
//! ```
//!
//! THE APP OWNS THE DATA: a chart is built from [`ChartSeries`] (a name and
//! points `(x, y)`, f64) and, for a category axis, the category names - a
//! series' point `i` then sits in category `i` (its `x` is the index). Bars
//! and lines over the same categories line up: a line's point is drawn at
//! its category's centre. A pie or a donut shows the FIRST series' values
//! over the categories. The widget reports a click (or Enter) on a point
//! through ONE hook, `on_select` ([`ChartSelection`]); the app stores it and
//! hands it back with `with_selected` to draw the point selected.
//!
//! DRAWING: the plot is a box with an SVG user space of one unit per px
//! (`SvgNodeData::ViewBox`), and every series is ONE node over it with an SVG
//! path (`SvgNodeData::Path`): bars, dots and wedges are its FILL (the node's
//! background clipped to the path), a line is its STROKE (the node's border
//! width and colour - `stroke` / `stroke-width` are their spellings). The
//! gridlines, the baseline and the crosshair are plain 1 px boxes. No second
//! renderer: what the engine paints for `<svg>` it paints here.
//!
//! LARGE SERIES: a line is DECIMATED per pixel column before it is drawn
//! ([`decimate_line`]: the first, lowest, highest and last point of every
//! column - the picture of 500k points is the picture of 4 per column), a
//! scatter keeps one dot per occupied cell ([`thin_scatter`]). The pointer
//! asks the FULL data: the nearest point by binary search on a sorted x.
//!
//! HOVER: the tooltip, the crosshair and one marker per series live in the
//! plot's overlay from the start, hidden; the pointer moves them with
//! `set_css_property` and rewrites the tip with `change_node_text` - a hover
//! never rebuilds the DOM.
//!
//! COLOURS: the series take a categorical palette in a FIXED order (blue,
//! orange, aqua, yellow, magenta, green, violet, red - light and dark steps,
//! checked for colour-blind separation on both themes' surfaces); a ninth
//! series starts the order again. The app's accent
//! (`ShellThemeAccent::colors`) is too dark and grey to tell series apart,
//! so it marks the selection and the focus ring, never a series; an app that
//! wants a series in its own colour sets it ([`ChartSeries::with_color`]).
//! Text never wears a series colour: labels, legend and tooltip write in the
//! theme's inks, a swatch beside them carries the identity.
//!
//! KEYBOARD AND A11Y: the plot is one Tab stop (role `Chart`, named by the
//! title, described by [`Chart::summary`]). Left / Right walk the points,
//! Up / Down the series, Home / End jump to the ends, Enter / Space select,
//! Escape hides the tooltip; the tooltip is a live region, so a screen reader
//! reads the point it shows. `with_show_table(true)` adds the data as a
//! table under the chart.
//!
//! Key types: [`Chart`], [`ChartKind`], [`ChartSeries`], [`ChartPoint`],
//! [`ChartSelection`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomNodeId, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, SvgNodeData,
        TabIndex,
    },
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    svg::{SvgLine, SvgMultiPolygon, SvgPath, SvgPathElement, SvgPathElementVec, SvgPathVec},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    props::{
        basic::{color::ColorU, SvgPoint, SvgQuadraticCurve},
        layout::{LayoutLeft, LayoutTop},
        property::CssProperty,
        style::StyleOpacity,
    },
    AzString, OptionF64, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::themes::{OptionUiTheme, UiTheme},
};

// ---- classes ----

/// The widget's root.
pub const CHART_CLASS: &str = "__azul-native-chart";
/// The title over the chart.
pub const TITLE_CLASS: &str = "__azul-native-chart-title";
/// The box of the axes, their labels and the plot.
pub const FRAME_CLASS: &str = "__azul-native-chart-frame";
/// The plot: the user space the series are drawn in.
pub const PLOT_CLASS: &str = "__azul-native-chart-plot";
/// A gridline across the plot.
pub const GRID_CLASS: &str = "__azul-native-chart-grid";
/// The baseline (the zero line, or the plot's bottom edge).
pub const BASELINE_CLASS: &str = "__azul-native-chart-baseline";
/// One series' marks (a line, an area, bars, dots or wedges).
pub const SERIES_CLASS: &str = "__azul-native-chart-series";
/// A tick label on an axis.
pub const TICK_CLASS: &str = "__azul-native-chart-tick";
/// An axis' title.
pub const AXIS_TITLE_CLASS: &str = "__azul-native-chart-axis-title";
/// The overlay over the plot: the pointer's and the keyboard's target.
pub const OVERLAY_CLASS: &str = "__azul-native-chart-overlay";
/// The vertical crosshair at the hovered x.
pub const CROSSHAIR_CLASS: &str = "__azul-native-chart-crosshair";
/// The marker on the hovered point (one per series).
pub const MARKER_CLASS: &str = "__azul-native-chart-marker";
/// The tooltip over the hovered point.
pub const TOOLTIP_CLASS: &str = "__azul-native-chart-tooltip";
/// The ring around the selected point.
pub const SELECTION_CLASS: &str = "__azul-native-chart-selection";
/// The legend under the chart.
pub const LEGEND_CLASS: &str = "__azul-native-chart-legend";
/// One legend entry: a swatch and a name.
pub const LEGEND_ITEM_CLASS: &str = "__azul-native-chart-legend-item";
/// A legend entry's colour swatch.
pub const SWATCH_CLASS: &str = "__azul-native-chart-swatch";
/// The data table under the chart (`with_show_table`).
pub const TABLE_CLASS: &str = "__azul-native-chart-table";

// ---- metrics (px) ----

/// The chart's size when the app sets none.
pub const DEFAULT_WIDTH: f32 = 640.0;
/// See [`DEFAULT_WIDTH`].
pub const DEFAULT_HEIGHT: f32 = 320.0;
/// The title row's height.
pub const TITLE_HEIGHT: f32 = 28.0;
/// The legend row's height.
pub const LEGEND_HEIGHT: f32 = 28.0;
/// The column of y tick labels left of the plot.
pub const Y_GUTTER: f32 = 52.0;
/// The row of x tick labels under the plot.
pub const X_GUTTER: f32 = 22.0;
/// The line an axis title takes.
pub const AXIS_TITLE_HEIGHT: f32 = 18.0;
/// Air right of the plot (the last x label is centred on the edge) and over
/// it (the top y label is centred on the top gridline).
pub const PLOT_PAD_RIGHT: f32 = 16.0;
/// See [`PLOT_PAD_RIGHT`].
pub const PLOT_PAD_TOP: f32 = 10.0;
/// The least distance between two labelled x ticks.
pub const MIN_X_TICK_PX: f32 = 72.0;
/// The least distance between two labelled y ticks.
pub const MIN_Y_TICK_PX: f32 = 36.0;
/// A bar is never thicker than this; the band's rest is air.
pub const MAX_BAR_PX: f32 = 24.0;
/// The gap between touching marks (bars of a group, stacked segments, pie
/// wedges): the surface colour separates them, never a border.
pub const SURFACE_GAP_PX: f32 = 2.0;
/// The rounded data end of a bar.
pub const BAR_RADIUS_PX: f32 = 4.0;
/// A line's stroke.
pub const LINE_WIDTH_PX: f32 = 2.0;
/// A scatter dot's radius (an 8 px dot), and the radius of a dense one.
pub const DOT_RADIUS_PX: f32 = 4.0;
/// See [`DOT_RADIUS_PX`].
pub const DENSE_DOT_RADIUS_PX: f32 = 2.0;
/// More dots than this are drawn dense (smaller, on a finer grid).
pub const DENSE_DOTS: usize = 2000;
/// The hovered point's marker.
pub const MARKER_PX: f32 = 10.0;
/// How near (px) the pointer must come to a point for the tooltip to show.
pub const HOVER_REACH_PX: f32 = 32.0;
/// A donut's hole, as a fraction of its radius.
pub const DONUT_HOLE: f32 = 0.6;
/// More points than this in a series: the table view lists a summary row
/// for it instead of every point.
pub const MAX_TABLE_ROWS: usize = 100;
/// Series past this many reuse the palette from its first colour.
pub const PALETTE_LEN: usize = 8;

// ---- data ----

/// What a chart draws.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ChartKind {
    /// A line through each series' points: change over time.
    #[default]
    Line,
    /// [`Self::Line`] with a light wash of the series colour down to the
    /// baseline.
    Area,
    /// One bar per series in every category, side by side.
    Bar,
    /// The series stacked into one bar per category.
    StackedBar,
    /// A dot per point: how two measures relate.
    Scatter,
    /// The first series' values over the categories, as slices of a disc.
    Pie,
    /// [`Self::Pie`] with a hole in the middle.
    Donut,
}

impl ChartKind {
    /// A pie or a donut: no axes, the categories are the slices.
    #[must_use]
    pub const fn is_round(self) -> bool {
        matches!(self, Self::Pie | Self::Donut)
    }

    /// A bar chart, grouped or stacked: a category axis from zero.
    #[must_use]
    pub const fn is_bar(self) -> bool {
        matches!(self, Self::Bar | Self::StackedBar)
    }

    /// The chart's name, for the text summary ("line chart").
    #[must_use]
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Line => "line chart",
            Self::Area => "area chart",
            Self::Bar => "bar chart",
            Self::StackedBar => "stacked bar chart",
            Self::Scatter => "scatter chart",
            Self::Pie => "pie chart",
            Self::Donut => "donut chart",
        }
    }
}

/// One point of a series: `x` (a number, or the category's index) and `y`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct ChartPoint {
    /// Along the x axis: a number, or on a category axis the category's
    /// index.
    pub x: f64,
    /// Along the y axis.
    pub y: f64,
}

impl ChartPoint {
    /// The point `(x, y)`.
    #[must_use]
    pub const fn create(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl_option!(
    ChartPoint,
    OptionChartPoint,
    [Debug, Clone, Copy, PartialEq, PartialOrd]
);
impl_vec!(
    ChartPoint,
    ChartPointVec,
    ChartPointVecDestructor,
    ChartPointVecDestructorType,
    ChartPointVecSlice,
    OptionChartPoint
);
impl_vec_clone!(ChartPoint, ChartPointVec, ChartPointVecDestructor);
impl_vec_debug!(ChartPoint, ChartPointVec);
impl_vec_mut!(ChartPoint, ChartPointVec);

azul_css::impl_vec_partialeq!(ChartPoint, ChartPointVec);

/// A colour in the light mode and its step for the dark mode.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChartColor {
    /// On the light surface.
    pub light: ColorU,
    /// On the dark surface.
    pub dark: ColorU,
}

impl ChartColor {
    /// `light` by day, `dark` at night.
    #[must_use]
    pub const fn create(light: ColorU, dark: ColorU) -> Self {
        Self { light, dark }
    }

    /// The same colour in both modes.
    #[must_use]
    pub const fn same(color: ColorU) -> Self {
        Self {
            light: color,
            dark: color,
        }
    }
}

impl_option!(
    ChartColor,
    OptionChartColor,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// One series: a name (the legend's, the tooltip's) and its points.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    /// The points, in drawing order (a line joins them in this order).
    pub points: ChartPointVec,
    /// The name ("North", "Revenue").
    pub name: AzString,
    /// The series' colour, or `None` for its slot in the palette.
    pub color: OptionChartColor,
}

impl ChartSeries {
    /// The series `name` over `points`.
    #[must_use]
    pub const fn create(name: AzString, points: ChartPointVec) -> Self {
        Self {
            points,
            name,
            color: OptionChartColor::None,
        }
    }

    /// The series `name` with one value per category: point `i` is
    /// `(i, values[i])`.
    #[must_use]
    pub fn from_values(name: AzString, values: azul_css::F32Vec) -> Self {
        let points: Vec<ChartPoint> = values
            .as_slice()
            .iter()
            .enumerate()
            .map(|(i, v)| ChartPoint::create(i as f64, f64::from(*v)))
            .collect();
        Self::create(name, ChartPointVec::from_vec(points))
    }

    /// The series' own colour instead of its palette slot.
    pub const fn set_color(&mut self, color: ChartColor) {
        self.color = OptionChartColor::Some(color);
    }

    /// [`Self::set_color`] for the builder chain.
    #[must_use]
    pub const fn with_color(mut self, color: ChartColor) -> Self {
        self.set_color(color);
        self
    }
}

impl_option!(
    ChartSeries,
    OptionChartSeries,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ChartSeries,
    ChartSeriesVec,
    ChartSeriesVecDestructor,
    ChartSeriesVecDestructorType,
    ChartSeriesVecSlice,
    OptionChartSeries
);
impl_vec_clone!(ChartSeries, ChartSeriesVec, ChartSeriesVecDestructor);
impl_vec_debug!(ChartSeries, ChartSeriesVec);
impl_vec_mut!(ChartSeries, ChartSeriesVec);

azul_css::impl_vec_partialeq!(ChartSeries, ChartSeriesVec);

/// A point the user picked: which series, which of its points, and the
/// point's values.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct ChartSelection {
    /// The point's x (on a category axis, the category's index).
    pub x: f64,
    /// The point's y.
    pub y: f64,
    /// The series' index.
    pub series: usize,
    /// The point's index in its series.
    pub index: usize,
}

impl ChartSelection {
    /// Point `index` of series `series`, at `(x, y)`.
    #[must_use]
    pub const fn create(series: usize, index: usize, x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            series,
            index,
        }
    }
}

impl_option!(
    ChartSelection,
    OptionChartSelection,
    [Debug, Clone, Copy, PartialEq, PartialOrd]
);

/// Callback invoked when the user picks a point (a click, Enter / Space).
pub type ChartOnSelectCallbackType = extern "C" fn(RefAny, CallbackInfo, ChartSelection) -> Update;
impl_widget_callback!(
    ChartOnSelect,
    OptionChartOnSelect,
    ChartOnSelectCallback,
    ChartOnSelectCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ChartOnSelectCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CHART_ON_SELECT_INVOKER,
    invoker_ty:     AzChartOnSelectCallbackInvoker,
    thunk_fn:       az_chart_on_select_callback_thunk,
    setter_fn:      AzApp_setChartOnSelectCallbackInvoker,
    from_handle_fn: AzChartOnSelectCallback_createFromHostHandle,
    from_handle_byref_fn: AzChartOnSelectCallback_createFromHostHandleByref,
    extra_args:     [ selection: ChartSelection ],
}

// ---- the widget ----

/// A chart: series of numbers as lines, bars, dots or slices, with axes,
/// gridlines, a legend, a tooltip under the pointer and a text summary.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    /// The data, one entry per series.
    pub series: ChartSeriesVec,
    /// The categories along the x axis (a series' point `i` is in category
    /// `i`), or empty for a number axis.
    pub categories: StringVec,
    /// The title over the chart, and the chart's accessible name; empty for
    /// none.
    pub title: AzString,
    /// The x axis' title, under its labels; empty for none.
    pub x_title: AzString,
    /// The y axis' title, over its labels; empty for none.
    pub y_title: AzString,
    /// What a click (or Enter / Space) on a point reports.
    pub on_select: OptionChartOnSelect,
    /// The point drawn selected (ringed in the accent), if any.
    pub selected: OptionChartSelection,
    /// The y axis' lower end, or `None` to fit the data (a bar chart always
    /// includes zero).
    pub y_min: OptionF64,
    /// The y axis' upper end, or `None` to fit the data.
    pub y_max: OptionF64,
    /// The chart's width in px.
    pub width: f32,
    /// The chart's height in px (the table view, if shown, comes under it).
    pub height: f32,
    /// What it draws.
    pub kind: ChartKind,
    /// The widget theme this chart is PINNED to (`with_theme`), or `None` to
    /// follow the app theme.
    pub theme: OptionUiTheme,
    /// The legend under the plot (shown for two or more series, and for a
    /// pie's slices).
    pub show_legend: bool,
    /// Gridlines at the y ticks.
    pub show_grid: bool,
    /// The data as a table under the chart.
    pub show_table: bool,
}

impl Chart {
    /// An empty `kind` chart of `width` x `height` px, with gridlines and a
    /// legend.
    #[must_use]
    pub fn create(kind: ChartKind, width: f32, height: f32) -> Self {
        Self {
            series: ChartSeriesVec::from_const_slice(&[]),
            categories: StringVec::from_const_slice(&[]),
            title: AzString::from_const_str(""),
            x_title: AzString::from_const_str(""),
            y_title: AzString::from_const_str(""),
            on_select: OptionChartOnSelect::None,
            selected: OptionChartSelection::None,
            y_min: OptionF64::None,
            y_max: OptionF64::None,
            width,
            height,
            kind,
            theme: OptionUiTheme::None,
            show_legend: true,
            show_grid: true,
            show_table: false,
        }
    }

    /// What it draws.
    pub const fn set_kind(&mut self, kind: ChartKind) {
        self.kind = kind;
    }

    /// [`Self::set_kind`] for the builder chain.
    #[must_use]
    pub const fn with_kind(mut self, kind: ChartKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// The chart's size in px.
    pub const fn set_size(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
    }

    /// [`Self::set_size`] for the builder chain.
    #[must_use]
    pub const fn with_size(mut self, width: f32, height: f32) -> Self {
        self.set_size(width, height);
        self
    }

    /// Every series at once.
    pub fn set_series(&mut self, series: ChartSeriesVec) {
        self.series = series;
    }

    /// [`Self::set_series`] for the builder chain.
    #[must_use]
    pub fn with_series(mut self, series: ChartSeriesVec) -> Self {
        self.set_series(series);
        self
    }

    /// Adds a series after the others.
    pub fn add_series(&mut self, series: ChartSeries) {
        let mut all = core::mem::replace(&mut self.series, ChartSeriesVec::from_const_slice(&[]))
            .into_library_owned_vec();
        all.push(series);
        self.series = ChartSeriesVec::from_vec(all);
    }

    /// [`Self::add_series`] for the builder chain.
    #[must_use]
    pub fn with_added_series(mut self, series: ChartSeries) -> Self {
        self.add_series(series);
        self
    }

    /// The category names along the x axis (empty: a number axis).
    pub fn set_categories(&mut self, categories: StringVec) {
        self.categories = categories;
    }

    /// [`Self::set_categories`] for the builder chain.
    #[must_use]
    pub fn with_categories(mut self, categories: StringVec) -> Self {
        self.set_categories(categories);
        self
    }

    /// The title over the chart (also its accessible name).
    pub fn set_title(&mut self, title: AzString) {
        self.title = title;
    }

    /// [`Self::set_title`] for the builder chain.
    #[must_use]
    pub fn with_title(mut self, title: AzString) -> Self {
        self.set_title(title);
        self
    }

    /// The axes' titles: `x_title` under the x labels, `y_title` over the y
    /// labels (empty for none).
    pub fn set_axis_titles(&mut self, x_title: AzString, y_title: AzString) {
        self.x_title = x_title;
        self.y_title = y_title;
    }

    /// [`Self::set_axis_titles`] for the builder chain.
    #[must_use]
    pub fn with_axis_titles(mut self, x_title: AzString, y_title: AzString) -> Self {
        self.set_axis_titles(x_title, y_title);
        self
    }

    /// Fixes the y axis to `[min, max]` (rounded out to nice ticks), so it
    /// stays put while the data changes - a dashboard's filter.
    pub const fn set_y_range(&mut self, min: f64, max: f64) {
        self.y_min = OptionF64::Some(min);
        self.y_max = OptionF64::Some(max);
    }

    /// [`Self::set_y_range`] for the builder chain.
    #[must_use]
    pub const fn with_y_range(mut self, min: f64, max: f64) -> Self {
        self.set_y_range(min, max);
        self
    }

    /// The point drawn selected.
    pub const fn set_selected(&mut self, selected: ChartSelection) {
        self.selected = OptionChartSelection::Some(selected);
    }

    /// [`Self::set_selected`] for the builder chain.
    #[must_use]
    pub const fn with_selected(mut self, selected: ChartSelection) -> Self {
        self.set_selected(selected);
        self
    }

    /// The legend on or off.
    pub const fn set_show_legend(&mut self, show_legend: bool) {
        self.show_legend = show_legend;
    }

    /// [`Self::set_show_legend`] for the builder chain.
    #[must_use]
    pub const fn with_show_legend(mut self, show_legend: bool) -> Self {
        self.set_show_legend(show_legend);
        self
    }

    /// The gridlines on or off.
    pub const fn set_show_grid(&mut self, show_grid: bool) {
        self.show_grid = show_grid;
    }

    /// [`Self::set_show_grid`] for the builder chain.
    #[must_use]
    pub const fn with_show_grid(mut self, show_grid: bool) -> Self {
        self.set_show_grid(show_grid);
        self
    }

    /// The data table under the chart on or off.
    pub const fn set_show_table(&mut self, show_table: bool) {
        self.show_table = show_table;
    }

    /// [`Self::set_show_table`] for the builder chain.
    #[must_use]
    pub const fn with_show_table(mut self, show_table: bool) -> Self {
        self.set_show_table(show_table);
        self
    }

    /// What a click (or Enter / Space) on a point reports.
    pub fn set_on_select<C: Into<ChartOnSelectCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_select = Some(ChartOnSelect {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_select`] for the builder chain.
    #[must_use]
    pub fn with_on_select<C: Into<ChartOnSelectCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_select(data, callback);
        self
    }

    /// Pin the widget theme; unset, the chart follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty line chart and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(ChartKind::Line, DEFAULT_WIDTH, DEFAULT_HEIGHT);
        core::mem::swap(&mut s, self);
        s
    }
}

impl Default for Chart {
    fn default() -> Self {
        Self::create(ChartKind::Line, DEFAULT_WIDTH, DEFAULT_HEIGHT)
    }
}

// ==== the math (pure, unit-tested) ====

/// Slack for float comparisons in the tick math: `0.1 / 0.1` may come out a
/// hair above 1, and must still read as 1.
const TICK_EPS: f64 = 1e-9;

/// The ticks of an axis: from `min` to `max` (both ticks) every `step`,
/// the step 1, 2 or 5 times a power of ten.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NiceTicks {
    /// The axis' lower end: the first tick.
    pub min: f64,
    /// The axis' upper end: the last tick.
    pub max: f64,
    /// The distance between two ticks.
    pub step: f64,
}

impl NiceTicks {
    /// The tick values, `min` to `max`, each snapped to the step's grid (so
    /// three steps of 0.1 read 0.3, not 0.30000000000000004 when written).
    #[must_use]
    pub fn values(&self) -> Vec<f64> {
        if !(self.step.is_finite() && self.step > 0.0) || self.max < self.min {
            return alloc::vec![self.min];
        }
        // Bounded: a degenerate axis must not allocate a million ticks.
        let n = (((self.max - self.min) / self.step).round() as usize).min(10_000);
        (0..=n)
            .map(|i| {
                let v = self.min + i as f64 * self.step;
                (v / self.step).round() * self.step
            })
            .collect()
    }
}

/// The smallest "nice" step not below `raw`: 1, 2 or 5 times a power of
/// ten. A step that is not a positive finite number is 1.
#[must_use]
pub fn nice_step(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let exp = raw.log10().floor();
    let base = 10f64.powi(exp as i32);
    let f = raw / base;
    let nice = if f <= 1.0 + TICK_EPS {
        1.0
    } else if f <= 2.0 + TICK_EPS {
        2.0
    } else if f <= 5.0 + TICK_EPS {
        5.0
    } else {
        10.0
    };
    nice * base
}

/// Nice ticks over `[lo, hi]` in about `target` steps: the step from
/// [`nice_step`], the ends rounded OUT to a multiple of it. An empty range
/// (`lo == hi`) is widened around its value; the ends may come in either
/// order; a non-finite end reads as 0.
#[must_use]
pub fn nice_ticks(lo: f64, hi: f64, target: usize) -> NiceTicks {
    let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
    let (mut lo, mut hi) = (finite(lo), finite(hi));
    if lo > hi {
        core::mem::swap(&mut lo, &mut hi);
    }
    if hi - lo <= f64::EPSILON * lo.abs().max(hi.abs()).max(1.0) {
        if lo == 0.0 {
            hi = 1.0;
        } else {
            let pad = lo.abs() * 0.1;
            lo -= pad;
            hi += pad;
        }
    }
    let step = nice_step((hi - lo) / target.max(1) as f64);
    NiceTicks {
        min: (lo / step + TICK_EPS).floor() * step,
        max: (hi / step - TICK_EPS).ceil() * step,
        step,
    }
}

/// Where a chart's values land in its plot: the x and y domains and the
/// plot's size in px. A category axis (`bands > 0`) puts category `i` at the
/// centre of the `i`-th of `bands` equal bands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotFrame {
    /// The x domain's left end (a number axis).
    pub x_min: f64,
    /// The x domain's right end (a number axis).
    pub x_max: f64,
    /// The y domain's lower end (the plot's bottom edge).
    pub y_min: f64,
    /// The y domain's upper end (the plot's top edge).
    pub y_max: f64,
    /// The plot's width in px.
    pub width: f32,
    /// The plot's height in px.
    pub height: f32,
    /// The number of categories along x, or 0 for a number axis.
    pub bands: usize,
}

impl PlotFrame {
    /// A band's width in px (a category axis), or the whole width.
    #[must_use]
    pub fn band(&self) -> f32 {
        if self.bands > 0 {
            self.width / self.bands as f32
        } else {
            self.width
        }
    }

    /// `x` in px from the plot's left edge.
    #[must_use]
    pub fn px_x(&self, x: f64) -> f32 {
        if self.bands > 0 {
            return ((x + 0.5) * f64::from(self.band())) as f32;
        }
        let span = self.x_max - self.x_min;
        if !(span.abs() > 0.0) {
            return self.width / 2.0;
        }
        ((x - self.x_min) / span * f64::from(self.width)) as f32
    }

    /// `y` in px from the plot's top edge.
    #[must_use]
    pub fn px_y(&self, y: f64) -> f32 {
        let span = self.y_max - self.y_min;
        if !(span.abs() > 0.0) {
            return self.height / 2.0;
        }
        (f64::from(self.height) * (1.0 - (y - self.y_min) / span)) as f32
    }

    /// The x value at `px` from the plot's left edge (on a category axis,
    /// the fractional index whose band centre is there).
    #[must_use]
    pub fn x_at(&self, px: f32) -> f64 {
        if self.bands > 0 {
            let band = f64::from(self.band());
            return if band > 0.0 {
                f64::from(px) / band - 0.5
            } else {
                0.0
            };
        }
        let w = f64::from(self.width);
        if w > 0.0 {
            f64::from(px).mul_add((self.x_max - self.x_min) / w, self.x_min)
        } else {
            self.x_min
        }
    }

    /// The y value at `px` from the plot's top edge.
    #[must_use]
    pub fn y_at(&self, px: f32) -> f64 {
        let h = f64::from(self.height);
        if h > 0.0 {
            (1.0 - f64::from(px) / h).mul_add(self.y_max - self.y_min, self.y_min)
        } else {
            self.y_min
        }
    }
}

/// One pixel column's run of a line: its first and last point and the
/// lowest and highest in between (indices).
#[derive(Debug, Clone, Copy)]
struct ColumnRun {
    col: i64,
    first: usize,
    last: usize,
    lo: usize,
    hi: usize,
}

impl ColumnRun {
    /// The run's points, each once, in drawing order.
    fn flush(&self, out: &mut Vec<usize>) {
        let mut idx = [self.first, self.lo, self.hi, self.last];
        idx.sort_unstable();
        for (k, &i) in idx.iter().enumerate() {
            if k == 0 || i != idx[k - 1] {
                out.push(i);
            }
        }
    }
}

/// The points of a line worth drawing in `frame`: for every run of points
/// in one pixel column, its first, lowest, highest and last point (M4), in
/// drawing order - the picture of the whole line at a few points per
/// column. A line short enough to draw whole comes back whole. Points with
/// a non-finite coordinate are left out.
#[must_use]
pub fn decimate_line(points: &[ChartPoint], frame: &PlotFrame) -> Vec<usize> {
    let finite = |p: &ChartPoint| p.x.is_finite() && p.y.is_finite();
    let columns = frame.width.max(1.0).ceil() as usize;
    if points.len() <= columns * 4 {
        return (0..points.len()).filter(|&i| finite(&points[i])).collect();
    }
    let mut out = Vec::with_capacity(columns * 4 + 4);
    let mut run: Option<ColumnRun> = None;
    for (i, p) in points.iter().enumerate() {
        if !finite(p) {
            continue;
        }
        let col = frame.px_x(p.x).floor() as i64;
        let same_column = matches!(&run, Some(r) if r.col == col);
        if same_column {
            if let Some(r) = run.as_mut() {
                r.last = i;
                if p.y < points[r.lo].y {
                    r.lo = i;
                }
                if p.y > points[r.hi].y {
                    r.hi = i;
                }
            }
        } else {
            if let Some(r) = run.take() {
                r.flush(&mut out);
            }
            run = Some(ColumnRun {
                col,
                first: i,
                last: i,
                lo: i,
                hi: i,
            });
        }
    }
    if let Some(r) = run {
        r.flush(&mut out);
    }
    out
}

/// The dots of a scatter worth drawing in `frame`: the first dot of every
/// `cell` x `cell` px cell it occupies, in drawing order. Dots outside the
/// plot or with a non-finite coordinate are left out.
#[must_use]
pub fn thin_scatter(points: &[ChartPoint], frame: &PlotFrame, cell: f32) -> Vec<usize> {
    let cell = if cell.is_finite() && cell > 0.0 {
        cell
    } else {
        1.0
    };
    let cols = (frame.width.max(0.0) / cell).floor() as usize + 1;
    let rows = (frame.height.max(0.0) / cell).floor() as usize + 1;
    let mut taken = alloc::vec![false; cols * rows];
    let mut out = Vec::new();
    for (i, p) in points.iter().enumerate() {
        if !(p.x.is_finite() && p.y.is_finite()) {
            continue;
        }
        let (x, y) = (frame.px_x(p.x), frame.px_y(p.y));
        if !(x >= 0.0 && y >= 0.0 && x <= frame.width && y <= frame.height) {
            continue;
        }
        let c = ((x / cell) as usize).min(cols - 1);
        let r = ((y / cell) as usize).min(rows - 1);
        let k = r * cols + c;
        if !taken[k] {
            taken[k] = true;
            out.push(i);
        }
    }
    out
}

/// Whether the points' x never decreases (a line over time).
#[must_use]
pub fn is_sorted_by_x(points: &[ChartPoint]) -> bool {
    points.windows(2).all(|w| w[0].x <= w[1].x)
}

/// The point whose x is nearest to `x` in points sorted by x (binary
/// search); ties go to the earlier point.
#[must_use]
pub fn nearest_by_x(points: &[ChartPoint], x: f64) -> Option<usize> {
    if points.is_empty() {
        return None;
    }
    let i = points.partition_point(|p| p.x < x);
    if i == 0 {
        return Some(0);
    }
    if i >= points.len() {
        return Some(points.len() - 1);
    }
    let (a, b) = (i - 1, i);
    if x - points[a].x <= points[b].x - x {
        Some(a)
    } else {
        Some(b)
    }
}

/// How a tick value is written: in `unit`s (1, thousands, millions,
/// billions) with `suffix` ("", "K", "M", "B") and `decimals` places - one
/// format for every tick of an axis, so they line up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickFormat {
    /// What one written unit is worth.
    pub unit: f64,
    /// The unit's suffix.
    pub suffix: &'static str,
    /// The places after the point.
    pub decimals: usize,
}

impl TickFormat {
    /// The format of an axis with these ticks: the unit from its largest
    /// magnitude (thousands from 10,000 on), the places from its step.
    #[must_use]
    pub fn of(ticks: &NiceTicks) -> Self {
        let big = ticks.min.abs().max(ticks.max.abs());
        let (unit, suffix) = if big >= 1e9 {
            (1e9, "B")
        } else if big >= 1e6 {
            (1e6, "M")
        } else if big >= 1e4 {
            (1e3, "K")
        } else {
            (1.0, "")
        };
        let step = ticks.step / unit;
        let decimals = if step.is_finite() && step > 0.0 && step < 1.0 {
            (-step.log10() - TICK_EPS).ceil().max(0.0) as usize
        } else {
            0
        };
        Self {
            unit,
            suffix,
            decimals: decimals.min(9),
        }
    }

    /// `v` written in this format ("12.5K", "1,500", "0.25"); zero is
    /// written bare ("0", "0.0"), without a sign or a suffix.
    #[must_use]
    pub fn format(&self, v: f64) -> String {
        if !v.is_finite() {
            return String::from("-");
        }
        let text = format!("{:.*}", self.decimals, v / self.unit);
        let is_zero = text
            .trim_start_matches('-')
            .chars()
            .all(|c| c == '0' || c == '.');
        if is_zero {
            return format!("{:.*}", self.decimals, 0.0);
        }
        format!("{}{}", group_thousands(&text), self.suffix)
    }
}

/// `text` (a plain decimal number) with a comma between every three digits
/// of its whole part: "1234567.5" -> "1,234,567.5".
fn group_thousands(text: &str) -> String {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", text),
    };
    let (int, frac) = match rest.find('.') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let mut out = String::with_capacity(text.len() + int.len() / 3);
    out.push_str(sign);
    let digits = int.as_bytes();
    for (k, d) in digits.iter().enumerate() {
        if k > 0 && (digits.len() - k) % 3 == 0 {
            out.push(',');
        }
        out.push(char::from(*d));
    }
    out.push_str(frac);
    out
}

/// A value as the tooltip and the table write it: whole numbers with
/// thousands separators, others with two places (four significant digits
/// under 1), trailing zeros dropped.
#[must_use]
pub fn format_value(v: f64) -> String {
    if !v.is_finite() {
        return String::from("-");
    }
    if v == 0.0 {
        return String::from("0");
    }
    let text = if v.abs() >= 1.0 {
        format!("{v:.2}")
    } else {
        let places = (3.0 - v.abs().log10().floor()).clamp(0.0, 12.0) as usize;
        format!("{v:.places$}")
    };
    let trimmed: &str = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text.as_str()
    };
    group_thousands(trimmed)
}

#[cfg(test)]
mod math_tests {
    use super::*;

    fn is_one_two_five(step: f64) -> bool {
        let exp = step.log10().floor();
        let f = step / 10f64.powf(exp);
        [1.0, 2.0, 5.0, 10.0].iter().any(|n| (f - n).abs() < 1e-6)
    }

    #[test]
    fn a_nice_step_is_the_next_one_two_or_five() {
        assert_eq!(nice_step(1.0), 1.0);
        assert_eq!(nice_step(1.1), 2.0);
        assert_eq!(nice_step(3.0), 5.0);
        assert_eq!(nice_step(7.0), 10.0);
        assert_eq!(nice_step(250.0), 500.0);
        assert!((nice_step(0.3) - 0.5).abs() < 1e-12);
        assert!((nice_step(0.1) - 0.1).abs() < 1e-12, "0.1 is already nice");
        assert!(
            (nice_step(0.02) - 0.02).abs() < 1e-12,
            "0.02 is already nice"
        );
    }

    #[test]
    fn a_step_that_is_not_a_positive_number_is_one() {
        assert_eq!(nice_step(0.0), 1.0);
        assert_eq!(nice_step(-3.0), 1.0);
        assert_eq!(nice_step(f64::NAN), 1.0);
        assert_eq!(nice_step(f64::INFINITY), 1.0);
    }

    #[test]
    fn every_nice_step_is_one_two_or_five_times_a_power_of_ten() {
        let mut raw = 1e-6;
        while raw < 1e9 {
            let step = nice_step(raw);
            assert!(step >= raw * (1.0 - 1e-9), "{step} is below {raw}");
            assert!(is_one_two_five(step), "{step} (from {raw}) is not 1-2-5");
            assert!(step <= raw * 2.5 + 1e-12, "{step} is too coarse for {raw}");
            raw *= 1.37;
        }
    }

    #[test]
    fn nice_ticks_round_the_ends_out_to_the_step() {
        let t = nice_ticks(3.0, 97.0, 5);
        assert_eq!((t.min, t.max, t.step), (0.0, 100.0, 20.0));
        assert_eq!(t.values(), vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);

        let t = nice_ticks(-12.0, 47.0, 6);
        assert_eq!((t.min, t.max, t.step), (-20.0, 50.0, 10.0));
    }

    #[test]
    fn nice_ticks_cover_the_data_whatever_the_order_of_the_ends() {
        let t = nice_ticks(97.0, 3.0, 5);
        assert!(t.min <= 3.0 && t.max >= 97.0);
    }

    #[test]
    fn small_fractions_get_fractional_ticks() {
        let t = nice_ticks(0.001, 0.0093, 4);
        assert!((t.step - 0.005).abs() < 1e-12);
        assert!(t.min.abs() < 1e-12);
        assert!((t.max - 0.01).abs() < 1e-12);
    }

    #[test]
    fn an_empty_range_is_widened_around_its_value() {
        let t = nice_ticks(5.0, 5.0, 5);
        assert!(t.min < 5.0 && t.max > 5.0, "{t:?}");
        let t = nice_ticks(0.0, 0.0, 5);
        assert!(t.min <= 0.0 && t.max > 0.0, "{t:?}");
    }

    #[test]
    fn a_non_finite_end_reads_as_zero() {
        let t = nice_ticks(f64::NAN, 10.0, 5);
        assert!(t.min.is_finite() && t.max.is_finite());
        assert_eq!(t.min, 0.0);
    }

    #[test]
    fn the_tick_count_stays_near_the_target() {
        for (lo, hi) in [
            (0.0, 1.0),
            (0.0, 97.0),
            (-3.3, 8.8),
            (1e3, 7.7e6),
            (0.02, 0.031),
        ] {
            for target in 2..12 {
                let n = nice_ticks(lo, hi, target).values().len() - 1;
                assert!(
                    n >= 1 && n <= target * 3,
                    "{lo}..{hi} / {target}: {n} steps"
                );
            }
        }
    }

    fn frame(bands: usize) -> PlotFrame {
        PlotFrame {
            x_min: 0.0,
            x_max: 100.0,
            y_min: 0.0,
            y_max: 50.0,
            width: 200.0,
            height: 100.0,
            bands,
        }
    }

    #[test]
    fn a_number_axis_maps_its_domain_onto_the_plot() {
        let f = frame(0);
        assert_eq!(f.px_x(0.0), 0.0);
        assert_eq!(f.px_x(100.0), 200.0);
        assert_eq!(f.px_x(25.0), 50.0);
        assert_eq!(f.px_y(0.0), 100.0, "y grows upwards");
        assert_eq!(f.px_y(50.0), 0.0);
        assert!((f.x_at(50.0) - 25.0).abs() < 1e-9);
    }

    #[test]
    fn a_category_sits_at_the_centre_of_its_band() {
        let f = frame(4);
        assert_eq!(f.band(), 50.0);
        assert_eq!(f.px_x(0.0), 25.0);
        assert_eq!(f.px_x(3.0), 175.0);
        assert!((f.x_at(125.0) - 2.0).abs() < 1e-9);
    }

    fn wave(n: usize) -> Vec<ChartPoint> {
        (0..n)
            .map(|i| {
                ChartPoint::create(
                    i as f64,
                    ((i as f64) * 0.001).sin() + ((i * 7919) % 101) as f64 * 0.01,
                )
            })
            .collect()
    }

    fn wave_frame(n: usize, width: f32) -> PlotFrame {
        PlotFrame {
            x_min: 0.0,
            x_max: (n - 1) as f64,
            y_min: -2.0,
            y_max: 2.0,
            width,
            height: 200.0,
            bands: 0,
        }
    }

    #[test]
    fn a_long_line_keeps_at_most_four_points_per_pixel_column() {
        let points = wave(500_000);
        let f = wave_frame(points.len(), 500.0);
        let kept = decimate_line(&points, &f);
        assert!(kept.len() <= 4 * 501, "{} points kept", kept.len());
        assert!(
            kept.len() >= 500,
            "every column keeps a point, got {}",
            kept.len()
        );
    }

    #[test]
    fn the_decimated_line_keeps_every_columns_extremes_and_its_ends() {
        let points = wave(100_000);
        let f = wave_frame(points.len(), 300.0);
        let kept = decimate_line(&points, &f);
        assert_eq!(kept.first(), Some(&0));
        assert_eq!(kept.last(), Some(&(points.len() - 1)));
        let col = |p: &ChartPoint| f.px_x(p.x).floor() as i64;
        let mut full = std::collections::BTreeMap::<i64, (u64, u64)>::new();
        for p in &points {
            let e = full
                .entry(col(p))
                .or_insert((f64::MAX.to_bits(), f64::MIN.to_bits()));
            e.0 = f64::from_bits(e.0).min(p.y).to_bits();
            e.1 = f64::from_bits(e.1).max(p.y).to_bits();
        }
        let mut seen = std::collections::BTreeMap::<i64, (u64, u64)>::new();
        for &i in &kept {
            let p = &points[i];
            let e = seen
                .entry(col(p))
                .or_insert((f64::MAX.to_bits(), f64::MIN.to_bits()));
            e.0 = f64::from_bits(e.0).min(p.y).to_bits();
            e.1 = f64::from_bits(e.1).max(p.y).to_bits();
        }
        assert_eq!(full, seen, "a column lost its lowest or highest point");
    }

    #[test]
    fn the_decimated_line_is_in_drawing_order() {
        let points = wave(50_000);
        let kept = decimate_line(&points, &wave_frame(points.len(), 200.0));
        assert!(kept.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_short_line_is_kept_whole() {
        let points = wave(300);
        let kept = decimate_line(&points, &wave_frame(points.len(), 400.0));
        assert_eq!(kept, (0..300).collect::<Vec<_>>());
    }

    #[test]
    fn a_point_with_a_non_finite_coordinate_is_left_out() {
        let mut points = wave(10);
        points[4].y = f64::NAN;
        points[6].x = f64::INFINITY;
        let kept = decimate_line(&points, &wave_frame(10, 400.0));
        assert!(!kept.contains(&4) && !kept.contains(&6));
        assert_eq!(kept.len(), 8);
    }

    #[test]
    fn a_dense_scatter_keeps_one_dot_per_occupied_cell() {
        // 100k dots inside a 10 x 10 px patch of a 200 x 100 plot.
        let points: Vec<ChartPoint> = (0..100_000)
            .map(|i| {
                ChartPoint::create(
                    (i % 317) as f64 / 317.0 * 5.0,
                    (i % 211) as f64 / 211.0 * 5.0,
                )
            })
            .collect();
        let kept = thin_scatter(&points, &frame(0), 2.0);
        assert!(kept.len() <= 6 * 6 * 4, "{} dots kept", kept.len());
        assert!(!kept.is_empty());
    }

    #[test]
    fn a_sparse_scatter_keeps_every_dot() {
        let points: Vec<ChartPoint> = (0..20)
            .map(|i| ChartPoint::create(i as f64 * 5.0, i as f64 * 2.0))
            .collect();
        assert_eq!(
            thin_scatter(&points, &frame(0), 2.0),
            (0..20).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_dot_outside_the_plot_is_left_out() {
        let points = vec![
            ChartPoint::create(10.0, 10.0),
            ChartPoint::create(10.0, 500.0),
        ];
        assert_eq!(thin_scatter(&points, &frame(0), 2.0), vec![0]);
    }

    #[test]
    fn the_nearest_point_by_x_is_found_by_binary_search() {
        let points: Vec<ChartPoint> = (0..1000)
            .map(|i| ChartPoint::create(i as f64 * 2.0, 0.0))
            .collect();
        assert!(is_sorted_by_x(&points));
        assert_eq!(nearest_by_x(&points, 0.0), Some(0));
        assert_eq!(
            nearest_by_x(&points, 7.1),
            Some(4),
            "7.1 is nearer 8 than 6"
        );
        assert_eq!(nearest_by_x(&points, 6.9), Some(3));
        assert_eq!(nearest_by_x(&points, -50.0), Some(0));
        assert_eq!(nearest_by_x(&points, 1e9), Some(999));
        assert_eq!(nearest_by_x(&[], 1.0), None);
        assert!(!is_sorted_by_x(&[
            ChartPoint::create(2.0, 0.0),
            ChartPoint::create(1.0, 0.0)
        ]));
    }

    fn labels(t: &NiceTicks) -> Vec<String> {
        let f = TickFormat::of(t);
        t.values().iter().map(|v| f.format(*v)).collect()
    }

    #[test]
    fn an_axis_writes_every_tick_in_one_unit() {
        assert_eq!(
            labels(&nice_ticks(0.0, 19_000.0, 4)),
            vec!["0", "5K", "10K", "15K", "20K"]
        );
        assert_eq!(
            labels(&nice_ticks(0.0, 1400.0, 3)),
            vec!["0", "500", "1,000", "1,500"]
        );
        assert_eq!(
            labels(&nice_ticks(0.0, 1.0, 5)),
            vec!["0.0", "0.2", "0.4", "0.6", "0.8", "1.0"]
        );
        let t = nice_ticks(-2.5e6, 2.5e6, 4);
        let f = TickFormat::of(&t);
        assert_eq!(f.suffix, "M");
        assert_eq!(f.format(-2e6), "-2M");
    }

    #[test]
    fn a_tick_that_rounds_to_zero_is_written_without_a_sign() {
        let f = TickFormat {
            unit: 1.0,
            suffix: "",
            decimals: 1,
        };
        assert_eq!(f.format(-0.01), "0.0");
    }

    #[test]
    fn a_value_is_written_for_reading() {
        assert_eq!(format_value(1234.0), "1,234");
        assert_eq!(format_value(-1_234_567.0), "-1,234,567");
        assert_eq!(format_value(1234.5), "1,234.5");
        assert_eq!(format_value(3.14159), "3.14");
        assert_eq!(format_value(0.000_123_46), "0.0001235");
        assert_eq!(format_value(0.0), "0");
        assert_eq!(format_value(f64::NAN), "-");
    }
}

// CHART7-NEXT: the geometry, the build, the pointer and the keys.
