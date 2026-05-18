# Azul Web Backend — Handoff 2026-05-18 (M9 5-step e2e green)

> **SUPERSEDED 2026-05-19** — see [`STATUS_REPORT_M10_F_2026_05_19.md`](STATUS_REPORT_M10_F_2026_05_19.md)
> for the M10-E/F size-optimization landing (cb wasms 696-3,482 B,
> hello-world.bin 96 KB legacy / 147 KB sharded — sharded now BEATS
> legacy by 31% vs original baseline). [`STATUS_REPORT_M10_2026_05_19.md`](STATUS_REPORT_M10_2026_05_19.md)
> still describes the M10-D boundary-shard architecture. The table
> below is updated through `179a7d717`; everything below the
> "Original M9 close-out" divider is the original M9 handoff kept for
> context.

**Branch:** `layout-debug-clean`
**Last commit:** `f30d0ec02` (M10-D sharded mode).
**Loop state:** PAUSED — M10-D architectural plumbing landed. Step 3
(mini.wasm split) + M10-E (shared helper-IR runtime) are follow-ups.

## M10 progress (current)

| Workstream | Status | Result |
|------------|--------|--------|
| **A1** libsystem classifier override by address | ✅ landed `9f852fb6e` | full-cycle.js GREEN on `hello-world.bin`; layout.wasm 388→353 KB (-9%) |
| **B1.a** alias-scope metadata on guest/host mem ops | ✅ landed `c600271d9` | layout.wasm 353→294 KB (-17%); 50% target unmet — escalation to B1.b possible |
| **C1** bump-heap snapshot/reset helpers | ✅ landed `0fe055b4a` | new `bump-reset-loop.js` gate: 100 cycles, drift=0, counter 5→105 |
| (export-fix) | ✅ landed `486c9742c` | layout.wasm 297→184 KB (-38%, full hello-world.c) |
| **D** per-fn wasm sharding (M10-D Steps 1+2+4+5+6) | ✅ landed `f30d0ec02` | sharded gates GREEN; per-cb shrinks; full wire-byte win requires multi-cb dedup. See `STATUS_REPORT_M10_2026_05_19.md` |
| **E1** precise adrp+ldr data-mirror | ✅ landed `bacb2a3ee` + `0a29daafe` | per-cb data 12KB → 250B; MOV/LDP/LDUR widening |
| **E2** auto-merge for small cbs | ✅ landed `53c77c2f9` | cbs ≤30 fns inline transitively, SROA evaporates State; v5 on_click 14743 → 696 B |
| **F1** ADR + register-indexed LDRB | ✅ landed `b317b3fd3` | closed 10 fallback pages (jump-table dispatch); layout 124KB → 86KB |
| **F2** -Oz LLVM pipeline | ✅ landed `b317b3fd3` | PassBuilder Oz + CodeGenOpt::Default + --lto-O3; cross-cutting size compression |
| **D-Step3** mini.wasm split | not started | partition `EVENTLOOP_SYMBOLS` into MINI_SHARDS (deferred follow-up) |
| **E3** shared helper-IR runtime wasm | proposed | each boundary shard re-ships ~6 KB of `__remill_*` helpers; one shared runtime would close the size gap |
| **F3** smart selective inlining for large cbs | proposed | merged + alwaysinline-only-small-deps would close the 86 KB layout gap |
| **B1.b** real LLVM pass for provenance tracking | not started | needed if more SROA wins required beyond B1.a's 17% |
| **B2** stack_buf in linear memory | not started | follow-up to B1.x; eliminates last ptrtoint in wrapper |

### Current acceptance-gate status (all GREEN)

```
# Legacy (bundled) mode — default
scripts/m9_e2e/full-cycle.js          on hello-world-v5.bin   PASS
scripts/m9_e2e/click-only.js          on hello-world.bin      PASS
scripts/m9_e2e/full-cycle.js          on hello-world.bin      PASS  ← M10-A1
scripts/m9_e2e/bump-reset-loop.js     on hello-world-v5.bin   PASS  ← M10-C1 (100 cycles)

# Sharded mode — AZ_ENABLE_SHARDS=1
scripts/m9_e2e/full-cycle-sharded.js  on hello-world-v5.bin   PASS  ← M10-D
scripts/m9_e2e/full-cycle-sharded.js  on hello-world.bin      PASS  ← M10-D
scripts/m9_e2e/cross-cb-dedup.js      on hello-world*.bin     PASS  ← M10-D
scripts/m9_e2e/bundle-size-comparison.js                      INFO  ← M10-D
```

