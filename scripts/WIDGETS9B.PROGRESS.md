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

- Gauge done: 0a291c47b flat/flora `gauge_skin` (theme APPENDS `// ==== gauge ====`), 63dfe564d DOM tests +
  manifest (every_widget_dom "gauge", "gauge (linear)"; theme-contrast STATUS group).

## IN PROGRESS
- DateRangePicker (layout/src/widgets/date_range_picker.rs, new).

- DateRangePicker so far: 98edb438a `date_picker_look()` in both themes, ffc835915 `date_picker::day_grid` +
  pub(crate) helpers (month_name, day_accessibility_name, build_weekday_row_from, header_nav_button, ringed,
  washed, shifted_date, cell_faces, PREV_ARROW / NEXT_ARROW, HEADER_CLASS / HEADER_LABEL_CLASS),
  0974aae78 types + stubs, 0414f8059 RED range tests (`mod range_tests`).

  cd879c8ed GREEN range logic (day_number, presets, with_range / turned / right_month, click_day, shown_range,
  range_text).

  3fb03e2b0 the DOM + handlers (calendar_look merges both date_picker_looks per part; RangeShared /
  RangeDayData / PresetData payloads; days_around + repaint in place; report) + flat/flora
  `date_range_picker_skin` (theme APPENDS `// ==== date_range_picker ====`).

## NEXT (exact)
4. DateRangePicker tests (`mod dom_tests` at the end of date_range_picker.rs) + `pub(crate) mod fixtures
   { sample() }` (March 2026 left, today 2026-03-04, Monday start, range 4-10 Mar, named "Report period"):
   two months with 31 + 30 day cells, one Tab stop (4 Mar), presets column with 6 buttons, summary text,
   click on a day via rv::fire -> event Anchored then Picked (log through on_event), MouseEnter preview writes
   SetNodeStyle for every day, Escape after anchoring -> Cancelled, PageDown -> Navigated (view.month 4),
   preset click -> Preset with the span, follows the app theme, invariants; then manifest (INPUTS:
   "date_range_picker"). OLD notes for reference: the DOM (presets column of PushButton <p>s; two calendars = date_picker_look parts merged per part with
   theme_blocks::part_of over [flat::date_picker_look(), flora::date_picker_look()] when unpinned; header with
   ‹ on the left month / › on the right via header_nav_button; build_weekday_row_from; day_grid with own day
   cells (payload {date, shared}), click / MouseEnter preview / keys; summary line), handlers (restyle both grids
   in place: endpoints faces.selected, between washed(other), today ringed), theme appends
   `date_range_picker_skin` (root, presets, preset, summary), tests, manifest (INPUTS).
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
