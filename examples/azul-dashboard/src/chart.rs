//! The dashboard's charts: a line chart and a bar chart over the rows the
//! table shows. The table's filter changes the rows; the charts follow,
//! through the app's state - nothing here talks to the table directly.
//!
//! * The LINE runs along the shown rows in the table's order (x = the row's
//!   place, y = the value column). Every row is a point: half a million of
//!   them when nothing is filtered, and the chart widget draws them
//!   decimated - the first, lowest, highest and last point of every pixel
//!   column - while the tooltip still reads the exact row under the pointer.
//!   Sort the table by date and the line is the value over time.
//! * The BARS sum the value column per category (the category column), the
//!   largest first; past [`MAX_BARS`] the rest fold into one "Other" bar.
//!
//! Clicking a point or a bar (or Enter on the focused chart) selects it: the
//! chart rings it and the caption under the charts names it. The picked
//! category is offered to the app ([`Charts::picked_category`]) - the
//! dashboard uses it to filter the table to that category.
//!
//! The module owns its state ([`Charts`]) in a `RefAny` of its own, so its
//! callbacks need not know the app's type. The app hands the charts its rows
//! through one small trait ([`ChartSource`]) and calls [`charts_dom`] from
//! its layout.

use std::collections::HashMap;

use azul::{
    callbacks::ChartOnSelectCallbackType,
    prelude::*,
    str::String as AzString,
    widgets::{Chart, ChartKind, ChartPoint, ChartSelection, ChartSeries},
};

/// The charts' row (the app's `__azdashboard_` prefix).
pub const CHARTS: AzString = AzString::from_const_str("__azdashboard_charts");
/// The caption under the charts (what is selected).
pub const CHARTS_CAPTION: AzString = AzString::from_const_str("__azdashboard_charts_caption");

/// At most this many bars; the rest fold into "Other".
pub const MAX_BARS: usize = 12;
/// Each chart's height in px.
pub const CHART_HEIGHT: f32 = 300.0;
/// The gap around and between the charts.
pub const GAP: f32 = 16.0;
/// Narrower than this, the charts stack instead of standing side by side.
pub const SIDE_BY_SIDE_MIN: f32 = 900.0;

const ROW_CSS: &str = "display: flex; flex-direction: row; flex-wrap: wrap; column-gap: 16px; row-gap: 16px; padding: 16px; flex-shrink: 0;";
const CAPTION_CSS: &str = "margin: 0px 16px 12px 16px; font-size: 12px;";

/// What the charts read from the dashboard: the rows the table shows now
/// (after its filter and its sort), a text and a number per cell.
pub trait ChartSource {
    /// How many rows the table shows.
    fn shown_rows(&self) -> usize;
    /// Shown row `row`'s text in `column` (a category).
    fn text(&self, row: usize, column: usize) -> &str;
    /// Shown row `row`'s number in `column`, if the cell holds one.
    fn number(&self, row: usize, column: usize) -> Option<f64>;
    /// Column `column`'s name (the axis titles).
    fn column_name(&self, column: usize) -> &str;
    /// Changes whenever the shown rows change (a filter, a sort, an edit):
    /// the charts recompute their series only then.
    fn generation(&self) -> u64;
}

/// The series, computed from one generation of the shown rows.
#[derive(Debug, Clone, PartialEq)]
struct Series {
    generation: u64,
    line: Vec<ChartPoint>,
    bars: Vec<(String, f64)>,
}

/// The charts' state: which columns they read and what the user picked.
#[derive(Debug, Clone, PartialEq)]
pub struct Charts {
    /// The column the bars group by.
    pub category_column: usize,
    /// The column the line and the bars sum.
    pub value_column: usize,
    /// The point picked on the line.
    pub picked_point: Option<ChartSelection>,
    /// The bar picked.
    pub picked_bar: Option<ChartSelection>,
    series: Option<Series>,
}

impl Charts {
    /// Charts grouping by `category_column` and summing `value_column`.
    #[must_use]
    pub const fn new(category_column: usize, value_column: usize) -> Self {
        Self {
            category_column,
            value_column,
            picked_point: None,
            picked_bar: None,
            series: None,
        }
    }

