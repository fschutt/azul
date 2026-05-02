---
slug: layout-solver
title: Layout Solver (Flex/Grid)
language: en
canonical_slug: layout-solver
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Architecture of `solver3/` — the entry points for block, flex, grid, positioning, and how they share the inline engine.
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
---

# Layout Solver (Flex/Grid)

> **WIP** — `solver3` is the active engine. The block/inline/table path is azul-native; flex and grid delegate to [Taffy](https://github.com/DioxusLabs/taffy) through `taffy_bridge.rs`.

The solver lives in `layout/src/solver3/`. The single entry point is [`layout_document`](https://github.com/maps4print/azul/blob/master/layout/src/solver3/mod.rs) at `layout/src/solver3/mod.rs:402`. It borrows a `StyledDom`, mutates a `LayoutCache` in place, and returns a [`DisplayList`](https://github.com/maps4print/azul/blob/master/layout/src/solver3/display_list.rs). The cache survives between frames — an unchanged DOM hits the structural-identity fast path at `mod.rs:492` and returns the cached display list verbatim.

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

`new_dom` is borrowed (not owned). Earlier revisions took ownership, which forced every shell to clone the DOM (~2 MiB on `excel.html`); the borrow eliminates that copy.

## File map

| File | Purpose |
|---|---|
| `solver3/mod.rs` | Entry point, `LayoutContext`, sub-tree containing-block resolution |
| `solver3/cache.rs` | Reconciliation, dirty-flag propagation, `LayoutCacheMap` (Taffy 9+1 slots), `reconcile_and_invalidate()` |
| `solver3/layout_tree.rs` | `LayoutTree`, `LayoutNode` (hot/warm/cold split), `DirtyFlag`, `IfcId`, `CachedInlineLayout` |
| `solver3/fc.rs` | Formatting-context dispatcher: `layout_bfc`, `layout_ifc`, `layout_table_fc`, `layout_flex_grid` |
| `solver3/sizing.rs` | Bottom-up intrinsic size calculation (`calculate_intrinsic_sizes`) |
| `solver3/positioning.rs` | `adjust_relative_positions`, `adjust_sticky_positions`, `position_out_of_flow_elements` |
| `solver3/taffy_bridge.rs` | CSS → Taffy `Style` translator + `TraversePartialTree` / `LayoutPartialTree` impls |
| `solver3/display_list.rs` | Display-list emission from `LayoutTree` + `PositionVec` |
| `solver3/scrollbar.rs` | `ScrollbarRequirements` and gutter resolution |
| `solver3/getters.rs` | Cached CSS getters (`get_css_width`, `get_writing_mode`, …) keyed via the compact cache |
| `solver3/pagination.rs`, `paged_layout.rs` | Print-only pagination (see [Fragmentation](fragmentation.md)) |

## The 5-step pipeline

`layout_document` runs five steps in order. Each step has its own profile span (`crate::probe::Probe::span`).

| Step | Function | What it does |
|---|---|---|
| 0 | (inline) | Pointer-identity + viewport hash → return cached DL |
| 1 | `cache::reconcile_and_invalidate` | Build new `LayoutTree` from the new `StyledDom`, fingerprint-diff vs old, mark `intrinsic_dirty` and `layout_roots` |
| 1.4 | `cache::LayoutCacheMap::resize_to_tree` + remap | Move the 9+1 slot cache from old → new layout indices via stable identity (DOM id, then anon-by-parent ordinal) |
| 2 | `sizing::calculate_intrinsic_sizes` | Bottom-up min/max-content for every dirty node |
| 2 (loop) | `cache::calculate_layout_for_subtree` | Top-down for each `layout_root`. May trigger up to `MAX_SCROLLBAR_REFLOW_ITERATIONS = 10` iterations if scrollbar appearance changes available width |
| 3 | `positioning::adjust_relative_positions` | Apply `position: relative` offsets after sizing |
| 3.25 | `positioning::adjust_sticky_positions` | Clamp sticky elements against scroll offset |
| 3.5 | `positioning::position_out_of_flow_elements` | Place `position: absolute / fixed` |
| 3.75 | `LayoutWindow::compute_scroll_ids` | Stable scroll IDs for WebRender pipelines |
| 4 | `display_list::generate_display_list` | Emit `DisplayList` |

## `LayoutContext`

The single per-pass context (`mod.rs:201`). Holds borrows of the DOM, font manager, image cache, scroll offsets, debug-message vec, plus owned working state (counters, fragmentation context, cache map).

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

`cache_map` is moved out of `LayoutCache` via `std::mem::take` for the duration of the pass and moved back at the end. This avoids `&mut LayoutCache` aliasing during sizing/positioning, which both need `&mut tree` and `&cache_map` simultaneously.

`scrollbar_style_cache` uses `RefCell` so the Taffy bridge's `&self` `get_core_container_style` can mutate it without lifting the whole context to `&mut`. `compute_scrollbar_info_core` walks the cascade nine times per node per pass on a cold lookup; the cache eliminates that fan-out for repeated calls in the same render.

## `LayoutTree` and the hot/warm/cold split

`layout_tree.rs:LayoutNode` was historically a ~550-byte AoS struct. The current layout splits each node into three slabs indexed by the same `usize`:

- **hot** (~80 B): `parent`, `children`, `dom_node_id`, `dirty_flag`, `formatting_context`, `box_props`, `used_size`, `relative_position` — every traversal pass touches these
- **warm**: `inline_layout_result: Option<Arc<CachedInlineLayout>>`, `taffy_cache`, `intrinsic_sizes`, `unresolved_box_props`, IFC membership — touched only on dirty nodes
- **cold**: `node_data_fingerprint`, `subtree_hash`, scrollbar info, IFC ID — touched only at reconcile time

A 64-byte cache line now loads multiple hot records' fields together instead of one node's 50 unused bytes. The split is invisible to call sites; getter helpers (`tree.get(idx)`, `tree.warm(idx)`, `tree.cold(idx)`) hide the dispatch.

`children` lives in a single `children_arena: Vec<usize>` arena keyed by `(start, len)` per node — replaces the old `Vec<usize>` per node, eliminating N heap allocations.

## `DirtyFlag` (3-level)

```rust,ignore
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum DirtyFlag {
    #[default]
    None,    // clean
    Paint,   // geometry valid, color/decoration changed
    Layout,  // geometry invalid
}
```

`PartialOrd` makes `mark_dirty(idx, flag)` cheap: walking ancestors stops as soon as it hits a node already at `>= flag`. `mark_subtree_dirty(flag)` marks every descendant — used for inherited CSS changes.

## Reconciliation (`reconcile_and_invalidate`)

`cache.rs:838`. Builds a new `LayoutTree` from the current `StyledDom` and produces a `ReconciliationResult`:

```rust,ignore
pub struct ReconciliationResult {
    pub intrinsic_dirty: BTreeSet<usize>,  // bottom-up sizing needed
    pub layout_roots: BTreeSet<usize>,     // top-down pass roots
}
```

A viewport size change unconditionally adds `0` (the root) to `layout_roots`. Otherwise `reconcile_recursive` walks both trees in parallel and computes a `NodeDataFingerprint` for each new-DOM node:

```rust,ignore
pub struct NodeDataFingerprint {
    pub content_hash: u64,      // node_type
    pub state_hash: u64,        // hover/focus/active bits
    pub inline_css_hash: u64,
    pub ids_classes_hash: u64,
    pub callbacks_hash: u64,
    pub attrs_hash: u64,
}
```

`fingerprint.diff(old)` returns a `NodeChangeSet` (bitflags from `core/src/diff.rs:37`). `change_set.needs_layout()` triggers `DirtyFlag::Layout`; otherwise `DirtyFlag::Paint`. Pure additions get `DirtyFlag::Layout`. The original design (`scripts/INCREMENTAL_LAYOUT_ARCHITECTURE.md`) called for a per-property `RelayoutScope` plumbed all the way to a `ChangeAccumulator`; the implementation stopped at the multi-field fingerprint, which is enough for the common cases (text edits, hover, viewport resize). The `RelayoutScope` enum exists in `css/src/props/property.rs` but is not yet consumed by the solver.

After the recursive pass, redundant `layout_roots` are pruned: if a parent is already a root, its children are removed (the parent's top-down pass will re-position them).

## `LayoutCacheMap` — Taffy-style 9+1 slots

`cache.rs:NodeCache`. One per layout-tree node, kept in a flat `Vec<NodeCache>` parallel to `tree.nodes`. Each node has 9 measurement slots + 1 full-layout slot:

| Slot | Constraint shape |
|---|---|
| 0 | both width and height definite |
| 1, 2 | only width known (Definite/MaxContent vs MinContent) |
| 3, 4 | only height known (Definite/MaxContent vs MinContent) |
| 5–8 | neither known (2×2 combos) |

`SizingCacheEntry` stores `(available_size, result_size, baseline, escaped_top_margin, escaped_bottom_margin)` — no positions. `LayoutCacheEntry` (the +1 slot) adds `child_positions: Vec<(usize, LogicalPosition)>`, `content_size`, `scrollbar_info`. Slot index is deterministic from the constraint shape, so MinContent and Definite measurements never collide.

`cache_map.mark_dirty(idx, &nodes)` clears the node and walks ancestors; an ancestor whose own cache is already empty stops the walk (Taffy's optimisation — descendants would be re-cleared on the way down anyway).

### Cache remap on tree rebuild

`mod.rs:540` remaps `cache_map.entries` from old layout indices to new ones via two passes: first by `(dom_id → layout_idx)` on both trees, then anonymous wrappers (no DOM id) by `(parent_new_idx, ordinal)`. Without the second pass, anonymous box wrappers re-allocate empty every reconcile and invalidate their ancestors via `mark_dirty`.

## Formatting-context dispatch (`fc.rs:layout_formatting_context`)

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

`InlineBlock` always establishes a fresh BFC for its children even though it participates as an atomic inline in its parent IFC. Table-internal flex items are blockified at tree-build time (`blockify_flex_item_if_table_internal` in `layout_tree.rs`), so they arrive here as `Block`.

## BFC layout (`fc.rs:layout_bfc`)

CSS 2.2 § 9.4.1. Two-pass design:

1. **Pass 1 (`ComputeMode::ComputeSize`)** — measure every block-level child to get their border-box sizes and escaped margins. Stored in slots 0–8 of `NodeCache`. No positions are written.
2. **Pass 2 (`ComputeMode::PerformLayout`)** — stack children along the main axis, applying margin collapse (CSS 2.2 § 8.3.1). Writes positions into the +1 slot.

Margin collapse handling is the bulk of `layout_bfc`. The pen tracks accumulated top margin until a non-margin "blocker" (border, padding, content) resolves it; bottom margins of the last child can "escape" upward to the parent if no blocker intervenes. Floats from `float_cache` are placed exclusion-aware.

## IFC layout (`fc.rs:layout_ifc`)

CSS 2.2 § 9.4.2. The IFC root is the LayoutNode whose `inline_layout_result: Arc<CachedInlineLayout>` holds the shaped/positioned text. Descendant text nodes don't store their own — they record `IfcMembership { ifc_id, ifc_root_layout_index, run_index }` (`layout_tree.rs:71`) pointing back at the root.

```rust,ignore
pub struct CachedInlineLayout {
    pub layout: Arc<UnifiedLayout>,        // glyph runs, positions
    pub available_width: AvailableSpace,   // cache key
    pub has_floats: bool,
    pub constraints: Option<UnifiedConstraints>,
    pub item_metrics: Vec<InlineItemMetrics>,
    pub line_breaks: Option<CachedLineBreaks>,
}
```

`available_width` and `has_floats` are the cache-validity key — a layout shaped under min-content cannot be reused for the final pass. `item_metrics` and `line_breaks` enable incremental reshape (Phase 2c/d in `INCREMENTAL_LAYOUT_ARCHITECTURE.md`); the current path uses them as a cache-hit fast path. Real per-character incremental relayout for text edits lives in `LayoutWindow::try_incremental_text_relayout` and bypasses `layout_ifc` entirely.

`IfcId` (`layout_tree.rs:29`) is generated from a global `AtomicU32` counter that resets at the start of each `layout_document` call (`mod.rs:424`). Stable IDs across frames depend on stable DOM structure — same DOM → same IFC IDs.

## Flex/Grid via Taffy (`taffy_bridge.rs`)

`solver3` does not implement flex or grid directly. `layout_flex_grid` constructs a `TaffyBridge<'a, T>` over the current sub-tree and calls Taffy's `compute_root_layout`. The bridge implements:

- `TraversePartialTree` — child enumeration over `LayoutTree`
- `LayoutPartialTree` — `compute_child_layout` dispatches non-flex/grid children back to `layout_formatting_context` (i.e., Taffy calls into solver3 for block/inline children of a flex item)
- `CacheTree` — uses Taffy's per-node cache, separate from the 9+1 slots
- `LayoutFlexboxContainer` and `LayoutGridContainer` — invoke Taffy's flex/grid algorithms

Azul CSS values are translated to Taffy's `Style` per call. `from_layout_width` maps `LayoutWidth::Px(PixelValue)` to `taffy::Dimension`: absolute units (px/pt/em/rem/in/cm/mm) resolve against `DEFAULT_FONT_SIZE` for em/rem fallback; percentages map to `Dimension::percent(p)`; viewport units (`vw`/`vh`/…) currently fall back to `Dimension::auto()` because the Taffy bridge has no viewport context. This is documented in `scripts/PERCENTAGE_LAYOUT_ANALYSIS.md`.

```rust,ignore
fn from_layout_width(val: LayoutWidth) -> Dimension {
    match val {
        LayoutWidth::Auto => Dimension::auto(),
        LayoutWidth::Px(px) => match pixel_value_to_pixels_fallback(&px) {
            Some(pixels) => Dimension::length(pixels),
            None => match px.to_percent() {
                Some(p) => Dimension::percent(p.get()),
                None => Dimension::auto(),
            },
        },
        LayoutWidth::MinContent => Dimension::min_content(),
        LayoutWidth::MaxContent => Dimension::max_content(),
    }
}
```

The `ProgressBar` widget (`layout/src/widgets/progressbar.rs`) historically used a `flex-grow: 10000000` hack to simulate percentage widths because the bridge previously returned `None` for `SizeMetric::Percent`. Taffy now supports them; the hack is documented as removable in `PERCENTAGE_LAYOUT_ANALYSIS.md` § Phase 1.

## Sizing pass (`sizing.rs:calculate_intrinsic_sizes`)

Bottom-up, ancestor-closure-pruned. `dirty_closure` is the union of `dirty_nodes` and every ancestor up to root. A node not in the closure with a populated `intrinsic_sizes` skips its entire subtree walk — before the closure was added, every render walked the full tree from root, costing ~2 ms even when 3 nodes were dirty.

`tree.subtree_needs_intrinsic` (a static-DOM bitmap precomputed at tree-build time) is true if the node or any descendant establishes a shrink-to-fit context. When the caller is non-STF and the subtree is non-STF too, no one will ever read the intrinsic — the descent is skipped entirely.

## Positioning pass (`positioning.rs`)

Run in this exact order (`mod.rs:884`):

1. `adjust_relative_positions` — `position: relative` offsets applied AFTER sizing, so auto-height calculation in the BFC sees normal-flow positions (CSS 2.2 § 9.4.3).
2. `adjust_sticky_positions` — clamp against scroll offset and inset properties relative to the nearest scrollport. Run after relative because sticky elements establish containing blocks for absolute descendants.
3. `position_out_of_flow_elements` — `position: absolute / fixed`. Run after relative because absolute elements are positioned relative to the *post-adjustment* containing block.

Inverting any of these breaks specific CSS behaviour; the comments in `mod.rs:870–920` document which spec sections are at stake.

## Scrollbar reflow loop

`mod.rs:690`. Adding a vertical scrollbar reduces the available width for the next pass; the new layout may itself produce different scrollbars. The loop runs up to `MAX_SCROLLBAR_REFLOW_ITERATIONS = 10` (`mod.rs:137`) before bailing with a debug warning. Each iteration that flips `reflow_needed_for_scrollbars` does:

```rust,ignore
recon_result.layout_roots.clear();
recon_result.layout_roots.insert(new_tree.root);
recon_result.intrinsic_dirty = (0..new_tree.nodes.len()).collect();
```

— a full re-pass.

## Display-list cache and pointer-identity fast path

Two cache levels in `mod.rs`:

1. **Pointer-identity** (`mod.rs:463`) — `dom_ptr == cache.prev_dom_ptr && viewport == cache.prev_viewport && cached_display_list.is_some()`. Returns the cached DL without even running reconcile.
2. **Structural-identity** (`mod.rs:492`) — after reconcile, compare the new root's `subtree_hash` against the cached one. Returns the cached DL but pays the ~600 µs reconcile cost.

The structural cache fires whenever the DOM is structurally unchanged but a new `StyledDom` instance was passed (e.g., user's `layout_callback` returned a fresh `StyledDom::clone`). It saves the ~4 ms display-list emission step.

## `LayoutWindow` — the per-window orchestrator (`window.rs`)

`LayoutWindow` (`window.rs:374`) owns:

- `layout_cache: Solver3LayoutCache` and `text_cache: TextLayoutCache`
- `font_manager: FontManager<FontRef>`, `image_cache: ImageCache`, `renderer_resources`
- `layout_results: BTreeMap<DomId, DomLayoutResult>` — one per DOM (root + virtual views)
- All input-side managers: `scroll_manager`, `focus_manager`, `text_edit_manager`, `text_input_manager`, `hover_manager`, `gesture_drag_manager`, `clipboard_manager`, `drag_drop_manager`, `a11y_manager`, `gpu_state_manager`, `virtual_view_manager`
- `dirty_text_nodes: BTreeMap<(DomId, NodeId), DirtyTextNode>` — in-progress text edits not yet committed back to the DOM
- `pending_lifecycle_events`, `pending_unmount_invocations` — produced by `regenerate_layout`, drained by the shell
- `epoch: Epoch` and `gl_texture_cache: GlTextureCache` for WebRender resource cleanup

Three constructors share most of the body: `new(fc_cache)` for screens, `new_with_shared_fonts(fc_cache, parsed_fonts)` for shared font pools, and `new_paged(fc_cache, page_size)` for PDF (under `feature = "pdf"`). The duplication is flagged in `doc/target/autoreview/reports/layout__src__window.md` for cleanup; today each constructor sets `~50` fields verbatim.

### `layout_and_generate_display_list`

`window.rs:790`. The shell-level entry point. In order:

1. `layout_results.clear()` — full relayout, drops previous results.
2. `virtual_view_manager.reset_all_invocation_flags()` — without this, `VirtualViewManager` still has `was_invoked=true` from the previous frame, but the child DOM was just destroyed.
3. Recursive `layout_dom_recursive` — runs font resolution, builds GPU value cache, calls `solver3::layout_document`, registers scrollbar `TransformKey`/`OpacityKey` into the GPU cache, scans for `VirtualView` placeholders and recurses into them.
4. (with `feature = "a11y"`) `update_a11y_tree`.
5. `scroll_focused_cursor_into_view`.

### Font resolution skip via `font_stacks_hash`

`window.rs:887`. Before calling `layout_document`, `LayoutWindow` decides whether the font-resolution pipeline can be skipped entirely. Two signals:

1. `compact_cache.font_dirty_nodes.len() == 0` (set by `build_compact_cache` when no node's `font_family_hash` changed) AND `font_chain_cache` is non-empty.
2. A polynomial rolling hash over `prev_font_hashes` matches `font_manager.last_resolved_font_stacks_sig` — catches the case where `build_compact_cache` did not re-run but the font stacks are identical to what's already cached.

If both fail, `collect_and_resolve_font_chains_with_registration` runs the full 5-step pipeline (collect → resolve → diff → load from disk → set chain cache). The original design used a single XOR fingerprint (`font_stacks_hash`) of all per-node `font_family_hash` values; XOR is collision-prone (XOR(a, b, a, b) == 0) and does not survive removing+adding the same font in one frame. The current per-node dirty list plus rolling-hash fallback fixes both. See `scripts/FONT_INVALIDATION_AND_MEMORY_LAYOUT_ANALYSIS.md` for the full discussion.

The legacy `font_stacks_hash: u64` field on `LayoutWindow` (`window.rs:481`) is no longer the primary signal; it remains for compatibility with code paths that do not yet read `compact_cache.font_dirty_nodes`.

### Memory layout (per node, ballpark)

Numbers from `scripts/FONT_INVALIDATION_AND_MEMORY_LAYOUT_ANALYSIS.md` § 2.3:

| Structure | B/node | Access | Layout |
|---|---:|---|---|
| `CompactLayoutCache.tier1_enums` | 8 | hot | SoA |
| `CompactLayoutCache.tier2_dims` | 96 | hot | SoA |
| `CompactLayoutCache.tier2b_text` | 24 | warm | SoA |
| `LayoutNode` (post hot/warm/cold split) | ~90 hot, ~470 cold | hot/cold | split |
| `NodeCache` (9+1 slots) | ~260 | hot | SoA |
| `calculated_positions` | 8 | hot | SoA |
| StyledDom Vecs | ~150 | warm | SoA |

≈ 1 KiB per node. 10 K nodes ≈ 10 MiB resident. Cold-data fields stay out of the hot working set after the split.

## Adding a new CSS property

A property that does not affect layout (color, decoration) only needs cascade and getter wiring:

1. Add it to `CssProperty` and `CssPropertyType` in `css/src/props/property.rs`.
2. Implement `relayout_scope()` and `is_gpu_only_property()` if applicable.
3. Add a getter in `solver3/getters.rs` if any solver code reads it.
4. Hash it under `inline_css_hash` in `NodeDataFingerprint::compute` so it triggers `DirtyFlag::Paint` on change.

A property that changes geometry needs to be read in `sizing.rs` or one of the `fc.rs:layout_*` paths and must be classified as `RelayoutScope::SizingOnly`/`Full` so the change set marks `INLINE_STYLE_LAYOUT`.

## Known divergence from design docs

`scripts/INCREMENTAL_LAYOUT_ARCHITECTURE.md` proposed a unified `ChangeAccumulator` in `core/src/diff.rs` plumbing per-property `RelayoutScope` from restyle, runtime edits, and DOM rebuild into a single `ProcessEventResult::ShouldIncrementalRelayout`. The implementation stopped at the `NodeDataFingerprint` — fingerprint diffs map to a binary `DirtyFlag::Layout`/`Paint`, not the 4-level `RelayoutScope`. Hover-only restyle still triggers a full DOM rebuild via `ShouldRegenerateDomCurrentWindow`. The hooks (`RestyleResult.max_relayout_scope`, `CallbackChangeResult.css_properties_changed`) exist but are unread by the layout engine.
