# M10 — Next session prompt

Paste into the next agent's first message. Assumes fresh context.

---

You're continuing work on Azul's web backend (wasm-resident DOM).
M10 A1+B1.a+C1 landed and gates are green. B1.b is shipped as an
opt-in experimental knob (`AZ_REMILL_MERGED_COMPILE=1`) with mixed
results — wins for small dep graphs (on_click -23%), loses for
large dep graphs (layout +26%, 75 s lift time).

Three workstreams remain. Read these in order:
1. `scripts/HANDOFF_2026_05_18_M9_E2E_GREEN.md` — current state +
   gates.
2. `scripts/M10_DIAGNOSTIC_2026_05_18.md` — why B1.a hit 17% not
   50%; why B1.b's merged compile mixed-result.
3. This file — concrete plan for the remaining work.

## Branch + commit context

```
Branch: layout-debug-clean
HEAD:   64c638e38 web: M10-B1.b experimental merged-compile
```

Acceptance gates (all GREEN, default mode):
```
scripts/m9_e2e/full-cycle.js     on hello-world-v5.bin
scripts/m9_e2e/click-only.js     on hello-world.bin
scripts/m9_e2e/full-cycle.js     on hello-world.bin
scripts/m9_e2e/bump-reset-loop.js on hello-world-v5.bin (100 cycles)
```

## Workstream B1.b Option 3 — trim-state pass

**Goal:** shrink the 1088 B `%state_buf` to the actually-touched
~150 B subset (X0-X8 + X29/X30/SP/PC region). Same call structure
as today, proportionally smaller IR.

**Why this beats merged-compile:**
- Doesn't require inlining everything (no recursion-cycle hazard).
- Doesn't grow code size at small dep-graph scale.
- Each fn's State accesses shrink → less GEP+load/store traffic →
  smaller per-fn IR → smaller per-fn .o → smaller layout.wasm.

**Approach:**

Run as a C++ pass in `compile_inner` AFTER `linkInModule` and
BEFORE `buildPerModuleDefaultPipeline`:

1. Walk every function in the module.
2. For each, find `%struct.State` GEPs on its `%state` parameter.
   Record the (offset, size) of each access.
3. Module-wide: union all (offset, size) sets across all functions
   that share the State struct.
4. Compute a remap: live ranges → compact offsets, sorted.
5. Rewrite:
   - State allocas: change `[1088 x i8]` to `[live_size x i8]`.
   - Every `%state`-based GEP: replace the old offset with the new
     compact offset.
   - Typed GEPs (`getelementptr %struct.State, ptr %state, ...`):
     resolve via DataLayout to byte offset, remap, emit as byte GEP.

**Files to touch:**
- `dll/src/web/cpp/azul_remill.cpp` — add the pass.
- `dll/src/web/transpiler_remill.rs::emit_helper_ir` — change the
  State alloca declaration to a runtime-determined size if the
  pass needs help (probably not — the pass can rewrite both
  alloca and accessors).

**Risk:** typed-GEP traversal. The lift emits both byte GEPs
(`getelementptr i8, ptr %state, i64 544`) and typed GEPs
(`getelementptr %struct.State, ptr %state, i32 0, i32 0, i32 3, i32 17, ...`).
The pass must handle both. Typed GEPs need `DataLayout` lookups
to resolve to byte offsets, then byte GEP emission.

**Estimated effort:** 1-2 days of focused C++/LLVM work.

**Expected impact:** layout.wasm 294 KB → estimated 150-180 KB
(field-traffic proportionally drops to ~14% of original size).

## Workstream B2 — stack_buf in linear memory

**Goal:** eliminate the wrapper's only remaining `ptrtoint` (of
`%stack_buf`'s alloca address) by using a fixed wasm linear-memory
region for SP space.

**Why this is non-trivial:**

The naive "linkonce_odr global [32768 x i8]" approach collides:
every cb wasm declares its own copy, wasm-ld places each at its
local offset X, but because cb wasms share memory (imported from
mini), both end up writing to the same address X. Collisions.

The proper solution coordinates per-cb slots via the existing
`relocate_stack_if_non_mini` mechanism:

1. Add a new placeholder global in the wrapper IR:
   ```llvm
   @az_cb_stack_top = linkonce_odr global i32 0xDEADBEEF
   ```
2. In `relocate_stack_if_non_mini`, after patching `__stack_pointer`,
   ALSO patch `@az_cb_stack_top` to a unique linear-memory offset
   (e.g., `STACK_BUF_BASE + slot * 32 KiB + 32 KiB` = top of slot).
3. Wrapper IR uses:
   ```llvm
   %sp_int_i32 = load i32, ptr @az_cb_stack_top
   %sp_int = zext i32 %sp_int_i32 to i64
   ```
   No alloca, no ptrtoint of alloca.

**Files to touch:**
- `dll/src/web/transpiler_remill.rs::emit_helper_ir` — change
  wrapper template.
- `dll/src/web/transpiler_remill.rs::relocate_stack_if_non_mini` —
  patch the new global.
- `dll/src/web/transpiler_remill.rs::patch_wasm_sp_init` (or a new
  twin) — handle the second patch site.

**Estimated effort:** 1 day. The patching mechanism already
exists; just generalize to handle a second global.

**Expected impact:**
- cb wrapper IR ~50 bytes smaller (one fewer alloca + GEP + ptrtoint).
- AA picture cleaner — possibly small SROA wins on cascading
  analyses.
