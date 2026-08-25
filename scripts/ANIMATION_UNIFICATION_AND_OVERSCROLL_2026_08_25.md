# Two systemic findings: animation-path divergence, and the overscroll physics

Both were investigated to root cause on 2026-08-25 (branch `transient-window`).
Neither is landed. This file is the handoff.

---

# 1. `-azul-animation-in` / `-out` do not run in the real application

## The divergence

`regenerate_layout` exists TWICE:

| | reconciliation |
|---|---|
| desktop shell — `dll/src/desktop/shell2/common/layout.rs:~761` | a hand-rolled inline block |
| headless e2e runner — `layout/src/e2e/runner.rs:2907`, `:2918` | `LayoutWindow::begin_reconciliation` / `finish_reconciliation` |

The shared implementation (`layout/src/window.rs:8116` / `:8519`) is a strict
SUPERSET of the shell's copy. The shell's block does `reconcile_dom` +
`transfer_states` + `migrate_user_overrides_from` + the manager remap and stops
there. Everything the shared version *additionally* derives never happened in
the product:

- `mounted` / `unmounted` subtree roots (derived structurally from the
  correspondence, not from `diff.events`, which is gated on
  `has_mount_callback` and so misses a plain `<div>`)
- `exit_rects` — where a departing node was, so an exit has somewhere to
  animate FROM
- the CSS diff (`restyled`) and its `captured_transitions`
- zombie RETENTION for `-azul-animation-out`
- `-azul-animation-in` live tracks + the mid-flight zombie catch, in
  `finish_reconciliation`

So the entire enter/exit animation feature is live ONLY under the e2e runner.
`PendingReconciliation`'s own doc comment (`window.rs:18090`) names this exact
drift as the reason the type was created — the shared path was built and only
one caller was moved onto it.

This is the same class as the e2e `key_down` Backspace shortcut fixed earlier
this session (`543bfda17`): a test harness on a different code path than the
product, so the tests are green and the product is dead.

## What was tried, and the blocker

Replacing the shell block with `begin_reconciliation` + `finish_reconciliation`
WORKS and is small (the manager remap is literally the same call —
`update_managers_with_node_moves` is a one-line wrapper over `remap_node_ids`).
`BeforeUnmount` has to move INTO `begin_reconciliation` (it needs the old arena,
which dies at the swap) along with the ordinary lifecycle queue; that was done
and compiles.

**The blocker is damage, not reconciliation.** With the shell staging a CSS diff
for the first time, `pending_css_dirty` becomes non-empty on rebuild frames, and
`dll/.../headless/mod.rs::damage_survives_shadow_shrink_and_anonymous_boxes`
fails with **2830 stale pixels, first at (6,44) while the damage rect was
`384x98 @ (8,8)`** — the vacated BOX-SHADOW fringe to the left of the shrinking
box is not damaged. Attribution is definitive: stashing only the two unification
files makes the test pass again.

Mechanism: the item-level damage diff
(`layout/src/cpurender/compositor.rs:2433`) damages the items it can see differ.
A cascade change also moves paint attributable to no single item — a shadow
fringe extends outside both its own box and its parent's padding box — so the
diff damages the boxes and leaves the fringe on screen.

A blanket "cascade changed ⇒ full repaint" flag on `DisplayList` fixes that test
but is too blunt: it then breaks four damage-LOCALITY tests
(`damage_box_paint_change_is_local`, `damage_box_size_reflow`,
`damage_single_paint_in_large_grid_is_local`,
`native_target_render_matches_owned_and_retains_nothing`).

## Recommended next step

Narrow the bail: force a full repaint only when a node in the CSS diff carries
INK-OVERFLOWING paint (`box-shadow`, `outline`, `filter`), otherwise keep the
item diff. Equivalently, union the OLD and NEW `visual_bounds()` of every
css-dirty node into the damage — the patched-build path at
`layout/src/solver3/mod.rs:1521` already does exactly this for `changed`, and
that machinery could be reused for `css_dirty`.

Land the reconciliation switch and the damage fix together; the switch alone
ships stale pixels.

---

# 2. Overscroll: "queued" bounces, blocky motion, selection interference

Full input path: `scrollWheel:` (`macos/mod.rs:1008`) → `handle_scroll_wheel`
(`macos/events.rs:459`) → `record_scroll_from_hit_test`
(`layout/src/managers/scroll_state.rs:618`) → queue (`:173`) →
`scroll_physics_timer_callback` (`layout/src/scroll_timer.rs:230`) →
`ScrollTo{unclamped}` (`common/event.rs:4351`).

## There is no queue — three other things make bounces look queued

REFUTED: "one input drained per tick". `take_recent(100)` (`scroll_timer.rs:281`)
drains the WHOLE vec each tick and accumulates within the tick. No animation
queue exists anywhere — every physics map is one entry per node.

