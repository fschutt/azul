# L1_WIDGET_LEFTOVERS progress

Branch `wt/l1-widget-leftovers`, base `d9ce25179`. Nothing compiled (house rule).

## DONE
- 1. Time picker spin button: RED 8577516c1, fix e1756d917.
- 3. Twins `window_is_dark` / `renders_dark`: ALREADY GONE in the base (57cdc850e pagination,
  ccb4c8530 segmented, 3b62aeff7 stepper, 11dd02237 date_picker - "the last twin"); the one mode
  decision is `CallbackInfo::get_resolved_mode`. Nothing to do.
- 4. `get_property_slow` tiers folded into one loop: 90d420671 (no behaviour change).

## IN PROGRESS
- 2. Combobox: typing clears the active option; Tab / blur closes the list.

## NEXT
- Report `scripts/L1_WIDGET_LEFTOVERS_2026_09_29.md`.

## Open questions
- PageUp / PageDown steps chosen: 2 hours, 15 minutes (react-aria's). Say if other steps are wanted.
- `:backdrop` inline tier reads the declared static view with no theme rank (every other state
  reads the resolved, ranked view). Kept as is in the fold; unify?
