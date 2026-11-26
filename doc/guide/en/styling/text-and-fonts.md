---
slug: styling/text-and-fonts
title: Styling Text
language: en
canonical_slug: styling/text-and-fonts
audience: external
maturity: wip
guide_order: 73
topic_only: false
short_desc: Font family, size, weight, alignment, decoration, and the system font keywords
prerequisites: [styling]
tracked_files:
  - css/src/props/basic/font.rs
  - css/src/props/layout/text.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:49:28Z
default-search-keys:
  - FontRef
  - StyleFontFamily
  - StyleFontSize
  - StyleFontWeight
  - StyleFontStyle
  - PixelValue
  - LayoutTextJustify
  - Direction
---

# Styling Text

## Overview

Font fallback, the text shaper and variable fonts are stable: the axes of a variable font are driven from CSS (see "Variable fonts" below), and so are the OpenType features of the shaper (see "OpenType features"). The properties below all parse and apply; "What is not drawn yet" at the end of this page lists the few text properties the engine does not implement.

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

`StyleFontWeight` is a number from 1 to 1000, or a relative keyword:

- `Number(weight)`: any number from 1 to 1000. `normal` is the number 400 and `bold` is 700; `350`
  and `650.5` are weights like any other, and a number outside 1 to 1000 is invalid.
- `Lighter` and `Bolder` are worked out against the parent's weight (CSS Fonts 4, section 2.2);
  the descendants inherit the number.

A variable font with a `wght` axis is drawn at exactly the number; a family of separate faces
picks the face nearest to it.

## font-style

`StyleFontStyle`:

```rust,ignore
StyleFontStyle::Normal               // upright (default)
StyleFontStyle::Italic               // italic face
StyleFontStyle::Oblique(angle)       // slanted by `angle` degrees, -90 to 90 (`oblique 12deg`)
```

A bare `oblique` is `oblique 14deg`.

## Variable fonts

A variable font is drawn at the exact coordinates the CSS asks of its axes. For each axis the face
has:

| axis | set by | value |
|------|--------|-------|
| `wght` | `font-weight` | the weight, any number from 1 to 1000 |
| `wdth` | `font-stretch` (alias `font-width`) | the stretch in percent |
| `slnt` | `font-style: oblique <angle>` | minus the angle (CSS leans to the right, `slnt` counts counter-clockwise); 0 for `normal` and `italic` |
| `ital` | `font-style` | 1 for `italic`, 0 for `normal` and `oblique` |
| `opsz` | `font-optical-sizing: auto` (the initial value) | the used font size in CSS px, rounded to a whole number; `none` leaves the axis at its default |
| any tag | `font-variation-settings` | the value written, applied last: it wins over every property above |

Every value is clamped to the range of the axis, `font-variation-settings` tags the face does not
have are ignored, and a tag written twice keeps its last value. A face without an axis ignores the
part of the request that is for it, and a family of separate static faces is chosen by the nearest
weight bucket (`font-stretch` is not used to choose among static faces).

The face a text is shaped, drawn and embedded in a PDF with is never the variable font itself but
a STATIC instance of it, made once for the coordinates (rounded to 1/64) and shared by every
window, every paragraph and every spelling of the family name that asks for them. The cache keeps
at most 32 instances per font file (and 128 MiB of them); a request beyond that is drawn with the
nearest instance made. The system font of macOS is such a face: its `opsz` follows the font size
and its `wght` the weight, as in Chrome and CoreText.

## OpenType features

The shaper turns the OpenType features of a font on and off from CSS. Each row of the table is a
property and the features it asks for; a font without a feature draws the text as it would
without the property.

