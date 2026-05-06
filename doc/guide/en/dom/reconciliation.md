---
slug: dom/reconciliation
title: Reconciliation
language: en
canonical_slug: dom/reconciliation
audience: external
maturity: mature
guide_order: 31
topic_only: false
short_desc: Diffing, restyle scope, and damage-rect repaint
prerequisites: [dom]
tracked_files:
  - core/src/diff.rs
  - core/src/styled_dom.rs
  - css/src/props/property.rs
  - layout/src/cpurender.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Reconciliation

Azul rebuilds the `Dom` from scratch on every `RefreshDom`. The tree returned by `layout()` is frozen. The next state change runs `layout()` again and produces a new tree. The diff is what makes that affordable.

Three things happen between the new tree and the old one. First, each new node gets matched against an old one. Second, `compute_node_changes` classifies what's different on each matched pair into a 13-bit `NodeChangeSet`. Third, the changed CSS properties get classified into a four-level `RelayoutScope` so the layout engine can pick the cheapest path.

After layout, the renderer compares the new display list against the old one. `compute_display_list_damage` returns the rectangles that need re-rasterising. The rest of the framebuffer keeps its previous pixels.

The result: a hover highlight on one button repaints one rectangle. Tree size doesn't matter.

## The diff pass

The entry point is `reconcile_dom` in `core/src/diff.rs`. It takes the old and new node arrays, hierarchies, and layout rects. It returns a `DiffResult`:

```rust,ignore
pub struct DiffResult {
    pub events:     Vec<SyntheticEvent>,    // Mount, Unmount, Resize, Update
    pub node_moves: Vec<NodeMove>,          // old NodeId <-> new NodeId
}
```

Every node in the new tree is one of four things:

- **Stable**. Matched against an old node at the same logical position. Old node id maps to new node id. Merge callbacks run. Focus and scroll state migrate.
- **Moved**. Matched against an old node at a different parent or sibling position. Same migration treatment as Stable.
- **Created**. No match. A Mount event fires.
- **Destroyed**. The old tree had a node here, the new tree doesn't. An Unmount event fires. The dataset drops.

Matching tries three tiers in priority order. An old node is consumed as soon as one tier hits.

### Tier 1 - reconciliation key

`calculate_reconciliation_key` in `core/src/diff.rs` packs the node's logical identity into a `u64`. It checks three sources in strict priority:

1. **Explicit key**. Anything passed to `Dom::with_key(k)`. This is the strongest signal. If a new node has a key with no match in the old tree, it's classified as Created. There's no fall-through to the coarser tiers. An explicit key is an author assertion of identity.
2. **CSS ID**. Set via `with_id("my-thing")`. Hashed into the same `u64` slot.
3. **Structural key**. A recursive hash of `(node-type-discriminant, classes, nth-of-type-within-parent, parent's reconciliation key)`.

The structural-key fallback is what makes reconciliation work without explicit keys for trees with stable order. The third `<li>` inside a `<div id="notes">` produces the same key in both old and new trees. Its text content can change and it'll still match.

For trees whose order can change, you need explicit keys. Sortable lists, drag-and-drop, virtualised scrolling. Without keys you'll get "Destroyed plus Created" pairs where you wanted "Moved".

### Tier 2 - content hash

If Tier 1 misses (and the node has no explicit key), the framework checks a `DomNodeHash` covering the full node-data: type, attributes, callbacks, inline style, dataset discriminator. A hit here means an old node with identical content exists somewhere. This catches pure reorders of anonymous nodes.

### Tier 3 - structural hash

Last resort. A hash of `(discriminant, attrs)` that ignores text content. This is the text-edit fallback. `<span>Hello</span>` becomes `<span>Hellp</span>`. The content hash differs but the structural hash matches. Cursor and selection state migrate across.

A Tier-2 or Tier-3 match never fires an `Update` event. Tier 2 matched on identical visual content. Tier 3 matched on text-edit semantics that other code handles. Only Tier-1 matches fire `Update`, because only Tier 1 establishes "same logical thing, different content".

## What changed about a matched node

`compute_node_changes` in `core/src/diff.rs` walks the two `NodeData`s field by field. It returns a 13-bit `NodeChangeSet`. The flags split into three bands.

**Layout-affecting**. Any of these forces a relayout:

- `NODE_TYPE_CHANGED`. `Text` became `Image`, etc.
- `TEXT_CONTENT`. Text inside a `Text` node changed.
- `IDS_AND_CLASSES`. The class list changed. Different selectors may match.
- `INLINE_STYLE_LAYOUT`. Inline CSS that affects layout: `width`, `padding`, `margin`, `display`.
- `CHILDREN_CHANGED`. Direct children added, removed, or reordered.
- `IMAGE_CHANGED`. Image source changed. Intrinsic size may differ.
- `CONTENTEDITABLE`. The flag toggled.

**Paint-affecting**. Repaint without relayout:

- `INLINE_STYLE_PAINT`. Inline CSS that affects paint: `color`, `background`, `opacity`, `box-shadow`.
- `STYLED_STATE`. Pseudo-class state changed: `:hover`, `:focus`, `:active`.

**Neither**. No visual change:

- `CALLBACKS`. Different `RefAny` or different handlers. The user sees no callback.
- `DATASET`. Node-attached state changed.
- `ACCESSIBILITY`. A11y description or role changed.
- `TAB_INDEX`. Focus order changed. The focus model picks this up. The pixels don't.

`NodeChangeSet::needs_layout()` and `needs_paint()` collapse the bitmask. They check against the `AFFECTS_LAYOUT` and `AFFECTS_PAINT` composite masks defined alongside the flags. Those masks are the source of truth, not any single flag.

