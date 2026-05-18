# Azul web backend — status report

**Date:** 2026-05-18
**Branch:** `layout-debug-clean`
**Pipeline state:** in-process remill+LLVM+LLD lift, batched, with CSS cascade output. Layout-cb lifts end-to-end but is not yet dispatched at runtime.

## TL;DR

Hello-world's on_click counter increments through the WASM lift path. The
button now renders with the native azul theme. The full M8.9 pipeline
(lift → opt → llc → wasm-ld → wasm-opt) runs in-process when
`AZ_NATIVE_REMILL=1` is set; subprocess fallback still works. Layout-cb
*lifts* (146 transitive deps → 284 KB wasm) but never runs in the
browser because hit-test + diff/patch + dispatch is still JS-only.

## What works

  - **on_click via lifted WASM.** `node /tmp/e2e.js` →
    `counter 5 → 12 OK` in both subprocess and native modes.
  - **CSS cascade ships to the browser.** Buttons get their full
    Bootstrap-style native theme (`display: inline-flex; background:
    #0d6efdff; padding: 6px 12px; border-radius: 4px; ...`). Fix in
    `core/src/styled_dom.rs:1067` — `StyledDom::create_from_dom` now
    calls `apply_ua_css` + `compute_inherited_values` so
    `computed_values` is populated for the web renderer to read at
    `dll/src/web/html_render.rs:449`.
  - **Native in-process pipeline.** `az_remill_lift`,
    `az_remill_lift_batch`, `az_remill_compile_to_wasm32_obj` (via
    `llvm::Linker`), `az_remill_wasm_link` (via `lld::wasm::link`)
    all link statically into `libazul.dylib` (130 MB total) and
    fire without any external `remill-lift-17`/`opt`/`llc`/`wasm-ld`
    invocations when `AZ_NATIVE_REMILL=1`.
  - **Batched lift.** ARM64 BL/B bytes-scan pre-walks the
    transitive dep graph; one `az_remill_lift_batch` call shares
    one `LoadArchSemantics` (~30 ms) across N items. Hello-world
    eventloop (6 fns) + on_click transitive (14 fns) = **57 ms
    total lift cost** (was ~900 ms sequential — ~16× speedup).
    Layout-cb: 146 deps, 311 ms.
  - **Wasm-opt post-processing.** Every linked .wasm runs through
    `wasm-opt -Oz --strip-debug --strip-producers --vacuum`. Total
    hello-world payload: 325 KB → **295 KB** (-10%).
  - **Scratch dir cleanup.** `RemillTranspiler::drop` wipes
    `$TMPDIR/azul-web-transpiler-<pid>` on exit (override with
    `AZ_REMILL_KEEP_SCRATCH=1` for post-mortem debugging).

## What doesn't work yet

These are explicit feature gaps, not bugs:

### 1. Layout-cb dispatch in the browser (M8.8 Stage 2-3)

The layout-cb wasm is built and served (`/az/layout/layout.<hash>.wasm`,
284 KB), but `loader.js` doesn't instantiate it or call its `callback`
export. The serialized StyledDom isn't embedded in the HTML either, so
even if `AzStartup_dispatchEvent` ran on a click, it couldn't traverse
a hydrated DOM tree.

What's missing (per the M8.8 Stage 3 spec at
`scripts/M8.8_NEW_SESSION_PROMPT.md`):
  - `AzStartup_setAppData(state, refany_ptr)` after hydrate.
  - Embed `HydrationPayload` (already specced at
    `dll/src/web/hydration.rs:24-115`) into a
    `<script id="az-state">` block; `AzStartup_init` decodes it.
  - WASM-side hit-test in `AzStartup_dispatchEvent` (browser-side
    `azNodeIdxFromEvent` still hardcoded).
  - StyledDom serialize/deserialize via postcard (`AzStyledDom_*`).
  - Diff + patch emission after cb returns `Update::RefreshDom`.

### 2. DOM tree navigation from WASM (no exports)

`dll/src/web/eventloop.rs:45-52` exposes only 6 functions:
`alloc/free/init/hydrate/dispatchEvent/registerStateDeserializer`.
There are no `getParent` / `getChildren` / `getNodeAt` / `findById`
exports. The `EventloopState.current_dom: Option<StyledDom>` field at
`:118` is always `None` (`:251`, in `AzStartup_init`).

