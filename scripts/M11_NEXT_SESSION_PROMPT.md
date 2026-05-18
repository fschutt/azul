# M11 — Next-session prompt: make the web backend production-usable

You're continuing work on Azul's web backend. Size optimization (M10-E/F)
is **closed** — per-cb wasms are 700-3500 B, hello-world.bin is 96 KB
legacy / 147 KB sharded (-31% vs original). The pipeline is solid;
what's missing is **production-grade event handling + DOM mutation +
benchmarking** so Azul can stand next to React/Preact/Svelte as a
viable web framework.

## Branch + commit state

```
Branch: layout-debug-clean
HEAD:   33412df82 docs: M11 plan — production web event/patch coverage + js-framework-benchmark
```

## Read in order

1. `scripts/M11_PRODUCTION_PLAN_2026_05_19.md` — the full 5-stage plan
   from the Plan agent's investigation. Concrete file-level work items
   per stage, dependency graph, effort estimates.
2. `scripts/STATUS_REPORT_M10_F_2026_05_19.md` — what M10 ended on
   (size-axis wins, gates all green).
3. `scripts/HANDOFF_2026_05_18_M9_E2E_GREEN.md` — M10 progress table
   (workstreams A-F all landed).

Skip-reading (older, superseded by M11 plan):
- `scripts/M10_D_PLAN.md`, `scripts/M10_RELIABILITY_AND_OPTIMIZATION_PLAN.md`

## Hard direction from user — DO NOT deviate

1. **NO JS-side diffing.** The typed `StyledDom` must be hydrated
   wasm-side. The plan agent flagged this as the high-risk path; user
   says: take it anyway. Stage B.1's "preferred" path (layout cb
   returns `StyledDom`, wrapped via `Dom::style(&Css::empty())`) is
   the only path.

2. **No 10k flat-row TodoMVC bench.** The whole point of Azul's
   architecture is that you don't render 10k nodes if only 100 are
   visible. `core/src/callbacks.rs:172-289` already has
   `VirtualViewCallback` + `VirtualViewCallbackReason` with
   `EdgeScrolled(EdgeType)` for bounds-aware re-invocation. The
   "10k rows" benchmark variant MUST use VirtualView, not a flat
   table. The headline comparison narrative becomes:
   - **React/Preact/Svelte 10k**: render N=10000 virtual nodes,
     browser handles scroll. Wire bytes + paint time scale with N.
   - **Azul 10k**: VirtualView cb returns the ~30 visible rows per
     viewport. Wire bytes + paint time stay flat in N because only
     visible rows hit the DOM.

   This is a SELLING POINT, not a workaround. Document it that way.

3. **Hit-test is still a stub** — `eventloop.rs:689` returns
   `last_registered_cb_node_idx`. Plan thought it might already be
   real; it isn't. M9-3b (LayoutWindow embed) never landed. Stage A.3
   depends on this; don't gloss over it.

4. **Auto-virtualize above a threshold.** If a layout cb returns a
   DOM with > ~500 nodes inside a scrollable container, auto-wrap
   that subtree into a synthetic VirtualView. User-facing API stays
   the same; behavior matches the principle "render only what's
   visible." Document the threshold + add a knob (e.g.
   `AzAppConfig.disable_auto_virtualization`).

## Phase ordering for THIS session

Don't try to execute the whole 24.5d plan. Stage by stage. The right
opening sequence:

### Sprint 1 — unblock the diff path (B.1, ~2d)

