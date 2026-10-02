# CAL3 progress (AzCalendar on the Office scaffold, like Outlook 2010's calendar)

Branch `wt/cal3` from `16d19442c`. Worktree
`/Users/fschutt/Development/azul/.claude/worktrees/agent-a0f55d545305ff62c`. Nothing is compiled here
(house rule); the parent compiles. Brief: scratchpad `CAL3_go.md`, rules `wave4_common.md` (incl. the
outage-resilience section: commit every unit, keep this file exact).

Scratchpad helpers (`<scratchpad>/cal3/`): `api.py Type...` (generated binding signatures, from a copy of
`target/codegen/dll_api_external.rs`), `mod.py Name...` (which `azul::<module>` exports a name),
`fmtcheck.py [--write] FILE...` (rustfmt parse check), `methods.py FILE...` (method names vs the
bindings), `unused.py FILE...` (unused imports, heuristic), `red.py` / `stub_lib.py` (RED stubs).

## DONE (commit hashes, oldest first)

- `22e3f8f8e` plan
- model RED `4392bc17e` / GREEN `c1cf234de`: rrule.rs, ics.rs, event.rs v3, editor.rs, calendars.rs,
  views.rs, settings value/line/flag
- args + sample RED `734c016ea` / GREEN `02381effc`; tasks RED `ee5567a85` / GREEN `bf2451e64`;
  event::remove RED `0d49138d0` / GREEN `25bd33c74`; due reminders RED `ebd4c5b05` / GREEN `1e8575201`;
  editor repeat segments RED `920ef751b` / GREEN `bdf6897e4`
- UI (wip commits, compile as a whole only now): lib.rs `a6c872201` `49abda592`, timegrid.rs `35ebe5540`
  `4a9ae2583`, views_ui.rs `569746f2b`, editor_ui.rs `dd5efd03c` `e1f6afb29`, chrome.rs `887bc8303`
  `302fc8196` `11680bf24`, meeting::sibling_program `4733a3364`, shared picked()/typed() `edd864289`,
  imports `6a8be31c2`
- CAL2 scripts on the new UI: week_interactions `000e21e7f`, offline_links `dabf2ca6c`,
  mint-and-join `4bb53188c`

## IN PROGRESS

- nothing half-done

## DONE since (commit hashes)

- `scripts/azcalendar_e2e.py` `05356abaa`; calendar colour contrast guard `304c43b25`
- DatePicker lit range (engine widget): RED `7af90d031`, GREEN `ff96d1b81`; the app's navigator uses it
  `59804ab76`; duplicate fn name fix `ecc63a794`

## NEXT (in order, precise)

1. Review pass over the UI files for compile errors (names, borrows, types) - chrome.rs read up to
   the backstage builder; continue with its pages and callbacks, then editor_ui.rs, views_ui.rs,
   timegrid.rs, lib.rs. Commit each fix.
2. Report `scripts/CAL3_2026_10_01.md` (commits, api.json list: DatePicker.range_start / range_end /
   set_range / with_range; least-sure spots; test commands; what is left). Commit.

## Decisions (unattended run)

- Event files: a plain event is still written as version 2 byte for byte; any editor field makes it
  version 3 (an older AzCalendar leaves such a file alone instead of dropping fields). Storage stays CAL2's
  direct atomic writes (brief: "Event files stay as they are"); moving them onto a Thread + LocalDrive is
  listed as left.
- Calendars are files too: `calendars/<uuid>.json`, the default calendar `calendars/default.json` (only once
  renamed / recoloured). Hidden calendars are a device setting (`hidden_calendars=` in settings.txt).
  Tasks: `tasks/default/<uuid>.json`.
- .ics export writes floating times (AzCalendar's model is wall-clock); import converts UTC and TZID times
  (through the file's VTIMEZONE) into the reader's zone. Re-import updates by iCalendar UID (ours:
  `<id>@azcalendar`).
- One editor window at a time (the layout callback cannot tell two editor windows apart); a second request
  while one is open says so in the main window.
- The editor's repeat is picked on Segmented rows (scriptable), not a DropDown (a native menu a headless
  script cannot drive). Reminder and calendar are DropDowns.
- The navigation pane is a rail of 252 px (56 folded), not a split ratio (the date navigator needs its
  width).
- Headless child windows are not laid out / pumped in the base (MAIL2 owns that engine fix on `wt/mail2`);
  the E2E's editor stage addresses the window by `window_id` and reports BLOCKED when the debug server
  cannot reach it. The CAL2 scripts make their events in the week's popover (DOM 0) instead of the old
  side sheet.

## Open questions

- (none)
