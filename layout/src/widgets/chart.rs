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
        Dom, DomNodeId, EventFilter, HoverEventFilter, SvgNodeData,
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
/// A legend entry's name.
pub const LEGEND_LABEL_CLASS: &str = "__azul-native-chart-legend-label";
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
    pub const fn has_bars(self) -> bool {
        matches!(self, Self::Bar | Self::StackedBar)
    }

    /// The chart's name, for the text summary ("line chart").
    #[must_use]
    pub(crate) const fn noun(self) -> &'static str {
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
    /// The accent the selection ring and the focus ring wear, or `None` for
    /// the theme's (`with_shell_accent`: the app's accent family).
    pub accent: OptionChartColor,
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
            accent: OptionChartColor::None,
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

    /// The accent of the selection ring and the focus ring (never of a
    /// series: an accent is one colour, a palette many).
    pub const fn set_accent(&mut self, accent: ChartColor) {
        self.accent = OptionChartColor::Some(accent);
    }

    /// [`Self::set_accent`] for the builder chain.
    #[must_use]
    pub const fn with_accent(mut self, accent: ChartColor) -> Self {
        self.set_accent(accent);
        self
    }

    /// The app's accent family as the chart's accent: its stone by day,
    /// its glow at night (`ShellThemeAccent::colors`).
    pub const fn set_shell_accent(&mut self, accent: crate::widgets::shells::ShellThemeAccent) {
        self.set_accent(ChartColor::create(
            accent.colors(false).accent,
            accent.colors(true).glow,
        ));
    }

    /// [`Self::set_shell_accent`] for the builder chain.
    #[must_use]
    pub const fn with_shell_accent(
        mut self,
        accent: crate::widgets::shells::ShellThemeAccent,
    ) -> Self {
        self.set_shell_accent(accent);
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
pub(crate) struct NiceTicks {
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
pub(crate) fn nice_step(raw: f64) -> f64 {
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
pub(crate) fn nice_ticks(lo: f64, hi: f64, target: usize) -> NiceTicks {
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
pub(crate) struct PlotFrame {
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
pub(crate) fn decimate_line(points: &[ChartPoint], frame: &PlotFrame) -> Vec<usize> {
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
pub(crate) fn thin_scatter(points: &[ChartPoint], frame: &PlotFrame, cell: f32) -> Vec<usize> {
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
pub(crate) fn is_sorted_by_x(points: &[ChartPoint]) -> bool {
    points.windows(2).all(|w| w[0].x <= w[1].x)
}

/// The point whose x is nearest to `x` in points sorted by x (binary
/// search); ties go to the earlier point.
#[must_use]
pub(crate) fn nearest_by_x(points: &[ChartPoint], x: f64) -> Option<usize> {
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
pub(crate) struct TickFormat {
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
    out.push_str(&crate::widgets::money_input::group_digits(int, Some(',')));
    out.push_str(frac);
    out
}

/// A value as the tooltip and the table write it: whole numbers with
/// thousands separators, others with two places (four significant digits
/// under 1), trailing zeros dropped.
#[must_use]
pub(crate) fn format_value(v: f64) -> String {
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
        let mut full = alloc::collections::BTreeMap::<i64, (u64, u64)>::new();
        for p in &points {
            let e = full
                .entry(col(p))
                .or_insert((f64::MAX.to_bits(), f64::MIN.to_bits()));
            e.0 = f64::from_bits(e.0).min(p.y).to_bits();
            e.1 = f64::from_bits(e.1).max(p.y).to_bits();
        }
        let mut seen = alloc::collections::BTreeMap::<i64, (u64, u64)>::new();
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

// ==== the geometry (the plot's user space: one unit per px) ====

/// The least a plot is, in px, however small the chart.
const MIN_PLOT_PX: f32 = 20.0;
/// Air around a pie inside its frame.
const PIE_PAD: f32 = 8.0;
/// An arc is drawn in steps of at most 3 degrees.
const ARC_STEP: f32 = core::f32::consts::PI / 60.0;
/// The corners of a scatter dot.
const DOT_CORNERS: usize = 12;

/// A size the app gave, or `default` for one that is not a positive number.
fn finite_size(v: f32, default: f32) -> f32 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        default
    }
}

/// The category a point's `x` names on an axis of `bands` categories.
#[must_use]
pub(crate) fn category_of(x: f64, bands: usize) -> Option<usize> {
    let c = x.round();
    if c >= 0.0 && c < bands as f64 {
        Some(c as usize)
    } else {
        None
    }
}

/// How many categories the chart's x axis has: the names, or for a bar
/// chart or a pie without names the longest series; 0 for a number axis.
fn band_count(chart: &Chart) -> usize {
    let named = chart.categories.len();
    if named > 0 {
        return named;
    }
    if chart.kind.has_bars() || chart.kind.is_round() {
        return chart
            .series
            .as_slice()
            .iter()
            .map(|s| s.points.len())
            .max()
            .unwrap_or(0);
    }
    0
}

/// The finite y extent of the data as `chart.kind` draws it: a stacked
/// chart's stacks, every other chart's values.
fn y_extent(chart: &Chart, bands: usize) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    if chart.kind == ChartKind::StackedBar {
        let mut pos = alloc::vec![0.0f64; bands];
        let mut neg = alloc::vec![0.0f64; bands];
        for s in chart.series.as_slice() {
            for p in s.points.as_slice() {
                let Some(cat) = category_of(p.x, bands) else {
                    continue;
                };
                if !p.y.is_finite() {
                    continue;
                }
                if p.y >= 0.0 {
                    pos[cat] += p.y;
                } else {
                    neg[cat] += p.y;
                }
            }
        }
        for c in 0..bands {
            lo = lo.min(neg[c]);
            hi = hi.max(pos[c]);
        }
    } else {
        for s in chart.series.as_slice() {
            for p in s.points.as_slice() {
                if p.y.is_finite() {
                    lo = lo.min(p.y);
                    hi = hi.max(p.y);
                }
            }
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// The finite x extent of the data.
fn x_extent(chart: &Chart) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for s in chart.series.as_slice() {
        for p in s.points.as_slice() {
            if p.x.is_finite() {
                lo = lo.min(p.x);
                hi = hi.max(p.x);
            }
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// The chart laid out: where the plot sits in the frame (the box of the
/// axes and the plot, under the title), its domains and its ticks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChartGeometry {
    /// The plot's domains and size.
    pub(crate) frame: PlotFrame,
    /// The plot's left edge in the frame.
    pub(crate) plot_left: f32,
    /// The plot's top edge in the frame.
    pub(crate) plot_top: f32,
    /// The frame's width.
    pub(crate) frame_width: f32,
    /// The frame's height.
    pub(crate) frame_height: f32,
    /// The y axis' ticks (none for a pie).
    pub(crate) y_ticks: Option<NiceTicks>,
    /// The x axis' ticks on a number axis (none on a category axis).
    pub(crate) x_ticks: Option<NiceTicks>,
    /// The title row is shown.
    pub(crate) title: bool,
    /// The legend row is shown.
    pub(crate) legend: bool,
}

/// Lays `chart` out (module docs: the title row, the frame, the legend row).
#[must_use]
pub(crate) fn chart_geometry(chart: &Chart) -> ChartGeometry {
    let width = finite_size(chart.width, DEFAULT_WIDTH);
    let height = finite_size(chart.height, DEFAULT_HEIGHT);
    let title = !chart.title.as_str().is_empty();
    let count = chart.series.len();
    let legend = chart.show_legend && (count >= 2 || (chart.kind.is_round() && count >= 1));
    let title_h = if title { TITLE_HEIGHT } else { 0.0 };
    let legend_h = if legend { LEGEND_HEIGHT } else { 0.0 };
    let frame_height = (height - title_h - legend_h).max(MIN_PLOT_PX * 2.0);
    let bands = band_count(chart);

    if chart.kind.is_round() {
        let d = (width.min(frame_height) - 2.0 * PIE_PAD).max(MIN_PLOT_PX);
        return ChartGeometry {
            frame: PlotFrame {
                x_min: 0.0,
                x_max: 1.0,
                y_min: 0.0,
                y_max: 1.0,
                width: d,
                height: d,
                bands,
            },
            plot_left: (width - d) / 2.0,
            plot_top: (frame_height - d) / 2.0,
            frame_width: width,
            frame_height,
            y_ticks: None,
            x_ticks: None,
            title,
            legend,
        };
    }

    let y_title_h = if chart.y_title.as_str().is_empty() {
        0.0
    } else {
        AXIS_TITLE_HEIGHT
    };
    let x_title_h = if chart.x_title.as_str().is_empty() {
        0.0
    } else {
        AXIS_TITLE_HEIGHT
    };
    let plot_left = Y_GUTTER;
    let plot_top = PLOT_PAD_TOP + y_title_h;
    let plot_w = (width - plot_left - PLOT_PAD_RIGHT).max(MIN_PLOT_PX);
    let plot_h = (frame_height - plot_top - X_GUTTER - x_title_h).max(MIN_PLOT_PX);

    let (mut lo, mut hi) = y_extent(chart, bands).unwrap_or((0.0, 1.0));
    if chart.kind.has_bars() {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    if let Some(v) = chart.y_min.into_option().filter(|v| v.is_finite()) {
        lo = v;
    }
    if let Some(v) = chart.y_max.into_option().filter(|v| v.is_finite()) {
        hi = v;
    }
    let y_ticks = nice_ticks(lo, hi, ((plot_h / MIN_Y_TICK_PX).floor() as usize).max(2));

    let (x_min, x_max, x_ticks) = if bands > 0 {
        (-0.5, bands as f64 - 0.5, None)
    } else {
        let (lo, hi) = x_extent(chart).unwrap_or((0.0, 1.0));
        let (lo, hi) = if hi > lo {
            (lo, hi)
        } else {
            (lo - 0.5, hi + 0.5)
        };
        let target = ((plot_w / MIN_X_TICK_PX).floor() as usize).max(2);
        (lo, hi, Some(nice_ticks(lo, hi, target)))
    };

    ChartGeometry {
        frame: PlotFrame {
            x_min,
            x_max,
            y_min: y_ticks.min,
            y_max: y_ticks.max,
            width: plot_w,
            height: plot_h,
            bands,
        },
        plot_left,
        plot_top,
        frame_width: width,
        frame_height,
        y_ticks: Some(y_ticks),
        x_ticks,
        title,
        legend,
    }
}

/// A point of the plot's user space.
const fn pt(x: f32, y: f32) -> SvgPoint {
    SvgPoint { x, y }
}

/// A ring through `points`: open (a line) or closed back to its start (a
/// shape). Fewer than two points draw nothing.
fn ring(points: &[SvgPoint], closed: bool) -> SvgPath {
    let n = points.len();
    let mut items = Vec::with_capacity(n + 1);
    for w in points.windows(2) {
        items.push(SvgPathElement::Line(SvgLine::new(w[0], w[1])));
    }
    if closed && n > 2 {
        items.push(SvgPathElement::Line(SvgLine::new(points[n - 1], points[0])));
    }
    SvgPath::create(SvgPathElementVec::from_vec(items))
}

/// The shape of `rings`, leaving out the empty ones.
fn shape_of(rings: Vec<SvgPath>) -> SvgMultiPolygon {
    let rings: Vec<SvgPath> = rings
        .into_iter()
        .filter(|r| !r.items.as_slice().is_empty())
        .collect();
    SvgMultiPolygon::create(SvgPathVec::from_vec(rings))
}

/// The kept points of a series in px.
fn kept_px(points: &[ChartPoint], kept: &[usize], frame: &PlotFrame) -> Vec<SvgPoint> {
    kept.iter()
        .filter_map(|&i| points.get(i))
        .map(|p| pt(frame.px_x(p.x), frame.px_y(p.y)))
        .collect()
}

/// A series' line through its kept points (indices into `points`): one
/// open ring, stroked.
#[must_use]
pub(crate) fn line_shape(
    points: &[ChartPoint],
    kept: &[usize],
    frame: &PlotFrame,
) -> SvgMultiPolygon {
    shape_of(alloc::vec![ring(&kept_px(points, kept, frame), false)])
}

/// The area under a series' line, closed along the baseline at `base` px:
/// one closed ring, filled with a wash of the series colour.
#[must_use]
pub(crate) fn area_shape(
    points: &[ChartPoint],
    kept: &[usize],
    frame: &PlotFrame,
    base: f32,
) -> SvgMultiPolygon {
    let mut pts = kept_px(points, kept, frame);
    if pts.len() < 2 {
        return shape_of(Vec::new());
    }
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    pts.push(pt(last.x, base));
    pts.push(pt(first.x, base));
    shape_of(alloc::vec![ring(&pts, true)])
}

/// A scatter's dots at its kept points, `radius` px each: closed rings,
/// filled.
#[must_use]
pub(crate) fn dots_shape(
    points: &[ChartPoint],
    kept: &[usize],
    frame: &PlotFrame,
    radius: f32,
) -> SvgMultiPolygon {
    let rings = kept_px(points, kept, frame)
        .into_iter()
        .map(|c| {
            let corners: Vec<SvgPoint> = (0..DOT_CORNERS)
                .map(|k| {
                    let a = core::f32::consts::TAU * k as f32 / DOT_CORNERS as f32;
                    pt(c.x + radius * a.cos(), c.y + radius * a.sin())
                })
                .collect();
            ring(&corners, true)
        })
        .collect();
    shape_of(rings)
}

/// Which end of a bar is its data end, drawn rounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BarEnd {
    /// Neither (a segment inside a stack).
    None,
    /// The top: a positive bar.
    Top,
    /// The bottom: a negative bar.
    Bottom,
}

/// One bar (or stacked segment) in px, and the point it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BarRect {
    /// The left edge.
    pub(crate) x0: f32,
    /// The right edge.
    pub(crate) x1: f32,
    /// The top edge (the smaller y).
    pub(crate) top: f32,
    /// The bottom edge.
    pub(crate) bottom: f32,
    /// The rounded end.
    pub(crate) rounded: BarEnd,
    /// The series it belongs to.
    pub(crate) series: usize,
    /// The point's index in its series.
    pub(crate) index: usize,
}

impl BarRect {
    /// Whether `(x, y)` px is on the bar.
    #[must_use]
    pub(crate) fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.top && y <= self.bottom
    }
}

/// The bars of a bar chart, per series: grouped side by side in each
/// category (at most [`MAX_BAR_PX`] thick, [`SURFACE_GAP_PX`] apart), or
/// stacked (positive values up, negative down, a gap between segments).
/// The data end is rounded; a bar always grows from the zero line.
#[must_use]
pub(crate) fn bar_rects(
    kind: ChartKind,
    series: &[ChartSeries],
    frame: &PlotFrame,
) -> Vec<Vec<BarRect>> {
    let bands = frame.bands.max(1);
    let band = frame.band();
    let (y_lo, y_hi) = (frame.y_min.min(frame.y_max), frame.y_min.max(frame.y_max));
    let base = frame.px_y(0.0f64.clamp(y_lo, y_hi));
    let mut out: Vec<Vec<BarRect>> = series.iter().map(|_| Vec::new()).collect();

    if kind == ChartKind::StackedBar {
        let bar_w = (band * 0.8).min(MAX_BAR_PX).max(1.0);
        let mut pos = alloc::vec![0.0f64; bands];
        let mut neg = alloc::vec![0.0f64; bands];
        let mut last_pos: Vec<Option<(usize, usize)>> = alloc::vec![None; bands];
        let mut last_neg: Vec<Option<(usize, usize)>> = alloc::vec![None; bands];
        let mut segments: Vec<(usize, usize, usize, bool)> = Vec::new();
        for (s, ser) in series.iter().enumerate() {
            for (i, p) in ser.points.as_slice().iter().enumerate() {
                let Some(cat) = category_of(p.x, bands) else {
                    continue;
                };
                if !p.y.is_finite() || p.y == 0.0 {
                    continue;
                }
                let positive = p.y > 0.0;
                let (from, to) = if positive {
                    let f = pos[cat];
                    pos[cat] += p.y;
                    (f, pos[cat])
                } else {
                    let f = neg[cat];
                    neg[cat] += p.y;
                    (f, neg[cat])
                };
                let x0 = cat as f32 * band + (band - bar_w) / 2.0;
                let (ya, yb) = (frame.px_y(from), frame.px_y(to));
                out[s].push(BarRect {
                    x0,
                    x1: x0 + bar_w,
                    top: ya.min(yb),
                    bottom: ya.max(yb),
                    rounded: BarEnd::None,
                    series: s,
                    index: i,
                });
                let slot = (s, out[s].len() - 1);
                if positive {
                    last_pos[cat] = Some(slot);
                } else {
                    last_neg[cat] = Some(slot);
                }
                segments.push((s, slot.1, cat, positive));
            }
        }
        for (s, k, cat, positive) in segments {
            let last = if positive {
                last_pos[cat]
            } else {
                last_neg[cat]
            };
            let r = &mut out[s][k];
            if last == Some((s, k)) {
                r.rounded = if positive {
                    BarEnd::Top
                } else {
                    BarEnd::Bottom
                };
            } else if positive {
                r.top = (r.top + SURFACE_GAP_PX).min(r.bottom);
            } else {
                r.bottom = (r.bottom - SURFACE_GAP_PX).max(r.top);
            }
        }
        return out;
    }

    let n = series.len().max(1) as f32;
    let bar_w = ((band * 0.8 - (n - 1.0) * SURFACE_GAP_PX) / n)
        .min(MAX_BAR_PX)
        .max(1.0);
    let group_w = n * bar_w + (n - 1.0) * SURFACE_GAP_PX;
    for (s, ser) in series.iter().enumerate() {
        for (i, p) in ser.points.as_slice().iter().enumerate() {
            let Some(cat) = category_of(p.x, bands) else {
                continue;
            };
            if !p.y.is_finite() {
                continue;
            }
            let x0 =
                cat as f32 * band + (band - group_w) / 2.0 + s as f32 * (bar_w + SURFACE_GAP_PX);
            let end = frame.px_y(p.y);
            let (top, bottom, rounded) = if p.y >= 0.0 {
                (end.min(base), base, BarEnd::Top)
            } else {
                (base, end.max(base), BarEnd::Bottom)
            };
            out[s].push(BarRect {
                x0,
                x1: x0 + bar_w,
                top,
                bottom,
                rounded,
                series: s,
                index: i,
            });
        }
    }
    out
}

/// One bar's outline: a rectangle, its data end rounded with a radius of
/// [`BAR_RADIUS_PX`] (less on a thin or short bar).
#[must_use]
pub(crate) fn bar_ring(r: &BarRect) -> SvgPath {
    let (x0, x1, t, b) = (r.x0, r.x1, r.top, r.bottom);
    let rad = BAR_RADIUS_PX.min((x1 - x0) / 2.0).min(b - t).max(0.0);
    let line = |a: SvgPoint, z: SvgPoint| SvgPathElement::Line(SvgLine::new(a, z));
    let quad = |a: SvgPoint, c: SvgPoint, z: SvgPoint| {
        SvgPathElement::QuadraticCurve(SvgQuadraticCurve {
            start: a,
            ctrl: c,
            end: z,
        })
    };
    let items = match r.rounded {
        BarEnd::Top if rad > 0.0 => alloc::vec![
            line(pt(x0, b), pt(x0, t + rad)),
            quad(pt(x0, t + rad), pt(x0, t), pt(x0 + rad, t)),
            line(pt(x0 + rad, t), pt(x1 - rad, t)),
            quad(pt(x1 - rad, t), pt(x1, t), pt(x1, t + rad)),
            line(pt(x1, t + rad), pt(x1, b)),
            line(pt(x1, b), pt(x0, b)),
        ],
        BarEnd::Bottom if rad > 0.0 => alloc::vec![
            line(pt(x0, t), pt(x1, t)),
            line(pt(x1, t), pt(x1, b - rad)),
            quad(pt(x1, b - rad), pt(x1, b), pt(x1 - rad, b)),
            line(pt(x1 - rad, b), pt(x0 + rad, b)),
            quad(pt(x0 + rad, b), pt(x0, b), pt(x0, b - rad)),
            line(pt(x0, b - rad), pt(x0, t)),
        ],
        _ => alloc::vec![
            line(pt(x0, t), pt(x1, t)),
            line(pt(x1, t), pt(x1, b)),
            line(pt(x1, b), pt(x0, b)),
            line(pt(x0, b), pt(x0, t)),
        ],
    };
    SvgPath::create(SvgPathElementVec::from_vec(items))
}

/// One series' bars as one shape.
#[must_use]
pub(crate) fn bars_shape(rects: &[BarRect]) -> SvgMultiPolygon {
    shape_of(
        rects
            .iter()
            .filter(|r| r.bottom > r.top && r.x1 > r.x0)
            .map(bar_ring)
            .collect(),
    )
}

/// One slice of a pie: its angles (radians, clockwise from twelve
/// o'clock), its value and the category it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PieSlice {
    /// Where it starts.
    pub(crate) start: f32,
    /// Where it ends.
    pub(crate) end: f32,
    /// Its value (the sum of the folded categories for "Other").
    pub(crate) value: f64,
    /// The category it shows (the first folded one for "Other").
    pub(crate) index: usize,
    /// The slice of every category past the palette ("Other").
    pub(crate) other: bool,
}

/// A pie's slices over `values` (one per category), in proportion, from
/// twelve o'clock clockwise. A value that is not a positive number takes
/// no room; past [`PALETTE_LEN`] categories the rest fold into one "Other"
/// slice, so no two slices share a colour.
#[must_use]
pub(crate) fn pie_slices(values: &[f64]) -> Vec<PieSlice> {
    let usable = |v: f64| if v.is_finite() && v > 0.0 { v } else { 0.0 };
    let fold = values.len() > PALETTE_LEN;
    let shown = if fold { PALETTE_LEN - 1 } else { values.len() };
    let mut parts: Vec<(usize, f64, bool)> =
        (0..shown).map(|i| (i, usable(values[i]), false)).collect();
    if fold {
        let rest: f64 = values[shown..].iter().map(|v| usable(*v)).sum();
        parts.push((shown, rest, true));
    }
    let total: f64 = parts.iter().map(|p| p.1).sum();
    let mut angle = 0.0f64;
    parts
        .into_iter()
        .map(|(index, value, other)| {
            let start = angle;
            if total > 0.0 {
                angle += value / total * core::f64::consts::TAU;
            }
            PieSlice {
                start: start as f32,
                end: angle as f32,
                value,
                index,
                other,
            }
        })
        .collect()
}

/// A wedge of a disc around `(cx, cy)` from angle `a0` to `a1` (radians,
/// clockwise from twelve o'clock): to the centre for a pie, along an inner
/// arc of radius `r_in` for a donut.
#[must_use]
pub(crate) fn wedge_ring(cx: f32, cy: f32, r_out: f32, r_in: f32, a0: f32, a1: f32) -> SvgPath {
    let at = |r: f32, a: f32| pt(r.mul_add(a.sin(), cx), r.mul_add(-a.cos(), cy));
    let steps = (((a1 - a0).abs() / ARC_STEP).ceil() as usize).max(1);
    let angle = |k: usize| (a1 - a0).mul_add(k as f32 / steps as f32, a0);
    let mut pts = Vec::with_capacity(steps * 2 + 3);
    for k in 0..=steps {
        pts.push(at(r_out, angle(k)));
    }
    if r_in > 0.0 {
        for k in (0..=steps).rev() {
            pts.push(at(r_in, angle(k)));
        }
    } else {
        pts.push(pt(cx, cy));
    }
    ring(&pts, true)
}

// ==== the look ====

/// The series colours in their fixed order - blue, orange, aqua, yellow,
/// magenta, green, violet, red - each a light-mode step and its dark-mode
/// step. The ORDER is what keeps neighbours apart for colour-blind readers
/// (checked with the dataviz validator on the flat and flora surfaces in
/// both modes: every adjacent pair clears 8 dE under deutan / protan /
/// tritan simulation and 15 dE in full colour); three light steps sit under
/// 3:1 on a light surface, which is why the legend and the table view are
/// there.
pub const CHART_PALETTE: [ChartColor; PALETTE_LEN] = [
    ChartColor::create(ColorU::rgb(0x2A, 0x78, 0xD6), ColorU::rgb(0x39, 0x87, 0xE5)),
    ChartColor::create(ColorU::rgb(0xEB, 0x68, 0x34), ColorU::rgb(0xD9, 0x59, 0x26)),
    ChartColor::create(ColorU::rgb(0x1B, 0xAF, 0x7A), ColorU::rgb(0x19, 0x9E, 0x70)),
    ChartColor::create(ColorU::rgb(0xED, 0xA1, 0x00), ColorU::rgb(0xC9, 0x85, 0x00)),
    ChartColor::create(ColorU::rgb(0xE8, 0x7B, 0xA4), ColorU::rgb(0xD5, 0x51, 0x81)),
    ChartColor::create(ColorU::rgb(0x00, 0x83, 0x00), ColorU::rgb(0x00, 0x83, 0x00)),
    ChartColor::create(ColorU::rgb(0x4A, 0x3A, 0xA7), ColorU::rgb(0x90, 0x85, 0xE9)),
    ChartColor::create(ColorU::rgb(0xE3, 0x49, 0x48), ColorU::rgb(0xE6, 0x67, 0x67)),
];

/// What one widget theme decides about a chart: its surface, inks and
/// metrics. Built by `themes::flat::chart_skin` and
/// `themes::flora::chart_skin`; [`ChartLook`] builds every part from it.
#[derive(Debug, Clone)]
pub(crate) struct ChartSkin {
    /// The root: the chart's surface, its face (family and size), its ink
    /// and its corners.
    pub(crate) root: Vec<CssPropertyWithConditions>,
    /// The title: its size, weight and ink.
    pub(crate) title: Vec<CssPropertyWithConditions>,
    /// A tick label: the muted ink, a small size.
    pub(crate) tick: Vec<CssPropertyWithConditions>,
    /// An axis title and a legend name: the secondary ink.
    pub(crate) caption: Vec<CssPropertyWithConditions>,
    /// The tooltip's tip: the tooltip widget's own skin.
    pub(crate) tip: Vec<CssPropertyWithConditions>,
    /// The table view's header cells.
    pub(crate) table_head: Vec<CssPropertyWithConditions>,
    /// The table view's cells.
    pub(crate) table_cell: Vec<CssPropertyWithConditions>,
    /// The surface under the plot: the gap between touching marks and the
    /// ring around a dot or a marker are this colour.
    pub(crate) surface: ChartColor,
    /// A gridline.
    pub(crate) grid: ChartColor,
    /// The baseline.
    pub(crate) axis: ChartColor,
    /// The crosshair at the hovered x.
    pub(crate) crosshair: ChartColor,
    /// The selection ring and the plot's focus ring.
    pub(crate) accent: ChartColor,
    /// The series colours, in order.
    pub(crate) palette: [ChartColor; PALETTE_LEN],
    /// The theme's marker class on the root, if it has one.
    pub(crate) marker: Option<&'static str>,
}

/// The skins a chart is built with: the pinned theme's, or - unpinned -
/// flat's and flora's, every part carrying both, each theme's declarations
/// in its `@theme(<name>)` block (`theme_blocks::follow_props`). The DOM is
/// built ONCE either way: a chart of 500k points is not decimated twice.
#[derive(Debug, Clone)]
pub(crate) struct ChartLook {
    skins: Vec<ChartSkin>,
    /// The theme marker on the root: the pinned theme's, or the one of the
    /// theme the DOM is built for.
    pub(crate) marker: Option<&'static str>,
    /// The accent the app chose (`Chart::with_accent`), over the skins'.
    accent: Option<ChartColor>,
}

impl ChartLook {
    /// The look `theme` pins, or the look that follows the app theme.
    pub(crate) fn of(theme: OptionUiTheme, accent: Option<ChartColor>) -> Self {
        use crate::widgets::themes::{flat, flora};
        match theme.into_option() {
            Some(UiTheme::Flat) => {
                let s = flat::chart_skin();
                Self {
                    marker: s.marker,
                    skins: alloc::vec![s],
                    accent,
                }
            }
            Some(UiTheme::Flora) => {
                let s = flora::chart_skin();
                Self {
                    marker: s.marker,
                    skins: alloc::vec![s],
                    accent,
                }
            }
            None => {
                let (a, b) = (flat::chart_skin(), flora::chart_skin());
                let marker = match UiTheme::current() {
                    UiTheme::Flat => a.marker,
                    UiTheme::Flora => b.marker,
                };
                Self {
                    skins: alloc::vec![a, b],
                    marker,
                    accent,
                }
            }
        }
    }

    /// One part, built from every skin by `f` and merged.
    pub(crate) fn part(
        &self,
        f: impl Fn(&ChartSkin) -> Vec<CssPropertyWithConditions>,
    ) -> CssPropertyWithConditionsVec {
        crate::widgets::themes::theme_blocks::part_of(&self.skins, f)
    }

    /// `base` (the part's structure, the same in every theme), then the
    /// skin's paint `f` - the base declared once, outside every block.
    pub(crate) fn on_base(
        &self,
        base: &[CssPropertyWithConditions],
        f: impl Fn(&ChartSkin) -> Vec<CssPropertyWithConditions>,
    ) -> CssPropertyWithConditionsVec {
        self.part(|s| {
            let mut v = base.to_vec();
            v.extend(f(s));
            v
        })
    }

    /// The accent in `skin`: the app's if it chose one.
    fn accent_of(&self, skin: &ChartSkin) -> ChartColor {
        self.accent.unwrap_or(skin.accent)
    }
}

/// Series `index`'s colour in `skin`: its own, or its slot in the palette
/// (a ninth series starts the order again).
#[must_use]
pub(crate) fn series_color(series: &ChartSeries, index: usize, skin: &ChartSkin) -> ChartColor {
    series
        .color
        .into_option()
        .unwrap_or(skin.palette[index % PALETTE_LEN])
}

/// A fill in `color`, with its dark step.
fn fill_of(color: ChartColor) -> Vec<CssPropertyWithConditions> {
    crate::widgets::themes::decl::themed_fill(color.light, color.dark).to_vec()
}

/// A wash of `color`: the fill at about a tenth of its strength (a sixth
/// at night, where a tenth vanishes).
fn wash_of(color: ChartColor) -> Vec<CssPropertyWithConditions> {
    crate::widgets::themes::decl::themed_fill(
        ColorU {
            a: 26,
            ..color.light
        },
        ColorU {
            a: 42,
            ..color.dark
        },
    )
    .to_vec()
}

/// A stroke of `width` px in `color`: `stroke` / `stroke-width` are the
/// border's spellings, and on a node with an SVG path the display list
/// strokes the path with them instead of drawing a box border.
fn stroke_of(color: ChartColor, width: isize) -> Vec<CssPropertyWithConditions> {
    use crate::widgets::themes::decl;
    let mut v = decl::border(width).to_vec();
    v.extend(decl::themed_border_color(color.light, color.dark));
    v
}

// ==== the text summary and the table view ====

impl Chart {
    /// The chart in words, for a screen reader (the plot's description):
    /// what it is, its series and their ranges ("Revenue: line chart of 2
    /// series over 12 categories, Jan to Dec. North: 12 points, lowest 10,
    /// highest 98, last 54. ...").
    #[must_use]
    pub fn summary(&self) -> AzString {
        AzString::from(summary_text(self))
    }
}

/// The name of category `c`: its name, or its number counted from 1.
fn category_name(categories: &[AzString], c: usize) -> String {
    categories
        .get(c)
        .map_or_else(|| format!("{}", c + 1), |s| String::from(s.as_str()))
}

/// The finite (lowest, highest, sum, count, last) of a series' y.
fn y_stats(points: &[ChartPoint]) -> Option<(f64, f64, f64, usize, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut sum = 0.0;
    let mut n = 0usize;
    let mut last = f64::NAN;
    for p in points {
        if p.y.is_finite() {
            lo = lo.min(p.y);
            hi = hi.max(p.y);
            sum += p.y;
            n += 1;
            last = p.y;
        }
    }
    (n > 0).then_some((lo, hi, sum, n, last))
}

/// The values a pie shows: series 0's y per category.
fn pie_values(chart: &Chart) -> Vec<f64> {
    let bands = band_count(chart);
    let mut values = alloc::vec![f64::NAN; bands];
    if let Some(s) = chart.series.as_slice().first() {
        for p in s.points.as_slice() {
            if let Some(c) = category_of(p.x, bands) {
                values[c] = p.y;
            }
        }
    }
    values
}

/// [`Chart::summary`].
fn summary_text(chart: &Chart) -> String {
    let categories = chart.categories.as_slice();
    let mut out = String::new();
    if !chart.title.as_str().is_empty() {
        out.push_str(chart.title.as_str());
        out.push_str(": ");
    }
    let series = chart.series.as_slice();
    if chart.kind.is_round() {
        let values = pie_values(chart);
        let slices = pie_slices(&values);
        let total: f64 = slices.iter().map(|s| s.value).sum();
        out.push_str(&format!(
            "{} of {} categories",
            chart.kind.noun(),
            values.len()
        ));
        for s in slices.iter().filter(|s| s.value > 0.0) {
            let name = if s.other {
                String::from("Other")
            } else {
                category_name(categories, s.index)
            };
            let share = if total > 0.0 {
                s.value / total * 100.0
            } else {
                0.0
            };
            out.push_str(&format!(
                ". {name}: {} ({share:.0}%)",
                format_value(s.value)
            ));
        }
        out.push('.');
        return out;
    }
    out.push_str(&format!("{} of {} series", chart.kind.noun(), series.len()));
    let bands = band_count(chart);
    if bands > 0 {
        out.push_str(&format!(
            " over {bands} categories, {} to {}",
            category_name(categories, 0),
            category_name(categories, bands - 1)
        ));
    } else if let Some((lo, hi)) = x_extent(chart) {
        out.push_str(&format!(
            ", x from {} to {}",
            format_value(lo),
            format_value(hi)
        ));
    }
    for s in series.iter().take(PALETTE_LEN) {
        let points = s.points.as_slice();
        out.push_str(&format!(". {}: ", s.name.as_str()));
        match y_stats(points) {
            Some((lo, hi, _, n, last)) => {
                out.push_str(&format!(
                    "{} points, lowest {}, highest {}",
                    format_value(n as f64),
                    format_value(lo),
                    format_value(hi)
                ));
                if matches!(chart.kind, ChartKind::Line | ChartKind::Area) {
                    out.push_str(&format!(", last {}", format_value(last)));
                }
            }
            None => out.push_str("no values"),
        }
    }
    if series.len() > PALETTE_LEN {
        out.push_str(&format!(". And {} more series", series.len() - PALETTE_LEN));
    }
    out.push('.');
    out
}

/// The table view's header and rows: a row per category (a pie: the
/// category, its value and its share); on a number axis a row per point -
/// or, past [`MAX_TABLE_ROWS`] points in a series, one summary row per
/// series (its points, lowest, highest, mean and last value).
#[must_use]
pub(crate) fn table_rows(chart: &Chart) -> (Vec<String>, Vec<Vec<String>>) {
    let categories = chart.categories.as_slice();
    let series = chart.series.as_slice();
    if chart.kind.is_round() {
        let values = pie_values(chart);
        let total: f64 = values.iter().filter(|v| v.is_finite() && **v > 0.0).sum();
        let rows = values
            .iter()
            .enumerate()
            .take(MAX_TABLE_ROWS)
            .map(|(c, v)| {
                let share = if total > 0.0 && v.is_finite() && *v > 0.0 {
                    format!("{:.0}%", v / total * 100.0)
                } else {
                    String::from("-")
                };
                alloc::vec![category_name(categories, c), format_value(*v), share]
            })
            .collect();
        let head = alloc::vec![
            String::from("Category"),
            series
                .first()
                .map_or_else(|| String::from("Value"), |s| String::from(s.name.as_str())),
            String::from("Share"),
        ];
        return (head, rows);
    }
    let bands = band_count(chart);
    if bands > 0 {
        let mut head = alloc::vec![String::from("Category")];
        head.extend(series.iter().map(|s| String::from(s.name.as_str())));
        let mut grid =
            alloc::vec![alloc::vec![String::from("-"); series.len()]; bands.min(MAX_TABLE_ROWS)];
        for (s, ser) in series.iter().enumerate() {
            for p in ser.points.as_slice() {
                if let Some(c) = category_of(p.x, bands) {
                    if let Some(row) = grid.get_mut(c) {
                        row[s] = format_value(p.y);
                    }
                }
            }
        }
        let rows = grid
            .into_iter()
            .enumerate()
            .map(|(c, values)| {
                let mut row = alloc::vec![category_name(categories, c)];
                row.extend(values);
                row
            })
            .collect();
        return (head, rows);
    }
    let big = series.iter().any(|s| s.points.len() > MAX_TABLE_ROWS);
    if big {
        let head = ["Series", "Points", "Lowest", "Highest", "Mean", "Last"]
            .iter()
            .map(|s| String::from(*s))
            .collect();
        let rows = series
            .iter()
            .map(|s| {
                let name = String::from(s.name.as_str());
                match y_stats(s.points.as_slice()) {
                    Some((lo, hi, sum, n, last)) => alloc::vec![
                        name,
                        format_value(n as f64),
                        format_value(lo),
                        format_value(hi),
                        format_value(sum / n as f64),
                        format_value(last),
                    ],
                    None => alloc::vec![
                        name,
                        String::from("0"),
                        String::from("-"),
                        String::from("-"),
                        String::from("-"),
                        String::from("-"),
                    ],
                }
            })
            .collect();
        return (head, rows);
    }
    let head = ["Series", "x", "y"]
        .iter()
        .map(|s| String::from(*s))
        .collect();
    let rows = series
        .iter()
        .flat_map(|s| {
            s.points.as_slice().iter().map(move |p| {
                alloc::vec![
                    String::from(s.name.as_str()),
                    format_value(p.x),
                    format_value(p.y)
                ]
            })
        })
        .collect();
    (head, rows)
}

// ==== the build ====

/// A tick label's line box.
const LABEL_HEIGHT: f32 = 16.0;
/// The least room a category label gets (a narrower band labels every
/// n-th category).
const CATEGORY_LABEL_PX: f32 = 56.0;
/// The ring of a dot, a marker and a pie wedge, in the surface colour.
const RING_WIDTH: isize = 2;
/// The selection ring's inner size.
const SELECTION_PX: f32 = 14.0;
/// A legend swatch for a line: a short stroke.
const LINE_SWATCH: (f32, f32) = (14.0, 3.0);
/// A legend swatch for an area, bars, dots or a slice: a small square.
const BOX_SWATCH: (f32, f32) = (10.0, 10.0);

type Decl = CssPropertyWithConditions;

use crate::widgets::themes::decl::classes;

/// Absolutely placed at `(left, top)`, `w` x `h` px.
fn placed(left: f32, top: f32, w: f32, h: f32) -> Vec<Decl> {
    use crate::widgets::themes::decl;
    alloc::vec![
        decl::position(azul_css::props::layout::LayoutPosition::Absolute),
        decl::px_left(left),
        decl::px_top(top),
        decl::px_width(w.max(0.0)),
        decl::px_height(h.max(0.0)),
    ]
}

/// Over the whole plot, as the XML parser places an SVG shape: the box IS
/// the plot's user space, whatever border (stroke) it carries. (The gauge's
/// arcs sit over its dial the same way.)
pub(crate) fn over_plot() -> Vec<CssPropertyWithConditions> {
    use azul_css::props::layout::{LayoutInsetBottom, LayoutPosition, LayoutRight};

    use crate::widgets::themes::decl;
    alloc::vec![
        decl::position(LayoutPosition::Absolute),
        decl::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        decl::simple(CssProperty::const_top(LayoutTop::const_px(0))),
        decl::simple(CssProperty::const_right(LayoutRight::const_px(0))),
        decl::simple(CssProperty::const_bottom(LayoutInsetBottom::const_px(0))),
    ]
}

/// `text-align: <align>`.
fn text_align(align: azul_css::props::style::StyleTextAlign) -> Decl {
    crate::widgets::themes::decl::simple(CssProperty::const_text_align(align))
}

/// Hidden until the pointer or the keys show it.
fn hidden() -> Decl {
    crate::widgets::themes::decl::simple(CssProperty::const_opacity(StyleOpacity::const_new(0)))
}

/// A widget-owned text line (a `<p>` without the UA margin, not
/// selectable) with `class` and `style`.
fn text_p(text: &str, class: &'static str, style: CssPropertyWithConditionsVec) -> Dom {
    crate::widgets::widget_p_with_text(AzString::from(String::from(text)))
        .with_ids_and_classes(classes(&[class]))
        .with_css_props(style)
}

/// A box with `class` and `style`.
fn part_div(class: &'static str, style: CssPropertyWithConditionsVec) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(classes(&[class]))
        .with_css_props(style)
}

/// One series' marks: a node over the plot with an SVG path.
fn shape_node(shape: SvgMultiPolygon, style: CssPropertyWithConditionsVec) -> Dom {
    part_div(SERIES_CLASS, style).with_svg_data(SvgNodeData::Path(shape))
}

/// The y px of the zero line, or of the plot's edge nearest to zero.
fn baseline_px(frame: &PlotFrame) -> f32 {
    let (lo, hi) = (frame.y_min.min(frame.y_max), frame.y_min.max(frame.y_max));
    frame.px_y(0.0f64.clamp(lo, hi))
}

/// A pie's place in its plot.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct PieGeometry {
    /// The centre.
    pub(crate) cx: f32,
    /// See `cx`.
    pub(crate) cy: f32,
    /// The outer radius.
    pub(crate) r_out: f32,
    /// The hole's radius (0 for a pie).
    pub(crate) r_in: f32,
}

impl Chart {
    /// The chart's DOM: the title, the axes and the plot, the legend and -
    /// with `with_show_table(true)` - the data table. The look comes from
    /// the theme module (`themes::flat::chart_skin` /
    /// `themes::flora::chart_skin`); unpinned, every part carries both
    /// looks, each in its `@theme(<name>)` block, and the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl From<Chart> for Dom {
    fn from(c: Chart) -> Self {
        c.dom()
    }
}

/// [`Chart::dom`].
fn build(mut chart: Chart) -> Dom {
    use azul_css::props::{
        layout::{
            LayoutAlignItems, LayoutAlignSelf, LayoutFlexDirection, LayoutFlexWrap, LayoutPosition,
        },
        style::StyleTextAlign,
    };

    use crate::widgets::themes::{decl, theme_blocks::stack_parts};

    let look = ChartLook::of(chart.theme, chart.accent.into_option());
    let g = chart_geometry(&chart);
    let frame = g.frame;
    let width = g.frame_width;
    let categories: Vec<AzString> = chart.categories.as_slice().to_vec();
    let round = chart.kind.is_round();

    // ---- the plot: gridlines, the series' marks, the baseline ----
    let mut plot: Vec<Dom> = Vec::new();
    let base_y = baseline_px(&frame);
    if chart.show_grid {
        if let Some(t) = g.y_ticks {
            for v in t.values() {
                let y = frame.px_y(v).round();
                if (y - base_y.round()).abs() < 0.5 || y < 0.0 || y > frame.height {
                    continue;
                }
                plot.push(part_div(
                    GRID_CLASS,
                    look.on_base(&placed(0.0, y, frame.width, 1.0), |s| fill_of(s.grid)),
                ));
            }
        }
    }

    let mut markers = 0;
    let mut bars: Vec<Vec<BarRect>> = Vec::new();
    let mut slices: Vec<PieSlice> = Vec::new();
    let mut pie = PieGeometry::default();
    {
        let series = chart.series.as_slice();
        match chart.kind {
            ChartKind::Line | ChartKind::Area => {
                let kept: Vec<Vec<usize>> = series
                    .iter()
                    .map(|s| decimate_line(s.points.as_slice(), &frame))
                    .collect();
                if chart.kind == ChartKind::Area {
                    for (k, s) in series.iter().enumerate() {
                        let shape = area_shape(s.points.as_slice(), &kept[k], &frame, base_y);
                        let style =
                            look.on_base(&over_plot(), |sk| wash_of(series_color(s, k, sk)));
                        plot.push(shape_node(shape, style));
                    }
                }
                for (k, s) in series.iter().enumerate() {
                    let shape = line_shape(s.points.as_slice(), &kept[k], &frame);
                    let style = look.on_base(&over_plot(), |sk| {
                        stroke_of(series_color(s, k, sk), LINE_WIDTH_PX as isize)
                    });
                    plot.push(shape_node(shape, style));
                }
                markers = series.len();
            }
            ChartKind::Scatter => {
                let total: usize = series.iter().map(|s| s.points.len()).sum();
                let dense = total > DENSE_DOTS;
                let (radius, cell) = if dense {
                    (DENSE_DOT_RADIUS_PX, DENSE_DOT_RADIUS_PX * 1.5)
                } else {
                    (DOT_RADIUS_PX + 1.0, 1.0)
                };
                for (k, s) in series.iter().enumerate() {
                    let kept = thin_scatter(s.points.as_slice(), &frame, cell);
                    let shape = dots_shape(s.points.as_slice(), &kept, &frame, radius);
                    let style = look.on_base(&over_plot(), |sk| {
                        let mut v = fill_of(series_color(s, k, sk));
                        if !dense {
                            v.extend(stroke_of(sk.surface, RING_WIDTH));
                        }
                        v
                    });
                    plot.push(shape_node(shape, style));
                }
                markers = series.len();
            }
            ChartKind::Bar | ChartKind::StackedBar => {
                bars = bar_rects(chart.kind, series, &frame);
                for (k, s) in series.iter().enumerate() {
                    let style = look.on_base(&over_plot(), |sk| fill_of(series_color(s, k, sk)));
                    plot.push(shape_node(bars_shape(&bars[k]), style));
                }
            }
            ChartKind::Pie | ChartKind::Donut => {
                slices = pie_slices(&pie_values(&chart));
                let r_out = (frame.width / 2.0 - 1.0).max(1.0);
                pie = PieGeometry {
                    cx: frame.width / 2.0,
                    cy: frame.height / 2.0,
                    r_out,
                    r_in: if chart.kind == ChartKind::Donut {
                        r_out * DONUT_HOLE
                    } else {
                        0.0
                    },
                };
                for (k, sl) in slices.iter().enumerate() {
                    if sl.end <= sl.start {
                        continue;
                    }
                    let ring = wedge_ring(pie.cx, pie.cy, pie.r_out, pie.r_in, sl.start, sl.end);
                    let shape = SvgMultiPolygon::create(SvgPathVec::from_vec(alloc::vec![ring]));
                    let style = look.on_base(&over_plot(), |sk| {
                        let mut v = fill_of(sk.palette[k % PALETTE_LEN]);
                        v.extend(stroke_of(sk.surface, RING_WIDTH));
                        v
                    });
                    plot.push(shape_node(shape, style));
                }
            }
        }
    }
    if !round {
        let y = base_y.round().min(frame.height - 1.0).max(0.0);
        plot.push(part_div(
            BASELINE_CLASS,
            look.on_base(&placed(0.0, y, frame.width, 1.0), |s| fill_of(s.axis)),
        ));
    }

    // ---- the frame: the axes' labels and titles around the plot ----
    let mut frame_kids: Vec<Dom> = Vec::new();
    let tick_skin = look.part(|s| s.tick.clone());
    let caption_skin = look.part(|s| s.caption.clone());
    let label =
        |text: &str, class: &'static str, base: Vec<Decl>, skin: &CssPropertyWithConditionsVec| {
            text_p(
                text,
                class,
                stack_parts(&CssPropertyWithConditionsVec::from_vec(base), skin),
            )
        };
    if !round {
        if !chart.y_title.as_str().is_empty() {
            let mut base = placed(8.0, 0.0, g.plot_left + frame.width - 8.0, AXIS_TITLE_HEIGHT);
            base.push(decl::nowrap());
            frame_kids.push(label(
                chart.y_title.as_str(),
                AXIS_TITLE_CLASS,
                base,
                &caption_skin,
            ));
        }
        if let Some(t) = g.y_ticks {
            let fmt = TickFormat::of(&t);
            for v in t.values() {
                let y = g.plot_top + frame.px_y(v);
                let mut base = placed(0.0, y - LABEL_HEIGHT / 2.0, Y_GUTTER - 8.0, LABEL_HEIGHT);
                base.push(text_align(StyleTextAlign::Right));
                base.push(decl::nowrap());
                frame_kids.push(label(&fmt.format(v), TICK_CLASS, base, &tick_skin));
            }
        }
    }

    let mut plot_kids_tail: Vec<Dom> = Vec::new();
    let x_top = g.plot_top + frame.height + 4.0;
    let mut x_labels: Vec<Dom> = Vec::new();
    if !round {
        if frame.bands > 0 {
            let band = frame.band().max(1.0);
            let every = ((CATEGORY_LABEL_PX / band).ceil() as usize).max(1);
            let w = (band * every as f32).max(CATEGORY_LABEL_PX);
            for c in (0..frame.bands).step_by(every) {
                let x = g.plot_left + frame.px_x(c as f64);
                let mut base = placed(x - w / 2.0, x_top, w, LABEL_HEIGHT);
                base.push(text_align(StyleTextAlign::Center));
                base.push(decl::nowrap());
                x_labels.push(label(
                    &category_name(&categories, c),
                    TICK_CLASS,
                    base,
                    &tick_skin,
                ));
            }
        } else if let Some(t) = g.x_ticks {
            let fmt = TickFormat::of(&t);
            let slack = (frame.x_max - frame.x_min).abs() * 1e-9;
            for v in t.values() {
                if v < frame.x_min - slack || v > frame.x_max + slack {
                    continue;
                }
                let x = g.plot_left + frame.px_x(v);
                let mut base = placed(x - MIN_X_TICK_PX / 2.0, x_top, MIN_X_TICK_PX, LABEL_HEIGHT);
                base.push(text_align(StyleTextAlign::Center));
                base.push(decl::nowrap());
                x_labels.push(label(&fmt.format(v), TICK_CLASS, base, &tick_skin));
            }
        }
        if !chart.x_title.as_str().is_empty() {
            let mut base = placed(
                g.plot_left,
                g.plot_top + frame.height + X_GUTTER,
                frame.width,
                AXIS_TITLE_HEIGHT,
            );
            base.push(text_align(StyleTextAlign::Center));
            base.push(decl::nowrap());
            x_labels.push(label(
                chart.x_title.as_str(),
                AXIS_TITLE_CLASS,
                base,
                &caption_skin,
            ));
        }
    }

    // ---- the legend and the table, while the series are still the chart's ----
    let legend = if g.legend {
        Some(legend_dom(
            &chart,
            &look,
            &slices,
            &categories,
            if round { 8.0 } else { g.plot_left },
        ))
    } else {
        None
    };
    let table = if chart.show_table {
        Some(table_dom(&chart, &look))
    } else {
        None
    };
    let summary = summary_text(&chart);
    let name = if chart.title.as_str().is_empty() {
        String::from("Chart")
    } else {
        String::from(chart.title.as_str())
    };

    // ---- the overlay: the pointer's and the keyboard's state ----
    let series: Vec<ChartSeries> =
        core::mem::replace(&mut chart.series, ChartSeriesVec::from_const_slice(&[]))
            .into_library_owned_vec();
    let sorted = series
        .iter()
        .map(|s| is_sorted_by_x(s.points.as_slice()))
        .collect();
    let state = ChartState {
        kind: chart.kind,
        frame,
        series,
        sorted,
        categories,
        bars,
        slices,
        pie,
        markers,
        on_select: chart.on_select.clone(),
        hovered: None,
    };

    if let Some(sel) = chart.selected.into_option() {
        if let Some((x, y)) = state.point_px(sel.series, sel.index) {
            let outer = SELECTION_PX + 2.0 * RING_WIDTH as f32;
            let mut base = placed(x - outer / 2.0, y - outer / 2.0, SELECTION_PX, SELECTION_PX);
            base.extend(decl::radius((outer / 2.0) as isize));
            let style = look.on_base(&base, |s| stroke_of(look.accent_of(s), RING_WIDTH));
            plot_kids_tail.push(part_div(SELECTION_CLASS, style));
        }
    }

    let mut overlay_kids: Vec<Dom> = Vec::new();
    let mut crosshair = placed(0.0, 0.0, 1.0, frame.height);
    crosshair.push(hidden());
    overlay_kids.push(part_div(
        CROSSHAIR_CLASS,
        look.on_base(&crosshair, |s| fill_of(s.crosshair)),
    ));
    for k in 0..state.markers {
        let mut base = placed(0.0, 0.0, MARKER_PX, MARKER_PX);
        base.extend(decl::radius((MARKER_PX / 2.0) as isize + RING_WIDTH));
        base.push(hidden());
        let s = &state.series[k];
        let style = look.on_base(&base, |sk| {
            let mut v = fill_of(series_color(s, k, sk));
            v.extend(stroke_of(sk.surface, RING_WIDTH));
            v
        });
        overlay_kids.push(part_div(MARKER_CLASS, style));
    }
    overlay_kids.push(
        text_p(" ", TOOLTIP_CLASS, look.part(|s| s.tip.clone())).with_accessibility_info(
            AccessibilityInfo {
                role: AccessibilityRole::Tooltip,
                is_live_region: true,
                ..AccessibilityInfo::default()
            },
        ),
    );

    let has_select = chart.on_select.is_some();
    let state = RefAny::new(state);
    let on =
        |event: EventFilter, cb: extern "C" fn(RefAny, CallbackInfo) -> Update| CoreCallbackData {
            event,
            callback: CoreCallback {
                cb: cb as usize,
                ctx: OptionRefAny::None,
            },
            refany: state.clone(),
        };
    let mut callbacks = alloc::vec![
        on(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            on_chart_pointer_move
        ),
        on(
            EventFilter::Hover(HoverEventFilter::MouseLeave),
            on_chart_pointer_leave
        ),
        on(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            on_chart_key
        ),
        on(
            EventFilter::Focus(FocusEventFilter::FocusLost),
            on_chart_blur
        ),
    ];
    if has_select {
        // `Click` is what a pointer click, Enter / Space on the focused plot
        // and an assistive technology's default action all dispatch.
        callbacks.push(on(
            EventFilter::Hover(HoverEventFilter::Click),
            on_chart_click,
        ));
    }
    let overlay_style = look.on_base(&over_plot(), |s| {
        let a = look.accent_of(s);
        decl::focus_halo(a.light, a.dark).to_vec()
    });
    let overlay = part_div(OVERLAY_CLASS, overlay_style)
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            description: azul_css::OptionString::Some(AzString::from(summary)),
            ..AccessibilityInfo::named(name, AccessibilityRole::Chart)
        })
        .with_callbacks(callbacks.into())
        .with_children(overlay_kids.into());
    plot.extend(plot_kids_tail);
    plot.push(overlay);

    let plot_dom = Dom::create_div()
        .with_ids_and_classes(classes(&[PLOT_CLASS]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(placed(
            g.plot_left,
            g.plot_top,
            frame.width,
            frame.height,
        )))
        .with_svg_data(SvgNodeData::ViewBox {
            min_x: 0.0,
            min_y: 0.0,
            width: frame.width,
            height: frame.height,
        })
        .with_children(plot.into());
    frame_kids.push(plot_dom);
    frame_kids.extend(x_labels);

    let frame_dom = Dom::create_div()
        .with_ids_and_classes(classes(&[FRAME_CLASS]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(alloc::vec![
            decl::position(LayoutPosition::Relative),
            decl::px_width(width),
            decl::px_height(g.frame_height),
            decl::no_shrink(),
        ]))
        .with_children(frame_kids.into());

    // ---- the root ----
    let mut kids: Vec<Dom> = Vec::new();
    if g.title {
        let mut base = alloc::vec![
            decl::px_height(TITLE_HEIGHT - 6.0),
            decl::nowrap(),
            decl::no_shrink()
        ];
        base.extend(decl::padding(6, 8, 0, 8));
        kids.push(text_p(
            chart.title.as_str(),
            TITLE_CLASS,
            look.on_base(&base, |s| s.title.clone()),
        ));
    }
    kids.push(frame_dom);
    kids.extend(legend);
    kids.extend(table);

    let mut root_classes = alloc::vec![CHART_CLASS];
    if let Some(m) = look.marker {
        root_classes.push(m);
    }
    let root_base = alloc::vec![
        decl::display_flex(),
        decl::flex_direction(LayoutFlexDirection::Column),
        decl::px_width(width),
        decl::no_shrink(),
        decl::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    ];
    Dom::create_div()
        .with_ids_and_classes(classes(&root_classes))
        .with_css_props(look.on_base(&root_base, |s| s.root.clone()))
        .with_children(kids.into())
}

/// The legend: a swatch and a name per series - or, for a pie, per slice -
/// in a wrapping row under the plot, its left edge on the plot's.
fn legend_dom(
    chart: &Chart,
    look: &ChartLook,
    slices: &[PieSlice],
    categories: &[AzString],
    left: f32,
) -> Dom {
    use azul_css::props::{
        layout::{LayoutAlignItems, LayoutColumnGap, LayoutFlexDirection, LayoutFlexWrap},
        property::LayoutColumnGapValue,
    };

    use crate::widgets::themes::decl;

    let gap = |px: isize| {
        decl::simple(CssProperty::ColumnGap(LayoutColumnGapValue::Exact(
            LayoutColumnGap {
                inner: azul_css::props::basic::PixelValue::const_px(px),
            },
        )))
    };
    let row = |px: isize| {
        alloc::vec![
            decl::display_flex(),
            decl::flex_direction(LayoutFlexDirection::Row),
            decl::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
            gap(px),
        ]
    };
    let line_like = matches!(chart.kind, ChartKind::Line);
    let (sw, sh) = if line_like { LINE_SWATCH } else { BOX_SWATCH };
    let mut swatch_base = alloc::vec![decl::px_width(sw), decl::px_height(sh), decl::no_shrink()];
    swatch_base.extend(decl::radius(if line_like { 1 } else { 2 }));
    let caption = look.part(|s| s.caption.clone());
    let item = |name: String, swatch: CssPropertyWithConditionsVec| {
        part_div(
            LEGEND_ITEM_CLASS,
            CssPropertyWithConditionsVec::from_vec(row(6)),
        )
        .with_child(part_div(SWATCH_CLASS, swatch))
        .with_child(text_p(&name, LEGEND_LABEL_CLASS, caption.clone()))
    };
    let mut items: Vec<Dom> = Vec::new();
    if chart.kind.is_round() {
        for (k, sl) in slices.iter().enumerate() {
            let name = if sl.other {
                String::from("Other")
            } else {
                category_name(categories, sl.index)
            };
            items.push(item(
                name,
                look.on_base(&swatch_base, |s| fill_of(s.palette[k % PALETTE_LEN])),
            ));
        }
    } else {
        for (k, s) in chart.series.as_slice().iter().enumerate() {
            items.push(item(
                String::from(s.name.as_str()),
                look.on_base(&swatch_base, |sk| fill_of(series_color(s, k, sk))),
            ));
        }
    }
    let mut base = row(16);
    base.push(decl::simple(CssProperty::const_flex_wrap(
        LayoutFlexWrap::Wrap,
    )));
    base.push(decl::px_height(LEGEND_HEIGHT));
    base.push(decl::no_shrink());
    base.push(decl::simple(CssProperty::const_padding_left(
        azul_css::props::layout::LayoutPaddingLeft::px(left),
    )));
    part_div(LEGEND_CLASS, CssPropertyWithConditionsVec::from_vec(base)).with_children(items.into())
}

/// The table view: the data as a table under the chart (module docs).
fn table_dom(chart: &Chart, look: &ChartLook) -> Dom {
    use crate::widgets::themes::decl;

    let (head, rows) = table_rows(chart);
    let head_style = look.part(|s| s.table_head.clone());
    let cell_style = look.part(|s| s.table_cell.clone());
    let cell = |node: Dom, text: String, style: &CssPropertyWithConditionsVec| {
        node.with_css_props(style.clone()).with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from(text)),
        )
    };
    let head_row = Dom::create_tr().with_children(
        head.into_iter()
            .map(|t| cell(Dom::create_th(), t, &head_style))
            .collect::<Vec<Dom>>()
            .into(),
    );
    let body_rows: Vec<Dom> = rows
        .into_iter()
        .map(|r| {
            Dom::create_tr().with_children(
                r.into_iter()
                    .map(|t| cell(Dom::create_td(), t, &cell_style))
                    .collect::<Vec<Dom>>()
                    .into(),
            )
        })
        .collect();
    let name = if chart.title.as_str().is_empty() {
        String::from("Chart data")
    } else {
        format!("{} data", chart.title.as_str())
    };
    let mut base = decl::margin(8, 8, 8, 8).to_vec();
    base.push(decl::no_shrink());
    Dom::create_table_no_a11y()
        .with_ids_and_classes(classes(&[TABLE_CLASS]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(base))
        .with_accessibility_info(AccessibilityInfo::named(name, AccessibilityRole::Table))
        .with_child(Dom::create_thead().with_child(head_row))
        .with_child(Dom::create_tbody().with_children(body_rows.into()))
}

// ==== the pointer and the keys ====

/// What the overlay's handlers know: the chart as it was drawn (its frame,
/// its data and the shapes the pointer can hit) and the point under the
/// pointer or the keyboard.
#[derive(Debug)]
pub(crate) struct ChartState {
    /// What is drawn.
    pub(crate) kind: ChartKind,
    /// The plot's domains and size.
    pub(crate) frame: PlotFrame,
    /// The FULL data (the drawing is decimated, the pointer is not).
    pub(crate) series: Vec<ChartSeries>,
    /// Per series: its x never decreases (binary search on hover).
    pub(crate) sorted: Vec<bool>,
    /// The category names.
    pub(crate) categories: Vec<AzString>,
    /// The bars, per series (a bar chart).
    pub(crate) bars: Vec<Vec<BarRect>>,
    /// The slices (a pie).
    pub(crate) slices: Vec<PieSlice>,
    /// The pie's place.
    pub(crate) pie: PieGeometry,
    /// How many markers the overlay holds (one per series of a line, an
    /// area or a scatter; none otherwise).
    pub(crate) markers: usize,
    /// The app's hook.
    pub(crate) on_select: OptionChartOnSelect,
    /// The point shown: `(series, index)` - for a pie `(0, category)`.
    pub(crate) hovered: Option<(usize, usize)>,
}

/// The point of `points` nearest to `(x, y)` px in `frame`, and how far.
fn nearest_in_px(points: &[ChartPoint], frame: &PlotFrame, x: f32, y: f32) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for (i, p) in points.iter().enumerate() {
        if !(p.x.is_finite() && p.y.is_finite()) {
            continue;
        }
        let d = (frame.px_x(p.x) - x).hypot(frame.px_y(p.y) - y);
        if best.map_or(true, |b| d < b.1) {
            best = Some((i, d));
        }
    }
    best
}

impl ChartState {
    /// The point under `(x, y)` px of the plot: on a line the nearest x of
    /// the nearest series (a crosshair), on a scatter the nearest dot within
    /// [`HOVER_REACH_PX`], on a bar chart the bar under the pointer or the
    /// nearest one in its column, on a pie the slice.
    #[must_use]
    pub(crate) fn hit(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        match self.kind {
            ChartKind::Line | ChartKind::Area => {
                let target = self.frame.x_at(x);
                let mut best: Option<(f32, usize, usize)> = None;
                for (s, ser) in self.series.iter().enumerate() {
                    let pts = ser.points.as_slice();
                    let found = if self.sorted.get(s).copied().unwrap_or(false) {
                        nearest_by_x(pts, target)
                    } else {
                        nearest_in_px(pts, &self.frame, x, y).map(|(i, _)| i)
                    };
                    let Some(i) = found else {
                        continue;
                    };
                    let p = pts[i];
                    if !(p.x.is_finite() && p.y.is_finite()) {
                        continue;
                    }
                    let d = (self.frame.px_x(p.x) - x).hypot(self.frame.px_y(p.y) - y);
                    if best.map_or(true, |b| d < b.0) {
                        best = Some((d, s, i));
                    }
                }
                best.map(|(_, s, i)| (s, i))
            }
            ChartKind::Scatter => {
                let mut best: Option<(f32, usize, usize)> = None;
                for (s, ser) in self.series.iter().enumerate() {
                    if let Some((i, d)) = nearest_in_px(ser.points.as_slice(), &self.frame, x, y) {
                        if d <= HOVER_REACH_PX && best.map_or(true, |b| d < b.0) {
                            best = Some((d, s, i));
                        }
                    }
                }
                best.map(|(_, s, i)| (s, i))
            }
            ChartKind::Bar | ChartKind::StackedBar => {
                let mut best: Option<(f32, usize, usize)> = None;
                for r in self.bars.iter().flatten() {
                    if x < r.x0 - 1.0 || x > r.x1 + 1.0 {
                        continue;
                    }
                    let d = if y < r.top {
                        r.top - y
                    } else if y > r.bottom {
                        y - r.bottom
                    } else {
                        0.0
                    };
                    if best.map_or(true, |b| d < b.0) {
                        best = Some((d, r.series, r.index));
                    }
                }
                best.map(|(_, s, i)| (s, i))
            }
            ChartKind::Pie | ChartKind::Donut => {
                let (dx, dy) = (x - self.pie.cx, y - self.pie.cy);
                let r = dx.hypot(dy);
                if r > self.pie.r_out || r < self.pie.r_in {
                    return None;
                }
                let mut a = dx.atan2(-dy);
                if a < 0.0 {
                    a += core::f32::consts::TAU;
                }
                self.slices
                    .iter()
                    .find(|s| a >= s.start && a < s.end)
                    .map(|s| (0, s.index))
            }
        }
    }

    /// The slice that shows category `index` (folded ones: "Other").
    fn slice_of(&self, index: usize) -> Option<&PieSlice> {
        self.slices
            .iter()
            .find(|s| s.index == index || (s.other && index >= s.index))
    }

    /// Where point `index` of series `series` is drawn, in px of the plot:
    /// a line's or a dot's point, a bar's data end, a slice's middle.
    #[must_use]
    pub(crate) fn point_px(&self, series: usize, index: usize) -> Option<(f32, f32)> {
        match self.kind {
            ChartKind::Pie | ChartKind::Donut => {
                let sl = self.slice_of(index)?;
                let a = (sl.start + sl.end) / 2.0;
                let r = if self.pie.r_in > 0.0 {
                    (self.pie.r_in + self.pie.r_out) / 2.0
                } else {
                    self.pie.r_out * 0.62
                };
                Some((
                    r.mul_add(a.sin(), self.pie.cx),
                    r.mul_add(-a.cos(), self.pie.cy),
                ))
            }
            ChartKind::Bar | ChartKind::StackedBar => {
                let r = self.bars.get(series)?.iter().find(|r| r.index == index)?;
                let negative = self
                    .series
                    .get(series)
                    .and_then(|s| s.points.as_slice().get(index))
                    .is_some_and(|p| p.y < 0.0);
                Some(((r.x0 + r.x1) / 2.0, if negative { r.bottom } else { r.top }))
            }
            _ => {
                let p = self.series.get(series)?.points.as_slice().get(index)?;
                if !(p.x.is_finite() && p.y.is_finite()) {
                    return None;
                }
                Some((self.frame.px_x(p.x), self.frame.px_y(p.y)))
            }
        }
    }

    /// The tooltip's text for a point: "North, Mar: 1,234" - or, on a pie,
    /// "Mar: 1,234 (25%)".
    #[must_use]
    pub(crate) fn tooltip_text(&self, series: usize, index: usize) -> String {
        if self.kind.is_round() {
            let Some(sl) = self.slice_of(index) else {
                return String::new();
            };
            let total: f64 = self.slices.iter().map(|s| s.value).sum();
            let name = if sl.other {
                String::from("Other")
            } else {
                category_name(&self.categories, sl.index)
            };
            let share = if total > 0.0 {
                sl.value / total * 100.0
            } else {
                0.0
            };
            return format!("{name}: {} ({share:.0}%)", format_value(sl.value));
        }
        let Some(s) = self.series.get(series) else {
            return String::new();
        };
        let Some(p) = s.points.as_slice().get(index) else {
            return String::new();
        };
        let at = if self.frame.bands > 0 {
            category_of(p.x, self.frame.bands)
                .map_or_else(|| format_value(p.x), |c| category_name(&self.categories, c))
        } else {
            format_value(p.x)
        };
        format!("{}, {at}: {}", s.name.as_str(), format_value(p.y))
    }

    /// What `on_select` reports for a point.
    #[must_use]
    pub(crate) fn selection(&self, series: usize, index: usize) -> Option<ChartSelection> {
        if self.kind.is_round() {
            let sl = self.slice_of(index)?;
            return Some(ChartSelection::create(
                0,
                sl.index,
                sl.index as f64,
                sl.value,
            ));
        }
        let p = self.series.get(series)?.points.as_slice().get(index)?;
        Some(ChartSelection::create(series, index, p.x, p.y))
    }

    /// How many points series `series` offers the keys (a pie: its slices).
    fn len_of(&self, series: usize) -> usize {
        if self.kind.is_round() {
            self.slices.len()
        } else {
            self.series.get(series).map_or(0, |s| s.points.len())
        }
    }

    /// Where `key` moves the shown point: `None` for a key the chart leaves
    /// alone, `Some(None)` to hide it (Escape). Left / Right walk the
    /// points, Up / Down the series, Home / End jump to the ends; the first
    /// key shows the first point.
    #[must_use]
    pub(crate) fn step(&self, key: VirtualKeyCode) -> Option<Option<(usize, usize)>> {
        use VirtualKeyCode as K;
        let count = if self.kind.is_round() {
            usize::from(!self.slices.is_empty())
        } else {
            self.series.len()
        };
        if key == K::Escape {
            return self.hovered.map(|_| None);
        }
        if count == 0 {
            return None;
        }
        let (s, i) = self.hovered.unwrap_or((0, 0));
        let fresh = self.hovered.is_none();
        let next = match key {
            K::Left | K::Right => {
                let len = self.len_of(s);
                if len == 0 {
                    return None;
                }
                let i = if fresh {
                    0
                } else if key == K::Right {
                    (i + 1).min(len - 1)
                } else {
                    i.saturating_sub(1)
                };
                (s, i)
            }
            K::Home => (s, 0),
            K::End => (s, self.len_of(s).saturating_sub(1)),
            K::Up | K::Down => {
                let t = if fresh {
                    0
                } else if key == K::Down {
                    (s + 1).min(count - 1)
                } else {
                    s.saturating_sub(1)
                };
                let len = self.len_of(t);
                if len == 0 {
                    return Some(self.hovered);
                }
                (t, i.min(len - 1))
            }
            _ => return None,
        };
        Some(Some(next))
    }
}

/// Where the tooltip goes for a point at `(x, y)`: beside it, inside the
/// plot's width, above it unless it is near the top. The tip's width is
/// estimated from its text (it is not laid out yet).
#[must_use]
pub(crate) fn tooltip_place(x: f32, y: f32, text: &str, plot_width: f32) -> (f32, f32) {
    let estimate = text.chars().count() as f32 * 6.5 + 16.0;
    let left = if x + 12.0 + estimate <= plot_width {
        x + 12.0
    } else {
        (x - 12.0 - estimate).max(0.0)
    };
    let top = if y >= 36.0 { y - 34.0 } else { y + 14.0 };
    (left, top)
}

/// `opacity: 1` or `0`.
fn opacity(shown: bool) -> CssProperty {
    CssProperty::const_opacity(StyleOpacity::const_new(if shown { 100 } else { 0 }))
}

/// Shows the hovered point in `overlay` - the crosshair, its series'
/// marker, the tooltip - or hides them all. Written to the live nodes:
/// the DOM is not rebuilt.
fn show(st: &ChartState, info: &mut CallbackInfo, overlay: DomNodeId) {
    let Some(crosshair) = info.get_first_child(overlay) else {
        return;
    };
    let mut markers = Vec::with_capacity(st.markers);
    let mut cursor = info.get_next_sibling(crosshair);
    for _ in 0..st.markers {
        let Some(m) = cursor else {
            break;
        };
        markers.push(m);
        cursor = info.get_next_sibling(m);
    }
    let tooltip = info.get_last_child(overlay);

    let shown = st
        .hovered
        .and_then(|(s, i)| st.point_px(s, i).map(|p| (s, i, p)));
    let Some((s, i, (x, y))) = shown else {
        info.set_css_property(crosshair, opacity(false));
        for m in markers {
            info.set_css_property(m, opacity(false));
        }
        if let Some(t) = tooltip {
            info.set_css_property(t, opacity(false));
        }
        return;
    };

    let line = matches!(st.kind, ChartKind::Line | ChartKind::Area);
    if line {
        info.set_css_property(
            crosshair,
            CssProperty::const_left(LayoutLeft::px(x.round())),
        );
    }
    info.set_css_property(crosshair, opacity(line));
    let outer = MARKER_PX + 2.0 * RING_WIDTH as f32;
    for (k, m) in markers.iter().enumerate() {
        if k == s {
            info.set_css_property(*m, CssProperty::const_left(LayoutLeft::px(x - outer / 2.0)));
            info.set_css_property(*m, CssProperty::const_top(LayoutTop::px(y - outer / 2.0)));
        }
        info.set_css_property(*m, opacity(k == s));
    }
    if let Some(t) = tooltip {
        let text = st.tooltip_text(s, i);
        let (left, top) = tooltip_place(x, y, &text, st.frame.width);
        if let Some(node) = info.get_first_child(t) {
            info.change_node_text(node, AzString::from(text));
        }
        info.set_css_property(t, CssProperty::const_left(LayoutLeft::px(left)));
        info.set_css_property(t, CssProperty::const_top(LayoutTop::px(top)));
        info.set_css_property(t, opacity(true));
    }
}

/// The pointer moved over the plot: show the point under it.
pub(crate) extern "C" fn on_chart_pointer_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<ChartState>() else {
        return Update::DoNothing;
    };
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let hit = st.hit(pos.x, pos.y);
    if hit == st.hovered {
        return Update::DoNothing;
    }
    st.hovered = hit;
    let overlay = info.get_hit_node();
    show(&st, &mut info, overlay);
    Update::DoNothing
}

/// The pointer left the plot: hide the point - unless it only left one of
/// the overlay's own parts (every leave bubbles here; the cursor decides,
/// as the slider's leave does).
pub(crate) extern "C" fn on_chart_pointer_leave(
    mut data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let inside = match (
        info.get_cursor_relative_to_node().into_option(),
        info.get_hit_node_rect(),
    ) {
        (Some(p), Some(r)) => p.x >= 0.0 && p.y >= 0.0 && p.x < r.size.width && p.y < r.size.height,
        _ => false,
    };
    if inside {
        return Update::DoNothing;
    }
    let Some(mut st) = data.downcast_mut::<ChartState>() else {
        return Update::DoNothing;
    };
    if st.hovered.is_none() {
        return Update::DoNothing;
    }
    st.hovered = None;
    let overlay = info.get_hit_node();
    show(&st, &mut info, overlay);
    Update::DoNothing
}

/// The plot lost the keyboard: hide the point.
pub(crate) extern "C" fn on_chart_blur(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut st) = data.downcast_mut::<ChartState>() else {
        return Update::DoNothing;
    };
    if st.hovered.is_none() {
        return Update::DoNothing;
    }
    st.hovered = None;
    let overlay = info.get_hit_node();
    show(&st, &mut info, overlay);
    Update::DoNothing
}

/// A key on the focused plot ([`ChartState::step`]); the keys the chart
/// takes do not also scroll the page.
pub(crate) extern "C" fn on_chart_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let Some(mut st) = data.downcast_mut::<ChartState>() else {
        return Update::DoNothing;
    };
    let Some(next) = st.step(key) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    if next != st.hovered {
        st.hovered = next;
        let overlay = info.get_hit_node();
        show(&st, &mut info, overlay);
    }
    Update::DoNothing
}

