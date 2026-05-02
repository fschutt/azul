---
slug: layout-solver
title: Layout Solver (Flex/Grid)
language: en
canonical_slug: layout-solver
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: [code-organization, dom, css-properties]
tracked_files:
  - layout/src/lib.rs
  - layout/src/window.rs
  - layout/src/solver3/mod.rs
  - layout/src/solver3/cache.rs
  - layout/src/solver3/fc.rs
  - layout/src/solver3/layout_tree.rs
  - layout/src/solver3/sizing.rs
  - layout/src/solver3/positioning.rs
  - layout/src/solver3/display_list.rs
  - layout/src/solver3/taffy_bridge.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:32:10Z
---

# Layout Solver (Flex/Grid)

> **WIP** — `solver3` is the active engine; APIs may shift between releases. The block/inline path is azul-native; flex/grid delegate to [Taffy](../../../../layout/src/solver3/taffy_bridge.rs).

The solver lives at [`layout/src/solver3/`](../../../../layout/src/solver3/). The single entry point is [`layout_document`](../../../../layout/src/solver3/mod.rs) at `layout/src/solver3/mod.rs:402`. It takes a borrowed `StyledDom`, mutates a `LayoutCache` in place, and returns a `DisplayList`. The cache is keyed on the previous frame's tree and survives between calls — a clean re-render with an unchanged DOM hits the structural-identity fast path and returns the cached display list verbatim.

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
) -> Result<DisplayList>
```

The orchestrator is [`LayoutWindow::layout_and_generate_display_list`](../../../../layout/src/window.rs) at `layout/src/window.rs:790`. Platform shells call that once per frame; it walks the DOM forest (root + each iframe) and dispatches to `layout_document` per-DOM.

## Pipeline stages

`layout_document` runs five phases. Cache fast-paths short-circuit at the top.

| Step | Function | Purpose |
|---|---|---|
| 0 | pointer-identity check | same `&StyledDom` + viewport → return cached display list |
| 1 | `cache::reconcile_and_invalidate` | diff old vs new tree, mark dirty subtrees |
| 1.1 | subtree-hash check | unchanged root subtree + viewport → return cached display list |
| 1.3 | `cache::compute_counters` | resolve `counter-reset` / `counter-increment` |
| 1.4 | `LayoutCacheMap` remap | move per-node cache slots from old layout idx → new layout idx |
| 1.5 | early exit | clean tree → regenerate display list only |
| 2 | `sizing::calculate_intrinsic_sizes` | bottom-up min/max-content widths for dirty subtrees |
| 2 | `cache::calculate_layout_for_subtree` | top-down layout for each layout root |
| 2 | `cache::reposition_clean_subtrees` | shift unchanged siblings without re-layout |
| 3 | `positioning::adjust_relative_positions` | apply `position: relative` offsets |
| 3.25 | `positioning::adjust_sticky_positions` | clamp sticky elements based on scroll |
| 3.5 | `positioning::position_out_of_flow_elements` | absolute / fixed positioning |
| 3.75 | `LayoutWindow::compute_scroll_ids` | hash-stable scroll IDs for damage tracking |
| 4 | `display_list::generate_display_list` | flatten to paint commands |

The probe spans (`probe::Probe::span("…")`) bracket each phase; with `AZ_PROFILE=1` the timings land in `target/azul-probes/`.

## LayoutContext

Every phase takes `&mut LayoutContext` ([`layout/src/solver3/mod.rs:201`](../../../../layout/src/solver3/mod.rs)). It owns the per-pass scratch state:

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
    pub scrollbar_style_cache: RefCell<HashMap<NodeId, ComputedScrollbarStyle>>,
}
```

`cache_map` is moved into the context with `std::mem::take` for the duration of the pass and moved back into `LayoutCache` before return. This keeps the borrow checker happy while letting child phases own `&mut` access.

## LayoutTree

[`solver3::layout_tree::LayoutTree`](../../../../layout/src/solver3/layout_tree.rs) is the solver's working tree. It is *not* the styled DOM — it inserts anonymous boxes (CSS 2.1 §9.2.1) wherever a block-level container has both block and inline children, and it splits inline-level whitespace runs at boundaries. The tree is partitioned into hot/warm/cold slabs:

