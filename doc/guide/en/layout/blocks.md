---
slug: layout/blocks
title: Blocks, Sizing, and Positioning
language: en
canonical_slug: layout/blocks
audience: external
maturity: mature
guide_order: 51
topic_only: false
short_desc: Block formatting context — display, position, width / height / box-sizing, margin / padding, overflow, and z-index.
prerequisites: [layout]
tracked_files:
  - css/src/props/layout/display.rs
  - css/src/props/layout/dimensions.rs
  - css/src/props/layout/position.rs
  - css/src/props/layout/spacing.rs
  - css/src/props/layout/overflow.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:49:28Z
---

# Blocks, Sizing, and Positioning

Every element in a `Dom` starts at `display: block`. A block element takes the
full width of its parent, stacks below the previous sibling, and is sized by
the box-model properties (`width`, `height`, `padding`, `border`, `margin`).
Block layout is what runs by default — flexbox and grid are opt-in modes you
reach for when the default isn't enough.

## `display: block` and `inline-block`

`LayoutDisplay` (`css/src/props/layout/display.rs:13`):

| value | when to use |
|---|---|
| `block` (default) | full-width box; stacks vertically with siblings |
| `inline-block` | flow with text but accept `width` / `height` |
| `flex` | one-axis container — see [Flexbox](flex.md) |
| `grid` | two-axis container — see [Grid](grid.md) |
| `none` | remove from layout (keeps the element in the DOM) |

`inline` (without `-block`) is covered on [Inline and inline-block](inline.md);
the table values (`table`, `table-row`, `table-cell`, `list-item`,
`flow-root`, `run-in`, `marker`, `contents`) parse and store, and the solver
honours the table values. They're rare in app code.

```rust
# fn body() -> &'static str {
"<div style='display: block; padding: 16px; background: #e0e7ff;'>first</div>
<div style='display: block; padding: 16px; background: #ddd6fe;'>second</div>"
# }
```

## Sizing: `width`, `height`, and `min-` / `max-` constraints

`LayoutWidth` and `LayoutHeight` (`css/src/props/layout/dimensions.rs:302`)
accept:

- `auto` — the default; size from content (block) or stretch to container.
- `<length>` — pixels (`px`), points (`pt`), em (`em`), root em (`rem`),
  viewport (`vw` / `vh`), or `<percentage>`.