/// An activation of the plot - a click, Enter / Space, an assistive
/// technology's default action: report the shown point (or the one under
/// the pointer) to the app's `on_select`.
pub(crate) extern "C" fn on_chart_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let (hook, selection) = {
        let Some(st) = data.downcast_ref::<ChartState>() else {
            return Update::DoNothing;
        };
        let picked = st.hovered.or_else(|| {
            info.get_cursor_relative_to_node()
                .into_option()
                .and_then(|p| st.hit(p.x, p.y))
        });
        let Some((s, i)) = picked else {
            return Update::DoNothing;
        };
        let Some(selection) = st.selection(s, i) else {
            return Update::DoNothing;
        };
        (st.on_select.clone(), selection)
    };
    match hook.into_option() {
        Some(ChartOnSelect { refany, callback }) => callback.invoke(refany, info, selection),
        None => Update::DoNothing,
    }
}

// ==== fixtures (the widget manifest's sample) ====

/// The chart the widget manifest builds (`widgets::label_convention`): three
/// series of bars over four categories, with a title, axis titles, a legend,
/// a selected bar and the table view.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// The sample chart.
    pub(crate) fn sample() -> Chart {
        let series = |name: &str, ys: [f64; 4]| {
            ChartSeries::create(
                AzString::from(name),
                ChartPointVec::from_vec(
                    ys.iter()
                        .enumerate()
                        .map(|(i, y)| ChartPoint::create(i as f64, *y))
                        .collect(),
                ),
            )
        };
        Chart::create(ChartKind::Bar, 480.0, 280.0)
            .with_title(AzString::from("Revenue by quarter"))
            .with_axis_titles(AzString::from("Quarter"), AzString::from("Revenue"))
            .with_categories(StringVec::from_vec(
                ["Q1", "Q2", "Q3", "Q4"]
                    .iter()
                    .map(|s| AzString::from(*s))
                    .collect(),
            ))
            .with_added_series(series("North", [120.0, 150.0, 90.0, 180.0]))
            .with_added_series(series("South", [80.0, 95.0, 130.0, 110.0]))
            .with_added_series(series("West", [60.0, 70.0, 75.0, 90.0]))
            .with_selected(ChartSelection::create(0, 3, 3.0, 180.0))
            .with_show_table(true)
    }
}

