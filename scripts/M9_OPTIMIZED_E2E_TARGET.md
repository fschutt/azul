# Optimized e2e target — sub-10 KB wasm cb

**Status:** future work; after the "working e2e" for full
hello-world.c lands.

## Current size landscape (2026-05-18)

| Cb shape                     | HTML  | mini  | cb    | layout  | TOTAL    |
|------------------------------|-------|-------|-------|---------|----------|
| v5 (body + on_click)         | 22 KB | 14 KB | 16 KB | 30 KB   | **80 KB** |
| full hello-world.c           | 23 KB | 14 KB | 18 KB | 388 KB  | **433 KB** |

The big number is the layout cb when it transitively lifts deep
libazul + (mis-classified) libsystem deps. The on_click cb stays
small because its dep closure is narrow.

## Where the bytes go

Per-cb wasm = wrapper IR (~500 B) + lifted body + per-call State
spill/fill traffic. The State struct (1088 B) survives SROA today
because `inttoptr` of register-derived values forces LLVM's alias
analysis to assume guest memory might alias host State. That's
**structurally** unsolvable at the IR-text level — it needs IR
graph rewriting.

## The architectural lever: address-space partitioning

Move every guest-memory access onto `ptr addrspace(1)` and keep
the State alloca in `addrspace(0)`. Different address spaces in
LLVM are defined to never alias, so SROA can fully promote the
State alloca to scalar registers — the State struct **evaporates**.
`llc -mtriple=wasm32` lowers `addrspace(1)` to plain linear-memory
ops, no overhead.

mcsema / AnvILL already do this. It's the load-bearing trick.

### Concrete pass shape

The current intrinsic-lowering step (`__remill_read_memory_N` →
`load iN, ptr %addr`) becomes:

```llvm
; before
%addr = load i64, ptr %X1
%val  = call i32 @__remill_read_memory_32(ptr %memory, i64 %addr)

; after
%addr = load i64, ptr %X1
%p    = inttoptr i64 %addr to ptr addrspace(1)
%val  = load i32, ptr addrspace(1) %p
```

The wrapper passes guest memory as `ptr addrspace(1) %guest_mem`
parameter. Guest accesses become `getelementptr addrspace(1)`
off it — no `inttoptr` laundering.

After SROA evaporates State, a cb that touches no guest memory
shrinks to ~100 bytes (just the wrapper + arg moves). A cb that
mutates a hashmap has memory traffic proportional to **what the
program does**, not lift overhead.

### Cost

This requires real IR-graph rewriting via `llvm-sys` walking the
module (or a C++ LLVM pass à la `remill-opt`). String substitution
on the .ll is no longer sufficient — provenance tracking ("which
SSA values are guest addresses?") needs the typed graph.

This is the point where WB1.2 commits to a real pass
infrastructure. Document the dependency explicitly.

## Targets to hit

| Phase    | Per-cb size  | What changes |
|----------|--------------|--------------|
| Today    | 16-30 KB     | State alloca survives + 32 KB stack_buf |
| Address-space partition | <5 KB | SROA evaporates State; guest accesses in addrspace(1); stack_buf becomes proportional to actual depth |
| + dep dedup (Option A/B from WASM_SHIPPING_OPTIONS.md) | shared baseline + ~1 KB per cb | api.json fns shipped once across the whole app |

The "<10 KB per cb" claim is plausible only with both wins.

## Per-cb wasm shipping (sized for the optimized world)

Once each cb is ~1-5 KB and api.json fns are dedup'd:

  - One wasm per C function callback (`/az/cb/<fn_name>.wasm`).
  - Dependency manifest emitted alongside (JSON):
    `{ "on_click": ["__rust_alloc", "AzDom_createBody", ...] }`
  - JS bootstrap fetches manifest → kicks off all `WebAssembly.compileStreaming`
    in parallel → instantiates in dep order.
  - Browser HTTP/2 multiplexes the per-cb fetches.

Server complexity goes up (must compute dep closure + emit
manifest); app dev / end user see only "wasm streams in parallel,
counter clicks instantly".

## Don't ship until

  - Working e2e for full hello-world.c (current blocker).
  - Mirror trace doesn't skip libsystem pages silently.
  - Per-fn wasm sizes are stable across reboots (cache + content
    hash invariants verified).
