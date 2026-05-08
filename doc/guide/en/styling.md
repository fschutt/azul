---
slug: styling
title: Styling with CSS
language: en
canonical_slug: styling
audience: external
maturity: mature
guide_order: 40
topic_only: false
short_desc: Stylesheets, selectors, and the cascade
prerequisites: [dom]
tracked_files:
  - css/src/lib.rs
  - css/src/css.rs
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
  - css/src/system.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Styling with CSS

## Overview

A `Css` is a parsed stylesheet. You build one from a string, attach it to a
`Dom` subtree with `.style(css)`, and the cascade applies it on the next
layout pass. The dialect is a strict subset of standard CSS: tag, class, id,
and attribute selectors; descendant, child, sibling, and pseudo-class
combinators; the `@media`, `@os`, `@theme`, and `@lang` at-rules; and
shorthand properties for the common cases.

```rust,no_run
use azul::prelude::*;

let css = Css::from_string("
    body { font-family: sans-serif; padding: 20px; }
    .panel { background: #f0f0f0; border: 1px solid #ccc; padding: 12px; }
    .panel:hover { background: #e8e8e8; }
".into());

let _ = Dom::create_body()
    .with_child(Dom::create_div().with_class("panel".into()))
    .style(css);
```

```azul-render screenshot=styling-panel width=400 height=160 subtitle="A class-styled panel with hover-ready rules"
<html>
<head><style>
body { font-family: sans-serif; padding: 20px; background: white; }
.panel { background: #f0f0f0; border: 1px solid #ccc; padding: 12px; border-radius: 4px; }
</style></head>
<body><div class="panel">A panel with class-based styling</div></body>
</html>
```

## Three ways to attach styles

Pick the one that matches scope. They all parse to the same `CssProperty`
enum and feed the same cascade. The difference is *where* the rules live.

- **`Dom::with_css(s)`** scopes to *this node only*. The CSS string is
  parsed and pushed onto the node's inline-property list. Use it for
  inline tweaks and component-local styles.
- **`Dom::style(css)`** scopes to *this subtree*. The parsed `Css` is
  attached to the subtree root and the cascade walks it during the
  per-frame pass. Use it for component themes and per-page stylesheets.
- **`Dom::with_css_property(p)`** scopes to *this node*, programmatic
  single-property override. Use it when you have a typed `CssProperty`
  value and don't want to round-trip through string parsing.

```rust,no_run
use azul::prelude::*;

// 1. Inline string on one node
let _ = Dom::create_div()
    .with_css("color: blue; padding: 4px; :hover { color: red; }");

// 2. Stylesheet attached to a subtree
let theme = Css::from_string(".btn { background: #1976d2; color: white; }".into());
let _ = Dom::create_body()
    .style(theme)
    .with_child(
        Dom::create_button("Save", SmallAriaInfo::label("Save"))
            .with_class("btn".into())
    );
```

