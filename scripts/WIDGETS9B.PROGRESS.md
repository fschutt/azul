# WIDGETS9B progress - Gauge, DateRangePicker, MoneyInput, ReferencePicker

Branch `wt/widgets9b` from `e537ddbe2` (wave 8 integrated). Never compile (house rules).
Brief: scripts/waves/wave9/PLAN.md "WIDGETS9B"; spec ../azul-apps/planning/foundation/05-widget-backlog.md
(ReferencePicker "extend ComboBox: type-to-filter, arrow/Enter, async search callback, id->label, debounce,
create-new row"; Gauge "radial ring/arc value with centre label"; DateRangePicker "two-month range selection with
presets"; MoneyInput "fixed-precision decimal input with currency suffix, locale grouping").
Commit messages: written to /tmp/w9b_msg.txt, `git commit -F` (heredocs / `>>` appends are refused by the
worktree guard - edit files with the Edit tool; `rustfmt --edition 2021 <file>` only on MY new files).

## DONE
- 6719c21b2 progress file
- MoneyInput (layout/src/widgets/money_input.rs): 8c30cb223 types/stubs, 24196ba32 RED pure tests, b39859f4e
  GREEN, d873279dc widget + hooks, 176b6ff6d flat/flora skins (theme APPENDS `// ==== money_input ====`),
  9abb473ce DOM tests + manifest (every_widget_dom + INPUTS: "money_input", "money_input (empty, en)")
- 6992fc8c6 theme_blocks::{skins_of, structure_skin, part_of} (ChartLook::part delegates)
- Gauge (layout/src/widgets/gauge.rs): 7839d2b66 types/stubs, 571023f09 RED geometry tests, df38b2a03 GREEN,
  0ed2f6ca5 the DOM (dial / ring / bar, `GaugeSkin`, `value_color`, a11y meter). chart::over_plot is now pub(crate).

## IN PROGRESS
- Gauge: NOT yet done - the theme skins and the DOM tests and the manifest.

## NEXT (exact)
1. Gauge theme APPENDS: `pub(crate) fn gauge_skin() -> crate::widgets::gauge::GaugeSkin` at the END of
   layout/src/widgets/themes/flat.rs and flora.rs under `// ==== gauge ====` (append after `money_input_skin`).
   flat: root = [font_family(SYSTEM_UI_FAMILY)] + themed_ink(LIGHT_INK, DARK_INK); value_text = [semibold] +
   themed_ink(LIGHT_INK, DARK_INK); label = themed_ink(LIGHT_INTRO, DARK_INTRO); track (LIGHT_TRACK, DARK_TRACK);
   ok (#198754, #75B798), warn (#E0A800, #FFDA6A), bad (#DC3545, #EA868F), neutral (#6C757D, #ADB5BD),
   accent (LIGHT_ACC, DARK_ACC), marker None. flora: same inks; ok/warn/bad/neutral = STONE_LEAF / STONE_AMBER /
   STONE_CLAY / STONE_SLATE as ChartColor::create(stone, glow); accent (LIGHT_ACC, DARK_GLOW);
   marker Some("__azul-theme-flora").
2. Gauge tests (`mod dom_tests` at the end of gauge.rs) + `pub(crate) mod fixtures { sample() }` (CPU 73 %, bands
   ok 0-70 / warn 70-90 / bad 90-100, label "CPU", unit "%") and `linear()`: dial ViewBox = size; one path node
   each for track / band / value; no value node at min; value_color = band colour / accent; texts "73%" + "CPU";
   root role Indicator, name "CPU", value "73% (warning)"; no tab index; linear value width = f * size;
   follows the app theme (theme_blocks::checks::assert_follows_the_app_theme); invariants per theme.
3. Manifest in layout/src/widgets/mod.rs: append `all.push(("gauge", super::gauge::fixtures::sample().dom()))`
   and `("gauge (linear)", ...)` after the money_input pushes; add both names to the theme-contrast STATUS group.
4. DateRangePicker (date_range_picker.rs) - design below (D4).
5. ComboBox extensions + ReferencePicker (D5).
6. Report scripts/WIDGETS9B_<date>.md (api.json list, least-sure spots, test commands).

## Decisions (unattended)
- D1 App-owned state, as DataTable / Timeline / DatePicker month nav: every widget reports an event carrying the
  NEXT state; the app stores it and rebuilds (`Update::RefreshDom`). In-place restyles only where the change keeps
  the node set (the range preview, the active option).
- D2 MoneyInput wraps TextInput like NumberInput (no twin of the field); amounts are i64 MINOR units, never floats.
  Locale = `MoneyLocale { decimal_separator, group_separator (u32 code points), symbol_position, symbol_spaced }`
  with `from_tag` (a small table) and `from_sample` (separators read off a number the app's localizer
  formatted). Currency = `MoneyCurrency { code, symbol, minor_digits }`, `from_code` (ISO 4217 table).
  The currency code shows in an addon box beside the field. Keystroke rule: complete / still-completable text
  accepted (amount None while incomplete); garbage, too many decimals, '-' when negatives are off, i64 overflow
  vetoed; min / max reported (BelowMin / AboveMax), never vetoed. Blur reformats via `TextInput::set_text_in`
  and reports `on_commit`. Lenient decimal: a lone '.' / ',' with 1..=minor digits after it is the point.
  The field uses the plain TextInput look (not flora NumberInput's well) - no twin of number_input's styling.
- D3 Gauge draws with the chart's vector path (chart::wedge_ring + SvgNodeData::Path in a ViewBox), no second
  renderer; band / value colours are `ChartColor` pairs; role Indicator (= accesskit Meter) via `MeterAriaInfo`.
  Not focusable (like <meter>). STATUS theme-contrast group.
- D4 DateRangePicker (planned): reuse DatePicker's pieces - refactor `themes::{flat,flora}::date_picker` into
  `date_picker_look()` + `date_picker(d)`; extract a generic day-grid builder from `date_picker::build_grid_with`
  (rows of 7 with leading blanks, a closure per day cell); make `days_in_month`, `weekday`, `month_name`,
  `day_accessibility_name`, `shifted_date`, `build_weekday_row_from`, `header_nav_button`, `CellFaces` pub(crate).
  Own handlers: click (first = anchor, second = range end -> event), hover / keyboard focus = in-place preview
  (restyle both grids: endpoints selected face, between washed), Escape drops the anchor, PageUp/Down + header
  arrows = Navigated (app rebuilds), presets column (Today, Yesterday, Last 7 / 30 days, This / Last week,
  This / Last month, This / Last year) -> Preset event. State `DateRangePickerView { range: OptionDateRange,
  anchor: OptionDatePickerState, year, month }`; one `on_event` callback triple (DateRangePickerEvent).
- D5 ReferencePicker (planned): built ON ComboBox (no twin of the field / popup / keys): extend ComboBox with
  (a) the toggle asking `info.is_transient_window_open(popup)` instead of its stale flag (survives the app's
  rebuild on every query), (b) optional per-item detail lines, (c) an optional status line (Searching... /
  No matches) after the options, (d) open the list on typing. ReferencePicker: records `{ id: u64, label,
  detail }`, `query`, `selected: OptionU64`, `loading`, `create_label` ("Create ..." row = the last option),
  Local filter (contains, case-insensitive) or App filter (the app's async search), debounce via a timer
  (`Timer::create(..).with_delay(Duration::from_millis(..))`), one `on_event` (Query / Pick / Create / Clear).

## Open questions
- (none)
