---
slug: styling
title: Styling with CSS
language: en
canonical_slug: styling
audience: external
maturity: mature
guide_order: 40
topic_only: false
prerequisites: [dom]
tracked_files:
  - css/src/lib.rs
  - css/src/css.rs
  - css/src/parser2.rs
  - css/src/props/basic/angle.rs
  - css/src/props/basic/animation.rs
  - css/src/props/basic/color.rs
  - css/src/props/basic/direction.rs
  - css/src/props/basic/error.rs
  - css/src/props/basic/font.rs
  - css/src/props/basic/geometry.rs
  - css/src/props/basic/length.rs
  - css/src/props/basic/mod.rs
  - css/src/props/basic/parse.rs
  - css/src/props/basic/pixel.rs
  - css/src/props/basic/time.rs
  - css/src/props/style/azul_exclusion.rs
  - css/src/props/style/background.rs
  - css/src/props/style/border.rs
  - css/src/props/style/border_radius.rs
  - css/src/props/style/box_shadow.rs
  - css/src/props/style/content.rs
  - css/src/props/style/effects.rs
  - css/src/props/style/filter.rs
  - css/src/props/style/lists.rs
  - css/src/props/style/mod.rs
  - css/src/props/style/scrollbar.rs
  - css/src/props/style/selection.rs
  - css/src/props/style/text.rs
  - css/src/props/style/transform.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T17:30:00Z
---

# Styling with CSS

Style is attached to a `Dom` by parsing a CSS string into a `Css` value and
calling `dom.style(css)`. The result is a `StyledDom` — the same `Dom` plus
matched declarations per node, ready for layout.

```rust
# extern crate azul;
use azul::prelude::*;

const STYLE: &str = r#"
    body { padding: 20px; background: #fafafa; }
    .card { padding: 16px; background: white; border-radius: 8px; }
    .card > h1 { font-size: 20px; color: #333; }
"#;

extern "C" fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let css = Css::from_string(STYLE.into());
    Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_class("card".into())
                .with_child(Dom::h1("Hello".into()))
        )
        .style(css)
}
```

```azul-render screenshot=styling-card width=400 height=160 subtitle="Card layout from the snippet above"
<body style="padding: 20px; background: #fafafa;">
  <div style="padding: 16px; background: white; border-radius: 8px;">
    <h1 style="font-size: 20px; color: #333;">Hello</h1>
  </div>
</body>
```

## Css and Stylesheet

`Css` is a flat list of `Stylesheet`s; each `Stylesheet` is a list of
`CssRuleBlock`s; each block pairs a selector path with declarations.

```rust,ignore
pub struct Css { pub stylesheets: StylesheetVec }
pub struct Stylesheet { pub rules: CssRuleBlockVec }
pub struct CssRuleBlock {
    pub path: CssPath,                 // selectors
    pub declarations: CssDeclarationVec,
    pub conditions: DynamicSelectorVec, // @media / @os / @theme conditions
}
```

Constructors:

| Function | Returns | Notes |
|---|---|---|
| `Css::empty()` | `Css` | No rules. Equivalent to `Css::default()`. |
| `Css::from_string(s)` | `Css` | Parses CSS source. Errors become warnings; partial CSS still produces output. |

`from_string` never fails. Unrecognized properties, malformed selectors and
unbalanced braces are downgraded to warnings — the rest of the stylesheet is
parsed normally. To collect those warnings during development, call
`azul_css::css::Css::from_string_with_warnings` directly on the underlying
crate type, which returns `(Css, Vec<CssParseWarnMsgOwned>)`.

## Selectors

Selectors map onto the W3C model. The full grammar lives in
`css/src/css.rs:1435`.

| Selector | Matches |
|---|---|
| `*` | Every node |
| `div`, `p`, `h1`, `body`, `span`, ... | Nodes whose `NodeType` matches the tag |
| `.name` | Nodes whose class list contains `name` |
| `#name` | The (single) node whose id is `name` |
| `[attr]`, `[attr="v"]`, `[attr~="v"]`, `[attr|="v"]`, `[attr^="v"]`, `[attr$="v"]`, `[attr*="v"]` | Attribute presence and string-match operators |
| `A B` | Descendant: `B` inside `A` |
| `A > B` | Direct child |
| `A + B` | Adjacent sibling |
| `A ~ B` | General sibling |