| slab | contents | accessed |
|---|---|---|
| `LayoutNodeHot` | parent index, formatting context, dom node id | every traversal |
| `LayoutNodeWarm` | box props, taffy cache, IFC layout result | layout phases |
| `LayoutNodeCold` | subtree hash, fingerprints, css computed values | reconcile + diagnostics |

`LayoutTreeBuilder` rebuilds the tree from a `StyledDom`. Reconciliation reuses warm and cold slabs from the old tree where the `(NodeDataFingerprint, css_hash)` pair matches.

## The 9+1 cache

[`cache::NodeCache`](../../../../layout/src/solver3/cache.rs) stores per-node measurements. Each node has 9 sizing slots and 1 full-layout slot, indexed deterministically by the `(width_known, height_known, AvailableWidthType)` tuple — the same scheme Taffy uses, so a `MinContent` measurement never collides with a `Definite(0.0)` measurement of the same node.

```rust,ignore
pub struct NodeCache {
    pub measure_entries: [Option<SizingCacheEntry>; 9],
    pub layout_entry: Option<LayoutCacheEntry>,
    pub is_empty: bool,
}

pub enum AvailableWidthType { Definite, MinContent, MaxContent }
```

The cache is external to `LayoutTree` — `LayoutCacheMap.entries[i]` corresponds to `LayoutTree.nodes[i]`. This keeps `LayoutNode` slim and lets the cache resize independently.

`mark_dirty` walks parent chain and stops at the first ancestor whose `is_empty == true`, since that ancestor's ancestors must already be dirty too. This is Taffy's early-stop optimization.

`get_size` and `get_layout` implement the "result matches request" check: if a Pass-1 measurement returned size `S`, and Pass-2 then asks for layout with `available_size == S`, the entry hits even though `available_size` doesn't equal the original input. This is what makes two-pass layout O(n) instead of O(n²).

## Reconciliation

[`cache::reconcile_and_invalidate`](../../../../layout/src/solver3/cache.rs) emits a `ReconciliationResult`:

```rust,ignore
pub struct ReconciliationResult {
    pub intrinsic_dirty: BTreeSet<usize>,  // bottom-up resize roots
    pub layout_roots: BTreeSet<usize>,     // top-down layout roots
    pub paint_dirty: BTreeSet<usize>,      // paint-only nodes
}
```

A node is `intrinsic_dirty` when its content (text, images) changed. A node is a `layout_root` when its containing-block size or its position-in-parent changed. A node is `paint_dirty` when only background, border, or color changed — those don't affect geometry, so the layout pass is skipped and only `generate_display_list` re-runs.

`paint_dirty.is_empty() && layout_dirty.is_empty()` means a no-op frame — the cached display list is returned verbatim.

## Formatting contexts

`solver3::fc::layout_formatting_context` ([`layout/src/solver3/fc.rs`](../../../../layout/src/solver3/fc.rs)) dispatches by `FormattingContext`:

| variant | implementation | notes |
|---|---|---|
| `Block` | `layout_block` (in `fc.rs`) | margin collapsing via `MarginCollapseContext`, floats via `FloatingContext` |
| `Inline` | delegates to `text3` via `text3::cache::perform_fragment_layout` | covered in [Inline Layout and Text Shaping](inline-text3.md) |
| `Flex` | `taffy_bridge::layout_flex` | thin wrapper over Taffy |
| `Grid` | `taffy_bridge::layout_grid` | thin wrapper over Taffy |
| `Table` / `TableRowGroup` / `TableRow` / `TableCell` | `layout_table` (in `fc.rs`) | full CSS 2.1 §17 algorithm |

The block engine handles CSS floats (CSS 2.2 §9.5) natively. `BfcState` holds the `FloatingContext` for the BFC, and `available_line_box_space` tells the inline engine how wide a line box can be at a given y-coordinate before clipping into a float's margin box.

## The Taffy bridge

