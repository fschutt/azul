---
slug: css-parser
title: CSS Parser
language: en
canonical_slug: css-parser
audience: contributor
maturity: mature
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - css/src/parser2.rs
  - css/src/props/property.rs
  - css/src/props/macros.rs
  - css/src/props/basic/parse.rs
  - css/src/props/basic/angle.rs
  - css/src/props/basic/animation.rs
  - css/src/props/basic/color.rs
  - css/src/props/basic/direction.rs
  - css/src/props/basic/error.rs
  - css/src/props/basic/font.rs
  - css/src/props/basic/geometry.rs
  - css/src/props/basic/length.rs
  - css/src/props/basic/mod.rs
  - css/src/props/basic/pixel.rs
  - css/src/props/basic/time.rs
  - css/src/props/formatter.rs
  - css/src/props/layout/column.rs
  - css/src/props/layout/dimensions.rs
  - css/src/props/layout/display.rs
  - css/src/props/layout/flex.rs
  - css/src/props/layout/flow.rs
  - css/src/props/layout/fragmentation.rs
  - css/src/props/layout/grid.rs
  - css/src/props/layout/mod.rs
  - css/src/props/layout/overflow.rs
  - css/src/props/layout/position.rs
  - css/src/props/layout/shape.rs
  - css/src/props/layout/spacing.rs
  - css/src/props/layout/table.rs
  - css/src/props/layout/text.rs
  - css/src/props/layout/wrapping.rs
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
generated_at: 2026-05-01T20:43:38Z
---