    /// Recompute the series if the shown rows changed since the last time.
    /// A new generation also drops the picks: they named rows that may be
    /// gone.
    pub fn refresh(&mut self, src: &dyn ChartSource) {
        let generation = src.generation();
        if self
            .series
            .as_ref()
            .is_some_and(|s| s.generation == generation)
        {
            return;
        }
        self.series = Some(Series {
            generation,
            line: line_points(src, self.value_column),
            bars: bar_totals(src, self.category_column, self.value_column, MAX_BARS),
        });
        self.picked_point = None;
        self.picked_bar = None;
    }

    /// The category of the picked bar, for the app to filter the table by
    /// ("Other" is not a category: `None`).
    #[must_use]
    pub fn picked_category(&self) -> Option<&str> {
        let pick = self.picked_bar?;
        let (name, _) = self.series.as_ref()?.bars.get(pick.index)?;
        (name != OTHER).then_some(name.as_str())
    }
}

/// The folded bar's name.
const OTHER: &str = "Other";

/// The line's points: the value column along the shown rows, x = the row's
/// place. A row without a number in the column is left out (the line joins
/// its neighbours).
#[must_use]
pub fn line_points(src: &dyn ChartSource, value_column: usize) -> Vec<ChartPoint> {
    (0..src.shown_rows())
        .filter_map(|row| {
            let y = src.number(row, value_column)?;
            #[allow(clippy::cast_precision_loss)] // row counts are far below 2^52
            let x = row as f64;
            y.is_finite().then(|| ChartPoint::create(x, y))
        })
        .collect()
}

/// The bars: the value column summed per category, the largest first; past
/// `max` categories the rest fold into one "Other" bar (so no two bars share
/// a colour slot and the axis stays readable).
#[must_use]
pub fn bar_totals(
    src: &dyn ChartSource,
    category_column: usize,
    value_column: usize,
    max: usize,
) -> Vec<(String, f64)> {
    let mut sums: HashMap<&str, f64> = HashMap::new();
    for row in 0..src.shown_rows() {
        let Some(v) = src.number(row, value_column).filter(|v| v.is_finite()) else {
            continue;
        };
        *sums.entry(src.text(row, category_column)).or_insert(0.0) += v;
    }
    let mut totals: Vec<(String, f64)> = sums
        .into_iter()
        .map(|(name, sum)| (name.to_string(), sum))
        .collect();
    // Largest first; equal sums by name, so the order is stable.
    totals.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if max > 0 && totals.len() > max {
        let rest: f64 = totals[max - 1..].iter().map(|t| t.1).sum();
        totals.truncate(max - 1);
        totals.push((OTHER.to_string(), rest));
    }
    totals
}

/// The width of each chart: two side by side in `width` px, or one per row
/// when the window is narrower than [`SIDE_BY_SIDE_MIN`].
#[must_use]
pub fn chart_width(width: f32) -> f32 {
    if width >= SIDE_BY_SIDE_MIN {
        3.0f32.mul_add(-GAP, width) / 2.0
    } else {
        2.0f32.mul_add(-GAP, width).max(280.0)
    }
}

/// The caption under the charts: what is picked, or how to pick.
#[must_use]
pub fn caption(charts: &Charts, value_name: &str) -> String {
    let Some(series) = charts.series.as_ref() else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(p) = charts.picked_point {
        parts.push(format!("Row {}: {value_name} {:.2}", p.x + 1.0, p.y));
    }
    if let Some(b) = charts.picked_bar {
        if let Some((name, total)) = series.bars.get(b.index) {
            parts.push(format!("{name}: {value_name} {total:.2} in total"));
        }
    }
    if parts.is_empty() {
        format!(
            "{} rows charted. Click a point or a bar (or Tab to a chart and press Enter) to pick it.",
            series.line.len()
        )
    } else {
        parts.join("   |   ")
    }
}

