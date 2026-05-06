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

`LayoutFlexDirection` (`css/src/props/layout/flex.rs:250`):

| value | main axis | order |
|---|---|---|
| `row` (default) | horizontal | first → last |
| `row-reverse` | horizontal | last → first |
| `column` | vertical | first → last |
| `column-reverse` | vertical | last → first |

`row` makes the *inline* axis the main axis; `column` makes the *block* axis the
main axis. `LayoutFlexDirection::get_axis` and `is_reverse` expose this on the
type.

### `flex-wrap`

```rust,ignore
LayoutFlexWrap::NoWrap        // default — single line, items shrink to fit
LayoutFlexWrap::Wrap          // overflow wraps to a new line
LayoutFlexWrap::WrapReverse   // wraps in reverse cross-axis order
```

```rust
# fn body() -> &'static str {
"<div style='display: flex; flex-wrap: wrap; gap: 8px;'>
   <div style='width: 120px;'>one</div>
   <div style='width: 120px;'>two</div>
   <div style='width: 120px;'>three</div>
 </div>"
# }
```

### `justify-content` — main-axis alignment

`LayoutJustifyContent` (`css/src/props/layout/flex.rs:426`):

| value | distributes free space |
|---|---|
| `flex-start` / `start` (default) | at the end |
| `flex-end` / `end` | at the start |
| `center` | equally on both ends |
| `space-between` | between items, none at ends |
| `space-around` | half-space at ends, full between |
| `space-evenly` | equal everywhere |

Default is `Start` (alias of `flex-start`). The CSS-Box-Alignment names
(`start`, `end`) and the legacy flex names (`flex-start`, `flex-end`) parse to
distinct enum variants but produce the same layout.

### `align-items` — cross-axis alignment for every line

`LayoutAlignItems` (`css/src/props/layout/flex.rs:517`):

| value | effect |
|---|---|
| `stretch` (default) | fill the cross axis |
| `start` / `flex-start` | align to cross-start |
| `end` / `flex-end` | align to cross-end |
| `center` | align centred |
| `baseline` | align text baselines |

### `align-content` — cross-axis alignment between lines

Only takes effect when `flex-wrap: wrap` produces multiple lines.
`LayoutAlignContent` (`css/src/props/layout/flex.rs:599`) accepts the same set as
`align-items` plus `space-between` and `space-around`.

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

`LayoutFlexGrow` (`css/src/props/layout/flex.rs:25`) is a non-negative number;
default `0`. `LayoutFlexShrink` (`css/src/props/layout/flex.rs:143`) is the
mirror; default `1`.

When the container has free space on the main axis, items split it in
proportion to `flex-grow`. When the container is *over*flowing, items shrink in
proportion to `flex-shrink`.

```rust
# fn body() -> &'static str {
"<div style='display: flex;'>
   <div style='flex-grow: 1;'>fills available</div>
   <div style='flex-grow: 2;'>fills twice as much</div>
   <div>auto-sized</div>
 </div>"
# }
```

`flex-grow: 0; flex-shrink: 0` pins an item to its `flex-basis` size — useful
for sidebars and toolbars.

### `flex-basis`

`LayoutFlexBasis` (`css/src/props/layout/flex.rs:783`):

```rust,ignore
LayoutFlexBasis::Auto              // default — use width/height
LayoutFlexBasis::Exact(PixelValue) // fixed size before grow/shrink applies
```

`flex-basis` is the size the item starts at *before* `flex-grow` or
`flex-shrink` redistribute space. `auto` falls back to `width` (in `row`) or
`height` (in `column`).

### `align-self` — override `align-items` for one item

`LayoutAlignSelf` (`css/src/props/layout/flex.rs:684`):

```rust,ignore
LayoutAlignSelf::Auto      // default — inherit container's align-items
LayoutAlignSelf::Stretch
LayoutAlignSelf::Start
LayoutAlignSelf::End
LayoutAlignSelf::Center
LayoutAlignSelf::Baseline
```

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
