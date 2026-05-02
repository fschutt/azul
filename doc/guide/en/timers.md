---
slug: timers
title: Timers
language: en
canonical_slug: timers
audience: external
maturity: wip
guide_order: 100
topic_only: false
short_desc: Frame-rate independent timers, threads, and how scheduled work re-enters the layout pipeline.
prerequisites: [hello-world, events]
tracked_files:
  - core/src/task.rs
  - layout/src/timer.rs
  - layout/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T06:00:00Z
---

# Timers

> WIP. The Timer API is functional today; field names may shift before 1.0.

A `Timer` is a function that runs on the main UI thread on its own schedule, independently of input events. You install it from inside an event callback and tear it down the same way. The framework wakes the event loop to fire it; the callback receives a normal `CallbackInfo` plus a `TimerCallbackInfo` wrapper.

```rust,no_run
# use azul::prelude::*;
# extern crate azul_core;
# use azul_core::task::{Duration, SystemTimeDiff};
# extern "C" fn tick(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
#     TimerCallbackReturn::continue_and_refresh_dom()
# }
# extern "C" fn on_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
let timer = Timer::create(data.clone(), tick, info.get_system_time_fn())
    .with_interval(Duration::System(SystemTimeDiff::from_millis(16)));

info.add_timer(TimerId::unique(), timer);
#     Update::DoNothing
# }
```

## When to reach for a timer

Use a timer when something must happen on a clock, not in response to input:

| Need | Timer? | Notes |
|---|---|---|
| React to a click, hover, or key | No | Use an event filter ([events](events.md)). |
| Re-paint every frame | Yes | Interval = 16 ms; return `continue_and_refresh_dom()`. |
| Run something once after a delay | Yes | `with_delay(d)`; terminate from the callback. |
| Poll a long-running operation | No | Use a background thread (covered later in [background-tasks](background-tasks.md)). |
| Animate a CSS property | Not yet | The animation runtime is a stub; see [animations](animations.md). |

Timers run on the UI thread. Heavy work blocks input. For anything I/O-bound or CPU-heavy, spawn a thread.

## The signature

A timer callback has the same C-ABI shape as an event callback, but the second argument and return type differ.

```rust,ignore
pub type TimerCallbackType = extern "C" fn(
    RefAny,
    TimerCallbackInfo,
) -> TimerCallbackReturn;
```

Defined at `layout/src/timer.rs:35`. The `RefAny` is whatever you handed to `Timer::create` — most often the same data you handed to your layout callback. `TimerCallbackInfo` is documented in the next section. The return value is two enums packed into one struct:

```rust,ignore
pub struct TimerCallbackReturn {
    pub should_update: Update,
    pub should_terminate: TerminateTimer,
}
```

`Update` is the same enum event callbacks return — `DoNothing`, `RefreshDom`, `RefreshDomAllWindows`. `TerminateTimer` is `Continue` or `Terminate`. Four convenience constructors match the common combinations:

```rust,ignore
TimerCallbackReturn::continue_unchanged()        // keep ticking, no relayout
TimerCallbackReturn::continue_and_refresh_dom()  // keep ticking, re-run layout
TimerCallbackReturn::terminate_unchanged()       // stop, no relayout
TimerCallbackReturn::terminate_and_refresh_dom() // stop, re-run layout
```

## TimerCallbackInfo

The wrapper type gives the callback two things the event API doesn't: which call this is and when the frame started.

| Field | Use |
|---|---|
| `call_count: usize` | 0 on first invocation, monotonic from there. Useful for "after N ticks, do X". |
| `frame_start: Instant` | Monotonic timestamp captured before the callback runs. Use for animation interpolation rather than calling `Instant::now()` again. |
| `is_about_to_finish: bool` | `true` only on the final invocation when a timeout is configured (see below). |
| `node_id: OptionDomNodeId` | The node the timer was attached to, if any. |
| `callback_info: CallbackInfo` | The full event-API surface, exposed via `get_callback_info_mut()`. |

`TimerCallbackInfo` re-exports the methods you'd reach for from `CallbackInfo` directly — `add_timer`, `remove_timer`, `add_thread`, `remove_thread`, `modify_window_state`, `scroll_to`, `scroll_to_unclamped`, `set_cursor_visibility`, `reset_cursor_blink` (full list at `layout/src/timer.rs:325`). Mutations are recorded as `CallbackChange`s and applied after the callback returns, exactly as in event callbacks.

