# How we ship the lifted wasm — design options

**Date:** 2026-05-18
**Status:** future work; record-only after the WASM-resident DOM
cycle (lift → layout → hit-test → JS-apply-patches) is fully
green on full hello-world.c.

The per-page data mirror (commit `732960155`) brought mini.wasm
from 27 MiB to 13.5 KiB by mirroring ONLY the 4 KiB pages each
wasm's lifted code actually reads via ARM64 `adrp`. The current
on-wire layout for hello-world (one cb + one layout cb):

```
mini.wasm           13.5 KiB   (AzStartup_* + 1 mirrored page)
on_click cb.wasm    18.0 KiB   (cb body + 14 deps + 3 pages)
layout.wasm        339.0 KiB   (layout body + 146 deps + 14 pages)
                  ─────────
                  ~370 KiB total
```

That works for a one-cb demo. The architectural question this
document records — for after the full cycle is green — is how to
share lifted libazul functions across multiple callbacks without
duplicating their code in every cb wasm.

## What's duplicated today

Each cb's transitive lift walks every libazul function it
reaches and copies the lifted bytes into THAT cb's wasm.
`object_cache` dedupes the COMPILE step (so the same `.o` isn't
re-compiled for two cbs), but the resulting `.o` is linked into
EACH cb wasm's final binary.

For an app with 50 buttons, all calling
`AzDom_addChild` → `AzDom_addChild`'s lifted bytes appear 50
times across 50 cb wasms.

Hello-world doesn't show this because its single cb's deps don't
overlap with the (separate) layout cb's deps. A multi-cb demo
would.

## Options

### A. Per-fn shared wasm (`/az/api/AzDom_addChild.wasm`)

Each api.json function gets its own wasm export. JS imports them
on demand at cb instantiation time.

**Pro**
  - True per-fn deduplication. Each api.json fn shipped once
    across the whole app.
  - Lazy loading: a cb that doesn't call `AzDom_addChild` doesn't
    fetch its wasm.

**Con**
  - azul.json has ~3000 fns. Even if most apps use only ~50,
    each fn = one HTTP fetch + one WebAssembly.instantiate call
    + one entry in `azFnAddrToTableIdx`.
  - Per-fn wasm overhead (custom sections, type tables) becomes a
    measurable fraction when each fn is small.
  - Browsers can multiplex ~100 HTTP/2 streams comfortably; 50
    fetches in a flat batch is fine, but waterfalls (cb → loads
    `AzDom_addChild` → which needs `AzDom_finalize_non_leaf` → ...)
    serialize on the wire.

**User's framing** (2026-05-18 chat): "for example a regular app
only needs to fetch + wasm-instantiate a couple of them, not all
3000". Correct — the per-fn approach lazy-loads. The cost is the
proliferation of small wasms.

### B. One shared `libazul-runtime.wasm`

mini.wasm extended to carry every api.json fn that ANY cb in the
app reaches (computed at server startup by walking the union of
all cbs' transitive deps). Per-cb wasms become tiny — they
contain only the user's cb body + helper-IR stubs + `extern
"libazul" "<sub_synth>"` for each dep.

**Pro**
  - Per-cb wasms are MINIMAL (just user code).
  - One shared HTTP fetch covers every libazul reference.
  - JS instantiation: instantiate `libazul-runtime.wasm` once,
    pass its exports as imports for every cb.

**Con**
  - Two-pass build: lift all cbs first to discover the dep union,
    then build `libazul-runtime.wasm` with that union as exports,
    then re-link every cb wasm with the import declarations.
  - `libazul-runtime.wasm` ends up being the upper bound of what
    any cb might need — for hello-world's 146-fn layout cb deps,
    it'd be ~300 KiB. Still a single download.
  - First-paint latency: nothing runs until the big shared wasm
    arrives.

### C. Stay as-is (per-cb transitive lift + per-page mirror)

What we have now.

**Pro**
  - Zero further changes needed; demonstrably works.
  - Each cb wasm is self-contained (no dependency on a shared
    runtime wasm being instantiated first).
  - Per-cb wasms can be cached independently by the browser.

**Con**
  - Duplication across cbs that share deps.
  - For multi-cb apps with deep dep overlap (e.g., everything
    calls `AzDom_addChild`), the on-wire bytes can balloon
    linearly with cb count.

### D. Hybrid: per-fn for high-fan-in api.json fns, transitive for rest

Identify which libazul fns appear in MANY cbs' transitive
closures. For those, ship as standalone wasms (Option A
style); for one-off / low-fan-in deps, keep transitive lift
(Option C). Threshold heuristic-tunable.

**Pro**
  - Best of both worlds — common deps dedup, rare deps stay
    inline.

**Con**
  - More moving parts (two shipping mechanisms in one app).
  - Threshold heuristic needs tuning; introduces another
    surprise vector.

## Recommendation

Defer the decision until after the full lift → layout → hit-test
→ JS-apply-patches cycle is green on **full hello-world.c**
(currently traps deeper in libazul on layout cb due to one or
two unmirrored data sections). Once a real workload runs
end-to-end:

1. Build a small multi-cb test app (3 buttons, each calling
   a different subset of `AzDom_*`) and MEASURE the actual
   duplication vs. shared baseline.
2. Pick A or B (or D) based on whether the typical app fits
   the "many small cbs reuse a fixed core" pattern (favor B)
   or the "diverse cbs with sparse overlap" pattern (favor A).

Either pivot is a localized refactor of `transpiler_remill.rs`'s
link path. The wrapper / dispatch / patch architecture stays
the same.

## What to keep regardless of choice

  - `Pcs::HiddenPtrReturn` + the layout-cb wrapper synthesis
    (M9-1) — still needed for any cb returning a `>16B` aggregate.
  - The synthetic-address lift scheme (M9-after-review,
    `23d7174d5`) — eliminates the post-ASLR address-space
    pollution regardless of how lifted code gets bundled.
  - The per-page mirror (`732960155`) — even with shared
    libazul.wasm, each wasm still benefits from mirroring only
    the pages its code reaches.
  - The post-link stack relocator — cross-module stack collisions
    are independent of the dedup scheme.
