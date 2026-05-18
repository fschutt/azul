# M10 — Reliability + size optimization plan

**Goal:** make the lift pipeline reliable for full hello-world.c
(currently green only for `v5` shape — body + addCallback), shrink
each cb wasm meaningfully, and add memory reclamation so
long-running pages don't grow the bump heap unbounded.

Three workstreams. They're decoupled but land in the same release
(if any one slips, the others can ship without it).

---

## Workstream A — Make every cb shape work (reliability)

The 5-step e2e cycle works for v5. v4 (createText + delete + body)
returns rc=0 but zero data. Investigation (commits `c29508355`,
`39536136a`) narrowed:

- 12 libsystem/dyld pages get skipped by the per-page mirror
  (`AZ_WASM_MIRROR_TRACE=1` confirms). They're not in any tracked
  image; the lifted code that reads through them gets zeros.
- `sub_1866bc250` resolves to `_platform_memmove` via dladdr but
  classifies as `Recursable` (= "lift as a full Rust fn"). It
  gets lifted with its native synth_addr (no rebase) and produces
  junk.
- A naive "default Leaf if not Az-prefixed" classifier fix broke
  on_click — Rust-monomorphized bare names share the bucket and
  some ARE Recursable for real.

### Approach

**A1. Address-based classifier override** (target: 1 day)

Add an addr→class override pass: if a symbol's resolved address
is OUTSIDE the user binary AND outside libazul, force Leaf.
libsystem/dyld/anything-else can't lift cleanly through remill
because we don't track its image rebase, so the synth_addr stays
native → adrp pages stay unmirrored.

Implementation in `symbol_table.rs::assign_synthetic_addresses`:
after rebasing the tracked images, walk every entry whose
`canonical_addr` falls outside any `image_rebases` range and
set `classification = FnClass::Leaf`. Run BEFORE the table goes
live.

Test gate: full hello-world.c layout cb returns non-zero data
in `scripts/m9_e2e/full-cycle.js`. on_click counter regression
still passes.

**A2. Mirror libsystem opportunistically** (if A1 isn't enough)

Some lifted libsystem fns might be genuinely needed (rare). For
each `Leaf` resolution we'd previously have lifted, log the
symbol name. If the same symbol shows up across multiple cbs,
consider:
  - Implementing it manually in the helper IR (like BumpAlloc).
  - OR mirroring the library's __TEXT page so the lift can
    proceed (slow path — adds ~64 KiB per libsystem lib).

Most libsystem fns are sub-page leaves (memcpy, memmove); Leaf
classification is correct for them.

### Out of scope

- Address-space partitioning at addrspace(1) — wasm32 lowering
  doesn't support it for linear memory ("Encountered an
  unlowerable store to the wasm_var address space"). The wasm-
  native equivalent is alias-scope metadata; that's Workstream B.

---

## Workstream B — Shrink per-cb wasm (size + State evaporation)

Current sizes (full hello-world.c):

| Module       | Bytes  |
|--------------|--------|
| mini.wasm    | 13.6 KB |
| on_click.wasm| 18.4 KB |
| layout.wasm  | 388 KB  |
| **total**    | **433 KB** |

Layout cb is the elephant. The State alloca (1 KB) survives SROA
because `inttoptr i64 %addr to ptr` of register-derived guest
addresses forces LLVM AA to assume guest pointers might alias
the State alloca.

### Approach

**B1. Alias-scope metadata on every guest memory op** (target: 2 days)

Add LLVM metadata so AA can prove guest-memory ops don't alias
the State alloca:

```llvm
!host_domain = !{!host_domain}
!host_scope  = !{!"host", !host_domain}
!guest_scope = !{!"guest", !host_domain}

define i32 @__remill_read_memory_32(ptr %memory, i64 %addr) {
  %p = inttoptr i64 %addr to ptr
  %v = load i32, ptr %p, align 4,
       !alias.scope ![[guest_scope_ref]],
       !noalias    ![[host_scope_ref]]
  ret i32 %v
}
```

State accesses don't need metadata — they're tracked through the
alloca anyway. But for the noalias metadata to fire, the State
ops need to be tagged with the matching domain. Two options:

  - **B1.a (text-substitution route)**: append metadata to every
    `load`/`store` in the helper IR's __remill_*memory_* bodies.
    Also append `!alias.scope !host_scope` to every State load/
    store the lift emits — would require pattern-matching `ptr
    %X<N>` references in the lifted IR's text. Workable but
    fragile.

  - **B1.b (real IR pass route)**: stand up an `llvm-sys` /
    `inkwell` pass that walks the merged module after llvm-link
    and tags loads/stores by aliasing class (State alloca →
    host, everything else with `inttoptr` provenance → guest).
    Robust but moves WB1.2 into "real LLVM pass infrastructure"
    territory. **The doc edit promised in M9_OPTIMIZED_E2E_TARGET.md.**

Start with B1.a, see the size win, escalate to B1.b only if
text substitution misses too many ops.

**Expected outcome**: State alloca evaporates, opt -Oz further
shrinks bodies. Layout cb 388 KB → estimated 50-80 KB.

**B2. Stack buffer in linear memory** (target: 1 day, after B1)

Currently `%stack_buf = alloca [32768 x i8]` lives on the host
(wasm) stack — its `ptrtoint` for SP setup is the second
SROA-defeating ptrtoint. Move to a pre-allocated wasm linear-
memory region (mini.wasm exports a `getStackBuf` that hands out
unique 32 KB slots — like the current SP relocator).

After this: ALL ptrtoint in the wrapper is through wasm-memory
offsets that AA already knows are guest memory. State alloca
becomes pure SSA.

**Expected outcome**: cb wrapper ~200 B (down from ~500 B).

---

## Workstream C — Bump allocator reclamation

The wasm bump allocator (`@__az_bump_ptr`, helper IR) starts at
offset 96 MiB and grows linearly. Every cb invocation that
allocates (createText, addChild, etc.) bumps the pointer; nothing
ever returns memory.

A long-running page (100 layout cycles) leaks ~megabytes. Same
shape as native rust_alloc without rust_dealloc.

### How the native code already handles it

There are `__rust_dealloc` markers in libazul — points where
azul-internal code explicitly returns blocks. The native pipeline
honors them via the system allocator (jemalloc / libsystem
malloc). Lift could mirror this.

### Approach

**C1. Per-cycle bump reset** (target: half a day)

Snapshot `@__az_bump_ptr` at the START of every layout/dispatch
cycle. Reset it back at the END. All allocations during the
cycle are tracked into transient memory; nothing persists across
cycles.

Mechanism:
  - Add `AzStartup_resetBumpHeap(state, snapshot)` to eventloop —
    writes `snapshot` back to `@__az_bump_ptr`.
  - Add `AzStartup_snapshotBumpHeap(state) -> u32` — returns
    current `@__az_bump_ptr`.
  - `AzStartup_dispatchEvent` + `AzStartup_initLayoutCache`
    snapshot at entry, reset at exit (after copying the returned
    AzDom / patch out to the caller-provided buffer).

**Pro:**
  - Simple. Zero per-alloc overhead.
  - Matches the "arena per request" pattern used in azul natively.

**Con:**
  - Long-lived allocations within a cycle die at cycle end. If
    a cb returns a Dom containing pointers to bump-allocated
    children, those pointers are dangling after reset.

The Dom struct copied out by `AzDom_createBody` is a self-contained
struct (no internal pointers); the cb returns it BY VALUE through
out_ptr. So per-cycle reset is safe **for the cb's return value**.

If a cb pattern emerged that needs cross-cycle persistence (long-
lived refany state mutated through a chain of bumps), C1 would
need to escalate to C2.

**C2. Free-list bump allocator** (target: 2 days; deferred until
needed)

Replace the simple monotonic bump with a free-list-backed bump.
Implement `__rust_dealloc` body in the helper IR (instead of
classifying as Leaf). The free list lives in wasm memory; same
structure as Rust's small-bin allocator.

Wire to the dealloc markers in libazul — every `bl _free` or
`bl _rust_dealloc` lifts to a call to our wasm-side implementation.

**Pro:**
  - True per-alloc reclamation. Pages can run indefinitely.

**Con:**
  - Allocator overhead in the cb body (~50 instructions per dealloc).
  - More wasm code (the free-list impl ~ 200 LOC of IR).
  - Need to be careful about cross-cycle pointer validity.

### Memory.grow integration

wasm-ld declares an initial-memory + lets `memory.grow` extend
at runtime. Mini.wasm exports `memory` with growth allowed.

If reclamation isn't enough (some long-lived cb pattern actually
needs the heap), pages can `memory.grow(N)` and bump into the new
region. JS sees grow events and could optionally surface a
warning.

