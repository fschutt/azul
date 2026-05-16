# M8 — HeadlessWindow-simulator architecture for `AZ_BACKEND=web://`

**Drafted:** 2026-05-19
**Authoring agent:** the M0-M7 loop, paused at `76b40895b`.
**Prerequisite reading:**
- `scripts/HANDOFF_2026_05_19.md` — what M0-M7 shipped + what's mocked.
- `scripts/WEB_BACKEND_PLAN_2026_05_18.md` — the original 10-milestone roadmap.
- `doc/guide/en/internals/web.md` — five-phase web backend overview.
- `memory/m6_sroa_gap_2026_05_18.md` — SROA constraints on the lift path.

## End goal

`AZ_BACKEND=web://127.0.0.1:8080 ./hello-world.bin`, open
`http://localhost:8080` in any browser, click the "Increase counter"
button three times, see the counter go `5 → 6 → 7 → 8` with **zero
server round-trips after page load** (verified by `wc -l` on the
server access log). Page state (scroll, focus, etc.) preserved.

## Model

**Single tab = single window.** The browser tab is The Window. Browser
navigation away = window close. Refresh = full reinit. No multi-window
support in M8.

The user's program runs in two halves now:
- **Server-side**: builds the app, holds the RefAny, runs the layout
  callback once to render the initial HTML, then exits the runtime
  loop (the HTTP server still runs).
- **Client-side**: receives the initial HTML + serialized RefAny +
  WASM modules. From the first user interaction onward, EVERYTHING
  is client-side: event dispatch, callbacks, layout re-runs,
  reconciliation, DOM patching.

