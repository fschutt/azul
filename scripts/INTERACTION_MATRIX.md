# Interaction Matrix: Headless × Debug Server × E2E Runner

Three independent systems, each controlled by one environment variable:

| System        | Env Var                         | What it does                                                    |
|---------------|--------------------------------|-----------------------------------------------------------------|
| Headless mode | `AZUL_HEADLESS=1`              | StubWindow instead of real platform window                      |
| Debug server  | `AZUL_DEBUG=<port>`            | HTTP API on `127.0.0.1:<port>`, starts the queue-processing timer |
| E2E runner    | `AZUL_RUN_E2E_TESTS=<file>`    | Pushes one `run_e2e_tests` event onto the queue, starts timer   |

---

## The 8-cell matrix

| # | Headless | Debug Server | E2E Runner | Behaviour |
|---|----------|-------------|------------|-----------|
| 1 | —        | —           | —          | Normal windowed app. No timer, no queue. |
| 2 | ✓        | —           | —          | StubWindow (headless). Useful for CI rendering without GPU. No debug API. |
| 3 | —        | ✓           | —          | Windowed app **with** debug HTTP API on the given port. Debugger UI at `GET /`. Timer polls queue. |
| 4 | —        | —           | ✓          | Windowed app **with** E2E test queued. Timer processes the test, background thread prints results and calls `exit()`. No HTTP server — results go to stderr. |
| 5 | ✓        | ✓           | —          | StubWindow + debug HTTP API. Headless debug inspection (e.g. CI screenshot service). |
| 6 | ✓        | —           | ✓          | StubWindow + E2E runner. **Canonical CI mode**: headless, no network, results on stderr, exit code 0/1. |
| 7 | —        | ✓           | ✓          | Windowed app + debug server + E2E runner. Tests execute while the window is visible — useful for visual debugging. Results available on stderr **and** via `GET /` in the debugger UI. |
| 8 | ✓        | ✓           | ✓          | StubWindow + debug server + E2E runner. Full CI mode with network inspection. Connect the debugger UI to observe test execution live. |

---

## Shared infrastructure

All three systems share a single queue and timer:

```
┌─────────────────┐
│  REQUEST_QUEUE   │  OnceLock<Mutex<VecDeque<DebugRequest>>>
│  (shared)        │
└──────┬──────────┘
       │  push                          push
       │◄──────────────────┐◄─────────────────────┐
       │                   │                       │
┌──────┴──────┐    ┌───────┴───────┐      ┌───────┴───────┐
│ E2E Runner  │    │ Debug Server  │      │ External HTTP │
│ (startup)   │    │ (HTTP thread) │      │   client      │
│             │    │               │      │               │
│ queue_e2e   │    │ POST / {...}  │      │ POST / {...}  │
│ _tests()    │    │               │      │               │
└─────────────┘    └───────────────┘      └───────────────┘
       │
       │  recv (mpsc)
       ▼
┌─────────────────┐
│ Result Thread   │  Waits for DebugResponseData, prints
│ (background)    │  cargo-test output, calls exit()
└─────────────────┘

       ┌────────────────┐
       │ debug_timer    │  16 ms tick — pops from REQUEST_QUEUE,
       │ _callback()    │  calls process_debug_event(), sends
       │                │  response via mpsc::Sender
       └────────────────┘
       Registered by the event loop of whichever window is created
       (real platform window OR StubWindow)
```

### Initialisation paths

| Who initialises the queue? | When? |
|---------------------------|-------|
| `start_debug_server(port)` | `AZUL_DEBUG=<port>` detected in `App::create()`. Sets `DEBUG_ENABLED`, starts HTTP thread. |
| `queue_e2e_tests(tests)` | `AZUL_RUN_E2E_TESTS=<file>` detected in `run()`. Sets `E2E_ACTIVE`. No HTTP thread. |

Both use `ensure_queue_initialized()` → `OnceLock::get_or_init(...)`.  
`is_debug_enabled()` returns `DEBUG_ENABLED ∨ E2E_ACTIVE` — this is what the platform event loop checks to decide whether to register the timer.

### Timer registration

Each platform's window-setup code calls:

```rust
if debug_server::is_debug_enabled() {
    timers.insert(debug_server::create_debug_timer(app_data, get_time_fn));
}
```

This is platform-agnostic — macOS, Windows, X11, Wayland, and StubWindow all do the same check.

---

## Key design invariants

1. **No implied dependencies.** Setting `AZUL_RUN_E2E_TESTS` does NOT start the HTTP server. Setting `AZUL_DEBUG` does NOT force headless mode.

2. **`run_e2e_tests` is a normal `DebugEvent`.** It can be sent via:
   - The `AZUL_RUN_E2E_TESTS` env var (queue directly)
   - `POST /` with `{"op": "run_e2e_tests", "tests": [...]}` (HTTP, requires `AZUL_DEBUG`)
   - The debugger UI (browser, requires `AZUL_DEBUG`)

3. **One queue, one timer.** There is exactly one `REQUEST_QUEUE` and one timer callback. Multiple producers (E2E runner, HTTP server, external clients) feed into it; the single timer callback is the sole consumer.

4. **`setup_e2e_runner()` does not replace `run()`.** It pushes the event and spawns a result-waiting thread, then control returns to the normal windowed/headless startup path.

5. **Exit is driven by the result thread.** When the E2E response arrives, the background thread prints results and calls `std::process::exit()`. The event loop does not need to know about E2E completion.
