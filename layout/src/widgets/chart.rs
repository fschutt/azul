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
    dom::{Dom, DomNodeId, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, SvgNodeData, TabIndex},
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

impl_option!(ChartPoint, OptionChartPoint, [Debug, Clone, Copy, PartialEq, PartialOrd]);
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

impl_option!(ChartSeries, OptionChartSeries, copy = false, [Debug, Clone, PartialEq]);
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
        Self { x, y, series, index }
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
    pub fn with_on_select<C: Into<ChartOnSelectCallback>>(mut self, data: RefAny, callback: C) -> Self {
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

// CHART7-NEXT: the math (ticks, scales, decimation, formatting), the
// geometry, the build, the pointer and the keys, the tests.