// CHART7-NEXT: the geometry, the build, the pointer and the keys.
#[cfg(test)]
mod geometry_tests {
    use super::*;

    fn frame(bands: usize) -> PlotFrame {
        PlotFrame {
            x_min: 0.0,
            x_max: 10.0,
            y_min: -10.0,
            y_max: 30.0,
            width: 300.0,
            height: 200.0,
            bands,
        }
    }

    fn series(name: &str, ys: &[f64]) -> ChartSeries {
        ChartSeries::create(
            AzString::from(name),
            ChartPointVec::from_vec(
                ys.iter()
                    .enumerate()
                    .map(|(i, y)| ChartPoint::create(i as f64, *y))
                    .collect(),
            ),
        )
    }

    fn ends(path: &SvgPath) -> (SvgPoint, SvgPoint) {
        let items = path.items.as_slice();
        let start = match items.first().expect("a ring has elements") {
            SvgPathElement::Line(l) => l.start,
            SvgPathElement::QuadraticCurve(q) => q.start,
            SvgPathElement::CubicCurve(c) => c.start,
        };
        let end = match items.last().expect("a ring has elements") {
            SvgPathElement::Line(l) => l.end,
            SvgPathElement::QuadraticCurve(q) => q.end,
            SvgPathElement::CubicCurve(c) => c.end,
        };
        (start, end)
    }

