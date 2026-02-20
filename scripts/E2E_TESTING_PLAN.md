# Azul E2E Testing Architecture

## Overview

Azul's E2E testing system lets users **design, run, and export** end-to-end
tests entirely from the browser (via the debug server UI) or from the
command line (via `AZUL_RUN_E2E_TESTS`).  The same JSON test format is
used everywhere — browser designer, CI runner, and manual `curl` scripts.

```
┌──────────────────────────────────────────────────────────────────┐
│  Browser (debugger.html)                                        │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐  │
│  │  DOM Inspector   │  │  E2E Designer    │  │  Test Results  │  │
│  │  (existing)      │  │  + Run / RunAll  │  │  + screenshots │  │
│  └─────────────────┘  └────────┬────────┘  └───────▲────────┘  │
│                                │ POST /e2e/run      │ JSON      │
│                                ▼                    │           │
│  ┌──────────────────────────────────────────────────┘           │
│  │             Debug HTTP Server (port AZUL_DEBUG)              │
│  │             ─────────────────────────────────                │
│  │  GET /         → serves debugger.html                       │
│  │  GET /health   → health check JSON                          │
│  │  POST /        → existing single-command API                │
│  │  POST /e2e/run → run one or many E2E tests (new)            │
│  └──────────────────────────────┬───────────────────────────────┘
│                                 │                               │
│  For each test:                 ▼                               │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  1. Clone current app_data (RefAny)                      │   │
│  │  2. Optionally set_app_state / set_window_state          │   │
│  │  3. Create StubWindow with CpuBackend                    │   │
│  │  4. Execute steps sequentially (same as curl API)        │   │
│  │  5. After each step: collect logs, optional screenshot   │   │
│  │  6. Evaluate assertions → pass / fail                    │   │
│  │  7. Close StubWindow, return results                     │   │
│  └──────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘

CLI mode (no browser):
  AZUL_RUN_E2E_TESTS=tests.json ./my_app
  → Starts app headlessly
  → Runs all tests from JSON file
  → Prints cargo-test-style output
  → exit(0) on success, exit(1) on failure
  → Writes logs + screenshots to target/e2e_results/
```

## JSON Test Format

A single E2E test is a JSON object:

```json
{
  "name": "Button click increments counter",
  "description": "Verify that clicking the + button increases the counter display",
  "setup": {
    "window_width": 800,
    "window_height": 600,
    "dpi": 96,
    "app_state": { "counter": 0 }
  },
  "steps": [
    {
      "op": "click",
      "selector": ".increment-btn",
      "screenshot": true
    },
    {
      "op": "wait_frame"
    },
    {
      "op": "assert_text",
      "selector": ".counter-display",
      "expected": "1"
    },
    {
      "op": "take_screenshot"
    }
  ]
}
```

A test file can contain a single test or an array of tests:

```json
[
  { "name": "Test 1", "steps": [...] },
  { "name": "Test 2", "steps": [...] }
]
```

### Step Operations

All existing debug API operations are valid steps (see `DEBUG_API.md`).
Additionally, E2E tests support assertion operations:

| Operation | Parameters | Description |
|-----------|-----------|-------------|
| `assert_text` | `selector`, `expected` | Assert that the text content of a node matches |
| `assert_exists` | `selector` | Assert that a node matching the selector exists |
| `assert_not_exists` | `selector` | Assert that no node matches the selector |
| `assert_node_count` | `selector`, `expected` | Assert the number of matching nodes |
| `assert_layout` | `selector`, `property`, `expected`, `tolerance?` | Assert layout property (x, y, width, height) |
| `assert_css` | `selector`, `property`, `expected` | Assert computed CSS value |
| `assert_screenshot` | `reference` | Compare screenshot against a reference (base64 or filename) |
| `assert_app_state` | `path`, `expected` | Assert a field in the serialized app state (dot-notation) |
| `assert_scroll` | `selector`, `x?`, `y?`, `tolerance?` | Assert scroll position |

### Step Result (returned per step)

```json
{
  "step_index": 0,
  "op": "click",
  "status": "pass",
  "duration_ms": 12,
  "logs": [...],
  "screenshot": "data:image/png;base64,...",
  "error": null,
  "response": { ... }
}
```

### Test Result (returned per test)

```json
{
  "name": "Button click increments counter",
  "status": "pass",
  "duration_ms": 156,
  "step_count": 4,
  "steps_passed": 4,
  "steps_failed": 0,
  "steps": [ ... ],
  "final_screenshot": "data:image/png;base64,..."
}
```

