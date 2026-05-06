---
slug: layout/inline
title: Inline Layout
language: en
canonical_slug: layout/inline
audience: external
maturity: mature
guide_order: 52
topic_only: false
short_desc: Text flow, word breaks, writing modes, multi-column
prerequisites: [layout/blocks]
tracked_files:
  - css/src/props/layout/text.rs
  - css/src/props/layout/wrapping.rs
  - css/src/props/layout/fragmentation.rs
  - css/src/props/layout/column.rs
  - css/src/props/layout/display.rs
  - css/src/props/basic/font.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:49:28Z
---

# Inline, Inline-Block, and Text Flow

Inline content flows from left to right (or right to left, in RTL scripts)
inside a block. The browser-style line box wraps when it runs out of room
and pushes the next chunk down. Block elements stack vertically; inline
elements stack horizontally and wrap.

`display: inline` participates in line layout — it gets no explicit width
or height, paints between text runs, and breaks across lines if it has to.
`display: inline-block` *does* accept `width` and `height` (and so behaves
like a small block), but otherwise still rides along inline content.

```rust
# fn body() -> &'static str {
"<p>
  Inline:
  <span style='display: inline; background: #fef3c7;'>span</span> wraps with text.
  Inline-block:
  <span style='display: inline-block; width: 80px; background: #c4b5fd;'>fixed-width</span>
  is sized like a block but flows inline.
</p>"
# }
```

## Choosing between `inline` and `inline-block`

| value | accepts `width`/`height` | breaks across lines | typical use |
|---|---|---|---|
| `inline` | no | yes | text emphasis, anchors, icons inside prose |
| `inline-block` | yes | no | tags, chips, fixed-size inline buttons |

`inline-flex` and `inline-grid` are flex / grid containers that participate
inline (`LayoutDisplay::is_inline_level`, `display.rs`). Their *content*
lays out as flex / grid, but the box itself rides the surrounding line.

## How text becomes lines

Inline content is shaped, broken into lines, and laid out by the `text3`
engine (`layout/src/text3/`). The pipeline runs once per layout pass and
caches its results:

1. **Shape** — every character is mapped to a glyph through the active
   font and its OpenType tables. Ligatures, contextual alternates, and
   complex-script reordering happen here.
2. **Break** — a Knuth–Plass total-fit pass picks the line breaks that
   minimise stretch and shrink across the whole paragraph.
3. **BiDi reorder** — UAX #9 reorders runs by directional level so RTL
   and LTR text co-exist in one paragraph.
4. **Position** — glyphs are placed along the inline axis, with
   `letter-spacing`, `word-spacing`, and `tab-size` applied.
5. **Fit** — the resulting lines are stacked in the block axis with
   `line-height` between baselines.

You don't drive the engine yourself. You set CSS properties on the
container; the engine reads them at layout time. For the architecture, see
[Inline Layout and Text Shaping](../internals/inline-text3.md).

## Wrapping: `white-space`, `word-break`, `overflow-wrap`

`white-space` controls collapsing and wrapping (`css/src/props/layout/wrapping.rs`):

| value | collapse whitespace | wrap |
|---|---|---|
| `normal` (default) | yes | yes |
| `pre` | no | no (only at explicit breaks) |
| `nowrap` | yes | no |
| `pre-wrap` | no | yes |
| `pre-line` | yes (newlines preserved) | yes |
| `break-spaces` | no, breaks at every space | yes |

`word-break` and `overflow-wrap` decide what to do with sequences that
have no soft breaks (URLs, code, CJK):

```rust,ignore
StyleWordBreak::Normal      // default
StyleWordBreak::BreakAll    // allow break between any two characters
StyleWordBreak::KeepAll     // forbid break inside CJK
StyleWordBreak::BreakWord   // deprecated alias

StyleOverflowWrap::Normal   // only at allowed break points
StyleOverflowWrap::Anywhere // break anywhere to prevent overflow
StyleOverflowWrap::BreakWord
```

`hyphens: none | manual | auto` enables soft-hyphen breaking. `auto`
requires the layout crate's hyphenation resources for the active language.

```rust
# fn body() -> &'static str {
"
.code { white-space: pre; tab-size: 4em; }
.tag  { white-space: nowrap; }
.body { hyphens: auto; word-break: normal; overflow-wrap: anywhere; }
"
# }
```

## Spacing and metrics

The properties that modulate inline flow:

| property | type | default | meaning |
|---|---|---|---|
| `line-height` | `<percentage>` or `<length>` | `120%` | baseline-to-baseline distance |
| `letter-spacing` | `<length>` | `0px` | added between every glyph |
| `word-spacing` | `<length>` | `0px` | added between words |
| `tab-size` | `<length>` | `8em` | width of a tab character |
| `vertical-align` | enum | `baseline` | how inline-blocks line up against the line box |

```rust
# fn body() -> &'static str {
".body { line-height: 150%; letter-spacing: 0.02em; }
pre   { tab-size: 4em; }"
# }
```