The server's role after first-render reduces to:
- Serving static assets (HTML, WASM, fonts, images).
- POST `/az/exec/` is preserved as a debugging path but never called
  by the default `listener.js` (per the user's `default = client-side;
  complex callbacks broken-for-now is acceptable` direction).

## What ships to the browser

Four kinds of assets, served from `/az/*`:

| Asset | URL | Role | Source |
|---|---|---|---|
| HTML | `/<route>` | Initial DOM + state-hydration script tag | M0-M7 (works) |
| **Eventloop** | `/az/mini.<hash>.wasm` | HeadlessWindow simulator. Owns RefAny + StyledDom; dispatches events; reconciles DOM | M8 (NEW) |
| **Listener JS** | `/az/listener.js` | Bootstraps everything; native event listeners on root | M8 (rewrite) |
| **Layout cb** | `/az/layout/<hash>.wasm` | The window's layout callback, lifted | M8 (NEW) |
| **Event cbs** | `/az/cb/<sym>.<hash>.wasm` | Per-callback WASMs from M5-M7 | M0-M7 (works) |

Plus one inline:

| Inline | Where | Role |
|---|---|---|
| Initial RefAny state | `<script id="az-state" type="application/json">{...}</script>` in `<head>` | Server-serialized initial app state |

Total assets streamed for hello-world: 1 HTML + 1 mini.wasm + 1
layout.wasm + 1 cb.wasm + 1 listener.js + (fonts/images as needed).
All preloaded via `<link rel="preload">` in `<head>`.

## Component spec

### 1. `azul-mini.wasm` — the eventloop / HeadlessWindow simulator

**The architectural decision that matters most:** this is NOT
remill-lifted from the host binary. It's a **dedicated Rust crate
(`azul-web-eventloop`) compiled directly to `wasm32-unknown-unknown`**.

Rationale:
- The eventloop is azul's OWN code (not user code). We control the
  source. No need to round-trip through remill's State-struct
  representation.
- `cargo build --target wasm32-unknown-unknown` produces small,
  debuggable, idiomatic WASM. The lift pipeline's per-callback ~460B
  output would balloon to MB-scale for the full event dispatch
  logic if we tried to remill it.
- Build-time lifting via the same M5-M7 path can be revisited later
  for parts of the framework where it makes sense; the eventloop
  itself doesn't gain from it.

**Crate layout proposal**: new crate `web-eventloop/` (sibling of
`dll/`, `core/`, `layout/`). Depends on slim subsets of
`azul-core` + `azul-layout` (event dispatch, reconciliation,
StyledDom storage). NO desktop deps (webrender, fonts, GL,
event-loop crates). Probably needs `azul-core`/`-layout` to grow
`no_std` + `wasm32` feature gates that prune the desktop bits.

**Exported surface (called from JS via `listener.js`):**

```rust
// Hydrate initial state from server-serialized JSON bytes.
// Allocates the RefAny + parses StyledDom from the initial HTML
// markers. Called once on bootstrap.
#[no_mangle]
pub extern "C" fn az_init(state_json_ptr: u32, state_json_len: u32);

// Process a single input event. JS marshals (x, y, button, key_code,
// modifier_bits, node_id_at_target) into a fixed byte buffer at
// `event_bytes_ptr`. Returns the count of patch ops produced (0
// means "no DOM mutation needed").
#[no_mangle]
pub extern "C" fn az_dispatch_event(
    event_kind: u32,
    event_bytes_ptr: u32,
    event_bytes_len: u32,
) -> u32;

// Read the patch byte stream produced by the most recent dispatch.
// `out_ptr` is a JS-allocated buffer; eventloop writes TLV-encoded
// ops up to `out_cap` bytes; returns actual bytes written. Pattern:
//   let cap = 65536;
//   let buf = wasm_alloc(cap);
//   let n = az_get_patches(buf, cap);
//   let bytes = read_memory(buf, n);
//   apply_patches(parse_tlv(bytes));
//   wasm_free(buf, cap);
#[no_mangle]
pub extern "C" fn az_get_patches(out_ptr: u32, out_cap: u32) -> u32;

// WASM-side allocator. Exported for JS to use when staging byte
// buffers (event payloads, patch buffers, initial state, etc.).
#[no_mangle]
pub extern "C" fn az_alloc(size: u32) -> u32;
#[no_mangle]
pub extern "C" fn az_free(ptr: u32, size: u32);
```

**Imported surface (provided by JS via the bootstrap import object):**

```rust
// Console logging.
extern "C" {
    fn __az_log_debug(ptr: u32, len: u32);
    fn __az_log_error(ptr: u32, len: u32);
}

// `WebAssembly.Table` of `funcref` for indirect callback dispatch.
// Index 0 reserved for the layout callback. Indices 1+ for event
// callbacks. JS populates as per-callback WASMs load.
extern "C" {
    // type-erased; eventloop uses `call_indirect` with the
    // appropriate type-table entry.
}

// Shared `WebAssembly.Memory` — all modules import the same
// instance, so pointers are interchangeable across modules.
```

**Internal state:**

```rust
// One global static — single-tab/single-window assumption.
static EVENTLOOP: OnceLock<Mutex<EventloopState>> = OnceLock::new();

struct EventloopState {
    /// User's app data, materialised from the initial JSON.
    app_data: RefAny,
    /// Most recent layout output. Hit-tested against on incoming events.
    /// Reconciled against on RefreshDom.
    current_dom: StyledDom,
    /// Callback registry: node_idx → table index in the JS-owned
    /// `WebAssembly.Table`. Populated as per-callback WASMs load.
    cb_table_indices: BTreeMap<u32, u32>,
    /// Pending patch operations produced by the most recent
    /// dispatch — drained by `az_get_patches`.
    pending_patches: Vec<PatchOp>,
}
```

### 2. `layout_callback.<hash>.wasm`

The user's `layout` function, lifted via the same M5-M7 pipeline.
One additional wrinkle: it returns `AzStyledDom` (a struct larger
than 16B), which the AArch64 PCS passes via hidden pointer in X8.
This needs the `Pcs::HiddenPtrReturn` variant I noted in M7-arch's
`signature_for_callback_kind` table.

**Signature shape** (wrapper exposed to eventloop's indirect call):

```
fn layout_callback(
    refany_lo: i64,             // X0
    refany_hi: i64,             // X1
    cbinfo_ptr: i32,            // X2 — pointer to AzLayoutCallbackInfo
    return_buf_ptr: i32,        // X8 — caller-allocated AzStyledDom buffer
) -> ();
```

Or simpler — return the StyledDom as serialized bytes:
- Eventloop calls `layout_callback(refany_lo, refany_hi, info_ptr) -> bytes_ptr`.
- Bytes are a postcard / bincode-equivalent serialization of StyledDom.
- Eventloop deserializes for its in-memory representation.

The serialization path sidesteps the AArch64-hidden-pointer-return
PCS issue AND the cross-module pointer-sharing concern (Dom internal
pointers don't survive a serialize/deserialize round trip on the
allocator side, so we don't have to coordinate allocators across
modules). The cost is one extra serialize + deserialize per
RefreshDom — probably <1ms for hello-world.

**Decision needed:** struct return vs. serialize.

### 3. `/az/cb/<sym>.<hash>.wasm` — per-callback WASMs

Already exists from M5-M7. Each is ~460B (hello-world's on_click).

**Change needed for M8:** the M7 intercept pass currently stubs
framework calls as noops. For real framework integration, the
intercept needs to switch from `noop stub` to `extern import from
azul-mini` for any resolved symbol that exists in azul-mini's
exported surface.

Concretely, change M7's `emit_helper_ir` branch-stubs section:

```rust
for sym in branch_externs {
    let host_addr = branch_target_to_host_addr(sym, fn_addr);
    let resolved = resolve_fn_ptr(host_addr).name;
    if is_in_azul_mini_export_set(&resolved) {
        // M8: import from azul-mini, not stub.
        // (Or: rewrite the call site to call the canonical Az* name
        //  and let wasm-ld emit it as an import.)
        emit_extern_import_from_azul_mini(...)
    } else {
        // Keep M7 noop stub for unrecognized branches (panic
        // handlers, libc, etc.).
        emit_noop_stub(...)
    }
}
```

`is_in_azul_mini_export_set` reads the eventloop's exports —
either by parsing the produced `azul-mini.wasm` or by maintaining a
hand-curated allowlist from api.json's `classify_api_functions`
classification.

### 4. `/az/listener.js`

Rewrite from M4's "fetch+instantiate+dispatch via Proxy" shape.
M8's listener does much more.

**Initialization (on `DOMContentLoaded`):**

```js
async function azBootstrap() {
    // 1. Create shared memory + indirect-call table.
    const memory = new WebAssembly.Memory({ initial: 16, maximum: 256, shared: true });
    const cbTable = new WebAssembly.Table({ initial: 64, element: 'funcref' });

    // 2. Build the bootstrap import object.
    const imports = {
        env: {
            memory,
            __indirect_function_table: cbTable,
            __az_log_debug:  (ptr, len) => console.debug(readString(memory, ptr, len)),
            __az_log_error:  (ptr, len) => console.error(readString(memory, ptr, len)),
        },
    };

    // 3. Instantiate the eventloop.
    const miniMod = await fetchAndInstantiate('/az/mini.<hash>.wasm', imports);
    azMini = miniMod.exports;

    // 4. Instantiate layout callback + register at table[0].
    const layoutMod = await fetchAndInstantiate(
        document.querySelector('link[rel=preload][href*="/az/layout/"]').href,
        imports
    );
    cbTable.set(0, layoutMod.exports.callback);

    // 5. For every [data-az-wasm] node in the rendered DOM:
    //    fetch + instantiate + register at table[node_idx].
    for (const el of document.querySelectorAll('[data-az-wasm]')) {
        const url = el.getAttribute('data-az-wasm');
        const cbId = parseInt(el.getAttribute('data-az-cb'), 10);
        const cbMod = await fetchAndInstantiate(url, imports);
        cbTable.set(cbId, cbMod.exports.callback);
        // Tell the eventloop which table index handles this node.
        azMini.az_register_callback(cbId, EVENT_CLICK, cbId);
    }

    // 6. Hydrate initial state.
    const stateJson = document.getElementById('az-state').textContent;
    const stateBytes = new TextEncoder().encode(stateJson);
    const stateBuf = azMini.az_alloc(stateBytes.byteLength);
    new Uint8Array(memory.buffer, stateBuf, stateBytes.byteLength).set(stateBytes);
    azMini.az_init(stateBuf, stateBytes.byteLength);
    azMini.az_free(stateBuf, stateBytes.byteLength);

    // 7. Register native event listeners on the root.
    azRegisterNativeListeners();
}
```

**Native listener registration (the JS event source):**

```js
function azRegisterNativeListeners() {
    const root = document.body;
    // Mouse events — coordinates + button + node_id under cursor.
    root.addEventListener('mousedown',  e => azDispatch(EVENT_MOUSEDOWN, e));
    root.addEventListener('mouseup',    e => azDispatch(EVENT_MOUSEUP, e));
    root.addEventListener('mousemove',  e => azDispatch(EVENT_MOUSEMOVE, e));
    root.addEventListener('click',      e => azDispatch(EVENT_CLICK, e));
    root.addEventListener('dblclick',   e => azDispatch(EVENT_DBLCLICK, e));
    root.addEventListener('wheel',      e => azDispatch(EVENT_WHEEL, e));
    // Keyboard.
    document.addEventListener('keydown', e => azDispatch(EVENT_KEYDOWN, e));
    document.addEventListener('keyup',   e => azDispatch(EVENT_KEYUP, e));
    // Focus.
    root.addEventListener('focusin',    e => azDispatch(EVENT_FOCUSIN, e));
    root.addEventListener('focusout',   e => azDispatch(EVENT_FOCUSOUT, e));
    // Viewport.
    window.addEventListener('resize',   e => azDispatch(EVENT_RESIZE, e));
    window.addEventListener('scroll',   e => azDispatch(EVENT_SCROLL, e));

    // Note: NO `mouseover` / `mouseout` / `hover` listeners — the
    // eventloop's hit-test computes the hover state from mousemove
    // coordinates. The user's :hover EventFilter is checked WASM-side.
}

function azDispatch(eventKind, domEvent) {
    // Find the WASM-side node_idx from event.target.
    const nodeAttr = domEvent.target.id?.match(/^az_(\d+)$/);
    const nodeIdx = nodeAttr ? parseInt(nodeAttr[1], 10) : 0xFFFFFFFF;

    // Marshal event into a fixed byte buffer.
    const buf = azMini.az_alloc(64);
    const view = new DataView(memory.buffer, buf, 64);
    view.setUint32(0,  nodeIdx, true);
    view.setFloat32(4,  domEvent.clientX || 0, true);
    view.setFloat32(8,  domEvent.clientY || 0, true);
    view.setUint32(12, domEvent.button || 0, true);
    view.setUint32(16, domEvent.keyCode || 0, true);
    view.setUint32(20, modifierBits(domEvent), true);
    // ...

    // Call eventloop dispatch.
    const numPatches = azMini.az_dispatch_event(eventKind, buf, 64);
    azMini.az_free(buf, 64);

    // Drain + apply patches.
    if (numPatches > 0) {
        azApplyPendingPatches();
    }
}

function azApplyPendingPatches() {
    const cap = 65536;
    const out = azMini.az_alloc(cap);
    const n = azMini.az_get_patches(out, cap);
    const bytes = new Uint8Array(memory.buffer, out, n);
    parsePatchOps(bytes).forEach(applyOp);
    azMini.az_free(out, cap);
}
```

**Patch operations** (TLV byte stream):

```
struct PatchOp {
    kind: u8,        // 1=SetText, 2=SetAttr, 3=AddChild, 4=RemoveChild, ...
    node_idx: u32,   // the az_N synthetic ID to mutate
    payload_len: u32,
    payload: [u8; payload_len],
}
```

For hello-world: clicking the button → counter increments → layout
re-runs → diff says "text node az_1's content changed from 5 → 6"
→ emits `SetText(node_idx=1, payload="6")`. JS does
`document.getElementById('az_1').textContent = '6'`.

## Discovery & lifecycle on the server side

### What the server lifts at startup

```rust
// In dll/src/web/mod.rs::run_web
fn run_web(...) -> Result<()> {
    // ... (existing setup)

    // M8.1: Build/load azul-mini.wasm.
    //
    // Option A (build-time): cargo build -p azul-web-eventloop
    // --target wasm32-unknown-unknown — load the artifact bytes.
    //
    // Option B (cached at runtime): same, but the dll's build.rs
    // produces the wasm and embeds it via include_bytes!.
    let mini_wasm: &[u8] = include_bytes!(
        concat!(env!("OUT_DIR"), "/azul_web_eventloop.wasm")
    );

    // M8.2: Lift the root layout callback.
    let layout_fn_ptr = root_window.window_state.layout_callback.cb as usize;
    let layout_sym = resolve_fn_ptr(layout_fn_ptr);
    let layout_wasm = transpiler.lift_function(
        &layout_sym.name,
        layout_sym.addr,
        layout_sym.size,
    )?;

    // M8.3: Same for every route's layout callback if config.routes is non-empty.

    // M8.4: Existing M0-M7 callback discovery + lift (per-route).
    let cb_wasms = discover_and_transpile_callbacks(&discovered_per_route);

    // M8.5: Serialize initial app_data to JSON via the user's
    // `MyDataModel_toJson` route (the existing REFLECT_JSON
    // convention). For hello-world: {"counter": 5}.
    let initial_state_json = serialize_app_data_to_json(&app_data)?;

    // M8.6: Pass everything to the HTML emitter.
    // html_render now embeds the layout-wasm preload + the state JSON
    // script tag.

    // ... (existing serve)
}
```

### What the layout-callback lift needs

The layout callback returns `AzDom` (or `AzStyledDom` if the user
calls `Dom_style`). For hello-world it returns `AzDom`. AzDom is a
struct with a `Vec<NodeData>` + hierarchy info — larger than 16B.

The M7 wrapper currently can't handle this (it assumes `AzUpdate`-shaped
i32 returns). M8.0 (BEFORE M8.1) needs to extend the wrapper to
either:
- (a) Support hidden-pointer returns via X8.
- (b) Wrap the return in a serialization step: lifted layout
  returns bytes, eventloop deserializes.

(b) is recommended because it sidesteps the per-callback-type
return-shape complexity. The wrapper synthesis system from M7-arch
extends naturally: for any callback whose return is `>16B aggregate`,
emit a serialize step (calling a generated `__az_serialize_<T>` from
azul-mini.wasm).

This depends on EVERY framework type having a serializer. azul
already has `to_json`-style derives via api.json's `derive` field;
extending to a compact binary format is similar.

## Phased plan

### M8.0 — Pre-decisions (~1h, user-driven)

Decisions that gate the implementation:
1. Eventloop = dedicated Rust crate, NOT remill-lifted? (Recommend yes.)
2. Layout callback return: hidden-pointer or serialized bytes? (Recommend bytes.)
3. RefAny initial state: JSON via REFLECT_JSON, or binary postcard? (Recommend JSON — REFLECT_JSON already exists.)
4. Cross-module allocator: shared `az_alloc` in azul-mini, or per-module? (Recommend shared.)
5. Patch encoding: TLV bytes (proposed) or JSON? (Recommend TLV — smaller, faster.)
6. Event marshalling format: fixed 64-byte buffer (proposed) or per-event-kind variable? (Recommend fixed for simplicity.)

### M8.1 — `azul-web-eventloop` crate skeleton (~4-6h)

Create `web-eventloop/` crate:
- `Cargo.toml` targeting `wasm32-unknown-unknown` + `lib.crate-type = ["cdylib"]`.
- Pull in `azul-core` + `azul-layout` with minimum-viable feature flags (need to add `wasm` feature that prunes desktop deps).
- Implement `az_alloc` / `az_free` via `dlmalloc` or `wee_alloc`.
- Implement `az_init` reading state bytes (deserialize JSON via existing path).
- Stub `az_dispatch_event` returning 0.
- Stub `az_get_patches` returning 0.
- Build + verify: `wasm-validate` clean, size <100KB.

### M8.2 — Server-side layout-cb lift (~2-3h)

Extend `run_web` to:
- Read `root_window.window_state.layout_callback` fn-ptr.
- Lift via `RemillTranspiler::lift_function` (handles the new
  serialized-return shape).
- Serve at `/az/layout/<hash>.wasm`.
- Emit preload hint in HTML head.

Also extend M7-arch's `signature_for_callback_kind` with
"LayoutCallback" → serialized-bytes return shape.

### M8.3 — Hit-test + EventFilter in eventloop (~6-10h)

Port `azul-layout`'s `dispatch_events_propagated` + hit-test logic.
This is the core of M8 — needs careful translation:
- Hit-test: given `(x, y)` + a `StyledDom`, return list of node IDs
  the point is over.
- EventFilter match: given a list of nodes + an event type, return
  list of `(node_id, callback_id)` pairs to invoke.
- Dispatch: for each pair, look up the callback in the table,
  invoke via `call_indirect` with the user's RefAny + a synthesized
  `CallbackInfo`.

### M8.4 — Reconciliation + patch emission (~4-6h)

After RefreshDom, eventloop calls layout callback, gets new DOM,
diffs against `current_dom` via `reconcile_dom`, emits patch ops
into `pending_patches`. JS pulls patches via `az_get_patches`.

### M8.5 — `listener.js` rewrite (~3-4h)

Replace M4's loader with the bootstrap shape above. Wire native
event listeners on root, dispatch via fixed buffer, apply patches.

### M8.6 — Initial state hydration (~2-3h)

Server-side: serialize `app_data` to JSON. Add
`fn serialize_app_data_to_json(&RefAny) -> Result<String>` using
the existing REFLECT_JSON convention.
Embed in HTML head: `<script id="az-state" type="application/json">{...}</script>`.

### M8.7 — End-to-end hello-world (~2-4h)

Click button three times in a real browser. Counter 5→6→7→8.
Zero POST `/az/exec/` in server access log.

### M8.8 — Framework-call routing in per-callback intercept (~3-5h)

Update M7's `emit_helper_ir` branch-stubs section: when a resolved
symbol matches an azul-mini export, emit an import instead of a
noop stub. Eventloop's table-based dispatch handles the call.

Without M8.8 the counter wouldn't actually increment (the framework
calls in `on_click` are still noop-stubbed); WITH M8.8 the lifted
body calls into azul-mini for the framework helpers and the user's
counter mutation actually happens.

**This is the milestone that makes the demo work.**

### M8.9 — Cleanup + production-ish polish (~2-4h)

- Wasm size budget (azul-mini target <500KB).
- Error handling on instantiation failures.
- Browser compat: feature-detect `shared: true` memory.
- Doc + memory note about the integration.

### Estimated total: ~30-50h focused work

Front-loaded risk in M8.3 (hit-test + dispatch port) and M8.8
(framework-call routing). Both have clear paths but are real
engineering.

## What this architecture deliberately doesn't do (yet)

- **Multi-window.** One tab = one window. SPA navigation reloads.
- **Web Workers / ThreadCallback.** Single-threaded WASM only.
- **WebGL / WebGPU.** Browser handles rendering; no GPU code in the wasm.
- **OPFS / sandboxed file I/O.** AzFile_* falls through to noop or
  server fallback.
- **Async I/O.** AzHttp_fetch → JS shim that calls browser `fetch()`.
  Deferred to M8.10.
- **DevTools.** No source-map support for the lifted callbacks.
- **Hot reload.** Server restart = full client reload.
- **Recovery on broken callbacks.** If a callback throws inside WASM,
  the eventloop traps; we don't gracefully recover. User direction:
  "complex callbacks broken-for-now is acceptable."

## Risks worth re-flagging

1. **`azul-core` + `azul-layout` may not build for wasm32 cleanly.**
   Today they target `cfg(not(target_arch = "wasm32"))` paths
   throughout for things like file I/O, timers, threads. Pruning
   these via feature flags is real work — could be a multi-day
   refactor in itself if many dependencies leak through.

2. **dlmalloc vs wee_alloc decision.** wee_alloc is unmaintained;
   dlmalloc is heavier but actively maintained. Either works for
   the eventloop. Decision in M8.0.

3. **`call_indirect` type-matching.** Every callback the table
   holds must have a `funcref` type matching what the call site
   declares. Eventloop's dispatch needs distinct call sites per
   callback signature (Callback vs CheckBoxOnToggleCallback vs
   LayoutCallback). 5-10 signature variants total per `host_invoker_kinds`.

4. **Memory growth.** Each lifted callback alloca's 4096 bytes of
   stack scratch. If `azul-mini.wasm` and many per-callback wasms all
   share one linear memory, total wasm pages need to grow. Start at
   16 pages (1MB), allow grow to 256 pages (16MB).

5. **The SROA gap (memory/m6_sroa_gap_2026_05_18.md) still
   applies** — per-callback overhead stays ~460B-1KB. For 50
   callbacks: ~50KB of wrapper code. Adds to the asset bundle size.

## Specifically what a new agent should do first

1. Read this doc top-to-bottom.
2. Read the prerequisite memory notes (`m1_phase0_findings`,
   `m6_sroa_gap`) for the gotchas.
3. Read `scripts/HANDOFF_2026_05_19.md` for the M0-M7 end-state.
4. Discuss the M8.0 pre-decisions with the user.
5. Start with M8.1 (eventloop crate skeleton) — it's the lowest-risk
   high-leverage piece. Even before hit-test + dispatch are wired,
   having `cargo build -p azul-web-eventloop --target wasm32`
   succeed is meaningful progress.
6. After M8.1: M8.6 (initial state hydration) is the easiest
   end-to-end-visible win. Then M8.2 (layout-cb lift). Then the
   hard parts (M8.3 + M8.8).

## Build/run commands the new agent will need

```bash
# Build the eventloop (after M8.1).
cargo build -p azul-web-eventloop --target wasm32-unknown-unknown --release

# Build libazul with the web stack.
cargo build -p azul-dll --release --features "build-dll web web-transpiler" --no-default-features

# Build the C hello-world.
cd examples/c && cc -o hello-world.bin hello-world.c -lazul -L../../target/release -I../../dll

# Run + manually click in browser.
DYLD_LIBRARY_PATH=../../target/release AZ_BACKEND=web://127.0.0.1:8080 ./hello-world.bin
# Open http://127.0.0.1:8080/

# Verify zero POST round-trips after page load.
grep -c "POST /az/exec/" <server-log>  # expected: 0 after M8 done
```

End of M8 architecture sketch. ~30-50h of focused work to ship the
hello-world in a browser, in 9 small milestones with clear
checkpoints.