The CSS parser turns a `&str` stylesheet into a `Css` value (`css/src/css.rs`) — a `Vec<Stylesheet>` of typed [`CssRuleBlock`](https://github.com/maps4print/azul/blob/master/css/src/css.rs) entries, each pairing a [`CssPath`](https://github.com/maps4print/azul/blob/master/css/src/css.rs) selector with a `Vec<CssDeclaration>`. The entry point is `new_from_str` (`css/src/parser2.rs:658`); it never panics.

## File map

| File | Role |
|---|---|
| `css/src/parser2.rs:658` | `new_from_str` — top-level CSS parser, returns `(Css, Vec<CssParseWarnMsg>)` |
| `css/src/parser2.rs:790` | `parse_css_path` — selector parser (used by `<style>` elements with explicit selectors and by codegen) |
| `css/src/props/property.rs:560` | `CssProperty` enum — 180 typed property variants |
| `css/src/props/property.rs:805` | `CssPropertyType` enum — 180 discriminants for property keys |
| `css/src/props/property.rs:2875` | `parse_css_property(key, value) -> Result<CssProperty, _>` — dispatch table |
| `css/src/props/macros.rs` | `impl_pixel_value!`, `impl_percentage_value!`, `css_property_from_type!` macros |
| `css/src/props/basic/` | primitive types (length, color, angle, font, time, geometry, image, direction) |
| `css/src/props/layout/` | layout properties (display, flex, grid, position, dimensions, …) |
| `css/src/props/style/` | paint properties (background, border, shadow, transform, filter, …) |

## Parser architecture

Three layers, top to bottom:

1. **Top-level CSS parser** (`parser2.rs`). Tokenizes via [`azul_simplecss::Tokenizer`](https://github.com/RazrFalcon/simplecss), handles `@media` / `@lang` / `@theme` / `@supports` blocks, builds an intermediate `UnparsedCssRuleBlock<'a>` per rule, then resolves declarations into typed `CssDeclaration`s.
2. **Property dispatch** (`property.rs`). `parse_css_property(key, value)` looks at `key: CssPropertyType` and routes to the matching `parse_*` function in `props/layout/` or `props/style/`.
3. **Per-property parsers** (`props/layout/*.rs`, `props/style/*.rs`). Each parses one specific property syntax and returns its typed value.

Errors at every layer are non-fatal. Syntax errors become `CssParseWarnMsg` items in the warnings vector; an unparseable property is dropped while the rest of the rule survives.

## Top-level entry: `new_from_str`

```rust,ignore
use azul_css::parser2::new_from_str;

let (css, warnings) = new_from_str("\
    body { background: white; }\n\
    .button:hover { color: blue; }\n\
");

assert!(warnings.is_empty());
```

The signature is `pub fn new_from_str<'a>(css_string: &'a str) -> (Css, Vec<CssParseWarnMsg<'a>>)` (`css/src/parser2.rs:658`). The warnings borrow from `css_string`; the returned `Css` is owned (selectors and values are copied into `AzString` / typed values).

A hard tokenizer error wraps the whole stylesheet into a single `ParseError` warning; the `Css` value is returned empty rather than `None`. This lets the renderer keep going on malformed user CSS.

## Selectors: `CssPath` and `parse_css_path`

`parse_css_path(input) -> Result<CssPath, CssPathParseError>` (`css/src/parser2.rs:790`) handles the selector half of a rule independently. It's used by:

- `parser2.rs` itself (called per rule).
- `core/src/style.rs` for runtime `StyledDom::with_css(...)` overrides.
- `dll/src/web/cb_gen.rs` for the codegen pipeline that compiles HTML+CSS to const Rust.

```rust,ignore
use azul_css::parser2::parse_css_path;
use azul_css::css::CssPathSelector;

let path = parse_css_path("div > .item:hover").unwrap();
// path.selectors is a Vec<CssPathSelector> in order:
//   Type(Div), DirectChildren, Class("item"), PseudoSelector(Hover)
```

Supported tokens map 1:1 to `azul_simplecss::Token`:

| Token | Selector |
|---|---|
| `*` | `Global` |
| `tag` | `Type(NodeTypeTag)` if the tag is recognized; silently dropped otherwise |
| `#id` | `Id(AzString)` |
| `.class` | `Class(AzString)` |
| ` ` (space) | `Children` (descendant) |
| `>` | `DirectChildren` |
| `+` | `AdjacentSibling` |
| `~` | `GeneralSibling` |
| `:foo`, `:foo(arg)` | `PseudoSelector` (see `pseudo_selector_from_str`) |

Attribute selectors (`[lang="de"]`) are *not* parsed by this function — they live in `azul_css::dynamic_selector::DynamicSelector` and are wired in by the surrounding `@lang` / conditional infrastructure.

## Property dispatch: `parse_css_property`

```rust,ignore
use azul_css::props::property::{CssPropertyType, parse_css_property};

let prop = parse_css_property(CssPropertyType::Width, "50%").unwrap();
// CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth::percent(50.0)))
```

Defined at `css/src/props/property.rs:2875`. Three short-circuits run before the per-property dispatch:

```rust,ignore
match value.trim() {
    "auto"    if !has_typed_auto(key)    => return Ok(CssProperty::auto(key)),
    "none"    if !has_typed_none(key)    => return Ok(CssProperty::none(key)),
    "initial"                            => return Ok(CssProperty::initial(key)),
    "inherit"                            => return Ok(CssProperty::inherit(key)),
    _ => { /* per-property dispatch */ }
}
```

`has_typed_auto` / `has_typed_none` (lines 2887 and 2900) list the properties for which `auto`/`none` is a *typed* value rather than the generic CSS keyword (e.g. `display: none` is `LayoutDisplay::None`, not `CssPropertyValue::None`). The dispatch then matches the 180 variants of `CssPropertyType` to their parser:

```rust,ignore
match key {
    CssPropertyType::Width => parse_layout_width(value)?.into(),
    CssPropertyType::FlexGrow => parse_layout_flex_grow(value)?.into(),
    // …
}
```

Each `parse_<prop>` function lives next to the type it produces.

## Property modules

Properties are grouped by their effect on the layout pipeline.

- **`props/basic/`** — primitive value types: `pixel.rs` (`PixelValue`), `length.rs` (`FloatValue`, `PercentageValue`), `color.rs` (`ColorU`, `ColorF`), `angle.rs` (`AngleValue`), `time.rs` (`CssDuration`), `font.rs` (font-family/weight/style), `direction.rs` (gradient `Direction` / `DirectionCorner`), `geometry.rs` (`LayoutPoint`/`Size`/`Rect`), `animation.rs` (SVG curves and timing functions used by `transition-timing-function`), `image.rs` (re-exported via `parse.rs`), `error.rs` (`#[repr(C)]` mirrors of `core::num::ParseFloatError` / `ParseIntError`).
- **`props/layout/`** — properties that change box geometry and feed into the solver: `display.rs`, `dimensions.rs` (width/height/min/max + box-sizing), `position.rs` (top/right/bottom/left + position + z-index), `flex.rs`, `grid.rs`, `column.rs`, `flow.rs`, `fragmentation.rs` (`break-before`/`-after`/`-inside`), `overflow.rs`, `shape.rs` (`shape-outside`/`shape-inside`), `spacing.rs` (padding/margin/border-width/gap), `table.rs`, `text.rs` (text properties that influence layout: `text-align`, `letter-spacing`, `tab-size`, …), `wrapping.rs` (`white-space`, `word-break`, `overflow-wrap`, `writing-mode`, `direction`, `clear`).
- **`props/style/`** — properties that only affect paint: `background.rs`, `border.rs`, `border_radius.rs`, `box_shadow.rs`, `content.rs` (`content`, `counter-reset`, `string-set`), `effects.rs` (`opacity`, `mix-blend-mode`, …), `filter.rs`, `lists.rs`, `scrollbar.rs`, `selection.rs` (text-selection colors), `text.rs` (paint-only text properties: color, decoration, shadow), `transform.rs`, `azul_exclusion.rs` (the `-azul-*` extensions for floats / hyphenation / exclusions).

The split is what enables the `RelayoutScope` classification in `CssPropertyType::relayout_scope` — see [Cascade, Inheritance, Restyle](cascade.md).

## Primitive value types

### `PixelValue` and `FloatValue`

```rust,ignore
#[repr(C)]
pub struct FloatValue { pub number: isize }      // value × 1000

#[repr(C)]
pub struct PixelValue {
    pub metric: SizeMetric,                       // Px, Em, Pt, Percent, In, Cm, Mm
    pub number: FloatValue,
}
```

`FloatValue` is fixed-point at 0.001 precision (`length.rs:13`, multiplier = 1000). The fixed-point representation is what makes pixel values usable in `const` context — `FloatValue::const_new(45)` works at compile time because there's no `f32`. The `FP_PRECISION_MULTIPLIER` is also why integer-only sizes like `5px` round-trip exactly.

`PixelValue::px(5.0)`, `PixelValue::em(1.5)`, `PixelValue::percent(50.0)` are the runtime constructors. The `const_*` variants are used by codegen and hand-rolled UA-CSS tables.

### `AngleValue`

```rust,ignore
#[repr(C)]
pub struct AngleValue {
    pub metric: AngleMetric,                      // Degree, Radians, Grad, Turn, Percent
    pub number: FloatValue,
}
```

`AngleValue::to_degrees()` normalizes to `[0, 360)` modulo. `AngleValue::to_degrees_raw()` does *not* normalize — conic gradients need to distinguish 360deg from 0deg. Parser at `angle.rs:226`; bare numbers default to degrees.

### `ColorU`, `ColorF`

`ColorU` is `[r, g, b, a]: u8` — the canonical color representation throughout the engine. `ColorF` is the f32 variant, used by WebRender and the GPU compositor. `ColorOrSystem` (`color.rs`) carries either a literal color or a system-color name like `Canvas` / `CanvasText` so dark-mode resolution can defer until paint.

### `CssDuration`

```rust,ignore
#[repr(C)]
pub struct CssDuration { pub inner: u32 }   // milliseconds
```

`parse_duration("1.5s") == CssDuration { inner: 1500 }`. Negative durations error.

## Macros: `impl_pixel_value!` and `css_property_from_type!`

`css/src/props/macros.rs` exists to keep the per-property files boilerplate-free.

`impl_pixel_value!(LayoutWidth)` (`macros.rs:9`) generates 16 methods on a struct with an `inner: PixelValue` field: `zero()`, `const_px(isize)`, `const_em`, `const_pt`, `const_percent`, `const_in`, `const_cm`, `const_mm`, `const_from_metric`, `px(f32)`, `em`, `pt`, `percent`, `from_metric`, `interpolate`. Every numeric layout property uses this macro.

`impl_percentage_value!(StyleOpacity)` (`macros.rs:126`) does the equivalent for percentage wrappers, plus `Display`/`Debug` impls that print as `"X%"`.

`css_property_from_type!($key, $variant)` (`macros.rs:172`) is the giant match table that maps a `CssPropertyType` discriminant to a `CssProperty(CssPropertyValue::Variant)` constructor. It's invoked from `CssProperty::auto(key)`, `CssProperty::none(key)`, `CssProperty::initial(key)`, `CssProperty::inherit(key)` so that the four generic CSS keywords don't need 180 manual match arms.

## Shared parsing helpers in `basic/parse.rs`

| Helper | Purpose |
|---|---|
| `split_string_respect_comma(input)` | `url(a,b), url(c)` → `["url(a,b)", "url(c)"]`. Tracks paren depth. |
| `split_string_respect_whitespace(input)` | `"translateX(10px) rotate(90deg)"` → two items. Same depth tracking. |
| `parse_parentheses(input, &["url"]) -> Result<(stopword, inside)>` | Matches `<stopword>(...)` and returns the contents. |
| `strip_quotes(input) -> Result<QuoteStripped>` | Strips matching `"..."` or `'...'`. Errors if quotes don't match. |
| `parse_image(input) -> Result<AzString>` | `strip_quotes`, falling back to the trimmed input on no-quotes. |

These are explicitly not glob-re-exported from `basic/mod.rs` — use qualified paths (`crate::props::basic::parse::split_string_respect_comma`) so the helpers don't collide with property-specific parsers.

## Errors: owned vs borrowed

Every parser error type has two forms:

```rust,ignore
pub enum CssAngleValueParseError<'a> {           // borrowed — used during parsing
    EmptyString,
    NoValueGiven(&'a str, AngleMetric),
    ValueParseErr(ParseFloatError, &'a str),
    InvalidAngle(&'a str),
}

#[repr(C, u8)]
pub enum CssAngleValueParseErrorOwned {          // owned — for FFI / storage
    EmptyString,
    NoValueGiven(AngleNoValueGivenError),
    ValueParseErr(ParseFloatErrorWithInput),
    InvalidAngle(AzString),
}
```

`to_contained()` on the borrowed form clones strings into `AzString`s; `to_shared()` on the owned form returns a borrowed wrapper. Owned forms are `#[repr(C, u8)]` so they cross the FFI boundary.

`ParseFloatError` and `ParseIntError` (`basic/error.rs`) are `#[repr(C)]` mirrors of the `core::num` types — Rust's privacy on `ParseFloatError::kind` means we can't pass it through FFI directly, so the kind is reconstructed by comparing against known instances (`error.rs:32`).

## Adding a new CSS property

Putting it all together, here's what it takes to add `text-stroke: 1px red`:

1. Pick a module — `style/text.rs` if paint-only, `layout/text.rs` if it influences layout. Define the typed value struct (e.g. `StyleTextStroke { width: PixelValue, color: ColorU }`) plus a `CssPropertyValue` typedef and option/vec wrappers.
2. Add a `parse_style_text_stroke` function in the same file. Use `split_string_respect_whitespace` to tokenize and existing primitive parsers (`parse_pixel_value`, `parse_css_color`).
3. Add a variant to `CssProperty` (`property.rs:560`) and `CssPropertyType` (`property.rs:805`).
4. Add an arm to `parse_css_property` (`property.rs:2875`) routing to your parser.
5. Add the new variant to `css_property_from_type!` (`macros.rs:172`) so `auto`/`none`/`initial`/`inherit` work generically.
6. Implement `CssProperty::get_type()`, `relayout_scope()`, and the formatter (`props/formatter.rs`).
7. If the property should be inheritable, add it to the inheritance lists in `core/src/prop_cache.rs` and `core/src/compact_cache_builder.rs`.
8. If it has a UA default, add it to `core/src/ua_css.rs`.
9. If it's frequently set, encode into the [Compact Property Cache](compact-cache.md) instead of leaving it on the slow cascade path.

Each step is mechanical except the encoding decision — see `compact-cache.md` for that trade-off.

## See also

- [DOM Internals](dom.md) — the consumer of parsed CSS via `NodeData::css_props` and `Css` stylesheets.
- [Cascade, Inheritance, Restyle](cascade.md) — how the parsed `CssProperty` values become per-node resolved values.
- [Compact Property Cache](compact-cache.md) — where the resolved values end up.