`with_css` parses on every call but doesn't cascade. The parsed
properties get pushed onto the node's inline-property list. Matching and
inheritance happen once after `layout()` returns, in a single pass.
[The DOM page](dom.md#when-does-css-actually-apply-not-until-after-layout)
walks through the timing.

For a static stylesheet shared across many nodes, build the `Css` once at
app startup and pass it through `style()`. Multiple `style()` calls stack.
A later one overrides earlier ones at equal specificity.

## Selectors

The selector language matches W3C Selectors Level 3 minus a few rarely-used
pseudo-classes:

- **Universal**: `*` matches every node.
- **Type**: `div`, `button`, `h1` match nodes whose tag matches.
- **Class**: `.panel` matches nodes with `with_class("panel")`.
- **Id**: `#sidebar` matches nodes with `with_id("sidebar")`.
- **Attribute**: `[lang]`, `[lang="en"]`, `[lang^="en"]` test attribute
  presence and match. The operators are `=`, `~=`, `|=`, `^=`, `$=`, `*=`
  (see `AttributeMatchOp`).
- **Descendant**: `nav a` matches an `<a>` anywhere under `<nav>`.
- **Child**: `nav > a` matches a direct `<a>` child of `<nav>`.
- **Adjacent sibling**: `h2 + p` matches a `<p>` immediately after `<h2>`.
- **General sibling**: `h2 ~ p` matches any `<p>` after `<h2>` at the
  same level.
- **Pseudo-class**: `:hover`, `:focus`, `:nth-child(2n+1)` are
  runtime-evaluated state.

## Pseudo-classes

State pseudo-classes evaluate on every frame:

- `:hover`: pointer is over the element.
- `:active`: pointer is pressed and over the element.
- `:focus`: element has keyboard focus.
- `:first`, `:last`: first or last child of its parent.
- `:nth-child(n)`, `:nth-child(2n+1)`, `:nth-child(odd)`, `:nth-child(even)`: positional.
- `:lang(en)`: system locale matches the BCP 47 prefix.
- `:backdrop`: the containing window is unfocused. Use it for inactive-window styling.
- `:dragging`, `:drag-over`: drag-and-drop states.

These run without re-parsing the stylesheet.

## At-rules

Conditional rule blocks. The condition is evaluated per frame, so changing
the system theme or rotating a window adapts without re-cascading.

```rust,no_run
use azul::prelude::*;

let _ = Dom::create_div().with_css("
    color: black;
    @theme dark { color: white; }
    @os linux { font-family: 'Cantarell'; }
    @os windows { font-family: 'Segoe UI'; }
    @os macos { font-family: '.SF NS'; }
    @media (max-width: 600px) { font-size: 14px; }
");
```

- `@os <name>` / `@os(<name>)` matches the host platform. Names:
  `windows`, `macos`, `linux`, `android`, `ios`, `apple` (macOS+iOS),
  `web`, `any`.
- `@os(<family>:<de>)` narrows to a Linux desktop environment. DEs:
  `gnome`, `kde`, `xfce`, `unity`, `cinnamon`, `mate`. Example:
  `@os(linux:gnome) { ... }`.
- `@os(<family> <op> <version>)` narrows to an OS version. Operators:
  `>=`, `<=`, `=`. Examples: `@os(windows >= win-11)`,
  `@os(macos = sonoma)`.
- `@os(<family>:<de> <op> <version>)` combines DE with a version.
  Example: `@os(linux:gnome > 40)`.
- `@theme <variant>` matches the system theme. Variants: `dark`, `light`,
  plus any custom string.
- `@media (orientation: ...)` accepts `portrait` or `landscape`.
- `@media (min-width: Npx)` and friends match numeric viewport ranges.
- `@media (prefers-reduced-motion)` is the accessibility query for motion.
- `@media (prefers-contrast)` is the accessibility query for contrast.
- `@container` enables container queries by width, height, or name.
- `@lang(<bcp47>)` matches the system language by prefix.

Conditions nest. An `@os linux` block can contain a `:hover` block, and
both conditions have to hold for the rule to apply.

See [System Themes](styling/themes.md) for how the system populates these
values from OS settings.

## The cascade and specificity

When more than one rule sets the same property, the cascade picks one. The
rules, in order:

1. Higher specificity wins.
2. Equal specificity. The later rule wins.
3. `style()` calls stack. A later `style()` is "later" than an earlier one.
4. `with_css` (inline) outranks any stylesheet for that node.

Specificity is the W3C tuple `(ids, classes+pseudo+attrs, types, total)`.
Call `Css::sort_by_specificity()` once after parsing if you need
deterministic order. The parser doesn't sort by default. The framework
runs the sort during cascade.

## Property values: the keyword set

Every typed property is wrapped in `CssPropertyValue<T>`:

```rust,ignore
pub enum CssPropertyValue<T> {
    Auto,
    None,
    Initial,
    Inherit,
    Revert,
    Unset,
    Exact(T),
}
```

Most properties accept the CSS-wide keywords. `inherit` walks to the parent's
resolved value. `initial` resets to the property's spec default. `unset`
behaves as `inherit` for inheritable properties and `initial` otherwise.
`revert` returns to the user-agent default. The parser preserves the
keyword and the cascade picks an explicit value at the latest moment.

## Inheritable properties

Some properties propagate from parent to child by default; others don't.
Inheritability is fixed by the property. The inheritable set follows CSS
conventions:

- Text: `color`, `font-family`, `font-size`, `font-weight`, `line-height`,
  `text-align`, `letter-spacing`, `word-spacing`.
- Cursor: `cursor`.
- Visibility: `visibility`.
- Custom: `hyphenation-language`.

Layout properties (`width`, `padding`, `flex-grow`, ...) and most visual
properties (`background`, `border`, ...) don't inherit. Write `inherit`
explicitly if you want one to.

## Dynamic properties (var(...))

A dynamic declaration is a CSS value swappable from Rust per frame.
Syntax in CSS: `var(--my_id, <default>)`. It compiles to
`DynamicCssProperty`:

```rust,ignore
pub struct DynamicCssProperty {
    pub dynamic_id: AzString,
    pub default_value: CssProperty,
}
```

Use them when you want to change a single value (an accent color, a
spacing unit) without re-parsing the stylesheet. The override path lives
on `Dom::with_css_property`.

## system: keywords

Anywhere a colour or font is expected, `system:<name>` resolves at cascade
time against the running OS and theme:

```rust,no_run
use azul::prelude::*;

let _ = Dom::create_div().with_css("
    background: system:control;
    color: system:control-text;
    border: 1px solid system:separator;
    font-family: system:body;
    @theme dark { background: system:control; }
");
```

The lookup re-evaluates per frame, so a theme switch (light to dark)
updates without re-parsing the stylesheet. The available names
(`system:control`, `system:accent`, `system:body`, `system:monospace`,
...) are catalogued in [System Themes](styling/themes.md). They compose
with `@theme` and `@os` the same way any other property would.

## Parsing CSS

`Css::from_string` returns the parsed stylesheet:

```rust,no_run
use azul::prelude::*;

let css = Css::from_string("
    color: rebeccapurple;
".into());
```

The parser is feature-gated behind `parser` (always enabled in the
default build).

## Where styles meet the DOM

The cascade runs once per layout pass, after your `LayoutCallback` returns.
Inputs, in priority order (low to high):

1. The user-agent stylesheet sets HTML defaults (`h1` font sizes,
   `<button>` padding, `<a>` color, ...).
2. Each `Css` attached to a subtree via `Dom::style(...)`, in `style()`
   push order.
3. Inline `with_css` rules on each node.
4. Programmatic `with_css_property` overrides (highest priority short of
   `!important`).

Internally the framework collects every CSS attachment from the recursive
`Dom` tree, merges the stylesheets in push order, and runs the cascade in
a single sweep. The output is a `StyledDom`: a flat, indexed view of the
cascaded properties. Subsequent frames only re-cascade the nodes whose
inputs actually changed.

The reason this matters even at the styling layer: every CSS string you
parse via `with_css` or `Css::from_string` is "free" in the sense that it
is one parse and one push onto a list. Selector matching and inheritance
happen once after you return.

See [The DOM](dom.md#when-does-css-actually-apply-not-until-after-layout)
for the per-frame walkthrough, and [Layout](layout.md) for how the
cascaded properties feed the formatting algorithms.

Sub-pages cover the catalogue of properties, the platform integration,
and the icon and text-styling primitives:

- [CSS Properties Cheatsheet](styling/properties.md). Every property and
  the values it accepts.
- [System Themes](styling/themes.md). `system:*` colors and fonts,
  `@theme`, `@os`, and accessibility queries.
- [Text and Fonts](styling/text-and-fonts.md). `font-family`, weight,
  style, alignment, plus the `system:` font keywords.
- [Icon Packs](styling/icon-packs.md). Registering image and font icons
  under named packs.

## Coming Up Next

- [CSS Properties](styling/properties.md) — Reference of every CSS property azul recognises
- [System Themes](styling/themes.md) — System colors, `@theme`, `@os`, and accessibility queries
- [Layout](layout.md) — Overview of the layout solver
- [Document Object Model](dom.md) — The Dom tree - node types, hierarchy, and CSS