| property | value | features |
|----------|-------|----------|
| `font-kerning` | `none` | turns `kern` (the pair adjustments of the font) off; `normal` and `auto` leave it on |
| `font-variant-ligatures` | `common-ligatures` / `no-common-ligatures` | `liga` and `clig` on / off |
| | `discretionary-ligatures` / `no-discretionary-ligatures` | `dlig` on / off |
| | `historical-ligatures` / `no-historical-ligatures` | `hlig` on / off |
| | `contextual` / `no-contextual` | `calt` on / off |
| | `none` | all four off (the ligatures a script needs, `rlig`, stay) |
| `font-variant-caps` | `small-caps`, `all-small-caps`, `petite-caps`, `all-petite-caps`, `unicase`, `titling-caps` | `smcp`, `smcp` + `c2sc`, `pcap`, `pcap` + `c2pc`, `unic`, `titl` |
| `font-variant-numeric` | `lining-nums`, `oldstyle-nums`, `proportional-nums`, `tabular-nums`, `diagonal-fractions`, `stacked-fractions`, `ordinal`, `slashed-zero` | `lnum`, `onum`, `pnum`, `tnum`, `frac`, `afrc`, `ordn`, `zero` |
| `font-variant-east-asian` | `jis78`, `jis83`, `jis90`, `jis04`, `simplified`, `traditional` | `jp78`, `jp83`, `jp90`, `jp04`, `smpl`, `trad` |
| | `full-width`, `proportional-width` | `fwid`, `pwid` |
| | `ruby` | `ruby` |
| `font-variant-position` | `sub`, `super` | `subs`, `sups` |
| `text-rendering` | `optimizeSpeed` | turns `kern` and the ligatures (`liga`, `clig`, `dlig`, `hlig`, `calt`) off where `font-kerning` and `font-variant-ligatures` say nothing; `auto`, `optimizeLegibility` and `geometricPrecision` leave the shaper as it is |
| `font-feature-settings` | `"tag"`, `"tag" 1`, `"tag" on` / `"tag" 0`, `"tag" off` / `"tag" N` | the feature on / off / its Nth alternate, counted from 1 |

The order is the one of CSS Fonts 4: the features a script turns on by itself, the properties above
(a group of ligatures that is turned off takes its features out of the script's own; so does a
`letter-spacing` that is not zero for `liga` and `clig`, unless `common-ligatures` asks for them),
and last `font-feature-settings`, which has the last word. `font-feature-settings: "kern" 0`
turns the kerning off and `"kern" 1` on again after `font-kerning: none`, and the pair
adjustments of a font are applied once.

`font-variant: sub` and `font-variant: super` are accepted and set nothing: use `font-variant-position`.

A `font-feature-settings` value keeps its first 64 distinct tags (`MAX_FONT_FEATURES`; a font has a
few dozen features at most) and the stylesheet parser warns about more.

## What the engine fakes

`font-synthesis` says what the engine may fake where a face or a feature is missing. Its
longhands take `auto` (fake it, the initial value) or `none`; the shorthand is `none` or any of
`weight`, `style`, `small-caps`, `position` (the ones not listed are `none`). Inherited.

| longhand | what it switches |
|----------|------------------|
| `font-synthesis-weight` | the thickened bold of a text that asks for a weight of 600 or more from a face that has no bold cut: in both rasterizers and in the PDF writer (the run is filled and stroked) |
| `font-synthesis-small-caps` | the small capitals of `font-variant-caps` that the shaper builds from scaled capitals (at 0.7 of the size) where the font has no `smcp`: `none` sets the text as it is |
| `font-synthesis-style` | the slant of an italic that a family has no face for. Parsed, inherited and carried by the style of the run; **no consumer yet**: the engine draws no synthetic italic or oblique (see "What is not drawn yet") |
| `font-synthesis-position` | the smaller, raised glyphs of a super- or subscript that a font has no `sups` or `subs` for. Parsed, inherited and carried by the style of the run; **no consumer yet**: a `font-variant-position` that the font has no feature for is drawn plain |

## Vertical text

