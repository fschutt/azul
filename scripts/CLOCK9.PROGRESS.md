# CLOCK9 - AzClock (wave 9) - progress

Branch: wt/clock9 (base e537ddbe2). Brief: scripts/waves/wave9/PLAN.md "CLOCK9", planning
../azul-apps/planning/core/clock.md. Report: scripts/CLOCK9_2026_10_03.md.

## DONE
- d2106068f progress file; 6c3d3daa2 plan
- c4908aa79 RED crate skeleton + registration + alarm/tone tests
- 7cb992a6c GREEN alarm.rs (occurrences, DST, due/snooze/arm, labels) + tone.rs

## IN PROGRESS
- A2 RED timer.rs + stopwatch.rs + fmt.rs, then world.rs, schedule.rs

## NEXT (plan, in order)
- A1 skeleton + register (root Cargo.toml members, scripts/workspace_test_members.txt, rust.yml dll_tests step)
- A2 RED model tests: alarm.rs (next occurrence: DST gap / overlap, weekly across a weekend, one-shot,
  RRULE count / until, snooze), timer.rs (state machine), stopwatch.rs (laps), world.rs (zones, offsets,
  day/night), schedule.rs (which OS notifications to schedule / withdraw)
- A3 GREEN model
- E1 RED engine: Notification::with_deliver_at, layout ScheduledNotifications queue, wire helpers
  (apple trigger interval, windows delivery time), recorder keeps deliver_at
  -> layout/tests/native_notifications.rs (APPEND)
- E2 GREEN engine: core field, layout queue + wire, dll service (Apple UNTimeIntervalNotificationTrigger,
  Windows ScheduledToastNotification + AddToSchedule, in-process queue elsewhere), withdraw cancels
- A4 store (files in the data tree), sample data
- A5 UI (UtilityShell, modes, analog face, lists, editor modal w/ TimePicker + DateRepeatPicker,
  ringing overlay + tone), A6 notifications wiring, A7 settings/about/shortcuts/args
- A8 scripts/azclock_e2e.py; A9 report

## Decisions
- Time zones: chrono + chrono-tz 0.10 (both already in Cargo.lock: chrono-tz via ironcalc_base) and
  iana-time-zone 0.1 (in the lock, through chrono) for the local zone's name. The plan named jiff
  (not in the lock) - the wave rule "prefer what is in Cargo.lock" wins.
- DST rule (RFC 5545 3.3.5 / Temporal "compatible"): an alarm time in a spring-forward gap rings at
  the same wall time shifted by the gap (02:30 -> 03:30 CEST); in a fall-back overlap it rings once,
  at the earlier instant.
- Scheduling API shape: ONE new optional field on Notification, `deliver_at` (unix ms, UTC), the
  `NotificationSchedule::At` of scripts/NOTIFICATIONS_RESEARCH_2026_09_28.md section 6.5. The app
  expands its repeat rules into the next occurrences itself (DST-correct, tz-aware) and tops them up on
  every start: every backend can hold one-shot instants (Windows cannot repeat at all, UN calendar
  triggers cannot express every-N or DST shifts). Rule-based OS repeats are a later step.

## Open questions
- (none yet)
