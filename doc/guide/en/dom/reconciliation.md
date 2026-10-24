---
slug: dom/reconciliation
title: Reconciliation
language: en
canonical_slug: dom/reconciliation
audience: external
maturity: mature
guide_order: 51
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
default-search-keys:
  - Dom
  - Update
  - AttributeType
  - with_merge_callback
  - DatasetMergeCallback
  - EventFilter
---

# Reconciliation

## Introduction

Azul rebuilds the `Dom` from scratch on every `Update::RefreshDom`. The
tree returned by `layout()` is frozen. The next state change runs
`layout()` again and produces a new tree. The diff is what makes that
affordable.

The framework matches each new node against the most plausible old node,
classifies what changed, and picks the cheapest path forward. A hover
highlight on one button repaints one rectangle. Tree size doesn't matter.

## Matching new to old

Every node in the new tree is one of four things:

- **Stable**. Matched against an old node at the same logical position.
  Old node id maps to new node id. Merge callbacks run. Focus and scroll
  state migrate.
- **Moved**. Matched against an old node at a different parent or
  sibling position. Same migration treatment as Stable. It is laid out at
  its new place at once; it SLIDES there only if it declares
  `animation: move ..` ([Animations](../animations.md#moves)).
- **Created**. No match. A Mount event fires.
- **Destroyed**. The old tree had a node here, the new tree doesn't. An
  Unmount event fires. The dataset drops.

For trees with stable sibling order, structural matching works without
any help. The third `<li>` inside a `<div id="notes">` matches its
counterpart in the old tree. Its text content can change and it'll
still match.

The matching runs in this order, strongest evidence first:

1. **Key or id.** A node with `.with_key(..)` or a CSS id matches the old
   node with the same key.
2. **A distinct subtree that moved.** A subtree whose content is UNIQUE in
   its container on both sides (a paragraph re-paginated onto another page, a
   sorted row with its own text) follows its content to its new place.
3. **Its children.** A parent is the old node its already-matched children
   all descend from - that survives chrome being wrapped around the root.
4. **Its place.** Everything else matches by position: the same path of
   sibling indices, node type and classes.

**Identical siblings are matched by place**, never by content: four empty
cells, a row of pending map tiles, repeated words. When one of them changes
(a cell gains `selected`), the others keep their own old nodes - they do not
shift by one to fill the gap, so their state (focus, scroll, a widget's
dataset) stays with the node it belongs to and nothing appears to "move".

Classes are part of a keyless node's identity: a node whose class list
changes may be matched as a new node (an unmount and a mount) rather than
updated in place. Give it a key or an id when its state must survive a class
change.

For trees whose order can change (sortable lists, drag-and-drop,
virtualised scrolling), structural matching loses cursor position,
focus, and dataset state when items reorder. Those cases need a stable
identity. Set an `AttributeType::Id` on the node so the diff can match
across the reorder.

## What does and doesn't trigger work

Different changes cost different amounts of work. The framework
classifies each property change so it can pick the cheapest correct
path:

- **Repaint only**. Color, background, opacity, transform, caret
  styling, object-fit, clip path. No layout pass runs. The display
  list updates and the affected pixels re-rasterise.
- **Inline reshape**. Font-style flip, vertical-align change inside a
  paragraph, letter-spacing update. Only the paragraph's text reflows.
  Block-level siblings stay where they are.
- **Local resize**. Border-width tweak, scrollbar gutter, padding
  inside a text container. The node's size recomputes. The parent may
  reposition subsequent siblings. No recursive relayout.
- **Full subtree relayout**. Flips of `display`, `position`, `float`.
  Anything that changes the formatting context.

A `:hover` that only changes `background-color` lands in the repaint-
only bucket. A change to `display: flex` triggers a full subtree
relayout. The framework picks the right tier automatically.

Pseudo-class state changes (`:hover`, `:focus`, `:active`) go through
the same classification. A hover that flips just the background never
runs layout.

## Lazy paint

Even after restyle decides "repaint only", azul doesn't paint the whole
window. It compares the new display list against the previous one and
returns the rectangles that need re-rasterising. Everything outside
those rectangles keeps the previous frame's pixels.

For a hover-state change on a button, this is constant-time work. The
renderer rasterises maybe 100 by 30 pixels. The rest of the frame is a
straight blit from the previous one.

A grow-only window resize feeds a specialised producer. Just the new
right strip and bottom strip get rasterised. The existing pixels stay.

## Per-frame flow

1. `layout()` returns a fresh `Dom`.
2. The framework matches new nodes against old. Lifecycle events
   (Mount, Unmount, Update) fire.
3. Restyle promotes the changes to the smallest sufficient scope. If
   nothing layout-affecting changed, no layout pass runs.
4. The display list rebuilds for the parts that changed.
5. The renderer compares display lists and produces damage rects.
6. Only those rects get rasterised.

For a hover-state change on a button, steps 3 to 5 are constant-time.
Step 6 paints a single small rectangle.

## Merge callbacks across the diff

For Stable and Moved nodes, the merge callback (if registered) runs
during the diff. It receives the old and new `RefAny` values. Heavy
resources (a video decoder, a GL texture, the cursor inside a focused
input) can move from the old dataset to the new one before the old
tree drops. See [Merge Callbacks](merge-callbacks.md) for the protocol.

For Created nodes, the Mount lifecycle event fires (no merge runs).
For Destroyed nodes, the dataset drops via `Drop` in the natural way
and the Unmount event fires.

## Internals

The full algorithm covers matching tiers, the property-change bitmask,
the relayout-scope enum and damage-rect coalescing.
