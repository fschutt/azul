---
slug: layout/positioning
title: Positioning and Overflow
language: en
canonical_slug: layout/positioning
audience: external
maturity: mature
guide_order: 53
topic_only: false
prerequisites: [layout]
tracked_files:
  - css/src/props/layout/position.rs
  - css/src/props/layout/overflow.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T00:00:00Z
---

`position` decides where in the layout flow an element renders, and `overflow`
decides what happens when its content doesn't fit. The two interact: a
`position: sticky` element sticks within the nearest scroll container, and
`position: absolute` is laid out relative to the nearest *positioned* ancestor.

## `position`

`LayoutPosition` (`css/src/props/layout/position.rs:22`):

| value | placed by |
|---|---|
| `static` (default) | normal flow; offsets are ignored |
| `relative` | normal flow, then shifted by `top` / `right` / `bottom` / `left` |
| `absolute` | offsets, relative to the nearest positioned ancestor |
| `fixed` | offsets, relative to the window |
| `sticky` | flow until offsets would be violated, then pinned to the scroll container |

`LayoutPosition::is_positioned` returns `true` for everything except `Static`.
"Positioned ancestor" means *any* ancestor whose `position` is not `static`.

```rust
# fn body() -> &'static str {
"<div style='position: relative;'>
   <div style='position: absolute; top: 8px; right: 8px;'>badge</div>
   container content
 </div>"
# }
```

## Offsets: `top`, `right`, `bottom`, `left`

Each offset is a `<length>` or `<percentage>`. Default is `0` for the
`define_position_property!`-defined types; `auto` means "no constraint" and
solver-computed.

| value | resolves against |
|---|---|
| `<length>` | a fixed pixel distance |
| `<percentage>` | the *containing block*'s width (for `left`/`right`) or height (for `top`/`bottom`) |

For `position: absolute`:

- Setting `top` and `bottom` *both* stretches the element vertically.
- Setting `left` and `right` *both* stretches it horizontally.
- Combined with `width: auto` / `height: auto`, this is how you fill a
  positioned ancestor exactly.

```rust
# fn body() -> &'static str {
"<div style='position: relative; height: 200px;'>
   <div style='position: absolute; top: 0; bottom: 0; right: 0; width: 60px;'>
     full-height sidebar
   </div>
 </div>"
# }
```

## `z-index`

`LayoutZIndex` (`css/src/props/layout/position.rs:202`):

```rust,ignore
LayoutZIndex::Auto         // default — same stacking level as parent
LayoutZIndex::Integer(i32) // explicit stacking order; positive = above, negative = below
```

`z-index` only takes effect on positioned elements (`position` ≠ `static`). A
positioned element with `z-index: <integer>` creates a new *stacking context* —
its descendants then stack relative to it, not the document root.

## `position: sticky`

A sticky element behaves like `relative` until the user scrolls past the offsets,
at which point it sticks to the scroll container's edge. The offsets define
*when* the element pins:

```rust
# fn body() -> &'static str {
"<div style='overflow: auto; height: 300px;'>
   <h2 style='position: sticky; top: 0; background: white;'>section A</h2>
   <p>...long content...</p>
   <h2 style='position: sticky; top: 0; background: white;'>section B</h2>
   <p>...long content...</p>
 </div>"
# }
```

The nearest scroll container is whatever ancestor has `overflow: auto | scroll |
hidden | clip`. If the document body is the scroll container, the sticky
element pins to the window.

## `overflow`

`LayoutOverflow` (`css/src/props/layout/overflow.rs:16`) controls clipping and
scrolling. Apply `overflow-x` / `overflow-y` independently, or set both with the
shorthand `overflow`.

| value | clips | scrolls | scrollbar |
|---|---|---|---|
| `visible` (default) | no | no | — |
| `hidden` | yes | programmatic only | none |
| `clip` | yes | no | — |
| `scroll` | yes | yes | always shown |
| `auto` | yes | when needed | when content overflows |

`LayoutOverflow::is_clipped` reports whether an axis clips content.
`LayoutOverflow::needs_scrollbar(currently_overflowing)` decides if the
scrollbar is visible *now*.

### `clip` vs `hidden`

`clip` is a stricter form of `hidden`:

- Both clip overflowing content.
- `hidden` permits programmatic scrolling (`element.scrollTo(...)` analogues);
  `clip` does not. `clip` makes the element a "non-scroll container".
