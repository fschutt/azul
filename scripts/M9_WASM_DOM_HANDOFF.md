# M9 — Move the DOM into WASM, kill the JS-side hit-test

> **For the next Claude session.** Read this whole document first.
> Then read `scripts/M8.9_REMILL_HANDOFF.md` (now complete — Phases
> 1-4 shipped) and `scripts/STATUS_REPORT_2026_05_18.md` for the
> M8.9 close-out state. The six phases below land the
> "WASM-resident DOM" architecture: the server gives first paint,
> JS becomes a thin event-and-patch ferry, and a fully self-
> sufficient `HeadlessApp` inside `mini.wasm` owns layout,
> hit-test, dispatch, and DOM queries.

## The framing that matters

M8.9 closed the lift pipeline. Every callback in the on_click
chain compiles to working wasm, the layout cb compiles to a
285 KB wasm, and CSS shipping to the browser works. **What's
missing is the runtime architecture to actually CALL the layout cb
from inside wasm.** Today the layout wasm is served but never
instantiated; JS does hit-test via `id="az_N"` regex and hardcodes
the `textContent =` update.

The user's architecture (per 2026-05-18 clarification) is:

```
SERVER:  Pre-render HTML for instant first paint.
         Lift mini/cb/layout wasms. Serve over HTTP.

JS:      Render initial HTML.
         Instantiate all wasms.
         AzStartup_init → _hydrate → _initLayoutCache.
         Per event: encode (kind, raw_x, raw_y, mods),
                    call AzStartup_dispatchEvent,
                    decode returned TLV patches,
                    apply to browser DOM.

WASM:    Owns a complete "WASM DOM" — StyledDom + LayoutWindow
         (layout cache, font manager, hit-tester). NEVER asks JS
         about DOM state. JS only fulfills one request:
         __az_resolve_callback(fn_addr) → table_idx.

         Per dispatch: hit-test on its own layout cache,
                       look up cb fn-addr from current_dom,
                       call cb via call_indirect,
                       if RefreshDom: re-run layout, diff,
                                      emit TLV patches.

         User callbacks query the WASM DOM via normal libazul
         functions (AzCallback_getLayoutForNode, AzDom_getParent,
         etc.). These are lifted via the transitive walker like
         any other dep — no special wasm-side stubs needed.
```

The "WASM DOM" is the source of truth. The browser DOM is a
visual mirror updated only via patches.

## What's already committed (M8.9 — don't redo)

```
9780a92b3  docs: refresh web.md + add STATUS_REPORT_2026_05_18.md
b65538b8e  web: pipe every wasm through `wasm-opt -Oz` post-link
43ad8b8b8  web: fix CSS cascade output + shrink wasm + clean scratch dir
91c5ebe99  web: M8.9-4 Linux scaffolding in build.rs
9e15d499a  web: fix duplicate-symbol error in transitive batched lift
e513b8235  web: realloc + dealloc bridge bodies for the bump allocator
0e303b9e5  web: M8.9-3b transitive batched lift via ARM64 bytes-scan
ad2527cab  web: M8.9-3a az_remill_lift_batch + eventloop batched lift
f799ae05b  web: M8.9-2 in-process compile + wasm_link via llvm::Linker
8d1b5316d  web: M8.9-1-fix in-process lift trace divergence
```

Verified state:
- e2e (counter 5→12 over 7 clicks) passes in both subprocess and
  `AZ_NATIVE_REMILL=1` modes.
- CSS cascade ships to browser; button has native theme.
- Layout cb lifts (146 deps, 285 KB wasm) but is not instantiated.
- 16 MiB initial wasm memory (was 2 MiB).
- Scratch dir auto-cleanup on `RemillTranspiler::drop`.

## The 4 concrete blockers (in priority order)