What's missing:
  - `find_node_by_string_id` on `core/src/styled_dom.rs` (doesn't exist
    anywhere yet — only `NodeHierarchyItem::{parent_id,
    first_child_id, ...}` is there).
  - `AzStartup_getParent/getFirstChild/getNextSibling/findByAzId`
    exports + matching signature entries in `signature_for_eventloop_fn`.
  - Add new symbols to `EVENTLOOP_SYMBOLS` in `dll/src/web/mod.rs:45-52`.

### 3. HeadlessApp / HeadlessWindow layout cache (NOT initialized)

`dll/src/web/headless.rs:25-45` is structurally dead:
  - No `LayoutCache` / `Solver3LayoutCache` / `prev_dom_ptr` /
    `cached_display_list` fields.
  - No `LayoutWindow` (which would drag in `FontManager` +
    `RendererResources` + `CpuHitTester`).
  - `HeadlessApp::new()` is never called from anywhere in `dll/src/web/`.

Instead, `run_web` (`dll/src/web/mod.rs:602-858`) builds raw
`RenderedRoute` entries and a `WebServerState` directly. Each HTTP GET
that misses the pre-rendered cache calls `re_render_body`
(`server.rs:321-335`) → `render_initial_page` → `call_layout`
(`html_render.rs:494-524`), with a fresh empty `ImageCache`,
`OptionGlContextPtr::None`, default `SystemStyle` per request.

`render_initial_page` produces a `StyledDom` but never runs solver3 —
the `HydratedNode.bbox` fields (`hydration.rs:92`) would be `(0,0,0,0)`
if the hydration payload were emitted. A real `LayoutWindow` (with
the M8.8.5 solver3 cache) needs to be added to `HeadlessApp` and
threaded through.

### 4. Bump allocator is leak-only

`@__az_bump_ptr` (`transpiler_remill.rs:2282`) bumps and never frees.
The realloc body (committed 2026-05-18 as `BumpRealloc`) memcpys
old → new and leaks the old region. For short-lived per-request
lifts this is correct; for long-running clients it grows monotonically.