- Each cb invocation uses 32 KiB less wasm stack (so wasm stack
  can shrink from 64 KiB to ~32 KiB).

**Lower-priority after B1.b Option 3 lands** because by then the
State alloca is small enough that wrapper-level overhead doesn't
dominate.

## Workstream D — per-fn wasm sharding

**Goal:** ship each lifted dep as its own wasm file, dedup'd
across cbs. Today each cb bundles its full dep closure (e.g.
layout.wasm = 297 KB carrying 141 deps inline). With sharding,
each dep is its own ~1-5 KB wasm, fetched once per page.

**MVP scope (one session):**

1. Server-side bundle split:
   - In `lift_with_transitive_deps_batched`, after the lift loop,
     emit each dep's .o as its own wasm via `link_objects_to_wasm`
     with `--export=__az_dep_<addr>` only.
   - Each dep wasm imports its own deps from `env`.
   - Emit a manifest JSON: `{ "on_click.<hash>.wasm": {"deps":
     ["__az_dep_X.wasm", "__az_dep_Y.wasm", ...]}, ... }`.

2. Server-side serve:
   - Existing `/az/cb/<name>.<hash>.wasm` endpoint serves the
     entry wasm.
   - Add `/az/dep/<addr>.<hash>.wasm` endpoint for dep wasms.
   - Add `/az/manifest.json` endpoint.

3. Client-side loader.js:
   - Fetch manifest first.
   - Topologically sort deps; for each cb, fetch dep wasms in
     parallel.
   - Instantiate in dep order (deps first, then entry).
   - Wire each entry's `env.__az_dep_X` import to the dep wasm's
     export.

4. Acceptance: extend `full-cycle.js` to verify sharded mode works
   for the v5 cycle. Add a new gate
   `full-cycle-sharded.js` that asserts the same result with
   sharded loading.

**Files to touch:**
- `dll/src/web/transpiler_remill.rs::lift_with_transitive_deps_batched`
- `dll/src/web/server.rs` (manifest + dep endpoints)
- `dll/src/web/html_render.rs` (embed manifest URL in bootstrap HTML)
- `dll/src/web/loader_js.rs` (dep-graph traversal + parallel fetch)
- `scripts/m9_e2e/full-cycle-sharded.js` (new gate)

**Estimated effort:** 2-3 days. The architectural shape is clear;
the gritty bits are the manifest format + loader.js fetch
choreography.

**Expected impact:**
- First-paint latency: 141 small wasm downloads in parallel vs
  one big sequential download.
- Cross-cb dedup: deps shared by on_click + layout ship once.
- Cache: dep wasms keyed by content hash; cb body changes don't
  re-fetch deps.

## Order of operations

1. **B1.b Option 3 (trim-state pass)** — biggest single size win,
   no architectural change, foundation for everything else.
2. **B2 (stack_buf in linear memory)** — mechanical follow-up
   once State is small.
3. **D (per-fn sharding)** — orthogonal, ship after B1.b is
   stable so the sharded sizes reflect real post-trim numbers.

## Things NOT to try (already attempted)

- **alwaysinline on every dep + merged compile**: crashes silently
  on dep graphs with recursion cycles. Won't work without
  pre-pruning the call graph.
- **B1.a-style text-substitution metadata for State accesses**:
  already shipped; provides metadata but doesn't unblock SROA
  because State escapes via call to non-inlined deps.
- **Address-space partitioning at addrspace(1)**: wasm32 reserves
  it for `wasm_var` globals; LLVM errors at lowering.
- **LTO bitcode + wasm-ld --lto-O2**: requires switching llc
  output from wasm32 obj to LLVM bitcode; doesn't help the size
  picture (LTO inliner has same recursion hazard).

## Debug knobs (current)

- `AZ_NATIVE_REMILL=1` — fast in-process compile (default fast path)
- `AZ_REMILL_MERGED_COMPILE=1` — opt-in merged transitive compile
- `AZ_REMILL_MERGED_ALWAYSINLINE=1` — force alwaysinline-everywhere
  (crashes on cycles)
- `AZ_REMILL_KEEP_SCRATCH=1` — preserve `$TMPDIR/azul-web-transpiler-<pid>/`
- `AZ_WASM_DEBUG=1` — subprocess path + names section (slow)
- `AZ_REMILL_SKIP_WASM_OPT=1` — skip post-link `wasm-opt -Oz`
- `AZ_WASM_MIRROR_TRACE=1` — log per-page mirror skip events

## Gates to maintain

Every change MUST keep these passing:
```bash
cd examples/c
pkill -f hello-world; sleep 1
AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world-v5.bin &
sleep 5
node /Users/fschutt/Development/azul/scripts/m9_e2e/full-cycle.js
node /Users/fschutt/Development/azul/scripts/m9_e2e/bump-reset-loop.js
pkill -f hello-world; sleep 1

AZ_NATIVE_REMILL=1 DYLD_LIBRARY_PATH=../../target/release \
  AZ_BACKEND=web://127.0.0.1:8800 ./hello-world.bin &
sleep 8
node /Users/fschutt/Development/azul/scripts/m9_e2e/click-only.js
node /Users/fschutt/Development/azul/scripts/m9_e2e/full-cycle.js
pkill -f hello-world; sleep 1
```