Layout.wasm sizes (legacy mode):
- v5: 22 KB (was 30 KB pre-M10)
- full hello-world.c: 184 KB (was 388 KB pre-M10, -53% total)

Layout.wasm sizes (sharded mode, AZ_ENABLE_SHARDS=1):
- v5: 13 KB (-41% vs legacy v5)
- full hello-world.c: 133 KB (-28% vs legacy full)

(The per-cb wasm size shrinks proportionally to the boundary fns
factored out. The sharded total wire-bytes is HIGHER for single-cb
shapes due to per-shard helper-IR overhead — the win materializes
with cross-cb dedup. See STATUS_REPORT_M10_2026_05_19.md.)

---

## Original M9 close-out (kept for context)

---

## What works (acceptance gates)

Both of these pass, end-to-end, with no manual intervention:

```bash
# Layout cb runs in WASM, populates layout cache, hit-test → cb → patch.
cd examples/c
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world-v5.bin &
sleep 4
node ../../scripts/m9_e2e/full-cycle.js
# → PASS: full 5-step pipeline works end-to-end
#         bootstrap → layout → click → hit-test → cb → patch

# on_click cb counter, dispatched via mini's call_indirect bridge.
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 4
node ../../scripts/m9_e2e/click-only.js
# → click 5-7: Update=1 counter N -> N+1 OK
```

Wire sizes:

| Demo                         | HTML  | mini  | cb    | layout  | TOTAL    |
|------------------------------|-------|-------|-------|---------|----------|
| v5 (body + on_click)         | 22 KB | 14 KB | 16 KB | 30 KB   | **80 KB** |
| full hello-world.c           | 23 KB | 14 KB | 18 KB | 388 KB  | **433 KB** |

---

## What doesn't work (the M10 task list)

Three distinct issues, all written up in
[scripts/M10_RELIABILITY_AND_OPTIMIZATION_PLAN.md](M10_RELIABILITY_AND_OPTIMIZATION_PLAN.md):

1. **Full hello-world.c layout cb returns rc=0 with zero data**
   (cb shapes that pull in `AzString_copyFromBytes` /
   `AzDom_createText`). Bisected to libsystem stub
   mis-classification: `_platform_memmove` resolves to
   `sub_1866bc250` which classifies as `Recursable` (= lift as
   full Rust fn) but its native addr is outside any tracked image
   so the synth_addr stays at 6.5 GB → unreadable in wasm. The
   lifted body returns garbage that propagates.
   - Confirmed via `AZ_WASM_MIRROR_TRACE=1` showing 12 libsystem
     pages SKIP per cb (not in any tracked image).
   - Naive "default Leaf if not Az-prefixed" classifier change
     broke on_click (some bare-`Az*` Rust monomorphizations are
     genuinely Recursable). Needs addr+name based classifier.
   - **M10 Workstream A1** (1 day estimate).

2. **State alloca (1 KB) survives SROA in every cb wasm**, making
   layout.wasm 388 KB instead of estimated 50-80 KB. Root cause:
   `inttoptr i64 %addr to ptr` of register-derived guest addresses
   makes LLVM AA conservatively assume guest pointers might alias
   the State alloca. mcsema/AnvILL solve this on x86 with
   `addrspace(1)` partitioning, but **wasm32 reserves addrspace(1)
   for wasm_var (globals) — LLVM errors at lowering** (confirmed
   with a one-line experiment). The wasm-native equivalent is
   `!alias.scope` + `!noalias` metadata on every guest load/store.
   - **M10 Workstream B1** (2 days estimate).