Pseudo-classes:

| Pseudo | Trigger |
|---|---|
| `:hover` | Cursor is over the node |
| `:active` | Cursor is pressed and over the node |
| `:focus` | Node holds keyboard focus |
| `:first`, `:last` | First / last child of its parent |
| `:nth-child(N)`, `:nth-child(even)`, `:nth-child(odd)`, `:nth-child(An+B)` | Position in parent's child list |
| `:lang(de)` | Node language matches BCP 47 tag |
| `:backdrop` | Window has lost focus (GTK convention) |
| `:dragging`, `:drag-over` | Drag-and-drop states |

The `NodeTypeTag` enum lists every supported tag — `Div`, `P`, `Body`,
`Span`, `Button`, `Input`, `Svg`, and the rest of the HTML5 + SVG element
set (`css/src/css.rs:598`).

```css
button:hover { background: #e0e0e0; }
.row > .cell:nth-child(odd) { background: #f5f5f5; }
input[type="email"]:focus { border-color: #0078d4; }
li:lang(en) { list-style-type: disc; }
```

## The cascade

When several rules match the same node, the rule with the highest specificity
wins. Specificity is the (id, class, type, universal) tuple defined by CSS:

| Selector kind | id | class | type |
|---|---|---|---|
| `#nav` | 1 | 0 | 0 |
| `.row` | 0 | 1 | 0 |
| `div` | 0 | 0 | 1 |
| `div.row.active` | 0 | 2 | 1 |
| `*` | 0 | 0 | 0 |

`Css::sort_by_specificity()` orders rule blocks within each stylesheet so the
matcher can apply them in cascade order. Calling it is idempotent. Inline CSS
on a `Dom` node (set via `with_css(...)` or `with_css_property(...)`) is
treated as if it had highest specificity and overrides matched rules.

Inheritable properties (`color`, `font-family`, `font-size`, `line-height`,
text properties, `cursor`, `visibility`) propagate from parent to child unless
the child overrides them. Layout properties — `width`, `padding`, `display`,
`flex-*`, `grid-*` — never inherit.

## Attaching CSS to a DOM

`Dom::style(css) -> StyledDom` runs selector matching against every node and
caches the resolved declarations on each node. The resulting `StyledDom` is
what the layout solver consumes.

```rust,ignore
let css = Css::from_string(SOURCE.into());
let body: Dom = build_dom();
let styled: StyledDom = body.style(css);
```

Two equivalent ways exist to set styles per-node without writing CSS strings:

- **Inline CSS** — `Dom::with_css("color: red; padding: 4px")` parses a CSS
  declaration list and attaches it to a node, or `Dom::with_css_property(prop)`
  attaches one already-typed `CssProperty`. Useful for one-off overrides
  ("this exact button should be red") without polluting a global stylesheet.
- **IDs and classes** — `Dom::with_id("name")` and `Dom::with_class("name")`
  add identifiers so a global rule like `.name { ... }` or `#name { ... }`
  can match the node.

Both compose: a node can carry classes *and* inline CSS, and the inline CSS
will win if it sets the same property the matched rule sets.

## Dynamic CSS variables

Property values can refer to a runtime-mutable variable using CSS variable
syntax with a default:

```css
#avatar { padding: var(--avatar_pad, 16px); }
```

A callback can override `--avatar_pad` for the next frame; the default value
is used until then. Internally these are represented as
`CssDeclaration::Dynamic(DynamicCssProperty)` (`css/src/css.rs:127`). The
override path is documented under [DOM and Callbacks](dom.md).

## Conditional rules: @-blocks

Rules can be nested inside `@`-blocks that gate them on runtime conditions.
The conditions are recorded on each `CssRuleBlock` as a
`DynamicSelectorVec` (`css/src/dynamic_selector.rs:50`).

