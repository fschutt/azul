---
slug: dom/reconciliation
title: Reconciliation, Diffing, and Lazy Paint
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

# Reconciliation, Diffing, and Lazy Paint

Azul's `Dom` is [frozen the moment `layout()` returns it](../dom.md). The
framework never mutates the tree you handed it; the next state change
runs `layout()` again and produces a *fresh* tree. That sounds expensive
— a million-node tree rebuilt on every keystroke — and would be if every
fresh tree triggered a full restyle, relayout, and repaint.

It doesn't. Between the new tree and the old one sits a reconciliation
pass that:

- matches each new node against an old one (so focus, scroll position,
  and per-node datasets travel across),
- classifies *what specifically* changed about each matched node — text,
  classes, inline style, callbacks — into a 13-bit `NodeChangeSet`,
- promotes the changes to a four-level `RelayoutScope` so the layout
  engine can pick `repaint-only` / `IFC-only` / `sizing-only` / `full`,
- and finally diffs the resulting display list to produce **damage
  rects**: a list of rectangles that need re-rasterising. Anything
  outside those rects keeps the previous frame's pixels.

The result is that a one-pixel hover highlight on a single button
re-paints a single rectangle, regardless of how big the surrounding
tree is. This page walks the three layers — diff, scope, damage — and
points at the file:line locations that drive each one.

## The diff pass

The entry point is `reconcile_dom` (`core/src/diff.rs:459`). It takes
the old and new node arrays, the old and new hierarchies, the old and
new layout rectangles, and produces a `DiffResult`:

```rust,ignore
pub struct DiffResult {
    pub events:     Vec<SyntheticEvent>,    // Mount, Unmount, Resize, Update
    pub node_moves: Vec<NodeMove>,          // old NodeId ↔ new NodeId
}
```

Every node in the new tree falls into one of four categories:

- **Stable** — matched against an old node at the same logical
  position. Old node id ↔ new node id; the framework will run merge
  callbacks, migrate focus and scroll state, etc.
- **Moved** — matched against an old node, but at a different parent
  or sibling order. Same merge / migration treatment as Stable.
- **Created** — no match in the previous tree. Mount event fires.
- **Destroyed** — old tree had a node here, the new tree does not.
  Unmount event fires; the node and its dataset drop normally.

Matching uses three tiers, in priority order. A node is consumed from
the old tree as soon as one tier hits.

### Tier 1 — reconciliation key

`calculate_reconciliation_key` (`core/src/diff.rs:331`) encodes the
node's logical identity into a single `u64`. It tries three sources, in
strict priority:

1. **Explicit key** — anything you passed to `Dom::with_key(k)`. This
   is the strongest signal: if a new node has a `with_key` and no old
   node has the same key, the framework gives up on that node and
   classifies it as Created. There is no fall-through to coarser
   tiers — an explicit key is "I, the author, am asserting identity",
   and the diff respects that.
2. **CSS ID** — anything set via `with_id("my-thing")`. Hashed into the
   same `u64` slot.
3. **Structural key** — `(node-type-discriminant, classes,
   nth-of-type-within-parent, parent's reconciliation key)` hashed
   recursively. This is what lets a list of `<li>`s reconcile correctly
   when the user types a character: the `<li>` at position 3 in a parent
   div with id `notes` produces the same key in the old tree and the
   new tree, even though its text content changed.

The structural-key fallback is what makes the diff work *without* keys
for trees whose order is fixed. For trees whose order can change —
sortable lists, drag-and-drop, virtualised list with scroll — pass an
explicit `with_key` per item or you'll see "Destroyed + Created" pairs
where you wanted "Moved".

### Tier 2 — content hash

If Tier 1 doesn't produce a match (and the node has no explicit key),
the framework falls back to a `DomNodeHash` that covers the *full*
node-data: type, attributes, callbacks, inline style, dataset
discriminator, the lot. A hit here means "we found an old node with
identical content somewhere", which catches pure reorders of anonymous
nodes — siblings in a flexbox swapping positions, for instance.

### Tier 3 — structural hash

Last resort: a hash of `(discriminant, attrs)` that *ignores* text
content. This is the text-edit fallback: a `<span>Hello</span>` becomes
`<span>Hellp</span>`, the content hash differs, but the structural hash
is identical and the framework knows to migrate cursor / selection
state across.

A Tier-2 or Tier-3 match never fires an `Update` event — the node was
matched on visual content (Tier 2) or on text-edit semantics that other
machinery handles (Tier 3). Only Tier-1 matches fire `Update`, because
only Tier-1 establishes "same logical thing, different content".

## What changed about a matched node — `NodeChangeSet`