    #[test]
    fn a_line_is_one_open_ring_through_its_points() {
        let f = frame(0);
        let points = [
            ChartPoint::create(0.0, 0.0),
            ChartPoint::create(5.0, 10.0),
            ChartPoint::create(10.0, 30.0),
        ];
        let shape = line_shape(&points, &[0, 1, 2], &f);
        let rings = shape.rings.as_slice();
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].items.as_slice().len(), 2);
        let (start, end) = ends(&rings[0]);
        assert_eq!((start.x, start.y), (f.px_x(0.0), f.px_y(0.0)));
        assert_eq!((end.x, end.y), (300.0, 0.0));
    }

    #[test]
    fn an_area_closes_along_the_baseline() {
        let f = frame(0);
        let points = [
            ChartPoint::create(0.0, 10.0),
            ChartPoint::create(10.0, 20.0),
        ];
        let base = f.px_y(0.0);
        let shape = area_shape(&points, &[0, 1], &f, base);
        let ring = &shape.rings.as_slice()[0];
        let (start, end) = ends(ring);
        assert_eq!((start.x, start.y), (end.x, end.y), "the area is closed");
        let touches_base = ring.items.as_slice().iter().any(|e| match e {
            SvgPathElement::Line(l) => l.end.y == base,
            _ => false,
        });
        assert!(touches_base, "the area runs along the baseline");
    }

    #[test]
    fn grouped_bars_are_thin_two_px_apart_and_rounded_at_the_data_end() {
        let f = frame(3);
        let data = [
            series("a", &[10.0, 20.0, 5.0]),
            series("b", &[3.0, 4.0, 30.0]),
        ];
        let rects = bar_rects(ChartKind::Bar, &data, &f);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].len(), 3);
        let base = f.px_y(0.0);
        for r in rects.iter().flatten() {
            assert!(r.x1 - r.x0 <= MAX_BAR_PX + 1e-3, "a bar is at most 24 px");
            assert!(
                (r.bottom - base).abs() < 1e-3,
                "a positive bar stands on the baseline"
            );
            assert_eq!(r.rounded, BarEnd::Top);
        }
        let (a, b) = (rects[0][1], rects[1][1]);
        assert!(
            (b.x0 - a.x1 - SURFACE_GAP_PX).abs() < 1e-3,
            "the group's bars are 2 px apart"
        );
        let centre = (a.x0 + b.x1) / 2.0;
        assert!(
            (centre - f.px_x(1.0)).abs() < 1e-3,
            "the group is centred on its category"
        );
    }

    #[test]
    fn a_negative_bar_hangs_from_the_baseline_and_rounds_its_bottom() {
        let f = frame(1);
        let rects = bar_rects(ChartKind::Bar, &[series("a", &[-5.0])], &f);
        let r = rects[0][0];
        assert!((r.top - f.px_y(0.0)).abs() < 1e-3);
        assert!((r.bottom - f.px_y(-5.0)).abs() < 1e-3);
        assert_eq!(r.rounded, BarEnd::Bottom);
    }

    #[test]
    fn stacked_segments_sit_on_each_other_with_a_gap_and_only_the_top_one_is_rounded() {
        let f = frame(1);
        let data = [series("a", &[10.0]), series("b", &[5.0])];
        let rects = bar_rects(ChartKind::StackedBar, &data, &f);
        let (low, high) = (rects[0][0], rects[1][0]);
        assert!((low.bottom - f.px_y(0.0)).abs() < 1e-3);
        assert!((high.bottom - f.px_y(10.0)).abs() < 1e-3, "b stands on a");
        assert!(
            (low.top - (f.px_y(10.0) + SURFACE_GAP_PX)).abs() < 1e-3,
            "a 2 px gap under b"
        );
        assert!((high.top - f.px_y(15.0)).abs() < 1e-3);
        assert_eq!(low.rounded, BarEnd::None);
        assert_eq!(high.rounded, BarEnd::Top);
        assert!((low.x0 - high.x0).abs() < 1e-3 && (low.x1 - high.x1).abs() < 1e-3);
    }

    #[test]
    fn a_rounded_bar_ring_stays_inside_its_rectangle() {
        let r = BarRect {
            x0: 10.0,
            x1: 30.0,
            top: 50.0,
            bottom: 150.0,
            rounded: BarEnd::Top,
            series: 0,
            index: 0,
        };
        for e in bar_ring(&r).items.as_slice() {
            let pts: Vec<SvgPoint> = match e {
                SvgPathElement::Line(l) => vec![l.start, l.end],
                SvgPathElement::QuadraticCurve(q) => vec![q.start, q.ctrl, q.end],
                SvgPathElement::CubicCurve(c) => vec![c.start, c.ctrl_1, c.ctrl_2, c.end],
            };
            for p in pts {
                assert!(
                    p.x >= 10.0 && p.x <= 30.0 && p.y >= 50.0 && p.y <= 150.0,
                    "{p:?}"
                );
            }
        }
    }

    #[test]
    fn pie_slices_share_the_circle_in_proportion_from_twelve_o_clock() {
        let slices = pie_slices(&[1.0, 1.0, 2.0]);
        assert_eq!(slices.len(), 3);
        let tau = core::f32::consts::TAU;
        assert!(slices[0].start.abs() < 1e-6);
        assert!((slices[0].end - tau / 4.0).abs() < 1e-5);
        assert!((slices[1].end - tau / 2.0).abs() < 1e-5);
        assert!((slices[2].end - tau).abs() < 1e-5);
    }

    #[test]
    fn a_pie_of_many_categories_folds_the_rest_into_other() {
        let values: Vec<f64> = (0..12).map(|i| f64::from(i + 1)).collect();
        let slices = pie_slices(&values);
        assert_eq!(slices.len(), PALETTE_LEN);
        let other = slices.last().expect("an Other slice");
        assert!(other.other);
        let rest: f64 = values[PALETTE_LEN - 1..].iter().sum();
        assert!((other.value - rest).abs() < 1e-9);
    }

    #[test]
    fn a_negative_or_missing_value_takes_no_slice() {
        let slices = pie_slices(&[2.0, -1.0, f64::NAN, 2.0]);
        let shown: Vec<usize> = slices
            .iter()
            .filter(|s| s.end > s.start)
            .map(|s| s.index)
            .collect();
        assert_eq!(shown, vec![0, 3]);
    }

    #[test]
    fn a_wedge_stays_inside_its_circle() {
        let ring = wedge_ring(100.0, 100.0, 80.0, 48.0, 0.3, 2.0);
        for e in ring.items.as_slice() {
            if let SvgPathElement::Line(l) = e {
                for p in [l.start, l.end] {
                    let d = ((p.x - 100.0).powi(2) + (p.y - 100.0).powi(2)).sqrt();
                    assert!(d <= 80.0 + 1e-3 && d >= 48.0 - 1e-3, "{d}");
                }
            }
        }
    }

    fn chart(kind: ChartKind) -> Chart {
        Chart::create(kind, 600.0, 300.0)
            .with_added_series(series("a", &[1.0, 5.0, 3.0]))
            .with_added_series(series("b", &[2.0, 8.0, 4.0]))
    }

    #[test]
    fn the_plot_sits_inside_the_chart_right_of_its_y_labels() {
        let g = chart_geometry(&chart(ChartKind::Line).with_title(AzString::from("T")));
        assert!(g.plot_left >= Y_GUTTER);
        assert!(g.plot_left + g.frame.width <= 600.0);
        assert!(g.plot_top + g.frame.height <= g.frame_height);
        assert!(g.frame_height <= 300.0 - TITLE_HEIGHT - LEGEND_HEIGHT + 1e-3);
        assert!(g.legend, "two series get a legend");
        assert!(g.title);
    }

    #[test]
    fn a_bar_chart_starts_its_y_axis_at_zero() {
        let c = chart(ChartKind::Bar)
            .with_series(ChartSeriesVec::from_vec(vec![series("a", &[50.0, 60.0])]))
            .with_categories(StringVec::from_vec(vec![
                AzString::from("x"),
                AzString::from("y"),
            ]));
        let g = chart_geometry(&c);
        assert_eq!(g.frame.y_min, 0.0);
        assert!(g.frame.y_max >= 60.0);
        assert_eq!(g.frame.bands, 2);
        assert!(!g.legend, "one series needs no legend");
    }

    #[test]
    fn a_stacked_chart_fits_the_stacks_not_the_values() {
        let c = chart(ChartKind::StackedBar);
        let g = chart_geometry(&c);
        assert!(
            g.frame.y_max >= 13.0,
            "5 + 8 must fit, got {}",
            g.frame.y_max
        );
    }

    #[test]
    fn a_fixed_y_range_holds_whatever_the_data() {
        let g = chart_geometry(&chart(ChartKind::Line).with_y_range(0.0, 100.0));
        assert_eq!((g.frame.y_min, g.frame.y_max), (0.0, 100.0));
    }

    #[test]
    fn a_number_axis_spans_the_data_exactly() {
        let points: Vec<ChartPoint> = (0..50)
            .map(|i| ChartPoint::create(f64::from(i) * 0.5 + 3.0, 1.0))
            .collect();
        let c = Chart::create(ChartKind::Line, 600.0, 300.0).with_added_series(
            ChartSeries::create(AzString::from("a"), ChartPointVec::from_vec(points)),
        );
        let g = chart_geometry(&c);
        assert_eq!((g.frame.x_min, g.frame.x_max), (3.0, 27.5));
        assert_eq!(g.frame.bands, 0);
        assert!(g.x_ticks.is_some());
    }

    #[test]
    fn a_pie_is_a_centred_square_without_axes() {
        let c = chart(ChartKind::Pie).with_categories(StringVec::from_vec(vec![
            AzString::from("x"),
            AzString::from("y"),
            AzString::from("z"),
        ]));
        let g = chart_geometry(&c);
        assert_eq!(g.frame.width, g.frame.height);
        assert!(g.y_ticks.is_none() && g.x_ticks.is_none());
        assert!(g.legend, "a pie's slices get a legend");
    }

    #[test]
    fn an_empty_chart_still_lays_out() {
        let g = chart_geometry(&Chart::create(ChartKind::Line, 100.0, 50.0));
        assert!(g.frame.width > 0.0 && g.frame.height > 0.0);
        assert!(g.frame.y_max > g.frame.y_min);
    }
}

