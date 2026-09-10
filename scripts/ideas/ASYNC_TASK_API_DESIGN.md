# Async-task API — design (for review before implementation)

Status: **DESIGN — not yet implemented.** Requested 2026-06-10. Build only after sign-off.

## 1. Goal & constraints

A first-class, ergonomic way for app code (and widgets like `MapWidget`) to run
**I/O- or CPU-bound work off the main thread** and be notified, on the main
thread, when each result is ready.

Hard constraints (from the user):
- **No external async deps** (no tokio, no async-std, no mio). Pure `std` + azul's
  existing FFI vec/option/callback types.
- **Cross-platform** (Linux/macOS/Windows/Android/iOS — the same desktop+mobile
  matrix azul already targets).
- **Submit-a-callback model**: the user hands in a closure to run async and a
  closure to run on completion. No futures, no executors exposed.
- Results delivered **one at a time** as they finish (not all-at-once), so a
  cache (e.g. map tiles) updates incrementally.
- Support **batches of N** in flight + **priority** (centre map tiles first).

## 2. What already exists (and why it's not enough)

azul already has a cross-platform, no-tokio async primitive — the **`Thread`** API
(`core/src/task.rs`, exposed in `api.json`):

```rust
let thread = Thread::create(init_data: RefAny, writeback_data: RefAny, callback);
info.add_thread(ThreadId::unique(), thread);
```

- `callback` runs on a fresh **`std::thread`**; it can send results back via a
  `ThreadWriteBackMsg`.
- The framework **drains** finished threads on the frame loop:
  `process_timers_and_threads` → `invoke_thread_callbacks` (dll `event.rs`) →
  `LayoutWindow::run_all_threads` (`window.rs:4043`), which invokes the writeback
  callback **on the main thread**, one message at a time. This is exactly the
  "notified one-by-one" behaviour we want — and the new API will reuse it verbatim.

`MapWidget` uses this today (`spawn_pending_tile_fetches`, `map.rs:1036`):
one `Thread::create` + `add_thread` **per tile**, up to 16 per call.

**Gaps the async helper must close:**
1. **One OS thread per task.** A viewport pan spawns ~16+ raw `std::thread`s; a
   fast zoom/pan storm spawns hundreds. We want a **bounded worker pool**.
2. **No priority.** Tiles spawn in `BTreeMap` order, not centre-first.
3. **Ergonomics.** `Thread::create(init, writeback, cb)` with two `RefAny`s and a
   manual `ThreadWriteBackMsg` is low-level. The user wants a clean
   "submit work + on_complete" call.

## 3. Proposed public API

A new primitive layered **on top of** the existing thread/writeback plumbing.

```rust
// New FFI-friendly types (codegen'd into api.json via azul-doc):

/// A unit of background work + its main-thread completion handler.
pub struct AsyncTask {
    /// Opaque input handed to `work` on the worker thread.
    pub input: RefAny,
    /// Runs on a POOL WORKER thread. Pure compute / blocking I/O. Returns the
    /// result as a RefAny. Must not touch app/DOM state (no CallbackInfo here).
    pub work: AsyncWorkCallback,        // extern "C" fn(RefAny /*input*/) -> RefAny /*output*/
    /// Runs on the MAIN thread when `work` finishes, with the produced output
    /// and the app's data. This is where you mutate app state / cache.
    pub on_complete: AsyncCompleteCallback, // extern "C" fn(RefAny /*app data*/, RefAny /*output*/, CallbackInfo) -> Update
    /// Higher runs first. Centre map tiles use a higher priority than edges.
    pub priority: i32,
}

pub struct AsyncTaskId { /* unique, for cancellation */ }

impl CallbackInfo {
    /// Submit a task to the window's shared worker pool. Returns its id so the
    /// caller can cancel it (e.g. a tile that scrolled out of view). Cheap:
    /// just enqueues; a pool worker picks it up when a slot frees.
    pub fn spawn_async(&mut self, task: AsyncTask) -> AsyncTaskId;
    /// Cancel a not-yet-started task (best-effort; running tasks finish).
    pub fn cancel_async(&mut self, id: AsyncTaskId) -> bool;
}
```