`taffy_bridge.rs` translates between azul's `LayoutTree` and Taffy's `Node` graph. For every node in a flex or grid container we synthesize a Taffy node, register a `MeasureFn` that calls back into `text3` for content measurement, and run Taffy's layout. Computed positions and sizes are written back into `LayoutTree.nodes[i].used_size` and `calculated_positions[i]`.

Taffy's per-node cache is cleared whenever the corresponding azul node is marked intrinsic-dirty — see the loop at `solver3/mod.rs:503`. Without this, Taffy can return stale measurements after a text edit.

## Scrollbar reflow loop

A scrollbar appearing or disappearing changes the available width, which can shift line breaks, which can shift content height, which can change scrollbar visibility again. The main loop guards against this:

```rust,ignore
const MAX_SCROLLBAR_REFLOW_ITERATIONS: usize = 10;

loop {
    loop_count += 1;
    if loop_count > MAX_SCROLLBAR_REFLOW_ITERATIONS { break; }
    let mut reflow_needed_for_scrollbars = false;
    // … layout pass …
    if !reflow_needed_for_scrollbars { break; }
    recon_result.layout_roots.clear();
    recon_result.layout_roots.insert(new_tree.root);
    recon_result.intrinsic_dirty = (0..new_tree.nodes.len()).collect();
}
```

The flag is set inside `calculate_layout_for_subtree` when `ScrollbarRequirements` change between the previous and current pass. Hitting the limit emits a debug warning and falls through with the last computed positions — this is a real-world layout that genuinely cannot stabilize, not a bug.

## Containing-block resolution

`get_containing_block_for_node` ([`solver3/mod.rs:987`](../../../../layout/src/solver3/mod.rs)) implements CSS 2.2 §10.1. The CB for an in-flow box is its parent's content-box; the CB for the root element is the viewport (initial containing block). Percentage sizes resolve against this rectangle. The function is called with the *parent's already-computed* margin-box position, and adds parent border + padding to derive the content-box. Margins live on the child, not the parent's content-box.

## Display list generation

[`display_list::generate_display_list`](../../../../layout/src/solver3/display_list.rs) walks the laid-out tree in paint order and emits a flat `Vec<DisplayListItem>`:

```rust,ignore
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
    // …
}
```

Items are absolute window-logical pixels. HiDPI scaling and scroll-offset translation happen later in the compositor (`dll/src/desktop/wr_translate2.rs` for WebRender, `layout/src/cpurender.rs` for CPU). The display list embeds:

- background colors and gradients
- borders (with per-side style/width/color)
- box shadows and filter effects
- text glyph runs (one `PushText` per shaped fragment)
- image references (resolved against `RendererResources` to a WebRender `ImageKey`)
- scrollbars and scroll-clip rects
- selection highlights and caret rectangles
- hit-test tags (encoded `(NodeId, tag_type)` for the compositor)

The list is returned to the caller and cached on `LayoutCache.cached_display_list = Some((root_subtree_hash, viewport, display_list))`. Step 1.1 of the next call hits this when nothing structural changed.

## When does the layout pass actually run?

Concretely, `layout_and_generate_display_list` is called from:

- `dll/src/desktop/shell2/common/event.rs` — every frame on render-required state
- `LayoutWindow::record_text_input` → `apply_text_changeset` — text-edit fast path
- `LayoutWindow::scroll_focused_cursor_into_view` — after layout to fix cursor visibility
- `dll/src/desktop/native_screenshot.rs` — for headless screenshots

The text-edit fast path is special: it patches the affected text node in place, then sets `dirty_text_overrides` on the next `LayoutContext` so layout reads the edited string instead of going back to the DOM. See [LayoutWindow Internals](#) (TODO) for the changeset machinery.

## See also

- [Inline Layout and Text Shaping](inline-text3.md) — what `FormattingContext::Inline` actually runs
- [Fragmentation](fragmentation.md) — paged-media break logic on top of the same solver
- [CSS Property Internals](css-properties.md) — how `CssProperty` flows into `LayoutContext`
- [Hit Testing and Scrolling Internals](hit-testing.md) — how display-list output drives hit tests