#[cfg(test)]
mod dom_tests {
    use azul_core::dom::{Dom, IdOrClass, NodeType, SvgNodeData};
    use azul_css::props::property::{CssProperty, CssPropertyType};

    use super::*;

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

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
            .filter(|n| classes(n).iter().any(|c| c == name))
            .collect()
    }

    fn one<'a>(dom: &'a Dom, name: &str) -> &'a Dom {
        let found = with_class(dom, name);
        assert_eq!(found.len(), 1, "exactly one {name}");
        found[0]
    }

    fn declares(node: &Dom, ty: CssPropertyType) -> bool {
        node.root
            .style
            .iter_inline_properties()
            .any(|(p, _)| p.get_type() == ty)
    }

    fn texts(dom: &Dom) -> Vec<String> {
        all_nodes(dom)
            .into_iter()
            .filter_map(|n| match n.root.get_node_type() {
                NodeType::Text(t) => Some(t.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    fn series(name: &str, ys: &[f64]) -> ChartSeries {
        ChartSeries::create(
            AzString::from(name),
            ChartPointVec::from_vec(
                ys.iter()
                    .enumerate()
                    .map(|(i, y)| ChartPoint::create(i as f64, *y))
                    .collect(),
            ),
        )
    }

    fn months() -> StringVec {
        StringVec::from_vec(
            ["Jan", "Feb", "Mar", "Apr"]
                .iter()
                .map(|s| AzString::from(*s))
                .collect(),
        )
    }

    fn line_chart() -> Chart {
        Chart::create(ChartKind::Line, 640.0, 320.0)
            .with_title(AzString::from("Revenue by month"))
            .with_categories(months())
            .with_added_series(series("North", &[10.0, 40.0, 25.0, 60.0]))
            .with_added_series(series("South", &[5.0, 15.0, 35.0, 30.0]))
            .with_added_series(series("West", &[20.0, 22.0, 18.0, 26.0]))
    }

    #[test]
    fn the_plot_declares_a_user_space_of_one_unit_per_px() {
        let chart = line_chart();
        let g = chart_geometry(&chart);
        let dom = chart.dom();
        let plot = one(&dom, PLOT_CLASS);
        match plot.root.get_svg_data() {
            Some(SvgNodeData::ViewBox {
                min_x,
                min_y,
                width,
                height,
            }) => {
                assert_eq!((*min_x, *min_y), (0.0, 0.0));
                assert_eq!((*width, *height), (g.frame.width, g.frame.height));
            }
            other => panic!("the plot carries no viewBox: {other:?}"),
        }
    }

    #[test]
    fn every_series_is_one_shape_node() {
        let dom = line_chart().dom();
        let shapes = with_class(&dom, SERIES_CLASS);
        assert_eq!(shapes.len(), 3, "one node per series");
        for s in shapes {
            assert!(matches!(s.root.get_svg_data(), Some(SvgNodeData::Path(_))));
        }
    }

    #[test]
    fn a_line_is_stroked_and_a_bar_is_filled() {
        let dom = line_chart().dom();
        let line = with_class(&dom, SERIES_CLASS)[0];
        assert!(
            declares(line, CssPropertyType::BorderTopWidth),
            "a line is its stroke"
        );
        assert!(declares(line, CssPropertyType::BorderTopColor));
        assert!(
            !declares(line, CssPropertyType::BackgroundContent),
            "a line has no fill"
        );

        let dom = line_chart().with_kind(ChartKind::Bar).dom();
        let bars = with_class(&dom, SERIES_CLASS)[0];
        assert!(
            declares(bars, CssPropertyType::BackgroundContent),
            "bars are their fill"
        );
    }

    #[test]
    fn an_area_chart_draws_a_wash_under_each_line() {
        let dom = line_chart().with_kind(ChartKind::Area).dom();
        assert_eq!(
            with_class(&dom, SERIES_CLASS).len(),
            6,
            "a wash and a line per series"
        );
    }

    #[test]
    fn the_overlay_is_one_tab_stop_named_by_the_title_and_described_by_the_summary() {
        let chart = line_chart();
        let summary = chart.summary();
        let dom = chart.dom();
        let overlay = one(&dom, OVERLAY_CLASS);
        assert!(overlay.root.get_tab_index().is_some());
        let a11y = overlay
            .root
            .get_accessibility_info()
            .expect("the overlay is named");
        assert_eq!(a11y.role, AccessibilityRole::Chart);
        assert_eq!(
            a11y.accessibility_name
                .as_ref()
                .map(|s| s.as_str().to_string()),
            Some("Revenue by month".to_string())
        );
        assert_eq!(
            a11y.description.as_ref().map(|s| s.as_str().to_string()),
            Some(summary.as_str().to_string())
        );
        let tab_stops = all_nodes(&dom)
            .into_iter()
            .filter(|n| n.root.get_tab_index().is_some())
            .count();
        assert_eq!(tab_stops, 1, "the chart is ONE Tab stop");
    }

    #[test]
    fn the_overlay_holds_the_crosshair_a_marker_per_series_and_the_tooltip() {
        let dom = line_chart().dom();
        let overlay = one(&dom, OVERLAY_CLASS);
        let kids = overlay.children.as_ref();
        assert_eq!(kids.len(), 1 + 3 + 1);
        assert!(classes(&kids[0]).iter().any(|c| c == CROSSHAIR_CLASS));
        for k in &kids[1..4] {
            assert!(classes(k).iter().any(|c| c == MARKER_CLASS));
        }
        assert!(classes(&kids[4]).iter().any(|c| c == TOOLTIP_CLASS));
    }

    #[test]
    fn the_tooltip_starts_hidden_and_is_a_live_region() {
        let dom = line_chart().dom();
        let tip = one(&dom, TOOLTIP_CLASS);
        let hidden = tip.root.style.iter_inline_properties().any(|(p, _)| {
            matches!(p, CssProperty::Opacity(v) if v.get_property().map(|o| o.inner.normalized() == 0.0).unwrap_or(false))
        });
        assert!(hidden, "the tooltip is hidden until a point is hovered");
        let a11y = tip
            .root
            .get_accessibility_info()
            .expect("the tip is announced");
        assert!(a11y.is_live_region);
        assert_eq!(
            tip.children.as_ref().len(),
            1,
            "one text node the pointer rewrites"
        );
    }

    #[test]
    fn a_legend_names_every_series_and_one_series_has_none() {
        let dom = line_chart().dom();
        assert_eq!(with_class(&dom, LEGEND_ITEM_CLASS).len(), 3);
        let legend = one(&dom, LEGEND_CLASS);
        let names = texts(legend);
        for n in ["North", "South", "West"] {
            assert!(names.iter().any(|t| t == n), "{n} missing from {names:?}");
        }
        let single = Chart::create(ChartKind::Line, 400.0, 200.0)
            .with_added_series(series("Only", &[1.0, 2.0]))
            .dom();
        assert!(with_class(&single, LEGEND_CLASS).is_empty());
    }

    #[test]
    fn a_pie_draws_a_wedge_per_category_and_names_them_in_the_legend() {
        let dom = Chart::create(ChartKind::Pie, 400.0, 300.0)
            .with_categories(months())
            .with_added_series(series("Share", &[1.0, 2.0, 3.0, 4.0]))
            .dom();
        assert_eq!(with_class(&dom, SERIES_CLASS).len(), 4);
        let legend = one(&dom, LEGEND_CLASS);
        let names = texts(legend);
        assert!(names.iter().any(|t| t == "Apr"), "{names:?}");
        assert!(with_class(&dom, TICK_CLASS).is_empty(), "a pie has no axes");
    }

    #[test]
    fn the_y_axis_labels_every_nice_tick() {
        let chart = line_chart();
        let g = chart_geometry(&chart);
        let ticks = g.y_ticks.expect("a line chart has a y axis").values().len();
        let dom = chart.dom();
        let labels = with_class(&dom, TICK_CLASS);
        assert!(
            labels.len() >= ticks,
            "{} labels for {ticks} ticks",
            labels.len()
        );
        let all = texts(&dom);
        assert!(
            all.iter().any(|t| t == "Mar"),
            "the categories label the x axis"
        );
    }

    #[test]
    fn a_large_series_is_drawn_decimated() {
        let points: Vec<ChartPoint> = (0..500_000)
            .map(|i| ChartPoint::create(f64::from(i), (f64::from(i) * 0.01).sin()))
            .collect();
        let chart = Chart::create(ChartKind::Line, 800.0, 300.0).with_added_series(
            ChartSeries::create(AzString::from("Signal"), ChartPointVec::from_vec(points)),
        );
        let width = chart_geometry(&chart).frame.width;
        let dom = chart.dom();
        let line = with_class(&dom, SERIES_CLASS)[0];
        let Some(SvgNodeData::Path(shape)) = line.root.get_svg_data() else {
            panic!("the line has a path");
        };
        let segments: usize = shape
            .rings
            .as_slice()
            .iter()
            .map(|r| r.items.as_slice().len())
            .sum();
        assert!(
            segments <= 4 * (width as usize + 2),
            "{segments} segments drawn for {width} px"
        );
    }

    #[test]
    fn the_table_view_lists_every_category() {
        let dom = line_chart().with_show_table(true).dom();
        let table = one(&dom, TABLE_CLASS);
        let rows = all_nodes(table)
            .into_iter()
            .filter(|n| matches!(n.root.get_node_type(), NodeType::Tr))
            .count();
        assert_eq!(rows, 1 + 4, "a header and a row per category");
        let cells = texts(table);
        assert!(cells.iter().any(|t| t == "South"));
        assert!(cells.iter().any(|t| t == "35"));
    }

    #[test]
    fn a_big_series_is_summarised_in_the_table() {
        let points: Vec<ChartPoint> = (0..5000)
            .map(|i| ChartPoint::create(f64::from(i), f64::from(i % 7)))
            .collect();
        let (head, rows) = table_rows(
            &Chart::create(ChartKind::Scatter, 400.0, 300.0).with_added_series(
                ChartSeries::create(AzString::from("Dots"), ChartPointVec::from_vec(points)),
            ),
        );
        assert_eq!(rows.len(), 1, "one summary row per series");
        assert_eq!(head.len(), rows[0].len());
        assert_eq!(rows[0][0], "Dots");
        assert_eq!(rows[0][1], "5,000");
    }

    #[test]
    fn the_summary_names_the_kind_the_series_and_their_ranges() {
        let s = line_chart().summary();
        let s = s.as_str();
        assert!(s.starts_with("Revenue by month: line chart"), "{s}");
        assert!(s.contains("3 series"), "{s}");
        assert!(s.contains("Jan to Apr"), "{s}");
        assert!(s.contains("North"), "{s}");
        assert!(s.contains("60"), "{s}");
    }

    #[test]
    fn a_selected_point_is_ringed() {
        let dom = line_chart()
            .with_selected(ChartSelection::create(1, 2, 2.0, 35.0))
            .dom();
        assert_eq!(with_class(&dom, SELECTION_CLASS).len(), 1);
        assert!(with_class(&line_chart().dom(), SELECTION_CLASS).is_empty());
    }

    #[test]
    fn an_empty_chart_builds() {
        let dom = Chart::create(ChartKind::Line, 300.0, 200.0).dom();
        assert_eq!(with_class(&dom, OVERLAY_CLASS).len(), 1);
        assert!(with_class(&dom, SERIES_CLASS).is_empty());
        for kind in [
            ChartKind::Area,
            ChartKind::Bar,
            ChartKind::StackedBar,
            ChartKind::Scatter,
            ChartKind::Pie,
            ChartKind::Donut,
        ] {
            let _ = Chart::create(kind, 300.0, 200.0).dom();
        }
    }

    #[test]
    fn the_palette_keeps_its_order_and_a_series_may_wear_its_own_colour() {
        let own = ChartColor::same(ColorU::rgb(1, 2, 3));
        let skin = crate::widgets::themes::flat::chart_skin();
        assert_eq!(series_color(&series("a", &[]), 0, &skin), skin.palette[0]);
        assert_eq!(
            series_color(&series("a", &[]), 9, &skin),
            skin.palette[1],
            "a ninth series starts the order again"
        );
        assert_eq!(
            series_color(&series("a", &[]).with_color(own), 0, &skin),
            own
        );
    }
}

#[cfg(test)]
mod pointer_tests {
    use azul_core::{
        dom::{Dom, IdOrClass},
        window::VirtualKeyCode,
    };

    use super::*;

    fn series(name: &str, ys: &[f64]) -> ChartSeries {
        ChartSeries::create(
            AzString::from(name),
            ChartPointVec::from_vec(
                ys.iter()
                    .enumerate()
                    .map(|(i, y)| ChartPoint::create(i as f64, *y))
                    .collect(),
            ),
        )
    }

    fn months() -> StringVec {
        StringVec::from_vec(
            ["Jan", "Feb", "Mar", "Apr"]
                .iter()
                .map(|s| AzString::from(*s))
                .collect(),
        )
    }

    fn chart(kind: ChartKind) -> Chart {
        Chart::create(kind, 640.0, 320.0)
            .with_categories(months())
            .with_added_series(series("North", &[10.0, 40.0, 25.0, 60.0]))
            .with_added_series(series("South", &[5.0, 15.0, 35.0, 30.0]))
    }

    fn overlay(dom: &Dom) -> &Dom {
        fn find(d: &Dom) -> Option<&Dom> {
            let is_overlay = d
                .root
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == OVERLAY_CLASS));
            if is_overlay {
                return Some(d);
            }
            d.children.as_ref().iter().find_map(find)
        }
        find(dom).expect("the chart has an overlay")
    }

    /// The state the built chart's overlay handlers hold.
    fn with_state<T>(chart: Chart, f: impl FnOnce(&ChartState) -> T) -> T {
        let dom = chart.dom();
        let mut data = overlay(&dom).root.get_callbacks().as_ref()[0]
            .refany
            .clone();
        let st = data
            .downcast_ref::<ChartState>()
            .expect("the overlay holds a ChartState");
        f(&st)
    }

    #[test]
    fn the_pointer_finds_the_nearest_point_of_the_nearest_line() {
        with_state(chart(ChartKind::Line), |st| {
            let (x, y) = st.point_px(1, 2).expect("South, Mar is drawn");
            assert_eq!(st.hit(x + 3.0, y - 2.0), Some((1, 2)));
            let (x, y) = st.point_px(0, 3).expect("North, Apr is drawn");
            assert_eq!(st.hit(x, y + 4.0), Some((0, 3)));
        });
    }

    #[test]
    fn a_dot_out_of_reach_is_not_hovered() {
        with_state(chart(ChartKind::Scatter), |st| {
            let (x, y) = st.point_px(0, 1).expect("drawn");
            assert_eq!(st.hit(x + 1.0, y + 1.0), Some((0, 1)));
            assert_eq!(st.hit(x + HOVER_REACH_PX * 3.0, y), None);
        });
    }

    #[test]
    fn the_bar_under_the_pointer_is_hit_and_a_pointer_over_it_finds_it_too() {
        with_state(chart(ChartKind::Bar), |st| {
            let (x, top) = st.point_px(1, 2).expect("South, Mar is drawn");
            assert_eq!(st.hit(x, top + 5.0), Some((1, 2)), "on the bar");
            assert_eq!(
                st.hit(x, top - 20.0),
                Some((1, 2)),
                "above it, in its column"
            );
        });
    }

    #[test]
    fn the_slice_under_the_pointer_is_hit() {
        let pie = Chart::create(ChartKind::Pie, 400.0, 300.0)
            .with_categories(months())
            .with_added_series(series("Share", &[1.0, 2.0, 3.0, 4.0]));
        with_state(pie, |st| {
            for c in 0..4 {
                let (x, y) = st.point_px(0, c).expect("every slice is drawn");
                assert_eq!(st.hit(x, y), Some((0, c)));
            }
            assert_eq!(
                st.hit(st.pie.cx, st.pie.cy - st.pie.r_out - 5.0),
                None,
                "outside the disc"
            );
        });
    }

    #[test]
    fn the_tooltip_names_the_series_the_category_and_the_value() {
        with_state(chart(ChartKind::Line), |st| {
            assert_eq!(st.tooltip_text(1, 2), "South, Mar: 35");
        });
        let pie = Chart::create(ChartKind::Donut, 400.0, 300.0)
            .with_categories(months())
            .with_added_series(series("Share", &[1.0, 2.0, 3.0, 4.0]));
        with_state(pie, |st| {
            assert_eq!(st.tooltip_text(0, 3), "Apr: 4 (40%)");
        });
    }

    #[test]
    fn a_click_reports_the_shown_point() {
        with_state(chart(ChartKind::Bar), |st| {
            assert_eq!(
                st.selection(1, 2),
                Some(ChartSelection::create(1, 2, 2.0, 35.0))
            );
            assert_eq!(st.selection(5, 0), None);
        });
    }

    #[test]
    fn the_keys_walk_the_points_and_the_series() {
        with_state(chart(ChartKind::Line), |st| {
            let mut probe = ChartState {
                kind: st.kind,
                frame: st.frame,
                series: st.series.clone(),
                sorted: st.sorted.clone(),
                categories: st.categories.clone(),
                bars: st.bars.clone(),
                slices: st.slices.clone(),
                pie: st.pie,
                markers: st.markers,
                on_select: OptionChartOnSelect::None,
                hovered: None,
            };
            let mut press = |k: VirtualKeyCode| {
                let next = probe.step(k);
                if let Some(n) = next {
                    probe.hovered = n;
                }
                next.map(|_| probe.hovered)
            };
            assert_eq!(
                press(VirtualKeyCode::Escape),
                None,
                "nothing shown: Escape is not ours"
            );
            assert_eq!(
                press(VirtualKeyCode::Right),
                Some(Some((0, 0))),
                "the first key shows the first point"
            );
            assert_eq!(press(VirtualKeyCode::Right), Some(Some((0, 1))));
            assert_eq!(press(VirtualKeyCode::Down), Some(Some((1, 1))));
            assert_eq!(press(VirtualKeyCode::End), Some(Some((1, 3))));
            assert_eq!(
                press(VirtualKeyCode::Right),
                Some(Some((1, 3))),
                "the last point stays"
            );
            assert_eq!(press(VirtualKeyCode::Up), Some(Some((0, 3))));
            assert_eq!(press(VirtualKeyCode::Home), Some(Some((0, 0))));
            assert_eq!(press(VirtualKeyCode::A), None, "letters are the app's");
            assert_eq!(press(VirtualKeyCode::Escape), Some(None), "Escape hides");
        });
    }

    #[test]
    fn the_tooltip_stays_inside_the_plot() {
        let (left, top) = tooltip_place(10.0, 100.0, "North, Mar: 1,234", 300.0);
        assert!(left >= 10.0 && top < 100.0, "right of and above the point");
        let (left, _) = tooltip_place(290.0, 100.0, "North, Mar: 1,234", 300.0);
        assert!(
            left < 290.0 && left >= 0.0,
            "left of a point near the right edge"
        );
        let (_, top) = tooltip_place(100.0, 5.0, "x", 300.0);
        assert!(top > 5.0, "below a point near the top");
    }
}
