---
slug: layout
title: Layout
language: en
canonical_slug: layout
audience: external
maturity: mature
guide_order: 50
topic_only: false
short_desc: Overview of the layout solver
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

## Overview

Azul lays out a `Dom` from its CSS-resolved styles. Every element runs
through one of four formatting modes selected by its `display` property:
block, inline, flex, or grid. The four sub-pages cover each mode from
simple to complex. Blocks first (the default), then inline text flow,
then the two opt-in container modes.

- **Block**. The default. Full-width stacked boxes. See
  [Blocks, Sizing, and Positioning](layout/blocks.md).
- **Inline**. Text and inline-block runs inside a block. See
  [Inline, Inline-Block, and Text Flow](layout/inline.md).
- **Flex**. One-axis containers (rows or columns). See
  [Flexbox](layout/flex.md).
- **Grid**. Two-axis containers (columns and rows). See
  [Grid](layout/grid.md).

## Picking a mode

Most apps use blocks for the page chrome, flex inside each block for rows
and columns, and grid where you need a true 2-D layout (dashboards, image
galleries, form labels with fields). Inline shows up naturally inside any
block that contains text. You usually don't choose it explicitly.

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
- `gap`, `row-gap`, `column-gap`. Honoured by flex and grid. Ignored on plain block.

Block-specific details (float, `clear`, and `display: table*`) live on
the [Blocks](layout/blocks.md) and [Inline](layout/inline.md) pages.

App code interacts with layout only through CSS properties and
`Dom::style(...)` / `Dom::with_css(...)`.

## Visual examples

Same three children, three formatting modes:

```azul-render screenshot=layout-block width=480 height=200 subtitle="Default block flow. Each element on its own line"
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

## Coming Up Next

- [Simple Layout](layout/blocks.md) — Explains block formatting, sizing, positioning, the box model and how to handle overflowing content
- [Inline Layout](layout/inline.md) — Text flow, word breaks, writing modes, multi-column
- [Flexbox](layout/flex.md) — One-axis container layout with grow/shrink/basis
- [Grid](layout/grid.md) — Two-axis container layout with tracks and areas
