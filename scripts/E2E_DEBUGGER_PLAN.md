# E2E Testing & Debugger Implementation Plan

## Status (2025-02-21)

### What works right now

| Component | Status | Notes |
|-----------|--------|-------|
| **Debug HTTP server** | ✅ Working | `AZUL_DEBUG=8765` starts server, `GET /` serves debugger.html |
| **Debugger frontend** | ✅ Basic UI | DOM Explorer, Test Explorer, Terminal panel, step editor |
| **Debug API endpoints** | ✅ Working | `get_state`, `get_html_string`, `get_dom_tree`, `get_display_list`, `get_app_state`, `set_app_state`, `click`, `text_input`, `key_down/up`, `mouse_move/down/up`, `scroll`, `resize`, `take_screenshot`, `relayout`, `close`, `run_e2e_tests` |
| **E2E assertion engine** | ✅ Implemented | `assert_text`, `assert_exists`, `assert_not_exists`, `assert_node_count`, `assert_layout`, `assert_css`, `assert_app_state`, `assert_scroll` |
| **CLI E2E runner** | ✅ Basic flow | `AZUL_RUN_E2E_TESTS=file.json` reads JSON, queues event, prints cargo-test output |
| **StubWindow/headless** | ✅ Layout works | `AZUL_HEADLESS=1` creates StubWindow with initial layout, timer ticking, CpuHitTester |
| **regenerate_layout()** | ✅ Cleaned up | No longer requires `WrRenderApi` / `DocumentId` — StubWindow can call it |

### What does NOT work yet

| Issue | Description | Priority |
|-------|-------------|----------|
| **Selector resolution** | Only tag name / `.class` / `#id` selectors work. Attribute selectors (`[data-az-node-id="6"]`), combinators (`div > span`), pseudo-classes (`:nth-child`) are not implemented. | High |
| **`click` action in E2E** | The `click` command uses `resolve_node_target()` which finds a node, but doesn't synthesize a real click event (mouse down → hit test → callback invocation → mouse up). It just logs. | High |
| **`text_input` / key events in E2E** | Same as click — the commands resolve but don't feed into the event processing pipeline. | High |
| **StubWindow event processing** | `StubEvent::MouseMove`, `MouseDown`, `MouseUp`, `KeyDown`, `KeyUp`, `TextInput`, `Resize` are defined but `run()` only handles `Close`. They need to feed into `PlatformWindow::process_*` methods. | High |
| **No CPU rendering in StubWindow** | `CpuBackend` has `last_frame: Option<Pixmap>` but `run()` never calls `cpurender::render()`. Screenshots in headless mode will be blank. | Medium |
| **Debugger frontend disconnected from E2E results** | The "Run Test" / "Run All" buttons in the debugger UI call `postE2e()` which sends `run_e2e_tests` via POST. The response comes back, but there's no live progress indication or step-by-step visualization during server-side execution. | Medium |
| **No `wait` / `wait_frame` implementation** | E2E steps `wait` (ms delay) and `wait_frame` (wait for next layout) are parsed but not implemented in `process_debug_event()`. | Low |
| **`assert_text` doesn't resolve `text` node type** | Selector `text` matches nothing because `text` is a DOM `NodeType`, not a tag name. Users must select the parent node. | Low |

---

## Architecture Overview

```
┌────────────────────────────────────────────────────────────────────┐
│                         Environment Variables                       │
│  AZUL_HEADLESS=1     AZUL_DEBUG=<port>     AZUL_RUN_E2E_TESTS=f   │
└────────┬──────────────────┬──────────────────────┬─────────────────┘
         │                  │                      │
         ▼                  ▼                      ▼
    ┌─────────┐      ┌────────────┐        ┌──────────────┐
    │ Stub    │      │ HTTP Server│        │ CLI Runner   │
    │ Window  │      │ (port)     │        │ (read JSON)  │
    └────┬────┘      └─────┬──────┘        └──────┬───────┘
         │                 │                      │
         │      ┌──────────┴──────────────────────┘
         │      │              push
         │      ▼
         │  ┌───────────────┐
         │  │ REQUEST_QUEUE │  One global queue, multiple producers
         │  └───────┬───────┘
         │          │ pop (16ms timer)
         │          ▼
         │  ┌───────────────────────┐
         │  │ debug_timer_callback  │  process_debug_event()
         │  │ → CallbackInfo        │  → has access to live DOM
         │  └───────────────────────┘
         │
         └──► regenerate_layout() → CpuHitTester
              cpurender::render() → Pixmap (screenshots)
```

