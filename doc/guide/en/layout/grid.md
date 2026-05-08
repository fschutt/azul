---
slug: layout/grid
title: Grid
language: en
canonical_slug: layout/grid
audience: external
maturity: mature
guide_order: 54
topic_only: false
short_desc: Two-axis container layout with tracks and areas
prerequisites: [layout]
tracked_files:
  - css/src/props/layout/grid.rs
  - css/src/props/layout/spacing.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:49:28Z
default-search-keys:
  - Dom
  - Css
  - CssProperty
  - StyledDom
---

# Grid

## Overview

`display: grid` lays out children on a two-dimensional grid of explicit columns
and rows. Tracks (`grid-template-columns` / `grid-template-rows`) decide where
the lines go; placement properties (`grid-column`, `grid-row`) decide which
cells each child occupies.

```html
<div style='display: grid;
            grid-template-columns: 200px 1fr 200px;
            grid-template-rows: auto 1fr auto;
            gap: 12px;
            height: 400px;'>
   <header style='grid-column: 1 / 4;'>header</header>
   <nav>nav</nav>
   <main>main</main>
   <aside>aside</aside>
   <footer style='grid-column: 1 / 4;'>footer</footer>
 </div>
```

## Track sizing

`grid-template-columns` and `grid-template-rows` accept a space-separated
list of track sizes:

- `<length>` (`100px`, `2em`, `30%`). Fixed size.
- `Nfr` (`1fr`, `0.5fr`, `2.5fr`). Fraction of remaining space.
  Fractional values are supported.
- `auto`. Sizes from content.
- `min-content`. Smallest size that doesn't overflow content.
- `max-content`. Sized to fit unwrapped content.
- `minmax(<min>, <max>)`. Clamps the track to a range.
- `fit-content(<length>)`. Equivalent to
  `min(max-content, max(min-content, <length>))`.

```html
<div style='display: grid;
            grid-template-columns: 100px 1fr minmax(200px, 2fr) auto;'>
   ...
 </div>
```

### repeat()

`repeat(N, <track-list>)` expands inline. The track list may contain
multiple tracks, which all repeat together:

- `repeat(3, 1fr)` expands to `1fr 1fr 1fr`.
- `repeat(2, 100px 1fr)` expands to `100px 1fr 100px 1fr`.
- `100px repeat(2, 1fr) auto` expands to `100px 1fr 1fr auto`.

The repeat count is capped at 10,000 to prevent runaway expansion.

## grid-template-areas

Names regions of the grid using quoted row strings. Each cell is a token; `.`
means "no area".

```html
<div style='display: grid;
            grid-template-columns: 200px 1fr 200px;
            grid-template-rows: 80px 1fr 60px;
            grid-template-areas:
              "header header header"
              "nav    main   aside"
              "footer footer footer";
            gap: 8px;'>
   <header style='grid-area: header;'>...</header>
   <nav    style='grid-area: nav;'>...</nav>
   <main   style='grid-area: main;'>...</main>
   <aside  style='grid-area: aside;'>...</aside>
   <footer style='grid-area: footer;'>...</footer>
 </div>
```

Every row string must have the same number of cells, and a named area
must form a rectangle.

## Item placement: grid-column / grid-row

`grid-column` and `grid-row` take `<start> / <end>`:

- `auto`. Auto-place.
- `2`. Start at line 2, span 1 cell.
- `2 / 4`. From line 2 up to (not including) line 4.
- `2 / span 3`. Start at line 2, span 3 tracks.
- `span 2`. Span 2 tracks, position auto.
- `header / footer`. Named lines.
- `-1`. Last line, counts from the end.

```html
<div style='grid-column: 1 / -1;'>full-width header</div>
<div style='grid-row: 2; grid-column: 2 / span 2;'>main content</div>
```

`grid-area: <name>` is the shorthand when you've defined `grid-template-areas`.

## Auto-placement

Children without explicit placement flow into the next free cell.
`grid-auto-flow` controls the direction:

- `row` (default). Fill rows left-to-right.
- `column`. Fill columns top-to-bottom.
- `row dense`. Backfill earlier holes.
- `column dense`. Backfill earlier holes, column-first.

