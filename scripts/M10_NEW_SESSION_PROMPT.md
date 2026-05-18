# M10 — New session prompt

Paste this into the next agent's first message. It assumes a fresh
context with no memory of the prior session.

---

You're continuing work on Azul's web backend (wasm-resident DOM).
The 5-step e2e cycle (bootstrap → layout → click → cb → patch) is
**green** for one demo (`hello-world-v5.bin`) and **broken** for
the full `hello-world.c`. Your task is the M10 reliability +
optimization plan.

**Branch:** `layout-debug-clean` (tip: `39536136a`).

## Read first (in order)

1. `scripts/HANDOFF_2026_05_18_M9_E2E_GREEN.md` — what works today,
   what doesn't, last 5 commits with context.
2. `scripts/M10_RELIABILITY_AND_OPTIMIZATION_PLAN.md` — the active
   plan, four workstreams with acceptance gates.
3. `scripts/M9_OPTIMIZED_E2E_TARGET.md` — long-term size target
   (sub-10 KB per cb).

Skim only:
- `doc/guide/en/internals/web.md` — runtime architecture, current state.
- `scripts/WASM_SHIPPING_OPTIONS.md` — per-fn vs bundled context for D.

Ignore (already superseded, banners at top):
- `scripts/M9_*HANDOFF*.md`, `scripts/M9_NEW_SESSION_PROMPT.md`,
  `scripts/M8.9_REMILL_HANDOFF.md`, `scripts/M8.8_NEW_SESSION_PROMPT.md`,
  `scripts/M8_7_HYDRATION_PLAN_2026_05_16.md`.

## Verify before changing anything

```bash
cd examples/c
# Build
(cd ../.. && cargo build -p azul-dll --release \
   --features "build-dll web web-transpiler web-transpiler-static" \
   --no-default-features)

# Gate 1: full 5-step cycle (currently GREEN)
pkill -f hello-world; sleep 1
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world-v5.bin &
sleep 4
node /Users/fschutt/Development/azul/scripts/m9_e2e/full-cycle.js
# Expect: PASS: full 5-step pipeline works end-to-end
pkill -f hello-world

# Gate 2: on_click counter (currently GREEN)
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 4
node /Users/fschutt/Development/azul/scripts/m9_e2e/click-only.js
# Expect: click N: Update=1 counter N -> N+1 OK (for 5,6,7)
pkill -f hello-world

# Gate 3: full hello-world.c layout (currently FAILS, target of A1)
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 4
node /Users/fschutt/Development/azul/scripts/m9_e2e/layout-probe.js
# Currently: rc=0 but current_dom[0..64]: all zeros → PROBE FAIL
# After A1: current_dom[0..64] has non-zero bytes → PROBE PASS
pkill -f hello-world
```

## Execute the M10 plan in order

Each workstream has an acceptance gate in the plan doc. Don't move
to the next until the current one's gate passes AND the prior
gates still pass (no regressions).

**Start with A1**: address-based classifier override in
`dll/src/web/symbol_table.rs::assign_synthetic_addresses`. After
rebasing the tracked images, walk every `SymbolEntry` and if its
`canonical_addr` is outside ALL `image_rebases` ranges, force
`classification = FnClass::Leaf`. The libsystem stub
`_platform_memmove` (and similar) currently classify as
`Recursable` based on name fallback; this fix overrides on address.

Estimated 1 day for A1, 2 days for B1.a (alias-scope metadata in
helper IR), half a day for C1 (per-cycle bump reset), then re-plan.

## Hard rules

- **Never use `addrspace(1)` on wasm32 for memory ops.** That
  address space is `wasm_var` (globals); LLVM errors with
  "Encountered an unlowerable store to the wasm_var address
  space". Use `!alias.scope`/`!noalias` metadata instead.
- **Don't change the classifier by name alone** without an
  address check — bare-`Az*` Rust monomorphizations share
  the bucket with libsystem and a name-only default-to-Leaf
  breaks on_click silently.
- **Keep both acceptance gates green** through every change.
- **Don't add backwards-compat shims** for the old SP-allocator
  / data-mirror code paths — the synthetic-address scheme is
  the load-bearing one, prior approaches are removed.

## Debug knobs (set as env vars)

- `AZ_WASM_MIRROR_TRACE=1` — log every native→synth page mapping
  + which pages get skipped (not in any tracked image). Surfaces
  the libsystem-stub issue immediately.
- `AZ_REMILL_KEEP_SCRATCH=1` — preserve
  `$TMPDIR/azul-web-transpiler-<pid>/` after exit so you can
  inspect `.lifted.ll`, `.patched.ll`, `.helper.ll`, `.o` files.
- `AZ_WASM_DEBUG=1` — keep names section + skip `wasm-opt`/`--lto-O2`
  in subprocess wasm-ld path so stack traces show
  `sub_<canonical_addr_hex>` instead of bare `func[N]`. Forces
  subprocess path (slower lift, ~30 s instead of ~4 s).
- `AZ_REMILL_SKIP_WASM_OPT=1` — skip `wasm-opt -Oz` post-process
  to see un-optimized link output.

## When you're done with each workstream

Commit with a clear message. Co-author line:
```
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

After each landing, update the progress section in
`scripts/HANDOFF_2026_05_18_M9_E2E_GREEN.md` so the next agent
knows where we are. When all of A+B+C+D land, write a fresh
`STATUS_REPORT_M10_<date>.md` and supersede this prompt with a
banner at the top.
