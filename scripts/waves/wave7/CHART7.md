# CHART7 - a Chart widget + the dashboard's charts + the tutorial (wave 7)
Why: same as DATATABLE7 (a beginner wants tables AND charts). You build the chart half.
Owns: NEW layout/src/widgets/chart.rs (+ tests, theme APPENDS, list APPENDS in widgets/mod.rs), a `chart` module in
examples/azul-dashboard (DATATABLE7 creates the crate - if it is not in your base, put your module in
examples/azul-dashboard/src/chart.rs anyway and note the lib.rs line for the parent), the tutorial
doc/guide/dashboard_tutorial.md (or wherever the site's guides live - search doc/ for the guides folder).
Read first: how existing widgets draw (svg / canvas / cpurender paths: grep for `SvgMultiPolygon`, `tessellate`,
the timeline / progress widgets), the theme accent colours (`ShellThemeAccent::colors`).

1. Chart: line, bar (grouped / stacked), scatter, pie / donut; data as series of (x, y) f64 (or category, value);
   axes with nice ticks (1-2-5 steps), labels, gridlines, legend, a title; hover shows the value under the pointer
   (tooltip), click reports the point (on_select callback). Colours from the theme accents; both themes x modes;
   a11y: a text summary / data table alternative for screen readers.
2. Large series: decimate for drawing (min / max per pixel column) so 500k points stay smooth; a unit test of the
   decimation and of the tick algorithm.
3. Drawing: use the engine's existing vector path (SVG tessellation / the display list) - no new rendering backend;
   if something is missing in the engine, RED test + note for PAINT7.
4. api.json list (exact methods, create*, callbacks as triples), the widget manifest entry.
5. The dashboard's chart half (a line chart and a bar chart over the same data set, updating when the table's
   filter changes - through the app's state) and the tutorial: "a dashboard over a big spreadsheet" - load a CSV /
   xlsx (IronCalc or a CSV reader already in the workspace), table + charts, step by step, the code from the example.

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/CHART7.PROGRESS.md exact. Finish with the report
scripts/CHART7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
