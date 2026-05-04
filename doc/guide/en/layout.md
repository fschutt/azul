---
slug: layout
title: Layout
language: en
canonical_slug: layout
audience: external
maturity: mature
guide_order: 50
topic_only: false
short_desc: Overview of the layout solver and the four formatting modes — block, inline, flex, and grid.
prerequisites: [styling]
tracked_files:
  - css/src/props/layout/mod.rs
  - css/src/props/layout/display.rs
  - css/src/props/layout/dimensions.rs
  - css/src/props/layout/spacing.rs
  - css/src/props/layout/wrapping.rs
  - css/src/props/layout/fragmentation.rs
  - css/src/props/layout/column.rs
  - css/src/props/layout/table.rs
  - css/src/props/layout/shape.rs
  - css/src/props/layout/flow.rs
  - layout/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:49:28Z
---

# Layout

Azul lays out a `Dom` by feeding its CSS-resolved styles into a single
solver. Every element runs through one of four formatting modes — block,
inline, flex, or grid — selected by its `display` property. The four
sub-pages cover each mode from simple to complex: blocks first (the
default), then inline text flow, then the two opt-in container modes.

- **Block** — the default; full-width stacked boxes. See
  [Blocks, Sizing, and Positioning](layout/blocks.md).
- **Inline** — text and inline-block runs inside a block. See
  [Inline, Inline-Block, and Text Flow](layout/inline.md).
- **Flex** — one-axis containers (rows or columns). See
  [Flexbox](layout/flex.md).
- **Grid** — two-axis containers (columns *and* rows). See
  [Grid](layout/grid.md).

## Picking a mode

Most apps use blocks for the page chrome, flex inside each block for rows
and columns, and grid for the few places where you need a true 2-D layout
(dashboards, image galleries, form labels-and-fields). Inline shows up
naturally inside any block that contains text — you usually don't choose
it explicitly.

```rust
# fn body() -> &'static str {
"<body>
  <header style='display: flex; gap: 16px;'>...</header>
  <main style='display: grid; grid-template-columns: 200px 1fr;'>
    <nav>...</nav>
    <article>The article body, which lays out as a block by default.</article>
  </main>
</body>"
# }
```

## Cross-mode properties

A few properties apply regardless of the formatting mode:

- `width` / `height` and the `min-` / `max-` constraints. See [Sizing](layout/blocks.md#sizing-width-height-and-min--max--constraints).
- `padding` and `margin`. See [`margin` and `padding`](layout/blocks.md#margin-and-padding).
- `box-sizing`. See [`box-sizing`](layout/blocks.md#box-sizing).
- `position`, `z-index`, and `overflow`. See [`position`](layout/blocks.md#position) and [`overflow`](layout/blocks.md#overflow).
- `gap`, `row-gap`, `column-gap`. Honoured by flex and grid; ignored on plain block.

Block-specific details — float, `clear`, and `display: table*` — live on
the [Blocks](layout/blocks.md) and [Inline](layout/inline.md) pages.

## What the solver actually reads

The layout solver does not walk the cascade output as a `BTreeMap` of
properties. By the time it runs, [the cascade has already been
flattened](styling.md#where-styles-meet-the-dom--the-deferred-cascade)
into a **compact cache** keyed by node id:

- **Tier 1** — `Vec<u64>`: 21 enum properties bit-packed into 8 B per node
  (`display`, `position`, `float`, `overflow_x/y`, `flex_direction`,
  `flex_wrap`, `justify_content`, `align_items`, `align_content`,
  `font_weight`, `text_align`, `vertical_align`, `visibility`,
  `white_space`, `direction`, `writing_mode`, `clear`, `box_sizing`,
  `font_style`, `border_collapse`).
- **Tier 2 hot** — `Vec<CompactNodeProps>`: the layout-critical numeric
  dimensions in 68 B per node (`width`, `height`, `min/max-width/height`,
  the four `padding`s, the four `margin`s, the four `border-width`s,
  `flex-basis`, `flex-grow`, `flex-shrink`).
- **Tier 2 cold** — `Vec<CompactNodePropsCold>`: paint-only properties in
  28 B per node (color, opacity, …) — read by the display-list builder,
  not by the solver.
- **Tier 2b** — `Vec<CompactTextProps>`: text/IFC properties in 24 B per
  node — read by the inline-formatting context.

Reading `display` for a node is a single `u64` load and bit-mask, not a
map lookup. Reading `width` is one `u32` decode. The solver issues one
linear pass over the tree, getting the data it needs from these arrays
directly — `layout/src/solver3/getters.rs` is full of fast-path getters
named `get_<property>_property` that decode the compact cache, with a slow
fall-through to the full cascade for properties that don't need a fast
path (background, transform, box-shadow).

That's why the [deferred-cascade design](dom.md#when-does-css-actually-apply-not-until-after-layout)
matters performance-wise: the heavy work happens once before layout, and
the solver pays cache-friendly array-indexing cost per node from then on.

## Where the solver lives

The integration point is `layout/src/solver3/` (`layout/src/lib.rs:58`).
It takes a styled DOM and returns positioned boxes for every node:

- `solver3/block.rs` — block formatting context.
- `solver3/inline/` — inline / line layout, used inside any block.
- `solver3/flex.rs` — flexbox.
- `solver3/grid.rs` — grid.
- `solver3/positioning.rs` — `position: absolute | fixed | sticky` resolution.
- `solver3/calc.rs` — `calc()` evaluation.
- `solver3/getters.rs` — the typed wrappers around the compact cache the
  formatting contexts call into.

These are contributor-facing details; app code interacts with the solver
only through CSS properties and `Dom::style(...)`.

## Visual examples

Same three children, three formatting modes:

```azul-render screenshot=layout-block width=480 height=200 subtitle="Default block flow — each element on its own line"
<body style="font-family: sans-serif;">
  <div style="background: #e0e7ff; padding: 8px;">first</div>
  <div style="background: #ddd6fe; padding: 8px;">second</div>
  <div style="background: #c4b5fd; padding: 8px;">third</div>
</body>
```

```azul-render screenshot=layout-flex width=480 height=160 subtitle="Same children with display: flex"
<body style="font-family: sans-serif;">
  <div style="display: flex; gap: 8px;">
    <div style="background: #e0e7ff; padding: 8px; flex-grow: 1;">first</div>
    <div style="background: #ddd6fe; padding: 8px; flex-grow: 1;">second</div>
    <div style="background: #c4b5fd; padding: 8px; flex-grow: 1;">third</div>
  </div>
</body>
```

```azul-render screenshot=layout-grid width=480 height=200 subtitle="Same children with display: grid"
<body style="font-family: sans-serif;">
  <div style="display: grid; grid-template-columns: 1fr 2fr 1fr; gap: 8px;">
    <div style="background: #e0e7ff; padding: 8px;">first</div>
    <div style="background: #ddd6fe; padding: 8px;">second (2fr)</div>
    <div style="background: #c4b5fd; padding: 8px;">third</div>
  </div>
</body>
```