1. **The spring is frozen for the entire OS momentum tail.**
   `pending_trackpad_positions.insert` runs at `scroll_timer.rs:392` for EVERY
   trackpad/momentum event, that key lands in `moved_by_finger_this_tick`
   (`:574`), and the integration loop `continue`s (`:578`). macOS momentum runs
   1-2 s after fingers-up, so a band armed at fingers-up sits motionless until
   the tail ends and only THEN plays. A perfectly axis-aligned flick escapes
   (both axes mask to 0 and the event `continue`s at `:326`), which is why the
   existing test passes and real diagonal flicks do not.
2. **Each flick delivers 2-3 `TrackpadEnd`s and the handler is not idempotent.**
   `macos/events.rs:536-541` maps `phase Ended`, `phase Cancelled`,
   `momentumPhase Ended` AND `momentumPhase Cancelled` all to `TrackpadEnd`.
   Each one re-arms a from-rest 400 ms bounce when overshot (`:526-538`), so N
   flicks give N sequential bounces.
3. **Nothing merges.** `scroll_timer.rs:398` `node_velocities.remove(&key)`
   discards the in-flight spring's velocity when a new finger lands. The
   POSITION retargets correctly; the velocity is thrown away.

The precedent for the fix is already in the file: `animate_targets` retargeting
deliberately keeps velocity (`:437`, test `animate_to_retarget_keeps_the_current_velocity`
at `:2455`). The band never got the same treatment.

## Blockiness is dt, not the integrator

The integrator is the closed form of a critically-damped spring
(`scroll_timer.rs:1141`) — unconditionally stable, NOT explicit Euler (Euler was
removed earlier because it rang). But `dt` is hard-coded
(`let dt = sp.timer_interval_ms.max(1) as f32 / 1000.0;`, `:241`) and the wall
clock is never consulted even though `TimerCallbackInfo::frame_start` carries it.

The real tick spacing is not 16 ms:
- `Timer::invoke` (`layout/src/timer.rs:206-216`) DROPS a fire that lands at
  15.9 ms and stamps `last_run = now` (actual fire time) rather than
  `last_run + interval`, so the phase never self-corrects and the next step is
  ~32 ms later.
- Two independent 16 ms NSTimers both drive `tickTimers:` (`macos/mod.rs:3493`
  and `:3571`), quantising admitted spacing to a jittering 16/24/32 ms.
- `0.016` is neither 16.667 ms (60 Hz) nor 8.333 ms (120 Hz), and the
  CVDisplayLink only calls `setViewsNeedDisplay:` (`macos/mod.rs:3974`) — it
  never steps physics.

Each tick advances the simulation by a fixed 16 ms while 16-32 ms of real time
passed ⇒ visible speed varies ±50 % frame to frame. The finger-down path is
POSITION-accumulating and dt-independent, which is exactly why dragging feels
fine and flinging/bouncing does not.

## Per-node state, not per-axis (5 leaks)

`NodeScrollPhysics { velocity, is_rubber_banding }` (`scroll_timer.rs:120`) —
the velocity is two floats but the flags and map keys are per NODE:
`:574` (`moved_by_finger_this_tick` skips BOTH axes), `:381` (wholesale velocity
replace zeroes the other axis), `:398` (remove kills both axes' velocity AND the
flag), `:534` (node-level `is_rubber_banding` guard), `:792` (one flag for two
axes). Commit `5da2f1200`'s per-axis masking (`:302-328`) fixes only the DROP
decision.

⚠ The existing test `a_rubber_band_on_one_axis_does_not_freeze_the_other_axis_fling`
(`scroll_timer.rs:3329`) PINS THE BUG — its final assertion `x > 100.5` asserts
that X does not spring back while Y flings. It must be inverted.

## Selection ↔ scroll: they fight, in two ways

There is no arbitration and no `is_selecting` flag anywhere; `scroll_timer.rs`
never mentions selection.

1. **Hit-testing reads the UNCLAMPED, mid-bounce offset.** The drag path
   (`window.rs:14563`) resolves through `current_offset`, which the physics
   timer writes unclamped (`scroll_timer.rs:919` → `event.rs:4359` →
   `scroll_state.rs:882`, no clamp). During a ~400 ms bounce a MOTIONLESS
   pointer resolves a different text index every tick, and an overshoot that
   pushes `local_pos` outside the anchor's `used_size` flips the selection
   between single-node and cross-block representation (`window.rs:14577`).
2. **Drag-autoscroll stomps the stretch every frame.** `auto_scroll_timer_callback`
   (`event.rs:339`) reads the overshot offset (`:407`), hard-clamps (`:474`) and
   writes back clamped. It is armed on EVERY selection drag move
   (`core/src/events.rs:4081`), and timer ids order it right after the physics
   timer (`core/src/task.rs:69`). Net: physics stretches, autoscroll collapses,
   repeat — visible flicker plus a jittering selection endpoint.

## Recommended fix

- **Per-axis state**: `{ x: AxisPhysics, y: AxisPhysics }`,
  `AxisPhysics { velocity, raw, mode }`,
  `mode ∈ {Idle, Gesture, Momentum, Fling, Band, Seek}`. Collapse
  `node_velocities` + `pending_trackpad_positions` + `trackpad_raw_positions` +
  `animate_targets` into one map.
