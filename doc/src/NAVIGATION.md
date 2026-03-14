# Azul Codebase Navigation

Quick-reference for navigating the azul CSS layout engine.
Three crates matter for layout bugs: `azul-css`, `azul-core`, `azul-layout`.

## Data Flow: XHTML → Pixels

```
XHTML source
  → css/src/props/property.rs        parse CSS text, expand shorthands (flex, margin, border, ...)
  → core/src/prop_cache.rs           cascade: match selectors, resolve specificity, store per-node
  → layout/src/solver3/getters.rs    read resolved CSS values for each node
  → layout/src/solver3/sizing.rs     compute widths / heights (CSS 2.2 §10.3/§10.6)
  → layout/src/solver3/fc.rs         formatting context dispatch (BFC, IFC, table, flex, grid)
  → layout/src/solver3/taffy_bridge.rs   flex/grid delegated to Taffy crate
  → layout/src/solver3/positioning.rs    relative + absolute positioning
  → layout/src/solver3/display_list.rs   build flat display list (draw commands)
  → layout/src/cpurender.rs          render display list to pixels via tiny-skia
```

## Crate: azul-css  (`css/`)

CSS parsing and type definitions. Zero external dependencies.

### CSS shorthand expansion — `css/src/props/property.rs`
**This is where CSS shorthands like `flex`, `margin`, `border`, `background` get
expanded into their longhand properties.** The function `parse_combined_css_property()`
dispatches on `CombinedCssPropertyType` — e.g. `CombinedCssPropertyType::Flex` expands
`flex: 1` into `flex-grow: 1; flex-shrink: 1; flex-basis: 0`.

If a CSS shorthand is being parsed wrong, look here first.

### CSS type definitions — `css/src/props/layout/`
Each file defines the Rust types for a CSS property category:
- `display.rs` — `LayoutDisplay` (None, Block, Inline, InlineBlock, Flex, Grid, Table, ...)
- `dimensions.rs` — `LayoutWidth`, `LayoutHeight`, `LayoutBoxSizing`
- `spacing.rs` — `LayoutMarginTop/Right/Bottom/Left`, `LayoutPaddingTop/Right/Bottom/Left`
- `position.rs` — `LayoutPosition` (Static, Relative, Absolute, Fixed, Sticky)
- `flex.rs` — `LayoutFlexGrow`, `LayoutFlexShrink`, `LayoutFlexBasis`, `LayoutFlexDirection`, ...
- `grid.rs` — Grid types (`LayoutGridTemplateColumns`, `LayoutGridRow`, ...)
- `overflow.rs` — `LayoutOverflow`
- `wrapping.rs` — `LayoutClear` (None, Left, Right, Both), `LayoutFloat`
- `text.rs` — `StyleLineHeight`, `StyleWhiteSpace`, `StyleTextIndent`, ...
- `table.rs` — Table layout types

### CSS visual/style types — `css/src/props/style/`
- `border.rs` — border width/style/color
- `background.rs` — backgrounds, gradients
- `text.rs` — `StyleFontSize`, `StyleFontFamily`, `StyleColor`, `StyleTextAlign`
- `transform.rs` — CSS transforms
- `effects.rs` — opacity

### CSS value primitives — `css/src/props/basic/`
- `pixel.rs` — `PixelValue`, `PixelValueNoPercent`
- `length.rs` — CSS length units (px, em, rem, %, vw, vh, ...)
- `color.rs` — `ColorU` (RGBA)
- `font.rs` — font parsing (family, weight, style)

## Crate: azul-core  (`core/`)

DOM structures, CSS cascade, and resource management.

### CSS cascade — `core/src/prop_cache.rs`
**Where CSS rules are matched to DOM nodes and specificity is resolved.**
- `CssPropertyCache` — the main cache, stores resolved CSS properties per node
- `CssPropertyCache::restyle()` — runs the cascade: matches selectors, applies rules by specificity
- `FlatVecVec<StatefulCssProperty>` — per-node storage of matched CSS properties
- `CssPropertyOrigin` — tracks where a property came from (user-agent, author, inline)

When multiple CSS rules set the same property on one element, cascade ordering
determines which wins. If the wrong value is picked, look here.

### DOM types — `core/src/dom.rs`
- `NodeType` — Div, Text, Image, IFrame, ...
- `FormattingContext` — Block, InlineFormattingContext, Table, Flex, Grid
- `NodeData` — per-node data (type, classes, ids, inline styles)

### Styled DOM — `core/src/styled_dom.rs`
- `StyledDom` — the styled DOM tree (DOM + resolved CSS properties)
- Access pattern: `styled_dom.node_data`, `styled_dom.css_property_cache`

### User-agent stylesheet — `core/src/ua_css.rs`
Default CSS rules applied before author styles (browser defaults).

## Crate: azul-layout  (`layout/`)

Layout engine, text layout, and rendering.

### Layout solver — `layout/src/solver3/`

**Entry point:** `mod.rs`
- `layout_document()` — main entry, creates `LayoutContext`, runs layout
- `LayoutContext` — carries debug_messages, font cache, viewport size
- `debug_info!` macro (line ~22) — conditional debug tracing, output goes to `.debug.json`

**Layout tree:** `layout_tree.rs`
- `LayoutTree` — the layout tree (parallel to DOM)
- `LayoutNode` — per-node: parent, children, box_props, used_size

