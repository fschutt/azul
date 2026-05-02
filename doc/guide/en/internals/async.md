---
slug: internals/async
title: Async, Timers, Threading
language: en
canonical_slug: internals/async
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - core/src/task.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:40:13Z
---

> **WIP**: the runtime is functional but `tick_timers` (`layout/src/window.rs:1977`) currently returns every timer ID instead of filtering by readiness. Per-timer gating happens inside `Timer::invoke`.

The async runtime in `core/src/task.rs` is the FFI-safe substrate for two
runtime systems that the layout crate builds on top: **timers** (callbacks
that fire from the main event loop on a clock) and **threads** (background
work that posts results back to the UI thread). Both systems are owned per
window — `LayoutWindow.timers: BTreeMap<TimerId, Timer>` and
`LayoutWindow.threads: BTreeMap<ThreadId, Thread>` — and are driven by the
platform shell (`dll/src/desktop/shell2/`) once per event-loop turn.

`task.rs` itself defines only the FFI primitives: ID types, time types,
thread channel ABI. The runtime — `Timer`, `Thread`, the `WriteBackCallback`
mechanism, `LayoutWindow::run_all_threads` — lives in `layout/src/timer.rs`,
`layout/src/thread.rs`, and `layout/src/window.rs`.

## ID allocation

Timer and thread IDs are monotonic atomic counters with reserved low
ranges for framework use:

| Range | Use |
|---|---|
| `TimerId { id: 0x0000..0x00FF }` | Reserved system timers |
| `TimerId { id: 0x0100.. }` | User timers (`TimerId::unique()`) |
| `ThreadId { id: 0..5 }` | Reserved (currently unused) |
| `ThreadId { id: 5.. }` | User threads (`ThreadId::unique()`) |

`USER_TIMER_ID_START = 0x0100` and `RESERVED_THREAD_ID_COUNT = 5` are the
gates (`core/src/task.rs:78,103`). Because both counters are
`AtomicUsize::fetch_add(1, SeqCst)`, IDs are unique across threads and
across windows.

### Reserved timer IDs

| ID | Name | Purpose |
|---|---|---|
| `0x0001` | `CURSOR_BLINK_TIMER_ID` | Caret blink in `contenteditable` (~530 ms) |
| `0x0002` | `SCROLL_MOMENTUM_TIMER_ID` | Inertia/flick animation |
| `0x0003` | `DRAG_AUTOSCROLL_TIMER_ID` | Edge auto-scroll during drag |
| `0x0004` | `TOOLTIP_DELAY_TIMER_ID` | Hover-delay before tooltip shows |

Defined at `core/src/task.rs:46-74`. Wiring is partial: the cursor blink
and scroll-momentum timers are driven by the platform event loop;
`DRAG_AUTOSCROLL_TIMER_ID` is referenced in `dll/src/desktop/shell2/common/event.rs`
but the autoscroll body is not yet implemented. There is **no**
`DOUBLE_CLICK_TIMER_ID` — double-click detection lives in
`GestureManager::detect_double_click`.

## Time types

`Instant` and `Duration` are both two-variant enums covering std and
embedded targets:

```rust,ignore
#[repr(C, u8)]
pub enum Instant {
    System(InstantPtr),    // wraps std::time::Instant, requires "std" feature
    Tick(SystemTick),      // u64 tick counter, no_std fallback
}

#[repr(C, u8)]
pub enum Duration {
    System(SystemTimeDiff),  // (secs: u64, nanos: u32) — mirrors std::time::Duration
    Tick(SystemTickDiff),    // tick_diff: u64
}
```

Mixing variants panics: `Instant::System.duration_since(Instant::Tick)`
hits the `_ => panic!(...)` arm at `core/src/task.rs:262`, as does
adding a `Duration::Tick` to an `Instant::System`. The convention is that
a runtime picks one variant at startup (via `GetSystemTimeCallback`) and
stays on it.

### `InstantPtr` — FFI-safe wrapper around `std::time::Instant`