Make the layout cb return a typed `StyledDom` instead of a raw blob.
The cleanest path:
- The lifted layout-cb wrapper currently writes to a caller-allocated
  256-byte `out_ptr` (per M9-1's `Pcs::HiddenPtrReturn`). The cb's
  return value is a `AzDom`. Change the framework-side wrapper so it
  also runs `StyledDom::from_dom(dom, &Css::empty())` and writes the
  resulting `StyledDom` blob to `out_ptr` instead. Size of `StyledDom`
  is bigger than `AzDom` — bump `out_ptr` allocation accordingly.
- Add a wasm-side parser that reads the blob into a typed
  `StyledDom`. Store in `EventloopState.current_dom`.
- Verify: `full-cycle.js` still passes, `state.current_dom` is `Some`
  after `initLayoutCache`.

Risk: `StyledDom::from_dom` may reach const-pool loads that don't
survive the lift. Mitigation: stub the cascade fn — emit only the
node hierarchy + raw attribute set, skip computed-style cascade.
For the diff loop we only need node identity + text + attrs +
inline-style; cascade can come later.

### Sprint 2 — real hit-test (A.3 prereq, ~1d)

`AzStartup_hitTest` walks `current_dom.positioned_rects` (computed
during `initLayoutCache`) and returns the deepest containing node.
Fall back to the M9-4 stub when `current_dom` is None.

Verify: synthesize a click at (200, 200) where the cb-bearing node is
at (100, 100)-(300, 300). Should return that node's az_id.

### Sprint 3 — diff loop + 4 essential patch kinds (B.2 + half of B.3/B.4, ~2d)

On `RefreshDom`, re-run layout cb → parse new blob → call
`reconcile_dom_with_changes` → encode the top-4 most-needed patch
kinds:
- kind=1 SetText (already wired; keep)
- kind=4 SetInlineStyle (style changes)
- kind=2 SetAttr (id/class changes via the existing attribute set)
- kind=5 RemoveNode + kind=6 InsertNode (the create/delete pair)

Defer kind=7 MoveNode + kind=8 ReplaceSubtree to Sprint 5.

JS decoder in `loader_js.rs:azApplyPatches` switches on each kind.

Verify: a hand-written cb that toggles a node's text + adds a class
emits exactly 2 patches (kind=1 + kind=11/2).

### Sprint 4 — event wiring for the bench's needs (A.1/A.2/A.4 narrow scope, ~1.5d)

For TodoMVC-style bench we need: `click` (have), `input`, `keydown`,
`focus`, `blur`, `scroll`. Skip touch/drag/composition/wheel for now.
Variable-width payload TLV per Sprint 3's plan. JS listener wiring.

### Sprint 5 — VirtualView wasm-side + auto-virtualization (~3d)

This is the architecturally-load-bearing piece:
- The wasm-side layout pass needs to invoke `VirtualViewCallback` when
  it encounters a `NodeType::VirtualView`. Today the desktop pass
  does this in `azul_layout::virtualized_view_manager`; check it lifts.
- Auto-virtualization heuristic: if a `Dom::ol`/`Dom::ul`/`Dom::table`
  subtree has > `AUTO_VIRTUALIZE_THRESHOLD` (=500) direct children,
  wrap as `Dom::virtual_view(...)` with a stub provider that returns
  exactly the visible slice based on `scroll_offset` + `bounds`.
- Wire scroll events (Sprint 4 already handles `scroll`) to call
  `VirtualViewCallback` with `EdgeScrolled(...)`.

### Sprint 6 — TodoMVC bench example (D.1 + bench narrative, ~2d)

`examples/c/azul-bench.c`. Two variants:
- `azul-bench-flat.c` — renders all rows directly (matches
  react/preact/svelte's "render all" approach). Tests the diff +
  insert/remove patches at moderate N (1000).
- `azul-bench-virtual.c` — uses `VirtualView` for the 10k case.
  Tests the virtualization narrative.

Harness HTML + measurement script (Stage D.2 + D.3). Report at
`scripts/BENCH_REPORT_M11_<date>.md`.

### Sprint 7 — CallbackChange plumbing (Stage C narrow, ~2d)

Just the variants the bench needs: `SetFocusTarget`, `ScrollTo`,
`ChangeNodeText`, `ChangeNodeCssProperties`. Defer timers /
clipboard / route-switch to later.

### Sprint 8 — docs + showcase (E, ~1.5d)

`EVENT_PATCH_SCHEMA.md` next to the code. `README-web.md` for
"how to deploy a C example as a web app." GIF/asciinema of the bench
running. Status report.

## Where the M11 plan is wrong / needs revision based on user direction

- **Drop "Workaround for B.1: JS-side diff against rendered HTML"**.
  User said no. Update the plan to remove that fallback when you
  start.
- **D.4 risk "create10000 may exceed bump allocator"** — irrelevant
  because we don't render 10k. The VirtualView approach renders ~30
  rows per viewport. The bump-allocator pressure disappears.
- **D.1's bench shape** should be the two-variant split above, not a
  single bench.

## Hard rules

- **Keep all 9 existing acceptance gates GREEN** throughout. They're
  the regression net. The four legacy gates exercise the existing
  `click → SetText` path; the four sharded gates exercise the
  manifest+boundary plumbing; `bundle-size-comparison.js` is the
  size regression check.
- **No JS-side workarounds for missing wasm fns.** If `reconcile_dom`
  needs to fire wasm-side, lift it. If a const-pool load breaks,
  fix the lifter (or use the M9-after-review synthetic-address +
  data-mirror approach for the missed page).
- **Document the VirtualView narrative early.** Even if Sprint 5
  isn't done, the prose framing of "azul doesn't render 10k flat
  nodes — it virtualizes" needs to be in the bench report from
  Sprint 6 onward.

## Debug knobs (carry-over from M10)

- `AZ_NATIVE_REMILL=1` — fast in-process compile (default fast path)
- `AZ_REMILL_KEEP_SCRATCH=1` — preserve `$TMPDIR/azul-web-transpiler-<pid>/`
- `AZ_WASM_DEBUG=1` — subprocess path + names section (slow, ~30s/lift)
- `AZ_REMILL_SKIP_WASM_OPT=1` — skip post-link `wasm-opt -Oz`
- `AZ_WASM_MIRROR_TRACE=1` — log per-page mirror skip events
- `AZ_ENABLE_SHARDS=1` — opt into M10-D per-fn boundary shards
- `AZ_REMILL_MERGED_COMPILE=1` — force merged compile (auto-on for ≤30 fns)
- `AZ_REMILL_DISABLE_AUTO_MERGE=1` — force per-fn .o path

## Acceptance gates to add (per sprint)

- Sprint 1: new `scripts/m9_e2e/styled-dom-hydrate.js` — confirms
  `current_dom` is `Some` after `initLayoutCache`.
- Sprint 2: new `scripts/m9_e2e/hit-test.js` — synthesize a click at
  known coordinates, verify the returned node_idx matches the bbox.
- Sprint 3: new `scripts/m9_e2e/diff-patches.js` — toggle text +
  style; verify the patch stream contains kind=1 + kind=4.
- Sprint 5: new `scripts/m9_e2e/virtual-view-scroll.js` — render a
  VirtualView with 10k logical rows; verify only ~30 are in DOM;
  scroll 1000px; verify the row set updated.
- Sprint 6: `scripts/bench-runner.js` — runs azul-bench-flat +
  azul-bench-virtual through 25 measure rounds per op, reports JSON.

## When you're done

- Commit each sprint with a clear message; co-author line:
  ```
  Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
  ```
- After all 8 sprints land:
  - Write `scripts/STATUS_REPORT_M11_<date>.md`.
  - Update `scripts/HANDOFF_2026_05_18_M9_E2E_GREEN.md` M11 row in the
    progress table.
  - Supersede this prompt with a banner pointing at the new status
    report.
  - Update `scripts/M11_PRODUCTION_PLAN_2026_05_19.md`'s banner with
    "landed" + commit refs.

## Order rationale

1. B.1 (StyledDom hydrate) unblocks B.2/B.3/A.3 — that's the
   architectural lynchpin. Go first.
2. Real hit-test (A.3 prereq) lets cb dispatch work for any UI, not
   just hello-world.
3. Diff loop + 4 patch kinds is the minimum to render anything
   non-trivial.
4. Event wiring for bench's needs is narrow scope but unblocks D.
5. VirtualView is the CORE distinguishing feature. Do it before bench.
6. Bench shows where we stand.
7. CallbackChange plumbing fleshes out the remaining cb surface.
8. Docs make it usable by others.

This sequence delivers a working web framework after Sprint 6
(11-12 days). Sprints 7-8 are polish.

## Things NOT to try

- **alwaysinline + merged compile on the layout cb**: tested in M10-F3,
  blew up the 141-fn layout to 275 KB. The 30-fn auto-threshold stays.
- **JS-side diff against rendered HTML**: user explicitly said no.
- **A separate "lift the cascade" workstream**: too big. The
  `Css::empty()` workaround in B.1 dodges it.
- **10k flat-row TodoMVC** for the bench: contradicts Azul's
  architectural philosophy. Always use VirtualView for that scale.