- **Retarget, never enqueue**: a new input on an axis mutates that axis's
  `{raw, velocity, mode}` IN PLACE, never resets velocity, never creates a
  second animation. Make `TrackpadEnd` idempotent (if already `Band`, do
  nothing) and seed its velocity from a per-axis EWMA release estimate.
- **Delete `moved_by_finger_this_tick`**; dispatch per axis on `AxisMode` and
  emit ONE `scroll_to_unclamped` per node per tick (an existing test pins that
  invariant).
- **Real dt**: step physics from the CVDisplayLink's `CVTimeStamp` delta, or at
  minimum take `dt` from `TimerCallbackInfo::frame_start` clamped to
  [1 ms, 50 ms], and fix `timer.rs:206` to advance `last_run` by `interval`
  (phase-accumulating) rather than stamping the actual fire time.
- **Split committed vs presented offset**: add
  `AnimatedScrollState::overscroll`, keep `current_offset` always clamped.
  Compositor/WR read the sum; hit-test, `accumulated_scroll_for_node` and the
  selection drag read `current_offset` only. This fixes BOTH selection problems
  without any mutual exclusion, and makes `scroll_state.rs:1063`'s "clamped" doc
  true.
- macOS: distinguish `momentumPhase == Cancelled` (a finger landed → cancel
  momentum, do NOT arm a band) from `Ended` at `macos/events.rs:536`.

### Tests that must change
Invert `scroll_timer.rs:3329`. Mechanical breaks on the struct change: `:1802`,
`:1868`, `:1897`, `:1930`, `:1941`, `:1954`. On the dt change: `:2284`, `:3391`.
On the committed/presented split: `scroll_state.rs:2436`, `:2448`, `:1139`.
Must keep passing: `:2957` (one ScrollTo per node per tick), `:3228`, `:3272`,
`:3432`, `:2455`, `layout/tests/drag_selection_scroll.rs:110`/`:138`,
`layout/src/e2e/full.rs:7468`.

---

# 3. `overscroll-behavior` — WIRED 2026-08-25 (commit 34a8f25c4)

Done. See that commit; the two traps worth remembering are that the shorthand
is a `CombinedCssPropertyType` (not a longhand), and that `auto`/`none` are
TYPED values that must be added to the `has_typed_auto` / `has_typed_none`
allowlists in `parse_css_property` or they resolve to "no value".

## ⚠ UPDATE to §2: the two queueing hypotheses did NOT reproduce

Two tests were written to state the CORRECT behaviour, and BOTH PASS on the
current code:

- `a_rubber_band_on_one_axis_does_not_freeze_the_other_axis_fling` — extended
  with the missing assertion: once momentum stops pushing X, X must spring back
  toward its edge. It does.
- `a_late_trackpad_end_does_not_restart_a_finished_bounce` — a trailing
  `TrackpadEnd` after a settled bounce must not bounce again. It does not.

So neither "the spring is frozen for the whole momentum tail" nor "each
TrackpadEnd re-arms a from-rest bounce" reproduces in the harness. Note the
investigation that proposed them ran NO builds — it was source-reading only.

That leaves two possibilities, and they need separating before any rewrite:
1. The harness diverges from the real macOS event stream. `closed_loop_tick`
   drives the physics callback directly and does NOT model `Timer::invoke`'s
   readiness gate (`layout/src/timer.rs:206`), which on a real device DROPS a
   fire that lands a hair under the interval and then stamps `last_run = now`.
   A harness that models that gate is the first thing to build.
2. The real cause is elsewhere — e.g. in the ingress classification
   (`macos/events.rs:526-556`) rather than the physics.

The dt finding stands on its own and is verifiable by reading:
`let dt = sp.timer_interval_ms.max(1) as f32 / 1000.0;` (`scroll_timer.rs:241`)
is a FIXED 16 ms, the wall clock is never consulted even though
`TimerCallbackInfo::frame_start` carries it, and the real spacing jitters. That
is a genuine defect and the most likely cause of "blocky", independent of the
queueing question.

# 3b. (original) `overscroll-behavior` is not a CSS property at all

The `OverscrollBehavior` enum exists (`css/src/props/style/scrollbar.rs:47`) and
the physics fully consume it (`scroll_timer.rs:764`, `:778`, `:870`, `:978`) —
but there is NO entry in the property table (`css/src/props/property.rs`), no
`CssProperty` variant, and no parser. `AnimatedScrollState.overscroll_behavior_x/y`
is hardcoded to `Auto` at `scroll_state.rs:1182` and `:1422` and never updated,
so every `contain` / `none` branch in the physics is unreachable.

Wiring it needs the `ScrollbarWidth` treatment (~15 sites): property-table entry,
`CssProperty::OverscrollBehaviorX/Y` variants + value type aliases,
`css/src/props/macros.rs:410`-style arm, parser dispatch
(`property.rs:3404`), `impl_from_css_prop!`, `as_*` accessor, the
`get_css_value_fmt` / `is_initial` / `FormatAsRustCode` arms, a getter in
`layout/src/solver3/getters.rs`, and finally reading it in
`layout/src/managers/scroll_registration.rs` when the node is registered. It is
also FFI, so `api.json` + `codegen all` are required.