`std::time::Instant` is opaque and not `#[repr(C)]`, so `InstantPtr`
boxes it and carries clone/destructor function pointers so that the
struct can cross the C ABI:

```rust,ignore
#[repr(C)]
pub struct InstantPtr {
    pub ptr: Box<StdInstant>,
    pub clone_fn: InstantPtrCloneCallback,
    pub destructor: InstantPtrDestructorCallback,
    pub run_destructor: bool,
}
```

The default `clone_fn` is `std_instant_clone` and the default destructor
is the no-op `std_instant_drop` (the box's own destructor handles
deallocation). When constructed via `From<StdInstant>`, both are wired
automatically (`core/src/task.rs:380-403`).

`run_destructor` is set to `false` after the destructor fires once, so
moving an `InstantPtr` through FFI without an explicit clone does not
double-free.

### `GetSystemTimeCallback`

```rust,ignore
pub type GetSystemTimeCallbackType = extern "C" fn() -> Instant;

pub extern "C" fn get_system_time_libstd() -> Instant {
    StdInstant::now().into()  // panics on wasm32, falls back to Tick(0)
}
```

The runtime never calls `Instant::now()` directly — it calls the
configured `GetSystemTimeCallback` so that embedded targets and WASM
(where `std::time::Instant::now()` panics) get a sensible fallback. The
desktop shell wires `get_system_time_libstd`; the web backend uses the
same function via `ExternalSystemCallbacks::rust_internal()`.

### `Duration::greater_than` / `smaller_than`

`Duration` derives `PartialOrd` and `Ord`, so `>` and `<` already work.
The named methods (`core/src/task.rs:526-571`) duplicate this with
explicit panic branches for mismatched variants. They predate the derived
impls; new code should prefer the comparison operators.

## Timer system

The `Timer` struct lives in `layout/src/timer.rs:111`:

```rust,ignore
#[repr(C)]
pub struct Timer {
    pub refany: RefAny,
    pub node_id: OptionDomNodeId,
    pub created: Instant,
    pub last_run: OptionInstant,
    pub run_count: usize,
    pub delay: OptionDuration,
    pub interval: OptionDuration,
    pub timeout: OptionDuration,
    pub callback: TimerCallback,
}
```

Three timing knobs:

- `delay` — wait this long before the *first* run.
- `interval` — minimum gap between runs after the first.
- `timeout` — total lifetime; once exceeded, the next invocation
  forces `TerminateTimer::Terminate`.

All three are checked inside `Timer::invoke` (`layout/src/timer.rs:194`).
If the timer is not ready, it returns
`TimerCallbackReturn { should_update: DoNothing, should_terminate: Continue }`
without firing the user callback. If the timer is past its `timeout`,
it fires once more and then returns `Terminate`.

### Lifecycle

```text
                                 ┌─ Timer::invoke ──────────────────────┐
add_timer(id, timer) ──┐         │ now = get_system_time_fn()           │
                       │         │ if last_run.is_none()                │
LayoutWindow.timers   ─┴─ tick ──┤   && delay not elapsed: return idle  │
                                 │ if interval not elapsed: return idle │
                                 │ if past timeout:                     │
                                 │   force should_terminate = Terminate │
                                 │ run user callback                    │
                                 │ run_count += 1; last_run = Some(now) │
                                 └──────────────────────────────────────┘
                                                │
                                                ▼
                                  apply_user_change(CallbackChange)
                                  if Terminate → RemoveTimer
```

The platform shell calls `tick_timers` and `Timer::invoke` once per
event-loop turn through `LayoutWindow::run_all_timers` (the existing
`tick_timers` at `layout/src/window.rs:1977` returns all IDs unfiltered;
the per-timer readiness check happens inside `invoke`).

### `time_until_next_timer_ms`

```rust,ignore
pub fn time_until_next_timer_ms(
    &self,
    get_system_time_fn: &GetSystemTimeCallback,
) -> Option<u64>
```

