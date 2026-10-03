---
slug: dashboard-tutorial
title: "Tutorial: a dashboard over a big spreadsheet"
language: en
canonical_slug: dashboard-tutorial
audience: external
maturity: wip
guide_order: 116
topic_only: false
short_desc: Load a CSV or xlsx file, show it in a table, chart it with a line and a bar chart that follow the table's filter
prerequisites: [widgets, events/callbacks]
tracked_files:
  - layout/src/widgets/chart.rs
  - examples/azul-dashboard/src/chart.rs
last_generated_rev: 2e55eef06
generated_at: 2026-10-03T00:00:00Z
default-search-keys:
  - Chart
  - ChartKind
  - ChartSeries
  - ChartPoint
  - ChartSelection
  - with_on_select
  - with_show_table
  - DataTable
---

# Tutorial: a dashboard over a big spreadsheet

## What we build

A window with a table of half a million rows - sales records loaded from a
CSV or an xlsx file - and two charts under it: a **line chart** of the value
column along the table's rows, and a **bar chart** of the value summed per
category. Filter the table and both charts follow. Click a bar and the table
filters to that category. Everything works with the mouse, the keyboard and a
screen reader, in the flat and the flora theme, light and dark.

The finished app is `examples/azul-dashboard`. This page walks through it in
the order you would write it.

## 1. The crate

An azul app is one crate: a library with the app (and the unit tests of its
model) and a thin `main.rs`. Copy the layout of `examples/azul-drive`:

```toml
[package]
name = "AzDashboard"
version = "0.1.0"
edition = "2021"

[lib]
name = "azdashboard"
path = "src/lib.rs"

[[bin]]
name = "AzDashboard"
path = "src/main.rs"

[dependencies]
azul = { path = "../../dll", package = "azul-dll", default-features = false, features = ["link-dynamic"] }
csv = "1.4"
```

`csv` is already in the workspace (AzSheets reads and writes CSV with it). For
`.xlsx` files, AzSheets' engine is `ironcalc` - the same crate opens a
workbook and hands you its cells.

## 2. Load the data

Read the file into a plain table model: a header and rows of cells, each a
text and, when it parses, a number. Do it once at start (or from an azul
`Thread` if the file is large and you want the window up first) - never from a
callback on the UI thread.

```rust
/// One cell: what the file says, and the number it holds if it is one.
pub struct Cell {
    pub text: String,
    pub number: Option<f64>,
}

/// The spreadsheet: a header and rows.
pub struct Sheet {
    pub header: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

/// A CSV file as a `Sheet`: the first record is the header.
pub fn read_csv(path: &std::path::Path) -> Result<Sheet, csv::Error> {
    let mut reader = csv::Reader::from_path(path)?;
    let header = reader.headers()?.iter().map(str::to_string).collect();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        rows.push(
            record
                .iter()
                .map(|t| Cell {
                    text: t.to_string(),
                    number: t.trim().parse::<f64>().ok(),
                })
                .collect(),
        );
    }
    Ok(Sheet { header, rows })
}
```

Half a million rows of 25 columns is a few hundred megabytes of strings; a
real app keeps the numbers as numbers and the texts interned. The example
generates its sample data (`--sample`) instead of shipping a file.

## 3. The table

The table half is the `DataTable` widget: a virtualized grid (only the rows in
view are in the DOM, so 500k rows scroll smoothly) with sorting, filtering and
editing built in, keyboard navigation and a screen-reader grid. The app keeps
the sheet; the table tells the app which rows it shows - after its filter and
its sort - and the app keeps that list in its state.

That list is all the charts need. The example's `table` module owns the
table; the rest of this page is the `chart` module
(`examples/azul-dashboard/src/chart.rs`).

## 4. Hand the charts the shown rows

The charts read the data through one small trait, so the chart code never
depends on the table's types:

```rust
pub trait ChartSource {
    /// How many rows the table shows.
    fn shown_rows(&self) -> usize;
    /// Shown row `row`'s text in `column` (a category).
    fn text(&self, row: usize, column: usize) -> &str;
    /// Shown row `row`'s number in `column`, if the cell holds one.
    fn number(&self, row: usize, column: usize) -> Option<f64>;
    /// Column `column`'s name (the axis titles).
    fn column_name(&self, column: usize) -> &str;
    /// Changes whenever the shown rows change (a filter, a sort, an edit).
    fn generation(&self) -> u64;
}
```

The app implements it over the sheet and the table's list of shown rows -
four one-line methods: `text` and `number` look up `sheet.rows[shown[row]]`,
`generation` returns a counter the app bumps whenever the table reports a new
filter or sort.

## 5. Compute the series - only when the rows change

Two series come out of the shown rows:

* the **line**: one point per row, `x` = the row's place in the table, `y` =
  the value column;
* the **bars**: the value column summed per category, the largest first; past
  twelve categories the rest fold into one "Other" bar - a chart with forty
  colours is a chart nobody can read.

```rust
pub fn line_points(src: &dyn ChartSource, value_column: usize) -> Vec<ChartPoint> {
    (0..src.shown_rows())
        .filter_map(|row| {
            let y = src.number(row, value_column)?;
            y.is_finite().then(|| ChartPoint::create(row as f64, y))
        })
        .collect()
}
```