`grid-auto-rows` and `grid-auto-columns` size implicitly-created tracks.
For example, if you've defined three columns but place a child on column
5, columns 4 and 5 are sized by `grid-auto-columns`.

## gap, row-gap, column-gap

`gap: <row> <column>`. The longhands take a single `<length>`. Same as
flexbox.

```html
<div style='display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 16px 8px;'>
   ...
 </div>
```

## Alignment

Grid uses two pairs of axes (rows by columns) and reuses the flex names:

- **Row (cross) axis.** Container uses `align-items`. Items override with `align-self`.
- **Column (main) axis.** Container uses `justify-items`. Items override with `justify-self`.
- **Line spacing within tracks (cross).** Container uses `align-content`. No item override.
- **Line spacing within tracks (main).** Container uses `justify-content`. No item override.

`justify-items` accepts `stretch` (default), `start`, `end`, `center`.
`justify-self` accepts `auto` (default, inherits `justify-items`),
`stretch`, `start`, `end`, `center`.

## Recipes

### Holy grail layout

```azul-render screenshot=grid-holy-grail width=480 height=320 subtitle="Header / nav / main / aside / footer with grid-template-areas"
<body style="font-family: sans-serif;">
  <div style="display: grid;
              grid-template-columns: 100px 1fr 100px;
              grid-template-rows: 60px 1fr 50px;
              grid-template-areas: 'h h h' 'n m a' 'f f f';
              gap: 4px;
              height: 300px; padding: 8px;">
    <div style="grid-area: h; background: #818cf8; padding: 8px;">header</div>
    <div style="grid-area: n; background: #c4b5fd; padding: 8px;">nav</div>
    <div style="grid-area: m; background: #ddd6fe; padding: 8px;">main</div>
    <div style="grid-area: a; background: #c4b5fd; padding: 8px;">aside</div>
    <div style="grid-area: f; background: #818cf8; padding: 8px;">footer</div>
  </div>
</body>
```

### Card grid

```azul-render screenshot=grid-cards width=480 height=240 subtitle="Auto-flowing cards in a 3-column grid"
<body style="font-family: sans-serif;">
  <div style="display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 12px; padding: 8px;">
    <div style="background: #fef3c7; padding: 16px;">card 1</div>
    <div style="background: #fde68a; padding: 16px;">card 2</div>
    <div style="background: #fcd34d; padding: 16px;">card 3</div>
    <div style="background: #fef3c7; padding: 16px;">card 4</div>
    <div style="background: #fde68a; padding: 16px;">card 5</div>
    <div style="background: #fcd34d; padding: 16px;">card 6</div>
  </div>
</body>
```

### Responsive minmax

```azul-render screenshot=grid-minmax width=480 height=200 subtitle="Tracks that clamp to a range with minmax()"
<body style="font-family: sans-serif;">
  <div style="display: grid; grid-template-columns: minmax(120px, 1fr) 2fr minmax(80px, 200px); gap: 8px; padding: 8px;">
    <div style="background: #a7f3d0; padding: 12px;">120 → 1fr</div>
    <div style="background: #6ee7b7; padding: 12px;">2fr</div>
    <div style="background: #34d399; padding: 12px;">80 → 200</div>
  </div>
</body>
```

## Default values at a glance

- `grid-template-columns` defaults to `none`.
- `grid-template-rows` defaults to `none`.
- `grid-template-areas` defaults to `none`.
- `grid-auto-flow` defaults to `row`.
- `grid-auto-columns` defaults to `auto`.
- `grid-auto-rows` defaults to `auto`.
- `grid-column` and `grid-row` default to `auto / auto`.
- `justify-items` defaults to `stretch`.
- `justify-self` defaults to `auto`.
- `align-items` defaults to `stretch`.
- `align-self` defaults to `auto`.
- `gap` defaults to `0`.

## Coming Up Next

- [Events](../events.md) — Callbacks, event filters, and how state triggers relayout
- [Images](../images.md) — Loading raster images and CSS backgrounds
- [Flexbox](flex.md) — One-axis container layout with grow/shrink/basis
- [Text Input](../text-input.md) — Editable text, IME, and the selection model