Defined at `layout/src/window.rs:1999`. Returns the minimum number of
milliseconds until any timer's `instant_of_next_run()` arrives, or `None`
if there are no timers (in which case the caller may block
indefinitely). The Linux X11 and Wayland backends pass this to
`poll`/`epoll_wait` so they don't busy-loop at 16 ms when nothing is
pending.

### `TimerCallbackInfo`

```rust,ignore
#[repr(C)]
pub struct TimerCallbackInfo {
    pub callback_info: CallbackInfo,
    pub node_id: OptionDomNodeId,
    pub frame_start: Instant,
    pub call_count: usize,
    pub is_about_to_finish: bool,
    pub _abi_ref: *const c_void,
    pub _abi_mut: *mut c_void,
}
```

Wraps the regular `CallbackInfo` (so timer callbacks have full DOM /
hit-test access) and adds:

- `frame_start` — the `Instant` captured at the start of this tick.
  All timers in the same tick see the same `frame_start`, so animations
  driven by multiple timers stay in lockstep.
- `call_count` — number of prior invocations.
- `is_about_to_finish` — `true` when this is the last call before the
  `timeout` boundary; lets the callback emit a final value.
- `_abi_ref` / `_abi_mut` — reserved padding for future FFI extensions.

## Thread system

The `Thread` is owned by the framework, not by user code. The user
provides three things:

```rust,ignore
let thread = Thread::create(
    thread_initialize_data,  // RefAny: passed to the thread fn
    writeback_data,          // RefAny: state owned by main thread
    thread_callback,         // extern "C" fn
);
```

`Thread::create` calls `create_thread_libstd` (`layout/src/thread.rs:812`)
which:

1. Creates a `Sender<ThreadReceiveMsg>` / `Receiver<ThreadReceiveMsg>` pair
   for thread → main writeback messages.
2. Creates a `Sender<ThreadSendMsg>` / `Receiver<ThreadSendMsg>` pair for
   main → thread control messages (`Tick`, `TerminateThread`, `Custom`).
3. Spawns a `std::thread::spawn` that calls
   `thread_callback(thread_initialize_data, ThreadSender, ThreadReceiver)`.
4. Holds an `Arc<()>` strong/weak pair as a "drop check" — when the
   strong reference drops at the end of the spawned closure, the main
   thread's `Weak::upgrade` returns `None`, signalling completion.

Once registered with `LayoutWindow.threads`, the framework polls it
every event-loop turn via `run_all_threads`.

### Channel ABI

Two enum types govern messages in each direction:

```rust,ignore
// Main → background
#[repr(C, u8)]
pub enum ThreadSendMsg {
    TerminateThread,
    Tick,
    Custom(RefAny),
}

// Background → main
#[repr(C, u8)]
pub enum ThreadReceiveMsg {
    WriteBack(ThreadWriteBackMsg),
    Update(Update),
}
```

`ThreadSendMsg::Tick` is sent by `run_all_threads` once per turn so the
thread can use it as a frame heartbeat. `ThreadSendMsg::Custom` carries
arbitrary `RefAny` payload — the channel does not interpret it.

`ThreadReceiveMsg::Update(Update)` is the lightweight path: the thread
just wants the UI to redraw and has no data for the main thread to
inspect. `ThreadReceiveMsg::WriteBack` is the heavyweight path: the
thread sends a `RefAny` payload and a function pointer that runs on the
main thread with full `CallbackInfo` access.

### `ThreadReceiver` / `ThreadSender`

Both are FFI-safe wrappers around the std `mpsc` channels with manual
clone/drop callbacks (`core/src/task.rs:570-625` and
`layout/src/thread.rs:90-153`). The pattern matches `InstantPtr`:
`ptr: Box<Arc<Mutex<…Inner>>>` plus extern function pointers for `recv`
/ `send` / destructor. The `ctx: OptionRefAny` slot holds an FFI
callable (e.g., a Python `PyFunction`) so foreign-language thread
callbacks can be re-entered from the C side.

