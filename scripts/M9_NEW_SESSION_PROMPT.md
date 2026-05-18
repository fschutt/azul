# M9 — Session-starter prompt

Copy-paste-able starter for the next session. Goal: land the
"WASM-resident DOM" architecture. JS becomes a thin event-and-
patch ferry; the WASM side owns layout, hit-test, dispatch, and
DOM queries.

---

Read `scripts/M9_WASM_DOM_HANDOFF.md` first, then
`scripts/STATUS_REPORT_2026_05_18.md` for the M8.9 close-out
state. Background context: `scripts/M8.9_REMILL_HANDOFF.md`
(now complete) describes the lift pipeline that M9 builds on
top of.

## The framing

M8.9 closed the lift pipeline. Every callback compiles to wasm,
the layout cb compiles to a 285 KB wasm, and CSS ships to the
browser correctly. What's missing is the runtime architecture
to call the layout cb from inside wasm and have a WASM-resident
DOM.

The user's vision: the server gives instant first paint; JS is
a thin ferry; the WASM mini.wasm holds the COMPLETE DOM (styled
dom + layout cache + font manager + hit-tester) and answers
every DOM query from inside wasm. The ONLY JS↔WASM bridge for
dispatch is `__az_resolve_callback(fn_addr) → table_idx`. User
callbacks like `AzCallback_getLayoutForNode(node_id)` read from
the wasm-resident layout cache via normal libazul functions
(which the transitive lifter picks up automatically).

## The 4 blockers Phase 1 closes

1. Wrapper synthesizes `Callback`-shape sig for ALL kinds.
   Layout-cb returns `AzStyledDom` (large aggregate via ARM64
   hidden X8 register) — the wrapper never seeds X8.
2. `LayoutCallbackInfo` carries refs to `&ImageCache`,
   `&FcFontCache`, etc. — not constructible from JS or current
   wasm. Need wasm-side stubs.
3. Returned StyledDom lands in stack-scratch buffer (freed on
   wrapper return). Must memcpy into bump heap.
4. Nothing calls the layout cb from inside wasm yet. Need
   `AzStartup_initLayoutCache` to invoke it + build the
   `LayoutWindow` cache.

## Architectural decisions — committed (don't relitigate)

(Per `scripts/M9_WASM_DOM_HANDOFF.md` § "Architectural decisions
— committed answers")

1. **StyledDom return**: CALLER-allocated destination buffer.
   Wrapper signature is `(refany_lo, refany_hi, info_ptr,
   out_styled_dom_ptr) → u32 status`. Wrapper writes State.X8
   from `out_styled_dom_ptr` before invoking the lifted body.
   `EventloopState.current_dom` IS the destination — no separate
   alloc, no leak. WHY destination buffers exist at all: ARM64
   AAPCS64 returns structs > 16 bytes via X8 (Indirect Result
   Location Register); WASM can return at most one scalar.
   Either way somebody allocates; caller-allocated makes
   ownership explicit.

2. **LayoutCallback cb-ptr → table-idx**: keep `cb` as fn-ptr,
   add a wasm-only `extern "C"` trampoline that calls
   `__az_call_indirect_layout4`. Minimal upstream change.

3. **Diff algorithm**: investigate `azul-core/src/diff.rs`
   FIRST. If `reconcile_dom` produces a `DiffResult` that maps
   cleanly to the TLV ops, use it. Otherwise write a small
   StyledDom-aware diff in `eventloop.rs`.

## Target: ship by evening 2026-05-18

**Critical path = phases 1-3** (layout cb runs in wasm,
populates the WASM DOM). Phases 4-6 remove the remaining JS
hacks and are shippable in any order after Phase 3 lands.

Phase order:

1. **M9-1** wrapper sig + `out_styled_dom_ptr` arg + X8 seed (small, ~120 LOC)
2. **M9-2** `AzStartup_buildLayoutInfo` + JS instantiates layout (~200 LOC)
3. **M9-3** embed LayoutWindow in EventloopState + `AzStartup_initLayoutCache` (~400 LOC)
4. **M9-4** `AzStartup_hitTest` + drop JS regex (~150 LOC)
5. **M9-5** diff + TLV patch emission (~250 LOC)
6. **M9-6** loader.js cleanup — mostly deletions (~150 LOC)

Each phase = one shippable commit. Verify per phase against
`/tmp/e2e.js` (counter 5→12) plus the new layout-probe scripts
in the handoff doc. Don't conflate phases — small commits are
easier to bisect when something breaks.

## CRITICAL verification (don't skip)

Per phase:
- `node /tmp/e2e.js` returns 5→12 OK in BOTH subprocess
  (`AZ_NATIVE_REMILL` unset) AND native (`AZ_NATIVE_REMILL=1`)
  modes.
- The probe script for THIS phase passes (see handoff for
  exact scripts).

After Phase 6:
- Loader.js contains zero references to `data-az-cb`, `id="az_*"`,
  `textContent`, `getElementById`, `azNodeIdxFromEvent`,
  `azInvokeCbDirect`.
- A multi-button demo (write `examples/c/counter-list.c` with
  Add/Remove row buttons) tests Insert/Replace/Remove patches.

## What NOT to break

- M8.9 e2e (counter 5→12 over 7 clicks) in both subprocess +
  native modes.
- CSS cascade output in the `<style>` block (M8.9 fix).
- M8.9-1 lift trace divergence fix (SimpleTraceManager mirror).
- M8.9-3b duplicate-symbol fix (`export_as` as .o stem).

## Safety net

Before starting: `git tag m8.9-victory` at HEAD of
`layout-debug-clean` (currently `9780a92b3`). `git reset --hard
m8.9-victory` returns to the M8.9 close-out state.

## What NOT to do

- Don't update `doc/guide/en/internals/web.md` stage by stage.
  Rewrite the dispatch + Phase B/C sections ONCE at the end of
  M9, mirroring M8.9.
- Don't add JS↔WASM round-trips beyond
  `__az_resolve_callback(fn_addr) → table_idx`. If a phase
  needs one, something is wrong with the architecture.
- Don't serialize StyledDom across the JS boundary. It stays
  wasm-resident forever. Only TLV patches cross.
- Don't add a separate `__az_resolve_layout` JS bridge. The
  layout cb is just another cb whose table_idx is registered
  via the existing `__az_resolve_callback` machinery.

## Commit message conventions

```
web: M9-1 layout-cb wrapper with X8 hidden return
web: M9-2 LayoutCallbackInfo builder + JS layout-cb instantiation
web: M9-3 WASM-resident LayoutWindow + initLayoutCache
web: M9-4 WASM-side hit-test replaces JS azNodeIdxFromEvent
web: M9-5 diff + TLV patch emission, kill textContent= hack
web: M9-6 minimize loader.js to event-encode + patch-apply only
```

Plus a final `docs: refresh web.md + STATUS_REPORT for M9` once
all six phases land.

Good luck.
