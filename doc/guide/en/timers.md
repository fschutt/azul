---
slug: timers
title: Timers
language: en
canonical_slug: timers
audience: external
maturity: wip
guide_order: 100
topic_only: false
prerequisites: [events]
tracked_files:
  - core/src/task.rs
  - core/src/callbacks.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:34:08Z
---

# Timers

> **WIP** — `Timer` and `TimerCallbackReturn` are stable; the host loop tick semantics may shift if the framework moves to a per-window event-loop scheduler.

A `Timer` is a function the framework runs on the main thread on a recurring schedule. You register it with the window via `CallbackInfo::add_timer`; the host event loop wakes it on its interval and invokes the callback. The return value tells the framework whether to refresh the DOM and whether to keep the timer alive.

```rust,no_run
# use azul::prelude::*;
extern "C" fn tick(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::continue_and_refresh_dom()
}
```

## The timer signature

A timer callback has the same `RefAny`-erased shape as any other callback, but returns `TimerCallbackReturn` instead of `Update`:

```rust,ignore
extern "C" fn(RefAny, TimerCallbackInfo) -> TimerCallbackReturn
```

`TimerCallbackReturn` carries two flags (`core/src/callbacks.rs:441`):

```rust,ignore
pub struct TimerCallbackReturn {
    pub should_update: Update,
    pub should_terminate: TerminateTimer,
}
```

`should_update` is the same `Update` enum used elsewhere (`DoNothing`, `RefreshDom`, `RefreshDomAllWindows`). `should_terminate` is `TerminateTimer::Continue` to keep the timer running, or `TerminateTimer::Terminate` to remove it. Convenience constructors:

```rust,ignore
TimerCallbackReturn::continue_unchanged()       // DoNothing + Continue
TimerCallbackReturn::continue_and_refresh_dom() // RefreshDom + Continue
TimerCallbackReturn::terminate_unchanged()      // DoNothing + Terminate
TimerCallbackReturn::terminate_and_refresh_dom()// RefreshDom + Terminate
```

## Registering a timer

Inside any callback, build a `Timer` and pass it to `info.add_timer(id, timer)`:

```rust,no_run
# use azul::prelude::*;
# struct App { ticks: usize }
extern "C" fn on_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let timer = Timer::create(data.clone(), tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(33)));
    info.add_timer(TimerId::unique(), timer);
    Update::DoNothing
}

extern "C" fn tick(mut data: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    if let Some(mut a) = data.downcast_mut::<App>() {
        a.ticks += 1;
    }
    TimerCallbackReturn::continue_and_refresh_dom()
}
```