`#[cfg(not(feature = "std"))]` builds compile to no-ops: `recv`
returns `OptionThreadSendMsg::None` and `send` returns `false`. There is
no real thread on no_std.

### `WriteBackCallback`

```rust,ignore
pub type WriteBackCallbackType = extern "C" fn(
    /* original thread data */ RefAny,
    /* data to write back   */ RefAny,
    /* callback info        */ CallbackInfo,
) -> Update;
```

Bundled with the `RefAny` payload into `ThreadWriteBackMsg`. When the
main thread pulls a `ThreadReceiveMsg::WriteBack` off the channel,
`run_all_threads` constructs a `CallbackInfo` (same shape as a regular
event callback — full DOM access, scroll states, hit-test, monitor
list) and invokes the writeback. The return `Update` is folded into the
event-loop's outgoing change set.

This is the only path by which a background thread is allowed to
mutate UI state. Direct `RefAny::downcast_mut` from the spawned thread
would race with the main thread; the writeback callback runs on the
main thread holding the borrow.

### `run_all_threads`

Defined at `layout/src/window.rs:3900`. The per-thread loop is:

1. Acquire the thread's mutex; on poison, emit
   `CallbackChange::RemoveThread` and skip.
2. Send a `Tick` (best-effort; the receiver may have dropped).
3. `try_recv` one `ThreadReceiveMsg`. If `None`, continue to the next
   thread — never block the main thread on a background thread.
4. Match the message:
   - `Update(u)` → fold into the outgoing `Update`, no callback.
   - `WriteBack(msg)` → run the writeback callback with full
     `CallbackInfo` and append its `CallbackChange`s.
5. Check `is_finished` (the dropcheck `Weak::upgrade`); if completed,
   emit `CallbackChange::RemoveThread`.

Only **one** message is drained per thread per turn. A thread that
floods the channel will be polled across multiple turns rather than
starving the event loop.

### `is_finished` and the dropcheck

`ThreadInner.dropcheck: Box<Weak<()>>` holds a weak reference to an
`Arc<()>` that is bound as `_thread_check_guard` inside the spawned
closure (`layout/src/thread.rs:849`). Binding to a *named* variable is
critical — `_` would drop the `Arc` immediately, fooling the main
thread into thinking the thread had already finished. When the closure
returns, the `Arc` drops, the `Weak::upgrade` on the main side returns
`None`, and `is_finished()` reports `true`.

## Where async hooks into the event loop

The platform shell calls these in order each turn (see
`dll/src/desktop/shell2/common/event.rs`):

```text
poll_window_events
  ├─ invoke_user_callbacks   (event handlers)
  ├─ invoke_timer_callbacks  (LayoutWindow::run_all_timers)
  ├─ invoke_thread_callbacks (LayoutWindow::run_all_threads, line 4566)
  └─ relayout / repaint
```

Both timer and thread invocation produce
`Vec<crate::callbacks::CallbackChange>` — the same change-set type that
event callbacks emit — so the shell applies them through a single
`apply_user_change` codepath. From the rest of the system's perspective,
a timer or thread is just another callback source.

## Cross-references

- `layout/src/timer.rs` — `Timer`, `TimerCallback`, `TimerCallbackInfo`,
  invocation logic.
- `layout/src/thread.rs` — `Thread`, `ThreadSender`, `ThreadReceiver`,
  `WriteBackCallback`, `create_thread_libstd`.
- `layout/src/window.rs:1948-2027` — `LayoutWindow` timer storage and
  `time_until_next_timer_ms`.
- `layout/src/window.rs:3900` — `run_all_threads`.
- `dll/src/desktop/shell2/common/event.rs:4566` — shell-side timer/thread
  driver.
- [Event System Internals](event-system.md) — the change-set type that
  timer and thread callbacks emit.
- [Shell2 Common Layer](shell2-common.md) — how the per-turn driver is
  invoked.