`vertical-align` accepts `baseline`, `top`, `middle`, `bottom`, `sub`,
`super`, `text-top`, `text-bottom`, a `<percentage>`, or a `<length>`.
Because the line box is sized by the tallest inline-block on the line,
mixing `vertical-align: top` and `vertical-align: middle` produces the
same line height — only the glyph placement within that line changes.

## Direction and writing modes

For bidirectional text, set `direction` on the block container:

```rust,ignore
StyleDirection::Ltr   // left-to-right (default)
StyleDirection::Rtl   // right-to-left
```

`unicode-bidi` (`StyleUnicodeBidi`) sets the bidi-override embedding for an
inline element. Most apps don't touch it — the BiDi pass handles mixed text
correctly without help.

`writing-mode` swaps the block and inline axes. Inline text then flows down
(or up) instead of across:

```rust,ignore
LayoutWritingMode::HorizontalTb   // default — Latin scripts
LayoutWritingMode::VerticalRl     // CJK vertical
LayoutWritingMode::VerticalLr     // Mongolian
```

Properties that follow the inline axis (`padding-inline-start`,
`padding-inline-end`, `margin-inline-*`, `gap`'s row/column meaning) re-orient
when the writing mode is vertical.

```rust
# fn body() -> &'static str {
"<div style='writing-mode: vertical-rl; height: 200px;'>
  Vertical CJK-style flow, top-to-bottom right-to-left.
</div>"
# }
```

## Multi-column

`column-count`, `column-width`, and the `column-rule-*` longhands flow
block content into multiple columns:

```rust
# fn body() -> &'static str {
"article { column-count: 2; column-gap: 24px; column-rule: 1px solid #ccc; }"
# }
```

| property | values |
|---|---|
| `column-count` | integer or `auto` |
| `column-width` | `<length>` or `auto` |
| `column-gap` | `<length>` |
| `column-rule` | shorthand for `width style color` |
| `column-span` | `none`, `all` (span every column) |
| `column-fill` | `balance` (default — even split), `auto` (fill columns one by one) |

See `css/src/props/layout/column.rs` for the full set.

## Pagination and fragmentation

For print and paginated views, fragmentation properties decide where
breaks may occur:

| property | value |
|---|---|
| `page-break-before` / `page-break-after` | `auto`, `always`, `avoid`, `left`, `right` |
| `break-inside` | `auto`, `avoid`, `avoid-page`, `avoid-column` |
| `widows`, `orphans` | minimum lines kept together |
| `box-decoration-break` | `slice`, `clone` |

These properties are wired through `layout/src/fragmentation.rs`. They
have no visible effect inside a window — they fire when the layout is
rendered to PDF.

## Floats and `clear`

`clear: left | right | both | none` interacts only with floats and is
honoured on block elements next to `display: float` siblings. Floats are
rare in modern UI code; flexbox and grid handle the same problems with
fewer surprises.

## Recipes

### Wrapping prose

```azul-render screenshot=inline-prose width=480 height=200 subtitle="Default inline flow with hyphens: auto"
<body style="font-family: serif; padding: 16px;">
  <p style="line-height: 150%; hyphens: auto; text-align: justify;">
    Inline text wraps, justifies, and hyphenates. The Knuth–Plass total-fit
    line breaker minimises whole-paragraph stretch instead of breaking line
    by line.
  </p>
</body>
```

### Inline tags / chips

```azul-render screenshot=inline-tags width=480 height=140 subtitle="display: inline-block tags inside flowing text"
<body style="font-family: sans-serif; padding: 16px;">
  <p style="line-height: 1.8;">
    A paragraph with
    <span style="display: inline-block; padding: 2px 8px; background: #fef3c7; border-radius: 12px;">tag</span>
    and another
    <span style="display: inline-block; padding: 2px 8px; background: #ddd6fe; border-radius: 12px;">chip</span>
    placed inline with the prose.
  </p>
</body>
```

### Two-column article

```azul-render screenshot=inline-multicol width=520 height=200 subtitle="column-count: 2 with column-gap and column-rule"
<body style="font-family: serif; padding: 16px;">
  <article style="column-count: 2; column-gap: 24px; column-rule: 1px solid #ccc;">
    Long-form prose that flows naturally across two balanced columns.
    column-fill: balance is the default — content splits evenly between
    columns instead of filling the first one before moving on.
  </article>
</body>
```

## Default values at a glance

| property | default |
|---|---|
| `display` | `block` (override to `inline` / `inline-block` per element) |
| `white-space` | `normal` |
| `word-break` | `normal` |
| `overflow-wrap` | `normal` |
| `hyphens` | `manual` |
| `direction` | `ltr` |
| `writing-mode` | `horizontal-tb` |
| `line-height` | `120%` |
| `letter-spacing` | `0px` |
| `word-spacing` | `0px` |
| `tab-size` | `8em` |
| `vertical-align` | `baseline` |
| `column-count` | `auto` |
| `column-fill` | `balance` |
| `column-rule` | `medium none currentColor` |
| `widows` / `orphans` | `2` |
