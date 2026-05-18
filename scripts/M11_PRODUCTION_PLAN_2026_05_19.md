# M11 — Production-grade Web Backend + js-framework-benchmark

**Goal:** every browser event flows through to callbacks; every callback
side-effect (patches, focus, scroll, timers) propagates back to the DOM
— same surface as a desktop window. Plus a TodoMVC-flavored
`azul-bench.bin` to publish numbers against React/Preact/Svelte.

**Status:** PLAN. Sequel to M10-E/F (size optimization).

---

## Current state (post-M10-F, what's wired vs what isn't)

| Layer | Defined | Actually wired E2E |
|---|---|---|
| Event kinds | 50 (Hover / Focus / Window / Component / Application) | **1** (click) |
| Patch kinds | 7 documented constants | **1** (SetText) |
| CallbackChange variants | 50+ side-effects | **0** plumbed |
| Hit-test | M9-4 stub returns last-registered cb node | (stub) |
| Wasm-resident StyledDom for diff | `current_dom: Option<StyledDom>` field exists | permanently `None` — only raw blob `current_dom_ptr` set |
| Layout-diff loop on RefreshDom | desktop path uses `reconcile_dom_with_changes` | hardcoded SetText with `counter as decimal` |

The diagnostic detail: `event_filter_to_js_name` already emits
`data-az-ev="<name>"` for 9 distinct event names, but
`loader_js.rs:577-580` ignores that attribute and only binds
`body.addEventListener('click')`. Same shape on the patch side —
`eventloop.rs:702-718` comments enumerate 7 patch kinds; only kind=1
SetText is implemented.

---

## Stage A — Production-grade event coverage (4.5d in-scope + 1.5d deferred touch/drag)

### A.1 Expand `event_kind` discriminator table (0.5d)
`dll/src/web/eventloop.rs:72-85` (`pub mod event_kind`). Add the 25+
kinds from `core/src/events.rs::EventType`: `CONTEXT_MENU`, `KEYPRESS`,
`COMPOSITION_*`, `INPUT`, `CHANGE`, `SUBMIT`, `SCROLL_*`, `DRAG_*`,
`TOUCH_*`, `COPY`/`CUT`/`PASTE`, `WINDOW_FOCUS_*`, `THEME_CHANGE`,
`DPI_CHANGED`, `POINTER_MOVE`, plus synthetic `MOUNT`/`UNMOUNT`.
Mirror as JS `EVT_*` constants in `loader_js.rs:48-59`.

### A.2 Variable-width event payload (0.5d)
Replace the fixed 256-byte buffer with a 20-byte common header
(`node_idx | x | y | button_or_key | modifiers`) plus a kind-specific
TLV tail. Per-kind tails:
- `WHEEL`: `delta_x:f32 | delta_y:f32 | delta_mode:u32`
- `KEYDOWN/UP/PRESS`: `keycode:u32 | location:u32 | repeat:u8 | key_len:u16 | key:[u8] | code_len:u16 | code:[u8]`
- `INPUT/CHANGE`: `text_len:u32 | text:[u8]`
- `COMPOSITION_*`: `data_len:u32 | data:[u8]`
- `RESIZE`: `width:f32 | height:f32 | dpr:f32`
- `TOUCH_*`: `n_touches:u8` then `n × { id, x, y, radius_x, radius_y, force }`
- `SCROLL/_START/_END`: `scroll_x:f32 | scroll_y:f32`

Bump `EVENT_BYTES_LEN` 256 → 4096.

### A.3 `AzStartup_dispatchEvent` extension (2d, depends on B.1)
Rewrite the kind switch (`eventloop.rs:842-932`) so each branch
hit-tests, walks the wasm-resident StyledDom to find the matching
`CallbackData.event`, marshals the per-kind payload into native
`KeyboardState`/`MouseState`, then invokes the cb.

Promote `AzStartup_hitTest` (currently the M9-4 stub) to a real bbox
walk using `LayoutWindow.positioned_rects` (computed during
`initLayoutCache`).

### A.4 JS listener wiring (1d)
Replace the single `body.addEventListener('click')` with a discovery
loop in `loader_js.rs:570-580` binding one listener per JS event name
to `body`. Plus per-kind payload encoders (`azEncodeWheel`,
`azEncodeKey`, `azEncodeInput`, `azEncodeComposition`, `azEncodeResize`).

### A.5 Resize/focus/scroll plumbing (0.5d)
`window.addEventListener('resize')` → `azDispatchResize`.
`document.addEventListener('focusin'/'focusout')` for focus events.

