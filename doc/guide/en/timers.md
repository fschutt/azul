---
slug: timers
title: Timers
language: en
canonical_slug: timers
audience: external
maturity: wip
guide_order: 100
topic_only: false
short_desc: Timers, threads, and scheduled work
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
use azul::prelude::*;

extern "C" 
fn tick(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::continue_and_refresh_dom()
}

extern "C" 
fn on_click(mut data: RefAny, mut info: CallbackInfo) -> Update {

    let timer = Timer::create(
        data.clone(), tick, info.get_system_time_fn()
    ).with_interval(Duration::System(
        SystemTimeDiff::from_millis(16)
    ));
    
    info.add_timer(TimerId::unique(), timer);

    Update::DoNothing
}
```

## When to reach for a timer

Use a timer when something must happen on a clock, not in response to input:

- React to a click, hover, or key. Not a timer. Use an event filter (see [events](events.md)).
- Re-paint every frame. Yes. Interval = 16 ms; return `TimerCallbackReturn::continue_and_refresh_dom()`.
- Run something once after a delay. Yes. `with_delay(d)`; terminate from the callback.
- Poll a long-running operation. Not a timer. Use a background thread (covered in [background-tasks](background-tasks.md)).
- Animate a CSS property. Not yet. The animation runtime is a stub; see [animations](animations.md).

Timers run on the UI thread. Heavy work blocks input. For anything I/O-bound or CPU-heavy, spawn a thread.

## The signature

A timer callback has the same C-ABI shape as an event callback, but the second argument and return type differ.

```rust,ignore
pub type TimerCallbackType = extern "C" fn(
    RefAny,
    TimerCallbackInfo,
) -> TimerCallbackReturn;
```

The `RefAny` is whatever you handed to `Timer::create`. It's most often the same data you handed to your layout callback. `TimerCallbackInfo` is documented in the next section. The return value packs two enums:

```rust,ignore
pub struct TimerCallbackReturn {
    pub should_update: Update,
    pub should_terminate: TerminateTimer,
}
```

`Update` is the same enum event callbacks return: `DoNothing`, `RefreshDom`, `RefreshDomAllWindows`. `TerminateTimer` is `Continue` or `Terminate`. Four convenience constructors match the common combinations:

```rust,ignore
TimerCallbackReturn::continue_unchanged()        // keep ticking, no relayout
TimerCallbackReturn::continue_and_refresh_dom()  // keep ticking, re-run layout
TimerCallbackReturn::terminate_unchanged()       // stop, no relayout
TimerCallbackReturn::terminate_and_refresh_dom() // stop, re-run layout
```

## TimerCallbackInfo

The wrapper type gives the callback two things the event API doesn't: which call this is and when the frame started.

- `call_count: usize`. 0 on first invocation, monotonic from there. Useful for "after N ticks, do X".
- `frame_start: Instant`. Monotonic timestamp captured before the callback runs. Use it for animation interpolation rather than calling `Instant::now()` again.
- `is_about_to_finish: bool`. `true` only on the final invocation when a timeout is configured (see below).
- `node_id: OptionDomNodeId`. The node the timer was attached to, if any.
- `callback_info: CallbackInfo`. The full event-API surface.

`TimerCallbackInfo` re-exports the methods you'd reach for from `CallbackInfo` directly: `add_timer`, `remove_timer`, `add_thread`, `remove_thread`, `modify_window_state`, `scroll_to`, `update_all_image_callbacks`, `trigger_virtual_view_rerender`. Mutations are recorded and applied after the callback returns, exactly as in event callbacks.

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
    pub fn with_delay(self, delay: Duration) -> Self;       // wait before first fire
    pub fn with_interval(self, interval: Duration) -> Self; // gap between fires
    pub fn with_timeout(self, timeout: Duration) -> Self;   // stop after this much elapsed
}
```

A timer with no interval set defaults to a short tick. A timer with no timeout runs until the callback returns `TerminateTimer::Terminate` or you call `remove_timer`.

```rust,no_run
use azul::prelude::*;

extern "C" 
fn tick(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::continue_and_refresh_dom()
}

extern "C" 
fn on_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    
    // Run timer after 500ms delay from now, then every 16ms, 
    // for at most 5 seconds.

    let initial_delay = SystemTimeDiff::from_millis(500);
    let interval = SystemTimeDiff::from_millis(16);
    let timeout = SystemTimeDiff::from_secs(5);
    
    let timer = Timer::create(
        data.clone(), 
        tick, 
        info.get_system_time_fn()
    )
    .with_delay(Duration::System(initial_delay))
    .with_interval(Duration::System(interval))
    .with_timeout(Duration::System(timeout));

    info.add_timer(TimerId::unique(), timer);
    
    Update::DoNothing
}
```

## Stopping a timer

Three ways, in order of preference:

1. Return `TerminateTimer::Terminate` from the callback when its work is done. The normal teardown path.
2. Set `with_timeout` when the schedule is bounded. The runtime forces termination on the tick that crosses the deadline; the callback sees `is_about_to_finish == true` on its final call so it can flush state.
3. Call `info.remove_timer(timer_id)` from any other callback when an external event (a window close, a cancel button) needs to kill the timer.

Returning `Terminate` doesn't drop the `RefAny` immediately. The framework releases its clone after the callback returns.

## TimerId — the handle

`TimerId` is a wrapper used to look up and remove timers. The framework reserves the first few IDs for built-in timers (caret blink, scroll momentum, drag autoscroll, tooltip delay). Always create user IDs with `TimerId::unique()`:

```rust,ignore
let id = TimerId::unique();
```

Don't construct a literal `TimerId { id: 0..=4 }`. That collides with the framework's own timers.

## Duration and Instant

`Instant` and `Duration` have two variants each: `System` (wraps `std::time::Instant` / `Duration` on platforms with `std`) and `Tick` (a counter for embedded targets). Mixing variants panics. Every `Instant` you'll see comes from the framework's own time function, so all values you compose match.

The constants you'll use most:

```rust,ignore
use azul_core::task::{Duration, SystemTimeDiff};

Duration::System(SystemTimeDiff::from_millis(16))    // 60 fps tick
Duration::System(SystemTimeDiff::from_millis(500))   // 0.5 s
Duration::System(SystemTimeDiff::from_secs(5))       // 5 s
Duration::System(SystemTimeDiff::from_nanos(16_667_000)) // 60 fps, exact
```

`Instant::linear_interpolate(start, end) -> f32` is the single most useful method on `Instant`. Given the current time and a `(start, end)` pair, it returns a clamped 0.0..=1.0 fraction. It's the building block for the [animation runtime](animations.md) once it lands.

## Timers that re-render images, not the DOM

When a timer animates pixels (a video frame, a GL texture, a canvas), re-running the layout pass for every tick is wasteful. Two narrower triggers exist:

- `info.update_all_image_callbacks()` re-invokes every image callback in the tree without touching layout. Pair with `TimerCallbackReturn::continue_unchanged()`.
- `info.trigger_virtual_view_rerender(dom_id, node_id)` re-invokes a single virtual-view callback for lazy-rendered scroll regions.

Both are applied after the callback returns. Use them in preference to `RefreshDom` whenever the DOM structure isn't changing.

## Common patterns

### Run once after a delay

```rust,no_run
use azul::prelude::*;

extern "C" 
fn run_once(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    // ... do the deferred work ...
    TimerCallbackReturn::terminate_and_refresh_dom()
}

extern "C" fn on_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    
    let timer = Timer::create(
        data.clone(), 
        run_once, 
        info.get_system_time_fn()
    )
    .with_delay(Duration::System(SystemTimeDiff::from_millis(300)));
    
    info.add_timer(TimerId::unique(), timer);
    
    Update::DoNothing
}
```

### Tick at 60 fps until a flag flips

```rust,no_run
use azul::prelude::*;

struct State { running: bool }

extern "C" 
fn frame(data: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    let state = data.downcast_ref::<State>().unwrap();
    if !state.running {
        TimerCallbackReturn::terminate_unchanged()
    } else {
        TimerCallbackReturn::continue_and_refresh_dom()
    }
}

extern "C" 
fn on_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    
    let timer = Timer::create(
        data.clone(), 
        frame, 
        info.get_system_time_fn()
    )
    .with_interval(Duration::System(SystemTimeDiff::from_millis(16)));
    
    info.add_timer(TimerId::unique(), timer);

    Update::DoNothing
}
```

### Cancel from somewhere else

Stash the `TimerId` in your model when you create the timer; remove it from any callback that owns the model.

```rust,no_run
use azul::prelude::*;

struct State { spinner_timer: Option<TimerId> }

extern "C" 
fn on_cancel(data: RefAny, mut info: CallbackInfo) -> Update {
    let mut state = data.downcast_mut::<State>().unwrap();
    if let Some(id) = state.spinner_timer.take() {
        info.remove_timer(id);
    }
    Update::RefreshDom
}
```

## Limits

- Timers fire on the UI thread. A callback that takes 50 ms blocks input for 50 ms.
- Tick precision is bounded by the platform timer resolution and by competing input. Don't expect sub-millisecond accuracy.
- A timer's `RefAny` is held alive by the framework until the timer terminates. A timer that holds the only reference to your model keeps the model alive for the lifetime of the timer.
- The schedule is enforced per tick, not per millisecond. If a system stall bunches several ticks together, the callback fires once per tick. Compute deltas from `frame_start`, not from `call_count`.

## Coming Up Next

- [Animations](animations.md) — CSS transitions and @keyframes
- [Background Tasks](background-tasks.md) — Running long jobs off the layout thread
- [Networking](networking.md) — HTTP from a callback