`writing-mode: vertical-rl | vertical-lr` is read for the text of every node: the glyph runs the
PDF writer is given say the writing mode of their text. `text-combine-upright: all` and `digits N`
set the characters of a run side by side in one em of the vertical line (tate-chu-yoko) - in the
display list and in the PDF. In horizontal text the property has no effect.

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
- `letter-spacing` accepts a `<length>` or `normal`. Default `0px`. Added after every glyph. `normal` is `0` and ends the spacing a parent set.
- `word-spacing` accepts a `<length>` or `normal`. Default `0px`. Added after every space. `normal` is `0` and ends the spacing a parent set.
- `tab-size` accepts a `<number>` or a `<length>`. Default `8`. A number is the distance between the tab stops in widths of a space (`tab-size: 4`), a length is that distance in px (`tab-size: 30px`); `0` draws no tab. The stops lie at multiples of the distance from the start of the line, so a tab advances to the next stop wherever on the line it stands. The width of a space counts the `letter-spacing` and `word-spacing` of the text.
- `text-indent` accepts a `<length>` or `<percentage>`, with `hanging` (indent every line but the first) and `each-line` (indent the line after a forced break too).
- `text-align-last` accepts `auto`, `start`, `end`, `left`, `right`, `center`, or `justify` and aligns the last line of a block (and the lines before a forced break) the way `text-align` does the others; `auto` follows `text-align`, except that `justify` leaves the last line at `start`.

```css
.body { line-height: 150%; letter-spacing: 0.02em; }
pre   { tab-size: 4; }
```

## Wrapping and breaks

`white-space` controls collapsing and wrapping:

- `normal` (default). Collapses whitespace and wraps.
- `pre`. Preserves whitespace. No wrap (only at explicit breaks).
- `nowrap`. Collapses whitespace. No wrap.
- `pre-wrap`. Preserves whitespace and wraps.
- `pre-line`. Collapses whitespace but preserves newlines. Wraps.
- `break-spaces`. Preserves whitespace and breaks at every space. Wraps.

`text-wrap` decides how the lines of a paragraph are broken (inherited):

- `wrap` (default) and `stable`. Each line takes all the words that fit.
- `nowrap`. The text stays on one line. A `white-space` that does not wrap (`nowrap`, `pre`) wins over every other value.
- `balance`. The paragraph is broken as a whole so that its lines are as even in length as the words allow; the last line counts as much as the others, so it is no single word where the words allow better. Up to ten lines and 3,000 shaped items: a longer text is broken as `wrap` is.
- `pretty`. The paragraph is broken as a whole to avoid a ragged edge (the same breaker as `balance`; the last line is as long as it falls). Up to 3,000 shaped items.

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
The layout crate's `text_layout_hyphenation` feature embeds the English (US)
patterns only (91 KB); `text_layout_hyphenation_all` embeds all 82 languages
(2.9 MB). Text in a language the build has no patterns for is not hyphenated.

## Direction and writing mode

For bidirectional text:

```rust,ignore
StyleDirection::Ltr   // left-to-right (default)
StyleDirection::Rtl   // right-to-left

StyleUnicodeBidi      // normal, embed, isolate, bidi-override, isolate-override, plaintext
```

`direction` is the base direction of the block: a right-to-left block starts its lines at the
right edge (`text-align: start` follows it, `left` and `right` do not), and the letters of a Latin
word keep their order in it. Arabic and Hebrew words run from the right in a block of either
direction. On the block that holds the text, `unicode-bidi: bidi-override` and `isolate-override`
make every letter run in the direction of the block whatever its own (reversed in a right-to-left
block, in the order they are written in a left-to-right one) and `plaintext` takes the base
direction from the first strong letter of the text. `embed` and `isolate` open no level of their
own in a block.

