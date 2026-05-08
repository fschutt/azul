---
slug: background-tasks
title: Background Tasks
language: en
canonical_slug: background-tasks
audience: external
maturity: wip
guide_order: 260
topic_only: false
short_desc: Running long jobs off the layout thread
prerequisites: [timers]
tracked_files:
  - core/src/task.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Background Tasks

> WIP. `Thread::create`, `ThreadSender`, and `ThreadReceiver` are stable. Channel ergonomics may shift if the framework grows a typed message bus on top of `RefAny` payloads.

`Thread` is azul's background-work primitive: an OS thread plus two typed channels back to the main thread, owned and ticked by the event loop. Use it for blocking I/O, long computations, and anything you can't finish inside a single frame budget. The framework polls each registered `Thread` every frame, drains messages, and runs your write-back callbacks on the main thread.

There's no embedded async runtime. The framework gives you threads and [timers](timers.md). If you want futures, you bring the runtime.

## When to use a thread vs. a timer

- Blocking I/O (file, network, DB) or long compute. Use `Thread`.
- Periodic main-thread work (animation, polling app state). Use `Timer`.
- Wait for a future without a runtime. Use `Thread` running a current-thread executor.

A `Timer` runs on the main thread and is bounded by your frame budget; see [timers](timers.md). A `Thread` runs in its own OS thread and reports back through a channel.

## Spawning a thread

The worker function signature is fixed:

```rust,ignore
pub type ThreadCallbackType =
    extern "C" fn(RefAny, ThreadSender, ThreadReceiver);
```

Build a `Thread` with `Thread::create` and hand it to the event loop via `CallbackInfo::add_thread`:

```rust,no_run
use azul::prelude::*;

extern "C" fn worker(
    initial: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    // blocking work goes here
}

extern "C" fn on_click(mut data: RefAny, mut event: CallbackInfo) -> Update {
    let init_data      = RefAny::new(/* per-thread input */ ());
    let writeback_data = data.clone();
    let thread = Thread::create(init_data, writeback_data, worker);
    event.add_thread(ThreadId::unique(), thread);
    Update::DoNothing
}
```

`Thread::create(thread_initialize_data, writeback_data, callback)` takes:

- `thread_initialize_data`. Moved into the worker. Available as the first `RefAny` argument of the worker function. Use this for inputs the thread needs but the main side doesn't.
- `writeback_data`. Kept on the main side; passed back to every `WriteBackCallback` invocation. This is the handle the callback uses to mutate application state in response to the thread's output.
- `callback`. The `extern "C" fn(RefAny, ThreadSender, ThreadReceiver)` that runs on the new thread.

`event.add_thread(ThreadId::unique(), thread)` hands the thread to the event loop. From this point the framework polls it every frame.

## Sending data back: WriteBackCallback

The thread can't touch main-thread state directly. To update application data, send a `ThreadReceiveMsg::WriteBack` message. The payload is a `RefAny` plus a callback that runs on the main thread:

```rust,ignore
pub type WriteBackCallbackType =
    extern "C" fn(RefAny, RefAny, CallbackInfo) -> Update;
```

Full example:

```rust,no_run
use azul::prelude::*;

struct Loaded { rows: Vec<u32> }
struct MyModel { rows: Vec<u32> }

extern "C" fn apply_loaded(
    mut app: RefAny,
    mut payload: RefAny,
    _info: CallbackInfo,
) -> Update {
    let mut model = match app.downcast_mut::<MyModel>() {
        Some(m) => m, None => return Update::DoNothing,
    };
    let mut loaded = match payload.downcast_mut::<Loaded>() {
        Some(p) => p, None => return Update::DoNothing,
    };
    model.rows.append(&mut loaded.rows);
    Update::RefreshDom
}

extern "C" fn worker(
    _initial: RefAny,
    mut sender: ThreadSender,
    mut _recv: ThreadReceiver,
) {
    let rows = blocking_query();
    let msg  = ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany:   RefAny::new(Loaded { rows }),
        callback: WriteBackCallback {
            cb: apply_loaded,
            ctx: OptionRefAny::None,
        },
    });
    sender.send(msg);
}
# fn blocking_query() -> Vec<u32> { Vec::new() }
```

The first `RefAny` argument of `apply_loaded` is the `writeback_data` passed to `Thread::create`; the second is the payload the worker sent. Return `Update::RefreshDom` to trigger a re-layout. `Update::DoNothing` keeps the existing DOM.

`sender.send` returns `bool`. `true` if the message was queued, `false` if the channel is closed (the framework has already torn the thread down).

## Returning Update directly

For the no-payload case (a thread that just wants to tell the UI to refresh) send `ThreadReceiveMsg::Update`:

