# CAL3 progress (AzCalendar on the Office scaffold, like Outlook 2010's calendar)

Branch `wt/cal3` from `16d19442c`. Worktree
`/Users/fschutt/Development/azul/.claude/worktrees/agent-a0f55d545305ff62c`. Nothing is compiled here
(house rule); the parent compiles. Brief: scratchpad `CAL3_go.md`, rules `wave4_common.md`.

## DONE (commit hashes)

- (none yet)

## IN PROGRESS

- reading CAL2 / FB1 / SHELLS / MAILWIDGETS, the app, the generated bindings (copied to the scratchpad
  `cal3/` with `api.py`, a lookup helper)

## NEXT (in order)

1. Model, RED then GREEN (pure Rust in the app crate, unit tests):
   `rrule.rs` (RRULE subset: FREQ DAILY/WEEKLY/MONTHLY/YEARLY, INTERVAL, COUNT, UNTIL, BYDAY incl.
   nth weekday, BYMONTHDAY, BYMONTH; EXDATE; expansion into a date range),
   `ics.rs` (unfold / fold at 75 octets, escaping, VEVENT <-> Event, DTSTART/DTEND with VALUE=DATE,
   UTC, TZID resolved through the file's VTIMEZONE, RRULE, EXDATE), `event.rs` grows the editor's
   fields (all day, location, notes, attendees, reminder, calendar, repeat, exceptions) - a plain
   event is still written as version 2, byte for byte; `calendars.rs` (calendars + colours),
   `views.rs` (day / work week / week / month with "+N more" / schedule / agenda).
2. UI: OfficeShell window (ribbon File/Home/View, backstage Info / Open & Import / Export / Print
   (later) / Options / About, navigation pane with the inline DatePicker + My calendars + the module
   switcher, the views, To-Do bar, status bar), the event editor as a second window.
3. Light / dark, flat / flora.
4. `scripts/azcalendar_e2e.py`.
5. Report `scripts/CAL3_2026_10_01.md`.

## Decisions (unattended run)

- (filled in as they are made)

## Open questions

- (none yet)