### A.6 Touch + drag (1.5d, deferred — TodoMVC doesn't need)
TouchList + DataTransfer encoding. Gated on a real-app example
needing them.

---

## Stage B — Production-grade patch coverage (7d in-scope + 1.5d deferred)

### B.1 Hydrate `current_dom: Option<StyledDom>` wasm-side (2d, HIGH RISK)
After `AzStartup_initLayoutCache` writes the layout cb's return blob,
parse it into a typed `StyledDom`. Two paths:
- **Preferred**: make the layout cb return a `StyledDom` directly (wrap
  user's `Dom` in `Dom::style(&Css::empty())` inside the lifted
  wrapper). Avoids depending on lifting the cascade.
- **Fallback**: JS-side diff against rendered HTML (worse architecture
  but ships without StyledDom lift risk).

Risk: `StyledDom`'s field layout requires const-pool loads that may
not survive the lift (same hazard that hit M8.7c-3's
`Box::new(StructLiteral)`).

### B.2 Diff loop in `AzStartup_dispatchEvent` (1d)
On `RefreshDom`: re-run layout cb → new blob → parse → call
`azul_core::diff::reconcile_dom_with_changes` (already exists at
`core/src/diff.rs:457-466`) → encode each `(NodeChangeSet, RelayoutScope,
old/new node_data)` triple into TLV patches. Update `state.current_dom`.

### B.3 Extend TLV format (2.5d)
Common header for all kinds: `kind:u8 | node_idx:u32 | payload_len:u32`.
Kinds 1-12:
- 1 SetText, 2 SetAttr, 3 RemoveAttr, 4 SetInlineStyle, 5 RemoveNode,
  6 InsertNode, 7 MoveNode, 8 ReplaceSubtree, 9 Focus, 10 ScrollTo,
  11 AddClass, 12 RemoveClass.

Rename `AzStartup_buildCounterPatch` →
`AzStartup_buildPatchStream(diff_ptr, out_buf, cap) -> u32`. Translate
each diff result into the smallest patch sequence
(`TEXT_CONTENT only` → kind=1; `INLINE_STYLE` → kind=4 with
`format_css()`; `NODE_TYPE_CHANGED` → kind=8 at deepest convergent
ancestor; etc.).

### B.4 JS decoder + dynamic listener re-attach (1.5d)
`loader_js.rs:547-568` switch on every kind. After every patch batch,
walk `[data-az-cb][data-az-wasm]:not([data-az-bound="1"])` to discover
newly-inserted cb-bearing nodes and instantiate their per-cb wasms.

### B.6 Wasm-side `html_render::render_node_recursive` for InsertNode subtrees (1.5d, deferred)
Required if kind=6 InsertNode payloads need server-rendered HTML at
runtime. Workaround for now: route through a JS-side renderer that
walks the inserted subtree from a fresh sub-`StyledDom`.

---

## Stage C — CallbackChange / `CallbackInfo` plumbing (6.5d + 2d deferred)

### C.1 Real `CallbackInfo` wasm-side (2d)
Today the cb is invoked with `event_bytes_ptr` masquerading as a
`CallbackInfo*` — wrong. Build a proper `CallbackInfo` blob with stub
`ref_data` (empty fonts/image-cache), allocate
`Arc<Mutex<Vec<CallbackChange>>>` wasm-side, pass real pointer.
Drain via `CallbackInfo::take_changes` after cb returns.

### C.2 Per-variant patch encoders (2.5d)
| `CallbackChange` | Web action |
|---|---|
| `SetFocusTarget` | kind=9 Focus |
| `StopPropagation`/`PreventDefault` | dispatch-return flag → `event.stopPropagation()` / `preventDefault()` |
| `AddTimer`/`RemoveTimer` | kind=13/14 → JS `setInterval`/`clearInterval` + new export `AzStartup_fireTimer(state, id)` |
| `ChangeNodeText` | kind=1 SetText |
| `ChangeNodeCssProperties` / `OverrideNodeCssProperties` | kind=4 SetInlineStyle |
| `ScrollTo` | kind=10 ScrollTo |
| `ScrollIntoView` | kind=15 ScrollIntoView |
| `AddImageToCache` | kind=16 RegisterImage → JS `URL.createObjectURL(Blob([png]))` → `azImageCache.set(id, url)` |
| `OpenMenu` | kind=17 → custom `<div class="az-menu">` overlay |
| `ShowTooltip`/`HideTooltip` | kind=18/19 → cursor-positioned `<div>` |
| `SetCopyContent`/`SetCutContent` | kind=21 ClipboardWrite → `navigator.clipboard.writeText` |
| `InsertChildNode`/`DeleteNode` | RefreshDom path → diff produces kind=6/5 |
| `SwitchRoute` | kind=22 → `azNavigate(path)` (already exists) |
| `ModifyWindowState` | `document.title = ...`, `window.resizeTo` (browser-best-effort) |
| `AddThread`/`RemoveThread` | stub + warn (no web threading yet) |