Once the diff has matched old ↔ new, `compute_node_changes`
(`core/src/diff.rs:167`) walks the two `NodeData`s and produces a
13-bit `NodeChangeSet` describing what's different.

The flags split into three bands:

**Layout-affecting** — any of these forces a relayout:

- `NODE_TYPE_CHANGED` — `Text` became `Image`, etc.
- `TEXT_CONTENT` — text changed inside a `Text` node.
- `IDS_AND_CLASSES` — class list changed (may match different selectors).
- `INLINE_STYLE_LAYOUT` — inline CSS that affects layout (`width`,
  `padding`, `margin`, `display`, …).
- `CHILDREN_CHANGED` — direct children added, removed, or reordered.
- `IMAGE_CHANGED` — image source changed (intrinsic size may differ).
- `CONTENTEDITABLE` — flag toggled.

**Paint-affecting** — repaint without relayout:

- `INLINE_STYLE_PAINT` — inline CSS that affects paint only
  (`color`, `background`, `opacity`, `box-shadow`, …).
- `STYLED_STATE` — pseudo-class state changed (`:hover`, `:focus`,
  `:active`, …).

**Neither** — no visual change:

- `CALLBACKS` — different `RefAny` or different handlers, but the user
  doesn't see a callback.
- `DATASET` — node-attached state changed.
- `ACCESSIBILITY` — a11y description / role changed.
- `TAB_INDEX` — focus order changed; the focus model picks this up,
  but the pixels don't.

`NodeChangeSet::needs_layout()` and `needs_paint()` (defined alongside
the flags at `core/src/diff.rs:122`) collapse the bitmask into the
question the renderer actually asks. The composite masks
`AFFECTS_LAYOUT` and `AFFECTS_PAINT` are the framework's source of
truth — neither is hard-coded against a single flag.

A node whose `NodeChangeSet` has only `CALLBACKS | DATASET` set goes
through the diff, gets its merge callback fired if needed, and *does
not trigger any layout or paint work whatsoever*. The pixels on screen
are byte-identical to the previous frame.

## Promoting changes to a `RelayoutScope`

Knowing "layout changed" is too coarse. Setting `color: red` and
setting `display: flex` both flip `INLINE_STYLE_LAYOUT` (well — the
first flips `INLINE_STYLE_PAINT`, the second flips
`INLINE_STYLE_LAYOUT`), but the *cost* of reacting to them is wildly
different. Re-running flexbox on a 1,000-node subtree because someone
changed an opacity is unacceptable.

`RelayoutScope` (`css/src/props/property.rs:784`) is the four-level
cost classification, computed per-CSS-property by
`CssPropertyType::relayout_scope` (`css/src/props/property.rs:1567`):

- **`None`** — repaint only. Color, background, opacity, transform,
  caret styling, object-fit, clip path: changing them produces *zero*
  layout work. The display list is updated, the damage rects are
  computed, and the affected rectangles re-rasterise.
- **`IfcOnly`** — only the inline formatting context that contains the
  node needs re-shaping. Font-style flip, vertical-align change inside
  a paragraph, letter-spacing update: the IFC re-shapes lines, but
  block-level siblings stay where they are. If the IFC's height ends
  up changing, this auto-upgrades to `SizingOnly`.
- **`SizingOnly`** — this node's size needs recomputing; the parent
  may need to reposition subsequent siblings, but no recursive
  relayout. Border-width tweak, scrollbar gutter, padding inside a
  text container.
- **`Full`** — the whole subtree needs re-laying out. Flips of
  `display`, `position`, `float` — anything that changes the formatting
  context.

This is a deliberate improvement over Taffy's binary clean/dirty flag.
A binary classifier upgrades every change to "full subtree relayout";
azul's four levels let an opacity hover land in the `None` bucket and
take the cheap path.

`RestyleResult::max_relayout_scope` (`core/src/styled_dom.rs:160`)
tracks the highest scope seen across all property changes for a frame.
The layout engine reads it and picks the cheapest path that's still
correct: when `max_relayout_scope <= IfcOnly`, the engine skips
`calculate_layout_for_subtree` entirely and runs the IFC fast path.

`StyledDom::restyle_on_state_change` (`core/src/styled_dom.rs:1604`) is
the path hover / focus / active state changes go through. Each property
that flips contributes its `relayout_scope` to the result; the engine
picks the maximum and acts on it. A `:hover` that only changes
`background-color` round-trips the entire pseudo-class machinery and
ends with `max_relayout_scope = None` — repaint a damage rect, done.

## Lazy paint — damage rects

