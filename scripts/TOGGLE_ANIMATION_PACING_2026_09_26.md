# Switch toggle: smooth on macOS, not on Wayland/X11 (2026-09-26)

Two agents, both read-only on the main checkout, no builds.

1. **Analysis** (read-only, no code): the cause, with file:line evidence.
2. **Fix** in worktree branch `wt/animation-pacing` (worktree
   `.claude/worktrees/agent-ab0ba1d8799a9fee8`, starts at 97f211f96). Four
   RED/fix pairs, **uncompiled**.

File shorthand: `ev` = dll/src/desktop/shell2/common/event.rs, `win` =
layout/src/window.rs, `x11` = dll/src/desktop/shell2/linux/x11/mod.rs, `wl` =
dll/src/desktop/shell2/linux/wayland/mod.rs, `mac` =
dll/src/desktop/shell2/macos/mod.rs, `run` = dll/src/desktop/shell2/run.rs.

## 1. Analysis: why it differs per platform

No compositor or damage bug. The difference is how often the CSS animation
driver (`CSS_ANIMATION_TIMER_ID`, 16 ms, `advance_css_animations_now`) runs.
macOS advances it once per 16 ms; X11 and Wayland on EVERY event-loop pass,
and each pass costs a relayout of the whole window.

- **macOS:** `process_timers_and_threads` runs only when an NSTimer fires
  (mac:1437, 2294); the draw path deliberately does not (mac:7704-7709);
  presents once per display refresh (mac:4790-4835, 8708-8732). Windows only on
  WM_TIMER (windows/mod.rs:6232).
- **Cause 1 (both Linux):** `tick_timers` returns every registered timer id
  whether due or not (win:8694-8706, pinned by a test at win:23292);
  `invoke_expired_timers` counts the CSS timer as fired just because it is in
  that list (ev:13306, 13347). Both Linux `poll_event`s call
  `check_timers_and_threads()` at the top of every iteration (x11:3015,
  wl:1198) on top of the call after a timerfd wake (x11:4987, wl:3299);
  Wayland calls `poll_event` again whenever events were dispatched (run:2211,
  wl:1298). Every pass rewrites the knob's margin-left and flags a relayout
  (win:11855-11861, 11911-11913); ev:12936-12953 relayouts the whole window,
  rebuilds the hit tester and requests a frame -> ~4-6 relayouts per
  compositor frame on Wayland. The capability-pump / long-press timer is gated
  the same way (ev:13287, 13382).
- **Cause 2 (X11):** each pass calls `request_redraw()` (x11:8898) which
  posts a synthetic Expose (x11:3195); the Expose handler renders immediately,
  bypassing the frame pacer (x11:5137-5173), and wakes the loop again.
- **Cause 3 (X11):** the first tick runs BEFORE the DOM rebuild the Switch's
  `RefreshDom` causes; the rebuild's duration lands in the second tick's dt;
  transitions have no dt cap (win:11802-11806) -> ~40% jump in one frame.
- **Cause 4 (both Linux):** dt truncated to whole milliseconds every pass
  (win:11660-11662, ev:4171-4176): a sub-ms pass advances nothing but resets
  the clock.
- **Ruled out:** clocks (same monotonic clock), timerfd wakeups, arming
  (shared code), Wayland frame-callback re-issue (wl:8581-8623), busy-buffer
  retry (wl:3022-3040), damage.

## 2. Fix branch `wt/animation-pacing` (UNCOMPILED)

```
888594a52 fix(animation): a DOM rebuild costs a glide at most one frame, not its duration
a4e3951ad test(animation): a DOM rebuild between two driver frames is not animation time
d32a856ee fix(timer): the CSS animation driver steps once per frame, not once per loop pass
1b35c7697 test(timer): the CSS animation driver steps once per timer period, not per pass
24db47e9b fix(animation): the animation clock steps in nanoseconds, not whole milliseconds
27081896f test(animation): sub-millisecond frames advance a glide by their real length
3299d3758 fix(animation): a transition step that moves nothing restyles nothing
05300e709 test(animation): a zero-length tick owes no relayout
```

Expected RED values (hand-derived):
- A `layout/tests/switch_animation.rs:255`: `!lw.take_transition_relayout()` is `true` after `tick_animations(0.0)`.
- B `switch_animation.rs:292`: ten 1.5 ms frames should give t = 0.1, today 0.0667.
- C `dll/src/desktop/shell2/headless/mod.rs:7850`: second pass at the same instant returns `true`. Companion test at :7888 already passes (pins a constraint).
- D `headless/mod.rs:7922`: `after - before <= 0.232` fails; a 60 ms rebuild moves the knob 0.111 -> 0.618.

