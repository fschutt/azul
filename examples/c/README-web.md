# Running Azul C examples on the web (M11 Sprint 8)

Every Azul C example can run as a web server with no C-side code
changes. The framework lifts the example's binary into wasm at
runtime and serves it from a built-in HTTP server.

## Quick start

```bash
# 1. Build libazul with the web backend feature.
cd /Users/fschutt/Development/azul
cargo build -p azul-dll --release \
  --features "build-dll web web-transpiler web-transpiler-static" \
  --no-default-features

# 2. Build any C example (using the normal native flags).
cd examples/c
clang hello-world.c -L../../target/release -lazul -I. -o hello-world.bin

# 3. Run with AZ_BACKEND=web://...
AZ_NATIVE_REMILL=1 \
  DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 \
  ./hello-world.bin

# 4. Open http://127.0.0.1:8800 in a browser.
```

The server lifts your example's callbacks + layout function at
startup (~250 ms for hello-world; ~3-5 s for larger UIs), then
serves an HTML page that bootstraps the lifted wasm via a small
JS loader.

## What gets lifted

Per route:
  - **`/az/mini.<hash>.wasm`** — the framework event loop
    (~27 KB after M11 cascade lift). One per server.
  - **`/az/layout/<name>.<hash>.wasm`** — your `layout(...)`
    function + its transitive Rust deps (~85 KB for typical
    layouts). One per route.
  - **`/az/cb/<sym>.<hash>.wasm`** — each user callback as a
    standalone wasm (~700 B - 1 KB after M10-F optimization).

Sharded mode (`AZ_ENABLE_SHARDS=1`) splits framework helpers into
per-fn shards served from `/az/fn/<name>.<hash>.wasm`, with a
manifest at `/az/manifest.json`. Total wire bytes drop ~20-30%
vs legacy bundled mode on full apps.

## Event flow

Browser DOM events go through the wasm-side event loop (no
JS-side hit-test / cb dispatch / diff). Per event:

1. JS listener in `loader.js` encodes the event into a fixed
   256-byte buffer (see `dll/src/web/EVENT_PATCH_SCHEMA.md`).
2. `AzStartup_dispatchEvent` runs in wasm:
   - hit-tests the positioned-rects cache (from layout solve).
   - resolves the cb fn-addr → wasm table_idx.
   - `call_indirect`s the cb wasm.
   - if cb returns `RefreshDom`: emits TLV patches.
3. JS `azApplyPatches` decodes the TLV stream + mutates DOM.

See [`dll/src/web/EVENT_PATCH_SCHEMA.md`](../../dll/src/web/EVENT_PATCH_SCHEMA.md) for the full schema.

## What works (M11 status)

  - **Layout cb runs wasm-side**: returns `AzDom` via X8
    hidden-return.
  - **StyledDom cascade**: `StyledDom::create` lifts + runs
    (199-340 fns). Box::new init gap limits internal field
    population — see memory note `m11-complex-struct-box-new-lift`.
  - **Hit-test**: real bbox walk over the positioned-rects
    cache. Falls back to last-registered cb when cache is empty.
  - **Patch emission**: 12 TLV kinds in JS decoder
    (SetText, SetAttr, RemoveAttr, SetInlineStyle, RemoveNode,
    InsertNode, Focus, ScrollTo, AddClass, RemoveClass).
  - **Sharded mode** (`AZ_ENABLE_SHARDS=1`): boundary symbols
    factored into per-fn wasms; manifest-driven loading.

## What's deferred (next session)

  - **Real layout solver**: today's placeholder block layout
    stacks each node 30px below the previous. CSS-correct
    layout (text shaping, flexbox, grid) waits for the cascade
    init gap.
  - **`VirtualView` provider**: infrastructure in place; full
    auto-wrap + scroll-edge invocation pending layout solver.
  - **Touch / drag / composition / wheel events**: deferred per
    M11 Stage A.6 (not needed for the bench).
  - **CallbackChange → patch**: `Arc<Mutex<Vec<...>>>` drain
    deferred (Sprint 7 narrow). Today user cbs that call
    `info.set_focus_target(...)` etc. no-op on web.

## Deployment

The web backend is a single self-contained HTTP server. To
deploy:

  - Build the binary as above.
  - `scp` the binary + libazul.{so,dylib} to your server.
  - Run behind nginx / a reverse proxy.
  - `AZ_BACKEND=web://0.0.0.0:80` (or whatever port).

There's no separate frontend bundle to ship — the loader.js +
HTML are served by the binary at request time.

## Benchmark

The `azul-bench-flat.c` example + `scripts/bench-runner.js`
measure dispatch throughput. Initial numbers
(`scripts/BENCH_REPORT_M11_2026_05_19.md`): ~94,000 ops/sec at
10.7 μs/op on macOS arm64.

A `azul-bench-virtual.c` for the 10k-rows-via-VirtualView case
is staged but waits for the layout solver work.

## Debug knobs

| Env var | Effect |
|---------|--------|
| `AZ_NATIVE_REMILL=1` | Use in-process LLVM (fast). Default fast path. |
| `AZ_REMILL_KEEP_SCRATCH=1` | Preserve `$TMPDIR/azul-web-transpiler-<pid>/` for post-mortem. |
| `AZ_WASM_DEBUG=1` | Skip strip + LTO so wasm-objdump shows lifted-symbol names. |
| `AZ_REMILL_SKIP_WASM_OPT=1` | Skip post-link `wasm-opt -Oz`. |
| `AZ_WASM_MIRROR_TRACE=1` | Log per-page mirror skip events. |
| `AZ_ENABLE_SHARDS=1` | Opt into M10-D per-fn boundary shards. |
| `AZ_REMILL_MERGED_COMPILE=1` | Force merged compile (auto-on for ≤30 fns). |
| `AZ_REMILL_DISABLE_AUTO_MERGE=1` | Force per-fn .o path (regression-test). |