For the v1 release: 128 MiB initial + grow allowed is plenty for
any reasonable hello-world-tier app.

---

## Workstream D — Per-fn wasm sharding (post-reliability)

Once A + B land, ship deduplication via per-fn wasm files. Already
documented in `scripts/WASM_SHIPPING_OPTIONS.md`; this is the
implementation slot.

Key open question for D: with B's metadata-driven AA, are deps
small enough that per-fn shipping beats one bundled `layout.wasm`
on first-paint latency? Re-measure after B lands.

---

## Order of operations + acceptance gates

1. **A1** (libsystem classifier override)
   - Gate: `scripts/m9_e2e/full-cycle.js` passes for
     `hello-world.bin` (the full one, not v5).
   - Gate: `scripts/m9_e2e/click-only.js` still passes (no
     on_click regression).

2. **B1.a** (alias-scope metadata in helper IR)
   - Gate: layout.wasm size drops measurably (target: -50%).
   - Gate: A1 gates still pass.

3. **C1** (per-cycle bump reset)
   - Gate: 100 dispatch cycles in a row leave `@__az_bump_ptr`
     unchanged. Add to full-cycle.js as a loop.
   - Gate: A1 + B1 gates still pass.

4. **B2** (stack_buf in linear memory)
   - Gate: cb wrapper size drops to <300 B.
   - Gate: prior gates still pass.

5. **B1.b** (real LLVM pass, if B1.a wasn't enough)
   - Gate: cb body shrinks per dropped State accesses.

6. **D** (per-fn wasm sharding)
   - Gate: multi-cb demo (3 buttons different deps) — total wire
     bytes drop vs. bundled.

Each workstream is independently revertable. The acceptance gates
are kept under `scripts/m9_e2e/`.

---

## What we already know we won't try

- Addrspace(1) for linear memory — wasm32 reserves it for
  `wasm_var` globals; LLVM errors at lowering. Use alias-scope
  metadata instead.
- Per-state copies per call — would multiply alloca traffic 50×
  for deep call chains. Save/restore via SP-relative stores
  (what native code already does) is the right shape; the issue
  is the AA fence, not the model.
- Rewriting remill itself — out of scope; we treat remill output
  as the source of truth and reshape via post-passes.
