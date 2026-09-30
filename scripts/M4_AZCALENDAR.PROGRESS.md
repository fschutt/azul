# M4 AzCalendar - progress

Branch `wt/m4-azcalendar` (base `8e9a0a683`). Report: `scripts/M4_AZCALENDAR_2026_09_29.md`.

## DONE
- `0cc765f17` RED: crate `examples/azul-calendar` (package AzCalendar, lib `azcalendar` + bin), workspace
  member, Cargo.lock entry; `event.rs` / `week.rs` / `meeting.rs` signatures with `todo!()` and their tests;
  AzMeet's `rooms.rs` compiled in as `meet_rooms` (`#[path]`).
- `960137a1d` feat: event.rs / week.rs / meeting.rs implemented (type-checked lib + tests with rustc
  --emit=metadata, scratchpad `m4/check_pure.sh`; not run).
- `8d8292e42` RED: `examples/azul-calendar/scripts/mint-and-join.mjs` (headless e2e).
- `4bc9ec077` feat: lib.rs UI (week view, New event form, Save -> mint -> file, Join meeting); the whole lib
  type-checks (lib + tests) against target/release's link-dynamic azul rmeta (scratchpad `m4/check_lib.sh`).

## IN PROGRESS
- dry run of mint-and-join.mjs against Node stand-ins for AzCalendar / AzMeet (scratchpad `m4/fake-*.mjs`).

## NEXT
4. report.

## Open questions
- The Worker's rooms live 24 h (extended by announcements); an event next week keeps a link to a room the
  server has forgotten by then. Needs a Worker change (a room minted for a start time) - not in this task.
