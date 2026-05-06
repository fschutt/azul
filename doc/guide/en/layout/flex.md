---
slug: layout/flex
title: Flexbox
language: en
canonical_slug: layout/flex
audience: external
maturity: mature
guide_order: 53
topic_only: false
short_desc: One-axis container layout with grow/shrink/basis
prerequisites: [layout]
tracked_files:
  - css/src/props/layout/flex.rs
  - css/src/props/layout/spacing.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:49:28Z
---

# Flexbox

`display: flex` lays out children along one axis and aligns them on the other.
The axis is set by `flex-direction`; the rest of the properties decide how
remaining space is distributed and where items align.

```rust
# fn body() -> &'static str {
"<div style='display: flex; gap: 8px; padding: 8px;'>
   <div>one</div>
   <div>two</div>
   <div>three</div>
 </div>"
# }
```

## Container properties

These apply to the element with `display: flex` (or `inline-flex`).

### `flex-direction`

- `row` (default). Main axis is horizontal. Items run first to last.
- `row-reverse`. Main axis is horizontal. Items run last to first.
- `column`. Main axis is vertical. Items run first to last.
- `column-reverse`. Main axis is vertical. Items run last to first.

`row` makes the inline axis the main axis. `column` makes the block axis
the main axis.

### `flex-wrap`

- `nowrap` (default). Single line. Items shrink to fit.
- `wrap`. Overflow wraps to a new line.
- `wrap-reverse`. Wraps in reverse cross-axis order.

```rust
# fn body() -> &'static str {
"<div style='display: flex; flex-wrap: wrap; gap: 8px;'>
   <div style='width: 120px;'>one</div>
   <div style='width: 120px;'>two</div>
   <div style='width: 120px;'>three</div>
 </div>"
# }
```

### `justify-content`: main-axis alignment

`justify-content` distributes free space along the main axis:

- `flex-start` / `start` (default). Free space at the end.
- `flex-end` / `end`. Free space at the start.
- `center`. Free space split equally on both ends.
- `space-between`. Space between items, none at ends.
- `space-around`. Half-space at ends, full between.
- `space-evenly`. Equal space everywhere.

The CSS-Box-Alignment names (`start`, `end`) and the legacy flex names
(`flex-start`, `flex-end`) produce the same layout.

### `align-items`: cross-axis alignment for every line

- `stretch` (default). Fills the cross axis.
- `start` / `flex-start`. Aligns to cross-start.
- `end` / `flex-end`. Aligns to cross-end.
- `center`. Centres on cross axis.
- `baseline`. Aligns text baselines.

### `align-content`: cross-axis alignment between lines

Only takes effect when `flex-wrap: wrap` produces multiple lines.
`align-content` accepts the same set as `align-items` plus
`space-between` and `space-around`.

### `gap`, `row-gap`, `column-gap`

Adds space between items without margins. In `flex-direction: row`, `column-gap`
is the main-axis gap and `row-gap` is the cross-axis gap (between wrapped lines).

```rust
# fn body() -> &'static str {
"<div style='display: flex; flex-wrap: wrap; row-gap: 12px; column-gap: 8px;'>
   ...
 </div>"
# }
```

## Item properties

These apply to children of a flex container.

### `flex-grow` and `flex-shrink`

`flex-grow` is a non-negative number, default `0`. `flex-shrink` is its
mirror, default `1`.

When the container has free space on the main axis, items split it in
proportion to `flex-grow`. When the container is overflowing, items
shrink in proportion to `flex-shrink`.

```rust
# fn body() -> &'static str {
"<div style='display: flex;'>
   <div style='flex-grow: 1;'>fills available</div>
   <div style='flex-grow: 2;'>fills twice as much</div>
   <div>auto-sized</div>
 </div>"
# }
```

`flex-grow: 0; flex-shrink: 0` pins an item to its `flex-basis` size.
That's useful for sidebars and toolbars.

### `flex-basis`

- `auto` (default). Uses `width` or `height`.
- `<length>`. Fixed size before grow/shrink applies.

`flex-basis` is the size the item starts at before `flex-grow` or
`flex-shrink` redistribute space. `auto` falls back to `width` (in `row`)
or `height` (in `column`).

### `align-self`: override `align-items` for one item

`align-self` accepts:

- `auto` (default). Inherits the container's `align-items`.
- `stretch`, `start`, `end`, `center`, `baseline`.

```rust
# fn body() -> &'static str {
"<div style='display: flex; align-items: stretch;'>
   <div>stretches</div>
   <div style='align-self: center;'>centred only</div>
 </div>"
# }
```

## Recipes

### Sidebar + content

```azul-render screenshot=flex-sidebar width=480 height=200 subtitle="Fixed sidebar with growing content area"
<body style="font-family: sans-serif;">
  <div style="display: flex; height: 180px; gap: 8px;">
    <div style="width: 120px; background: #e0e7ff; padding: 8px;">sidebar</div>
    <div style="flex-grow: 1; background: #f5f3ff; padding: 8px;">content fills the rest</div>
  </div>
</body>
```

### Equal columns

```azul-render screenshot=flex-equal width=480 height=160 subtitle="Three equal columns via flex-grow: 1"
<body style="font-family: sans-serif;">
  <div style="display: flex; gap: 8px; padding: 8px;">
    <div style="flex-grow: 1; background: #fce7f3; padding: 8px;">A</div>
    <div style="flex-grow: 1; background: #fbcfe8; padding: 8px;">B</div>
    <div style="flex-grow: 1; background: #f9a8d4; padding: 8px;">C</div>
  </div>
</body>
```

### Centred content

```azul-render screenshot=flex-center width=400 height=200 subtitle="Both axes centred"
<body style="font-family: sans-serif;">
  <div style="display: flex; justify-content: center; align-items: center; height: 180px; background: #ecfeff;">
    <div style="background: #a5f3fc; padding: 16px;">centred</div>
  </div>
</body>
```

### Wrapping cards

```azul-render screenshot=flex-wrap width=480 height=240 subtitle="Cards wrap onto multiple lines as the container narrows"
<body style="font-family: sans-serif;">
  <div style="display: flex; flex-wrap: wrap; gap: 12px; padding: 8px;">
    <div style="width: 140px; background: #fef3c7; padding: 12px;">card 1</div>
    <div style="width: 140px; background: #fde68a; padding: 12px;">card 2</div>
    <div style="width: 140px; background: #fcd34d; padding: 12px;">card 3</div>
    <div style="width: 140px; background: #fbbf24; padding: 12px;">card 4</div>
  </div>
</body>
```

## Default values at a glance

| property | default |
|---|---|
| `flex-direction` | `row` |
| `flex-wrap` | `nowrap` |
| `justify-content` | `start` |
| `align-items` | `stretch` |
| `align-content` | `stretch` |
| `flex-grow` | `0` |
| `flex-shrink` | `1` |
| `flex-basis` | `auto` |
| `align-self` | `auto` |
| `gap` | `0` |
