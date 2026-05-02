---
slug: layout/grid
title: Grid
language: en
canonical_slug: layout/grid
audience: external
maturity: mature
guide_order: 52
topic_only: false
prerequisites: [layout]
tracked_files:
  - css/src/props/layout/grid.rs
  - css/src/props/layout/spacing.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T00:00:00Z
---

`display: grid` lays out children on a two-dimensional grid of explicit columns
and rows. Tracks (`grid-template-columns` / `grid-template-rows`) decide where
the lines go; placement properties (`grid-column`, `grid-row`) decide which
cells each child occupies.

```rust
# fn body() -> &'static str {
"<div style='display: grid;
            grid-template-columns: 200px 1fr 200px;
            grid-template-rows: auto 1fr auto;
            gap: 12px;
            height: 400px;'>
   <header style='grid-column: 1 / 4;'>header</header>
   <nav>nav</nav>
   <main>main</main>
   <aside>aside</aside>
   <footer style='grid-column: 1 / 4;'>footer</footer>
 </div>"
# }
```

## Track sizing

`grid-template-columns` and `grid-template-rows` accept a space-separated list
of `GridTrackSizing` values (`css/src/props/layout/grid.rs:42`).

| value | meaning |
|---|---|
| `<length>` (`100px`, `2em`, `30%`) | fixed size |
| `Nfr` (e.g. `1fr`, `0.5fr`, `2.5fr`) | fraction of remaining space |
| `auto` | size from content |
| `min-content` | smallest size that doesn't overflow content |
| `max-content` | size to fit unwrapped content |
| `minmax(<min>, <max>)` | clamp track to a range |
| `fit-content(<length>)` | `min(max-content, max(min-content, <length>))` |

`fr` units are stored as `i32` scaled by `FR_SCALING_FACTOR = 100`
(`css/src/props/layout/grid.rs:446`), so `1fr` = `Fr(100)` and `0.5fr` =
`Fr(50)`. The type stays `Eq + Ord + Hash`; the visible behaviour matches CSS
exactly — fractional values work.

```rust
# fn body() -> &'static str {
"<div style='display: grid;
            grid-template-columns: 100px 1fr minmax(200px, 2fr) auto;'>
   ...
 </div>"
# }
```

### `repeat()`

`repeat(N, <track-list>)` expands inline. The track list may contain multiple
tracks, which all repeat together:

| input | expands to |
|---|---|
| `repeat(3, 1fr)` | `1fr 1fr 1fr` |
| `repeat(2, 100px 1fr)` | `100px 1fr 100px 1fr` |
| `100px repeat(2, 1fr) auto` | `100px 1fr 1fr auto` |

The repeat count is capped at 10,000 to prevent runaway expansion
(`MAX_GRID_REPEAT_COUNT` in `css/src/props/layout/grid.rs:402`).

## `grid-template-areas`

Names regions of the grid using quoted row strings. Each cell is a token; `.`
means "no area".

```rust
# fn body() -> &'static str {
"<div style='display: grid;
            grid-template-columns: 200px 1fr 200px;
            grid-template-rows: 80px 1fr 60px;
            grid-template-areas:
              \"header header header\"
              \"nav    main   aside\"
              \"footer footer footer\";
            gap: 8px;'>
   <header style='grid-area: header;'>...</header>
   <nav    style='grid-area: nav;'>...</nav>
   <main   style='grid-area: main;'>...</main>
   <aside  style='grid-area: aside;'>...</aside>
   <footer style='grid-area: footer;'>...</footer>
 </div>"
# }
```

Every row string must have the same number of cells, and a named area must form
a rectangle. The parser normalises `name → (row_start, row_end, column_start,
column_end)` in 1-based grid-line numbers (`css/src/props/layout/grid.rs:1316`).

## Item placement: `grid-column` / `grid-row`

`GridPlacement` (`css/src/props/layout/grid.rs:266`) is `<start> / <end>`:

| form | meaning |
|---|---|
| `auto` | auto-place |
| `2` | start at line 2, span 1 cell |
| `2 / 4` | from line 2 up to (not including) line 4 |
| `2 / span 3` | start at line 2, span 3 tracks |
| `span 2` | span 2 tracks, position auto |
| `header / footer` | named lines |
| `-1` | last line; counts from the end |

```rust
# fn body() -> &'static str {
"<div style='grid-column: 1 / -1;'>full-width header</div>
<div style='grid-row: 2; grid-column: 2 / span 2;'>main content</div>"
# }
```

`grid-area: <name>` is the shorthand when you've defined `grid-template-areas`.

## Auto-placement

Children without explicit placement flow into the next free cell. `grid-auto-flow`
controls the direction:

```rust,ignore
LayoutGridAutoFlow::Row          // default — fill rows left-to-right
LayoutGridAutoFlow::Column       // fill columns top-to-bottom
LayoutGridAutoFlow::RowDense     // backfill earlier holes
LayoutGridAutoFlow::ColumnDense  // backfill earlier holes, column-first
```

`grid-auto-rows` and `grid-auto-columns` size implicitly-created tracks. For
example, if you've defined three columns but place a child on column 5, columns
4 and 5 are sized by `grid-auto-columns`.

## `gap`, `row-gap`, `column-gap`

`gap: <row> <column>`. The longhands take a single `<length>`. Same property as
flexbox, same `LayoutGap` type (`css/src/props/layout/grid.rs:762`).

```rust
# fn body() -> &'static str {
"<div style='display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 16px 8px;'>
   ...
 </div>"
# }
```

## Alignment

Grid uses two pairs of axes (rows × columns) and reuses the flex names:

| axis | container | item override |
|---|---|---|
| row (cross) | `align-items` | `align-self` |
| column (main) | `justify-items` | `justify-self` |
| line spacing within tracks | `align-content` | — |
| line spacing within tracks | `justify-content` | — |

```rust,ignore
LayoutJustifyItems::Stretch  // default — fill the cell horizontally
LayoutJustifyItems::Start
LayoutJustifyItems::End
LayoutJustifyItems::Center

LayoutJustifySelf::Auto      // default — inherit justify-items
LayoutJustifySelf::Stretch
LayoutJustifySelf::Start
LayoutJustifySelf::End
LayoutJustifySelf::Center
```

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

| property | default |
|---|---|
| `grid-template-columns` | `none` |
| `grid-template-rows` | `none` |
| `grid-template-areas` | `none` |
| `grid-auto-flow` | `row` |
| `grid-auto-columns` | `auto` |
| `grid-auto-rows` | `auto` |
| `grid-column` / `grid-row` | `auto / auto` |
| `justify-items` | `stretch` |
| `justify-self` | `auto` |
| `align-items` | `stretch` |
| `align-self` | `auto` |
| `gap` | `0` |
