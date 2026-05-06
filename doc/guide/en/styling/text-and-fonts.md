---
slug: styling/text-and-fonts
title: Styling Text
language: en
canonical_slug: styling/text-and-fonts
audience: external
maturity: wip
guide_order: 43
topic_only: false
short_desc: Font family, size, weight, alignment, decoration, and the system font keywords
prerequisites: [styling]
tracked_files:
  - css/src/props/basic/font.rs
  - css/src/props/layout/text.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:49:28Z
---

> **WIP.** Font fallback and the text shaper are stable; some advanced
> typography (`font-feature-settings`, variable axes) is not yet wired through
> from CSS to the layout engine. The properties below all parse and apply.

Text rendering is driven by four font properties (`font-family`, `font-size`,
`font-weight`, `font-style`) and modulated by alignment, justification, and
line metrics. The shaper turns the resolved style plus a `&str` into glyph
runs; the layout solver decides where each line breaks and how text fits inside
its box.

## `font-family`

`StyleFontFamily` (`css/src/props/basic/font.rs:287`) is one of:

| variant | example CSS | resolved at runtime to |
|---|---|---|
| `System(name)` | `"Arial"`, `Times New Roman` | a face matching the family name |
| `SystemType(SystemFontType)` | `system:ui`, `system:monospace:bold` | a platform UI font |
| `File(url)` | `url(/fonts/Inter.ttf)` | a font file loaded from the URL |
| `Ref(FontRef)` | not addressable from CSS | a pre-loaded font handle |

The property takes a comma-separated *fallback list*. Each entry is tried in
order; the first one that resolves to a face containing the requested glyph
wins.

```rust
# fn body() -> &'static str {
"body { font-family: \"Inter\", system:ui, sans-serif; }
code { font-family: system:monospace, \"SF Mono\", Menlo, monospace; }"
# }
```

### `system:` fonts

The `system:<role>[:<variant>]` prefix selects a platform UI font without
hard-coding a family name. It resolves to the OS's preferred face for that
role:

| selector | role |
|---|---|
| `system:ui` | the default UI font |
| `system:ui:bold` | the UI bold variant |
| `system:monospace`, `system:monospace:bold`, `system:monospace:italic` | platform monospace |
| `system:title`, `system:title:bold` | larger UI text |
| `system:menu` | menu/menu-item label font |
| `system:small` | small-print UI font |
| `system:serif`, `system:serif:bold` | platform serif |

If the role isn't recognised, the parser keeps the literal string as a
`System` family (so `system:invalid` becomes the family named
`"system:invalid"`).

### `FontRef` and pre-loaded fonts

`FontRef` (`css/src/props/basic/font.rs:176`) is a reference-counted handle
backed by an atomic counter. It points at parsed font data (the
`ParsedFont` from the layout crate). You won't construct `FontRef` from CSS,
but you'll see it on the Rust side when binding a font once and using it
across multiple DOMs.

## `font-size`

`StyleFontSize` (`css/src/props/basic/font.rs:144`) wraps a `PixelValue`. The
default is `12pt` — pick whatever your design system needs:

```rust
# fn body() -> &'static str {
"h1 { font-size: 28px; }
p  { font-size: 1em; }
small { font-size: 80%; }"
# }
```

`em` is relative to the parent's `font-size`; `rem` is relative to the root.
`%` resolves the same way as `em`.

## `font-weight`

`StyleFontWeight` (`css/src/props/basic/font.rs:45`):

| value | numeric |
|---|---|
| `lighter` | lighter than parent |
| `100` … `300` | `W100` … `W300` |
| `normal`, `400` | `Normal` |
| `500`, `600` | `W500`, `W600` |
| `bold`, `700` | `Bold` |
| `800`, `900` | `W800`, `W900` |
| `bolder` | heavier than parent |

The numeric scale is the OpenType weight class; the parser maps standard
numbers to enum variants. `450` and other in-between numbers are *not*
accepted — use the named variant for the closest weight.

## `font-style`

`StyleFontStyle` (`css/src/props/basic/font.rs:107`):

```rust,ignore
StyleFontStyle::Normal   // upright (default)
StyleFontStyle::Italic   // italic face
StyleFontStyle::Oblique  // oblique (synthesised slant if no italic face)
```

## `text-align`

Horizontal alignment of inline content within its line box:

| value | effect |
|---|---|
| `start` (default) | left in LTR, right in RTL |
| `end` | right in LTR, left in RTL |
| `left` / `right` | absolute, ignoring text direction |
| `center` | centred |
| `justify` | spread to fill the line; see `text-justify` below |

## `text-justify`

`LayoutTextJustify` (`css/src/props/layout/text.rs:14`) refines what
`text-align: justify` does:

| value | distributes whitespace by |
|---|---|
| `auto` (default) | UA picks the appropriate algorithm for the script |
| `none` | no justification (`text-align: justify` is treated as `start`) |
| `inter-word` | only between words |
| `inter-character` | between every character (CJK-friendly) |
| `distribute` | legacy alias of `inter-character` |

The legacy `distribute` value computes to `inter-character` per the spec
(`css/src/props/layout/text.rs:88`).

## Line metrics