| At-rule | Example | Condition |
|---|---|---|
| `@media (min-width: 800px)` | viewport width / height ranges, orientation, aspect-ratio | `MinMaxRange` evaluated against the window |
| `@os(linux)` | `windows`, `macos`, `linux`, `android`, `ios` | Detected platform |
| `@os-version(macos >= sonoma)` | OS family + version comparison | `OsVersionCondition` |
| `@theme(dark)` | `light`, `dark`, `system` | User's OS theme |
| `@lang("de-DE")` | BCP 47 language tag | Document or system language |
| `@container (min-width: 400px)` | size of nearest container-sized ancestor | `ContainerWidth` / `ContainerHeight` |

```css
@media (max-width: 600px) {
    .sidebar { display: none; }
}

@theme(dark) {
    body { background: #1e1e1e; color: #ddd; }
}

@os(macos) {
    button { font-family: "SF Pro Text"; }
}
```

Conditions stack: a rule block inside `@media (...) { @theme(dark) { ... } }`
must satisfy both. Multiple conditions on the same block are AND-combined.

## Property categories

Every recognized property is one variant of `CssProperty` (~250 variants
total, declared in `css/src/props/property.rs:561`). They group into:

- **Box model**: `width`, `height`, `min/max-*`, `padding-*`, `margin-*`,
  `box-sizing`, `border-*`, `border-radius`, `box-shadow`, `outline-*`.
- **Background**: `background`, `background-color`, `background-image`,
  `background-position`, `background-size`, `background-repeat`. Image values
  accept solid colors, linear/radial/conic gradients, and `url(...)`
  references.
- **Text**: `color`, `font-family`, `font-size`, `font-weight`, `font-style`,
  `line-height`, `letter-spacing`, `word-spacing`, `text-align`,
  `text-decoration`, `white-space`, `word-break`, `hyphens`,
  `text-transform`, `direction`, `writing-mode`.
- **Layout**: `display` (`flex`, `grid`, `block`, `inline-block`, `none`),
  `position`, `top/right/bottom/left`, `z-index`, `flex-*`, `grid-*`,
  `justify-*`, `align-*`, `gap`.
- **Effects**: `opacity`, `visibility`, `transform`, `filter`,
  `backdrop-filter`, `mix-blend-mode`, `cursor`.
- **Scrollbar**: `overflow-x`, `overflow-y`, `scrollbar-width`,
  `scrollbar-color`, plus `scrollbar-track`, `scrollbar-thumb`,
  `scrollbar-button`, `scrollbar-corner` for per-part theming.
- **Selection**: `selection-background-color`, `selection-color`,
  `selection-radius` (Azul-specific).

The full table with values per property is in
[CSS Properties Cheatsheet](styling/properties.md).

## Units

| Unit | Use |
|---|---|
| `px` | CSS pixels (resolution-independent) |
| `em` | Multiple of the current `font-size` |
| `rem` | Multiple of the root `font-size` |
| `pt` | 1pt = 4/3 px |
| `%` | Percentage of the parent's resolved value (per-property semantics) |
| `vw` / `vh` | Percentage of the viewport's width / height |
| `deg` / `rad` / `grad` / `turn` | Angles (`transform`, `gradient`, `filter`) |
| `s` / `ms` | Time (`transition`, `animation`, `caret-animation-duration`) |

A bare number with no unit is degrees in angle context (`rotate(45)`),
otherwise it parses as `px` for length-typed properties.

## Errors and warnings

The CSS parser never panics and never fails outright — bad rules are
dropped, good rules are kept. To inspect what was rejected during
development, call `azul_css::css::Css::from_string_with_warnings` directly
on the underlying crate type. It returns the same `Css` plus a
`Vec<CssParseWarnMsgOwned>`. Each warning carries the source byte position
(`ErrorLocation`) and an enum describing the cause: unknown property name,
invalid value for a known property, unbalanced braces, malformed selector.

## Where to look next

- [CSS Properties Cheatsheet](styling/properties.md) — every supported
  property with its value grammar, grouped by category.
- [System Themes](styling/themes.md) — discovering the user's OS theme,
  reading native colors and fonts, and writing themeable apps.
- [Layout](layout.md) — how the layout solver consumes a `StyledDom`.
- [Animations](animations.md) — `transition` and `animation` property
  semantics.