| # | Blocker | Where the fix lives |
|---|---|---|
| 1 | Wrapper synthesizes `Callback`-shape sig (`i64 lo, i64 hi, i32 info_ptr) → i32`) for ALL kinds, including layout-cb. Layout-cb is `fn(AzRefAny, LayoutCallbackInfo) -> AzStyledDom`. The StyledDom return uses ARM64's hidden X8 register, which the wrapper never seeds. | `dll/src/web/transpiler_remill.rs`: `Pcs` enum at ~67-101, `signature_for_callback_kind` at ~244-253, `emit_wrapper_args_and_prologue` at ~259, `emit_wrapper_return` at ~315-352, `lift_function` at ~1519-1542 |
| 2 | `LayoutCallbackInfo` carries `*LayoutCallbackInfoRefData` referencing `&ImageCache`, `&FcFontCache`, `Arc<SystemStyle>`, `Option<&RouteMatch>`. None are constructible from JS or the current wasm side. Need wasm-side stubs (empty caches, default style) backed by `AzStartup_alloc`. | New `AzStartup_buildLayoutInfo` in `dll/src/web/eventloop.rs`; struct shapes in `azul-core/src/callbacks.rs:521-534, 570-587` |
| 3 | Returned StyledDom would land in the wrapper's stack-scratch buffer (freed on wrapper return). Must memcpy into bump heap so it survives the call boundary. | Wrapper epilogue in `emit_wrapper_return` (`transpiler_remill.rs:315-352`) |
| 4 | Even with 1-3 working, nothing CALLS the layout cb from inside WASM. Need `AzStartup_initLayoutCache(refany_ptr, viewport_w, viewport_h)` which invokes layout cb → builds StyledDom → runs solver3 → stashes everything in `EventloopState`. | New export in `dll/src/web/eventloop.rs`; new entry in `EVENTLOOP_SYMBOLS` (`mod.rs:45-52`) + `signature_for_eventloop_fn` (`transpiler_remill.rs:123-189`) |

## The plan — 6 phases

Each phase is one shippable commit. Verify per phase against the
existing e2e (counter 5→12) PLUS the new layout-cb assertions
listed inline.

### Phase 1 — Fix the layout-cb wrapper signature (small)

**Goal:** Layout cb is CALLABLE from inside wasm (with the right
args set up).

**Changes:**

1.1. In `Pcs` enum (`transpiler_remill.rs:~67`):
```rust
pub enum Pcs {
    GprI64,       // existing
    GprI64Pair,   // existing
    GprPtr32,     // existing
    Wreg,         // existing
    HiddenPtrReturn { x8_offset: u64, ret_buf_size: u64 },  // NEW
}
```

1.2. `signature_for_callback_kind("LayoutCallback")` returns:
- args: `[AzRefAny → GprI64Pair(X0/X1), LayoutCallbackInfo →
  GprPtr32(X2)]`
- ret: `HiddenPtrReturn { x8_offset: 672, ret_buf_size: 120 }`
  (size of `AzStyledDom`; verify via `sizeof::<StyledDom>()` at
  compile time or hardcode for now)

1.3. `emit_wrapper_args_and_prologue` writes the X8 slot:
```llvm
  ; State.X8 (offset 672 from State pointer) ← bump-alloc'd ret buf
  %ret_buf = call i32 @AzStartup_alloc(i32 120)
  %ret_buf_i64 = zext i32 %ret_buf to i64
  %x8_slot = getelementptr inbounds i8, ptr %state_buf, i64 672
  store i64 %ret_buf_i64, ptr %x8_slot, align 8
```

1.4. `emit_wrapper_return` for `HiddenPtrReturn`: read back the
ret_buf offset and return it as i32 (the caller will treat it
as a wasm offset pointing to the StyledDom).