**CSS property access:** `getters.rs`
- Getter functions that read resolved CSS values from `CssPropertyCache`
- Pattern: `get_width()`, `get_margin_top()`, `get_display()`, etc.

**Box geometry:** `geometry.rs`
- `BoxProps`, `EdgeSizes { top, right, bottom, left }` (margin/padding/border)
- `IntrinsicSizes`, `UnresolvedBoxProps`

**Width/height calculation:** `sizing.rs`
- CSS 2.2 §10.3 (width) and §10.6 (height) implementation
- Intrinsic size computation (min-content, max-content)

**Formatting contexts:** `fc.rs`  (**most bugs are here**)
- Block formatting context (BFC) — normal flow, margin collapse, clearance, floats
- Inline formatting context (IFC) — line boxes, inline layout
- Table formatting context dispatch
- This file has the float placement, clear logic, and margin collapse code

**Flex/Grid bridge:** `taffy_bridge.rs`
- Converts Azul CSS values to Taffy types, runs Taffy, reads results back
- **Do NOT modify Taffy itself** — bugs are in how we feed data TO Taffy or read FROM it

**Positioning:** `positioning.rs`
- Relative positioning (offset from normal flow position)
- Absolute/fixed positioning (removed from flow, positioned relative to containing block)

**Display list:** `display_list.rs`
- Converts laid-out tree into flat vector of draw commands (rects, borders, text, images)

**Other solver3 files:**
- `cache.rs` — incremental layout cache
- `calc.rs` — `calc()` expression evaluation
- `counters.rs` — CSS counters (`counter-reset`, `counter-increment`)
- `paged_layout.rs` / `pagination.rs` — paged media
- `scrollbar.rs` — scrollbar rendering

### CPU rendering — `layout/src/cpurender.rs`
Renders the flat `DisplayList` to pixels via tiny-skia. If colors, borders, or
backgrounds render wrong (but layout positions are correct), look here.

### Text layout — `layout/src/text3/`
- `mod.rs` — text layout orchestration
- `cache.rs` — text layout cache, constraint builder
- `glyphs.rs` — glyph metrics, shaping, line height
- `knuth_plass.rs` — Knuth-Plass line breaking algorithm
- `default.rs` — default text layout parameters

## Key Types Quick Reference

| Type | Location | Purpose |
|------|----------|---------|
| `LayoutDisplay` | `css/src/props/layout/display.rs` | Block, Inline, Flex, Grid, Table, ... |
| `LayoutPosition` | `css/src/props/layout/position.rs` | Static, Relative, Absolute, Fixed |
| `LayoutClear` | `css/src/props/layout/wrapping.rs` | None, Left, Right, Both |
| `LayoutFloat` | `css/src/props/layout/wrapping.rs` | None, Left, Right |
| `CssPropertyCache` | `core/src/prop_cache.rs` | Resolved CSS per node |
| `FormattingContext` | `core/src/dom.rs` | BFC, IFC, Table, Flex, Grid |
| `LayoutContext` | `layout/src/solver3/mod.rs` | Layout state (debug, fonts, viewport) |
| `LayoutTree` | `layout/src/solver3/layout_tree.rs` | Layout tree parallel to DOM |
| `BoxProps` | `layout/src/solver3/geometry.rs` | Margin, padding, border edges |
| `LogicalPosition` | `core/src/geom.rs` | x, y in CSS logical units |
| `LogicalSize` | `core/src/geom.rs` | width, height in CSS logical units |
| `LogicalRect` | `core/src/geom.rs` | position + size |

## Layout Patterns

- `calculated_positions[idx]` stores the **margin-box** position of node idx
- Containing block = **content-box** of parent (after subtracting border + padding)
- Normal flow: BFC children laid out top-to-bottom with margin collapse
- Floats reduce available width for subsequent line boxes
- `clear` moves below preceding floats (uses clearance offset)
- Relative positioning applied AFTER normal flow, absolute positioning AFTER that
- Flex/Grid: data converted to Taffy types → Taffy computes layout → results read back

## Debug Tracing

The `debug_info!` macro in `layout/src/solver3/mod.rs` (line ~22) outputs trace
messages when `ctx.debug_messages.is_some()`. Convention: `[Tag] message`:

```rust
debug_info!(ctx, "[BFC] child {} width={} margin_left={}", node_id, width, margin);
```

Output appears in `.debug.json` under `render_warnings`. Search with:
```bash
cat '<path>.debug.json' | jq '.render_warnings[]' | grep -i 'width\|margin\|clear'
```

## Common Bug Locations

| Symptom | Likely file | What to check |
|---------|-------------|---------------|
| Wrong width/height | `sizing.rs` | percentage resolution, min/max constraints |
| Margin collapse wrong | `fc.rs` | `last_margin_bottom`, clearance baseline |
| Float positioning | `fc.rs` | float placement, available width reduction |
| Clear not working | `fc.rs` | clearance offset calculation |
| Absolute positioning off | `positioning.rs` | containing block resolution |
| CSS shorthand ignored | `css/src/props/property.rs` | shorthand expansion logic |
| Wrong CSS value picked | `core/src/prop_cache.rs` | cascade specificity, duplicate properties |
| Flex/grid sizing wrong | `taffy_bridge.rs` | value conversion TO Taffy |
| Text position wrong | `text3/glyphs.rs` | line height, baseline alignment |
| Colors/borders wrong | `cpurender.rs` | display list rendering |
| Background missing | `display_list.rs` + `cpurender.rs` | draw command generation |
