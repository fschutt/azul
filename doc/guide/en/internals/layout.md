---
slug: internals/layout
title: Layout
language: en
canonical_slug: internals/layout
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: solver3 architecture, formatting contexts, and the per-frame relayout cycle
prerequisites: [code-organization, dom-internals, cascade, compact-cache]
tracked_files:
  - layout/src/lib.rs
  - layout/src/window.rs
  - layout/src/solver3/mod.rs
  - layout/src/solver3/cache.rs
  - layout/src/solver3/fc.rs
  - layout/src/solver3/layout_tree.rs
  - layout/src/solver3/sizing.rs
  - layout/src/solver3/positioning.rs
  - layout/src/solver3/taffy_bridge.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:55:41Z
default-search-keys:
  - StyledDom
  - Dom
  - Css
  - CssProperty
  - LayoutDisplay
---

# Layout

## Overview

Layout takes a fully styled DOM and produces a flat display list of drawing
primitives in absolute window coordinates. The active engine is `solver3`, the
third iteration of azul's layout pipeline. *WIP — solver3 is in production use
but a few CSS features (column fragmentation, `initial-letter`, parts of
vertical-align) are still incomplete; see the per-page notes below.*

The layer is split between `solver3` (block, inline, table; flex and grid via
[Taffy](https://github.com/DioxusLabs/taffy)) and `text3` (shaping, line
breaking, BiDi, hyphenation, editing). The solver hands inline formatting
contexts to `text3` through `layout_ifc`, and the text engine
returns a `UnifiedLayout` that the solver caches on the IFC root. The
[Inline Text](layout/inline-text.md) page is the deep dive on `text3`.

`solver3` runs once per frame and writes into a `LayoutCache` that survives
between frames. An unchanged DOM hits a pointer-identity fast path and skips
reconciliation entirely. A small structural change (a hover state flip, a text
edit, a viewport resize) invalidates only the affected nodes via the
`DirtyFlag` propagation described below. For the printable-output path —
splitting one tall layout into pages, headers, and footers — see
[Fragmentation](layout/fragmentation.md).

## Entry point

`layout_document` in `solver3/mod.rs` is the single entry. It borrows a
`StyledDom` (earlier revisions took ownership and forced every shell to clone
~2 MiB on `excel.html`), mutates a `LayoutCache` in place, and returns a
`DisplayList`.

```rust,ignore
pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    text_selections: &BTreeMap<DomId, TextSelection>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
    cursor_is_visible: bool,
    cursor_locations: Vec<(DomId, NodeId, TextCursor)>,
    preedit_text: Option<String>,
    image_cache: &ImageCache,
    system_style: Option<Arc<SystemStyle>>,
    get_system_time_fn: GetSystemTimeCallback,
) -> Result<DisplayList>;
```

## The 5-step pipeline

`layout_document` runs five steps in order, each with its own profile span via
`probe::Probe::span`. The shape:

- **Step 0.** Pointer-identity check on the styled DOM and viewport hash.
  Returns the cached display list unchanged.
- **Step 1.** Build a new `LayoutTree` from the new `StyledDom`,
  fingerprint-diff against the old tree, and mark `intrinsic_dirty` and
  `layout_roots`. Implemented in `cache::reconcile_and_invalidate`.
- **Step 1.4.** Move the 9+1 slot cache from old to new layout indices via
  stable identity — first by DOM id, then by anonymous-by-parent ordinal.
  Implemented in `cache::LayoutCacheMap::resize_to_tree`.
- **Step 2.** Bottom-up min/max-content for every dirty node, in
  `sizing::calculate_intrinsic_sizes`.
- **Step 2 loop.** Top-down for each `layout_root` via
  `cache::calculate_layout_for_subtree`. May trigger up to
  `MAX_SCROLLBAR_REFLOW_ITERATIONS = 10` iterations when scrollbar appearance
  changes the available width.
- **Step 3.** Apply `position: relative` offsets after sizing
  (`positioning::adjust_relative_positions`).
- **Step 3.25.** Clamp sticky elements against the scroll offset
  (`positioning::adjust_sticky_positions`).
- **Step 3.5.** Place `position: absolute` and `position: fixed` elements
  (`positioning::position_out_of_flow_elements`).
- **Step 3.75.** Compute stable scroll IDs for WebRender pipelines, in
  `LayoutWindow::compute_scroll_ids`.
- **Step 4.** Emit the `DisplayList` via `display_list::generate_display_list`.

Inverting the order of the three positioning steps breaks specific CSS
behaviour. Relative offsets must run before sticky and absolute because both
of those resolve their containing blocks against the post-relative geometry.
The comments in `solver3/mod.rs` document which spec sections each step
implements.

## LayoutContext

The single per-pass context borrows the DOM, font manager, image cache, scroll
offsets, and debug-message vec, plus owned working state (counters,
fragmentation context, cache map).

```rust,ignore
pub struct LayoutContext<'a, T: ParsedFontTrait> {
    pub styled_dom: &'a StyledDom,
    pub font_manager: &'a FontManager<T>,
    pub text_selections: &'a BTreeMap<DomId, TextSelection>,
    pub debug_messages: &'a mut Option<Vec<LayoutDebugMessage>>,
    pub counters: &'a mut HashMap<(usize, String), i32>,
    pub viewport_size: LogicalSize,
    pub fragmentation_context: Option<&'a mut FragmentationContext>,
    pub cursor_is_visible: bool,
    pub cursor_locations: Vec<(DomId, NodeId, TextCursor)>,
    pub preedit_text: Option<String>,
    pub dirty_text_overrides: BTreeMap<(DomId, NodeId), String>,
    pub cache_map: cache::LayoutCacheMap,
    pub image_cache: &'a ImageCache,
    pub system_style: Option<Arc<SystemStyle>>,
    pub get_system_time_fn: GetSystemTimeCallback,
    pub scrollbar_style_cache:
        RefCell<HashMap<NodeId, ComputedScrollbarStyle>>,
}
```

`cache_map` is `mem::take`n out of `LayoutCache` for the duration of the pass
and moved back at the end. This avoids `&mut LayoutCache` aliasing during
sizing and positioning, which both need `&mut tree` and `&cache_map`
simultaneously. `scrollbar_style_cache` uses `RefCell` so the Taffy bridge's
`&self` `get_core_container_style` can mutate it without lifting the whole
context to `&mut`. `compute_scrollbar_info_core` would otherwise walk the
cascade nine times per node per pass on a cold lookup.

## LayoutTree and the hot/warm/cold split

`LayoutNode` was historically a ~550-byte AoS struct. The current layout
splits each node into three slabs indexed by the same `usize`:

- **hot** (~80 B): `parent`, `children`, `dom_node_id`, `dirty_flag`,
  `formatting_context`, `box_props`, `used_size`, `relative_position`. Every
  traversal pass touches these.
- **warm**: `inline_layout_result: Option<Arc<CachedInlineLayout>>`,
  `taffy_cache`, `intrinsic_sizes`, `unresolved_box_props`, IFC membership.
  Touched only on dirty nodes.
- **cold**: `node_data_fingerprint`, `subtree_hash`, scrollbar info, IFC ID.
  Touched only at reconcile time.

A 64-byte cache line now loads multiple hot records' fields together rather
than one node's 50 unused bytes. The split is invisible to call sites; getter
helpers (`tree.get(idx)`, `tree.warm(idx)`, `tree.cold(idx)`) hide the
dispatch. `children` lives in a single `children_arena: Vec<usize>` keyed by
`(start, len)` per node, eliminating N heap allocations per tree.

## DirtyFlag

```rust,ignore
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum DirtyFlag {
    #[default]
    None,
    Paint,
    Layout,
}
```

`PartialOrd` makes `mark_dirty(idx, flag)` cheap: the ancestor walk stops as
soon as it hits a node already at `>= flag`. `mark_subtree_dirty(flag)` marks
every descendant — used for inherited CSS changes such as font family.

## Reconciliation

`cache::reconcile_and_invalidate` builds a new `LayoutTree` from the current
`StyledDom` and produces a `ReconciliationResult { intrinsic_dirty,
layout_roots }`. A viewport size change unconditionally adds the root to
`layout_roots`. Otherwise `reconcile_recursive` walks both trees in parallel
and computes a `NodeDataFingerprint` for each new-DOM node:

```rust,ignore
pub struct NodeDataFingerprint {
    pub content_hash: u64,
    pub state_hash: u64,
    pub inline_css_hash: u64,
    pub ids_classes_hash: u64,
    pub callbacks_hash: u64,
    pub attrs_hash: u64,
}
```

`fingerprint.diff(old)` returns a `NodeChangeSet`. `change_set.needs_layout()`
triggers `DirtyFlag::Layout`; otherwise `DirtyFlag::Paint`. Pure additions get
`DirtyFlag::Layout`. The original design called for a per-property
`RelayoutScope` plumbed through to a `ChangeAccumulator`; the implementation
stopped at the multi-field fingerprint, which is enough for the common cases
(text edits, hover, viewport resize). The `RelayoutScope` enum exists but is
not yet consumed by the solver. Hover-only restyle still triggers a full DOM
rebuild via `ShouldRegenerateDomCurrentWindow`.

After the recursive pass, redundant `layout_roots` are pruned: if a parent is
already a root, its children are removed (the parent's top-down pass will
re-position them).

## LayoutCacheMap — Taffy-style 9+1 slots

`NodeCache` lives one per layout-tree node, kept in a flat `Vec<NodeCache>`
parallel to `tree.nodes`. Each node has 9 measurement slots plus 1
full-layout slot:

- Slot 0 — both width and height definite.
- Slots 1 and 2 — only width is known (Definite/MaxContent versus MinContent).
- Slots 3 and 4 — only height is known (Definite/MaxContent versus MinContent).
- Slots 5 through 8 — neither is known (2x2 combos).

`SizingCacheEntry` stores `(available_size, result_size, baseline,
escaped_top_margin, escaped_bottom_margin)` — no positions.
`LayoutCacheEntry` (the +1 slot) adds `child_positions: Vec<(usize,
LogicalPosition)>`, `content_size`, and `scrollbar_info`. Slot index is
deterministic from the constraint shape, so MinContent and Definite
measurements never collide. `cache_map.mark_dirty(idx, &nodes)` clears the
node and walks ancestors; an ancestor whose own cache is already empty stops
the walk (Taffy's optimisation — descendants would be re-cleared on the way
down anyway).

When the tree is rebuilt, `cache_map.entries` are remapped from old layout
indices to new ones in two passes: first by `(dom_id → layout_idx)`, then
anonymous wrappers (no DOM id) by `(parent_new_idx, ordinal)`. Without the
second pass, anonymous box wrappers re-allocate empty every reconcile and
invalidate their ancestors via `mark_dirty`.

## Formatting-context dispatch

`layout_formatting_context` routes each node to one of seven layout
functions:

```rust,ignore
match node.formatting_context {
    FormattingContext::Block { .. } =>
        layout_bfc(ctx, tree, text_cache, node_index, constraints, float_cache),
    FormattingContext::Inline =>
        layout_ifc(ctx, text_cache, tree, node_index, constraints)
            .map(BfcLayoutResult::from_output),
    FormattingContext::InlineBlock =>
        layout_bfc(ctx, tree, text_cache, node_index, constraints, &mut HashMap::new()),
    FormattingContext::Table =>
        layout_table_fc(ctx, tree, text_cache, node_index, constraints)
            .map(BfcLayoutResult::from_output),
    FormattingContext::Flex | FormattingContext::Grid =>
        layout_flex_grid(ctx, tree, text_cache, node_index, constraints),
    FormattingContext::TableCell | FormattingContext::TableCaption =>
        layout_bfc(ctx, tree, text_cache, node_index, constraints, &mut HashMap::new()),
    _ => layout_bfc(ctx, tree, text_cache, node_index, constraints, &mut HashMap::new()),
}
```

`InlineBlock` always establishes a fresh BFC for its children even though it
participates as an atomic inline in its parent IFC. Table-internal flex items
are blockified at tree-build time (`blockify_flex_item_if_table_internal`),
so they arrive here as `Block`.

## Block formatting context

`layout_bfc` follows CSS 2.2 § 9.4.1 with a two-pass design. The first pass
(`ComputeMode::ComputeSize`) measures every block-level child to obtain
border-box sizes and escaped margins. The second pass
(`ComputeMode::PerformLayout`) stacks children along the main axis, applying
margin collapse (CSS 2.2 § 8.3.1) and writing positions into the +1 slot. The
pen tracks the accumulated top margin until a non-margin "blocker" — border,
padding, or content — resolves it. Bottom margins of the last child can
escape upward to the parent if no blocker intervenes. Floats from
`float_cache` are placed exclusion-aware.

## Inline formatting context

`layout_ifc` is the bridge from box layout to text layout. The IFC root is
the `LayoutNode` whose `inline_layout_result: Arc<CachedInlineLayout>` holds
the shaped/positioned text. Descendant text nodes don't store their own;
they record `IfcMembership { ifc_id, ifc_root_layout_index, run_index }`
pointing back at the root.

```rust,ignore
pub struct CachedInlineLayout {
    pub layout: Arc<UnifiedLayout>,
    pub available_width: AvailableSpace,
    pub has_floats: bool,
    pub constraints: Option<UnifiedConstraints>,
    pub item_metrics: Vec<InlineItemMetrics>,
    pub line_breaks: Option<CachedLineBreaks>,
}
```

`available_width` and `has_floats` are the cache-validity key — a layout
shaped under min-content cannot be reused for the final pass. `item_metrics`
and `line_breaks` enable incremental reshape; the current path uses them as
a cache-hit fast path. Real per-character incremental relayout for text edits
lives in `LayoutWindow::try_incremental_text_relayout` and bypasses
`layout_ifc` entirely. `IfcId` is generated from a global `AtomicU32` counter
that resets at the start of each `layout_document` call. Stable IDs across
frames depend on stable DOM structure.

The full text-shaping pipeline — five stages, fallback chain resolution,
BiDi, hyphenation, the editing surface — lives in `text3` and is documented
in [Inline Text](layout/inline-text.md).

## Flex and grid via Taffy

`solver3` does not implement flex or grid directly. `layout_flex_grid`
constructs a `TaffyBridge<'a, T>` over the current sub-tree and calls Taffy's
`compute_root_layout`. The bridge implements:

- `TraversePartialTree` for child enumeration over `LayoutTree`.
- `LayoutPartialTree` so `compute_child_layout` can dispatch non-flex/grid
  children back to `layout_formatting_context`. Taffy thus calls into solver3
  for the block/inline children of a flex item.
- `CacheTree` using Taffy's per-node cache, separate from the 9+1 slots.
- `LayoutFlexboxContainer` and `LayoutGridContainer` for Taffy's flex/grid
  algorithms.

Azul CSS values are translated to Taffy's `Style` per call. `from_layout_width`
maps `LayoutWidth::Px(PixelValue)` to `taffy::Dimension`: absolute units
(px/pt/em/rem/in/cm/mm) resolve against `DEFAULT_FONT_SIZE` for em/rem
fallback, percentages map to `Dimension::percent(p)`, and viewport units
(`vw`, `vh`, ...) currently fall back to `Dimension::auto()` because the
Taffy bridge has no viewport context.

## Sizing pass

`sizing::calculate_intrinsic_sizes` runs bottom-up and is ancestor-closure
pruned. `dirty_closure` is the union of `dirty_nodes` and every ancestor up
to the root. A node not in the closure with a populated `intrinsic_sizes`
skips its entire subtree walk — before the closure was added, every render
walked the full tree from root, costing ~2 ms even when 3 nodes were dirty.
`tree.subtree_needs_intrinsic` (a static-DOM bitmap precomputed at tree-build
time) is true if the node or any descendant establishes a shrink-to-fit
context. When the caller is non-STF and the subtree is non-STF too, no one
will ever read the intrinsic, and the descent is skipped entirely.

## Scrollbar reflow

Adding a vertical scrollbar reduces the available width for the next pass;
the new layout may itself produce different scrollbars. The loop runs up to
`MAX_SCROLLBAR_REFLOW_ITERATIONS = 10` before bailing with a debug warning.
Each iteration that flips `reflow_needed_for_scrollbars` clears
`layout_roots` to the tree root and marks every node in `intrinsic_dirty`,
forcing a full re-pass.

## Display-list cache

Two cache levels short-circuit the pipeline:

- **Pointer identity.** `dom_ptr == cache.prev_dom_ptr && viewport ==
  cache.prev_viewport && cached_display_list.is_some()`. Returns the cached
  display list without running reconcile.
- **Structural identity.** After reconcile, compare the new root's
  `subtree_hash` against the cached one. Returns the cached display list but
  pays the ~600 µs reconcile cost.

The structural cache fires whenever the DOM is structurally unchanged but a
new `StyledDom` instance was passed (e.g. the user's `layout_callback`
returned a fresh `StyledDom::clone`). It saves the ~4 ms display-list
emission step.

## LayoutWindow

`LayoutWindow` is the per-window aggregate. It owns `layout_cache:
Solver3LayoutCache` and `text_cache: TextLayoutCache`, the `font_manager`,
`image_cache`, `renderer_resources`, the `layout_results` map (one
`DomLayoutResult` per DOM — root plus virtual views), every input-side
manager (`scroll_manager`, `focus_manager`, `text_edit_manager`,
`text_input_manager`, `hover_manager`, `gesture_drag_manager`,
`clipboard_manager`, `a11y_manager`,
`gpu_state_manager`, `virtual_view_manager`), `dirty_text_nodes` for
in-progress text edits, `pending_lifecycle_events` and
`pending_unmount_invocations` produced by `regenerate_layout`, and the
`Epoch` plus `gl_texture_cache` for WebRender resource cleanup.

`layout_and_generate_display_list` is the shell-level entry point. It clears
`layout_results` (full relayout drops previous results), resets virtual-view
invocation flags so a destroyed child DOM doesn't leak `was_invoked=true`
into the next frame, recursively runs `layout_dom_recursive` (which performs
font resolution, builds the GPU value cache, calls
`solver3::layout_document`, registers scrollbar `TransformKey` and
`OpacityKey` into the GPU cache, and recurses into `VirtualView`
placeholders), updates the accessibility tree under `feature = "a11y"`, and
scrolls the focused cursor into view.

### Font resolution skip

Before calling `layout_document`, `LayoutWindow` decides whether the font
resolution pipeline can be skipped entirely. Two signals guard the skip:

1. `compact_cache.font_dirty_nodes.len() == 0` (set by `build_compact_cache`
   when no node's `font_family_hash` changed) AND `font_chain_cache` is
   non-empty.
2. A polynomial rolling hash over `prev_font_hashes` matches
   `font_manager.last_resolved_font_stacks_sig`. This catches the case where
   `build_compact_cache` did not re-run but the font stacks are identical to
   what's already cached.

If both fail,
`collect_and_resolve_font_chains_with_registration` runs the full 5-step
pipeline. The original design used a single XOR fingerprint of all per-node
`font_family_hash` values; XOR is collision-prone (XOR(a, b, a, b) == 0) and
does not survive removing+adding the same font in one frame. The current
per-node dirty list plus rolling-hash fallback fixes both. The deeper
discussion of this resolution path lives in
[Text Pipeline](rendering/text-pipeline.md).

## Memory layout per node

Numbers approximate per node:

- `CompactLayoutCache.tier1_enums` — 8 B/node, hot, SoA.
- `CompactLayoutCache.tier2_dims` — 96 B/node, hot, SoA.
- `CompactLayoutCache.tier2b_text` — 24 B/node, warm, SoA.
- `LayoutNode` (post hot/warm/cold split) — ~90 B hot, ~470 B cold.
- `NodeCache` (9+1 slots) — ~260 B/node, hot, SoA.
- `calculated_positions` — 8 B/node, hot, SoA.
- StyledDom Vecs — ~150 B/node, warm, SoA.

Roughly 1 KiB per node; 10 K nodes is roughly 10 MiB resident. Cold-data
fields stay out of the hot working set after the split.

## Adding a CSS property

A property that does not affect layout (color, decoration) only needs cascade
and getter wiring:

1. Add it to `CssProperty` and `CssPropertyType` in
   `css/src/props/property.rs`.
2. Implement `relayout_scope()` and `is_gpu_only_property()` if applicable.
3. Add a getter in `solver3/getters.rs` if any solver code reads it.
4. Hash it under `inline_css_hash` in `NodeDataFingerprint::compute` so it
   triggers `DirtyFlag::Paint` on change.

A property that changes geometry needs to be read in `sizing.rs` or one of
the `layout_*` formatting-context paths, and must be classified as
`RelayoutScope::SizingOnly` or `Full` so the change set marks
`INLINE_STYLE_LAYOUT`.

## Coming Up Next

- [Inline Text](layout/inline-text.md) — text3, shaping, line breaking, BiDi, hyphenation
- [Fragmentation](layout/fragmentation.md) — page breaks, widows, orphans, paged media
- [Rendering](rendering.md) — display list to pixels, WebRender, GL, image and text resources
- [DOM Internals](dom.md) — how the public `Dom` type is built and stored
- [Compact Property Cache](styling/compact-cache.md) — how layout reads CSS values