### Three independent systems (see INTERACTION_MATRIX.md)

1. **Headless mode** (`AZUL_HEADLESS`): Uses `StubWindow` + `CpuBackend` instead of a real OS window. No GPU, no OpenGL. Layout works via the shared `regenerate_layout()`. Hit-testing via `CpuHitTester`.

2. **Debug server** (`AZUL_DEBUG=port`): HTTP server on `127.0.0.1:port`. `GET /` serves `debugger.html/js/css`. `POST /` accepts JSON commands. Commands are pushed onto `REQUEST_QUEUE`, processed by a 16ms timer callback that has full `CallbackInfo` access.

3. **E2E runner** (`AZUL_RUN_E2E_TESTS=file.json`): Reads test file at startup, pushes a single `RunE2eTests` event onto the queue. Spawns a background thread that waits for results via `mpsc::Receiver`, prints cargo-test-style output, calls `exit(0)` or `exit(1)`.

---

## Implementation Roadmap

### Phase 1: Debug Logging & Observability (Current)

**Goal**: Add enough `console.log` / Rust `log_debug!` statements to understand the flow.

#### Debugger frontend (`debugger.js`)
- [ ] Log every API request/response with request timing
- [ ] Log DOM tree parsing and node count on refresh
- [ ] Log test step execution progress (start, complete, error)
- [ ] Log server-side test submission and result parsing
- [ ] Log connection status changes
- [ ] Log E2E step creation/deletion/modification

#### Rust E2E runner (`debug_server.rs`, `run.rs`, `stub/mod.rs`)
- [ ] Log when `setup_e2e_runner()` reads and parses the test file
- [ ] Log each E2E step as it executes in `process_debug_event()`
- [ ] Log assertion evaluation details (selector resolution, matched nodes, comparison)
- [ ] Log `StubWindow` initial layout success/failure and node count
- [ ] Log timer registration and tick count in StubWindow
- [ ] Log when `RunE2eTests` event is dequeued and processing begins
- [ ] Log when results are sent back via mpsc channel

### Phase 2: Wire StubWindow Events (Next)

**Goal**: Make `StubEvent::{MouseMove, MouseDown, MouseUp, KeyDown, KeyUp, TextInput, Resize}` actually process through the `PlatformWindow` event pipeline.

```rust
// In StubWindow::run(), Phase 1:
StubEvent::MouseMove { x, y } => {
    self.common.current_window_state.mouse_state.cursor_position = LogicalPosition { x, y };
    // Call the shared hit-test + hover logic
}
StubEvent::MouseDown { button } => {
    // Synthesize mouse-down event → PlatformWindow::process_mouse_down()
}
// etc.
```

This requires implementing the `process_*` methods on StubWindow using the `CpuHitTester` instead of WebRender's `AsyncHitTester`.

### Phase 3: E2E Actions → Real Events

**Goal**: Make E2E commands (`click`, `text_input`, `scroll`, etc.) inject real events into the StubWindow event queue.

Currently `click` only calls `resolve_node_target()` to find the node. It needs to:
1. Get the node's layout rect from `CpuHitTester` / `LayoutWindow`
2. Calculate center point of the rect
3. Inject `StubEvent::MouseMove { center }`, `MouseDown { Left }`, `MouseUp { Left }`
4. Process those events through the normal event pipeline
5. Return result after the callback has fired

### Phase 4: CPU Rendering for Screenshots

**Goal**: `take_screenshot` in headless mode should produce an actual image.

```rust
// In StubWindow, after regenerate_layout():
#[cfg(feature = "cpurender")]
{
    if let Some(lw) = self.common.layout_window.as_ref() {
        let pixmap = azul_layout::cpurender::render(lw, &self.common.renderer_resources);
        self.cpu_backend.last_frame = Some(pixmap);
    }
}
```

The `take_screenshot` handler in `process_debug_event()` then reads `cpu_backend.last_frame` and returns base64-encoded PNG.

