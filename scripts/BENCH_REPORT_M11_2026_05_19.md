# Azul web backend — bench report (M11 Sprint 6)

**Date:** 2026-05-19
**Branch:** layout-debug-clean
**Bench binary:** `examples/c/azul-bench-flat.bin`
**Runner:** `scripts/bench-runner.js`

## TL;DR

Dispatch round-trip throughput: **~94,000 ops/sec** on macOS arm64
(Node 25 + WebAssembly). Per-op latency: **10.7 μs**.

The "flat 1000 rows" variant is the only one shipping in this
iteration. The "virtual 10k rows" variant (using `VirtualView`) is
deferred until Sprint 5's full virtualization wiring closes — the
underlying cascade Box::new init gap (memory note
`m11-complex-struct-box-new-lift`) blocks producing real
`StyledDom` internals that the virtualization heuristic would
walk.

## Numbers

```
{
  "bench": "azul-flat",
  "n_ops": 1000,
  "warmup": 50,
  "elapsed_ms": "10.66",
  "per_op_us": "10.661",
  "ops_per_sec": 93798,
  "total_patch_bytes": 15002,
  "avg_patch_bytes": "15.00",
  "final_row_count": 1000
}
```

## What this measures

Each "op" is one full `AzStartup_dispatchEvent(state, kind=CLICK,
evt_ptr, 256, out_len_ptr)` round trip:

  1. JS encodes a 256-byte event buffer.
  2. Wasm hit-tests via `AzStartup_hitTest` against the
     positioned-rects cache (cache hit on rect 0).
  3. Wasm resolves the cb fn-addr → table_idx via
     `__az_resolve_callback`.
  4. Wasm `call_indirect`s the per-cb wasm.
  5. The cb body (lifted from native `on_run`) mutates the
     `BenchModel.row_count` field via the hydrated `AzRefAny`.
  6. cb returns `RefreshDom`.
  7. Wasm reads `state.model_ptr → row_count`, encodes a SetText
     TLV patch, returns its buffer offset + length.

Each iteration runs all 7 steps. No native DOM mutation (Node
harness), so this is the "fast path" — JS-side `azApplyPatches`
adds another few μs in a real browser when it walks `[data-az-cb]`
nodes and updates `textContent` / attributes.

## What this does NOT measure (yet)

  - **Real DOM mutation cost**: the bench runs in Node, not a
    browser. JS-side patch application against actual DOM nodes
    is excluded.
  - **Cascade time**: the cascade lift runs once at hydrate, not
    per-dispatch. (Lazy re-cascade would be a Sprint 7 follow-up.)
  - **Layout solve time**: same — runs once at hydrate. The
    placeholder block layout is O(node_count) so re-solve at
    every dispatch would add ~6 μs per 1000 rows.
  - **N=10k virtual variant**: depends on the deferred
    VirtualView wiring. The architectural advantage (render only
    visible slice, ~30 rows of a 10k logical list) waits for
    that work.

## Comparison narrative (per M11 hard direction #2)

React/Preact/Svelte (krausest benchmarks) at N=1000 typically
publish 8-25 ms per *create-all* op (real DOM creation + layout
+ paint, browser-driven). Azul's 10.66 ms for 1000 dispatches
each emitting one SetText patch is in the same ballpark on the
wasm side, but the comparison isn't apples-to-apples until we
add real DOM-mutation timing.

The Azul story for N=10k changes shape: the planned
`azul-bench-virtual.c` doesn't render 10k flat rows — its
`VirtualView` cb returns only the ~30 visible rows per viewport.
Wire-bytes + paint time stay flat in N because only visible rows
hit the DOM. This is the architectural selling point and the
reason the 10k variant deserves to ship before the next round of
benchmark publishing.

## Reproducing

```bash
cd /Users/fschutt/Development/azul
cargo build -p azul-dll --release \
  --features "build-dll web web-transpiler web-transpiler-static" \
  --no-default-features

cd examples/c
clang azul-bench-flat.c -L../../target/release -lazul -I. -o azul-bench-flat.bin
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./azul-bench-flat.bin &
sleep 25

cd ../..
node scripts/bench-runner.js --ops 1000 --warmup 50
```

## Open follow-ups

1. **Virtual variant** (`azul-bench-virtual.c`): use
   `Dom::virtual_view(provider)` for the 10k logical-row case
   once VirtualView wiring lands.
2. **Browser harness** (`examples/c/bench-harness.html`):
   wrap with `performance.now()` + `MutationObserver` per
   krausest's protocol so the numbers include real paint time.
3. **comparison script** (`scripts/bench-compare.py`): fetch
   krausest's `current.html`, build a markdown table comparing
   create / update / swap / delete times across frameworks.
4. **Real per-row patch streams**: today's SetText patches are
   for the counter display, not per-row text. Wiring the diff
   loop (Sprint 3+) against re-laid-out trees would produce
   real per-row patches.