Notes:
- `work` deliberately gets **no `CallbackInfo`** — it runs off-thread and must not
  reach DOM/app state. `on_complete` gets the full `CallbackInfo` (main thread).
- `input`/`output` are `RefAny` (azul's existing type-erased, refcounted handle),
  so the API is FFI-safe and language-binding-friendly with zero new machinery.
- Returning `Update` from `on_complete` integrates with the existing relayout/
  repaint flow (e.g. `DoNothing` + an in-place VirtualView re-render for the map).

## 4. The worker pool (no tokio, cross-platform)

A single bounded pool **owned by the window** (`LayoutWindow`), lazily created on
the first `spawn_async`:

- `N = clamp(num_cpus - 1, 1, 8)` workers (configurable later). For network-bound
  tile fetches N can be higher than cores; start with cores-1 and make it tunable.
- Each worker is a long-lived `std::thread` looping on a shared
  **priority queue** (a `BinaryHeap` behind a `Mutex` + `Condvar`, all `std`).
- A worker pops the highest-priority task, runs `work(input)`, and pushes the
  `(task_id, output)` onto the **same completion channel the existing
  `Thread` writeback uses** — so `run_all_threads` drains it on the frame loop and
  calls `on_complete` on the main thread, one at a time. **No new drain path.**
- Cancellation: `cancel_async` flags the id; the queue skips flagged tasks when
  popping (running tasks are not interrupted — they just have their result
  dropped if cancelled).
- Shutdown: the pool joins its workers on window close (Condvar wake + a `stop`
  flag). No detached threads.

Why a pool, not epoll/async-IO: the user said "epoll, async IO, whatever". For
blocking HTTP tile fetches a bounded pool is the simplest correct cross-platform
answer (one in-flight request per worker), needs zero new deps, and reuses the
existing main-thread drain. True readiness-based async-IO (epoll/kqueue/IOCP)
would mean a platform-specific reactor — far more surface for no real win on this
workload. (If a future workload needs 1000s of concurrent sockets, revisit.)

## 5. `MapWidget` migration

- Replace the per-tile `Thread::create` loop (`spawn_pending_tile_fetches`) with
  one `spawn_async` per pending tile, **priority = -(distance² from viewport
  centre)** so centre tiles fetch first.
- Keep the existing writeback semantics (the same cache-clone `RefAny` →
  `on_complete` stamps `Ready` and triggers an in-place VirtualView re-render).
- **Batches of N**: drop the per-call `MAX_SPAWN_PER_CALL` cap — the pool bounds
  concurrency now, so we can enqueue the whole viewport and let priority order it.
- **LRU eviction** (task #13): cap `MapTileCache.tiles` (e.g. 256 tiles); on
  insert, evict the least-recently-rendered tiles outside the current overscan.
  Independent of the async API; lands alongside it.

## 6. api.json / codegen plan (NEVER hand-edit api.json)

Per `[[azul-codegen-pipeline]]`:
1. Add the Rust types (`AsyncTask`, `AsyncTaskId`, the two callback typedefs) +
   `CallbackInfo::spawn_async/cancel_async` in `core` / `layout`, marked for
   public export like the existing `Thread`/`Timer`.
2. `azul-doc autofix` → review the generated `target/autofix/patches/*.json` →
   `azul-doc autofix apply <patch>` (writes api.json) → `azul-doc normalize`.
3. `azul-doc codegen all` → `cargo build -r -p azul-dll --features build-dll`.
4. Regenerate C/C++/Python headers come for free from codegen.

## 7. Open decisions (please confirm before build)

1. **Pool size policy** — cores-1 default, or a fixed small N (e.g. 6) for
   network work? Make it configurable via `AppConfig`?
2. **`spawn_async` vs extending `Thread`** — new primitive (as above), or add
   `priority` + an internal pool *under* the existing `add_thread` and keep one
   public API? (New primitive reads cleaner; reusing `Thread` is less api.json
   churn.)
3. **Cancellation granularity** — id-based cancel is best-effort (can't interrupt
   a running blocking fetch). OK, or do we need a cooperative cancel flag passed
   into `work`?
4. **Priority type** — `i32` (simple) vs a small enum (`High/Normal/Low`)?

Once these are settled I'll implement §3–§6 and migrate the map.
