# CLOCK9 - AzClock (wave 9) - progress

Branch: wt/clock9 (base e537ddbe2). Brief: scripts/waves/wave9/PLAN.md "CLOCK9", planning
../azul-apps/planning/core/clock.md. Report: scripts/CLOCK9_2026_10_03.md.

## DONE
- d2106068f progress file; 6c3d3daa2 plan
- c4908aa79 RED crate skeleton + registration + alarm/tone tests
- 7cb992a6c GREEN alarm.rs (occurrences, DST, due/snooze/arm, labels) + tone.rs
- f9f8e6149 RED / b0a6d4ee7 GREEN timer.rs, stopwatch.rs, fmt.rs
- de56163d4 RED / 04941615e GREEN world.rs (chrono-tz zones, rows, search)
- 9e6f356ec RED / cf8ab9ae9 GREEN schedule.rs (OS notification plan, diff, payloads)
- 999c75f76 RED engine tests (layout/tests/native_notifications.rs mod scheduled; e2e tooling_tests)
- ef410cccd GREEN core Notification::deliver_at + with_deliver_at, layout ScheduledNotifications + wire
  (delivery_delay_ms, apple_trigger_interval, windows_datetime), assert_notification scheduled/deliver_at
- a87ba8425 dll: Apple UNTimeIntervalNotificationTrigger, Windows ScheduledToastNotification, held queue +
  deadline thread elsewhere, withdraw cancels
- ae51363d3 RED / 38c1661b6 GREEN store.rs (keys clock/alarms/<id>.json etc, load_jobs, read_loaded, sample)

## IN PROGRESS
- A5 the window: examples/azul-clock/src/ui.rs (NOT written yet). lib.rs `start()` is still an empty stub.

## NEXT (exact)
1. Write examples/azul-clock/src/ui.rs (model it on examples/azul-calculator/src/ui.rs: kit::create_kit,
   kit::window_options, kit::app_config, UtilityShell + kit::title_row, ShellThemeScope::body()):
   - state struct ClockApp { kit, screen (World/Alarms/Timer/Stopwatch), alarms, timers, world, stopwatch,
     queue: azul_pim::write_queue::WriteQueue, last_plan: Option<schedule::Plan>, editor: Option<draft>,
     ringing: Vec<Ring>, sink: Option<AudioSink>, notice };
   - layout: modes row (Segmented) + "+" button + Settings; screens world / alarms / timer / stopwatch;
     alarm editor Modal (TimePicker, DateRepeatPicker, TextInput label, DropDown sound, NumberInput snooze,
     Delete / Cancel / Save); ringing Modal (Snooze / Dismiss); add-city Modal (search TextInput + result buttons)
   - 1 s Timer tick: alarm.due -> ring (tone via AudioSink::open + tone::pattern), timer.tick, refresh;
     fast 33 ms tick only while the stopwatch runs on screen: change_node_text on a marker (no relayout)
   - notifications: after every change schedule::plan + schedule::diff(last_plan) -> info.post_notification(
     Notification::create(id,title).with_body(..).with_payload(..).with_action("snooze",..).with_action(
     "dismiss",..).with_deliver_at(at_ms as u64)) / info.withdraw_notification(id); app-level handler via
     AppConfig::set_notification_handler(app_ref, on_notification)
   - writes: WriteQueue -> FileJob::Put/Delete via kit::spawn_file_jobs (tag TAG_SAVE), load via store::load_jobs
   - Linux / no OS scheduling: CloseRequested -> prevent_window_close + minimize while an alarm/timer is armed
2. Then lib.rs start() -> ui::start(); ids.rs names as needed.
3. scripts/azclock_e2e.py (model scripts/shells_e2e.py), then the report scripts/CLOCK9_2026_10_03.md
   (api.json list: Notification.deliver_at field (OptionU64, LAST field) + Notification::with_deliver_at(
   unix_ms: u64) "object.with_deliver_at(unix_ms)"; Cargo.lock gains the AzClock package; TODO(WIDGETS9B)
   Gauge for the timer ring (ProgressBar for now)).

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