## Implementation Phases

### Phase 1: Unified Debugger UI

**Goal:** Merge `debugger1.html` and `debugger2.html` into a single
`debugger.html` served by the debug server.

**Changes:**

- [x] File: `dll/src/desktop/shell2/common/debugger.html` (new, merged)
  - Merge the dropdown menu from debugger1 (File → New Test, Import; 
    Import → UI from HTML, E2E Tests; Export → Workspace, E2E Tests)
    into debugger2's menu bar
  - Add "Run Test" and "Run All Tests" buttons to the E2E testing view
  - Add screenshot preview panel (displays base64 PNG inline)
  - Add assertion step types to the step schema

- [x] Delete: `debugger1.html`, `debugger2.html`

- [x] File: `dll/src/desktop/shell2/common/debug_server.rs`
  - `GET /` → serve `debugger.html` (embedded via `include_str!`)
  - `GET /health` → existing health JSON (unchanged)
  - `POST /` → existing single-command API (unchanged)
  - `POST /e2e/run` → new: run E2E test(s) and return results

### Phase 2: Server-Side E2E Execution Engine

**Goal:** The debug server can accept E2E test JSON, create a StubWindow,
execute steps, and return structured results.

**Changes:**

- File: `dll/src/desktop/shell2/common/debug_server.rs`
  - New `DebugEvent::RunE2eTests { tests: Vec<E2eTest> }` variant
  - New `E2eTest`, `E2eStep`, `E2eTestResult`, `E2eStepResult` structs
  - Handler for `RunE2eTests`:
    1. Clone `app_data` from the running application
    2. For each test (can run in parallel via `rayon` or sequentially):
       a. Create `StubWindow::new(...)` with cloned state
       b. Apply `setup` (window size, DPI, app_state)
       c. For each step: dispatch as if it were a regular debug command,
          collect response, logs, optional screenshot
       d. Evaluate assertions
       e. Close StubWindow
    3. Return `Vec<E2eTestResult>`

- File: `dll/src/desktop/shell2/stub/mod.rs`
  - Add `StubWindow::run_e2e_test(test: E2eTest) -> E2eTestResult`
    - Creates the window, applies setup
    - Iterates steps, calling the same `process_debug_event()` logic
    - Collects per-step logs and screenshots
    - Returns structured result
  - CpuBackend rendering: implement `take_screenshot_base64()` for
    StubWindow using `tiny_skia` (or a simpler "layout-only" approach
    that renders rectangles + text glyphs to a Pixmap)

### Phase 3: Assertion Engine

**Goal:** Evaluate pass/fail conditions for E2E test steps.

**Changes:**

- File: `dll/src/desktop/shell2/common/e2e_assertions.rs` (new)
  - `evaluate_assertion(step, response, layout_window) -> AssertionResult`
  - Handles each `assert_*` operation type
  - `assert_text`: resolve selector → get node text content → compare
  - `assert_exists` / `assert_not_exists`: resolve selector → check
  - `assert_layout`: resolve selector → get layout rect → compare with tolerance
  - `assert_css`: resolve selector → get computed CSS → compare
  - `assert_app_state`: deserialize app state → navigate dot-path → compare
  - `assert_screenshot`: pixel-diff against reference image (optional, can be skipped)

### Phase 4: Browser UI for Results

**Goal:** The debugger UI can display E2E test results with screenshots.

**Changes:**

- File: `debugger.html`
  - "Run Test" button: POST to `/e2e/run` with current test JSON
  - "Run All Tests" button: POST with all tests
  - Show spinner while running
  - Display results: per-step pass/fail markers, expandable logs,
    inline screenshot previews (base64 `<img>` tags)
  - Side-by-side screenshot comparison for `assert_screenshot`

### Phase 5: Headless CLI Runner

**Goal:** `AZUL_RUN_E2E_TESTS=file.json ./my_app` runs tests and exits.

**Changes:**

- File: `dll/src/desktop/shell2/run.rs`
  - Check `AZUL_RUN_E2E_TESTS` env var *before* `AZUL_HEADLESS`
  - If set: parse JSON file, create StubWindow, run all tests sequentially,
    print cargo-test-style output, write results to `target/e2e_results/`,
    `exit(0)` or `exit(1)`

