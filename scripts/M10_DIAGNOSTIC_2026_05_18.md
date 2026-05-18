# M10 — Diagnostic: why B1.a stopped at 17%

**Date:** 2026-05-18 (continuation session).
**Context:** M10 A1+B1.a+C1 landed (`9f852fb6e..0fe055b4a`). B1.b
investigation done; conclusion: the SROA wall is structural to the
inter-fn call graph, not the metadata.

---

## The investigation

Goal: B1.a's plan target was -50% on `layout.wasm`; actual was -17%
(353 → 294 KB). Find out why.

Approach: run the subprocess path (`unset AZ_NATIVE_REMILL`) on
`hello-world-v5.bin` so `opt -O2 -S` writes `*.opt.ll` to scratch.
Inspect `callback.opt.ll`.

## What B1.a actually achieved

Metadata IS preserved through `opt -O2`:

```llvm
define noundef i32 @callback(i64 %arg0_lo, i64 %arg0_hi, i32 %arg1, i32 %out_ptr) {
  %state_buf = alloca [1088 x i8], align 16        ; ←─ STILL HERE
  %stack_buf = alloca [32768 x i8], align 16       ; ←─ STILL HERE
  call void @llvm.memset(... %state_buf, ...)
  %arg0_lo_p = getelementptr ... %state_buf, i64 544
  store i64 %arg0_lo, ptr %arg0_lo_p, align 16, !alias.scope !0, !noalias !3
  ...
  store i64 %sub.i.i.i, ptr %sp_slot, align 16, !alias.scope !8, !noalias !9
  %p.i19 = inttoptr i64 %1 to ptr
  store i64 0, ptr %p.i19, align 16, !alias.scope !3, !noalias !8
  ...
  ; calls to lifted dep functions:
  %3 = call ptr @sub_13ab04(ptr nonnull %state_buf, i64 81556, ptr null)
  %4 = load i64, ptr %sp_slot, ...
  %6 = call ptr @sub_97fcac(ptr nonnull %state_buf, i64 81592, ptr null)
  %7 = call ptr @sub_1485ec(ptr nonnull %state_buf, i64 81568, ptr null)
  ...
```

Several distinct scope IDs (`!0/!3/!8/!9`) appear — llvm-link
renumbered our string-named scopes; the metadata graph is intact.

LLVM even inserted `call void @llvm.experimental.noalias.scope.decl(metadata !5)`
on its own, indicating the metadata IS being consumed by ScopedAA.

## Why SROA still doesn't fire

Look at the calls:
```llvm
%3 = call ptr @sub_13ab04(ptr nonnull %state_buf, i64 81556, ptr null)
%6 = call ptr @sub_97fcac(ptr nonnull %state_buf, ...)
%7 = call ptr @sub_1485ec(ptr nonnull %state_buf, ...)
```

`%state_buf` is **passed by pointer** to 141+ non-inlined dep
functions. SROA's contract: promote an alloca to scalars when it
can model every read/write to the alloca's bytes. When the alloca
escapes via a function call, SROA can't model the callee's
read/write pattern without inlining — even if `noalias` says the
pointer doesn't alias anything else, the callee can still write
arbitrary fields.

The lift emits each `sub_*` as a separate function (only the entry
gets `alwaysinline`; deps are normal functions). After `opt -O2`,
the calls remain. State alloca survives because of escape-via-call.

## What B1.b would need

To actually evaporate the State alloca:

**Option 1: alwaysinline every dep.** Adds `alwaysinline` to all
141+ lifted fns. After inlining, SROA can see the whole
read/write pattern. **Cost:** layout.wasm IR text size grows
significantly before LLVM's DCE catches up. May or may not
shrink the final wasm. May also break the recursion-cutoff (some
deps call each other — `alwaysinline` recursion is a hard error).

**Option 2: a real C++ pass that synthesizes per-field IPA.** For
each call `@sub_X(%state_buf, ...)`, walk into `@sub_X` to compute
the set of `State` offsets it reads/writes. Emit synthetic
ModRef summaries that SROA can consume. **Cost:** substantial
LLVM pass infrastructure work.

**Option 3: just accept the State alloca and shrink it.** The
State struct is 1088 B because remill emits a full AArch64 reg
file (32 GPRs × 16 B + SIMD + flags). Most cbs only touch
X0-X8 + X29/X30/SP/PC — ~150 B worth. A trim-state pass could
shrink the alloca to actually-used fields. **Cost:** moderate;
needs a custom analysis that walks the typed `%struct.State`
GEPs and finds the live set.

## Recommendation for B1.b session

Start with **Option 3 (trim state)** — fastest path to a measurable
size win without changing the call structure. Run as a custom IR
pass: walk every GEP off `%state` in the post-link module, collect
the touched field set, rewrite the State alloca + GEPs to a
compact struct.

Expected impact: 1088 B alloca → ~150 B alloca; per-fn IR shrinks
proportionally; total layout.wasm probably another -20-30%.

After Option 3, **B2 (stack_buf in linear memory)** becomes the
next mechanical win — the only remaining `ptrtoint` in the wrapper
is the SP setup, and at that point the State alloca is small
enough that AA's conservatism doesn't cost much.

## Why D (per-fn sharding) is separate value

D is **orthogonal** to B1.b/B2. Even if every cb stays at the
current 294 KB, sharding gives:

- First-paint wins: 141 deps load in parallel rather than one
  bundled wasm.
- Cache hits across cbs: dep `__rust_alloc` ships once, all cbs
  import it.
- Foundation for sub-10 KB-per-cb (after B1.b + B2 land, the
  per-cb body shrinks enough that the dep closure dominates).

Doing D first would still need a re-bundle once B1.b lands, so
B1.b-then-D is the conservative order.

## Concrete next-session reading list

1. This document.
2. `scripts/HANDOFF_2026_05_18_M9_E2E_GREEN.md` — current state +
   acceptance gates.
3. `scripts/M10_RELIABILITY_AND_OPTIMIZATION_PLAN.md` — the plan B1.b
   pre-dates this diagnostic; its "real LLVM pass route" sketch
   still applies but should pivot to Option 3 above first.

## Files to inspect for B1.b work

- `dll/src/web/transpiler_remill.rs::emit_helper_ir` — current
  State alloca def.
- `dll/src/web/transpiler_remill.rs::tag_state_accesses` — B1.a's
  text-substitution.
- `dll/src/web/cpp/azul_remill.cpp::compile_inner` — the only place
  where the merged module is in-memory pre-codegen. Custom
  passes hook here.
- `dll/src/web/transpiler_remill.rs::inject_alwaysinline` — model
  for Option 1's "alwaysinline everywhere" extension.

## Why the 17% wasn't the metadata's fault

The actual gain from B1.a came from:
- Inlined `__remill_*memory_*` getting tagged correctly, letting
  the in-helper-body `inttoptr` loads CSE/DSE across each other
  (intra-helper-fn AA wins).
- A1's libsystem-as-Leaf catching extra dead-code paths the deps
  used to reach.

The State alloca itself was never going to evaporate without
inlining or a per-field IPA — that's the structural finding from
the call-graph inspection.