| property | type | default | meaning |
|---|---|---|---|
| `line-height` | `<percentage>` | `120%` | multiplier applied to `font-size` |
| `letter-spacing` | `<length>` | `0px` | added between every glyph |
| `word-spacing` | `<length>` | `0px` | added between words |
| `tab-size` | `<length>` | `8em` | width of a tab character |

```rust
# fn body() -> &'static str {
".body { line-height: 150%; letter-spacing: 0.02em; }
pre   { tab-size: 4em; }"
# }
```

## Wrapping and breaks

`white-space` controls collapsing and wrapping:

| value | collapse whitespace | wrap |
|---|---|---|
| `normal` (default) | yes | yes |
| `pre` | no | no (only at explicit breaks) |
| `nowrap` | yes | no |
| `pre-wrap` | no | yes |
| `pre-line` | yes (newlines preserved) | yes |
| `break-spaces` | no, breaks at every space | yes |

`word-break` and `overflow-wrap` decide what to do with long unbreakable
sequences (URLs, code, CJK):

```rust,ignore
StyleWordBreak::Normal      // default
StyleWordBreak::BreakAll    // allow break between any two characters
StyleWordBreak::KeepAll     // forbid break inside CJK
StyleWordBreak::BreakWord   // deprecated alias

StyleOverflowWrap::Normal   // only at allowed break points
StyleOverflowWrap::Anywhere // break anywhere to prevent overflow
StyleOverflowWrap::BreakWord
```

`hyphens: none | manual | auto` enables soft-hyphen breaking. `auto` requires
the layout crate's hyphenation resources for the active language.

## Direction and writing mode

For bidirectional text:

```rust,ignore
StyleDirection::Ltr   // left-to-right (default)
StyleDirection::Rtl   // right-to-left

StyleUnicodeBidi      // bidi-override embedding for the inline element
```

For vertical scripts, set `writing-mode` on the block container — see
[Inline and Text Flow](../layout/inline.md#direction-and-writing-modes) for
the values.

## Text decoration and selection

| property | values |
|---|---|
| `text-decoration` | `none`, `underline`, `overline`, `line-through` |
| `user-select` | `auto`, `text`, `none`, `all` |
| `vertical-align` | `baseline`, `top`, `middle`, `bottom`, `sub`, `super`, `text-top`, `text-bottom`, `<percentage>`, `<length>` |

`user-select: none` is what you want on buttons and icon glyphs to prevent
double-click text selection from overlapping the click.

## Recipes

### Heading + body

```azul-render screenshot=text-heading width=480 height=200 subtitle="A heading and paragraph using system fonts"
<body style="font-family: system:ui, sans-serif; padding: 16px;">
  <h1 style="font-size: 24px; font-weight: bold; margin: 0 0 8px 0;">Heading</h1>
  <p style="font-size: 14px; line-height: 150%; margin: 0;">
    Body text rendered at 14px with 150% line-height. Long enough to wrap on
    a 480px-wide line.
  </p>
</body>
```

### Justified paragraph

```azul-render screenshot=text-justify width=400 height=200 subtitle="text-align: justify with default text-justify: auto"
<body style="font-family: serif; padding: 16px;">
  <p style="text-align: justify; line-height: 140%; font-size: 14px;">
    Justified text spreads each line to fill the box. The last line keeps its
    natural alignment because text-align-last defaults to auto.
  </p>
</body>
```

### Mixed weights

```azul-render screenshot=text-weights width=400 height=200 subtitle="lighter / normal / bold weights side by side"
<body style="font-family: system:ui; padding: 16px; font-size: 16px;">
  <p style="font-weight: 300;">Light weight (300)</p>
  <p style="font-weight: normal;">Normal weight (400)</p>
  <p style="font-weight: 600;">Semi-bold (600)</p>
  <p style="font-weight: bold;">Bold (700)</p>
</body>
```

## Font metrics

`FontMetrics` (`css/src/props/basic/font.rs:655`) exposes the OpenType `head`,
`hhea`, and `OS/2` tables for an installed face. The layout solver reads these
to position baselines, line boxes, and the `vertical-align` keyword set
(`text-top`, `text-bottom`, `super`, `sub`).

App code rarely touches `FontMetrics` directly — the metrics drive the
positioning of glyphs you've already styled with `font-size`,
`vertical-align`, and `line-height`. They're documented here because the
layout solver's behaviour around `vertical-align: middle` and the
`leading-trim` half-leading rule depends on the OS/2 v2+ `sxHeight` and
`sCapHeight` fields, which not every font ships.

## Default values at a glance

| property | default |
|---|---|
| `font-family` | `serif` (platform default) |
| `font-size` | `12pt` |
| `font-weight` | `normal` (400) |
| `font-style` | `normal` |
| `line-height` | `120%` |
| `letter-spacing` | `0px` |
| `word-spacing` | `0px` |
| `tab-size` | `8em` |
| `text-align` | `start` |
| `text-justify` | `auto` |
| `white-space` | `normal` |
| `word-break` | `normal` |
| `overflow-wrap` | `normal` |
| `hyphens` | `manual` |
| `direction` | `ltr` |
| `text-decoration` | `none` |
| `user-select` | `auto` |
| `vertical-align` | `baseline` |
