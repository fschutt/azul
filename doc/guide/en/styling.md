---
slug: styling
title: Styling with CSS
language: en
canonical_slug: styling
audience: external
maturity: mature
guide_order: 40
topic_only: false
short_desc: Stylesheets and the cascade — selectors, pseudo-classes, at-rules, specificity, and the three ways to attach styles to a Dom.
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

A `Css` is a parsed stylesheet. You build one from a string, attach it to a
`Dom` subtree with `.style(css)`, and the cascade applies it on the next
layout pass. The dialect is a strict subset of standard CSS: tag, class, id,
and attribute selectors; descendant, child, sibling, and pseudo-class
combinators; the `@media`, `@os`, `@theme`, and `@lang` at-rules; and
shorthand properties for the common cases.

```rust,no_run
# use azul::prelude::*;
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
enum and feed the same cascade — the difference is *where* the rules live.

| API | Scope | When to use |
|---|---|---|
| `Dom::with_css(s)` | This node only | Inline tweaks; component-local styles |
| `Dom::style(css)` | This subtree | Component themes; per-page stylesheets |
| `Dom::with_css_property(p)` | This node | Programmatic single-property values |

```rust,no_run
# use azul::prelude::*;
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

`with_css` parses on every call but does *not* cascade — the parsed
properties just get pushed onto the node's `css_props` vector. The actual
matching, inheritance, and compaction happens once after `layout()`
returns, in a single pass. The "Where styles meet the DOM" section below
covers the pipeline, and [The DOM — when does CSS actually apply?](dom.md#when-does-css-actually-apply-not-until-after-layout)
walks through the timing.

For a static stylesheet shared across many nodes, build the `Css` once at
app startup and pass it through `style()`. Multiple `style()` calls stack:
later ones override earlier ones at equal specificity (`core/src/dom.rs:4906`).

## Selectors

The selector language matches W3C Selectors Level 3 minus a few rarely-used
pseudo-classes. From `css/src/css.rs:1432`:

| Selector | Example | Matches |
|---|---|---|
| `*` | `*` | every node |
| Type | `div`, `button`, `h1` | nodes whose `NodeType` matches the tag |
| Class | `.panel` | nodes with `with_class("panel")` |
| Id | `#sidebar` | nodes with `with_id("sidebar")` |
| Attribute | `[lang]`, `[lang="en"]`, `[lang^="en"]` | attribute presence/match |
| Descendant | `nav a` | `<a>` anywhere under `<nav>` |
| Child | `nav > a` | direct `<a>` child of `<nav>` |
| Adjacent sibling | `h2 + p` | `<p>` immediately after `<h2>` |
| General sibling | `h2 ~ p` | any `<p>` after `<h2>` at the same level |
| Pseudo-class | `:hover`, `:focus`, `:nth-child(2n+1)` | runtime-evaluated state |

Attribute operators follow the standard set (`=`, `~=`, `|=`, `^=`, `$=`,
`*=`) — see `AttributeMatchOp` at `css/src/css.rs:1481`.

## Pseudo-classes

State pseudo-classes evaluate on every frame. From `CssPathPseudoSelector`
at `css/src/css.rs:1556`:

- `:hover` — pointer is over the element
- `:active` — pointer is pressed and over the element
- `:focus` — element has keyboard focus
- `:first`, `:last` — first/last child of its parent
- `:nth-child(n)`, `:nth-child(2n+1)`, `:nth-child(odd)`, `:nth-child(even)` — positional
- `:lang(en)` — system locale matches the BCP 47 prefix
- `:backdrop` — the containing window is unfocused (use this for inactive-window styling)
- `:dragging`, `:drag-over` — drag-and-drop states

These run through `DynamicSelector::PseudoState` (`css/src/dynamic_selector.rs:78`)
without re-parsing the stylesheet.

## At-rules

Conditional rule blocks. The condition is evaluated per frame, so changing
the system theme or rotating a window adapts without re-cascading.

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    color: black;
    @theme dark { color: white; }
    @os linux { font-family: 'Cantarell'; }
    @os windows { font-family: 'Segoe UI'; }
    @os macos { font-family: '.SF NS'; }
    @media (max-width: 600px) { font-size: 14px; }
");
```

| At-rule | Backed by | Notes |
|---|---|---|
| `@os <name>` | `DynamicSelector::Os` | `windows`, `macos`, `linux`, `android`, `ios` |
| `@os-version` | `DynamicSelector::OsVersion` | `>= win-11`, `>= macos-14`, `linux gnome`, ... |
| `@theme <variant>` | `DynamicSelector::Theme` | `dark`, `light`, custom string |
| `@media (orientation: ...)` | `DynamicSelector::Orientation` | `portrait`, `landscape` |
| `@media (min-width: Npx)` etc. | `DynamicSelector::ViewportWidth/Height` | numeric viewport ranges |
| `@media (prefers-reduced-motion)` | `DynamicSelector::PrefersReducedMotion` | accessibility |
| `@media (prefers-contrast)` | `DynamicSelector::PrefersHighContrast` | accessibility |
| `@container` | `DynamicSelector::ContainerWidth/Height/Name` | container queries |
| `@lang(<bcp47>)` | `DynamicSelector::Language` | matches by prefix |

Conditions nest: an `@os linux` block can contain a `:hover` block, and the
two conditions both have to hold for the rule to apply
(`css/src/css.rs:528`, the `conditions` field on `CssRuleBlock`).

The full enum is at `css/src/dynamic_selector.rs:50`. See [System Themes](styling/themes.md)
for how the system populates these values from OS settings.

## The cascade and specificity

When more than one rule sets the same property, the cascade picks one. The
rules, in order:

1. Higher specificity wins.
2. Equal specificity → later rule wins.
3. `style()` calls stack; a later `style()` is "later" than an earlier one.
4. `with_css` (inline) outranks any stylesheet for that node.

Specificity is the W3C tuple `(ids, classes+pseudo+attrs, types, total)`,
computed by `get_specificity` at `css/src/css.rs:1693`. Call
`Css::sort_by_specificity()` (or `Stylesheet::sort_by_specificity()`) once
after parsing if you need deterministic order — the parser does not sort by
default. The framework runs the sort during cascade.

## Property values: the keyword set

Every typed property is wrapped in `CssPropertyValue<T>` (`css/src/css.rs:374`):

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
resolved value; `initial` resets to the property's spec default; `unset`
behaves as `inherit` for inheritable properties and `initial` otherwise.
`revert` returns to the user-agent default (the same defaults `core/src/ua_css.rs`
loads). The parser preserves the keyword and the cascade picks an explicit
value at the latest moment.

## Inheritable properties

Some properties propagate from parent to child by default; others do not.
Inheritability is fixed by the property and queryable via
`CssDeclaration::is_inheritable()` (`css/src/css.rs:161`). The inheritable
set follows CSS conventions:

- Text: `color`, `font-family`, `font-size`, `font-weight`, `line-height`,
  `text-align`, `letter-spacing`, `word-spacing`
- Cursor: `cursor`
- Visibility: `visibility`
- Custom: `hyphenation-language`

Layout properties (`width`, `padding`, `flex-grow`, ...) and most visual
properties (`background`, `border`, ...) do not inherit — write `inherit`
explicitly if you want one to.

## Dynamic properties (`var(...)`)

A `Dynamic` declaration is a CSS value swappable from Rust per frame.
Syntax in CSS: `var(--my_id, <default>)`. It compiles to `DynamicCssProperty`
(`css/src/css.rs:210`):

```rust,ignore
pub struct DynamicCssProperty {
    pub dynamic_id: AzString,
    pub default_value: CssProperty,
}
```

Use them when you want to change a single value (an accent color, a
spacing unit) without re-parsing the stylesheet. The override path lives
on `Dom::with_css_property`, which feeds a `CssPropertyWithConditions`
directly into the node's prop vector.

## `system:` keywords

Anywhere a colour or font is expected, `system:<name>` resolves at cascade
time against the running OS / theme:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    background: system:control;
    color: system:control-text;
    border: 1px solid system:separator;
    font-family: system:body;
    @theme dark { background: system:control; }
");
```

The lookup runs through `SystemStyle::detect()` at `AppConfig::create()`
time and re-evaluates per frame, so a theme switch (light → dark) updates
without re-parsing the stylesheet. The available names — `system:control`,
`system:accent`, `system:body`, `system:monospace`, … — are catalogued in
[System Themes](styling/themes.md). They compose with `@theme` and `@os`
the same way any other property would.

## Parsing CSS

`Css::from_string` returns the parsed stylesheet; the warning-collecting
variant returns parser diagnostics for unrecognised properties:

```rust,no_run
# use azul::prelude::*;
let (css, warnings) = Css::from_string_with_warnings("
    color: rebeccapurple;
    bogus-property: 1;
".into());

for w in &warnings {
    eprintln!("css warning at line {}: {:?}", w.location.line, w.warning);
}
```

The parser is feature-gated behind `parser` (always enabled in the
default build). Internals: `css/src/parser2.rs` is the entry point;
each property's `parse_*` function lives next to its type.

## Where styles meet the DOM — the deferred cascade

The cascade runs **once** per layout pass, after your `LayoutCallback`
returns. Inputs, in priority order (low → high):

1. The user-agent stylesheet (`core/src/ua_css.rs`) sets HTML defaults
   (`h1` font sizes, `<button>` padding, `<a>` color, ...).
2. Each `Css` attached to a subtree via `Dom::style(...)`, in `style()`
   push order.
3. Inline `with_css` rules on each node.
4. Programmatic `with_css_property` overrides (highest priority short of `!important`).

Inside `StyledDom::create_from_dom()` the framework collects every CSS
attachment from the recursive `Dom` tree, strips them off the nodes,
flattens the tree into a `CompactDom`, merges the stylesheets in push
order, and runs the cascade in a single sweep. The output is a `StyledDom`
— a flat, indexed view of the cascaded properties.

Then a second pass — `CssPropertyCache::build_compact_cache`
([`core/src/compact_cache_builder.rs:35`](../../core/src/compact_cache_builder.rs))
— re-encodes the layout-hot subset of properties into three packed tiers
the layout solver reads directly: a `Vec<u64>` for the 21 enum properties
(display, position, float, overflow, flex/grid alignment, font weight,
text-align, …), a hot dimensions array (width, height, padding, margin,
border, flex-basis), a cold paint-only array, and a text-properties array.
Less common properties (background, box-shadow, transform) stay in the
slow cascade path because the layout engine doesn't read them on hot
paths.

The reason this matters even at the styling layer: every CSS string you
parse via `with_css` or `Css::from_string` is "free" in the sense that it
is one parse and one push onto a vector. Selector matching, inheritance,
and the compact-cache build all happen once after you return — and the
framework only re-runs them on the deltas that need a recascade.

See [The DOM — when does CSS actually apply?](dom.md#when-does-css-actually-apply-not-until-after-layout)
for the per-frame walkthrough, and [Layout — what the solver actually reads](layout.md#what-the-solver-actually-reads)
for how the compact cache feeds the formatting algorithms.

Sub-pages cover the catalogue of properties, the platform integration,
and the icon and text-styling primitives:

- [CSS Properties Cheatsheet](styling/properties.md) — every property and the
  values it accepts.
- [System Themes](styling/themes.md) — `system:*` colors and fonts, `@theme`,
  `@os`, and accessibility queries.
- [Text and Fonts](styling/text-and-fonts.md) — `font-family`, weight, style,
  alignment, and the `system:` font keywords.
- [Icon Packs](styling/icon-packs.md) — registering image and font icons under
  named packs.

## Where to read the source

- `css/src/css.rs:25` — `Css` and `Stylesheet` definitions
- `css/src/css.rs:528` — `CssRuleBlock` (selector + declarations + conditions)
- `css/src/css.rs:1432` — `CssPathSelector` (selector AST)
- `css/src/css.rs:1556` — `CssPathPseudoSelector` (pseudo-class AST)
- `css/src/css.rs:1693` — `get_specificity`
- `css/src/dynamic_selector.rs:50` — `DynamicSelector` (`@os`, `@theme`, `@media`, `@lang`)
- `core/src/dom.rs:4906` — `Dom::style`
- `core/src/dom.rs:5099` — `Dom::with_css`
- `core/src/styled_dom.rs:1169` — `create_from_dom` (collect → cascade)
- `core/src/compact_cache_builder.rs:35` — `build_compact_cache` (the post-cascade compaction pass)
- `css/src/compact_cache.rs` — three-tier numeric encoding