1.5. Thread `kind: &str` through `Transpiler::lift_function` (so
the lifter knows it's a layout cb) and propagate to
`signature_for_callback_kind`. Either add a `lift_function_kind`
method or change `lift_function`'s signature.

1.6. Update `mod.rs::lift_layout_callbacks` to pass `"LayoutCallback"`
and `mod.rs::discover_and_transpile_callbacks` to pass `"Callback"`.

**Verification:**
- `node /tmp/e2e.js` still passes 5→12 (no regression on on_click).
- A new `/tmp/layout-probe.js` script (see Phase 2 below) gets
  past the instantiation step without trapping on signature
  mismatch.
- The layout wasm's `callback` export should now have signature
  `(i64, i64, i32) → i32` where the return is the StyledDom wasm
  offset.

**Commit:** `web: M9-1 layout-cb wrapper with X8 hidden return`

### Phase 2 — LayoutCallbackInfo builder + instantiate from JS (medium)

**Goal:** Layout cb runs end-to-end when called from JS, returns
a StyledDom offset (we don't yet do anything with the result).

**Changes:**

2.1. `AzStartup_buildLayoutInfo(viewport_w, viewport_h, theme) ->
info_ptr` in `dll/src/web/eventloop.rs`:
```rust
#[no_mangle]
pub extern "C" fn AzStartup_buildLayoutInfo(
    viewport_w: u32, viewport_h: u32, theme: u32,
) -> u32 {
    // Bump-allocate LayoutCallbackInfoRefData with wasm-side stubs:
    //   image_cache    = ImageCache::default()
    //   gl_context     = OptionGlContextPtr::None
    //   fc_cache       = FcFontCache::default() (empty)
    //   system_style   = Arc::new(SystemStyle::default())
    //   active_route   = None
    //   window_state   = synthetic FullWindowState with viewport size
    //   ... etc per `callbacks.rs:521-534`
    // Bump-allocate LayoutCallbackInfo wrapping the above.
    // Return the LayoutCallbackInfo offset as u32.
}
```

2.2. In `loader_js.rs`, after `azHydrate()`:
- Scan `<link rel="preload" href="/az/layout/...">` for the layout
  cb URL (or add a `<meta data-az-layout-wasm="...">` in
  `html_render.rs` for clarity).
- Instantiate the layout wasm with the SAME `env` as cb wasms
  (`env.memory = mini.memory`, `env.__indirect_function_table =
  azTable`, Proxy fallback).
- Store the `callback` export's table index in
  `azLayoutCbTableIdx`.

2.3. Add `EVENTLOOP_SYMBOLS` entry `AzStartup_buildLayoutInfo` +
matching `signature_for_eventloop_fn` entry.

**Verification:** new probe script `/tmp/layout-probe.js`:
```javascript
// Instantiate mini + layout, hydrate, then call layout cb directly.
// Expected: returns nonzero StyledDom offset; no trap.
const styledDomPtr = layoutI.exports.callback(
    refanyLo, refanyHi,
    mini.AzStartup_buildLayoutInfo(800, 600, 0)
);
console.log("StyledDom offset:", styledDomPtr);
// Sample first 32 bytes to verify it's a real StyledDom shape.
```

**Commit:** `web: M9-2 LayoutCallbackInfo builder + JS layout-cb instantiation`

### Phase 3 — Embed LayoutWindow in EventloopState (large)

**Goal:** The "WASM DOM" exists. After init, `EventloopState`
carries the full layout cache.

**Changes:**

3.1. Extend `EventloopState` (`eventloop.rs:~109`):
```rust
pub struct EventloopState {
    // ... existing fields ...
    pub layout_window: Option<LayoutWindow>,  // NEW
    pub layout_cb_table_idx: u32,             // NEW (set by JS)
    pub cb_fn_cache: HashMap<usize, usize>,   // NEW: node_idx → cb fn_addr
}
```

3.2. `AzStartup_setLayoutCbTableIdx(state, idx)` — JS calls this
once after instantiating the layout wasm.

3.3. `AzStartup_initLayoutCache(state, viewport_w, viewport_h, theme)`:
```rust
#[no_mangle]
pub extern "C" fn AzStartup_initLayoutCache(...) -> u32 {
    let info_ptr = AzStartup_buildLayoutInfo(viewport_w, viewport_h, theme);
    // Call layout cb via __az_call_indirect using the stashed
    // layout_cb_table_idx. This returns a StyledDom offset.
    let styled_dom_ptr = az_call_indirect_layout(
        state.layout_cb_table_idx,
        state.app_data_lo, state.app_data_hi,
        info_ptr,
    );
    // Read the StyledDom from the bump-alloc'd region.
    let styled_dom: StyledDom = unsafe { ptr::read(styled_dom_ptr as *const _) };
    // Build LayoutWindow + run solver3.
    let mut layout_window = LayoutWindow::new(FcFontCache::default());
    layout_window.do_the_layout(&styled_dom, /* viewport */);
    // Populate cb_fn_cache by walking styled_dom.
    for (node_idx, node) in styled_dom.iter_nodes() {
        if let Some(fn_addr) = extract_cb_addr(node) {
            state.cb_fn_cache.insert(node_idx, fn_addr);
        }
    }
    state.current_dom = Some(styled_dom);
    state.layout_window = Some(layout_window);
    1  // success
}
```

3.4. **CRITICAL refactor:** `LayoutCallback.cb` is currently a
host `extern "C" fn` pointer. In wasm, that pointer doesn't
exist — we need a table index instead. Either:
- **(A)** Keep `cb` as fn-ptr; provide a wasm-only `extern "C"
  fn invoke_layout_cb_trampoline` that calls __az_call_indirect.
- **(B)** Refactor `LayoutCallback` to be `enum {
  Native(fn-ptr), TableIdx(u32) }` cfg-gated by `target_arch =
  "wasm32"`.

**Recommendation: (A)** — minimal upstream change. The
trampoline is a tiny wasm-only function.

3.5. Add `__az_call_indirect_layout` to the helper IR bridge
generators. Signature differs from `__az_call_indirect`
(returns i32 = StyledDom offset, takes 3 args instead of 4).

**Verification:** new probe script `/tmp/layout-init-probe.js`:
```javascript
// After hydrate + setLayoutCbTableIdx:
const rc = mini.AzStartup_initLayoutCache(state, 800, 600, 0);
console.log("initLayoutCache rc:", rc);  // expect 1
// Verify EventloopState.current_dom is now Some via a debug export.
// Verify EventloopState.layout_window.layout_cache has results.
```

**Commit:** `web: M9-3 WASM-resident LayoutWindow + initLayoutCache`

### Phase 4 — WASM-side hit-test (medium)

**Goal:** Kill the JS-side `azNodeIdxFromEvent` regex hack.

**Changes:**

4.1. `AzStartup_hitTest(state, x_f32_bits, y_f32_bits) → u32`
  (returns node_idx or `SENTINEL_NO_NODE = 0xFFFFFFFF`):
```rust
#[no_mangle]
pub extern "C" fn AzStartup_hitTest(state: *mut EventloopState, x: u32, y: u32) -> u32 {
    let x = f32::from_bits(x);
    let y = f32::from_bits(y);
    let lw = state.layout_window.as_ref()?;
    let dom = state.current_dom.as_ref()?;
    // Use solver3's FullHitTest::new(...) or equivalent.
    let hit = lw.hit_test(dom, LogicalPosition::new(x, y));
    hit.map(|h| h.node_id.index() as u32).unwrap_or(SENTINEL_NO_NODE)
}
```

4.2. Rewire `AzStartup_dispatchEvent` (`eventloop.rs:~422`):
```rust
let node_idx = if event_buf_supplied_idx == SENTINEL_NO_NODE {
    AzStartup_hitTest(state, x, y)
} else {
    event_buf_supplied_idx  // backwards compat
};
let cb_fn_addr = state.cb_fn_cache.get(&node_idx)?;
let table_idx = __az_resolve_callback(cb_fn_addr);
let update = __az_call_indirect(table_idx, refany_lo, refany_hi, info_ptr);
```

4.3. Loader.js: drop the `azNodeIdxFromEvent` regex. Encode the
event with `node_idx = SENTINEL_NO_NODE` and let WASM hit-test.

**Verification:**
- `node /tmp/e2e.js` still passes 5→12.
- Click handler in loader.js no longer references
  `getElementById` or `id="az_N"`. Use the network tab to confirm
  the request shape changed.
- Browser DOM has no `id="az_*"` attributes (they become
  decorative for CSS only).

**Commit:** `web: M9-4 WASM-side hit-test replaces JS azNodeIdxFromEvent`

### Phase 5 — Diff + TLV patch emission (medium)

**Goal:** Kill the hardcoded `textContent =` JS update. WASM emits
patches; JS applies them blindly.

**Changes:**

5.1. After `Update::RefreshDom` in `AzStartup_dispatchEvent`:
- Re-run the layout cb via `AzStartup_runLayout(state)` (same
  flow as `initLayoutCache` but reads existing cached `current_dom`
  for diff baseline).
- Save the OLD StyledDom; build the NEW.
- Call `azul_core::diff::reconcile_dom(old, new)` (or similar — check
  what's there).
- Translate the diff into TLV patches.

5.2. Extend TLV schema (currently only `SetText`, kind=1):
```text
kind=1 SetText        u32 node_idx, u32 text_len, [u8] text
kind=2 SetAttr        u32 node_idx, u16 name_len, [u8] name, u16 val_len, [u8] val
kind=3 SetInlineStyle u32 node_idx, u16 css_len, [u8] css   (sets style.cssText)
kind=4 RemoveNode     u32 node_idx
kind=5 InsertNode     u32 parent_idx, u32 before_sibling_idx, u32 html_len, [u8] html
kind=6 MoveNode       u32 node_idx, u32 new_parent, u32 before_sibling
kind=7 ReplaceSubtree u32 node_idx, u32 html_len, [u8] html
```
For Vec-resize cases (list grows from 3 to 5 items), emit
`ReplaceSubtree` on the parent — simpler than per-item Insert.

5.3. `AzStartup_dispatchEvent` returns `(u32 patch_buf_ptr, u32 patch_buf_len)`.
The buffer is bump-alloc'd before encoding.

5.4. Loader.js: extend `azApplyPatches` (already has SetText)
with the new kinds.

**Verification:**
- Hello-world counter still increments via patches (no
  `textContent =` hardcode).
- A multi-button demo (TBD: write `examples/c/counter-list.c`
  with a "Add row" button) tests Insert/Replace patches.

**Commit:** `web: M9-5 diff + TLV patch emission, kill textContent= hack`

### Phase 6 — Loader.js minimization + cleanup (small)

**Goal:** Loader.js is purely event-encode + patch-apply. No DOM
queries, no node-id regex, no cb-direct-call.

**Changes:**

6.1. Delete `azNodeIdxFromEvent`, `azInvokeCbDirect`, the
hardcoded `getElementById('az_1').textContent = ...` chain.

6.2. Rename `azWireListeners` to be event-kind-driven only:
each listener encodes (kind, raw_x, raw_y, mods, time) and calls
`AzStartup_dispatchEvent`.

6.3. After dispatch, decode the returned patch buffer and apply
via the extended `azApplyPatches`.

6.4. Free the patch buffer via `mini.AzStartup_free(ptr, len)`.

6.5. Drop `azNodeCbFns` Map (cb-fn-cache moved to WASM).

**Verification:** loader.js no longer mentions `data-az-cb`,
`id="az_*"`, `textContent`, `getElementById`. The HTML can drop
those attributes too (they're decorative for the initial paint
only — once WASM takes over, they're unused).

**Commit:** `web: M9-6 minimize loader.js to event-encode + patch-apply only`

## Safety net

- **Tag** `m8.9-victory` (NEW — create at HEAD of `layout-debug-clean`
  before starting M9). `git reset --hard m8.9-victory` returns
  to the M8.9 close-out state.
- **Tag** `m8.7c-victory` → commit `7530483aa` remains the
  pre-M8.9 fallback.
- **Branch** `backup/m8.7c-victory-2026-05-16` mirrors the same.

## What NOT to break

- The on_click counter e2e (5→12 over 7 clicks). Every phase
  must pass this BOTH with the new dispatch path AND with the
  legacy direct cb call (until phase 6 deletes the legacy).
- Subprocess pipeline (`web-transpiler` without
  `web-transpiler-static`). The lift pipeline is orthogonal to
  the dispatch architecture; both should work.
- M8.9-1 fix (SimpleTraceManager mirror) — never re-introduce
  the trace divergence.
- M8.9-3b fix (`export_as` as .o stem) — never go back to
  fn-name-as-stem.
- The CSS cascade ships to the browser (`apply_ua_css` +
  `compute_inherited_values` in `core/src/styled_dom.rs`).

## Architectural decisions to make UPFRONT

Before starting Phase 1, ask the user (or commit to a choice
in writing):

1. **StyledDom return buffer**: (A) Bump-alloc fresh per layout
   call, leak it. (B) Pass dest buffer as wrapper arg. **Default
   answer: A** — re-layout is rare, leak is bounded by # layouts
   per session, simpler diff.

2. **LayoutCallback cb-ptr → table-idx**: (A) Keep `cb` as fn-ptr,
   wasm-only trampoline. (B) Refactor `LayoutCallback` enum
   cfg-gated. **Default answer: A** — minimal upstream change.

3. **Diff algorithm**: use existing `azul_core::diff::reconcile_dom`?
   Or write a simpler StyledDom-aware diff? Check what's there at
   `azul-core/src/diff.rs` BEFORE starting Phase 5.

## Glossary of files

| file | role |
|---|---|
| `dll/src/web/eventloop.rs` | `EventloopState`, AzStartup_* exports. Phase 3-5 add fields + new exports here. |
| `dll/src/web/transpiler_remill.rs` | Wrapper synthesis + lift pipeline. Phase 1 fixes `Pcs` + `emit_wrapper_*`. |
| `dll/src/web/mod.rs` | `EVENTLOOP_SYMBOLS`, `lift_layout_callbacks`, `discover_and_transpile_callbacks`. Phase 1 threads `kind`; Phase 3 adds new symbol entries. |
| `dll/src/web/headless.rs` | DEAD CODE in M8.9. Either delete it or actually wire it (Phase 3 — bag of EventloopState fields). |
| `dll/src/web/html_render.rs` | Server-side render. Phase 5 may need to invoke `html_render::render_recursive` for `InsertNode`/`ReplaceSubtree` HTML blobs. |
| `dll/src/web/loader_js.rs` | The JS bootstrap. Phase 2 adds layout instantiation; Phase 4 drops hit-test; Phase 6 deletes everything else. |
| `dll/src/web/hydration.rs` | Pre-defined `HydrationPayload`. Not used today; M9 may obsolete it OR repurpose for serializing the wasm-side `cb_fn_cache`. |
| `dll/src/web/server.rs` | HTTP. Phase 5 may add a POST endpoint for the patch-apply round-trip if patches are too big for return value (unlikely). |
| `azul-core/src/callbacks.rs:521-587` | `LayoutCallbackInfo` / `LayoutCallbackInfoRefData` struct shapes. Phase 2 reads these to build the wasm-side stubs. |
| `azul-core/src/styled_dom.rs` | StyledDom — what the layout cb returns. Phase 3 reads from this. |
| `azul-layout/src/window.rs` | `LayoutWindow` — Phase 3 embeds this in EventloopState. |
| `azul-layout/src/solver3/` | Layout solver — Phase 3 invokes via `LayoutWindow.do_the_layout`. |
| `azul-core/src/diff.rs` | Existing diff machinery (if any). Phase 5 reads or replaces. |
| `scripts/STATUS_REPORT_2026_05_18.md` | M8.9 close-out snapshot. |
| `scripts/M8.9_REMILL_HANDOFF.md` | Predecessor handoff — for context. |
| `doc/guide/en/internals/web.md` | Current architecture doc. M9 rewrites the dispatch section + Phase B/C at the end. |

## Verification scripts

Save under `/tmp/`:

- `/tmp/e2e.js` (already exists from M8.9) — counter 5→12 in 7
  clicks. Every M9 phase must keep this passing.
- `/tmp/layout-probe.js` (Phase 2) — instantiate layout wasm,
  call directly, sample the returned StyledDom bytes.
- `/tmp/layout-init-probe.js` (Phase 3) — call
  `AzStartup_initLayoutCache`, verify rc=1 + downstream queries
  work.
- `/tmp/m9-e2e.js` (Phase 6) — full end-to-end via WASM-side
  dispatch only (no `id="az_*"` regex, no `textContent =`
  hardcode in loader.js).

## Phase-commit summary (target diffs)

| Phase | LOC est. | Files touched |
|---|---:|---|
| 1 — wrapper | ~120 | `transpiler_remill.rs`, `mod.rs` |
| 2 — info + JS instantiate | ~200 | `eventloop.rs`, `loader_js.rs`, `html_render.rs` |
| 3 — LayoutWindow embed | ~400 | `eventloop.rs` (most), `mod.rs`, possibly `azul-core/callbacks.rs` for the cb-ptr refactor |
| 4 — hit-test | ~150 | `eventloop.rs`, `loader_js.rs` |
| 5 — diff + patches | ~250 | `eventloop.rs`, `loader_js.rs` (JS patch decoder) |
| 6 — loader cleanup | ~150 (mostly deletions) | `loader_js.rs`, `html_render.rs` |
| **total** | **~1270** | |

## Final note

The user's vision is **"WASM is fully self-contained"**. Every
phase should make the WASM side MORE self-sufficient, not less.
If a phase ends up adding more JS↔WASM round-trips beyond
`__az_resolve_callback(fn_addr) → table_idx`, something is wrong.

User callbacks query DOM state via NORMAL libazul functions
(`AzCallback_getLayoutForNode`, `AzDom_getParent`, etc.) —
these are lifted via the existing transitive walker like any
other dep. No special wasm-side surface needed for them. The
lifter already handles this because they're just `Az*` functions
in api.json classified as `Framework` → `FnClass::Recursable`.

Don't update `doc/guide/en/internals/web.md` stage by stage.
Rewrite the dispatch + Phase B/C sections ONCE at the end of M9
with the final architecture, mirroring how M8.9 was handled.

Good luck.
