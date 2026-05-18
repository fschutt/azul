# Azul web backend — M9 close-out report

**Date:** 2026-05-18
**Branch:** `layout-debug-clean`
**Pipeline state:** WASM-resident DOM architecture landed. Layout cb

> **📋 Architectural retrospective**: see
> [M9_REVIEW_AND_OPTION_A.md](M9_REVIEW_AND_OPTION_A.md) for the
> post-M9 review that identifies the synthetic-address lift fix
> as the single 50-line change that retires most of the M9
> scaffolding. The "what's deferred" list at the bottom of this
> status report is reframed there as "what gets cheap under the
> fix" — much shorter list.


runs in wasm (minimal cb verified end-to-end); dispatch + hit-test +
patch emission now happen wasm-side for the on_click flow.

## TL;DR

All six phases of the M9 plan shipped:

| phase | what landed | commit |
|------:|-------------|--------|
| 1 | Layout-cb wrapper with X8 hidden return (`Pcs::HiddenPtrReturn`) | `7a9250fde` |
| 2 | `AzStartup_buildLayoutInfo` + JS layout-cb instantiation | `2a25f4eff` |
| 3a | `AzStartup_initLayoutCache` + wasm-resident Dom blob + post-link stack relocator | `4f421454f` |
| 3b | User-binary data-section mirror scaffolding | `4472850e7` |
| 4 | `AzStartup_hitTest` stub + cb-node registration | `27fffd51d` |
| 5 | `AzStartup_buildCounterPatch` wasm-side TLV encoder | `4c4e94ff8` |
| 6 | loader.js minimization (kill direct cb invoke + regex hit-test) | `d79d58d9f` |

## What works end-to-end

  - **on_click counter e2e** (`/tmp/e2e.js`, counter 5→12 in 7
    clicks) passes in BOTH `web-transpiler` subprocess mode AND
    `AZ_NATIVE_REMILL=1` in-process mode.
  - **Minimal layout probe** (`/tmp/layout-probe.js` against
    `examples/c/hello-world-minimal.bin`) reports:
      `initLayoutCache rc=0`, `current_dom_ptr=...` (non-zero),
      `current_dom[0..64]: "2 0 0 0 0 0 8 0 ..."` (real Dom bytes).
    No `resolve_callback` round-trips — pure lifted wasm.
  - **Patch encoding probe** (`/tmp/m95-patch-probe.js`) covers
    counter values `0`, `12`, `4294967295` — all decode to
    correct `(kind=1 SetText, node_idx, payload_len, decimal text)`.
  - **Stack relocator** (M9-3a) hands out non-overlapping stack
    regions to mini (slot 0 → SP 192 KiB), per-cb wasms
    (slot 1 → 320 KiB), and per-layout (slot 2 → 448 KiB), so
    cross-module calls don't corrupt each other's State buffers.

## Architecture summary

```
SERVER (per route, at startup):
  - Pre-render HTML for instant first paint.
  - Lift mini / cb / layout wasms via remill+LLVM+LLD (M8.9).
  - Each non-mini wasm's stack is post-link relocated to a unique
    192 KiB+ offset (M9-3a) to dodge mini's stack.
  - mini.wasm exports:
      AzStartup_alloc / _free / _init / _hydrate
        (M8.x baseline)
      AzStartup_registerStateDeserializer
        (M8.x baseline)
      AzStartup_buildLayoutInfo
        (M9-2: 512-byte zero blob for hello-world; real stubs
         deferred to M9-3b)
      AzStartup_setLayoutCbTableIdx / _setRefAny / _initLayoutCache /
      _getCurrentDomPtr / _getLastLayoutStatus
        (M9-3a: layout cb dispatch + Dom blob storage)
      AzStartup_registerCbNode / _hitTest
        (M9-4: wasm-side hit-test, currently stub-returns
         last registered cb node)
      AzStartup_buildCounterPatch
        (M9-5: wasm-side SetText TLV encoder)
      AzStartup_setModelPtr / _setDisplayNode
        (M9-6: wasm-side dispatch state)
      AzStartup_dispatchEvent
        (M9-6: now uses hydrated refany + emits patches
         via _buildCounterPatch on RefreshDom)

JS (loader.js):
  - Instantiates mini, cb, layout wasms with shared memory + table.
  - Calls hydrate + the new setters: setLayoutCbTableIdx,
    setRefAny, setModelPtr, setDisplayNode, registerCbNode.
  - Calls AzStartup_initLayoutCache once at bootstrap to populate
    state.current_dom_ptr.
  - On click: encodes (kind=CLICK, node_idx=SENTINEL, x_bits, y_bits,
    mods) into a 256-byte event buffer, calls dispatchEvent,
    decodes returned TLV patches via the existing azApplyPatches.

WASM (AzStartup_dispatchEvent):
  1. Hit-test if node_idx is SENTINEL (M9-4).
  2. Resolve cb fn-addr → table_idx (existing JS-imported
     __az_resolve_callback bridge — kept as the ONLY JS↔WASM
     dispatch round-trip per the WASM-resident DOM vision).
  3. Invoke cb via __az_call_indirect with hydrated refany_ptr.
  4. On Update::RefreshDom, read counter from state.model_ptr,
     encode SetText TLV via AzStartup_buildCounterPatch, return
     (buffer_ptr, len) for JS to apply.
```

## What's deferred (follow-up work)

