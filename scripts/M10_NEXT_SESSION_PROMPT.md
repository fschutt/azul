# M10 — Next session prompt (post-export-fix)

Paste into the next agent's first message. Assumes fresh context.

---

You're continuing work on Azul's web backend (wasm-resident DOM).
The lift pipeline is green and shipping; per-cycle wasm sizes are
now layout 184 KB, on_click 15.5 KB, mini 13 KB.

Your task: implement **Workstream D + mini.wasm split** together.
Both are about partitioning monolithic wasm bundles into per-fn
shards keyed off `api.json`'s Framework set. The full architecture
plan is in `scripts/M10_D_PLAN.md` — read that first.

## Branch + commit state

```
Branch: layout-debug-clean
HEAD:   486c9742c web: stop exporting dep wrappers — layout.wasm 297 → 184 KB
```

## Read in order

1. `scripts/M10_D_PLAN.md` — comprehensive technical plan for
   per-fn sharding + mini split. Numbers, manifest shape,
   loader.js sketch, risks, acceptance.
2. `scripts/HANDOFF_2026_05_18_M9_E2E_GREEN.md` — current state
   + the four green acceptance gates.
3. `scripts/M10_DIAGNOSTIC_2026_05_18.md` — explains the prior
   B1.a 17%-not-50% finding; **WHY sharding is the right next
   move** (the State alloca survives via escape-via-call, so
   shrinking starts from "ship fewer per-cb things" not "make
   each thing smaller").

Skip-reading (older, superseded by the export fix + the D plan):
- `scripts/M10_RELIABILITY_AND_OPTIMIZATION_PLAN.md` — pre-A1
  framing; B1.b/B2/D sections all overtaken by the diagnostic +
  D plan.

## Verify gates before changing anything

```bash
cd examples/c
(cd ../.. && cargo build -p azul-dll --release \
   --features "build-dll web web-transpiler web-transpiler-static" \
   --no-default-features)

# Gate 1
pkill -f hello-world; sleep 1
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world-v5.bin &
sleep 5
node ../../scripts/m9_e2e/full-cycle.js          # PASS
node ../../scripts/m9_e2e/bump-reset-loop.js     # PASS 100 cycles
pkill -f hello-world; sleep 1

# Gate 2 + 3
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 8
node ../../scripts/m9_e2e/click-only.js          # PASS 7 clicks
node ../../scripts/m9_e2e/full-cycle.js          # PASS
node ../../scripts/m9_e2e/bump-reset-loop.js     # PASS 100 cycles
pkill -f hello-world; sleep 1
```

Expected sizes in the logs:
- `azul-mini.wasm: 13011 bytes`
- `lifted: on_click → 15510 bytes`
- `lifted: layout → 183991 bytes`

If those don't match, the previous session's work isn't all on
disk — investigate before starting D.

## Execute the M10-D plan in order

See `M10_D_PLAN.md` for full details. Six steps:

1. **Classifier + lift changes** — add `FnClass::BoundaryImport`;
   route `api.json::Framework` to it; lift BFS stops at
   boundaries. (~1 day)
2. **Boundary-lift pass** — second pass per server start that
   lifts every BoundaryImport's body into its own
   `/az/fn/<name>.<hash>.wasm`. (~1 day)
3. **mini.wasm split** — partition `EVENTLOOP_SYMBOLS` into
   `MINI_SHARDS` (core / events / layout / patches). Same
   lift-and-link pipeline, separate output wasm per shard.
   (~0.5 day)
4. **Manifest emission** — server emits
   `/az/manifest.<hash>.json` with `{shards, routes}`. (~0.5 day)
5. **loader.js dep-graph fetch + topo-instantiate** — parses
   manifest, parallel-fetches shards, topo-sorts, instantiates,
   wires cross-shard imports. (~0.5 day)
6. **New gates** — `full-cycle-sharded.js`,
   `bundle-size-comparison.js`, `cross-cb-dedup.js`. (~0.5 day)

## Hard rules

- **Keep the four existing acceptance gates GREEN.** They run
  against the bundled mode (gate behind `AZ_BUNDLED_LEGACY=1` if
  needed to keep them building after the shard migration).
- **Don't break the `Az*` API name surface.** Loader.js still
  wires `window.AZ` from the manifest-loaded shards; user-facing
  JS shouldn't notice the architectural change.
- **Cross-shard pointers stay raw i32 offsets.** All shards
  share one `WebAssembly.Memory` instance. No marshaling.
- **Boundary cycles get inlined back into the calling shard.**
  Detect via the visited-set; warn at lift time.
- **Don't ship before `bundle-size-comparison.js` shows ≥50%
  total wire bytes saved** for `hello-world.bin` first paint.

## Order rationale (why D before B1.b Option 3 / B2)

The diagnostic showed each per-cb body is dominated by lifted
State-machine simulation (~1088 B State alloca + GEPs per fn).
B1.b Option 3 (trim-state pass) shrinks each body proportionally
— but operates on each body in isolation.

D's win comes from a different axis: **each `Az*` body is shipped
ONCE per page** instead of per-cb. With 141 deps in layout and
~10 in on_click, the dedup factor alone is ~10×. After D, the
total per-cb wire bytes are dominated by the user's `sub_<entry>`
body, NOT the framework bodies. Then B1.b Option 3 can deliver
its proportional shrinkage on a much smaller surface.

So: D first (architectural), then B1.b Option 3 (per-body
shrinkage), then B2 (wrapper cleanup).

## When you're done

Commit each step with a clear message; co-author line:
```
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

After all six steps land:
- Update `scripts/HANDOFF_2026_05_18_M9_E2E_GREEN.md`'s M10
  progress table.
- Write a fresh `STATUS_REPORT_M10_<date>.md` with measured
  before/after sizes for hello-world-v5 and hello-world.bin.
- Supersede this prompt with a banner pointing at the new
  status report.

## Debug knobs (current)

- `AZ_NATIVE_REMILL=1` — fast in-process compile (default fast path)
- `AZ_REMILL_KEEP_SCRATCH=1` — preserve `$TMPDIR/azul-web-transpiler-<pid>/`
- `AZ_WASM_DEBUG=1` — subprocess path + names section (slow, ~30s/lift)
- `AZ_REMILL_SKIP_WASM_OPT=1` — skip post-link `wasm-opt -Oz`
- `AZ_WASM_MIRROR_TRACE=1` — log per-page mirror skip events
- `AZ_REMILL_MERGED_COMPILE=1` — opt-in M10-B1.b experimental
  (don't use for D work; mixed results)
- `AZ_BUNDLED_LEGACY=1` (NEW, add during step 6) — keep the old
  bundled per-cb wasm output for the legacy gates

## Things NOT to try

These were attempted previously and don't work — see
`M10_DIAGNOSTIC_2026_05_18.md` for details:
- alwaysinline on every dep + merged compile (recursion crashes)
- B1.a-style text-substitution metadata for State accesses
  (already shipped; provides metadata but doesn't unblock SROA
  because State escapes via call)
- addrspace(1) for wasm32 (LLVM errors at lowering)
- LTO bitcode + `wasm-ld --lto-O2` (same recursion hazard as
  alwaysinline-everywhere)