- `min-content`, `max-content`, `fit-content(<length>)` — content-driven.
- `calc(<expr>)` — arithmetic over the above. See [`calc()`](#calc) below.

`LayoutMinWidth`, `LayoutMinHeight`, `LayoutMaxWidth`, `LayoutMaxHeight` take
a single pixel value. Defaults: `min-*` = `0px`, `max-*` = unconstrained.

```rust
# fn body() -> &'static str {
"
.card { width: 320px; min-height: 100px; max-width: 100%; }
.fluid { width: calc(50% - 20px); }
"
# }
```

### `box-sizing`

```rust,ignore
LayoutBoxSizing::ContentBox  // default — width / height applies to content only
LayoutBoxSizing::BorderBox   // width / height includes padding and border
```

`box-sizing: border-box` is the option you usually want for forms and grids;
the content-box default matches the original CSS 1 behaviour.

### `calc()`

`calc()` is parsed into a flat stack-machine encoded as `CalcAstItemVec`
(`CalcAstItem` at `css/src/props/layout/dimensions.rs:45`). The solver in
`layout/src/solver3/calc.rs` evaluates it left-to-right, resolving each
parenthesised span when the matching `BraceClose` is reached.

```rust
# fn body() -> &'static str {
".sidebar { width: calc(33.333% - 10px); }
.gutter  { width: calc((100% - 40px) / 3); }"
# }
```

Operators: `+`, `-`, `*`, `/`. Negative numeric literals are recognised when
they follow an operator or `(`; otherwise `-` is a subtraction operator.

## `margin` and `padding`

Padding insets the content within the box; margin separates the box from its
siblings:

```rust
# fn body() -> &'static str {
".card { padding: 12px 16px; margin: 0 auto; }
.tight { padding-top: 4px; padding-inline-start: 8px; }"
# }
```

The shorthand expands the standard CSS way: 1 value = all four sides; 2 =
vertical horizontal; 3 = top, horizontal, bottom; 4 = top, right, bottom,
left (`css/src/props/layout/spacing.rs:188`). `padding-inline-start` and
`padding-inline-end` follow `writing-mode` and `direction`.

Margins on `position: static` block elements collapse vertically the way CSS
specifies. Margins inside `display: flex` and `display: grid` containers do
*not* collapse — use `gap` instead.

## `position`

`LayoutPosition` (`css/src/props/layout/position.rs:22`):

| value | placed by |
|---|---|
| `static` (default) | normal flow; offsets are ignored |
| `relative` | normal flow, then shifted by `top` / `right` / `bottom` / `left` |
| `absolute` | offsets, relative to the nearest positioned ancestor |
| `fixed` | offsets, relative to the window |
| `sticky` | flow until offsets would be violated, then pinned to the scroll container |

`LayoutPosition::is_positioned` returns `true` for everything except
`Static`. "Positioned ancestor" means *any* ancestor whose `position` is not
`static`.

```rust
# fn body() -> &'static str {
"<div style='position: relative;'>
   <div style='position: absolute; top: 8px; right: 8px;'>badge</div>
   container content
 </div>"
# }
```

### Offsets: `top`, `right`, `bottom`, `left`

Each offset is a `<length>` or `<percentage>`. `auto` (the default) means
"no constraint" and lets the solver compute it.

| value | resolves against |
|---|---|
| `<length>` | a fixed pixel distance |
| `<percentage>` | the *containing block*'s width (`left` / `right`) or height (`top` / `bottom`) |

For `position: absolute`:

- Setting `top` *and* `bottom` stretches the element vertically.
- Setting `left` *and* `right` stretches it horizontally.
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

### `position: sticky`

A sticky element behaves like `relative` until the user scrolls past the
offsets, at which point it pins to the scroll container's edge:

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

The nearest scroll container is whatever ancestor has `overflow: auto |
scroll | hidden | clip`. If the document body is the scroll container, the
sticky element pins to the window.

## `z-index`

`LayoutZIndex` (`css/src/props/layout/position.rs:202`):

```rust,ignore
LayoutZIndex::Auto         // default — same stacking level as parent
LayoutZIndex::Integer(i32) // explicit stacking order; +ve above, -ve below
```

`z-index` only takes effect on positioned elements (`position` ≠ `static`).
A positioned element with `z-index: <integer>` creates a new *stacking
context* — its descendants then stack relative to it, not the document
root.

## `overflow`

`LayoutOverflow` (`css/src/props/layout/overflow.rs:16`) controls clipping
and scrolling. Apply `overflow-x` / `overflow-y` independently, or set both
with the shorthand `overflow`.

| value | clips | scrolls | scrollbar |
|---|---|---|---|
| `visible` (default) | no | no | — |
| `hidden` | yes | programmatic only | none |
| `clip` | yes | no | — |
| `scroll` | yes | yes | always shown |
| `auto` | yes | when needed | when content overflows |

`LayoutOverflow::is_clipped` reports whether an axis clips content;
`LayoutOverflow::needs_scrollbar(currently_overflowing)` decides if the
scrollbar is visible *now*.

### `clip` vs `hidden`

`clip` is a stricter form of `hidden`:

- Both clip overflowing content.
- `hidden` permits programmatic scrolling (`element.scrollTo(...)`
  analogues); `clip` does not. `clip` makes the element a "non-scroll
  container".
- `overflow-clip-margin` extends the clip region outside the box; it has no
  effect on `hidden`.

### Computed-value coupling

CSS Overflow 3 § 3.1 specifies that `visible` / `clip` on one axis becomes
`auto` / `hidden` if the *other* axis is scrollable.
`LayoutOverflow::resolve_computed` (`css/src/props/layout/overflow.rs:85`)
runs the ratchet at computed-value time, before layout sees the property:

```text
overflow-x: visible, overflow-y: scroll  →  computed: auto / scroll
overflow-x: clip,    overflow-y: hidden  →  computed: hidden / hidden
overflow-x: visible, overflow-y: visible →  unchanged
```

### `scrollbar-gutter`

`StyleScrollbarGutter` (`css/src/props/layout/overflow.rs:182`) reserves
space for the scrollbar so layout doesn't shift when content starts
overflowing.

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

### `overflow-clip-margin`

`StyleOverflowClipMargin` (`css/src/props/layout/overflow.rs:300`) extends
the clip region outside the box. Only applies when `overflow: clip` is set.

Syntax: `<visual-box> || <length>`. The box defaults to `padding-box`; the
length defaults to `0px`.

```rust
# fn body() -> &'static str {
".badge { overflow: clip; overflow-clip-margin: padding-box 4px; }"
# }
```

### The legacy `clip: rect(...)` property

`StyleClipRect` (`css/src/props/layout/overflow.rs:417`) implements the
deprecated CSS 2.1 `clip` property used with `position: absolute`. Each
edge is either `auto` (the corresponding box edge) or a `<length>`:

```rust
# fn body() -> &'static str {
".overlay { position: absolute; clip: rect(0, 100px, 50px, 0); }"
# }
```

Prefer `clip-path` for new code (covered in CSS Shapes; see the contributor
docs).

## `gap`, `row-gap`, `column-gap`

`gap` adds space between flex / grid items without a margin on each child:

```rust
# fn body() -> &'static str {
".grid { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 16px; }
.row  { display: flex; column-gap: 8px; row-gap: 4px; }"
# }
```

The shorthand `gap: <row> <column>` and the longhands `row-gap` /
`column-gap` all take a single `<length>` (`css/src/props/layout/spacing.rs:126`,
`css/src/props/layout/grid.rs:762`).

`gap` does nothing on plain block children (it is flex / grid only).

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

### Default block flow

```azul-render screenshot=layout-block width=480 height=200 subtitle="Default block flow — each element on its own line"
<body style="font-family: sans-serif;">
  <div style="background: #e0e7ff; padding: 8px;">first</div>
  <div style="background: #ddd6fe; padding: 8px;">second</div>
  <div style="background: #c4b5fd; padding: 8px;">third</div>
</body>
```

## Default values at a glance

| property | default |
|---|---|
| `display` | `block` |
| `width` / `height` | `auto` |
| `box-sizing` | `content-box` |
| `min-width` / `min-height` | `0px` |
| `max-width` / `max-height` | unconstrained |
| `margin-*` / `padding-*` | `0px` |
| `position` | `static` |
| `top` / `right` / `bottom` / `left` | `auto` |
| `z-index` | `auto` |
| `overflow-x` / `overflow-y` | `visible` |
| `scrollbar-gutter` | `auto` |
| `overflow-clip-margin` | `padding-box 0px` |
