# Web Backend Roadmap — `AZ_BACKEND=web://` for `examples/c/hello-world.c`

**Drafted:** 2026-05-18
**Branch:** `layout-debug-clean`
**Last commit:** `4816e8e62` (web.md architecture doc).
**Target:** Click "Increase counter" in a browser tab pointed at the running C hello-world and have the page update.

## The progression

Two milestones, each with concrete sub-steps. **Intermediate goal** is "non-working callbacks load in the browser" — the lift pipeline produces real per-callback WASM and the browser fetches + instantiates it, but the callbacks do nothing functional. The existing `POST /az/exec/{node_id}` server-side dispatch is still what actually updates the DOM. **End goal** is real client-side execution: the WASM mutates state, calls back into `azul-mini.wasm`'s framework primitives, returns an `Update`, and the browser patches the DOM without a server round-trip.

The intermediate goal is the right de-risking layer because it isolates **runtime wiring** (callback discovery, content-hashing, route serving, browser-side instantiate + dispatch) from **lift-pipeline correctness** (the four IR passes). Each can fail independently; sequencing them this way means each failure has at most one possible source.

---

## M0 — Unblock the in-dll smoke test (prerequisite)

`dll/src/web/server.rs` has pre-existing compile errors when `web + cabi_internal` are both enabled — private imports of `azul_core::window::{DomId, DomNodeId, OptionLogicalPosition, ...}` and `azul_layout::image::encode_png` after upstream refactors. 161 errors total; none from any WB work.

**Action:** triage server.rs imports. Likely fixes: `azul_core::dom::{DomId, DomNodeId}` (the `pub use` re-export from `azul_core::window` was dropped); `azul_layout::image::png::encode` or similar relocation. ~30 min triage + ~30 min fix.

**Validation:** `cargo build -p azul-dll --features "web cabi_internal" --no-default-features` exits 0. Then re-port the smoke-test template from `memory/wb1_progress_2026_05_17.md` and verify a trivial `extern "C" fn add(i32, i32) -> i32` lifts to valid WASM via `RemillTranspiler::lift_function`.

**Risk:** Low. Pure import-path triage; existing tests cover the underlying behavior.

---

## M1 — Verify the existing Phase 0 path works for hello-world.c end-to-end

Before adding any new wiring, confirm what already works. The C hello-world should be runnable as-is against the existing Phase D/E web backend with server-side dispatch.

**Steps:**
1. Build: `cd examples/c && make hello-world.bin` (or however the C build is invoked — verify the current makefile/script).
2. Run: `AZ_BACKEND=web://127.0.0.1:8080 ./hello-world.bin`.
3. Open `http://127.0.0.1:8080/` in a browser.
4. Verify: initial counter (5) renders. Click "Increase counter". Page reloads via `document.write` and counter becomes 6.