Even after restyle has decided "repaint only, no layout work", azul
still doesn't paint the whole window. The renderer compares the new
display list against the previous one item-by-item and produces a list
of rectangles that need re-rasterising. The rest of the framebuffer
stays as it was.

`compute_display_list_damage` (`layout/src/cpurender.rs:2095`):

```rust,ignore
pub fn compute_display_list_damage(
    old: &DisplayList,
    new: &DisplayList,
) -> Option<Vec<LogicalRect>>
```

Returns:

- `Some(rects)` — those rectangles need repainting; everything else
  keeps the previous frame's pixels.
- `None` — full repaint (item count differs, or two items at the same
  index have different discriminants — a structural change the
  rect-by-rect comparison can't safely express).

The comparison is conservative. Every display-list item exposes
`is_visually_equal` and `visual_bounds`; a change inside the same
bounds (a colour swap, a text-content edit) still produces a damage
rect covering both the old and new bounds, so neither overdraws onto
stale pixels nor under-paints.

`coalesce_damage_rects` (`layout/src/cpurender.rs:2129`) then merges
overlapping or near-adjacent rects (within an 8-pixel gap). The merge
is `O(n²)` but the typical damage count is well under twenty, so the
constant factor is fine and the result is a small list of well-spaced
rectangles instead of dozens of slivers that would each cause a
redundant repaint.

A few special-cased producers feed into the same pipeline:

- `compute_resize_damage` (`layout/src/cpurender.rs:2175`) handles a
  grow-only window resize: it produces just the right strip and bottom
  strip, skipping a full re-rasterise of the entire window.
- The macOS shell tracks a per-frame `gpu_damage_rects` field
  (`dll/src/desktop/shell2/macos/mod.rs:2250`) which gets fed back
  to the GPU presenter so the OS compositor only invalidates those
  rects, saving copy bandwidth on Retina displays.

The overall flow per frame:

1. `layout()` returns a fresh `Dom`.
2. `reconcile_dom` matches old ↔ new, fires lifecycle events,
   produces `NodeChangeSet`s.
3. Restyle promotes the changes to a `RelayoutScope`. If
   `max_relayout_scope == None`, no layout pass runs.
4. The display list is rebuilt only for the parts that changed.
5. `compute_display_list_damage` produces the damage rects.
6. The renderer rasterises only those rects.

For a hover-state change on a button, steps 3–5 are constant-time and
step 6 paints maybe 100 × 30 pixels.

## When the framework matches old ↔ new

Bringing the three layers together, here is the per-node flow during a
`RefreshDom`:

- The diff classifies the node (Stable / Moved / Created / Destroyed).
- For Stable / Moved: `compute_node_changes` produces a
  `NodeChangeSet`; the merge callback (if any) runs *here*, claiming
  resources from the old dataset before it drops. See
  [Merge Callbacks](merge-callbacks.md) for the reconcile-style
  protocol that uses this hook.
- For Created: the new node is mounted, lifecycle Mount event fires
  if registered; no merge runs.
- For Destroyed: the old node and its dataset drop; lifecycle Unmount
  event fires. Heavy resources owned by the dataset clean up via
  `Drop` in the natural way.

The same `NodeChangeSet` that drove restyle / layout decisions is
exposed as `ExtendedDiffResult.node_changes` (`core/src/diff.rs:147`)
for tools that want to see a per-node change report — the live
debugger uses this to highlight which nodes actually changed in the
last frame, and which ones rendered identical pixels.

## Where to read the source

- `core/src/diff.rs:41` — `NodeChangeSet` (the 13-bit flag struct)
- `core/src/diff.rs:84` — `AFFECTS_LAYOUT` / `AFFECTS_PAINT` masks
- `core/src/diff.rs:167` — `compute_node_changes` (per-pair diff)
- `core/src/diff.rs:331` — `calculate_reconciliation_key` (Tier 1)
- `core/src/diff.rs:418` — `DiffResult`
- `core/src/diff.rs:459` — `reconcile_dom` (entry point)
- `core/src/diff.rs:776` — `transfer_states` (where merge callbacks fire)
- `css/src/props/property.rs:784` — `RelayoutScope` (four-level scope)
- `css/src/props/property.rs:1567` — `relayout_scope` per property
- `core/src/styled_dom.rs:140` — `RestyleResult` with
  `max_relayout_scope`
- `core/src/styled_dom.rs:1604` — `restyle_on_state_change`
- `layout/src/cpurender.rs:2095` — `compute_display_list_damage`
- `layout/src/cpurender.rs:2129` — `coalesce_damage_rects`
- `layout/src/cpurender.rs:2175` — `compute_resize_damage`