`Timer::create` takes the `RefAny` to pass to the callback, the function pointer, and a system-time callback (used to record the timer's creation instant). `info.get_system_time_fn()` returns the framework-provided implementation.

The change is queued and applied after the callback returns. The next event-loop iteration picks the timer up and starts ticking it.

## Tuning the schedule

`Timer` exposes three optional knobs as builder methods:

| method | meaning | default |
|---|---|---|
| `.with_delay(d)` | Wait `d` before the first invocation. | fire on the next tick |
| `.with_interval(d)` | Minimum gap between invocations. | every frame (~10ms) |
| `.with_timeout(d)` | Force-terminate after `d` since creation. | never |

```rust,no_run
# use azul::prelude::*;
# struct State;
# extern "C" fn cb(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn { TimerCallbackReturn::continue_unchanged() }
# fn build(data: RefAny, sys: GetSystemTimeCallback) {
let timer = Timer::create(data, cb, sys)
    .with_delay(Duration::System(SystemTimeDiff::from_millis(500)))
    .with_interval(Duration::System(SystemTimeDiff::from_millis(16)))
    .with_timeout(Duration::System(SystemTimeDiff::from_secs(10)));
# }
```

A timer with `with_interval(16ms)` fires up to 60 times per second. The host loop is frame-driven; a long callback delays the next tick. Don't sleep or block inside a timer callback — start a `Thread` instead (covered in `background-tasks.md`).

When the timeout elapses, the framework forces `should_terminate = Terminate` regardless of what the callback returned.

## `Duration` and `Instant`

Time types live in `core/src/task.rs`. They have two variants each:

```rust,ignore
pub enum Instant  { System(InstantPtr),    Tick(SystemTick) }
pub enum Duration { System(SystemTimeDiff), Tick(SystemTickDiff) }
```

`System` uses `std::time::Instant`; `Tick` is a counter for embedded targets without a real-time clock. On desktop you only ever see the `System` variant.

Construct durations with the typed constructors:

```rust,ignore
SystemTimeDiff::from_millis(16)   // 16 ms
SystemTimeDiff::from_secs(2)      // 2 s
SystemTimeDiff::from_nanos(1_000) // 1 µs
```

`Instant::now()` returns the current instant on `std`-targets and `Instant::Tick(SystemTick::new(0))` elsewhere.

## Inside the callback

`TimerCallbackInfo` (`layout/src/timer.rs:278`) wraps a regular `CallbackInfo` plus timer-specific fields:

```rust,ignore
pub struct TimerCallbackInfo {
    pub callback_info: CallbackInfo,
    pub node_id: OptionDomNodeId,    // node this timer was attached to (if any)
    pub frame_start: Instant,         // when the host began this frame
    pub call_count: usize,            // 0-based, increments per fire
    pub is_about_to_finish: bool,     // true on the last invocation before timeout
    // ...
}
```

The full `CallbackInfo` API (focus, scroll, mutations) is reachable via `info.get_callback_info_mut()`. Use `frame_start` for animation interpolation; use `call_count` for one-time setup on the first tick:

```rust,no_run
# use azul::prelude::*;
extern "C" fn tick(_: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    if info.call_count == 0 {
        // first run
    }
    if info.is_about_to_finish {
        // last run before timeout
    }
    TimerCallbackReturn::continue_unchanged()
}
```

## Removing a timer

Two options:

1. **Inside the callback**: return a `should_terminate` of `TerminateTimer::Terminate`.
2. **From any other callback**: call `info.remove_timer(timer_id)`. Save the `TimerId` returned by `TimerId::unique()` somewhere your handler can reach it (typically inside your `RefAny`).

```rust,no_run
# use azul::prelude::*;
# struct App { timer: Option<TimerId> }
extern "C" fn on_stop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(a) = data.downcast_ref::<App>() {
        if let Some(id) = a.timer {
            info.remove_timer(id);
        }
    }
    Update::DoNothing
}
```

A timer also dies when the window closes. The `RefAny` it holds drops at that point.

## Reserved timer IDs

User timers start at `0x0100`. IDs `0x0001..0x00FF` are reserved for framework-internal timers (`core/src/task.rs:65`):

| ID | name | purpose |
|---|---|---|
| `0x0001` | `CURSOR_BLINK_TIMER_ID` | Caret blink in `contenteditable` |
| `0x0002` | `SCROLL_MOMENTUM_TIMER_ID` | Inertia / fling scroll |
| `0x0003` | `DRAG_AUTOSCROLL_TIMER_ID` | Auto-scroll while drag is near edge |
| `0x0004` | `TOOLTIP_DELAY_TIMER_ID` | Hover-to-tooltip delay |

`TimerId::unique()` allocates the next free user ID. Don't construct a `TimerId { id: ... }` literal in user code.

## Common patterns

**Re-render on a clock tick**:

```rust,no_run
# use azul::prelude::*;
# struct Clock;
extern "C" fn tick(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::continue_and_refresh_dom()
}
```

The `RefreshDom` return causes the layout callback to re-run; reading `Instant::now()` inside layout produces a wall clock that updates every interval.

**One-shot delayed action**:

```rust,no_run
# use azul::prelude::*;
extern "C" fn fire_once(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    // ... do the thing ...
    TimerCallbackReturn::terminate_and_refresh_dom()
}
```

Pair with `.with_delay(...)` and no `.with_interval(...)`, or use `.with_timeout(d)` so the framework forces termination.

**Polling external state**:

```rust,no_run
# use azul::prelude::*;
# struct App { last_check: Option<Instant> }
extern "C" fn poll(mut data: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    // read external value, decide whether to refresh
    TimerCallbackReturn::continue_and_refresh_dom()
}
```

For genuinely blocking work (file I/O, network), don't poll — spawn a `Thread` and let it message the main loop on completion.

## What is not covered

- Background threads with `ThreadSendMsg` — see `background-tasks.md`.
- Frame-locked animation tweens — see [Animations](animations.md).
- Scroll momentum (uses a reserved internal timer) — see [Scrolling and Drag-and-Drop](scrolling-and-drag.md).

## Next

- [Animations](animations.md) — the planned animation runtime built on timers.
- [Scrolling and Drag-and-Drop](scrolling-and-drag.md) — scroll events.
- [Windows, Menus, Decorations](windowing.md) — multi-window apps.
