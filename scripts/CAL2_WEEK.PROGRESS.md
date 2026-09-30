# CAL2_WEEK progress (AzCalendar week interactions)

Branch `wt/cal2-week-interactions`, base `8812e832c`. Nothing is compiled here (house rule).

## DONE
- `4e6f11855` RED: week.rs tests (whole day, y <-> time, click / drag ranges, press on an event,
  zoom: limits, wheel, pinch, anchor; first hour shown; draft label), layout pin test
  `layout/tests/a_scroll_area_under_a_fixed_header_reaches_its_whole_content.rs` (+ all.rs),
  E2E `examples/azul-calendar/scripts/week_interactions.py`.
- `1d4736f8b` feat: week.rs math, lib.rs scroll area / zoom / click + drag drafts with a
  `<transient-window>` popover.
- `f225eee5b` E2E: independent stages, id-based draft checks, per-wait timeout (dry-run PASS against
  a stand-in; FAIL modes checked).
- `5b401249d` RED / `398033831` feat: a saved event out of view scrolls the week to it (keeps
  mint-and-join's "Join meeting" click on screen).
- Report `scripts/CAL2_WEEK_2026_09_30.md`.

## IN PROGRESS
- nothing

## NEXT
- Parent: compile, run the suites and the E2E (commands in the report).

## Findings
- Why the day was not reachable: the view only ever held 08:00 - 20:00, nothing in the window
  scrolled, and the flex chain had no `min-height: 0`. App cause, not engine (the layout pin test
  asks the engine for exactly the new structure; expected GREEN).
- Headless E2E limit: a `<transient-window>` popup is a separate window that the headless run loop
  never runs; keys the parent receives while it is open are forwarded to it and spent. Enter /
  Escape cannot reach the popover headless; the E2E drives the popover's nodes in DOM 0 instead.
  Engine follow-up proposed in the report.

## Open questions
- none
