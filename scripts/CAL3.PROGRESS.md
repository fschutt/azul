# CAL3 progress (AzCalendar on the Office scaffold, like Outlook 2010's calendar)

Branch `wt/cal3` from `16d19442c`. Worktree
`/Users/fschutt/Development/azul/.claude/worktrees/agent-a0f55d545305ff62c`. Nothing is compiled here
(house rule); the parent compiles. Brief: scratchpad `CAL3_go.md`, rules `wave4_common.md` (incl. the
outage-resilience section: commit every unit, keep this file exact).

Scratchpad helpers (`<scratchpad>/cal3/`): `api.py Type...` (generated binding signatures, from a copy of
`target/codegen/dll_api_external.rs`), `mod.py Name...` (which `azul::<module>` exports a name),
`fmtcheck.py [--write] FILE...` (rustfmt parse check), `red.py` / `stub_lib.py` (RED stubs), `green/` (GREEN
copies of the model files as of the RED commit).

## DONE (commit hashes)

- `22e3f8f8e` progress file and plan
- `4392bc17e` test(azcalendar) RED: rrule.rs, ics.rs, event.rs v3 fields, editor.rs, calendars.rs,
  views.rs, settings value/line/flag (todo!() bodies; lib.rs declares the modules)
- (this commit) feat(azcalendar) GREEN: the same model, implemented

## IN PROGRESS

- nothing half-done

## NEXT (in order, precise)

1. UI, file by file, each committed on its own:
   a. `src/args.rs` (`--screen`, `--theme flat|flora`, `--mode light|dark`, `--sample`, `--date`), with
      unit tests; `src/sample.rs` (first-run sample events).
   b. `src/chrome.rs`: title row, ribbon (FILE / HOME / VIEW), navigation pane (inline DatePicker month +
      "My calendars" checkboxes with swatches + module switcher), status bar, To-Do bar, backstage pages
      (Info, Open & Export (.ics import path + Browse, export), Print (later), Calendars, Options on
      ShellSettingsLayout (meeting server), About).
   c. `src/timegrid.rs`: CAL2's week grid generalised to Day / Work Week / Week (ids `#week-scroll`,
      `#week-grid`, `#day-<i>`, `#draft*` kept), all-day row, today line.
   d. `src/month_ui.rs`: month (with "+N more"), schedule, list (agenda) views.
   e. `src/editor_ui.rs`: the event editor window (`info.create_window`, `window_id` "azcalendar-editor").
   f. `src/lib.rs`: CalState grows (view, anchor, calendars, hidden, editor, backstage, ...), the layout on
      OfficeShell + ShellThemeScope, callbacks; CAL2's sync / zoom / popover code kept.
2. DatePicker range highlight (engine widget, RED first) - only if time allows; else note.
3. `scripts/azcalendar_e2e.py` (switch views, editor window create, weekly repeat on next week, import .ics).
4. Report `scripts/CAL3_2026_10_01.md`.

## Decisions (unattended run)

- Event files: a plain event is still written as version 2 byte for byte; any editor field makes it
  version 3 (an older AzCalendar leaves such a file alone instead of dropping fields). Storage stays CAL2's
  direct atomic writes (brief: "Event files stay as they are"); moving them onto a Thread + LocalDrive is
  listed as left.
- Calendars are files too: `calendars/<uuid>.json`, the default calendar `calendars/default.json` (only once
  renamed / recoloured). Hidden calendars are a device setting (`hidden_calendars=` in settings.txt).
- .ics export writes floating times (AzCalendar's model is wall-clock); import converts UTC and TZID times
  (through the file's VTIMEZONE) into the reader's zone.
- One editor window at a time (the layout callback cannot tell two editor windows apart); a second request
  while one is open says so in the main window.
- Headless child windows are not laid out / pumped in the base (MAIL2 owns that engine fix on `wt/mail2`); the
  E2E's editor stage addresses the window by `window_id` and reports BLOCKED when the debug server cannot
  reach it.

## Open questions

- (none)
