# Making lifted OBJECTS cacheable across builds

Status: design note · Goal: cut relift wall-clock by removing `opt` + `llc`
from cache hits, not just `remill`.

## Why this is the remaining lever

Measured on one real function's linked IR (10.6 MB, the heavy tail that
dominates total time):

| stage | -O2 | -O0 |
|---|---|---|
| `opt`  | 22.8 s | 1.8 s |
| `llc`  | 88.0 s | 58.0 s |
| object size | 762 KB | 4.3 MB |

So `llc` is the wall at any optimization level, and dropping to `-O0`
trades 34% of it for 5.6× bigger objects (slower links, bigger wasm). The
relocation-canonical cache already removes `remill` from ~65% of functions;
those hits still pay opt+llc in full.

An on-disk object cache ALREADY exists (`obj_cache_path`), keyed on the
post-rewrite IR text. It never hits across builds for one reason: **the IR
text embeds layout-dependent values**, so the same function hashes
differently in every build.

That reframes the work. We do not need a new cache — we need the **IR to be
layout-independent**, and the existing cache starts hitting for free.

## What is layout-dependent in the IR today

1. **Symbol names.** Bodies are `@sub_<synth_hex>`, and synth addresses move
   whenever the guest image layout moves.
2. **Numeric slots.** Jump-table targets, PC constants and data addresses
   appear as literal `i64` constants (this is exactly the set the v6
   template already identifies and translates).

Everything else in the IR is already position-independent.

## The two changes

### Stage 1 — identity-based symbol names

Name each lifted body by a **stable identity** instead of its address. The
v6 canonical key is already exactly that identity: the relocation-masked
byte hash plus the ordered site identities, which is invariant across
builds by construction. So a body becomes `@fn_<canonical_key>` and every
reference to it resolves by name at link time.

Affected: the post-lift rename pass (already rewrites every `sub_` token),
the wrapper's callee name, and the indirect-call dispatcher — the
dispatcher keeps mapping *runtime PC values* (build-specific, and it is
regenerated per build anyway) to those stable names.

### Stage 2 — numeric slots become linker-resolved loads

Replace each template slot constant with a load from an external global:

```llvm
@__azslot_<key>_<i> = external global i64
...
%s = load i64, ptr @__azslot_<key>_<i>
```

The object then carries a `R_WASM_MEMORY_ADDR_SLEB` relocation (verified:
this is what llc emits for a symbol-derived value, in a padded LEB the
linker patches) instead of a baked constant, and the IR text no longer
mentions the address at all.

Per build we emit ONE small values object defining every slot global with
the value the v6 identity resolution produces — a single extra llc run over
a module that is just globals.

Note `wasm-ld` in this toolchain has no `--defsym`, so the value must live
in a defined global rather than a link-time symbol assignment; the load is
the price. It costs one memory read per slot *site execution* and blocks
constant folding at those sites, so it is gated: fast-iteration builds turn
it on, release builds keep baked constants.

## Expected effect

A rebuild that changes only host-side code (the common case while
debugging the transpiler) leaves every guest function's identity unchanged,
so both remill AND opt+llc drop out for ~all functions and a relift becomes
link-bound — minutes instead of ~an hour. A guest source change invalidates
only the functions whose bytes actually changed.

## Sequencing

Stage 1 is a prerequisite (objects cannot be reused while their symbol
names move), and it alone helps every function whose IR has no numeric
slots. Stage 2 completes it. Both land behind a flag first, validated on
the hello-world gate — which has a full pass/fail signal in ~15 minutes —
before AzWriter uses them.
