# CHART7 progress (wave 7, 2026-10-03)

Branch: wt/chart7 (base 2e55eef06)

## DONE
- 3a3269baa progress file; b94bb9bf1 design
- 9e152cc78 chart.rs piece 1: data types + Chart builder, `pub mod chart;` appended in widgets/mod.rs
- cb7fef475 RED math tests; 2323d7036 GREEN math (nice ticks, PlotFrame, decimate_line, thin_scatter,
  nearest_by_x, TickFormat, format_value)
- 2f15dd751 RED geometry tests; a4730e3c3 GREEN geometry (chart_geometry, line/area/dots/bars/wedges)
- 2f0afcff4 RED DOM tests; b619781ee look + summary + table rows; b4ccaa6a6 the DOM build;
  1ae310e66 pointer + keys (ChartState, show(), on_chart_*); 77225318f flat/flora chart_skin appends;
  be755a7d7 manifest entry (every_widget_dom + CHROME group) + chart::fixtures

## IN PROGRESS
- item 1: layout/src/widgets/chart.rs, written in pieces (types -> math -> geometry -> build -> callbacks)

## NEXT
- review pass of chart.rs for compile errors (read it all once)
- item 3: an integration test in layout/tests (render a chart with cpurender, count series pixels) + run the
  prebuilt binaries? (no app has a chart yet -> pixels via render_dom_to_rgba in a test only)
- item 4: api.json list; item 5: examples/azul-dashboard/src/chart.rs + doc/guide/en/dashboard_tutorial.md

## Design (decided, read before continuing)
- DRAWING: the engine's SVG path - a plot div carries `SvgNodeData::ViewBox{0,0,pw,ph}` (1 unit = 1 px), every
  series is ONE absolutely placed div (insets 0, like the XML parser's shapes) with `SvgNodeData::Path`:
  fill = background (bars / scatter dots / pie wedges, clipped to the path), stroke = border width + colour
  (lines; `display_list::svg_stroke_for`). Gridlines / baseline / crosshair are plain 1px divs.
- COLOURS: the dataviz reference categorical order (blue, orange, aqua, yellow, magenta, green, violet, red),
  light + dark steps, validated with the dataviz validator on flat (#FFFFFF / #212529) and flora (#F2F1ED /
  #232323) surfaces: all hard gates pass; light mode has 3 slots under 3:1 -> relief = legend + table view.
  The ShellThemeAccent stones FAIL as series colours (lightness band, chroma floor, leaf/clay CVD dE 2.1), so
  the accent colours the chart's focus ring / selection ring, never a series; an app can override a series
  colour (ChartSeries::with_color(ChartColor)).
- THEMES: `ChartSkin` per theme (flat.rs / flora.rs appends `chart_skin()`), `ChartLook::of(theme)` builds every
  part from the pinned skin, or from both merged with `theme_blocks::follow_props` (DOM built ONCE - no
  follow_app_theme double build of 500k points).
- DATA: series of ChartPoint{x,y} f64; `categories` non-empty -> categorical x (band scale: bars, and lines /
  scatter at band centres so a line and a bar over the same data align). Pie / donut: series 0 over categories.
- LARGE: line = M4 decimation per pixel column (first / min / max / last, run-based so unsorted x also works);
  scatter = one dot per occupied grid cell. Hover = nearest point by binary search (sorted x) or scan.
- HOVER without rebuild: the overlay (last child of the plot, the Tab stop, role Chart) holds the crosshair, one
  marker per series and the tooltip (the tooltip widget's skin, `tooltip::skin_of`); MouseMove writes
  left / top / opacity with `set_css_property` and the text with `change_node_text`. Click -> on_select.
  Keys: Left / Right walk points, Up / Down series, Home / End, Enter / Space select, Escape hides.
- A11Y: overlay AccessibilityInfo role Chart, name = title, description = text summary (`Chart::summary`);
  `with_table_view(true)` adds a data table (full for categories, per-series summary rows for big series).
- SIZE: the chart is sized in px (`create(kind, width, height)`), like the spinner: ticks, decimation and
  stroke widths need the pixel size at build time.

## Decisions
- examples/azul-dashboard is not in the base (DATATABLE7 creates it on its own branch): the chart module goes to
  examples/azul-dashboard/src/chart.rs anyway; the lib.rs lines are named in the report.

## Open questions
- (none yet)