A node whose `NodeChangeSet` has only `CALLBACKS | DATASET` set runs through the diff. Its merge callback fires if needed. No layout work runs. No paint work runs. The pixels on screen are byte-identical to the previous frame.

## Promoting changes to a `RelayoutScope`

"Layout changed" is too coarse. Setting `color: red` flips `INLINE_STYLE_PAINT`. Setting `display: flex` flips `INLINE_STYLE_LAYOUT`. The cost of reacting is wildly different. Re-running flexbox on a 1,000-node subtree because someone changed an opacity is unacceptable.

`RelayoutScope` in `css/src/props/property.rs` is a four-level cost classification. `CssPropertyType::relayout_scope` returns one of:

- **`None`**. Repaint only. Color, background, opacity, transform, caret styling, object-fit, clip path. Zero layout work. The display list updates, damage rects are computed, the affected rectangles re-rasterise.
- **`IfcOnly`**. Only the inline formatting context containing the node needs re-shaping. Font-style flip, vertical-align change inside a paragraph, letter-spacing update. Block-level siblings stay where they are. If the IFC's height changes, this auto-upgrades to `SizingOnly`.
- **`SizingOnly`**. This node's size needs recomputing. The parent may need to reposition subsequent siblings. No recursive relayout. Border-width tweak, scrollbar gutter, padding inside a text container.
- **`Full`**. The whole subtree needs re-laying out. Flips of `display`, `position`, `float`. Anything that changes the formatting context.

The source comment is explicit about the comparison: this is a deliberate improvement on Taffy's binary clean/dirty flag. A binary classifier upgrades every change to "full subtree relayout". Four levels let an opacity hover land in `None` and take the cheap path.

`RestyleResult::max_relayout_scope` (in `core/src/styled_dom.rs`) tracks the highest scope across all property changes for a frame. The layout engine reads it and picks the cheapest correct path. When `max_relayout_scope <= IfcOnly`, the engine skips `calculate_layout_for_subtree` and runs the IFC fast path.

`StyledDom::restyle_on_state_change` is the path for hover, focus, and active state changes. Each property that flips contributes its `relayout_scope`. The result keeps the maximum. A `:hover` that only changes `background-color` ends with `max_relayout_scope = None`. Repaint a damage rect, done.

## Lazy paint - damage rects

Even after restyle decides "repaint only", azul doesn't paint the whole window. The renderer compares the new display list against the previous one item by item. It returns the rectangles that need re-rasterising.

`compute_display_list_damage` in `layout/src/cpurender.rs`:

```rust,ignore
pub fn compute_display_list_damage(
    old: &DisplayList,
    new: &DisplayList,
) -> Option<Vec<LogicalRect>>
```

The return type carries the policy:

- `Some(rects)`. Those rectangles need repainting. Everything else keeps the previous frame's pixels.
- `None`. Full repaint. Item count differs, or two items at the same index have different discriminants. The rect-by-rect comparison can't safely express the change.

The comparison is conservative. Every display-list item exposes `is_visually_equal` and `visual_bounds`. A change inside the same bounds (a colour swap, a text edit) produces a damage rect covering both the old and new bounds. Nothing overdraws onto stale pixels. Nothing under-paints.

`coalesce_damage_rects` then merges overlapping or near-adjacent rects. The threshold is 8 logical pixels. The merge is `O(n^2)`. Typical damage counts are well under twenty, so the constant factor is fine. The result is a small list of well-spaced rectangles instead of dozens of slivers that would each cause a redundant repaint.

Two specialised producers feed the same pipeline:

- `compute_resize_damage` handles a grow-only window resize. It produces just the right strip and bottom strip. No full re-rasterise.
- The macOS shell tracks a per-frame `gpu_damage_rects` field that's fed back to the GPU presenter. The OS compositor only invalidates those rects. This saves copy bandwidth on Retina displays.

The per-frame flow:

1. `layout()` returns a fresh `Dom`.
2. `reconcile_dom` matches old to new, fires lifecycle events, produces `NodeChangeSet`s.
3. Restyle promotes the changes to a `RelayoutScope`. If `max_relayout_scope == None`, no layout pass runs.
4. The display list is rebuilt for the parts that changed.
5. `compute_display_list_damage` produces the damage rects.
6. The renderer rasterises only those rects.

For a hover-state change on a button, steps 3 to 5 are constant-time. Step 6 paints maybe 100 by 30 pixels.

## When the framework matches old to new

The per-node flow during a `RefreshDom`:

- The diff classifies the node as Stable, Moved, Created, or Destroyed.
- For Stable and Moved: `compute_node_changes` produces a `NodeChangeSet`. The merge callback (if registered) runs here, in `transfer_states`. It claims resources from the old dataset before the old tree drops. See [Merge Callbacks](merge-callbacks.md) for the protocol.
- For Created: the new node is mounted. The Mount lifecycle event fires if registered. No merge runs.
- For Destroyed: the old node and its dataset drop. The Unmount lifecycle event fires. Heavy resources owned by the dataset clean up via `Drop` in the natural way.

The same `NodeChangeSet` that drives restyle and layout decisions is exposed as `ExtendedDiffResult.node_changes` in `core/src/diff.rs`. The live debugger reads it to highlight which nodes actually changed in the last frame.

## Where to read the source

- `core/src/diff.rs` - `NodeChangeSet`, `compute_node_changes`, `calculate_reconciliation_key`, `DiffResult`, `reconcile_dom`, `transfer_states`.
- `css/src/props/property.rs` - `RelayoutScope` (the four-level enum) and `CssPropertyType::relayout_scope`.
- `core/src/styled_dom.rs` - `RestyleResult` with `max_relayout_scope`, `restyle_on_state_change`.
- `layout/src/cpurender.rs` - `compute_display_list_damage`, `coalesce_damage_rects`, `compute_resize_damage`.
