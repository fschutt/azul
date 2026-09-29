# E1 - E2E tooling follow-ups - PROGRESS

Branch `wt/e1-e2e-tooling`, based on `0a326afe5`. Nothing compiled (house rule).

## Audit (before any change)

| # | item | verdict | evidence |
|---|---|---|---|
| 1 | per-platform scenarios (`only_on` gate, SKIP verdict, hotkey scenario split, docs) | STILL OPEN | `E2eTest` (`layout/src/e2e/full.rs` ~5027) has no platform field; `render_report` (`report.rs`) and the AZ_E2E printer (`dll/.../shell2/run.rs` ~470, a TWIN of `render_report`) know PASS/FAIL/XFAIL/XPASS only; `examples/azul-widgets/e2e/global_hotkey.json` spells `Ctrl+Alt+K` while a Mac host declares `Cmd+Shift+K` (`examples/azul-widgets/src/hotkeys.rs:47`); `doc/guide/en/debugging/e2e-testing.md` "File and step shape" has no gate (nor `expect`) |
| 2 | notification E2E op + `assert_notification` payload | STILL OPEN | no `DebugEvent` variant queues a `NotificationEvent`; `eval_assert_notification` (`full.rs` ~7058) accepts `id/title/body/action/withdrawn/count`, no `payload`; F2 report section 7 "E2E" |
| 3 | `get_selection_state` drops the affinity | DONE (selection-leftovers) | commits `034da1190` (helper), `e67e74dc8` (RED `selection_state_tests::a_select_all_range_reports_the_byte_after_its_last_character`, `full.rs` ~20578), `c81877d74` (fix: byte offsets affinity-resolved + `cursor_affinity`/`start_affinity`/`end_affinity`). RESIDUAL (report of that agent): `get_cursor_state.position` is still the raw `start_byte_in_run` (`full.rs` ~17786), affinity only beside it |
| 4 | headless `step()` ignores Scroll / TextInput | DONE (selection-leftovers) | `0813655e1` fix, tests `a_stepped_text_input_types_into_the_focused_field` / `a_stepped_wheel_scrolls_the_box_under_the_pointer` (`dll/src/desktop/shell2/headless/mod.rs` ~9159 / ~9221), both ancestors of the base |
| 5 | runner has no `ScrollFocusedContainer` arm | STILL OPEN | `layout/src/e2e/runner.rs` ~4058 comment + `_ => DoNothing`; dll arm `dll/src/desktop/shell2/common/event.rs` ~12412 re-implements the magnitude math `LayoutWindow::scroll_container_by_keyboard` (`layout/src/window.rs` ~10890) already has (TWIN) |
| 6 | runner never fires `Dismissed` | STILL OPEN | `Runner::dismiss_popups_on_escape` (`runner.rs` ~832) calls `transient_windows.dismiss` (no open window in the runner, so `LayoutWindow::dismiss_transient_window` would return before queueing) and fires nothing; the runner never drains `pending_lifecycle_events` |
| 7 | `gene2e.rs` OP_POLICY rows for new ops | follows 2 | `doc/src/gene2e.rs` `OP_POLICY` ~382; test ~3382 fails on an unclassified op |

## DONE

- `53ebc1803` audit
- item 1: `61799c708` RED (tooling_tests.rs, 7 tests), `b3ebe4615` fix (gate, SKIP tally,
  run.rs uses `render_report` + `load_e2e_tests`, loader reads arrays, CI sed, debugger icon),
  `c562b79f8` scenario split + guide, `841921086` RED (`parse_summary` stub),
  `b89b1ecc5` fix (dispatcher sums the children's tallies; mobile reader uses `parse_summary`)

## IN PROGRESS

- item 2 RED

## NEXT

2. item 2 + 7: RED, op + payload + OP_POLICY row + docs + demo scenario
3. item 5: RED, `LayoutWindow::scroll_focused_container_by_keyboard` used by runner and dll
4. item 6: RED, runner fires `Dismissed`
5. item 3 residual (`get_cursor_state.position`) if consumers allow
6. report

## Open questions

- none yet
