# Azul Web Backend — Handoff 2026-05-18 (M9 5-step e2e green)

**Branch:** `layout-debug-clean`
**Last commit:** `39536136a` (M9 mirror trace + optimized-e2e roadmap).
**Loop state:** PAUSED — full 5-step pipeline green for `v5` shape;
M10 plan written, ready for user-driven execution.

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