```rust,no_run
use azul::prelude::*;
fn _stub(mut sender: ThreadSender) {
  sender.send(ThreadReceiveMsg::Update(Update::RefreshDom));
}
```

The framework applies the `Update` value verbatim. No callback runs.

## Receiving messages from the main thread

`ThreadReceiver::recv()` is non-blocking and returns `OptionThreadSendMsg`. The main thread sends three kinds of message:

- `ThreadSendMsg::Tick`. One frame elapsed. An opportunity to check progress, send a chunk back.
- `ThreadSendMsg::TerminateThread`. The framework is dropping the thread; finish quickly.
- `ThreadSendMsg::Custom(RefAny)`. App-defined message.

`Tick` arrives automatically on each frame. `TerminateThread` is sent when the thread is removed via `CallbackInfo::remove_thread` or the owning `Thread` handle is dropped. `Custom` is only delivered if your code arranges for it.

## Cooperative termination

A long-running thread should poll for `TerminateThread` between work units:

```rust,no_run
use azul::prelude::*;

fn _stub(mut recv: ThreadReceiver, items: Vec<u32>) {
  for item in items {
    if let OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) = recv.recv() {
        return;
    }
    process(item);
  }
}

fn process(_: u32) { }
```

If the worker doesn't check, it runs to completion regardless. The framework's destructor sends `TerminateThread` and then joins. A non-cooperative thread blocks teardown until its callback returns.

## Cancelling from the main thread

```rust,no_run
use azul::prelude::*;
struct MyModel { 
    thread_id: Option<ThreadId> 
}

extern "C" 
fn on_cancel(mut data: RefAny, mut event: CallbackInfo) -> Update {
    let mut m = match data.downcast_mut::<MyModel>() {
        Some(m) => m, None => return Update::DoNothing,
    };
    if let Some(id) = m.thread_id.take() {
        event.remove_thread(id);
    }
    Update::RefreshDom
}
```

`remove_thread` schedules the same `TerminateThread` + drop sequence the destructor runs.

## Sleeping inside a thread

```rust,ignore
Thread::sleep_ms(milliseconds);
Thread::sleep_us(microseconds);
Thread::sleep_ns(nanoseconds);
```

These are FFI-safe wrappers around `std::thread::sleep`. They exist so non-Rust bindings can sleep. Inside a Rust callback `std::thread::sleep` works equally well.

## Instant and Duration

The thread API uses `Instant` and `Duration` from the framework rather than the std types directly so timing logic compiles on `no_std` targets. Both are two-variant enums:

```rust,ignore
pub enum Instant  { System(InstantPtr),    Tick(SystemTick) }
pub enum Duration { System(SystemTimeDiff), Tick(SystemTickDiff) }
```

On `std` targets, `Instant::System` wraps `std::time::Instant`. On embedded or WASM targets that lack a real-time clock the variant is `Instant::Tick(SystemTick)`, a frame counter you advance from your event loop. Mixing variants panics, so pick one per platform and stay consistent.

There's no `Duration::from_millis` shorthand. Build the `SystemTimeDiff` explicitly:

```rust,ignore
Duration::System(SystemTimeDiff::from_millis(250))
Duration::System(SystemTimeDiff::from_secs(2))
Duration::System(SystemTimeDiff::from_nanos(1_000))
```

`Instant` exposes:

- `Instant::now()`. Current time on the active variant.
- `duration_since(&earlier) -> Duration`. Panics if `earlier > self` or if the variants don't match.
- `linear_interpolate(start, end) -> f32`. Clamped 0.0–1.0, useful for animation progress.
- `add_duration(...) -> Self`. Additive offsets.

## Reserved thread IDs

`ThreadId` reserves the first few IDs for framework-internal use. `ThreadId::unique()` skips past them, so user code never collides. Don't construct a literal `ThreadId { id: 0..=4 }` in user code.

## What you can't do today

- Run `async fn` directly. The framework doesn't provide an executor. To use Tokio, futures-rs, or smol, spawn a `Thread`, build a `Runtime` inside it, and use `WriteBackCallback` to surface results.
- Stream raw socket data through the framework. Networking has its own page; see [networking](networking.md). Until the runtime side lands, use a `Thread` plus `std::net::TcpStream` for the same shape as the example above.
- Share `&mut` references between thread and main. Communication is `RefAny` payloads only. Lock-free shared state isn't part of the framework. Wrap a `Mutex` inside `RefAny` if you need it.

## Coming Up Next

- [Networking](networking.md) — HTTP from a callback
- [Timers](timers.md) — Timers, threads, and scheduled work
- [Events](events.md) — Callbacks, event filters, and how state triggers relayout