### C.3 Timer pump (1d)
JS `azTimers = new Map()`. AddTimer →
`setInterval(() => azMini.AzStartup_fireTimer(state, BigInt(id)), ms)`.
`fireTimer` runs the same dispatch dance but uses the timer's stored
cb fn-ptr. `interval_ms == 0` means rAF (animation timers).

### C.4 Focus / scroll / route mapping (1d)

### C.5 Clipboard / drag-and-drop / threads (2d, deferred)

---

## Stage D — `examples/c/azul-bench.c` (5d)

### D.1 Bench example (1d)
Matches the krausest non-keyed/keyed shape: `BenchModel { rows[], selected_id, next_id }` with 8 callbacks (run, runlots, add, update, clear, swaprows, select_row, delete_row) and a `<table class="table">` layout. Adjectives/colors/nouns arrays verbatim from krausest for label compat.

### D.2 Harness (1d)
`examples/c/bench-harness.html` — wraps `performance.now()` +
`MutationObserver` on `<tbody>` (same primitive as krausest). Taps
`patchesLen` for wire-byte counting. Runs 10 warmup + 25 measure per op.

### D.3 Comparison script (0.5d)
`scripts/bench-compare.py` reads our JSON, fetches krausest's
`current.html` for React/Preact/Svelte numbers, builds a Markdown
comparison table.

### D.4 Debug round (2d budget)
Each op surfaces gaps — swap → MoveNode patch must work; select_row
→ `CallbackInfo.hit_dom_node` must surface `data-id` attribute.

### D.5 Bench report (0.5d)
`scripts/BENCH_REPORT_*.md` with measured numbers.

### Risks
- `create10000` may exceed bump-allocator scratch (12+ MB of NodeData).
  Pre-grow wasm memory to 32 MiB. If still tight, wire up `__rust_dealloc`
  (currently noop).
- `reconcile_dom` is O(n) but with allocations per bucket — may take
  50-100 ms on 10000 rows. Mitigation: `Update::KeyedFastPath` hint that
  skips reconcile when user knows the keys are intact.

---

## Stage E — Docs + showcase (1.5d)

- `dll/src/web/EVENT_PATCH_SCHEMA.md`: canonical TLV/event spec
  hand-maintained next to the code.
- `examples/c/README-web.md`: how to serve any C example on the web.
- Asciinema cast of bench page running create1k → swap → clear.
- Deploy `azul-bench.bin` to `bench.azul.rs/`; embed in README.

---

## Dependency graph + critical path

```
A.1 ─┬─> A.4 ─┐
A.2 ─┘        ├──> A.3 ──> (real hit-test) ──┐
B.1 ─> B.2 ──┘                                ├──> Stage D
B.3 ─> B.4 ────────────────────────────────────┘
C.1 ─> C.2/C.3/C.4 ─> (RefreshDom path completeness)
                            │
                            └─> Stage D needs C for swap-row selection state
Stage D ──> Stage E
```

Critical path: **B.1 → B.2 → B.3 → B.4 → D.1 → D.4 ≈ 10 days realistic**.

## Totals

| Stage | Effort | Risk |
|---|---|---|
| A — events | 4.5 d (+1.5 d deferred touch/drag) | medium (hit-test) |
| B — patches + diff | 7 d (+1.5 d deferred wasm html_render) | **high** (StyledDom lift) |
| C — CallbackChange plumbing | 6.5 d (+2 d deferred) | medium |
| D — benchmark | 5 d | medium |
| E — docs/showcase | 1.5 d | low |
| **TOTAL in-scope** | **24.5 d** | |
| **TOTAL incl. deferred** | **29 d** | |

## Critical files for implementation

- `dll/src/web/eventloop.rs` — event_kind, dispatchEvent, hit-test, patch encoder
- `dll/src/web/loader_js.rs` — JS listeners, payload encoders, patch decoder, timer pump
- `dll/src/web/html_render.rs` — preserve `data-az-ev` attribute emission
- `core/src/diff.rs` — `reconcile_dom_with_changes` (already exists, reuse)
- `layout/src/callbacks.rs` — `CallbackChange` enum (already exists, reuse), `CallbackInfo::take_changes`