Initial memory is now 16 MiB (was 2 MiB — would trap with "memory
access out of bounds" after ~1 MiB of allocations). JS can `.grow()`
past 16 MiB but doesn't do so proactively. A real allocator
(dlmalloc / wee_alloc) would close this gap; a partial workaround is
the existing classify chain (BumpAlloc/BumpRealloc/BumpDealloc — all
3 Rust allocator API layers covered).

### 5. Linux build is scaffolded, not tested

`dll/build.rs::build_in_process_remill` now branches on Linux vs
macOS (commit `91c5ebe99`), but no Linux host with the matching
cxx-common bundle has actually built it. Windows still skips with a
warning (needs MSVC-built remill + LLVM, no cxx-common Windows
bundle ships from Trail of Bits).

## Performance numbers (hello-world, AZ_NATIVE_REMILL=1, macOS arm64)

| stage                       | time      | notes                              |
|-----------------------------|----------:|------------------------------------|
| Eventloop batched lift (6)  |    27 ms  | One LoadArchSemantics for all 6    |
| on_click transitive (14)    |    30 ms  | Bytes-scan pre-walk + batched      |
| Layout-cb transitive (146)  |   311 ms  | Same flow, larger dep graph        |
| Per-fn compile (opt+llc)    |   ~10 ms  | Per-fn, sequential (FFI_LOCK)      |
| wasm-ld link                |   ~10 ms  | One per wasm output                |
| wasm-opt -Oz                |   ~50 ms  | One per wasm output                |
| **Total first-request**     | ~400 ms   | Cached on subsequent requests      |

Compared to the M8.7c subprocess baseline (~5 s for the same workload),
this is ~12× faster on first request. Cached requests are instant.

## Wasm sizes (hello-world, served from `http://127.0.0.1:8800/az/...`)

| file        | M8.7c | post --gc/--strip | post wasm-opt -Oz | shrink |
|-------------|-------|-------------------|-------------------|--------|
| mini.wasm   |  3275 |              2927 |              2726 | -16.8% |
| on_click    |  8523 |              7984 |              7232 | -15.2% |
| layout      | 313969|            309384 |            284698 |  -9.3% |
| **total**   |325767 |            320295 |            294656 |  -9.6% |

Bigger savings on small wasms because the fixed-cost custom sections
(name/producer) dominate. Layout-cb is dominated by lifted IR which
opt-O2 + wasm-opt already shrink heavily.

## Open issues (file:line)

  - `core/src/styled_dom.rs:1067` — `compute_inherited_values` is now
    called twice in the cascade flow (once via the compact builder,
    once explicitly). The compact form should grow a "read computed
    values" API that the web renderer migrates to; then the explicit
    call goes away.
  - `dll/src/web/html_render.rs:494` — `call_layout` builds fresh
    `ImageCache` + default `SystemStyle` per request. Cache them in
    `WebServerState`.
  - `dll/src/web/mod.rs:850` — Replace ad-hoc `WebServerState` fields
    with `Arc<Mutex<HeadlessApp>>` once HeadlessApp grows a layout
    cache.
  - `dll/src/web/eventloop.rs:464` — TODO: "M8.5d (TBD)... diff against
    state.current_dom". Hydration code populates nothing here.
  - `dll/src/web/transpiler_remill.rs:884` — FFI_LOCK serializes every
    compile call. Once LLVM thread-safety is audited, drop the lock on
    `compile_to_wasm32_obj` only (keep on lift + wasm_link) for ~5×
    speedup on parallel compile via rayon.

## Recommended next priorities

  1. **M8.8 Stage 2 layout-cb probe.** Instantiate
     `layout.wasm` in JS, call its `callback` export with a stub
     refany, see what traps. The lift pipeline is no longer the
     blocker — runtime invocation is.
  2. **Hydration payload emission.** Embed `HydrationPayload`
     (already specced) into the HTML. Without it, no wasm-side DOM
     navigation is possible.
  3. **WASM-side hit-test.** Move the JS-side `azNodeIdxFromEvent`
     hack into `AzStartup_dispatchEvent` using the hydrated bboxes.
  4. **HeadlessApp + LayoutWindow.** Add the real layout cache so
     per-request work is amortized + bboxes are real.

## Commands

```bash
# Build with the in-process pipeline:
cargo build -p azul-dll --release \
  --features "build-dll web web-transpiler web-transpiler-static" \
  --no-default-features

# Run the demo:
cd examples/c && cc -o hello-world.bin hello-world.c -lazul -L../../target/release -I../../dll
DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 AZ_NATIVE_REMILL=1 \
  ./hello-world.bin

# In another terminal:
node /tmp/e2e.js  # expects counter 5 → 12 OK over 7 clicks
open http://127.0.0.1:8800/  # see the styled button in a browser
```

## Environment knobs

| Env var                    | Effect                                            |
|----------------------------|---------------------------------------------------|
| `AZ_NATIVE_REMILL=1`       | Use in-process lift/compile/link (default off)    |
| `AZ_REMILL_DEBUG=1`        | Verbose: trace inventory, target addr dedup, etc. |
| `AZ_REMILL_KEEP_SCRATCH=1` | Don't wipe scratch dir on RemillTranspiler drop   |
| `AZ_REMILL_SKIP_WASM_OPT=1`| Skip wasm-opt post-processing                     |
| `AZ_SKIP_LAYOUT_LIFT=1`    | Skip the layout-cb lift entirely (faster boot)    |
| `AZ_BACKEND=web://HOST:PORT` | Switch from desktop to web mode                |

## Recent session commits (2026-05-18)

```
b65538b8e web: pipe every wasm through `wasm-opt -Oz` post-link
43ad8b8b8 web: fix CSS cascade output + shrink wasm + clean scratch dir
91c5ebe99 web: M8.9-4 Linux scaffolding in build.rs
9e15d499a web: fix duplicate-symbol error in transitive batched lift
e513b8235 web: realloc + dealloc bridge bodies for the bump allocator
0e303b9e5 web: M8.9-3b transitive batched lift via ARM64 bytes-scan pre-walk
ad2527cab web: M8.9-3a az_remill_lift_batch + eventloop batched lift
f799ae05b web: M8.9-2 in-process compile + wasm_link via llvm::Linker + lld::wasm
8d1b5316d web: M8.9-1-fix in-process lift trace divergence — SimpleTraceManager mirror
```
