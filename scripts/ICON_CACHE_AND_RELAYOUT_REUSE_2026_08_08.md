# `<icon>` resolution cost, and why the DOM diff never reaches layout

2026-08-08. Companion to `RSS_MAP_2026_08_07.md` §36; written after the mouse-
resize capture measured `regenerate_layout` at 654–942 ms while `resize_surface`
costs 0.09–0.45 ms. Two subjects: the `<icon>` waste (FIXED, landed in core) and
the layout-reuse gap (ROOT-CAUSED here, design below, not yet implemented).

---

## 1. The `<icon>` feature is right; its cost model was wrong

The design intent (core/src/icon.rs module docs, CLEANUP_PLAN.md "swappable
`<icon>`"): a resolver callback `(RefAny, &StyledDom, &SystemStyle) -> StyledDom`
so ANY icon source — image, font glyph, SVG, animated — can be substituted, and
the replacement is a full `StyledDom` so the resolver can "fix up" whatever it
likes. That flexibility is the point and is retained untouched.

The cost was never in the flexibility. It was in WHEN the work re-ran. Per icon,
per `resolve_icons_in_styled_dom`, per DOM regeneration:

| step | what it allocated / computed | fate |
|---|---|---|
| `extract_single_node_styled_dom` | clone of the node's `NodeData` incl. its inline `Css` (one `CssDeclarationVec` PER PROPERTY), 5 one-element vecs, a fresh `CssPropertyCache::empty` | read once by the resolver, dropped |
| `lookup_spec` | lowercase `String` per lookup, `RefAny` clone | dropped |
| resolver (`layout/src/icon.rs`) | style-vec clone off the original, a `Dom`, then **`StyledDom::create` = the FULL single-node cascade**: `apply_ua_css`, `compute_inherited_values`, `build_compact_cache_with_inheritance`, `prune_compact_normal_props`, `generate_tag_ids` — the `[CASCADE] 1 nodes / total=2` log signature | **100% discarded** |
| `apply_single_node_replacement` | moves node_type / style / a11y / styled_node in | the only part that survives |

The cascade discard is structural, not accidental: `regenerate_layout` step 3.4
re-computes inheritance + compact cache **on the composed tree, after icons**
(dll/src/desktop/shell2/common/layout.rs, `after_recompute_cache`), so any
property-cache work done inside a replacement `StyledDom` is overwritten
wholesale. The resolver paid for a cascade nobody could ever read.

Scale, measured: miniword's ribbon has ~66 icon nodes; a 5-second drag-resize
delivers 373 regenerations → **≈24 600 throwaway single-node cascades** and
~50k+ allocations/second of pure churn — feeding exactly the freed-but-held
fragmentation curve of RSS_MAP §31. Same name, same styles, same theme → the
24 600 results were bit-identical.

### The fix (LANDED, core-only, no API change)

A resolution cache on `SharedIconProvider` (`core/src/icon.rs`):

- **Key** = (icon spec, the original node's full `NodeData`, its `StyledNode`).
  The whole `NodeData`, because a custom resolver may read anything from
  `original_icon_dom`; the `StyledNode`, so a hover-state flip re-resolves.
  Same name with different inline styles → separate entries
  (`distinct_inline_styles_are_distinct_cache_entries`).
- **Value** = the resolver's output **deconstructed** into exactly the four
  fields the replacement consumes (`CachedIconResolution::SingleNode { node_type,
  style, accessibility, styled_node }`). A hit is four field clones — no `Dom`,
  no `StyledDom`, no cascade, no `CssPropertyCache`, and no extraction of the
  original either. This is the "just swap the NodeData in" fast path, applied to
  the *producing* side (the applying side already moved instead of cloning).
- **Invalidation**: `SystemStyle` mismatch flushes everything (resolvers read
  theme/tint/grayscale), checked once per batch. Registration/resolver changes
  need no invalidation post-share — `App::run` consumes the handle and
  `SharedIconProvider` exposes no mutation, so the icon set and resolver are
  frozen by construction. Cap 512 entries; overflow flushes all (degrades to
  uncached, never to unbounded memory).
- **Animated icons** stay compatible: animation rides in the returned DATA
  (e.g. an image-callback node that animates per frame). Re-resolution only
  ever happened per DOM regeneration, so caching it changes nothing observable.

Reproduction test (`icon_cache_tests::identical_icons_across_frames_resolve_
exactly_once`): 3 identical icons × 3 frames. **Cache disabled: 9 resolver
calls (verified red). Cache enabled: 1.** Parity is pinned by
`cached_hit_produces_an_identical_node` (`NodeData` bit-equality between a
fresh resolve and a hit).

What this does NOT fix: multi-node splicing is still root-only
(`apply_multi_node_replacement`'s TODO), and the cache deliberately stores the
full subtree so real splicing later cannot be silently truncated by it.

---

## 2. Why the DOM diff never reaches layout — the receipts

The user's hypothesis: "layout isn't cached properly across frames, the DOM
diff probably runs but is ignored." Confirmed, with a sharper mechanism — on
the path that hurts (resize), the diff does not even run:

1. **The only diff is `is_layout_equivalent`** (core/src/styled_dom.rs:2760),
   called from ONE place: `regenerate_layout` step 3.7
   (dll/src/desktop/shell2/common/layout.rs). It is **gated on
   `!window_size_changed`** — so a resize, the one case where the DOM is
   *guaranteed* unchanged (only the viewport moved), **bypasses the check
   entirely** and falls through to the full pipeline.

2. **Its outcome is binary and dies where it is computed.** Equivalent →
   skip everything (patch image/event callbacks into the OLD result, return
   `LayoutUnchanged`). Not equivalent, or bypassed → the fresh `StyledDom` is
   handed to `layout_and_generate_display_list(root_dom, …)`
   (layout/src/window.rs:1367), **whose signature has no diff parameter of any
   kind**. Nothing downstream can know whether one node changed or all of them.

3. **Layout's first act is to forget the previous frame.**
   `window.rs:1406: self.layout_results.clear()` — commented "Clear previous
   results for a full relayout" — plus
   `window.rs:1412: virtual_view_manager.reset_all_invocation_flags()`, which
   forces every VirtualView back through `InitialRender` on EVERY regeneration
   (this is the miniword document view; re-invoked 373 times per drag).

4. **The produce side reruns unconditionally before any of that**: user layout
   callback → `create_from_dom` full cascade (the measured 921- and 1251-node
   `[CASCADE]` pair) → icons (now cached) → CSD injection → step-3.4 cascade
   recompute → step-3.5 state migration. Note the order: on the equivalent
   path, migration has ALREADY moved heavy resources into the new DOM — which
   is then thrown away.

5. **Solver3's persistent cache survives across frames** (`layout_cache`, the
   warm shaping data of the memory map) but must RECONCILE the brand-new
   `StyledDom` against it by content each time. When it is handed the *same
   object* the match is trivial and shaping is reused; a fresh equal-but-not-
   identical tree makes reuse a heuristic instead of a guarantee — and the
   reconcile itself was already one NodeId-identity bug (b975d58ba).

**The proof the machinery exists**: `incremental_relayout`
(common/layout.rs:1005) does exactly the right thing — pulls the OLD
`DomLayoutResult` out, moves its `styled_dom` back into
`layout_and_generate_display_list`, skips the entire produce side. Scroll and
animation updates use it. **Resize does not**: every backend's configure
handler calls `request_regeneration(RelayoutReason::Resize)` → full
`regenerate_layout`.

## 3. Design: make Resize reuse what the engine already keeps

Phased so each step is separately measurable (PhaseTimer landed in
`regenerate_layout` — one `[phases]` line per relayout, gated on
`AZ_LOG=debug` at Window category — gives the per-phase split needed to verify
each claim below before and after).

**Phase R1 — equivalence check on the resize path too (small, safe).**
Remove the `!window_size_changed` gate; on `size_changed && equivalent`, do NOT
take the skip-everything arm — instead drop the freshly-built DOM and run the
`incremental_relayout` ownership dance: old `styled_dom` (same object) back
through layout at the new viewport. Produce side still paid; solver3 reconcile
becomes exact-match → shaping/intrinsics reuse is guaranteed instead of
heuristic, and VirtualViews can be re-invoked with a Resize reason rather than
InitialRender. Risk: none beyond the check's own cost (O(n) compare; make its
per-node Vec-collecting id/class compare allocation-free while there).

**Phase R2 — skip the produce side when a resize cannot change the DOM (the
big one).** The configure handler already computes
`viewport_breakpoint_changed` (wayland/events.rs:1603). If reason == Resize
AND no CSS breakpoint was crossed, the DOM cannot have changed through any
*declarative* channel — go STRAIGHT to `incremental_relayout` and skip the
user callback, cascade, icons, CSD, recompute and migration entirely.
Caveat that needs a USER DECISION: an app may read window dimensions
imperatively inside `layout()` and build a different DOM without any @media
breakpoint (miniword's ribbon does size-adaptation via inline @media — the
known-inert feature — but the capability exists). Options: (a) opt-in
`AppConfig` flag "resize_reuses_dom", (b) opt-out, defaulting to reuse, with
the imperative-read case documented as requiring the flag, (c) detect via a
dirty-bit the app sets. Recommendation: (b) — the imperative pattern is rare,
the 75-relayouts-per-second drag is universal.

**Phase R3 — a real diff parameter (structural edits, later).**
`layout_and_generate_display_list(root_dom, diff: DomDiff)` where
`DomDiff ∈ {Identical, Reflow(viewport), Nodes(dirty set), Structural}`, so
text edits stop re-laying-out the ribbon. This is the general fix the binary
equivalence check cannot express; it subsumes R1/R2 and is where the
`is_layout_equivalent` call should eventually migrate into.

**Sequencing note**: R1/R2 change WHEN 654–942 ms is paid (once per final
size instead of 75×/s). Making the relayout itself cheaper is the separate,
frozen memory-plan work (shaped text dominates); these are complementary, and
the PhaseTimer split should be captured FIRST so each phase's win is a
number, not a belief.
