# Azul Web Backend — M10-D Status Report (2026-05-19)

> **SUPERSEDED by M10-E/F** — see
> [`STATUS_REPORT_M10_F_2026_05_19.md`](STATUS_REPORT_M10_F_2026_05_19.md)
> for the latest measurements. The size axis has since shrunk per-cb
> wasms by another 73-95% via precise data-mirror, auto-merge, and
> -Oz pipeline. Sharded mode now BEATS legacy by 31% on
> `hello-world.bin` (this doc still showed it losing by +68%).
> The architectural description below is still accurate; only the
> wire-byte numbers are stale.

**Branch:** `layout-debug-clean`
**Commits this session:** `f30d0ec02` (web: M10-D sharded mode)
**Predecessor:** `c77ff2265` (docs: M10-D plan)

## TL;DR

M10-D **architectural plumbing landed**. `api.json::Framework` symbols
now ship as separate per-fn wasm shards behind `AZ_ENABLE_SHARDS=1`;
`/az/manifest.json` describes the shard graph; loader.js parses it
and pre-loads boundary bodies into the shared env namespace. All
four legacy gates stay GREEN; three new sharded gates pass.

**Net wire bytes for single-cb shapes are higher** in sharded mode
(per-shard helper-IR overhead dominates). The architectural win
materializes for multi-cb apps where the same boundary is referenced
by many cbs — there the (N-1) × per-boundary-savings dwarf the
overhead. Mini.wasm split + shared-helper-IR runtime are follow-ups
that close the remaining size gap.

## Acceptance-gate matrix

| Mode    | Binary              | Gate                            | Result | Notes |
|---------|---------------------|---------------------------------|--------|-------|
| LEGACY  | hello-world-v5.bin  | `full-cycle.js`                 | PASS   | unchanged |
| LEGACY  | hello-world-v5.bin  | `bump-reset-loop.js`            | PASS   | 100 cycles, drift=0 |
| LEGACY  | hello-world.bin     | `click-only.js`                 | PASS   | 7 clicks |
| LEGACY  | hello-world.bin     | `full-cycle.js`                 | PASS   | unchanged |
| SHARDED | hello-world-v5.bin  | `full-cycle-sharded.js`         | PASS   | 3 boundaries loaded |
| SHARDED | hello-world-v5.bin  | `cross-cb-dedup.js`             | PASS   | invariants hold |
| SHARDED | hello-world.bin     | `full-cycle-sharded.js`         | PASS   | 12 boundaries loaded |
| SHARDED | hello-world.bin     | `cross-cb-dedup.js`             | PASS   | invariants hold |
| SHARDED | hello-world.bin     | `bundle-size-comparison.js`     | INFO   | see below |

## Wire-bytes — before vs after

### hello-world-v5.bin (small cb, 2 boundaries)

| Mode    | mini  | cb     | layout | boundaries (3×) | **Total** |
|---------|-------|--------|--------|-----------------|-----------|
| LEGACY  | 13011 | 14743  | 22535  | bundled inline  | **50289** |
| SHARDED | 13011 | 14242  | 13201  | 27511 (3 shards)| **67965** |
| Δ       | 0     | -501   | -9334  | +27511          | +17676 (+35%) |

### hello-world.bin (full app, 12 boundaries)

| Mode    | mini  | cb     | layout | boundaries (12×) | **Total** |
|---------|-------|--------|--------|------------------|-----------|
| LEGACY  | 13011 | 15510  | 183991 | bundled inline   | **212512**|
| SHARDED | 13011 | 14995  | 132716 | 195357 (12 shards)| **356079**|
| Δ       | 0     | -515   | -51275 | +195357          | +143567 (+68%) |

The boundary shards include their own copy of the per-shard helper
IR (the `__remill_*` intrinsics, branch stubs, State alloca
scaffolding) plus any Rust-internal deps the boundary itself calls
(non-Framework deps still get bundled — only `Az*` symbols become
boundaries). For `AzButton_dom`, that internal closure is 110 KB.

The win arrives when N cbs share the same boundary set: legacy mode
ships each shared body N times; sharded mode ships it once.

## What landed

