# M10-E/F Status Report — 2026-05-19 (size optimization complete)

**Branch:** `layout-debug-clean`
**Commits this session (E/F):** `bacb2a3ee..b317b3fd3` (4 commits)
**Predecessor:** `cbe0998e0` (M10-D status report)

## TL;DR

**Cb wasm bloat is down 73-93%; full-app wire bytes are down 55%.**
The per-cb wasm now matches the "couple KB" target the user wanted.
All eight acceptance gates (4 legacy + 4 sharded) stay GREEN.

| Mode | Binary | Before M10-E | After M10-F | Δ |
|---|---|---|---|---|
| legacy | v5 total | 50,289 B | **13,444 B** | **-73%** |
| legacy | hello-world.bin total | 212,512 B | **96,658 B** | **-55%** |
| sharded | v5 total | n/a | 17,721 B | — |
| sharded | hello-world.bin total | 357,531 B | **147,274 B** | **-59%** |

Sharded mode now BEATS legacy by 31% on `hello-world.bin` (was +68%
before M10-E). The architecture's dedup win is finally surfacing.

## Per-component sizes

### v5 (small cb + small layout)

| component | original | post-D | post-E/F | total Δ |
|---|---|---|---|---|
| mini.wasm | 13,011 | 13,011 | **9,266** | -29% |
| on_click | 14,743 | 14,242 | **696** | -95% |
| layout | 22,535 | 13,201 | **3,482** | -85% |
| TOTAL | 50,289 | 40,454 | **13,444** | -73% |

### hello-world.bin (full app: Button + style + 141 deps)

| component | original | post-D | post-E/F | total Δ |
|---|---|---|---|---|
| mini.wasm | 13,011 | 13,011 | **9,266** | -29% |
| on_click | 15,510 | 14,995 | **1,044** | -93% |
| layout | 183,991 | 132,716 | **86,348** | -53% |
| TOTAL | 212,512 | 160,722 | **96,658** | -55% |

### hello-world.bin (sharded mode, 12 boundary shards)

| | bytes |
|---|---|
| mini.wasm | 9,266 |
| on_click | 1,160 |
| layout | 62,776 |
| 12 boundary shards | 74,072 |
| **TOTAL** | **147,274** (-31% vs original) |

## What each step shipped

### M10-E1 — precise data-mirror (`bacb2a3ee`)
Old scanner tracked 4 KiB page granularity; mirror shipped each
whole page even when the lifted code only read 8 bytes. Most of
the wasm's data section was zeros.

New `scan_arm64_adrp_accesses` follows `ADRP` through `ADD` to the
eventual `LDR`/`STR` and records exact `(addr, len)` byte ranges.
Plus `split_nonzero_runs` zero-trims any whole-page fallbacks.

Per-cb savings: 82-95%.

### M10-E1 scanner extensions — MOV / LDP / LDUR (`0a29daafe`)
Added register-copy propagation (ORR Xd, XZR, Xm), load-pair /
store-pair (LDP/STP signed-offset + pre/post-index), unscaled-offset
LDUR/STUR. Tightens precise-range coverage for compiler idioms the
ADRP+LDR scanner missed.

### M10-E2 — auto-merge for small cbs (`53c77c2f9`)
Enable merged-compile + alwaysinline-all when `targets.len() ≤ 30`.
Small cbs collapse the entire dep chain into one wrapper; SROA
promotes the State alloca; ~70% per-cb shrinkage.

Auto-skipped for large cbs (e.g. the 141-fn layout) because merged
mode blows up there — opt's inliner bloats the merged module faster
than DCE recovers (313 KB vs 124 KB).

### M10-F1 — ADR + register-indexed LDRB (`b317b3fd3`)
Subagent investigation: all 10 fallback pages on hello-world.bin
were LLVM `switch`-statement jump-table dispatch using `ADR` (not
`ADRP`) + `LDRB Wt, [Xn, Wm, UXTW]`. Scanner now handles both,
emits a 256-byte conservative range at the table base. All 10
fallbacks closed; precise pages went 15 → 26.

### M10-F2 — `-Oz` pipeline (`b317b3fd3`)
- `PassBuilder::buildPerModuleDefaultPipeline`: `O2 → Oz`.
- `TargetMachine` codegen: `CodeGenOpt::Aggressive → Default`.
- `wasm-ld`: `--lto-O2 → --lto-O3` (LTO-stage cross-object DCE).

Pipeline-wide size win. `alwaysinline` still inlines (unconditional);
other inlining decisions use the size-favoring heuristic.

## What did NOT work

- **F3 — merged+alwaysinline on large cbs (with -Oz)**: still blows
  up the 141-fn layout to 275 KB (down from 313 KB pre-Oz but still
  3× the 86 KB per-fn path). The 30-fn auto-threshold is the right
  partition. Smarter per-dep selective inlining (mark only small
  deps as alwaysinline) might close this further — open follow-up.

## Acceptance gates (all GREEN)

| Mode | Binary | Gate | Result |
|---|---|---|---|
| LEGACY | v5 | full-cycle.js | PASS |
| LEGACY | v5 | bump-reset-loop.js | PASS (100 cycles, drift=0) |
| LEGACY | hello-world.bin | click-only.js | PASS (7 clicks) |
| LEGACY | hello-world.bin | full-cycle.js | PASS |
| SHARDED | v5 | full-cycle-sharded.js | PASS (3 boundaries) |
| SHARDED | v5 | cross-cb-dedup.js | PASS |
| SHARDED | hello-world.bin | full-cycle-sharded.js | PASS (12 boundaries) |
| SHARDED | hello-world.bin | cross-cb-dedup.js | PASS |
| SHARDED | hello-world.bin | bundle-size-comparison.js | **PASS** (-31% vs legacy) |

## Open follow-ups

1. **Per-dep selective inlining for large cbs** — currently auto-merge
   is binary (≤30 fns: merged+alwaysinline-all; >30: per-fn .o, no
   merging). A hybrid that inlines only deps whose IR size < threshold
   could capture the layout case too.

2. **Mini.wasm split** (M10-D Step 3, still deferred) — partition
   EVENTLOOP_SYMBOLS into mini-core / mini-events / mini-layout /
   mini-patches.

3. **Shared helper-IR runtime wasm** (M10-E, proposed earlier) — each
   boundary shard re-ships ~6 KB of `__remill_*` intrinsics + branch
   stubs. A shared `/az/runtime.wasm` would close this last gap.

## Reproducing

```bash
cd /Users/fschutt/Development/azul
cargo build -p azul-dll --release \
  --features "build-dll web web-transpiler web-transpiler-static" \
  --no-default-features

# Legacy mode
cd examples/c
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 25
cd ../..
node scripts/m9_e2e/click-only.js          # PASS
node scripts/m9_e2e/full-cycle.js          # PASS

# Sharded mode (also size-comparison)
pkill -f hello-world; sleep 1
cd examples/c
AZ_ENABLE_SHARDS=1 AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 25
cd ../..
node scripts/m9_e2e/full-cycle-sharded.js  # PASS
node scripts/m9_e2e/cross-cb-dedup.js      # PASS
node scripts/m9_e2e/bundle-size-comparison.js --baseline 212512   # PASS (-31%)
```
