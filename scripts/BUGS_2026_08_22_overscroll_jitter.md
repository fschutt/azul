# Overscroll jitter on the macOS trackpad — investigation (2026-08-22)

Branch `fix/open-bugs-wave-2026-08-22`, worktree `debug-slider-scroll-2026-08-22`.
Read-only investigation: no source edited, no cargo run. Numbers below come
from a scratch simulation of the exact formulas in `scroll_timer.rs` with the
`ScrollPhysics::macos()` preset (elasticity 0.3, max overscroll 80 px, bounce
400 ms, 16 ms tick, velocity threshold 30 px/s).

## Symptom (verbatim)

> "overscroll jitters" — when scrolling past the end of the content (rubber
> band), the view jitters instead of stretching smoothly and springing back.

Reported on the AzWidgets demo (one `overflow-y: auto` column,
`examples/azul-widgets/src/lib.rs:439-456`) on macOS with a trackpad. The
demo binary predates `51da00316` (closed-form spring).

## TL;DR

`51da00316` fixed the *spring-back oscillation* (explicit Euler ringing).
It did **not** touch the part of the path the finger is in, and that part
is where the macOS trackpad jitter lives:

**While the finger (or the OS momentum) is pushing past the edge, the
rubber-band curve is re-applied every tick to an offset that was already
rubber-banded on the previous tick.** The displayed overscroll is therefore
not a function of how far the finger has travelled past the edge (what
AppKit/UIKit do) but of how much delta arrived *in the current 16 ms tick*
— a per-tick *velocity* readout, scaled by ≈ 0.41. Anything that changes the
per-tick batch (a 120 Hz trackpad against the 62.5 Hz timer alternating 1
and 2 events per tick, NSTimer jitter, a CPU-render frame that delays the
tick, the finger's own micro-speed) shows up directly as the view moving
back and forth by 1-3 px at 30-60 Hz. That is the jitter. The spring-back
has the same defect on top (the spring's output is passed through the band
again), which turns the configured 400 ms bounce into a 3-frame snap and,
for overshoots ≥ 20 px, leaks the crossing velocity into a free-momentum
drift into the content.

Secondary: on macOS the momentum-phase events are classified as
`TrackpadContinuous`, and every `TrackpadContinuous` removes the node from
`node_velocities` — so the spring armed at finger-lift is killed by the
first momentum event and the user never sees a spring-back at all, only
the momentum decay filtered through the same per-tick map.

## Where the path lives (file:line map, all in this worktree)

| Stage | Location |
| --- | --- |
| macOS ingress, phase → source mapping | `dll/src/desktop/shell2/macos/events.rs:444-637`; mapping at `:511-533`; 0-delta gate at `:544-546`; timer creation with a **fresh** `ScrollPhysicsState` at `:594-630` |
| Queue + direction sign | `layout/src/managers/scroll_state.rs:593-600` (`record_scroll_input`), `:608-660` (`record_scroll_from_hit_test`), target choice `:670-692`, `can_consume_delta` `:721-757` |
| Timer: finger path | `layout/src/scroll_timer.rs:231-262` (accumulate onto `pending_trackpad_positions` **or the committed offset**, `:243-252`; `node_velocities.remove` `:261`) |
| Timer: gesture end | `scroll_timer.rs:356-407` |
| Timer: spring / momentum integration | `scroll_timer.rs:430-628`; closed-form step `:898-902`; the band re-applied to the spring output `:556-568`; `is_rubber_banding` reset `:612-617`; threshold zeroing `:619-625`; retain `:646-651` |
| Timer: commit of the finger position (band applied) | `scroll_timer.rs:714-740` → `scroll_to_unclamped` |
| Timer: self-termination | `scroll_timer.rs:117-128` (`is_active`), `:767-776` |
| `rubber_band_clamp` (the curve `D`) | `scroll_timer.rs:822-852` |
| Commit into the manager (stores the banded value verbatim) | `scroll_state.rs:872-891` (`set_scroll_position_unclamped`), read back by `get_scroll_node_info` `:1021-1043` |
| ScrollTo application in the shell | `dll/src/desktop/shell2/common/event.rs:4031-4075` (lightweight repaint) |
| Hard clamp after every layout pass | `scroll_state.rs:1128-1137` via `layout/src/managers/scroll_registration.rs:109`, called from `dll/src/desktop/shell2/common/layout.rs:46-65` and `:1481` |
| Hover restyle → incremental relayout | `common/event.rs:627-668` |
| NSTimer tick (free-running, not vsync) | `dll/src/desktop/shell2/macos/mod.rs:3240-3305`, tick → `request_redraw` `:1151-1187` |
| CPU present: sub-pixel baseline / integer shift | `dll/src/desktop/shell2/headless/mod.rs:611-661`, `layout/src/cpurender/compositor.rs:1008-1023` |
| Preset | `css/src/props/style/scrollbar.rs:163-177`, selected at `css/src/system.rs:2252` |

## The curve, for reference

`rubber_band_clamp` (`scroll_timer.rs:841-842`) maps a raw overshoot `o`
to a displayed overshoot

    D(o) = M · (1 − e^(−e·o/M))        e = 0.3, M = 80   (macOS)

`D(0) = 0`, `D'(0) = e = 0.3`, concave, so **`D(o) ≤ 0.3·o` for every
`o > 0`**. Applying `D` to a value that already went through `D` shrinks it
by ~70 %. That single property explains candidates 1, 2 and 5.

## Candidates

### 1. Band applied to an already-banded base — STILL OPEN, ROOT CAUSE

Evidence:

- `scroll_timer.rs:243-252`: `current = pending_trackpad_positions[key]`
  **or** `info.current_offset`. `current_offset` is what the previous tick
  committed through `scroll_to_unclamped` → `set_scroll_position_unclamped`
  (`scroll_state.rs:888`, stores the banded value verbatim) → read back by
  `get_scroll_node_info` (`scroll_state.rs:1034`). So across ticks the base
  is the **displayed (diminished)** offset, never the finger's raw travel.
- `scroll_timer.rs:254-258`: `new_pos = current + delta` (raw within the
  tick only); `:724-735`: `rubber_band_clamp(new_pos)` is committed.
- Net per-tick map, `x` = displayed overshoot, `d` = Σ deltas in the tick:
  `x ← D(x + d)`. With `D' ≈ 0.3` this is a contraction that forgets its
  history in ~2 ticks; its fixed point for a constant `d` is `≈ 0.41·d`.

Simulation (macOS preset, node at its bottom edge, 10 px deltas):

| input pattern | displayed overshoot per tick |
| --- | --- |
| 1 event/tick (60 Hz device) | 2.9, 3.8, 4.0, 4.1, 4.1, 4.1, … saturates at **4.1 px** |
| 2 events/tick (120 Hz device) | 5.8, 7.4, 7.8, 7.9, 8.0, … saturates at **8.0 px** |
| alternating 1/2 per tick (120 Hz device vs 62.5 Hz timer, the real case) | 2.9, **6.6, 4.8, 7.1, 5.0, 7.2, 5.0, 7.2**, … — a ±1.1 px sawtooth at ~31 Hz |
| finger nearly still for 3 ticks (0.02 px deltas pass the 0.01 gate) | 4.1 → **1.2 → 0.4 → 0.1** → 3.0 → 3.8 when it moves again |
| AppKit/UIKit reference `D(Σ deltas)` | 2.9, 5.8, 8.5, 11.1, 13.7, … 42.2 after 20 events — identical for 1 or 2 per tick |

So on the current code the stretch (a) never gets past ~5-15 px instead of
up to 80, (b) tracks the finger's instantaneous speed, and (c) flickers
with the event/tick beat. In-range scrolling is immune because there the
map is the identity (`x ← x + d`, batching cancels out) — which is exactly
why the user sees jitter *only* in overscroll.

Where the beat comes from on macOS: trackpad `scrollWheel` phase events
arrive at the display rate (60 Hz, or 120 Hz on ProMotion), the physics
runs on a free-running 16 ms `NSTimer` (`macos/mod.rs:3264-3286`, not a
display link), and the CPU renderer's `drawRect` on a Retina window delays
the run loop, so ticks see 0, 1, 2 or 3 events. On a 60 Hz device the
empty tick also makes the timer **terminate** (`scroll_timer.rs:767-776`,
`is_active` is false once the node has no velocity entry and nothing is
staged) and the next event re-creates it with a fresh `ScrollPhysicsState`
(`macos/events.rs:616`), resetting the tick phase — more batching noise.

Not covered by any test: every trackpad test in `scroll_timer.rs`
(`:2574`, `:2671`, `:2706`, `:2748`, `:2869`) pushes all deltas into **one**
tick (a single 150 px event) and then `TrackpadEnd`; none drives N deltas
across N ticks past the edge.

### 2. Momentum phase kills the spring-back — STILL OPEN, secondary

Evidence:

- `macos/events.rs:521-529`: `phase == Ended|Cancelled` and
  `momentumPhase == Ended|Cancelled` → `TrackpadEnd`; everything else with
  precise deltas — including `momentumPhase == Began|Changed` — →
  `TrackpadContinuous`.
- `scroll_timer.rs:261`: every `TrackpadContinuous` does
  `node_velocities.remove(&key)`, which drops `is_rubber_banding` and the
  spring state.
- AppKit sends `phase=Ended` (finger lift) and then, for a fling, a stream
  of `momentumPhase=Changed` events at the display rate for up to ~1.5 s,
  regardless of our content bounds (we are not an `NSScrollView`).

Sequence at a fling into the edge: lift → `TrackpadEnd` arms the spring
(`:383-391`) → first momentum event (same or next tick) removes it →
every momentum delta goes through candidate 1's map (`x ← D(x + d_n)`,
`d_n` decaying) → `momentumPhase=Ended` re-arms the spring from whatever
is left (< 1 px). Simulated: start 8 px, momentum deltas 15 → 0.9 px:
6.6, 5.9, 5.5, 5.1, 4.8, … 1.7 over 20 ticks. The user therefore never
sees a spring; the view "wobbles back" along the momentum decay, with the
batching sawtooth on top (momentum events at 60 Hz vs 62.5 Hz ticks → one
empty tick, i.e. a stall plus a timer restart, every ~400 ms).

