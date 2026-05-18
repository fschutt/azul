# M10 — Workstream D: per-fn sharding + mini.wasm split

> **SUPERSEDED 2026-05-19** — Steps 1+2+4+5+6 landed in `f30d0ec02`.
> See [`STATUS_REPORT_M10_2026_05_19.md`](STATUS_REPORT_M10_2026_05_19.md)
> for measured before/after sizes and the rationale for why
> Step 3 (mini.wasm split) was deferred.
>
> Highlights:
> - All four legacy gates remain GREEN under default behavior.
> - Three new sharded gates (`full-cycle-sharded.js`,
>   `cross-cb-dedup.js`, `bundle-size-comparison.js`) pass on both
>   `hello-world-v5.bin` and `hello-world.bin`.
> - Boundary lift produces 12 shards for `hello-world.bin`,
>   3 for `hello-world-v5.bin`.
> - Net wire-byte savings require multi-cb dedup; single-cb shapes
>   see overhead from per-shard helper IR. Follow-up workstream M10-E
>   (shared helper-IR runtime wasm) would close this gap.

**Status:** PLANNED. Next-session work.
**Prereqs:** M10 A1+B1.a+C1 landed; export-fix landed
(commit `486c9742c`); current layout.wasm = 184 KB,
on_click.wasm = 15.5 KB, mini.wasm = 13 KB. All four gates GREEN.

## Goal

Convert from "one fat wasm per callback" to "one tiny wasm per Az*
boundary function". Every `Az*` framework symbol becomes its own
`/az/fn/<name>.<hash>.wasm` — shipped once per page regardless of
how many callbacks need it. Same architecture extended to
mini.wasm: split the 24 `AzStartup_*` symbols by feature so a
static page only ships ~3 KB of bootstrap.

Target after D + mini-split (per-cycle wire bytes for a v5-shape
page, first paint, deduplicated):
```
mini-core.wasm                ~3 KB   alloc + init + hydrate + bump-helpers
mini-events.wasm              ~3 KB   register + hitTest + dispatchEvent
AzRefAny_clone.wasm           ~0.5 KB
MyDataModel_downcastMut.wasm  ~1 KB
on_click.wasm                 ~0.5 KB ← just YOUR code's body + wrapper
TOTAL                         ~8 KB    (vs current ~30 KB)
```

Bigger cbs (e.g. the full hello-world.c layout with 141 deps)
shrink even more proportionally because most of their deps are
shared `Az*` framework code that ships once and is imported by
every cb that needs it.

## Why now

The export-fix (`486c9742c`) cleared the trivial bloat. What's
left in each cb's wasm is the lifted-body code for transitive
deps. Those deps are mostly `Az*` framework symbols already
classified `Recursable` in `symbol_table::classify_for_name`. They
all show up in `api.json`. The boundary set is FREE — no source
annotations needed.

Implementation is mechanical:
1. New `FnClass::BoundaryImport` variant.
2. Classifier emits `BoundaryImport` for every `Az*` Framework
   symbol (instead of `Recursable`).
3. Lift BFS stops at BoundaryImports (emits a `declare` instead
   of pulling in the body).
4. Separate boundary-lift pass: for each unique `BoundaryImport`
   referenced by any cb in any route, lift it once into its own
   `.wasm`. Recursively walks that fn's own deps (boundary-import
   stopping rule applies again).
5. Server emits a manifest JSON keyed by shard name with
   `{url, exports, imports}`.
6. loader.js parses manifest, fetches all needed shards in
   parallel, instantiates in topological order, wires imports.

## Why mini.wasm split goes in the same workstream

The split mechanism is identical: it's a build-time partition of a
symbol set with declared inter-shard imports. The only difference
is the source list (`EVENTLOOP_SYMBOLS` instead of `api.json
Framework`). Wire the same manifest infrastructure once; serve
both kinds of shards through it.

Reusing one mechanism keeps the loader.js logic simple — it
doesn't need to distinguish "framework" vs "mini" shards; it just
does dep-graph fetch + instantiate.

## Concrete plan

### Step 1 — classifier + lift changes (Rust, ~1 day)

`dll/src/web/symbol_table.rs`:

```rust
pub enum FnClass {
    Recursable,
    /// M10-D: `Az*` framework symbol that ships as its own wasm
    /// shard. Lift treats this like Leaf inside cb lifts (emits
    /// `declare` only), and a separate boundary-lift pass produces
    /// the body wasm.
    BoundaryImport,
    BumpAlloc,
    // ... existing variants ...
}
```

`classify_for_name`:

```rust
if let Some(api_class) = api.get(name) {
    return match api_class {
        ApiFnClass::Framework => FnClass::BoundaryImport,  // was Recursable
        ApiFnClass::ServerEntryPoint => FnClass::NeverLift,
        ApiFnClass::ReplaceWithDomPatcher => FnClass::Leaf,
    };
}
```

A1's post-classification override (for libsystem) still runs —
nothing changes there.

`dll/src/web/transpiler_remill.rs::lift_with_transitive_deps_batched`
BFS:

```rust
for dep_addr in bl_targets {
    let Some(entry) = table.resolve(dep_addr) else { continue };
    // M10-D: stop at boundary imports — they ship as separate wasms.
    if matches!(entry.classification, FnClass::BoundaryImport) {
        // Record the boundary the cb depends on; helper IR emits
        // `declare ptr @sub_<addr>` (existing behavior for
        // un-lifted externs).
        used_boundaries.insert(entry.canonical_addr);
        continue;
    }
    if !entry.classification.is_recursable() { continue; }
    // ... existing recurse logic ...
}
```

`emit_helper_ir` `BranchExternKind` already handles a "no body"
case for `Recursable` (declare emitted by lifted IR, body resolved
at link from sibling .o). Add a similar no-body case for
`BoundaryImport` — but with `import_module="env"` /
`import_name="<canonical_name>"` annotations so wasm-ld emits a
wasm function-import declaration instead of a missing-symbol
error.

```llvm
declare ptr @sub_<addr>(ptr noalias, i64, ptr noalias)
  "wasm-import-module"="env"
  "wasm-import-name"="<canonical_name>_lifted"
```

(The `_lifted` suffix distinguishes from the per-shard wrapper
export, which uses the plain `<canonical_name>`. The shard wasm's
internal lifted body is what the cb's `sub_<addr>` actually
needs.)

### Step 2 — boundary-lift pass (Rust, ~1 day)

After per-route cb lift completes (current
`lift_with_transitive_deps_batched`), do a SECOND pass:

```rust
// Union of all BoundaryImports across all cbs / all routes.
let mut all_boundaries: HashSet<usize> = HashSet::new();
for route in &routes {
    for cb in &route.cbs {
        all_boundaries.extend(cb.used_boundaries.iter());
    }
}

// For each boundary, lift its body into its own wasm (separate
// from cbs). Stop at OTHER BoundaryImports (so AzString_copy
// can import AzString_alloc instead of bundling it).
for &boundary_addr in &all_boundaries {
    let wasm_bytes = lift_boundary_to_wasm(boundary_addr)?;
    boundary_shards.push(BoundaryShard {
        name: lookup_canonical_name(boundary_addr),
        bytes: wasm_bytes,
        deps: scan_boundary_deps(boundary_addr),
    });
}
```

Each `lift_boundary_to_wasm` runs the same per-fn pipeline as a
single cb — emit_helper_ir generates a wrapper, link to .wasm,
relocate stack, mirror data pages.