**What this de-risks:**
- The `AzBackend::Web(cfg)` dispatch path is reached from C-built binaries (not just Rust ones — should be transparent, but `App::run`'s env-var check happens inside libazul so it should Just Work).
- `render_initial_page` produces correct HTML for the C-built `layout` callback's `Dom`.
- `loader_js`'s `data-az-cb` wiring fires on click.
- The server's `POST /az/exec/{node_id}` rerunning the layout callback observes the C-side `on_click` mutation.
- The full-document replacement (`document.open() + document.write()`) actually updates the visible page.

**Expected output:** screencap or screen recording of the click increment happening (via server round-trip). One PR comment proving "Phase 0 works for the C entry point."

**Risk:** Medium. The C-side `AzApp_run` dispatch may surface gaps that nothing exercises today (e.g. the C ABI struct layouts vs. what `run_web` expects). If `run_web` blows up, root-cause + fix before proceeding.

**If broken:** likely culprits are `WindowCreateOptions` C-ABI vs. Rust-ABI alignment, or the layout callback's `RefAny` not surviving the dispatch boundary. Both are existing-state issues, not new work.

---

## M2 — Callback discovery via `dladdr`

Today `discover_and_transpile_callbacks` returns `Vec::new()` so the HTML emits no `<link rel="preload">` hints and the server's `/az/cb/*` route always 404s.

The first real change: walk the rendered DOM (each route, after `render_initial_page` runs) and collect every callback function pointer that landed on a node. For each pointer, call `dladdr(fn_addr)` to resolve `(symbol_name, fn_addr, fn_size)`. The size from `dladdr` is approximate; for the lift we'll over-read into a generous window (128–256 bytes) and let the disassembler stop at `ret`.

**Steps:**
1. Walk `StyledDom`'s node arena, collect every `CallbackData` (the typed wrapper around a callback fn pointer).
2. Deduplicate by `(fn_addr, fn_size)` — multiple nodes can share the same callback.
3. For each unique pointer, `dladdr` → `(name, addr, size)`.
4. Store the result in `WebServerState.discovered_callbacks` (new field).
5. Update `cb_gen::discover_and_transpile_callbacks` to return the list (not the WASM bytes yet — just the metadata).

**What this de-risks:**
- `dladdr` works at runtime on the macOS/Linux host (the C-built binary has full symbol info; the Rust-built one needs `--export-dynamic` or `RUSTFLAGS=-C link-arg=-rdynamic` to surface symbols outside the binary's own exports).
- The DOM walk reaches the callback function pointers without crossing any FFI boundary that strips them.

**Expected output:** debug-log line per discovered callback: `[az/web] discovered cb: on_click @ 0x100002a40 (size=24)`. Hello-world should surface exactly one (`on_click` from the C source).

**Risk:** Medium. The Rust binary may need `-rdynamic` to make `on_click` visible to `dladdr`. The C binary should work out of the box because C symbols live in `.dynsym` by default. If `dladdr` returns `None` for the callback address, fall back to `backtrace_symbols` or surface the issue.

---

## M3 — Hand-rolled no-op WASM per callback (intermediate-goal half)

Skip the lift pipeline entirely for the first browser-side milestone. For each discovered callback, emit a fixed ~50-byte WASM module that exports one function matching the callback's ABI signature and returns the integer encoding of `Update::DoNothing` (0).

This is the smallest thing that puts a *real* per-callback WASM module on the wire. It validates the HTML emission, the content-hashing, the `/az/cb/*` route, the browser-side fetch, and the click-handler dispatch — all without touching remill.

**Steps:**
1. Add a `cb_gen::emit_noop_wasm(name) -> Vec<u8>` helper that produces:
   ```wat
   (module
     (func (export "on_click") (param i32 i32) (result i32)
       i32.const 0))
   ```
   Hand-assembled WASM, ~37 bytes per callback. (Or use `wasm-encoder`, which is already a transitive dep.)
2. Wire it into `cb_gen::discover_and_transpile_callbacks` so each discovered callback gets a `CallbackWasm { name, bytes, content_hash }` entry.
3. `html_render` already has scaffolding for `<link rel="preload" href="/az/cb/{name}.{hash}.wasm">` — verify it actually emits when `cb_wasms` is non-empty (today empty `Vec` means nothing emitted).
4. Server's `GET /az/cb/{name}.{hash}.wasm` route already exists; today it 404s because `state.cb_wasms` is empty. With M3, it serves real bytes.

**What this de-risks:**
- HTML correctly emits the preload hints, with content-hashed URLs.
- The server's `/az/cb/*` route serves the right bytes for the right URL.
- `Cache-Control: immutable, max-age=1y` headers are correct (already implemented for img/font; should mirror).

**Expected output:** `curl http://localhost:8080/az/cb/on_click.<hash>.wasm | wasm-objdump -h -` shows a single export. Browser DevTools Network tab shows the `<link rel="preload">` fetching the module before any click.

**Risk:** Low. Hand-rolled WASM is mechanical; the routes are scaffolded.

---

## M4 — Browser-side fetch, instantiate, dispatch (intermediate-goal complete)

Update `loader_js` so that when the browser sees a `data-az-cb` element:
1. Fetch the matching WASM module (`/az/cb/{name}.{hash}.wasm`).
2. Instantiate via `WebAssembly.instantiateStreaming`.
3. On click, call the exported function with `(0, 0)` (placeholder args — the real arg-marshalling waits for M7+).
4. Ignore the return value.
5. **Fall through to the existing `POST /az/exec/{cbId}` flow.** The server still re-runs the layout callback and returns the updated HTML; the browser still does `document.write` to replace the page.

The point is that the WASM call is functionally a no-op, but it *runs*. Console-log the return value so DevTools shows the dispatch is wired.

**Steps:**
1. In `loader_js::generate_loader_js`, accept the `cb_wasms` list, generate a JS table mapping `cbId → moduleUrl + exportName`.
2. On `DOMContentLoaded`, fetch + instantiate each module in parallel. Cache the instances by `cbId`.
3. In the click handler, before the existing `fetch('/az/exec/' + cbId, ...)`, call `wasmInstance.exports[exportName](0, 0)`. Log the return to `console.log`.
4. Continue with the existing POST + `document.write` flow.

**What this de-risks:**
- Per-callback WASM modules instantiate successfully against a minimal import object (none needed for hand-rolled).
- The browser's CSP / CORS doesn't reject the WASM fetch (same-origin, so should be fine; verify).
- The click handler's WASM call doesn't throw.
- The fall-through to POST still works (`document.write` page replacement).

**Expected output:** click "Increase counter" in DevTools. Network shows: `GET /az/cb/on_click.<hash>.wasm` (one-time, from preload), then `POST /az/exec/<cbId>`. Console shows: `[az] cb on_click returned 0`. Page updates as before (server round-trip).

**This is the intermediate-goal end state.** Callbacks are compiled to WASM (well, hand-rolled), load in the browser, are invoked on click, do nothing functional. Server fallback handles the actual DOM update.

**Risk:** Medium. `WebAssembly.instantiateStreaming` requires correct `Content-Type: application/wasm` (verify the server's `Content-Type` header). The click handler ordering matters: WASM call must complete before the POST, or the POST's `document.write` will tear down the WASM instance mid-call (since `document.write` invalidates the global scope).

---

## M5 — Swap the no-op WASM for `RemillTranspiler::lift_function` output

Now the lift pipeline actually runs. Replace `emit_noop_wasm` with a call to `RemillTranspiler::lift_function(name, addr, size)`. The output is the bloated remill-lifted WASM — large (probably 50–200 KB per callback because the `%struct.State` register-file is never optimised away), references `__remill_*` opaque imports, but otherwise valid.

To make it load in the browser without crashing:
1. Provide JS-side stubs for the `__remill_*` imports. All as no-ops:
   ```js
   const remillStubs = {
     __remill_function_return: () => 0,
     __remill_function_call: () => 0,
     __remill_read_memory_8: () => 0,
     __remill_read_memory_16: () => 0,
     __remill_read_memory_32: () => 0,
     __remill_read_memory_64: () => 0n,
     __remill_write_memory_8: () => {},
     // ...etc
     __remill_missing_block: () => 0,
     __remill_error: () => 0,
   };
   ```
2. Pass them as the import object to `instantiateStreaming`.

The callback's State struct lives in WASM linear memory (the lift allocates it as a stack alloca inside the lifted function). Reads through GEPs to State fields are real `i32.load`s — they work. The opaque `__remill_*` calls are noop-stubbed. The function returns whatever the noop stubs let it compute. For a trivial leaf it'll likely return 0 (DoNothing).

**Steps:**
1. Replace `cb_gen::emit_noop_wasm` with `transpiler.lift_function(...)`.
2. Wire `loader_js` to declare the `__remill_*` import set.
3. Verify the lifted WASM loads (no instantiation error) and the call returns without trapping.

**What this de-risks:**
- The lift toolchain produces something the browser will load (correct WASM magic, valid module sections, no malformed imports).
- The default `--allow-undefined` behavior of `wasm-ld` correctly emits the `__remill_*` symbols as imports rather than failing to link.
- Browser's wasm engine handles remill's State-struct allocas without OOM (each callback gets its own ~500-byte stack frame for the State).

**Expected output:** same browser-visible behavior as M4. WASM sizes jump from ~37 bytes (hand-rolled) to 50–200 KB per callback (lifted but unoptimised). Server fallback still drives the actual DOM update.

**Risk:** Medium-high. This is the first place the lift toolchain meets the browser. Possible failures: the lifted module has wasm features the browser doesn't accept (`bulk-memory`, etc. — wasm-ld's defaults are conservative but check), or the linked module exports the lifted function under the wrong name (`sub_100000000` vs. expected `on_click`). The latter is fixable in the lift pipeline by setting `--export-symbol` correctly per callback.

**This is the M5 finish line:** the intermediate-goal sentence "non-working callbacks compiled to WASM and loaded in the browser" is literally true. Each callback runs through the full remill pipeline. The output is functionally inert but structurally real. Server-side dispatch handles correctness.

---

## M6 — Intrinsic lowering + signature rewrite passes (WB1.2)

Now we start making the lift produce *useful* WASM. First pass: give bodies to the `__remill_*` intrinsics so opt can do its work.

**Steps:**
1. Add `dll/src/web/lift_passes.rs` (or a sibling crate if it grows). Use `llvm-sys` for in-process IR manipulation, or stage as a textual rewrite (lower-effort but uglier; pick based on M5's surfaced complexity).
2. Implement intrinsic lowering:
   - `__remill_function_return(state, pc, memory)` → load return register from State, return as the source ABI's return type.
   - `__remill_read_memory_N(memory, addr)` → `load iN, ptr %addr`.
   - `__remill_write_memory_N(memory, addr, val)` → `store iN %val, ptr %addr`.
3. Implement signature rewrite: emit a wrapper `define i32 @on_click(i32 %data_ptr, i32 %event_ptr)` that allocates State on the stack, writes args into the host-ABI argument registers, calls the original `sub_<addr>`, reads the return register out, returns it as `i32`.
4. Wire `RemillTranspiler::lift_function` to run these passes after `remill-lift-17` and before `llc`.
5. Also run `opt -O2` after the passes to SROA the State struct. Output WASM size should collapse to ~hundreds of bytes for a leaf, KB for non-trivial.

**What this de-risks:**
- The State-struct evaporation insight (the load-bearing architectural claim).
- The signature-rewrite pass correctly extracts args from State per host ABI (aarch64 PCS, System V AMD64).
- Combined output is small enough to ship in `<link rel="preload">` without blowing the cache budget.

**Expected output:** Per-callback WASM sizes drop from 50–200 KB (M5) to <2 KB for the hello-world `on_click` (which is a leaf: read counter, increment, write back, return RefreshDom). `wasm-objdump -d` shows clean `i32.add` / `i32.store` ops instead of the State-struct shuffle.

**Risk:** High. This is the first piece of real LLVM-pass engineering. Likely surprises: remill's State struct layout per-arch (especially the SIMD section), per-arch ABI quirks (struct returns by hidden pointer on aarch64, varargs on Sys V), and the wrapper's argument-write order on the State struct.

**If stalled past 2 sessions:** consider running `remill-opt` (remill's own canonical post-processing pass — `tools/remill-opt` in the submodule) as a subprocess before our own passes. May handle most of intrinsic lowering for free; check the output to see if it produces IR clean enough for `opt -O2`.

---

## M7 — Symbol intercept pass (WB1.3) + framework imports

The hello-world `on_click` doesn't call any framework functions (it's pure compute on a struct field), but `layout` definitely does — `AzDom_createBody`, `AzDom_addChild`, `AzCss_empty`, `AzButton_create`, etc. Once we want to lift `layout` (M8), the intercept pass becomes load-bearing.

**Steps:**
1. Walk `__remill_function_call(state, constant_target_pc, memory)` instances in the lifted IR.
2. Resolve `constant_target_pc` → symbol via `dladdr` (the same map already built in M2).
3. For each symbol, consult `classify_api_functions` (refined to the 5-variant enum from `web.md`).
4. Replace each call with a typed extern declaration + call:
   - Args extracted from State per host ABI (same logic as M6's signature-rewrite pass).
   - Result written back to State so subsequent code sees the right value.
5. After `opt -O2`, the State writes-then-reads collapse into direct SSA values.
6. `wasm-ld --import-module=azul-mini` marks the extern declarations as imports satisfied by `azul-mini.wasm` at load time.

**What this de-risks:**
- Callback's framework calls land on `azul-mini.wasm`'s exports with correct ABI marshalling.
- The intercept pass correctly handles all five `FnClass` variants (or at least the two that hello-world hits: `ImportFromAzulMini` for `AzDom_*` and `LiftAsIs` is implicit for the callback's own body).

**Expected output:** `wasm-objdump -j Import on_click.wasm` shows imports like `(import "azul-mini" "AzDom_createBody" (func))`, one per distinct framework call in the callback's source.

**Risk:** High. The intercept pass needs accurate api.json signatures for every `Az*` function. Today api.json has them, but verify completeness (some callbacks may call functions that aren't in api.json's surface).

---

## M8 — Build & link `azul-mini.wasm` (WB1.4/1.5)

Switch `generate_mini_wasm` from the 8-byte stub to the real `RemillTranspiler::lift_and_link_framework(FnClass::Framework_subset)`.

**Steps:**
1. Iterate `classify_api_functions()` output, filter to `FnClass::ImportFromAzulMini` (the renamed `Framework` variant).
2. For each, resolve `(name, addr, size)` via `dladdr` against the running binary's exports.
3. Lift each function through the full pipeline (remill + intrinsic-lower + signature-rewrite + opt + llc).
4. Link the resulting objects into a single `azul-mini.wasm` via `wasm-ld --export-dynamic` so callback modules can import them.
5. Stash the bytes in `WebServerState.mini_wasm` (currently the 8-byte stub).
6. Server already serves it at `/az/mini.{hash}.wasm`; the URL hash gets recomputed automatically.

**Subset for hello-world:** the callback uses ~6 framework functions (`AzDom_createBody`, `AzDom_addChild`, `AzDom_createDiv`, `AzCss_empty`, `AzString_copyFromBytes`, `AzCssProperty_fontSize`). Lift these first to keep `azul-mini.wasm` under 100 KB for the hello-world demo. Expand to the full ~200-function set in a follow-up.

**What this de-risks:**
- The framework's functions lift cleanly (these are mostly C-ABI shims over Rust impls — should be among the friendliest cases for remill).
- The link step correctly resolves cross-function references *within* `azul-mini.wasm` (e.g. `AzDom_createBody` internally calls `AzDom_new`).
- Hand-written shims for browser-native operations (`AzHttp_fetch` → JS `fetch()`) are wired into `azul-mini.wasm`'s body, not into per-callback modules.

**Expected output:** `azul-mini.wasm` size 50–500 KB. Browser DevTools shows `<link rel="preload" href="/az/mini.<hash>.wasm">` fetching before any callback.

**Risk:** High. First time the framework gets lifted. Likely surprises: functions that touch native pointers (`AzGl_*` — should be classified `DomPatcher`, not Framework), functions that take callbacks-as-args (need a separate pattern), functions that call into `std`'s collections (may need feature-flagging or shimming).

---

## M9 — Client-side DOM patching

Today `loader_js` does `document.open() + document.write(serverResponse) + document.close()` after the server POST returns. That tears down everything and rebuilds the whole page — works, but loses scroll position, form state, focus.

Client-side patching:
1. Callback returns `Update::RefreshDom` (1).
2. Browser calls `azul-mini.wasm`'s `run_layout(refany_ptr)` (or similar) to recompute the StyledDom *in the browser*.
3. A patching algorithm (morphdom-style) diffs the new HTML against the live DOM and applies minimal mutations.

This is the piece that turns "POST round-trip per click" into "no server round-trip ever after the initial load."

**Steps:**
1. Export `run_layout` from `azul-mini.wasm` (probably already exists; verify).
2. Add a client-side morphdom-ish patcher (vendor or write — ~200 LOC of vanilla JS).
3. In the click handler, after calling the per-callback WASM:
   - If return is `DoNothing` (0): bail.
   - If `RefreshDom` (1): call `azul-mini.wasm.run_layout(refany)`, get new HTML, diff against `document`, apply.
   - If `RefreshDomAllWindows` etc.: full re-render (rare).
4. Skip the POST entirely on `RefreshDom`.

**What this de-risks:**
- The end-to-end "browser is a dumb render target, azul runs its own cascade in WASM" vision.
- The performance claim (no network on click).

**Expected output:** Counter increment is instant. DevTools Network shows no POST. Page state (scroll, focus, etc.) is preserved.

**Risk:** Medium. The patching algorithm is well-trodden ground (morphdom, idiomorph). The wasm-side `run_layout` may need new exports or wrappers.

---

## End-to-end validation (M10)

When all of M1–M9 land, the validation script:

```bash
# Build the C hello-world against the web-transpiler-enabled libazul.
cd examples/c
make hello-world.bin AZUL_FEATURES="web web-transpiler cabi_internal"

# Run with the web backend.
AZ_BACKEND=web://127.0.0.1:8080 ./hello-world.bin &
HW_PID=$!

# Wait for the server to come up.
until curl -fs http://127.0.0.1:8080/ > /dev/null; do sleep 0.5; done

# Headless browser: load the page, click the button 3 times, read the counter.
node scripts/click_three_times.mjs http://127.0.0.1:8080/ > counter_after.txt

# Expected: counter = 5 (initial) + 3 = 8.
test "$(cat counter_after.txt)" = "8"

# Check that the click did NOT round-trip to the server.
# (server access log should show /az/cb/* fetches but ZERO /az/exec/* POSTs).
grep -c "POST /az/exec/" server.log
# Expected: 0

kill $HW_PID
```

The "zero POSTs" assertion is what proves client-side dispatch is working. Anything > 0 means we're falling back to the server.

---

## Pre-decisions worth surfacing

These are choices that'll matter at some milestone; flagging them now so they don't surprise mid-implementation:

1. **Where lifting runs.** Today: nowhere (stub). M2–M5: at runtime during `discover_and_transpile_callbacks` (~seconds per callback because of `fork+exec` to `remill-lift-17`). For a real app with 50 callbacks, that's a noticeable startup hit. Long-term: build-time lifting (DWARF + symbol table) is preferred; runtime is only needed for closures with no static symbol. **Decision:** ship runtime-first for the hello-world (one callback, fast enough), revisit when an app has >10 callbacks.
2. **Memory sharing model.** WB1.x sketch is "`azul-mini.wasm` exports memory, every callback module imports it." Alternative: each callback has its own memory + an explicit `RefAny_marshall(from_memory_a, to_memory_b)` shim. Sharing is simpler but couples cache invalidation across modules. **Decision:** start with sharing (matches the architectural sketch in web.md); fall back to per-module memory only if a real issue surfaces.
3. **Hash function for content URLs.** Today: FNV-1a 64-bit, hex-encoded. Sufficient for cache-busting; not cryptographic. **No change needed.**
4. **WASM features.** wasm-ld defaults emit modules using bulk-memory, multi-value, etc. Browser support is universal as of 2024+. **No special flags needed.**
5. **Build-system integration.** `web-transpiler` is a Cargo feature; `cabi_internal` is required for the dll's web module. Need to confirm the C build setup (cargo invocation in `examples/c/Makefile` or `build.sh`) passes the right feature combination. **Action:** check during M1.
6. **macOS sandboxing.** `dladdr` on macOS works against the same image's symbols but may need the binary to be built with `--export-dynamic` (Rust default for `cdylib`; check). For `bin` targets, may need explicit `RUSTFLAGS=-C link-arg=-rdynamic`. **Action:** verify during M2.

---

## Estimated effort

| Milestone | Effort | Risk |
|---|---|---|
| M0 — fix server.rs imports | ~1 h | Low |
| M1 — verify Phase 0 for C | ~1–2 h | Medium |
| M2 — dladdr callback discovery | ~3–4 h | Medium |
| M3 — hand-rolled no-op WASM | ~2 h | Low |
| M4 — browser-side fetch + dispatch | ~3 h | Medium |
| M5 — real lift via RemillTranspiler | ~3–4 h | Medium-high |
| **Intermediate goal complete** | **~13–16 h** | |
| M6 — intrinsic-lower + signature rewrite passes | ~6–10 h | High |
| M7 — symbol intercept pass | ~4–6 h | High |
| M8 — build azul-mini.wasm | ~3–5 h | High |
| M9 — client-side DOM patching | ~4–6 h | Medium |
| M10 — E2E validation | ~1–2 h | Low |
| **End goal complete** | **~31–45 h total** | |

Front-loaded risk is in M6/M7 (the IR-pass engineering). M0–M5 is mostly mechanical wiring and validates the runtime story before any heavy IR work.

---

## Loop integration

This plan is suitable for the autonomous-loop cadence: each milestone is independent and commit-safe. Suggested wake-on-completion order: M0 → M1 → M2 → ... → M5 sequentially, then pause for user review before M6 (the heavy IR work). M6+ benefits from interactive debugging on lifted output, so the loop may not be the right vehicle there.

**Open question for the user:** should the autonomous loop pick this up after the current pause, or do you want to drive M0–M5 interactively given the high "wiring across a lot of files" character of those milestones? Either works; the plan is durable.