| Layer | Change | File |
|-------|--------|------|
| classifier | `FnClass::BoundaryImport` variant + `api.json::Framework → BoundaryImport` (gated on `AZ_ENABLE_SHARDS`) | `symbol_table.rs` |
| BFS | `used_boundaries: HashSet<usize>` tracked through both batched + sequential lifts | `transpiler_remill.rs` |
| helper IR | `BoundaryImport` arm in `emit_helper_ir` (no body — wasm-ld's `--allow-undefined` emits env-import) | `transpiler_remill.rs` |
| TransitiveLiftRoot | `extra_exports: Vec<String>` field — appended to wasm-ld `--export` list at link | `transpiler_remill.rs` |
| boundary lift | `RemillTranspiler::lift_boundary_to_wasm` + `BoundaryShard` struct | `transpiler_remill.rs` |
| orchestration | `web::lift_boundary_shards` — work-queue lift of unique boundaries with transitive closure | `mod.rs` |
| route | `GET /az/fn/<name>.<hash>.wasm` (shard delivery) | `server.rs` |
| route | `GET /az/manifest.json` (no-cache shard map) | `server.rs` |
| loader.js | `azLoadBoundaryShards()` — parallel-fetch + register into `azBoundarySymbols` map | `loader_js.rs` |
| loader.js | `azCallbackImports()` env-proxy routes boundary symbols before stub-noop fallback | `loader_js.rs` |
| gate | `scripts/m9_e2e/full-cycle-sharded.js` — manifest-driven e2e | new |
| gate | `scripts/m9_e2e/bundle-size-comparison.js` — per-shard byte table | new |
| gate | `scripts/m9_e2e/cross-cb-dedup.js` — manifest invariants | new |

## Rollout knobs

| Env var | Effect |
|---------|--------|
| `AZ_ENABLE_SHARDS=1` | Opt-in: api.json Framework → BoundaryImport, boundary lift pass runs, manifest contains shards. |
| `AZ_BUNDLED_LEGACY=1` | Force legacy behavior (overrides `AZ_ENABLE_SHARDS`). Mostly for future-proofing once default flips. |
| (default) | Legacy bundled mode. All four legacy gates pass unchanged. |

## What's NOT done (and why)

### M10-D Step 3 — `mini.wasm` split (DEFERRED)

The original plan called for partitioning `EVENTLOOP_SYMBOLS` into
MINI_SHARDS (`core` / `events` / `layout` / `patches`) so a static
page can ship only ~3 KB of bootstrap. Architecturally identical to
boundary sharding but operating on the AzStartup_* symbol set
instead of api.json Framework.

Deferred because:
- The four boundary-lift steps + manifest + loader rewiring took
  the bulk of this session.
- Mini.wasm is already small (13 KB) — splitting it shaves a few
  KB at most.
- The boundary architecture is independently testable and shippable.

To implement: add `MINI_SHARDS: &[(&str, &[&str])]` partition table,
modify `lift_and_link_eventloop` to lift each shard to its own
wasm, surface them in the manifest as `mini_shards: [...]`.

### B1.b Option 3 — trim State alloca (FOLLOW-UP)

Pre-M10-D diagnostic
(`scripts/M10_DIAGNOSTIC_2026_05_18.md`) showed each per-cb wasm
dominated by 1088-byte State alloca + GEPs. M10-D shrunk the SET
of per-cb deps but each surviving dep still has the State overhead.
A trim-state pass would compact 1088 B → ~150 B per fn — multiplied
across every lifted body (cb + layout + boundary shards).

### M10-E — shared helper-IR runtime wasm (NEW IDEA)

Each boundary shard ships its OWN copy of the helper IR (~16 KB:
`__remill_*` intrinsics + branch stubs + State alloca scaffolding +
`@__az_bump_ptr` global). With 12 boundary shards × 16 KB = ~200 KB
of duplicated helpers. A shared `/az/runtime.wasm` exporting the
helper symbols + globals, imported by every boundary/cb/layout
shard, would close this gap entirely.

`linkonce_odr` already dedupes within a single wasm-ld link; M10-E
extends the same idea across links via wasm imports.

## Reproducing

### Legacy gates
```bash
cd examples/c
(cd ../.. && cargo build -p azul-dll --release \
   --features "build-dll web web-transpiler web-transpiler-static" \
   --no-default-features)

# v5
pkill -f hello-world; sleep 1
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world-v5.bin &
sleep 5
node ../../scripts/m9_e2e/full-cycle.js          # PASS
node ../../scripts/m9_e2e/bump-reset-loop.js     # PASS
pkill -f hello-world; sleep 1

# full hello-world.c
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 10
node ../../scripts/m9_e2e/click-only.js          # PASS
node ../../scripts/m9_e2e/full-cycle.js          # PASS
pkill -f hello-world; sleep 1
```

### Sharded gates
```bash
# v5
pkill -f hello-world; sleep 1
AZ_ENABLE_SHARDS=1 AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world-v5.bin &
sleep 10
node ../../scripts/m9_e2e/full-cycle-sharded.js  # PASS
node ../../scripts/m9_e2e/cross-cb-dedup.js      # PASS
pkill -f hello-world; sleep 1

# full hello-world.c
AZ_ENABLE_SHARDS=1 AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 25
node ../../scripts/m9_e2e/full-cycle-sharded.js  # PASS
node ../../scripts/m9_e2e/cross-cb-dedup.js      # PASS
node ../../scripts/m9_e2e/bundle-size-comparison.js --baseline 212512   # INFO
pkill -f hello-world; sleep 1
```