## Scheduling a timer

`Timer` uses a builder. The constructor takes the data, the callback, and a system-time function (passed in from the event loop):

```rust,ignore
pub fn create<C: Into<TimerCallback>>(
    refany: RefAny,
    callback: C,
    get_system_time_fn: GetSystemTimeCallback,
) -> Self
```

The three modifier methods cover all common scheduling shapes:

```rust,ignore
impl Timer {
    pub fn with_delay(self, delay: Duration) -> Self;     // wait before first fire
    pub fn with_interval(self, interval: Duration) -> Self; // gap between fires
    pub fn with_timeout(self, timeout: Duration) -> Self;   // stop after this much elapsed
}
```

A timer with no `interval` set defaults to a 10 ms tick (`DEFAULT_TIMER_TICK_MS`, `layout/src/timer.rs:32`). A timer with no `timeout` runs until the callback returns `TerminateTimer::Terminate` or you call `remove_timer`.

```rust,no_run
# use azul::prelude::*;
# extern crate azul_core;
# use azul_core::task::{Duration, SystemTimeDiff};
# extern "C" fn tick(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
#     TimerCallbackReturn::continue_and_refresh_dom()
# }
# extern "C" fn on_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
// Run after 500ms, then every 16ms, for at most 5 seconds.
let timer = Timer::create(data.clone(), tick, info.get_system_time_fn())
    .with_delay(Duration::System(SystemTimeDiff::from_millis(500)))
    .with_interval(Duration::System(SystemTimeDiff::from_millis(16)))
    .with_timeout(Duration::System(SystemTimeDiff::from_secs(5)));

info.add_timer(TimerId::unique(), timer);
#     Update::DoNothing
# }
```

## Stopping a timer

Three ways, in order of preference:

1. **Return `TerminateTimer::Terminate`** from the callback when its work is done. The normal teardown path.
2. **Set `with_timeout`** when the schedule is bounded. The runtime forces termination on the tick that crosses the deadline; the callback sees `is_about_to_finish == true` on its final call so it can flush state.
3. **Call `info.remove_timer(timer_id)`** from any other callback when an external event (a window close, a cancel button) needs to kill the timer.

Returning `Terminate` does not drop the `RefAny` immediately — the framework releases its clone after the callback returns.

## TimerId — the handle

`TimerId` is a `usize` wrapper used to look up and remove timers. IDs in `0x0000`..`0x00FF` are reserved for the framework (`core/src/task.rs:60`). Always create user IDs with `TimerId::unique()`:

```rust,ignore
pub fn unique() -> Self {
    TimerId { id: MAX_TIMER_ID.fetch_add(1, Ordering::SeqCst) }
}
```

The reserved IDs you may encounter:

| Constant | Purpose |
|---|---|
| `CURSOR_BLINK_TIMER_ID` (0x0001) | Caret blink in `<input>` / contenteditable elements. |
| `SCROLL_MOMENTUM_TIMER_ID` (0x0002) | Scroll inertia / smooth scroll animation. |
| `DRAG_AUTOSCROLL_TIMER_ID` (0x0003) | Auto-scrolling when a drag approaches the edge of a scroll container. |
| `TOOLTIP_DELAY_TIMER_ID` (0x0004) | Delay before a hover tooltip appears (`SystemStyle::input_metrics.hover_time_ms`). |

Don't reuse these IDs — `add_timer` with one of them replaces the framework's own timer and breaks the corresponding feature.

## How a tick reaches the callback

1. The event loop computes the next wake-up time as the minimum of every active timer's `instant_of_next_run()` (current time + delay or + interval).
2. The platform shell waits at most until that time.
3. On wake, `LayoutWindow::tick_timers(now)` collects every timer whose next-run is `<= now`.
4. Each timer's `invoke()` is called sequentially. The framework re-checks `delay` and `interval` inside `invoke()` — if the timer is not actually ready, it returns `continue_unchanged()` without running the user callback.
5. Each timer's `CallbackChange`s are applied between calls, so a timer that adds a second timer makes the new one visible to subsequent ticks in the same event-loop iteration.

