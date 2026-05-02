---
slug: layout
title: Layout
language: en
canonical_slug: layout
audience: external
maturity: mature
guide_order: 50
topic_only: false
prerequisites: [css]
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
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T00:00:00Z
---

Azul lays out a `Dom` by feeding its CSS-resolved styles into a single solver that
implements the `display` modes documented in CSS Display 3, sized by the
`dimensions` properties, separated by `margin` / `padding`, and clipped by
`overflow`. This page covers the properties every container needs; the four
sub-pages describe the per-model details.

- [Flexbox](layout/flex.md) — `display: flex`, the default container layout.
- [Grid](layout/grid.md) — `display: grid` with templates, areas, and `repeat()`.
- [Positioning and Overflow](layout/positioning.md) — `position`, offsets, `z-index`, `overflow`.
- [Text and Fonts](text-and-fonts.md) — `font-family`, `font-size`, weight, style, justification.

## Choosing a `display` mode

Every element starts at `display: block` (`css/src/props/layout/display.rs:13`). The five values
you reach for in app code:

| value | when to use |
|---|---|
| `block` | default; child of another block |
| `flex` | one-axis container; rows or columns of children |
| `grid` | two-axis container; columns *and* rows |
| `inline-block` | flow inline with text but accept width/height |
| `none` | remove the element from layout (keeps it in the DOM) |

```rust
# use azul_css::CssProperty;
# use azul_css::LayoutDisplay;
let css = format!("display: flex;");
```

Table values (`table`, `table-row`, `table-cell`, …), `list-item`, `flow-root`,
`run-in`, `marker`, and `contents` are parsed and stored, and the solver honours
the table values; they're rare in app code.

`InlineFlex` and `InlineGrid` produce the same children layout as `Flex` / `Grid`
but participate inline with surrounding text. `LayoutDisplay::is_inline_level`
identifies these.

## Sizing: `width`, `height`, and the `min-`/`max-` constraints

`LayoutWidth` and `LayoutHeight` (`css/src/props/layout/dimensions.rs:302`) accept:

- `auto` — the default; size from content or container.
- `<length>` — pixels (`px`), points (`pt`), em-units (`em`), root em (`rem`),
  viewport (`vw`/`vh`), percentages (`%`).
