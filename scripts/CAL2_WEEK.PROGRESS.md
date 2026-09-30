# CAL2_WEEK progress (AzCalendar week interactions)

Branch `wt/cal2-week-interactions`, base `8812e832c`. Nothing is compiled here (house rule).

## DONE
- `4e6f11855` RED: week.rs tests (whole day, y <-> time, click / drag ranges, press on an event,
  zoom: limits, wheel, pinch, anchor; first hour shown; draft label), layout pin test
  `layout/tests/a_scroll_area_under_a_fixed_header_reaches_its_whole_content.rs` (+ all.rs),
  E2E `examples/azul-calendar/scripts/week_interactions.py`.

- `1d4736f8b` feat: week.rs math, lib.rs scroll area / zoom / click + drag drafts with a
  `<transient-window>` popover.

## IN PROGRESS
- Review pass over lib.rs against the generated Rust API (target/codegen/dll_api_external.rs).

## NEXT
- Report `scripts/CAL2_WEEK_2026_09_30.md`.

## Findings so far
- Why the day was not reachable: the view only ever held 08:00 - 20:00 (`week::FIRST_HOUR` /
  `END_HOUR`), and nothing in the window scrolls: no `overflow-y` anywhere, and the flex chain
  (body > main > week grid) has no `min-height: 0`, so a scroll box there would have grown to its
  content instead of overflowing. App cause, not engine (flex is taffy; the layout pin test asks the
  engine for exactly this structure).
- Headless E2E limit: a `<transient-window>` popup is a separate window; the parent does not lay
  its content out (`LayoutWindow::layout_transient_content`, scratch caches), the headless run loop
  never runs a pass for child windows (`HeadlessWindow::run` Phase 4 only polls Close), and keys the
  parent receives while a focus-taking popup is open are forwarded to that popup's mailbox and
  consumed (`forward_keys_to_popup`). So Enter cannot reach the popover headless; the E2E drives
  the popover's nodes in the parent's DOM (the transient subtree is part of DOM 0) instead.

## Open questions
- none