### Phase 5: Enhanced Selector Resolution

**Goal**: Support more CSS selector syntax in `resolve_all_matching_nodes()`.

| Selector | Example | Status |
|----------|---------|--------|
| Tag name | `div`, `body`, `button` | ✅ Works |
| Class | `.my-class` | ✅ Works |
| ID | `#my-id` | ✅ Works |
| Attribute `[attr="val"]` | `[data-az-node-id="6"]` | ❌ TODO |
| Descendant combinator | `div span` | ❌ TODO |
| Child combinator | `div > span` | ❌ TODO |
| Node type pseudo | `:text`, `:button` | ❌ TODO |
| `:nth-child(n)` | `div:nth-child(2)` | ❌ TODO |
| `:first-child`, `:last-child` | `li:first-child` | ❌ TODO |

### Phase 6: Debugger UI Polish

**Goal**: Make the browser debugger a fully functional E2E test authoring tool.

- [ ] Live DOM tree updates (polling or WebSocket)
- [ ] Click-to-select nodes in the DOM tree → auto-fills assertion selectors
- [ ] Step-by-step execution visualization (highlight current step, show assertion results inline)
- [ ] Screenshot comparison (reference vs actual)
- [ ] Test recording mode (capture user interactions as E2E steps)
- [ ] Export to `AZUL_RUN_E2E_TESTS` JSON format

---

## File Map

| File | Purpose | Lines |
|------|---------|-------|
| `dll/src/desktop/shell2/common/debug_server.rs` | Core debug server, HTTP handling, event routing, E2E types, assertion engine | ~5900 |
| `dll/src/desktop/shell2/common/debugger/debugger.html` | Browser debugger UI (HTML) | 190 |
| `dll/src/desktop/shell2/common/debugger/debugger.js` | Browser debugger logic (JS) | 667 |
| `dll/src/desktop/shell2/common/debugger/debugger.css` | Browser debugger styles | 284 |
| `dll/src/desktop/shell2/common/layout.rs` | Cross-platform layout regeneration | 863 |
| `dll/src/desktop/shell2/stub/mod.rs` | StubWindow (headless + CPU backend) | ~740 |
| `dll/src/desktop/shell2/run.rs` | Platform run() + E2E runner setup | 1150 |
| `layout/src/headless.rs` | CpuHitTester (layout-based hit testing) | 391 |
| `layout/src/cpurender.rs` | CPU rendering to tiny_skia Pixmap | 1028 |
| `scripts/INTERACTION_MATRIX.md` | 8-cell interaction matrix documentation | 102 |

---

## Testing Cheatsheet

### Run windowed app with debug server
```bash
cc -o hello-world examples/c/hello-world.c -I dll/ -L target/release/ -lazul
AZUL_DEBUG=8765 ./hello-world
# Open http://localhost:8765 in browser
```

### Run E2E tests (windowed — visual debugging)
```bash
AZUL_RUN_E2E_TESTS=tests/e2e/test.json ./hello-world
```

### Run E2E tests (headless — CI mode)
```bash
AZUL_HEADLESS=1 AZUL_RUN_E2E_TESTS=tests/e2e/test.json ./hello-world
```

### Run E2E tests (headless + debug server — full CI with inspection)
```bash
AZUL_HEADLESS=1 AZUL_DEBUG=8765 AZUL_RUN_E2E_TESTS=tests/e2e/test.json ./hello-world
```

### Send manual debug command
```bash
curl -s -X POST http://localhost:8765 -d '{"op":"get_html_string"}' | python3 -m json.tool
```

### Run E2E test via HTTP API
```bash
curl -s -X POST http://localhost:8765 -d '{
  "op": "run_e2e_tests",
  "tests": [{
    "name": "counter-check",
    "steps": [
      {"op": "assert_exists", "selector": "body"},
      {"op": "assert_exists", "selector": "button"}
    ]
  }]
}' | python3 -m json.tool
```

### E2E Test JSON Format
```json
[
  {
    "name": "test-counter-increment",
    "steps": [
      { "op": "assert_exists", "selector": "body" },
      { "op": "assert_exists", "selector": "button" },
      { "op": "click", "selector": "button" },
      { "op": "assert_text", "selector": "div", "expected": "6" }
    ]
  }
]
```
