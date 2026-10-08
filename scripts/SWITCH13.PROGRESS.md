# SWITCH13 - why some toggle switches lag (settings pages) while AzWidgets' are smooth

Worktree branch `worktree-agent-a74fe8ae45a4fc609`, fast-forwarded to cbd0ae0a8 (the lead's HEAD).

## DONE
- Read: switch.rs (knob slides by `transform`, track fades by `background`, both declared tweens; the
  click handler writes both imperatively and returns the app's `on_toggle` result), appkit ui.rs
  (`set_value` -> `save_settings` -> file thread -> `on_settings_saved` returns `RefreshDom`),
  AzCalc / AzWidgets toggle handlers (both return `RefreshDom`).

## IN PROGRESS
- Engine read: `begin_reconciliation` CSS diff vs an in-flight transition, `tick_animations` GPU path,
  `regenerate_layout` pre-cascade skip, thread write-back polling.
- Waiting for the lead's rebuild (AzWidgets, AzCalculator, ...) to measure.

## NEXT
- Measure AzWidgets switch vs AzCalc settings switch (frame report, AZ_PROFILE=cpu, sample).

## Open questions
- Which settings page did the user see? Switch widgets in settings: AzCalc, AzClock, AzKeys, AzNews
  (AzMail / AzPlayer options use check boxes / Media Center rows, no Switch).
