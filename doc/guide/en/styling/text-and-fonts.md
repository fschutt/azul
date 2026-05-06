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

# Styling Text

> **WIP.** Font fallback and the text shaper are stable; some advanced
> typography (`font-feature-settings`, variable axes) is not yet wired
> through from CSS to the layout engine. The properties below all parse
> and apply.

Text rendering is driven by four font properties (`font-family`,
`font-size`, `font-weight`, `font-style`) and modulated by alignment,
justification, and line metrics.

## font-family

`StyleFontFamily` is one of:

- `System(name)`. CSS like `"Arial"` or `Times New Roman`. Resolves to a face matching the family name.
- `SystemType(SystemFontType)`. CSS like `system:ui` or `system:monospace:bold`. Resolves to a platform UI font.
- `File(url)`. CSS like `url(/fonts/Inter.ttf)`. Resolves to a font file loaded from the URL.
- `Ref(FontRef)`. Not addressable from CSS. Resolves to a pre-loaded font handle.

The property takes a comma-separated *fallback list*. Each entry is tried
in order; the first one that resolves to a face containing the requested
glyph wins.

```css
body { font-family: "Inter", system:ui, sans-serif; }
code { font-family: system:monospace, "SF Mono", Menlo, monospace; }
```

### system: fonts

The `system:<role>[:<variant>]` prefix selects a platform UI font without
hard-coding a family name. It resolves to the OS's preferred face for
that role:

- `system:ui`. The default UI font.
- `system:ui:bold`. The UI bold variant.
- `system:monospace`, `system:monospace:bold`, `system:monospace:italic`. Platform monospace.
- `system:title`, `system:title:bold`. Larger UI text.
- `system:menu`. Menu and menu-item label font.
- `system:small`. Small-print UI font.
- `system:serif`, `system:serif:bold`. Platform serif.

If the role isn't recognised, the parser keeps the literal string as a
`System` family (so `system:invalid` becomes the family named
`"system:invalid"`).

### FontRef and pre-loaded fonts

`FontRef` is a reference-counted handle that points at parsed font data.
You won't construct `FontRef` from CSS, but you'll see it on the Rust side
when binding a font once and using it across multiple DOMs.

## font-size

`StyleFontSize` wraps a `PixelValue`. The default is `12pt`. Pick whatever
your design system needs:

```css
h1 { font-size: 28px; }
p  { font-size: 1em; }
small { font-size: 80%; }
```

`em` is relative to the parent's `font-size`. `rem` is relative to the
root. `%` resolves the same way as `em`.

## font-weight

`StyleFontWeight`:

- `lighter`. Lighter than parent.
- `100` ... `300`. Maps to `W100` ... `W300`.
- `normal`, `400`. Maps to `Normal`.
- `500`, `600`. Maps to `W500`, `W600`.
- `bold`, `700`. Maps to `Bold`.
- `800`, `900`. Maps to `W800`, `W900`.
- `bolder`. Heavier than parent.

The numeric scale is the OpenType weight class. The parser maps standard
numbers to enum variants. `450` and other in-between numbers are *not*
accepted; use the named variant for the closest weight.

## font-style

`StyleFontStyle`:

```rust,ignore
StyleFontStyle::Normal   // upright (default)
StyleFontStyle::Italic   // italic face
StyleFontStyle::Oblique  // oblique (synthesised slant if no italic face)
```

## text-align

Horizontal alignment of inline content within its line box:

- `start` (default). Left in LTR, right in RTL.
- `end`. Right in LTR, left in RTL.
- `left` / `right`. Absolute, ignoring text direction.
- `center`. Centred.
- `justify`. Spread to fill the line. See `text-justify` below.

## text-justify

`LayoutTextJustify` refines what `text-align: justify` does:

- `auto` (default). UA picks the appropriate algorithm for the script.
- `none`. No justification (`text-align: justify` is treated as `start`).
- `inter-word`. Distributes whitespace only between words.
- `inter-character`. Distributes whitespace between every character (CJK-friendly).
- `distribute`. Legacy alias of `inter-character`.

The legacy `distribute` value computes to `inter-character` per the spec.

## Line metrics

- `line-height` accepts `<percentage>`. Default `120%`. Multiplier applied to `font-size`.
- `letter-spacing` accepts `<length>`. Default `0px`. Added between every glyph.
- `word-spacing` accepts `<length>`. Default `0px`. Added between words.
- `tab-size` accepts `<length>`. Default `8em`. Width of a tab character.

```css
.body { line-height: 150%; letter-spacing: 0.02em; }
pre   { tab-size: 4em; }
```

## Wrapping and breaks

`white-space` controls collapsing and wrapping:

- `normal` (default). Collapses whitespace and wraps.
- `pre`. Preserves whitespace. No wrap (only at explicit breaks).
- `nowrap`. Collapses whitespace. No wrap.
- `pre-wrap`. Preserves whitespace and wraps.
- `pre-line`. Collapses whitespace but preserves newlines. Wraps.
- `break-spaces`. Preserves whitespace and breaks at every space. Wraps.

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

`hyphens: none | manual | auto` enables soft-hyphen breaking. `auto`
requires the layout crate's hyphenation resources for the active language.

## Direction and writing mode

For bidirectional text:

```rust,ignore
StyleDirection::Ltr   // left-to-right (default)
StyleDirection::Rtl   // right-to-left

StyleUnicodeBidi      // bidi-override embedding for the inline element
```

For vertical scripts, set `writing-mode` on the block container. See
[Inline and Text Flow](../layout/inline.md#direction-and-writing-modes)
for the values.

## Text decoration and selection

- `text-decoration` accepts `none`, `underline`, `overline`, or `line-through`.
- `user-select` accepts `auto`, `text`, `none`, or `all`.
- `vertical-align` accepts `baseline`, `top`, `middle`, `bottom`, `sub`, `super`, `text-top`, `text-bottom`, `<percentage>`, or `<length>`.

`user-select: none` is what you want on buttons and icon glyphs to prevent
double-click text selection from overlapping the click.

## Recipes

### Heading and body

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

## Default values at a glance

- `font-family`. Default `serif` (platform default).
- `font-size`. Default `12pt`.
- `font-weight`. Default `normal` (400).
- `font-style`. Default `normal`.
- `line-height`. Default `120%`.
- `letter-spacing`. Default `0px`.
- `word-spacing`. Default `0px`.
- `tab-size`. Default `8em`.
- `text-align`. Default `start`.
- `text-justify`. Default `auto`.
- `white-space`. Default `normal`.
- `word-break`. Default `normal`.
- `overflow-wrap`. Default `normal`.
- `hyphens`. Default `manual`.
- `direction`. Default `ltr`.
- `text-decoration`. Default `none`.
- `user-select`. Default `auto`.
- `vertical-align`. Default `baseline`.

## Coming Up Next

- [Icon Packs](icon-packs.md) — Register icons and use them with `Dom::create_icon` or `<icon>`
- [Inline Layout](../layout/inline.md) — Text flow, word breaks, writing modes, multi-column
- [Text Input](../text-input.md) — Editable text, IME, and the selection model