Boundary shard exports: `<canonical_name>` (the wrapper) + the
internal `sub_<addr>` (so cbs that import via
`<canonical_name>_lifted` can resolve to the body, NOT through the
wrapper's State-alloca overhead). Actually... simpler: cb's
import resolves directly to the boundary's wrapper, which calls
its own internal sub_<addr>. One extra wrapper-overhead per call
but vastly simpler import wiring. Profile later.

### Step 3 — mini.wasm split (Rust, ~0.5 day)

`dll/src/web/mod.rs`:

```rust
pub const MINI_SHARDS: &[(&str, &[&str])] = &[
    ("core", &[
        "AzStartup_alloc",
        "AzStartup_free",
        "AzStartup_init",
        "AzStartup_hydrate",
        "AzStartup_snapshotBumpHeap",  // M10-C1
        "AzStartup_resetBumpHeap",     // M10-C1
        "AzStartup_registerStateDeserializer",
    ]),
    ("events", &[
        "AzStartup_registerCbNode",
        "AzStartup_hitTest",
        "AzStartup_dispatchEvent",
    ]),
    ("layout", &[
        "AzStartup_buildLayoutInfo",
        "AzStartup_setLayoutCbTableIdx",
        "AzStartup_setRefAny",
        "AzStartup_initLayoutCache",
        "AzStartup_getCurrentDomPtr",
        "AzStartup_getLastLayoutStatus",
        "AzStartup_setModelPtr",
        "AzStartup_setDisplayNode",
    ]),
    ("patches", &[
        "AzStartup_buildCounterPatch",
    ]),
];
```

`lift_and_link_eventloop` becomes `lift_and_link_eventloop_shards`
that does ONE call to `link_objects_to_wasm` per shard. Each
shard's .o objects come from the existing per-symbol lift.

Cross-shard deps: a `mini-events` symbol like `dispatchEvent` may
call into `mini-core`'s `AzStartup_alloc`. Wire as wasm imports:
the events shard declares `(import "env" "AzStartup_alloc" ...)`,
loader.js wires `coreInstance.exports.AzStartup_alloc`.

Per-route shard usage: scan each route's cbs for which mini
features they use. Static page (no cbs) → just `core`.
Interactive → `core + events`. With layout cb → also `layout`.
With counter patches → also `patches`.

### Step 4 — manifest emission (Rust, ~0.5 day)

`dll/src/web/server.rs` adds a manifest builder that walks every
shard (mini + boundary + cb) and emits:

```json
{
  "version": 1,
  "shards": {
    "core": {
      "url": "/az/mini/core.abc123.wasm",
      "exports": ["AzStartup_alloc", "AzStartup_free", ...],
      "imports": []
    },
    "events": {
      "url": "/az/mini/events.def456.wasm",
      "exports": ["AzStartup_dispatchEvent", ...],
      "imports": ["core"]
    },
    "AzRefAny_clone": {
      "url": "/az/fn/AzRefAny_clone.789.wasm",
      "exports": ["AzRefAny_clone"],
      "imports": ["core"]
    },
    "on_click_<hash>": {
      "url": "/az/cb/on_click.xyz.wasm",
      "exports": ["callback"],
      "imports": ["core", "events", "AzRefAny_clone", ...]
    }
  },
  "routes": {
    "/": {
      "cbs": ["on_click_<hash>", "layout_<hash>"],
      "hydrate": { "type_id": "12345", "data_ptr": 0, ... }
    }
  }
}
```

Imports are by shard name (loader.js looks them up in the manifest
to find URLs). Order matters for instantiation but the loader
does topological sort.

Serve at `/az/manifest.<hash>.json`. The bootstrap HTML embeds
`<script>window.AZ_MANIFEST_URL = "..."</script>` so loader.js
finds it.

### Step 5 — loader.js dep-graph fetch + topo-instantiate (~0.5 day)

`dll/src/web/loader_js.rs` rewrite the bootstrap section:

```javascript
async function bootAzul() {
  const manifest = await fetch(window.AZ_MANIFEST_URL).then(r => r.json());
  const route = manifest.routes[location.pathname] || manifest.routes['/'];

  // Collect transitive shard set for this route.
  const needed = new Set();
  function collect(name) {
    if (needed.has(name)) return;
    needed.add(name);
    for (const dep of manifest.shards[name].imports) collect(dep);
  }
  for (const cb of route.cbs) collect(cb);

  // Parallel fetch all needed shard bytes.
  const compiled = {};
  await Promise.all([...needed].map(async name => {
    const bytes = await fetch(manifest.shards[name].url).then(r => r.arrayBuffer());
    compiled[name] = await WebAssembly.compile(bytes);
  }));

  // Topo-sort + sequential instantiate (imports must be ready).
  const sorted = topoSort([...needed], n => manifest.shards[n].imports);
  const memory = new WebAssembly.Memory({ initial: 2048, maximum: 16384 });
  const table = new WebAssembly.Table({ initial: 256, element: 'anyfunc' });
  const instances = {};
  for (const name of sorted) {
    const shard = manifest.shards[name];
    const imports = { env: { memory, __indirect_function_table: table,
                             memset: ..., memcpy: ..., memmove: ...,
                             __az_resolve_callback: ..., } };
    for (const depName of shard.imports) {
      Object.assign(imports.env, instances[depName].exports);
    }
    instances[name] = await WebAssembly.instantiate(compiled[name], imports);
  }

  // Wire JS API surface from the entry shards.
  window.AZ = {
    mini: instances['core'].exports,
    // ...
  };

  // Run the hydrate + initial layout sequence (as today's
  // loader.js does after instantiating mini + cb).
  // ...
}
bootAzul();
```

The cross-shard imports use a shared `env`; each shard sees only
the exports of its declared dep shards. Since wasm imports are
resolved by name, exporting under the same name from a dep shard
makes the wiring transparent.

### Step 6 — acceptance gates (~0.5 day)

New gate scripts:

**`scripts/m9_e2e/full-cycle-sharded.js`** — mirrors full-cycle.js
but uses the manifest + multi-wasm load instead of the
hardcoded `/az/mini.<hash>.wasm` + `/az/cb/<name>.<hash>.wasm`
URLs. Runs the same bootstrap → layout → click → cb → patch
sequence.

**`scripts/m9_e2e/bundle-size-comparison.js`** — fetches both
the legacy bundled URLs (if still emitted) and the manifest's
sharded URLs, sums the bytes, asserts sharded ≤ bundled. Logs
the savings.

**`scripts/m9_e2e/cross-cb-dedup.js`** — confirms that the
manifest lists each `Az*` shard exactly once even when
multiple cbs reference it.

Existing gates (full-cycle / click-only / bump-reset) keep
running against the bundled mode (gated by an env knob like
`AZ_BUNDLED_LEGACY=1`) so the migration is reversible.

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| Shard fetch fan-out exceeds browser parallel-connection limit | Use HTTP/2 server (multiplexed). For HTTP/1.1 deploy, cap concurrency to 6 and pipeline. |
| Manifest grows huge with many cbs / shards | Compress + cache via hashed URLs; deltas via etag for revisits. |
| Cross-shard call overhead | Each shard call is a wasm-to-wasm indirect via imported function. Profile vs bundled — if hot, fold high-frequency boundaries back into the cb (per-cb override list). |
| `Az*` boundaries that take addrspace pointers across shards | All shards share linear memory (one `WebAssembly.Memory`); pointers are just i32 offsets, no marshaling. |
| Recursive Az* (Az_foo calls Az_bar which calls Az_foo) | The boundary lift's BFS would loop. Detect via visited-set; emit warning and inline one of the boundaries into the other shard. |
| Caching busting on every code change | Hash each shard independently; only changed shards re-download. Big win for dev iteration. |

## Acceptance criteria (D + mini-split combined)

1. New `full-cycle-sharded.js` gate PASSES on `hello-world-v5.bin`.
2. New `full-cycle-sharded.js` gate PASSES on `hello-world.bin`.
3. Total wire bytes for sharded mode ≤ 50% of bundled mode for
   `hello-world.bin` (measured via `bundle-size-comparison.js`).
4. Shard dedup: layout cb + on_click cb reference the same
   `AzRefAny_clone` shard URL (verified via `cross-cb-dedup.js`).
5. mini-core.wasm < 5 KB, mini-events.wasm < 4 KB, mini-layout.wasm
   < 5 KB (independently verifiable via `wasm-objdump`).
6. All four existing gates still GREEN in bundled mode (set
   `AZ_BUNDLED_LEGACY=1`).

## Estimated effort

**3-4 days** total:
- Day 1: classifier + lift changes (Step 1)
- Day 2: boundary-lift pass (Step 2)
- Day 3 AM: mini split (Step 3)
- Day 3 PM: manifest + loader.js (Steps 4-5)
- Day 4: acceptance gates + bundled-mode kept as fallback (Step 6)

## What this UNLOCKS

After D + mini-split lands, the per-cb wasm size is dominated by
the user's own code (the `sub_<entry>` body) plus a thin wrapper.
That's the right shape for **B1.b Option 3 (trim-state pass)** to
deliver real wins — the trim-state pass shrinks every lifted
body proportionally to its actually-touched State fields. Combined,
the end-state per-cb body is ~100-200 B of actual work.

That's the "couple bytes of wasm instruction" target.
