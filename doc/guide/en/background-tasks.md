---
slug: background-tasks
title: Background Tasks
language: en
canonical_slug: background-tasks
audience: external
maturity: wip
guide_order: 260
topic_only: false
prerequisites: [timers]
tracked_files:
  - core/src/task.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T12:00:00Z
---

# Background Tasks

> **WIP** — the API shape is stable, but `Thread`-driven examples may need
> minor adjustments as the framework evolves.

`Thread` is azul's background-work primitive: an OS thread plus two typed
channels back to the main thread, owned and ticked by the event loop. Use
it for blocking I/O, long computations, and anything you cannot finish in
a single frame budget. The framework polls each registered `Thread` every
frame, drains messages, and runs your write-back callbacks on the main
thread.

There is no embedded async runtime. The framework gives you threads and
timers; if you want futures, you bring the runtime.

## When to use a thread vs. a timer

| Need | Use |
|---|---|
| Blocking I/O (file, network, DB), long compute | `Thread` |
| Periodic main-thread work (animations, polling) | `Timer` |
| Wait for a future without a runtime | `Timer` polling a `Mutex<Option<T>>` |

`Timer` runs on the main thread and is bounded by your frame budget — see
[timers](timers.md). A `Thread` runs in its own OS thread and reports back
through a channel.

## Spawning a thread

```rust,no_run
# use azul::prelude::*;
extern "C" fn worker(
    initial: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    // blocking work goes here
}

extern "C" fn on_click(mut data: RefAny, mut event: CallbackInfo) -> Update {
    let init_data     = RefAny::new(/* per-thread input */ ());
    let writeback_data = data.clone();      // same-typed handle the
                                            // writeback callback receives
    let thread = Thread::create(init_data, writeback_data, worker);
    event.add_thread(ThreadId::unique(), thread);
    Update::DoNothing
}
```

`Thread::create(thread_initialize_data, writeback_data, callback)` takes:

- **`thread_initialize_data`** — moved into the thread, available as the
  first `RefAny` argument of `worker`. Use this for inputs the thread
  needs but the main side does not.
- **`writeback_data`** — kept on the main side; passed back to every
  `WriteBackCallback` invocation. This is the handle the callback uses to
  *mutate* application state in response to the thread's output.
- **`callback`** — the `extern "C" fn(RefAny, ThreadSender, ThreadReceiver)`
  that runs on the new thread.

`event.add_thread(ThreadId::unique(), thread)` hands the thread to the
event loop. From this point the framework polls it every frame.

## Sending data back: `WriteBackCallback`

The thread cannot touch main-thread state directly. To update application
data, it sends a `WriteBack` message containing a payload (`RefAny`) and a
callback that runs on the main thread:

```rust,no_run
# use azul::prelude::*;
# use azul::option::OptionRefAny;
struct Loaded { rows: Vec<u32> }

extern "C" fn apply_loaded(
    mut app: RefAny,
    mut payload: RefAny,
    _info: CallbackInfo,
) -> Update {
    let mut model = match app.downcast_mut::<MyModel>() { Some(m) => m, None => return Update::DoNothing };
    let loaded    = match payload.downcast_mut::<Loaded>() { Some(p) => p, None => return Update::DoNothing };
    model.rows.append(&mut loaded.rows);
    Update::RefreshDom
}

extern "C" fn worker(
    initial: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    let rows = blocking_query();
    let msg  = ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany:   RefAny::new(Loaded { rows }),
        callback: WriteBackCallback { cb: apply_loaded, ctx: OptionRefAny::None },
    });
    sender.send(msg);
}
# struct MyModel { rows: Vec<u32> }
# fn blocking_query() -> Vec<u32> { Vec::new() }
```

The callback signature is fixed:

```rust,ignore
extern "C" fn(
    /* app side, the writeback_data from Thread::create */ RefAny,
    /* thread side, the payload from ThreadWriteBackMsg */ RefAny,
    CallbackInfo,
) -> Update;
```

Return `Update::RefreshDom` to trigger a re-layout. `Update::DoNothing`
keeps the existing DOM.

## Returning `Update` directly

For the no-payload case — a thread that just wants to tell the UI to
refresh — send `ThreadReceiveMsg::Update`:

```rust,no_run
# use azul::prelude::*;
# fn _stub(mut sender: ThreadSender) {
sender.send(ThreadReceiveMsg::Update(Update::RefreshDom));
# }
```

The framework applies the `Update` value verbatim, no callback runs.

## Receiving messages from the main thread

`ThreadReceiver::recv()` is non-blocking and returns
`OptionThreadSendMsg`. The main thread sends three kinds of message:

| Variant | Meaning |
|---|---|
| `ThreadSendMsg::Tick` | One frame elapsed — opportunity to check progress, send a chunk back |
| `ThreadSendMsg::TerminateThread` | The framework is dropping the thread; finish quickly |
| `ThreadSendMsg::Custom(RefAny)` | App-defined message |

The framework sends `Tick` automatically on each frame and `TerminateThread`
when the thread is removed via `CallbackInfo::remove_thread` or the
`Thread` handle is dropped. `Custom` is only delivered if your code
arranges for it.

## Cooperative termination

A long-running thread should poll for `TerminateThread` between work
units:

```rust,no_run
# use azul::prelude::*;
# fn _stub(mut recv: ThreadReceiver, items: Vec<u32>) {
for item in items {
    if recv.recv().into_option() == Some(ThreadSendMsg::TerminateThread) {
        return;
    }
    process(item);
}
# }
# fn process(_: u32) {}
```

If the thread does not check, it runs to completion regardless. The
framework's destructor sends `TerminateThread` and then `join()`s — a
non-cooperative thread blocks teardown until its callback returns.

## Cancelling from the main thread

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_cancel(mut data: RefAny, mut event: CallbackInfo) -> Update {
    let mut m = data.downcast_mut::<MyModel>().unwrap();
    if let Some(id) = m.thread_id.take() {
        event.remove_thread(id);
    }
    Update::RefreshDom
}
# struct MyModel { thread_id: Option<ThreadId> }
```

`remove_thread` schedules the same `TerminateThread` + drop sequence the
destructor runs.

## Polling without a thread

For non-blocking work that just needs to wake up periodically — for
example, polling a `Mutex<Option<T>>` shared with code outside azul — a
[`Timer`](timers.md) is enough. Timers run on the main thread, so any
`Update::RefreshDom` they return takes effect on the next frame without a
write-back step.

## Sleeping inside a thread

```rust,ignore
Thread::sleep_ms(milliseconds);   // alias of std::thread::sleep
Thread::sleep_us(microseconds);
Thread::sleep_ns(nanoseconds);
```

These are FFI-safe wrappers around `std::thread::sleep`. They exist
because non-Rust bindings cannot call `std::thread::sleep` directly.
Inside a Rust callback `std::thread::sleep` works equally well.

## `Instant` and `Duration`

The thread API uses `azul_core::task::Instant` and `Duration` rather than
the std types directly, so timing logic compiles on `no_std` targets.

```rust,no_run
# use azul::prelude::*;
let now      = Instant::now();
let interval = Duration::from_millis(250);
```

On `std` targets, `Instant::System` wraps `std::time::Instant`. On
embedded / WASM targets that lack a real-time clock the variant is
`Instant::Tick(SystemTick)` — a frame counter you advance from your
event loop. `Duration` mirrors the split: `Duration::System` and
`Duration::Tick`. Mixing variants panics, so pick one per platform and
stay consistent.

`Instant` exposes:

- `Instant::now()` — current time on the active variant.
- `duration_since(&earlier) -> Duration` — panics if `earlier > self`.
- `linear_interpolate(start, end) -> f32` — clamped 0.0–1.0, useful for
  animation progress.
- `add_optional_duration(Option<&Duration>) -> Self` — additive offsets.

## Reserved timer IDs

Some `TimerId` values are reserved for framework-internal use; user
timers must use IDs ≥ `0x0100`. Calling `TimerId::unique()` always
produces a user-range ID, so you only need this if you are constructing
`TimerId { id: ... }` literals manually:

| ID | Purpose |
|---|---|
| `0x0001` | `CURSOR_BLINK_TIMER_ID` — caret blink in editable text |
| `0x0002` | `SCROLL_MOMENTUM_TIMER_ID` — scroll inertia |
| `0x0003` | `DRAG_AUTOSCROLL_TIMER_ID` — auto-scroll near edges during drag |
| `0x0004` | `TOOLTIP_DELAY_TIMER_ID` — hover-to-tooltip delay |

`ThreadId` reserves the first five IDs (0–4) for framework use;
`ThreadId::unique()` skips past them.

## What you cannot do today

- **Run `async fn` directly.** The framework does not provide an executor.
  To use Tokio, futures-rs, or smol, spawn a `Thread`, build a
  `Runtime` inside it, and use `WriteBackCallback` to surface results.
- **Stream raw socket data.** Networking has its own page —
  see [networking](networking.md). Until it lands, use a `Thread` plus
  `std::net::TcpStream` for the same shape as the example above.
- **Share `&mut` references between thread and main.** Communication is
  `RefAny` payloads only. Lock-free shared state is not part of the
  framework — wrap a `Mutex` inside `RefAny` if you need it.
