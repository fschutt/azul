# M4 AzCalendar - progress

Branch `wt/m4-azcalendar` (base `8e9a0a683`). Report: `scripts/M4_AZCALENDAR_2026_09_29.md`.

## DONE
- (this commit) RED: crate `examples/azul-calendar` (package AzCalendar, lib `azcalendar` + bin), workspace
  member, Cargo.lock entry; `event.rs` / `week.rs` / `meeting.rs` signatures with `todo!()` and their tests;
  AzMeet's `rooms.rs` compiled in as `meet_rooms` (`#[path]`).

## IN PROGRESS
- pure logic: event file format, file names, store; week math and overlap lanes; the Worker's answer.

## NEXT
1. feat: implement event.rs / week.rs / meeting.rs (type-check with scratchpad `m4/check_pure.sh`).
2. RED: `examples/azul-calendar/scripts/mint-and-join.mjs` (headless e2e).
3. feat: lib.rs UI (week view, New event form, Save -> mint -> file, Join meeting).
4. report.

## Open questions
- The Worker's rooms live 24 h (extended by announcements); an event next week keeps a link to a room the
  server has forgotten by then. Needs a Worker change (a room minted for a start time) - not in this task.