These two pieces were carved off as separate tasks to keep M9
shippable:

### 1. M9-3b: full LayoutWindow + StyledDom embed

`AzStartup_initLayoutCache` currently stores the raw `AzDom` blob
returned by the cb. To support real bbox-based hit-test (currently
a stub returning `last_registered_cb_node_idx`) the next step is:

  - Run `StyledDom::create_from_dom(dom)` on the cb's return.
  - Construct a `LayoutWindow` (Solver3 cache, FontManager,
    ImageCache, GestureManager, …) in wasm.
  - Run `lw.do_the_layout(&styled_dom, viewport)` to populate
    real bboxes per node.
  - Walk the StyledDom to populate `cb_fn_cache` for
    AzStartup_hitTest + dispatchEvent.

This is substantial transitive-lift surface (LayoutWindow's deps
are ~hundreds of azul-layout fns). Tracked separately because the
M9 architectural scaffold above ships without it, and adding it
is mechanical once the architecture stabilises.

### 2. User-binary data section mirror

The `AzStartup_dispatchEvent` → layout-cb path traps on the FULL
`examples/c/hello-world.c` (vs the working `hello-world-minimal.c`)
because the lifted layout cb code does `adrp + ldr/add` to user
binary addresses — `__cstring` (format strings), `__const`, etc.
On wasm32 those native 64-bit addresses get truncated to 32 bits,
landing inside wasm linear memory at offsets that may or may not
be initialised.

M9-3b shipped the SCAFFOLDING: `SymbolTable::enumerate_low32_data_for_wasm`
+ `patch_wasm_add_data_segments` mirror binary data sections into
wasm Data segments at the truncated offsets — limited to 8 MiB so
the multi-MB libazul __const doesn't blow up mini.wasm. For
binary slides that fit under 8 MiB this works; ASLR runs with
larger slides see 0 segments injected and the cb traps.

To fully close the gap:

  - Detect each loaded image's slide at server startup.
  - Bump wasm `initial_memory` (currently 16 MiB) to cover the
    union of (slide + segment_size) for every relevant image.
  - Relocate the bump heap base (currently hardcoded 1 MiB in
    `emit_helper_ir`) above the highest mirrored offset.
  - Widen the 8 MiB limit accordingly.

The current limitation only affects callbacks with extensive
user-binary const-string or static-data usage (snprintf format
strings, string literals, etc.). Callbacks that use libazul APIs
exclusively (e.g. `hello-world-minimal.c`) work end-to-end today.

## What NOT to break

  - on_click counter e2e (`/tmp/e2e.js`, 5→12 in 7 clicks) in
    BOTH subprocess and AZ_NATIVE_REMILL=1 modes — verified at
    each of M9-1 through M9-6.
  - The M8.9-1 SimpleTraceManager fix (in-process lift trace
    divergence) — untouched.
  - The M8.9-3b export_as-as-stem fix (duplicate-symbol error in
    transitive batched lift) — untouched.
  - CSS cascade output in the `<style>` block — untouched.
  - `m8.9-victory` tag (commit `9780a92b3`) still recoverable via
    `git reset --hard m8.9-victory`.

## Files touched

```
dll/src/web/eventloop.rs           +293  / -12   (10 new exports)
dll/src/web/loader_js.rs           +131  / -100  (M9-2/3a/4/6 wiring)
dll/src/web/mod.rs                 +14  / -0     (EVENTLOOP_SYMBOLS)
dll/src/web/symbol_table.rs        +101  / -0    (M9-3b extractor)
dll/src/web/transpiler_remill.rs   +395  / -19   (M9-1 + relocator
                                                  + data injector +
                                                  per-export sigs)
```

## Verification scripts

  - `/tmp/e2e.js` (M8.9 baseline) — counter 5→12 over 7 clicks.
  - `/tmp/layout-probe.js` (M9-2 / 3a) — instantiate layout wasm,
    drive initLayoutCache, verify Dom blob populated.
  - `/tmp/m95-patch-probe.js` (M9-5) — exercise the SetText TLV
    encoder on three counter values.

## Commands

```bash
# Build with the M9 in-process pipeline:
cargo build -p azul-dll --release \
  --features "build-dll web web-transpiler web-transpiler-static" \
  --no-default-features

# Run the demo:
cd examples/c
cc -fno-stack-protector -o hello-world.bin hello-world.c \
   -lazul -L../../target/release -I../../dll
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
   AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin

# In another terminal:
node /tmp/e2e.js               # → counter 5 → 12 OK
open http://127.0.0.1:8800/    # browser-side click path goes
                               # through new wasm dispatch
```

## Recent session commits (2026-05-18)

```
d79d58d9f web: M9-6 loader.js minimization — wasm-side dispatch + patch apply
4c4e94ff8 web: M9-5 wasm-side SetText TLV patch encoding
27fffd51d web: M9-4 WASM-side hit-test stub replaces JS regex (preferred path)
4472850e7 web: M9-3b user-binary data-section mirror scaffolding
4f421454f web: M9-3a WASM-resident layout dispatch via AzStartup_initLayoutCache
2a25f4eff web: M9-2 LayoutCallbackInfo builder + JS layout-cb instantiation
7a9250fde web: M9-1 layout-cb wrapper with X8 hidden return
89654a82f docs: M9 plan — lock in caller-allocated dest buffer + tight schedule
```
