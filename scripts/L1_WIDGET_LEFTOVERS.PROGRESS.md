# L1_WIDGET_LEFTOVERS progress

Branch `wt/l1-widget-leftovers`, base `d9ce25179`. Nothing compiled (house rule).

## DONE
- 1. Time picker spin button: RED 8577516c1, fix e1756d917.
- 2. Combobox typing clears the active option / blur + Tab close the list: RED 83b01fc61, fix bc5cc8f9b.
- 3. Twins `window_is_dark` / `renders_dark`: ALREADY GONE in the base (57cdc850e pagination,
  ccb4c8530 segmented, 3b62aeff7 stepper, 11dd02237 date_picker - "the last twin"); the one mode
  decision is `CallbackInfo::get_resolved_mode`. Nothing to do.
- 4. `get_property_slow` tiers folded into one loop: 90d420671 (no behaviour change).
- Report `scripts/L1_WIDGET_LEFTOVERS_2026_09_29.md`.

## IN PROGRESS
- (none)

## NEXT
- Parent: compile, run the commands in the report section 6, api.json autofix for
  `ComboBoxStateWrapper.active_option_cleared`.

## Open questions
- PageUp / PageDown steps chosen: 2 hours, 15 minutes (react-aria's). Say if other steps are wanted.
- `:backdrop` inline tier reads the declared static view with no theme rank (every other state
  reads the resolved, ranked view). Kept as is in the fold; unify?
- Combobox: the popup's stale highlight after typing needs shell work (report section 8.2).