Both are recomputed only when `generation()` changes: the layout runs on every
hover and resize, the filter does not.

```rust
pub fn refresh(&mut self, src: &dyn ChartSource) {
    let generation = src.generation();
    if self.series.as_ref().is_some_and(|s| s.generation == generation) {
        return;
    }
    self.series = Some(Series {
        generation,
        line: line_points(src, self.value_column),
        bars: bar_totals(src, self.category_column, self.value_column, MAX_BARS),
    });
}
```

## 6. Draw the charts

A `Chart` is sized in pixels (its ticks, its labels and its decimation depend
on the size), takes series of `(x, y)` points and, for a category axis, the
category names:

```rust
let line_chart = Chart::create(ChartKind::Line, w, CHART_HEIGHT)
    .with_title("Revenue by row")
    .with_axis_titles("Row (table order)", "Revenue")
    .with_added_series(ChartSeries::create("Revenue", line))
    .with_on_select(charts_ref.clone(), on_point_select as ChartOnSelectCallbackType);

let bar_chart = Chart::create(ChartKind::Bar, w, CHART_HEIGHT)
    .with_title("Revenue by Region")
    .with_axis_titles("Region", "Revenue")
    .with_categories(names)
    .with_added_series(ChartSeries::create("Total Revenue", totals))
    .with_on_select(charts_ref.clone(), on_bar_select as ChartOnSelectCallbackType);
```

`.dom()` turns each into a `Dom`. What you get for free:

* **Nice axes.** The y axis ends on round numbers in steps of 1, 2 or 5 times
  a power of ten, and every tick of an axis is written in one unit: `0`,
  `5K`, `10K`, `15K`. A bar chart always starts at zero.
* **Half a million points.** A line is drawn *decimated*: for every pixel
  column the first, lowest, highest and last point - the picture of 500k
  points is the picture of four per column, and nothing visible is lost. The
  tooltip still reads the exact row under the pointer.
* **The tooltip and the crosshair.** Hover a line and the nearest point of
  the nearest series is marked and named ("Revenue, 12,345: 1,234.5"); hover
  a bar or a slice and it is named. The hover never rebuilds your DOM.
* **A legend** for two or more series (a single series is named by the
  title), and **colours** from a fixed categorical order that stays apart for
  colour-blind readers, with a separate step for the dark mode. Text never
  wears a series colour.
* **The keyboard and screen readers.** Each chart is one Tab stop. Left /
  Right walk the points, Up / Down the series, Home / End jump to the ends,
  Enter or Space picks the point, Escape hides the tooltip. The chart is
  announced as a chart, named by its title and described by a text summary
  (`Chart::summary`: the kind, the series and their ranges); the tooltip is a
  live region. `.with_show_table(true)` adds the data as a table under the
  chart.
* **Themes and modes.** The chart follows the app theme (flat, flora) and the
  OS mode (light, dark) by itself. `.with_shell_accent(accent)` gives the
  selection ring and the focus ring your app's accent family.

The other kinds are one word away: `ChartKind::Area` (the line with a light
wash under it), `StackedBar`, `Scatter` (dense clouds keep one dot per
occupied cell), `Pie` and `Donut` (the first series over the categories,
twelve o'clock clockwise, the rest folded into "Other" past eight slices).

## 7. Pick a point, filter the table

`with_on_select` reports what the user picked - a click, Enter or Space on
the focused chart, or a screen reader's default action - as a
`ChartSelection`: the series, the point's index and its values. The example
keeps the pick in its own state, hands it back with `.with_selected(pick)` so
the chart rings it, and offers the picked bar's category to the table:

```rust
extern "C" fn on_bar_select(mut data: RefAny, _info: CallbackInfo, picked: ChartSelection) -> Update {
    let Some(mut charts) = data.downcast_mut::<Charts>() else {
        return Update::DoNothing;
    };
    charts.picked_bar = Some(picked);
    Update::RefreshDom
}
```

The app's layout asks `charts.picked_category()` and, when it changed, sets
the table's filter to that category. The table reports the new shown rows,
the app bumps the generation, the charts recompute - and the line now shows
that one region.

## 8. Put it on the screen

The app's layout puts the table and the charts in the shell body:

```rust
let charts = chart::charts_dom(&app.charts, &shown_rows, window_width);
```

`charts_dom` lays the two charts side by side in a wide window and stacks
them in a narrow one, with a caption under them that names the pick.

## 9. Test it

The model is plain Rust, so most of it is unit-tested without a window: the
line has a point per shown row, the bars sum per category largest first, more
than twelve categories fold into "Other", a new generation recomputes and
drops the pick, the caption names the pick
(`examples/azul-dashboard/src/chart.rs`, `mod tests`). The widget itself is
tested in `layout/src/widgets/chart.rs` (the tick algorithm, the decimation,
the geometry, the DOM, the pointer and the keys) and its pixels in
`layout/tests/a_chart_paints_its_series_through_the_svg_path.rs`.

For the whole app, run it headless with the debug server
(`AZ_BACKEND=headless AZ_DEBUG=<port>`) and drive it with an E2E script: type
a filter, `wait_frame`, read the bar chart's tick labels and the caption with
`get_node_layout` / `get_node_hierarchy`, click a bar, read the table again.
