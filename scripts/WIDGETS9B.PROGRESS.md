# WIDGETS9B progress - Gauge, DateRangePicker, MoneyInput, ReferencePicker

Branch `wt/widgets9b` from `e537ddbe2` (wave 8 integrated). Never compile (house rules).
Brief: scripts/waves/wave9/PLAN.md "WIDGETS9B"; spec ../azul-apps/planning/foundation/05-widget-backlog.md
(ReferencePicker "extend ComboBox: type-to-filter, arrow/Enter, async search callback, id->label, debounce,
create-new row"; Gauge "radial ring/arc value with centre label"; DateRangePicker "two-month range selection with
presets"; MoneyInput "fixed-precision decimal input with currency suffix, locale grouping").

## DONE
- 6719c21b2 progress file
- MoneyInput: 8c30cb223 types/stubs, 24196ba32 RED pure tests, b39859f4e GREEN, d873279dc widget + hooks,
  176b6ff6d flat/flora skins, 9abb473ce DOM tests + manifest (INPUTS: "money_input", "money_input (empty, en)")
- 6992fc8c6 theme_blocks::{skins_of, structure_skin, part_of} (ChartLook::part delegates)

## IN PROGRESS
- Gauge (layout/src/widgets/gauge.rs)

## NEXT (order)
3. Gauge (gauge.rs): geometry RED/GREEN (angles, fractions, band of value) -> DOM (chart's `wedge_ring` +
   SvgNodeData) -> a11y (Indicator role = accesskit Meter) -> theme appends -> manifest (STATUS group).
4. DateRangePicker (date_range_picker.rs): range logic RED/GREEN (click sequence, preview, presets, keys) ->
   DOM on DatePicker's pieces (refactor: `date_picker_look()` in both themes, a generic day-grid builder,
   pub(crate) helpers) -> handlers (click / hover preview in place / keys / presets / nav) -> manifest.
5. ComboBox extensions + ReferencePicker (reference_picker.rs) -> manifest.
6. Report scripts/WIDGETS9B_2026_10_03.md (api.json list, least-sure spots, test commands).

## Decisions (unattended)
- D1 App-owned state, as DataTable / Timeline / DatePicker month nav: every widget reports an event carrying the
  NEXT state; the app stores it and rebuilds (`Update::RefreshDom`). In-place restyles only where the change keeps
  the node set (the range preview, the active option).
- D2 MoneyInput wraps TextInput like NumberInput (no twin of the field); amounts are i64 MINOR units, never floats.
  Locale = `MoneyLocale { decimal_separator, group_separator (u32 code points), symbol_position, symbol_spaced }`
  with `from_tag` (a small table: en / de / fr / ch / nl / pt-BR ...) and `from_sample` (separators read off a
  number the app's localizer formatted, e.g. ICU `format_decimal(123456789, 2)`). Currency =
  `MoneyCurrency { code, symbol, minor_digits }`, `from_code` (ISO 4217 minor digits for the common codes).
  The currency shows as an addon box beside the field (not inside the TextInput's border).
  Keystroke rule: a complete or still-completable text is accepted (amount None while incomplete); garbage, a
  digit past the currency's minor digits, a '-' when negatives are off, an i64 overflow are vetoed. Min / max
  are reported (error BelowMin / AboveMax) but never veto typing. Focus lost reformats (grouping) through
  `TextInput::set_text_in` and reports `on_commit`.
  Lenient decimal: a lone '.' or ',' that cannot be grouping (1..=minor digits after it, not 3) is the decimal
  point (the numeric keypad's key in either locale).
- D3 Gauge draws with the chart's vector path (chart::wedge_ring + SvgNodeData::Path in a ViewBox), no second
  renderer; band / value colours are `ChartColor` light/dark pairs; role Indicator (= accesskit Meter) via
  `MeterAriaInfo`.

## Open questions
- (none yet)