3. **Bump heap never reclaims**: `@__az_bump_ptr` starts at 96 MiB
   and grows monotonically. 100 layout cycles leak ~MBs. Native
   code has `__rust_dealloc` markers we ignore.
   - **M10 Workstream C1** (per-cycle reset, half a day). C2
     (free-list backed by lifted `__rust_dealloc`) deferred until
     a cb pattern actually needs cross-cycle persistence.

After A + B + C land, **M10 Workstream D** (per-fn wasm sharding)
deduplicates api.json fns across cbs.

---

## Today's commits (timeline)

```
39536136a web: M9 mirror trace + optimized-e2e roadmap
c29508355 web: full 5-step e2e cycle green (bootstrap → layout → click → cb → patch)
732960155 web: per-page data mirror — mini.wasm 27 MiB → 13.5 KiB
b5fd685f7 docs: web.md + M9_REVIEW updates for the synthetic-address landing
23d7174d5 web: M9-after-review synthetic-address lift scheme
```

`23d7174d5` introduced per-image synthetic addressing; `732960155`
replaced the 27 MiB section mirror with a 4 KiB per-page mirror
keyed off `scan_arm64_adrp_pages`; `c29508355` bumped wrapper
stack_buf 4 KiB→32 KiB and added pointer translation in mirrored
pages (every 8-byte aligned native pointer rewrites to its synth
counterpart) — this is what unblocked the 5-step e2e.

---

## Documents to read (and to NOT read)

**Read these in order:**

1. [scripts/M10_RELIABILITY_AND_OPTIMIZATION_PLAN.md](M10_RELIABILITY_AND_OPTIMIZATION_PLAN.md) — the active plan.
2. [scripts/M9_OPTIMIZED_E2E_TARGET.md](M9_OPTIMIZED_E2E_TARGET.md) — long-term size target context.
3. [scripts/WASM_SHIPPING_OPTIONS.md](WASM_SHIPPING_OPTIONS.md) — per-fn vs bundled wasm tradeoffs.
4. [scripts/M9_REVIEW_AND_OPTION_A.md](M9_REVIEW_AND_OPTION_A.md) — synthetic-address lift scheme details.
5. [doc/guide/en/internals/web.md](../doc/guide/en/internals/web.md) — runtime architecture overview.

**Already superseded** (banners at top, kept for git history only):

- `scripts/M9_WASM_DOM_HANDOFF.md`
- `scripts/M9_NEW_SESSION_PROMPT.md`
- `scripts/M8.9_REMILL_HANDOFF.md`
- `scripts/M8.8_NEW_SESSION_PROMPT.md`
- `scripts/M8_7_HYDRATION_PLAN_2026_05_16.md`

---

## Repro environment

- macOS arm64 (darwin 24.5.0).
- `cargo build -p azul-dll --release --features "build-dll web web-transpiler web-transpiler-static" --no-default-features`
- Subprocess wasm-ld must exist at `/opt/homebrew/opt/llvm@21/bin/wasm-ld` (lld 21). Static linker (`AZ_NATIVE_REMILL=1`) is the fast path; subprocess path is fallback + works with `AZ_WASM_DEBUG=1` to keep names section.
- Debug knobs: `AZ_WASM_MIRROR_TRACE=1` (per-page mirror diagnostic),
  `AZ_REMILL_KEEP_SCRATCH=1` (don't wipe `$TMPDIR/azul-web-transpiler-<pid>`),
  `AZ_REMILL_SKIP_WASM_OPT=1` (skip `-Oz` post-process), `AZ_WASM_DEBUG=1`
  (skip strip + LTO so wasm-objdump shows lifted-symbol names).

---

## Diagnostic .c shapes that pin down where things break

These are in `examples/c/` for quick repro:

- `hello-world-v5.c` — body + AzDom_addCallback. **Works** (full e2e).
- `hello-world-medium.c` — Button + AzDom_addChild (no AzString).
  Zero output, no trap.
- `hello-world-v4.c` — createText + delete + body. Zero output, no
  trap. Smallest repro of the libsystem-stub mis-classification
  bug.
- `hello-world.c` — original (snprintf + Button + addChild + style).
  Zero output for the same reason as v4.