/// The line chart picked a point.
extern "C" fn on_point_select(
    mut data: RefAny,
    _info: CallbackInfo,
    picked: ChartSelection,
) -> Update {
    let Some(mut charts) = data.downcast_mut::<Charts>() else {
        return Update::DoNothing;
    };
    charts.picked_point = Some(picked);
    Update::RefreshDom
}

/// The bar chart picked a bar.
extern "C" fn on_bar_select(
    mut data: RefAny,
    _info: CallbackInfo,
    picked: ChartSelection,
) -> Update {
    let Some(mut charts) = data.downcast_mut::<Charts>() else {
        return Update::DoNothing;
    };
    charts.picked_bar = Some(picked);
    Update::RefreshDom
}

/// The charts' row: the line chart and the bar chart over the rows `src`
/// shows, `width` px wide in total, and the caption. `charts_ref` is the
/// `RefAny` holding the [`Charts`]; the series are recomputed only when
/// `src.generation()` changed.
pub fn charts_dom(charts_ref: &RefAny, src: &dyn ChartSource, width: f32) -> Dom {
    let mut data = charts_ref.clone();
    let Some(mut charts) = data.downcast_mut::<Charts>() else {
        return Dom::create_div();
    };
    charts.refresh(src);
    let value_name = src.column_name(charts.value_column).to_string();
    let category_name = src.column_name(charts.category_column).to_string();
    let w = chart_width(width);
    let (line, bars) = charts
        .series
        .as_ref()
        .map(|s| (s.line.clone(), s.bars.clone()))
        .unwrap_or_default();

    let mut line_chart = Chart::create(ChartKind::Line, w, CHART_HEIGHT)
        .with_title(format!("{value_name} by row").as_str())
        .with_axis_titles("Row (table order)", value_name.as_str())
        .with_added_series(ChartSeries::create(value_name.as_str(), line))
        .with_on_select(
            charts_ref.clone(),
            on_point_select as ChartOnSelectCallbackType,
        );
    if let Some(p) = charts.picked_point {
        line_chart = line_chart.with_selected(p);
    }

    let names: Vec<AzString> = bars
        .iter()
        .map(|(n, _)| AzString::from(n.as_str()))
        .collect();
    #[allow(clippy::cast_precision_loss)] // at most MAX_BARS bars
    let totals: Vec<ChartPoint> = bars
        .iter()
        .enumerate()
        .map(|(i, (_, v))| ChartPoint::create(i as f64, *v))
        .collect();
    let mut bar_chart = Chart::create(ChartKind::Bar, w, CHART_HEIGHT)
        .with_title(format!("{value_name} by {category_name}").as_str())
        .with_axis_titles(category_name.as_str(), value_name.as_str())
        .with_categories(names)
        .with_added_series(ChartSeries::create(
            format!("Total {value_name}").as_str(),
            totals,
        ))
        .with_on_select(
            charts_ref.clone(),
            on_bar_select as ChartOnSelectCallbackType,
        );
    if let Some(b) = charts.picked_bar {
        bar_chart = bar_chart.with_selected(b);
    }

    let text = caption(&charts, &value_name);
    Dom::create_div()
        .with_class(CHARTS)
        .with_child(
            Dom::create_div()
                .with_css(ROW_CSS)
                .with_child(line_chart.dom())
                .with_child(bar_chart.dom()),
        )
        .with_child(
            Dom::create_p_with_text(text.as_str())
                .with_class(CHARTS_CAPTION)
                .with_css(CAPTION_CSS),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table of `(region, revenue)` rows, all shown.
    struct Rows {
        rows: Vec<(String, Option<f64>)>,
        generation: u64,
    }

    impl ChartSource for Rows {
        fn shown_rows(&self) -> usize {
            self.rows.len()
        }
        fn text(&self, row: usize, _column: usize) -> &str {
            &self.rows[row].0
        }
        fn number(&self, row: usize, _column: usize) -> Option<f64> {
            self.rows[row].1
        }
        fn column_name(&self, column: usize) -> &str {
            if column == 0 {
                "Region"
            } else {
                "Revenue"
            }
        }
        fn generation(&self) -> u64 {
            self.generation
        }
    }

    fn rows(data: &[(&str, Option<f64>)]) -> Rows {
        Rows {
            rows: data.iter().map(|(n, v)| ((*n).to_string(), *v)).collect(),
            generation: 1,
        }
    }

    #[test]
    fn the_line_has_a_point_per_shown_row_with_a_number() {
        let src = rows(&[("N", Some(1.0)), ("S", None), ("N", Some(3.0))]);
        let points = line_points(&src, 1);
        assert_eq!(points.len(), 2);
        assert_eq!(
            (points[1].x, points[1].y),
            (2.0, 3.0),
            "x is the row's place"
        );
    }

    #[test]
    fn the_bars_sum_per_category_largest_first() {
        let src = rows(&[
            ("North", Some(10.0)),
            ("South", Some(30.0)),
            ("North", Some(25.0)),
            ("West", None),
        ]);
        let bars = bar_totals(&src, 0, 1, MAX_BARS);
        assert_eq!(
            bars,
            vec![("North".to_string(), 35.0), ("South".to_string(), 30.0)]
        );
    }

    #[test]
    fn past_the_most_bars_the_rest_fold_into_other() {
        let data: Vec<(String, Option<f64>)> = (0..20)
            .map(|i| (format!("C{i:02}"), Some(f64::from(i))))
            .collect();
        let src = Rows {
            rows: data,
            generation: 1,
        };
        let bars = bar_totals(&src, 0, 1, 5);
        assert_eq!(bars.len(), 5);
        assert_eq!(bars[0].0, "C19");
        assert_eq!(bars[4].0, OTHER);
        let rest: f64 = (0..16).map(f64::from).sum();
        assert!((bars[4].1 - rest).abs() < 1e-9);
    }

    #[test]
    fn the_series_follow_a_new_generation_of_rows_and_drop_the_picks() {
        let mut charts = Charts::new(0, 1);
        let mut src = rows(&[("North", Some(1.0)), ("South", Some(2.0))]);
        charts.refresh(&src);
        charts.picked_bar = Some(ChartSelection::create(0, 1, 1.0, 1.0));
        assert_eq!(charts.picked_category(), Some("North"));

        charts.refresh(&src);
        assert!(charts.picked_bar.is_some(), "the same rows keep the pick");

        src.rows.push(("South".to_string(), Some(5.0)));
        src.generation = 2;
        charts.refresh(&src);
        assert_eq!(charts.picked_bar, None, "a new filter drops the pick");
        let bars = &charts.series.as_ref().expect("computed").bars;
        assert_eq!(bars[0], ("South".to_string(), 7.0));
    }

    #[test]
    fn other_is_not_a_category_to_filter_by() {
        let data: Vec<(String, Option<f64>)> =
            (0..20).map(|i| (format!("C{i}"), Some(1.0))).collect();
        let mut charts = Charts::new(0, 1);
        charts.refresh(&Rows {
            rows: data,
            generation: 1,
        });
        charts.picked_bar = Some(ChartSelection::create(0, MAX_BARS - 1, 0.0, 0.0));
        assert_eq!(charts.picked_category(), None);
    }

    #[test]
    fn two_charts_stand_side_by_side_in_a_wide_window_and_stack_in_a_narrow_one() {
        assert!((chart_width(1200.0) - 576.0).abs() < 1e-3);
        assert!((chart_width(600.0) - 568.0).abs() < 1e-3);
    }

    #[test]
    fn the_caption_names_the_pick() {
        let mut charts = Charts::new(0, 1);
        charts.refresh(&rows(&[("North", Some(10.0)), ("South", Some(4.0))]));
        assert!(caption(&charts, "Revenue").starts_with("2 rows charted"));
        charts.picked_bar = Some(ChartSelection::create(0, 0, 0.0, 10.0));
        assert_eq!(caption(&charts, "Revenue"), "North: Revenue 10.00 in total");
    }
}