- `overflow-clip-margin` extends the clip region outside the box; it has no
  effect on `hidden`.

### Computed-value coupling

CSS Overflow 3 § 3.1 specifies that `visible` / `clip` on one axis becomes
`auto` / `hidden` if the *other* axis is scrollable. The implementation lives
on `LayoutOverflow::resolve_computed` (`css/src/props/layout/overflow.rs:85`):
the ratchet runs at computed-value time, before layout sees the property.

```text
overflow-x: visible, overflow-y: scroll  →  computed: auto / scroll
overflow-x: clip,    overflow-y: hidden  →  computed: hidden / hidden
overflow-x: visible, overflow-y: visible →  unchanged
```

## `scrollbar-gutter`

`StyleScrollbarGutter` (`css/src/props/layout/overflow.rs:182`) reserves space
for the scrollbar so layout doesn't shift when content starts overflowing.

| value | behaviour |
|---|---|
| `auto` (default) | gutter only when scrollbar is shown |
| `stable` | gutter always reserved on the scrollbar's edge |
| `stable both-edges` | gutter on both edges (visual symmetry) |

```rust
# fn body() -> &'static str {
".pane { overflow-y: auto; scrollbar-gutter: stable; }"
# }
```

## `overflow-clip-margin`

`StyleOverflowClipMargin` (`css/src/props/layout/overflow.rs:300`) extends the
clip region outside the box. Only applies when `overflow: clip` is set.

Syntax: `<visual-box> || <length>`. The box defaults to `padding-box`; the
length defaults to `0px`.

```rust
# fn body() -> &'static str {
".badge { overflow: clip; overflow-clip-margin: padding-box 4px; }"
# }
```

## The legacy `clip: rect(...)` property

`StyleClipRect` (`css/src/props/layout/overflow.rs:417`) implements the
deprecated CSS 2.1 `clip` property used with `position: absolute`. Each edge is
either `auto` (the corresponding box edge) or a `<length>`:

```rust
# fn body() -> &'static str {
".overlay { position: absolute; clip: rect(0, 100px, 50px, 0); }"
# }
```

Prefer `clip-path` for new code (covered in CSS Shapes; see the contributor
docs).

## Recipes

### Modal overlay

```azul-render screenshot=pos-modal width=480 height=300 subtitle="Modal centred over a fixed-position backdrop"
<body style="font-family: sans-serif;">
  <div style="position: relative; height: 280px; background: #f3f4f6;">
    <div style="position: absolute; top: 0; left: 0; right: 0; bottom: 0; background: rgba(0,0,0,0.4);"></div>
    <div style="position: absolute; top: 50%; left: 50%; width: 240px; height: 120px; margin-left: -120px; margin-top: -60px; background: white; padding: 16px;">
      <strong>Modal title</strong>
      <p>Modal body content.</p>
    </div>
  </div>
</body>
```

### Pinned header

```azul-render screenshot=pos-sticky width=400 height=300 subtitle="position: sticky pins the header within a scroll container"
<body style="font-family: sans-serif;">
  <div style="overflow: auto; height: 280px; border: 1px solid #ddd;">
    <h2 style="position: sticky; top: 0; background: #fef3c7; margin: 0; padding: 8px;">Sticky header</h2>
    <div style="height: 100px; padding: 8px;">row 1</div>
    <div style="height: 100px; padding: 8px; background: #f3f4f6;">row 2</div>
    <div style="height: 100px; padding: 8px;">row 3</div>
  </div>
</body>
```

### Layered badge

```azul-render screenshot=pos-badge width=320 height=160 subtitle="Absolute-positioned badge on a relatively-positioned card"
<body style="font-family: sans-serif;">
  <div style="position: relative; width: 240px; height: 120px; background: #ddd6fe; padding: 16px; margin: 16px;">
    card body
    <div style="position: absolute; top: -8px; right: -8px; background: #ef4444; color: white; padding: 4px 8px; border-radius: 4px;">NEW</div>
  </div>
</body>
```

## Default values at a glance

| property | default |
|---|---|
| `position` | `static` |
| `top` / `right` / `bottom` / `left` | `0` (treated as `auto` by the solver when unset) |
| `z-index` | `auto` |
| `overflow-x` / `overflow-y` | `visible` |
| `scrollbar-gutter` | `auto` |
| `overflow-clip-margin` | `padding-box 0px` |