Implemented in `dll/src/desktop/shell2/common/event.rs::invoke_expired_timers`.

## Duration and Instant

`Instant` and `Duration` live in `core/src/task.rs`. They have two variants each: `System` (wraps `std::time::Instant` / `Duration` on platforms with `std`) and `Tick` (a counter for embedded targets). Mixing variants panics — but since every `Instant` you'll see comes from the framework's own `GetSystemTimeCallback`, all values you compose match.

The constants you'll use most:

```rust,ignore
use azul_core::task::{Duration, SystemTimeDiff};

Duration::System(SystemTimeDiff::from_millis(16))    // 60 fps tick
Duration::System(SystemTimeDiff::from_millis(500))   // 0.5 s
Duration::System(SystemTimeDiff::from_secs(5))       // 5 s
Duration::System(SystemTimeDiff::from_nanos(16_667_000)) // 60 fps, exact
```

`Instant::linear_interpolate(start, end) -> f32` is the single most useful method on `Instant`: given the current time and an `(start, end)` pair, it returns a clamped 0.0..=1.0 fraction. The building block for the [animation runtime](animations.md) once it lands.

## Timers that re-render images, not the DOM

When a timer animates pixels — a video frame, a GL texture, a canvas — re-running the layout pass for every tick is wasteful. Two narrower triggers exist:

- `info.update_all_image_callbacks()` re-invokes every `ImageCallback` in the tree without touching layout. Pair with `TimerCallbackReturn::continue_unchanged()`.
- `info.trigger_virtual_view_rerender(dom_id, node_id)` re-invokes a single `VirtualViewCallback` for lazy-rendered scroll regions.

Both are `CallbackChange`s applied after the callback returns. Use them in preference to `RefreshDom` whenever the DOM structure isn't changing.

## Common patterns

### Run once after a delay

```rust,no_run
# use azul::prelude::*;
# extern crate azul_core;
# use azul_core::task::{Duration, SystemTimeDiff};
extern "C" fn run_once(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    // ... do the deferred work ...
    TimerCallbackReturn::terminate_and_refresh_dom()
}

# extern "C" fn on_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
let timer = Timer::create(data.clone(), run_once, info.get_system_time_fn())
    .with_delay(Duration::System(SystemTimeDiff::from_millis(300)));
info.add_timer(TimerId::unique(), timer);
#     Update::DoNothing
# }
```

### Tick at 60 fps until a flag flips

```rust,no_run
# use azul::prelude::*;
# extern crate azul_core;
# use azul_core::task::{Duration, SystemTimeDiff};
# struct State { running: bool }
extern "C" fn frame(data: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    let state = data.downcast_ref::<State>().unwrap();
    if !state.running {
        TimerCallbackReturn::terminate_unchanged()
    } else {
        TimerCallbackReturn::continue_and_refresh_dom()
    }
}

# extern "C" fn on_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
let timer = Timer::create(data.clone(), frame, info.get_system_time_fn())
    .with_interval(Duration::System(SystemTimeDiff::from_millis(16)));
info.add_timer(TimerId::unique(), timer);
#     Update::DoNothing
# }
```

### Cancel from somewhere else

Stash the `TimerId` in your model when you create the timer; remove it from any callback that owns the model.

```rust,no_run
# use azul::prelude::*;
# struct State { spinner_timer: Option<TimerId> }
# extern "C" fn on_cancel(data: RefAny, mut info: CallbackInfo) -> Update {
let mut state = data.downcast_mut::<State>().unwrap();
if let Some(id) = state.spinner_timer.take() {
    info.remove_timer(id);
}
Update::RefreshDom
# }
```

## Limits

- Timers fire on the UI thread. A callback that takes 50 ms blocks input for 50 ms.
- Tick precision is bounded by the platform timer resolution and by competing input. Don't expect sub-millisecond accuracy.
- A timer's `RefAny` is held alive by the framework until the timer terminates. A timer that holds the only reference to your model keeps the model alive for the lifetime of the timer.
- The `delay`/`interval` schedule is enforced inside `invoke()` — if a system stall bunches several ticks together, the callback fires once per tick, not once per millisecond of elapsed catch-up time. Compute deltas from `frame_start`, not from `call_count`.