Fixes: A win:11890 (a transition whose t did not move is skipped); B
`animation_step_at` (win:11691) in nanoseconds, used by `tick_animations_now`
and the driver (ev:4180); C `CSS_ANIMATION_FRAME_MS` + `css_animation_step_due`
(win:416, 11746) called at ev:4174 - step only if idle-reset, >= 8 ms since the
last step, or a later 16 ms period of the driver's own timer; D
`forget_animation_stall` (win:11712) at ev:4393/4419.

Departures from the analysis: the unconditional per-pass
`process_timers_and_threads` stays (it is the only regular caller of
`sync_window_state()` on Linux); `tick_timers` + its pinned test unchanged; X11
synthetic Expose left (harmless after C, safe follow-up).

Least sure to compile: `Instant::now().into_std_instant()` /
`Instant::from(std::time::Instant)` in switch_animation.rs (needs azul-core
`std`); a `matches!(c, IdOrClass::Class(s) if ...)` binding in the headless helper.

Live check on Linux: `AZ_LOG=warn,+window,+timer AZ_LOG_FILE=/tmp/az.log`,
toggle once; between `Created timerfd N for timer 8` and `Closed timerfd N`
expect ~9-10 `<- incremental_relayout` lines over 150 ms (<= 2 per 16 ms).

Side finding: `make_one_shot_pass_timer` (long-press marker) uses
`with_interval` instead of `with_delay`, so any timer pass can fire it before
the long-press threshold - on Linux on the pass right after mouse-down.

## 3. Follow-up on the same branch (rebased onto fix/input-bugs-2026-09-19, UNCOMPILED)

`git log --oneline fix/input-bugs-2026-09-19..wt/animation-pacing` (13 commits):
```
a0fae0a9c fix(x11): a timer frame and our own Expose wait for the frame pacer
df1032a74 fix(gesture): a press held perfectly still becomes a long press
f7ee67869 test(gesture): a motionless press fires LongPress once past its threshold
84a88ab73 fix(gesture): the long-press wake-up fires at its threshold, not on the next timer pass
57b407cfc test(gesture): a timer pass before the threshold does not spend the long-press wake-up
17e3d1cb8 fix(animation): a DOM rebuild costs a glide at most one frame, not its duration
e6631c568 test(animation): a DOM rebuild between two driver frames is not animation time
cb0de39a9 fix(timer): the CSS animation driver steps once per frame, not once per loop pass
90823b5a1 test(timer): the CSS animation driver steps once per timer period, not per pass
a288a7371 fix(animation): the animation clock steps in nanoseconds, not whole milliseconds
83b388c18 test(animation): sub-millisecond frames advance a glide by their real length
4e45b7e66 fix(animation): a transition step that moves nothing restyles nothing
ab44ffafa test(animation): a zero-length tick owes no relayout
```
- Long-press early fire confirmed: `make_one_shot_pass_timer` only set an
  interval, so the marker ran on the first timer pass of any kind; and
  `invoke_expired_timers` ran the long-press pass just because the marker was
  registered. Fix: capability_pump.rs:271 (delay + same interval); event.rs runs
  the long-press pass only if that timer actually ran. RED: timer passes at the
  same instant and 5 ms later on a frozen clock -> `armed_after_same_instant &&
  armed_after_5ms` expected false.
- Second defect: a MOTIONLESS hold was never a long press (`detect_long_press`
  measures first-to-last sample; a still hold has only the press sample -> 0 ms).
  Fix: `GestureAndDragManager::record_hold_sample` (gesture.rs), recorded just
  before the long-press pass. RED: real-time hold on a 30 ms threshold -> 0 instead of 1.
- X11 (single commit, no RED - needs a live Display): `check_timers_and_threads`
  only marks a redraw; a synthetic Expose only marks a redraw; real Exposes
  still render immediately with a full present. Linux check: `AZ_LOG=warn,+window`,
  count `[X11] render_and_present ... took=` during a switch glide (~1 per refresh).
- Least sure to compile: `run_count` reads through `borrows.layout_window`
  around `run_single_timer`; the `drain` in `record_hold_sample`; the test writes
  the gesture manager's `config` field (must be pub).
