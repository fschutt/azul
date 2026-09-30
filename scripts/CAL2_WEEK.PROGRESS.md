# CAL2_WEEK progress (AzCalendar week interactions)

Branch `wt/cal2-week-interactions`, base `8812e832c`. Nothing is compiled here (house rule); the
E2E may be run against the parent's build (coordinator, 2026-09-30).

## DONE (round 1)
- `4e6f11855` RED / `1d4736f8b` feat: whole-day scroll area, zoom, click / drag drafts with a
  `<transient-window>` popover; layout pin test; E2E script.
- `f225eee5b` E2E stages; `5b401249d` RED / `398033831` feat: a saved event out of view is revealed.
- `24a0218e8` report `scripts/CAL2_WEEK_2026_09_30.md` (round 1).

## DONE (round 2: the parent ran the E2E, 7/7 failed)
- `de3b31ef6` RED / `5feb67d2b` fix(engine): `is_layout_equivalent` compares `with_css` sheets
  (`retained_author_css`); `dispatch_accessibility_events` runs the Default action's Click.
- `58680e715` E2E timing / direction / pinch pointer fixes. Against the parent's current build:
  wheel + outside PASS; the rest need `5feb67d2b`.
- `260701fb4` RED / `aa9eabfdc` fix(gesture): DetectedPinch is CUMULATIVE since the gesture began,
  new last field `began` (api.json!); every shell; map + AzCalendar zoom by successive ratios.
- `f51225208` RED / `638cf212e` feat: offline-first AzMeet links (made in the app, `pending` in the
  file, registered by POST /rooms {room, ...} when the server answers, retried every
  AZCAL_SYNC_SECONDS); Settings > Meeting server / Sync now in the menu bar; no "No meeting server"
  line; clipped titles. Worker patches `scripts/cal2/meet-0001` (RED) / `meet-0002` (GREEN) for
  azul-apps (node --test 69/69).
- `1fdb476d1` RED / `f3beb39f2` feat: settings.rs owns settings.txt (merging key=value lines);
  the week's zoom is saved there and read at start; shared test_dir::TempDir.
- Report round 2 in `scripts/CAL2_WEEK_2026_09_30.md` (commit list for the parent at the top).

## IN PROGRESS
- none

## NEXT
- Parent: regenerate bindings (DetectedPinch.began), apply the meet patches to azul-apps, build,
  run week_interactions.py, offline_links.py, mint-and-join.mjs.

## Open questions
- none