- `min-content`, `max-content`, `fit-content(<length>)` — content-driven sizes.
- `calc(<expr>)` — arithmetic over the above. See [calc()](#calc) below.

`LayoutMinWidth`, `LayoutMinHeight`, `LayoutMaxWidth`, `LayoutMaxHeight` accept a
single pixel value. Defaults: `min-*` = `0px`, `max-*` = `f32::MAX` (unconstrained).

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
LayoutBoxSizing::ContentBox  // default — width/height applies to content only
LayoutBoxSizing::BorderBox   // width/height includes padding and border
```

`box-sizing: border-box` is the option you usually want for forms and grids; the
content-box default matches the original CSS 1 behaviour.

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

Operators: `+`, `-`, `*`, `/`. Negative numeric literals are recognised when they
follow an operator or `(`; otherwise `-` is a subtraction operator.

## `margin` and `padding`

Padding insets the content within the box; margin separates the box from its
siblings. Each side is its own property:

```rust
# fn body() -> &'static str {
".card { padding: 12px 16px; margin: 0 auto; }
.tight { padding-top: 4px; padding-inline-start: 8px; }"
# }
```

The shorthand expands the standard CSS way: 1 value = all four sides; 2 = vertical
horizontal; 3 = top, horizontal, bottom; 4 = top, right, bottom, left
(`css/src/props/layout/spacing.rs:188`). `padding-inline-start` /
`padding-inline-end` follow `writing-mode` and `direction`.

Margins on `position: static` block elements collapse vertically the way CSS
specifies. Margins inside `display: flex` and `display: grid` containers do *not*
collapse — use `gap` instead.

## `gap`, `row-gap`, `column-gap`

`gap` adds space between flex/grid items without a margin on each child:

```rust
# fn body() -> &'static str {
".grid { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 16px; }
.row  { display: flex; column-gap: 8px; row-gap: 4px; }"
# }
```

The shorthand `gap: <row> <column>` and the longhands `row-gap`/`column-gap` all
take a single `<length>` (`css/src/props/layout/spacing.rs:126`,
`css/src/props/layout/grid.rs:762`).

## `overflow`

`overflow-x` and `overflow-y` decide what to do with content that doesn't fit:

| value | behaviour |
|---|---|
| `visible` | content paints outside the box (default) |
| `hidden` | content is clipped; no scrollbar |
| `clip` | clipped, like `hidden`, with no scroll port — see [Positioning](layout/positioning.md) |
| `scroll` | always shows a scrollbar |
| `auto` | scrollbar appears only when needed |

`LayoutOverflow::resolve_computed` (`css/src/props/layout/overflow.rs:85`)
applies the spec rule that `visible` / `clip` on one axis become `auto` /
`hidden` if the *other* axis is scrollable. The shorthand `overflow: hidden`
sets both axes.

See [Positioning and Overflow](layout/positioning.md) for `scrollbar-gutter`,
`overflow-clip-margin`, and how scrolling interacts with `position: sticky`.

## Writing modes

`writing-mode` swaps the block and inline axes:

```rust,ignore
LayoutWritingMode::HorizontalTb   // default — Latin scripts
LayoutWritingMode::VerticalRl     // CJK vertical
LayoutWritingMode::VerticalLr     // Mongolian
```

Properties that follow the inline axis (`padding-inline-start`,
`padding-inline-end`, `margin-inline-*`, `gap`'s row/column meaning) re-orient
when the writing mode is vertical.

`clear: left | right | both | none` interacts only with floats and is honoured
on block elements next to `display: float` siblings.

## Multi-column

`column-count`, `column-width`, and the `column-rule-*` longhands flow block
content into multiple columns:

```rust
# fn body() -> &'static str {
"article { column-count: 2; column-gap: 24px; column-rule: 1px solid #ccc; }"
# }
```

`column-span: all` makes a child span every column. `column-fill: balance`
(default) splits content evenly; `auto` fills columns one-by-one. See
`css/src/props/layout/column.rs` for the full set of properties.

## Tables

Use the `display: table*` cascade if you're rendering tabular data and want
proper column-width balancing:

| property | role |
|---|---|
| `table-layout: auto` / `fixed` | shrink-to-fit vs use first-row widths |
| `border-collapse: separate` / `collapse` | visual border model |
| `border-spacing: <h> <v>` | gap between separated borders |
| `caption-side: top` / `bottom` | where the `<caption>` renders |
| `empty-cells: show` / `hide` | display empty separated cells |

A typical layout:

```rust
# fn body() -> &'static str {
"<table style='display: table; border-collapse: collapse;'>
   <tr style='display: table-row;'>
     <td style='display: table-cell;'>A1</td>
     <td style='display: table-cell;'>B1</td>
   </tr>
 </table>"
# }
```

## Pagination and fragmentation

For print and paginated views, fragmentation properties decide where breaks may
occur:

| property | value |
|---|---|
| `page-break-before` / `page-break-after` | `auto`, `always`, `avoid`, `left`, `right` |
| `break-inside` | `auto`, `avoid`, `avoid-page`, `avoid-column` |
| `widows`, `orphans` | minimum lines kept together |
| `box-decoration-break` | `slice`, `clone` |

These properties are wired through `layout/src/fragmentation.rs`. They have no
visible effect inside a window — they fire when the layout is rendered to PDF.

## Where the solver lives

The integration point is `layout/src/solver3/` (`layout/src/lib.rs:58`). It
takes a styled DOM and returns positioned boxes for every node:

- `solver3/block.rs` — block formatting context.
- `solver3/flex.rs` — flexbox.
- `solver3/grid.rs` — grid.
- `solver3/positioning.rs` — `position: absolute | fixed | sticky` resolution.
- `solver3/calc.rs` — `calc()` evaluation.
- `solver3/inline/` — inline / line layout, used inside any block.

These are contributor-facing details; app code interacts with the solver only
through CSS properties and `Dom::style(...)`.

## Visual examples

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