For vertical scripts, set `writing-mode` on the block container. See
[Inline and Text Flow](../layout/inline.md#direction-and-writing-modes)
for the values.

## Text decoration and selection

- `text-decoration` accepts `none`, `underline`, `overline`, or `line-through`.
- `text-decoration-color` accepts `currentcolor` (the initial value) or a colour, a `system:` colour included. The line is drawn in the colour of the box that sets it (its `color`, which a line follows when only the colour changes), whatever the colour of the text it crosses: the underline of a red block is red under the blue span in it.
- `text-decoration-thickness` accepts `auto` (the initial value: 8 percent of the font size, at least 1 px), `from-font`, a length, or a percentage (of one em). An `em` and a percentage are parts of the font size of the box that sets the line, not of the text under it. A thickness of 0 draws no line, a negative one is no value, and one beyond 1,000 px is read as 1,000 px with a diagnostic. `from-font` reads no metric of the face yet, so it is `auto`.
- `text-underline-offset` accepts `auto` (the initial value), a length, or a percentage (of one em): it moves the underline down from the place of the font size, up for a negative length. An `em` and a percentage are parts of the font size of the box that sets the line, a zoom scales a length, and one beyond 1,000 px either way is read as 1,000 px with a diagnostic. It moves no overline and no line-through.
- `text-underline-position` accepts `auto` (the initial value), `from-font` or `under`, and `left` or `right`, each group at most once and in either order. `under` puts the top of the underline where the em box ends, below the descenders. `from-font` reads no metric of the face yet, so it is `auto`; `left` and `right` choose the side of vertical text, which the engine does not draw lines for yet (a horizontal line has no side).
- Both are inherited (a box inside one that sets them is placed by them, `auto` ends them); the colour and the thickness are not, and each kind of line has the paint of the box that sets it: an underline of a block and a line-through of a span in it are two lines of two paints under the text of the span. A thicker underline or overline grows downwards from the place the font puts it, a thicker line-through around its middle.
- `text-shadow` accepts `<x> <y> [blur] [color]`. The text is painted between a push and a pop of the shadow in the display list, so both rasterizers draw the shadow under the glyphs.
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
- `tab-size`. Default `8`.
- `text-align`. Default `start`.
- `text-justify`. Default `auto`.
- `white-space`. Default `normal`.
- `word-break`. Default `normal`.
- `overflow-wrap`. Default `normal`.
- `hyphens`. Default `manual`.
- `direction`. Default `ltr`.
- `text-decoration`. Default `none`.
- `text-decoration-color`. Default `currentcolor`.
- `text-decoration-thickness`. Default `auto`.
- `text-underline-offset`. Default `auto`.
- `text-underline-position`. Default `auto`.
- `user-select`. Default `auto`.
- `vertical-align`. Default `baseline`.

## What is not drawn yet

The text properties below are parsed or are not properties at all yet; the engine does not draw
what they say. Each row says what the layout does today.

| property | today |
|----------|-------|
| `font-synthesis-style`, `font-synthesis-position` | parsed and inherited, with no consumer: a `font-style: italic` text in a family without an italic face is drawn upright (the engine fakes no slant, in neither rasterizer nor in a PDF), and a super- or subscript is not faked either |
| `text-orientation: upright`, `sideways` | parsed; in a vertical block `upright` sets the line direction to left-to-right as the spec says, and no value turns a glyph: the text is `mixed` |
| `hanging-punctuation` | applied by the greedy line breaker and the positioning of every line; not by the breaker of `text-wrap: balance` and `pretty` (which count the mark in the line) and not by the intrinsic sizes (a box sized to its content holds the mark); the marks are the brackets and quotes and the stops and commas of Latin, CJK and fullwidth text |
| `initial-letter` | makes room: the lines beside the first letter start after a box N lines high, and the letter is drawn at the size of the text, not bigger and not moved into the room |
| `text-decoration-style`, `text-emphasis` | not properties: the lines of `text-decoration` are solid |
| `text-underline-position: left`, `right`, the lines of a vertical writing mode | a line is a horizontal rectangle from the first glyph of a run to the end of its extent along x, so a line of vertical text is not drawn along the run, on the side `left` or `right` says |
| `font-size-adjust` | not a property: the font size never follows the x-height of the face |
| `hyphenate-character`, `font-variant-alternates` | not properties (the stylesheet parser reports them as unknown): the hyphen drawn at the end of a line is a `-` of the font of the text, and the alternates of a font are reached by `font-feature-settings` |