It is not a spring-restart oscillation (the spring never gets a tick to
run), so by itself it is a "wrong feel", not a jitter generator — but it
removes the one phase that was supposed to look smooth.

### 3. Two writers / a hard clamp on the committed offset — MOSTLY RULED OUT, one narrow case STILL OPEN

- Single ingress: the macOS handler only queues (`macos/events.rs:556-578`);
  the only writer during a gesture is the timer's `ScrollTo { unclamped:
  true }` (`common/event.rs:4040-4045`). The discrete `scroll_to(…, 0 ms)`
  branch (`:4047-4052`, hard clamp) is reached only for `Programmatic`
  inputs (`scroll_timer.rs:701-712`). No double ingress on macOS.
- `quantize_thumb_offset` (commit `5a58d2e4a`) only touches the thumb
  value in the GPU cache (`layout/src/managers/gpu_state.rs:199-231`); it
  never writes the offset.
- `ScrollTo` is a lightweight repaint (`ShouldReRenderCurrentWindow`,
  `common/event.rs:4073`); no relayout per tick.
- **Still open (narrow):** `register_or_update_scroll_node`
  (`scroll_state.rs:1137`) hard-clamps `current_offset` to `[0, max]` on
  **every** layout pass (`register_scroll_nodes` runs after
  `regenerate_layout` and `incremental_relayout`,
  `common/layout.rs:63-65`, `:1481`). A layout-affecting `:hover` change
  under the cursor while the content moves (`apply_hover_restyle` →
  `ShouldIncrementalRelayout`, `common/event.rs:652-664`) therefore snaps an
  overscrolled view to the edge mid-gesture; the next tick pushes it out
  again from the clamped base. In AzWidgets the divider has
  `on_hover(height: 3px)` (`layout/src/widgets/divider.rs:877`); most other
  widget hover rules are colour-only (paint-only, no relayout). Plausible
  as an occasional extra snap, not as the steady jitter.

### 4. Sub-pixel / patch-translate present path — RULED OUT as a cause

- `last_patch_move` / `TranslateHint` is the *layout patch* blit
  (`layout/src/window.rs:12505-12517`), not scroll.
- Scroll shift rounds the **absolute** offsets and telescopes
  (`compositor.rs:1008-1023`: `round(new·dpi) − round(prev·dpi)`), and
  sub-half-device-pixel deltas keep their baseline and accumulate
  (`headless/mod.rs:637-661`, test `damage_subpixel_scroll_accumulates`).
  A monotone offset produces a monotone sequence of integer shifts; the
  present path cannot turn a monotone input into an oscillation. It does
  faithfully display candidate 1's ±1-2 logical px sawtooth as a ±2-4
  device-pixel flicker at 2×.

### 5. Friction / threshold zeroing on the spring tail — NOT a jitter cause, but the spring-back is broken in a different way — STILL OPEN

- Friction on the spring's velocity (`scroll_timer.rs:572-575`) is
  `exp(−0.003·0.016·60) ≈ 0.997` per tick — negligible.
- Threshold zeroing (`:619-625`) only restarts the closed form from
  `v = 0` for the last < 0.5 px; monotone, no stall.
- **The real defect:** the spring's exact step is committed through
  `rubber_band_clamp` **again** (`:534-535` builds
  `raw_new = current_offset + spring_disp`, `:556-568` bands it). With
  `D' ≈ 0.3` the displayed overshoot shrinks ~70 %/tick on top of the
  spring's own ~3 %/tick:

  | start overshoot | current code | exact closed form (what `51da00316` intended) |
  | --- | --- | --- |
  | 13.7 px (the test's value) | 13.7 → 3.9 → 1.0 → 0.15, done in **3 ticks / 48 ms** | 13.7 → 13.3 → 12.5 → 11.3 → … → 0.44 in 21 ticks / ~340 ms |
  | 40 px | 40 → 10.9 → 2.7 → 0.4 → **−0.5 → −1.0 → −1.3 …** | 40 → 38.9 → 36.4 → … → 0.44 in 26 ticks |

  The 40 px row shows a second, latent bug: the spring's velocity at the
  moment the displayed value crosses the edge is ≈ −69 px/s (> the 30 px/s
  threshold), `is_rubber_banding` is cleared at < 0.5 px (`:612-617`), and
  the remaining velocity continues as **free momentum with the macOS 0.997
  deceleration** (`:518-532`): ≈ 0.65 px/tick for ~100 ticks ≈ 60 px of
  drift into the content over ~1.7 s. Today candidate 1 keeps displayed
  overshoots under ~15 px so this rarely fires; once candidate 1 is fixed
  and the stretch reaches 40-80 px it will fire on every release.
- `a_rubber_band_spring_back_is_monotone_at_every_preset`
  (`scroll_timer.rs:2869`) cannot see either: it asserts only
  non-increasing values and `|settled − 100| < 1` after 240 ticks, from a
  13.7 px start.

## What `51da00316` fixed, and what the user will still see

Fixed: the explicit-Euler sign flip in both springs (wheel glide and
rubber-band return). That bug needed the spring to actually run, which on
the macOS trackpad path it only does after `momentumPhase=Ended`, from a
sub-pixel overshoot — so on macOS the commit changes nothing visible. Every
effect listed under 1, 2, 3 (narrow) and 5 is present on the branch tip.

## Single most likely remaining cause

**Candidate 1** — the finger's overscroll is recomputed every tick as
`D(displayed + Σdelta_this_tick)` instead of `D(raw finger travel)`. It is
the only mechanism here that converts ordinary event batching noise into
back-and-forth motion, it is specific to overscroll (in-range scrolling is
linear), and its magnitude (±1-3 px at 30-60 Hz, plus a collapse to zero
whenever the finger slows) matches "jitters instead of stretching". Candidate
5 (snap instead of spring, velocity leak) and candidate 2 (momentum kills
the spring) are what make the *release* look wrong; they must be fixed in
the same pass or fixing 1 will expose the 60 px drift.

## Fix plan

All in `layout/src/scroll_timer.rs` unless noted. Mirrors the
`UIScrollView`/`NSScrollView` model: the band is a function of the
unclamped content offset while a finger is down; a release converts into a
critically-damped spring that owns the axis until it lands.

### A. Accumulate the raw finger offset across ticks (candidate 1)

```rust
// ScrollPhysicsState
/// UNCLAMPED finger offset per node for the gesture in flight. Seeded
/// from the committed offset on the first delta (inverted through the
/// band if that offset is already overscrolled), advanced by every
/// TrackpadContinuous/TrackpadMomentum delta, dropped at TrackpadEnd or
/// after `GESTURE_STALE_MS` without a delta.
pub trackpad_raw_positions: BTreeMap<(DomId, NodeId), (LogicalPosition, Instant)>,
```

- `TrackpadContinuous` (`:231-262`): `raw = trackpad_raw_positions[key]`
  or `rubber_band_unclamp(info.current_offset)`; `raw += delta`; store it
  in `trackpad_raw_positions` **and** in `pending_trackpad_positions` (so
  step 3 at `:714-740` keeps banding it for display, unchanged). Add the
  inverse `rubber_band_unclamp(y) = boundary ± (M/e)·ln(1 − |y − boundary|/M)`
  (guard `|y − boundary| < M`).
- `TrackpadEnd`: remove the raw entry; arm the spring from the **displayed**
  overshoot as today. Do not zero an already-running spring velocity
  (`:389` → `or_insert` semantics) so a momentum bump (see D) survives a
  late `TrackpadEnd`.
- `is_active()` (`:117-128`): also true while `trackpad_raw_positions` is
  non-empty, so a slow finger (sparse events, empty ticks) does not
  terminate the timer and lose the accumulator; drop entries older than
  `GESTURE_STALE_MS` (≈ 200 ms, the X11 synthetic-end window is 100 ms,
  `linux/x11/mod.rs:3209-3265`) at the top of the tick so a lost `End`
  cannot pin the timer.
- `select_scroll_target` / `can_consume_delta` (`scroll_state.rs:721-757`)
  already treat an overscrolled node as pinned and fall back to the
  innermost, so a single scroller keeps receiving its own deltas. (Side
  note, not this bug: with a nested scroller under the cursor the fallback
  sends the page's outward overscroll deltas to the *inner* node.)

### B. The spring commits its own output; land exactly (candidate 5)

In the integration loop (`:556-568`): on an axis where the rubber-band
spring ran this tick, commit `boundary + overshoot_after` **directly**
(limit to `±max_overscroll_distance`), not through `rubber_band_clamp`.
The band models the finger's resistance, not the spring. Then:

- If `|overshoot_after| < 0.5` and `|v| < threshold`, snap to the boundary
  exactly, `v = 0`, `is_rubber_banding = false` (the seek path already does
  this for `animate_targets`, `:456-465`). Today the view parks at
  `max + 0.15…0.45` forever.
- If the step crosses the boundary (sign change of the overshoot), clamp
  to the boundary and zero the velocity on that axis — never let the
  crossing velocity fall through to the free-momentum branch (`:518-532`).
  A critically-damped spring cannot cross in exact arithmetic; any crossing
  is the band/threshold interplay.
- Free momentum that reaches an edge with rubber-band allowed: keep the
  velocity and set `is_rubber_banding = true` so the *next* tick's
  closed-form step starts with `v0 ≠ 0` (natural bump of `v0/(ω·e)` and
  return), instead of banding `v·dt` every tick (same velocity-readout
  artifact as candidate 1, just driven by `v` instead of the finger).

### C. Don't hard-clamp an overscrolled offset on an unrelated relayout (candidate 3, narrow)

`scroll_state.rs:1137`: re-clamp only when `container_rect`/`content_rect`
actually changed, or clamp to the band envelope
`[−max_over, max + max_over]`. Same for `update_node_bounds` (`:964`) and
`update_virtual_scroll_bounds` (`:992`) if they can run mid-gesture.

### D. Momentum events are their own source (candidate 2)

- `layout/src/managers/scroll_state.rs`: add
  `ScrollInputSource::TrackpadMomentum` (internal enum, not in `api.json`
  — no autofix round). `macos/events.rs:521-529`: `momentumPhase ==
  Began|Changed` → `TrackpadMomentum`; X11/Wayland unchanged (they only
  produce `Continuous` + a synthetic `End`).
- Timer: `TrackpadMomentum` behaves like `TrackpadContinuous` (accumulate
  raw, band for display) **while the axis is in range**; the first
  momentum delta that pushes past an edge arms the spring with
  `v = delta/dt` (bump) and from then on momentum deltas on that axis are
  dropped until `momentumPhase=Ended` — the spring owns the axis, the OS
  momentum knows nothing about our edge. Momentum deltas must **not**
  `node_velocities.remove` an armed spring.
- Keep `momentumPhase=Ended|Cancelled → TrackpadEnd` (harmless with the
  `or_insert` change in A).

Optional polish after A-D: the bounce feels more native if `TrackpadEnd`
seeds the spring with the release velocity (last two deltas / dt) instead
of zero.

## How to verify

Closed-loop unit tests in `scroll_timer.rs` `autotest_generated`, using the
existing `closed_loop_tick` harness (`:2414-2507`), `ScrollPhysics::macos()`,
node `(100×100)` over `(100×200)` so `max_scroll_y = 100`, seeded at the
edge with one in-range 100 px `Programmatic` input first.

1. `a_held_finger_past_the_edge_stretches_monotonically_and_independently_of_batching`
   — three runs of twenty 10 px `TrackpadContinuous` deltas: one per tick,
   two per tick, alternating 1/2 per tick (the 120 Hz-vs-62.5 Hz case).
   Assert per run: the committed offset is non-decreasing tick to tick
   (`w[1] >= w[0] − 1e-3`); every tick that had input strictly increased it;
   final offset `= 100 + D(200) = 142.2 ± 0.05`; the three finals agree
   within 0.05. Today: 104.1 / 108.0 / 105↔107 sawtooth — all three
   assertions fail.
2. `a_finger_that_pauses_or_creeps_keeps_its_stretch` — six 10 px deltas,
   three empty ticks, three ticks with a 0.02 px delta, three 10 px deltas:
   the offset never decreases while the finger is down and the timer never
   returns `Terminate` in between. Today: 104.1 → 101.2 → 100.4 on the
   creep, and the timer terminates on the empty ticks.
3. `the_spring_back_takes_the_configured_bounce_duration_and_lands_exactly`
   — stretch to a displayed 40 px (raw overshoot `(M/e)·ln 2 ≈ 185 px`,
   e.g. nineteen 10 px deltas), `TrackpadEnd`, 60 closed
   ticks: non-increasing; at tick 3 still `> 50 %` of the start; `< 0.5 px`
   first reached between tick 12 and tick 30 (400 ms bounce); final offset
   `== 100.0` exactly and never `< 100 − 0.01` (no drift). Today:
   40 → 10.9 → 2.7 → 0.4 → −0.5 → … then ~60 px of drift.
4. `momentum_deltas_after_the_finger_lifts_do_not_kill_the_spring_back` —
   five 10 px `Continuous` past the edge, `TrackpadEnd`, then twenty ticks
   each carrying one `TrackpadMomentum` delta (15 px decaying ×0.93) and a
   final momentum `TrackpadEnd`: from the lift on the offset is
   non-increasing (after at most one bump tick) and lands on 100.0. Plus an
   in-range fling that reaches the edge via momentum: overshoot `> 0` at
   some tick (bump), then monotone return.
5. Tighten `a_rubber_band_spring_back_is_monotone_at_every_preset` with the
   duration lower bound from (3) and an exact landing.
6. `register_or_update_scroll_node` with unchanged bounds keeps an
   overscrolled offset (candidate 3, `scroll_state.rs` tests).

On device (macOS, AzWidgets, trackpad): run with `AZ_SCROLL_DEBUG=1`
(`scroll_timer.rs:155-189`, `:541-554`) and push past the bottom edge at a
steady speed. Before the fix the `TICK … commit=` values at the edge bounce
by 1-3 px while the `IN` line count per tick alternates 1/2; after the fix
they are monotone and the stretch reaches tens of px. Check both a 60 Hz
external display and a 120 Hz ProMotion panel, and a fling release (spring
visible, no drift). Linux: re-run the X11/Wayland trackpad E2E scenarios —
A/B/C do not change their ingress, D adds a variant they never emit.

## Effort

- A + B + D (timer, `ScrollInputSource` variant, macOS mapping, six tests):
  ~1 day, one author. Release-only builds in this repo (memory:
  `release_only_builds_disk.md`).
- C: 1-2 h.
- On-device verification on macOS: 1 h; needs the user's trackpad.

## Overlaps / risks

- `51da00316` (closed-form spring; its two tests stay valid, test 5 extends
  one). `b44804467`/`dd90d4938` (slider/damage) are unrelated.
- The B1 regression tests
  (`two_trackpad_events_in_one_tick_accumulate_instead_of_overwriting`,
  `the_spring_does_not_also_write_a_node_the_finger_moved_this_tick`,
  `an_overscrolled_gesture_springs_back_to_the_boundary`) keep passing under
  A/B: the per-tick accumulation and the single-writer rule are preserved;
  only the *base* of the accumulation changes.
- `is_active()` keeping the timer alive while a raw entry exists changes
  the timer's self-termination contract; the stale-drop must be in the
  same commit or a backend that never sends `TrackpadEnd` (X11 before its
  synthetic end, a dropped event) would tick forever.
- Wayland's `axis_stop` and X11's 100 ms synthetic `TrackpadEnd`
  (`linux/wayland/mod.rs:3702-3736`, `linux/x11/mod.rs:3209-3265`) seed the
  spring the same way as macOS; A/B improve them identically (they have the
  same per-tick map today).
- The X11 comment at `linux/x11/mod.rs:3260-3262` ("the physics timer is
  still running") is wrong today — the timer terminates on the first empty
  tick; the shared arming site in `common/event.rs:6912-6960` is what
  rescues the synthetic `End`. Harmless, but worth a one-line fix while
  there.
- `ScrollPhysics::macos().invert_direction` is unused (macOS pre-applies
  natural scrolling; the manager has its own `scroll_sign`) — noted only so
  nobody "fixes" it into a double inversion.