- Output format (stdout):
  ```
  running 3 e2e tests
  test Button click increments counter ... ok (156ms)
  test Scroll down loads more items ... ok (342ms)
  test Invalid input shows error ... FAILED (89ms)

  failures:

  ---- Invalid input shows error ----
  Step 3 (assert_text): expected "Error: invalid", got "Please enter a value"
    selector: .error-message

  test result: FAILED. 2 passed; 1 failed; 0 ignored

  Screenshots and logs written to target/e2e_results/
  ```

- File: `dll/src/desktop/shell2/stub/mod.rs`
  - `StubWindow::run_e2e_tests(tests: Vec<E2eTest>) -> Vec<E2eTestResult>`
    Convenience wrapper that runs multiple tests and returns all results.

### Phase 6: CPU Rendering for Screenshots

**Goal:** StubWindow can produce pixel-accurate screenshots via CpuBackend.

**Status:** Partially implemented. `CpuBackend` has `#[cfg(feature = "cpurender")]`
field for `tiny_skia::Pixmap`.

**Changes:**

- File: `layout/src/cpurender.rs` (new or extend existing)
  - `render_to_pixmap(layout_result, resources, width, height, dpi) -> Pixmap`
  - Renders display list items to a `tiny_skia::Pixmap`:
    - Rectangles, borders, box-shadows → `tiny_skia` path fills
    - Text → glyph rasterization via `ab_glyph` or pre-rasterized glyphs
    - Images → decode + blit
  - Returns PNG-encoded base64 string

- File: `dll/src/desktop/shell2/stub/mod.rs`
  - `StubWindow::take_screenshot() -> Result<String, String>`
    Uses CpuBackend to render current layout to Pixmap, encode as PNG,
    return base64 data URI.

**Note:** Full pixel-perfect CPU rendering is a large task. Initially,
screenshots in headless mode can return a "layout wireframe" (colored
rectangles with text labels) which is sufficient for debugging. Full
rendering can be added later behind the `cpurender` feature flag.

## Architecture Notes

### Test Isolation

Each E2E test gets its own `StubWindow` with a **cloned** `app_data`.
This means:
- Tests don't interfere with each other
- Tests don't affect the running application
- Tests can run in parallel (different StubWindows on different threads)

**Caveat:** If the app_data doesn't implement `Clone` (it's a `RefAny`),
we use the `serialize → deserialize` round-trip to create a copy. If
neither is available, we fall back to the default constructor. This makes
the test non-deterministic unless the test starts with `set_app_state`.

### Event Processing in StubWindow

The E2E test runner drives the StubWindow's event loop directly (no
condvar needed). It's synchronous:

```
for step in test.steps:
    1. Inject event into StubWindow
    2. Run one iteration of the event loop (process events, tick timers)
    3. If step has screenshot: render via CpuBackend → capture
    4. Collect logs since last step
    5. Evaluate assertion (if applicable)
    6. Record step result
```

This is different from `StubWindow::run()` which blocks on a condvar.
The E2E runner calls the internal processing methods directly.

### Parallel Execution

When the browser sends "Run All Tests", the server can execute tests
in parallel using `std::thread::scope` or similar. Each test gets its
own thread with its own StubWindow. Results are collected and returned
as a single JSON array.

The CLI runner (`AZUL_RUN_E2E_TESTS`) runs tests sequentially by default
but could support `--parallel` in the future.

## File Overview

| File | Purpose |
|------|---------|
| `common/debugger.html` | Unified browser UI (merged from debugger1 + debugger2) |
| `common/debug_server.rs` | HTTP server, event routing, E2E test dispatch |
| `common/e2e_assertions.rs` | Assertion evaluation engine |
| `stub/mod.rs` | StubWindow + CpuBackend + E2E test runner |
| `run.rs` | CLI runner (`AZUL_RUN_E2E_TESTS`) integration |
| `layout/src/cpurender.rs` | CPU rendering to Pixmap (behind feature flag) |
| `scripts/DEBUG_API.md` | API documentation (existing, updated) |
| `scripts/E2E_TESTING_PLAN.md` | This document |

## Migration Path

1. Existing bash E2E tests (`tests/e2e/*.sh`) continue to work unchanged
2. New JSON E2E tests can be designed in the browser and exported
3. Exported JSON tests can be checked into `tests/e2e/*.json`
4. CI can run both: bash scripts (for complex scenarios) and JSON tests
   (for UI behaviour regression via `AZUL_RUN_E2E_TESTS`)
5. Over time, bash tests can be migrated to JSON format
