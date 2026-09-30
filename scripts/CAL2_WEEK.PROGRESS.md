# CAL2_WEEK progress (AzCalendar week interactions)

Branch `wt/cal2-week-interactions`, base `8812e832c`. Nothing is compiled here (house rule); the
E2E may be run against the parent's build (coordinator, 2026-09-30).

## DONE (round 1)
- `4e6f11855` RED / `1d4736f8b` feat: whole-day scroll area, zoom, click / drag drafts with a
  `<transient-window>` popover; layout pin test; E2E script.
- `f225eee5b` E2E stages; `5b401249d` RED / `398033831` feat: a saved event out of view is revealed.
- `24a0218e8` report `scripts/CAL2_WEEK_2026_09_30.md` (round 1).

## DONE (round 2: the parent ran the E2E, 7/7 failed)
- `de3b31ef6` RED / `5feb67d2b` fix(engine):
  1. `is_layout_equivalent` ignored `Dom::with_css` sheets (they live in the cascade's
     `retained_author_css`, not `NodeData::style`): a with_css-only rebuild (the zoom) kept the
     old layout. The pinch DID reach the app (its scroll_to was in the log).
  2. `dispatch_accessibility_events` skipped the Default action's `Click` filter: a screen
     reader's press (and the E2E `accessibility_action default` on Save / Cancel) ran nothing.
- `58680e715` E2E: `wait_frame` while waiting, wheel from the middle with the right sign, settle
  after the wheel, pointer before `pinch`, --only / --skip. Against the parent's current build:
  wheel + outside PASS; drag reaches the right draft; saves need the engine fix above.
- `260701fb4` RED / `aa9eabfdc` fix(gesture): DetectedPinch is CUMULATIVE since the gesture began,
  new last field `began` (api.json!); macOS accumulates magnify deltas over the NSEvent phase
  (`trackpad_magnify`), touch `began` edge (`note_pinch_dispatched`), iOS / Android / Wayland /
  X11 / Windows fixed; map + AzCalendar zoom by successive cumulative ratios.

## IN PROGRESS
- User requests (mid-turn): clip event titles; drop "No meeting server is set"; default meeting
  server + Settings menu; offline-first links (client-generated room ids, synced when the server
  is reachable). The Worker lives in `../azul-apps` (a patch file, since this agent cannot commit
  there).

## NEXT
- Report round 2 in `scripts/CAL2_WEEK_2026_09_30.md`.

## Open questions
- none
