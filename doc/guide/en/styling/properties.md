---
slug: styling/properties
title: CSS Properties
language: en
canonical_slug: styling/properties
audience: external
maturity: mature
guide_order: 41
topic_only: false
short_desc: Reference of every CSS property azul recognises
prerequisites: [styling]
tracked_files:
  - css/src/props/basic/angle.rs
  - css/src/props/basic/color.rs
  - css/src/props/basic/direction.rs
  - css/src/props/basic/font.rs
  - css/src/props/basic/length.rs
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
  - css/src/props/style/scrollbar.rs
  - css/src/props/style/selection.rs
  - css/src/props/style/text.rs
  - css/src/props/style/transform.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# CSS Properties
The properties azul recognises, grouped by what they style. Each entry lists
the syntax and the Rust type the parser produces.

## Value primitives

- Length (px/em/rem/...) accepts `12px`, `1.5em`, or `100%`. Type `PixelValue`.
- Length (no percent) accepts `4px` or `2em`. Type `PixelValueNoPercent`.
- Percentage accepts `50%`. Type `PercentageValue`.
- Angle accepts `90deg`, `0.5turn`, `1.57rad`, or `100grad`. Type `AngleValue`.
- Time accepts `200ms` or `0.3s`. Type `CssDuration`.
- Color accepts `#fff`, `#1a2b3c`, `rgb(...)`, `rgba(...)`, `hsl(...)`, `hsla(...)`, named colors, and `currentcolor`. Type `ColorU`.
- System color accepts `system:accent`, `system:text`, `system:background`, and so on. Type `ColorOrSystem`.
- System font accepts `system:ui`, `system:monospace`, `system:title`, and so on. Type `StyleFontFamily::SystemType`.
- Float accepts `1.5` or `0`. Type `FloatValue`.

Length units: `px`, `em`, `rem`, `pt`, `pc`, `cm`, `mm`, `in`, `vw`, `vh`,
`vmin`, `vmax`, `%`, `ex`, `ch`. Angle units: `deg`, `rad`, `grad`,
`turn`, `%`.

Bare `0` is accepted for any length without a unit. Bare numbers without
a unit are interpreted as degrees for angle properties.

## CSS-wide keywords

Every property accepts `auto`, `none`, `initial`, `inherit`, `unset`, and
`revert` in addition to its typed value (`CssPropertyValue<T>`). Some
properties only accept a subset (`border-style` ignores `auto`,
`font-family` rejects `initial`, etc.) but the parser is permissive and
the cascade handles the rest.

## Box model

- `width`, `height`, `min-width`, `min-height`, `max-width`, `max-height` accept a length, `auto`, `min-content`, `max-content`, or `fit-content`. Types `LayoutWidth`, `LayoutHeight`, and so on.
- `padding`, `padding-top`, `padding-right`, `padding-bottom`, `padding-left` accept a length. Type `LayoutPadding*`.
- `margin`, `margin-top`, `margin-right`, `margin-bottom`, `margin-left` accept a length or `auto`. Type `LayoutMargin*`.
- `box-sizing` accepts `content-box` or `border-box`. Type `LayoutBoxSizing`.
- `display` accepts `block`, `inline`, `inline-block`, `flex`, `inline-flex`, `grid`, `inline-grid`, `none`, or `inherit`. Type `LayoutDisplay`.
- `position` accepts `static`, `relative`, `absolute`, `fixed`, or `sticky`. Type `LayoutPosition`.
- `top`, `right`, `bottom`, `left` accept a length or `auto`. Types `LayoutTop` and so on.
- `overflow`, `overflow-x`, `overflow-y` accept `visible`, `hidden`, `scroll`, `auto`, or `clip`. Type `LayoutOverflow`.
- `z-index` accepts an integer. Type `LayoutZIndex`.

`width: auto` and `height: auto` defer to the layout algorithm.
`min-content`, `max-content`, and `fit-content` derive from intrinsic
sizing.

## Background

`background` is a shorthand for `background-color` and `background-image`.
Multiple comma-separated layers stack from front to back.

```css
background: linear-gradient(to right, #1976d2, #42a5f5);
background: radial-gradient(circle at 30% 30%, #fff, #999);
background: conic-gradient(from 0deg, red, yellow, green, blue, red);
background: url('chrome.png'), #f0f0f0;
```

- `background` (shorthand) accepts a color, gradient, image, or multiple layers. Type `StyleBackgroundContentVec`.
- `background-color` accepts a color. Type `ColorU`.
- `background-image` accepts `linear-gradient(...)`, `radial-gradient(...)`, `conic-gradient(...)`, `url(...)`, or `none`. Type `StyleBackgroundContent`.
- `background-position` accepts a length pair or `top`/`bottom`/`left`/`right`/`center`. Type `StyleBackgroundPosition`.
- `background-size` accepts a length pair, `cover`, `contain`, or `auto`. Type `StyleBackgroundSize`.
- `background-repeat` accepts `repeat`, `repeat-x`, `repeat-y`, `no-repeat`, `round`, or `space`. Type `StyleBackgroundRepeat`.

Gradient direction accepts angles (`90deg`), `to <side>` syntax
(`to top right`), or corner directions.

```azul-render screenshot=styling-gradients width=480 height=200 subtitle="Linear, radial, and conic backgrounds"
<html>
<head><style>
body { font-family: sans-serif; padding: 16px; background: white; display: flex; gap: 12px; }
.cell { width: 140px; height: 140px; border-radius: 8px; }
.lin { background: linear-gradient(135deg, #1976d2, #42a5f5); }
.rad { background: radial-gradient(circle at 30% 30%, #fff, #777); }
.con { background: conic-gradient(from 0deg, red, yellow, green, blue, red); }
</style></head>
<body><div class="cell lin"></div><div class="cell rad"></div><div class="cell con"></div></body>
</html>
```

## Border

- `border` (shorthand) accepts width, style, and color. Sets all four sides.
- `border-width`, `border-{top,right,bottom,left}-width` accept `thin`, `medium`, `thick`, or a length. Type `StyleBorder*Width`.
- `border-style`, `border-{top,right,bottom,left}-style` accept `none`, `solid`, `double`, `dotted`, `dashed`, `hidden`, `groove`, `ridge`, `inset`, or `outset`. Type `BorderStyle`.
- `border-color`, `border-{top,right,bottom,left}-color` accept a color. Type `StyleBorder*Color`.
- `border-radius`, `border-{top-left,top-right,bottom-left,bottom-right}-radius` accept a length. Type `StyleBorder*Radius`.

`border` shorthand expands to all four sides. `border-top` etc. expand to
the three properties of one side.

## Box shadow and filter

- `box-shadow` accepts `<x> <y> [blur] [spread] [color] [inset]`. Type `StyleBoxShadow`.
- `text-shadow` accepts `<x> <y> [blur] [color]`. Type `StyleBoxShadow`.
- `filter` accepts `blur(<len>)`, `brightness(<%>)`, `contrast(<%>)`, `grayscale(<%>)`, `hue-rotate(<angle>)`, `invert(<%>)`, `opacity(<%>)`, `saturate(<%>)`, `sepia(<%>)`, and `drop-shadow(...)`. Type `StyleFilterVec`.
- `backdrop-filter` accepts the same values as `filter`. Type `StyleFilterVec`.
- `mix-blend-mode` accepts `normal`, `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`, or `luminosity`. Type `StyleMixBlendMode`.

`box-shadow` accepts multiple comma-separated shadows. The first one is on
top.

## Text

```css
font-family: 'Inter', sans-serif;
font-size: 16px;
font-weight: 600;
line-height: 1.5;
color: #222;
text-align: justify;
letter-spacing: 0.02em;
text-decoration: underline;
```

- `color` accepts a color. Type `StyleTextColor`.
- `font-family` accepts a comma-separated list of names, a generic family (`sans-serif`, `serif`, `monospace`, `cursive`, `fantasy`), or `system:ui`. Type `StyleFontFamilyVec`.
- `font-size` accepts a length or percentage. Type `StyleFontSize`.
- `font-weight` accepts `100`-`900`, `normal`, `bold`, `bolder`, or `lighter`. Type `StyleFontWeight`.
- `font-style` accepts `normal`, `italic`, or `oblique`. Type `StyleFontStyle`.
- `line-height` accepts a unitless number, length, or percentage. Type `StyleLineHeight`.
- `letter-spacing` accepts a length. Type `StyleLetterSpacing`.
- `word-spacing` accepts a length. Type `StyleWordSpacing`.
- `tab-size` accepts an integer or length. Type `StyleTabSize`.
- `text-align` accepts `left`, `right`, `center`, `justify`, `start`, or `end`. Type `StyleTextAlign`.
- `text-decoration` accepts `none`, `underline`, `overline`, `line-through`, or combinations. Type `StyleTextDecoration`.
- `text-transform` accepts `none`, `uppercase`, `lowercase`, or `capitalize`. Parses, type not exposed.
- `text-orientation` accepts `mixed`, `upright`, or `sideways`. Type `StyleTextOrientation`.
- `direction` accepts `ltr` or `rtl`. Type `StyleDirection`.
- `white-space` accepts `normal`, `nowrap`, `pre`, `pre-wrap`, or `pre-line`. Type `StyleWhiteSpace`.
- `word-break` accepts `normal`, `break-all`, or `keep-all`. Type `StyleWordBreak`.
- `overflow-wrap` accepts `normal`, `break-word`, or `anywhere`. Type `StyleOverflowWrap`.
- `hyphens` accepts `none`, `manual`, or `auto`. Type `StyleHyphens`.
- `hyphenation-language` (Azul) accepts a BCP 47 tag. Type `StyleHyphenationLanguage`.

System font keywords resolve to platform defaults (`SF Pro`, `Segoe UI`,
`Cantarell`, ...) through `SystemFontType`. See
[System Themes](themes.md).

## Transforms and 3D

- `transform` accepts `translate(<len>, <len>)`, `translateX/Y/Z`, `scale(<n>, <n>)`, `scaleX/Y/Z`, `rotate(<angle>)`, `rotateX/Y/Z`, `skew(<angle>, <angle>)`, `matrix(...)`, `matrix3d(...)`, and `perspective(<len>)`. Type `StyleTransformVec`.
- `transform-origin` accepts a length pair. Type `StyleTransformOrigin`. Default `50% 50%`.
- `transform-style` accepts `flat` or `preserve-3d`. Parses, type not exposed.
- `perspective` accepts a length or `none`. Parses, type not exposed.
- `perspective-origin` accepts a length pair. Type `StylePerspectiveOrigin`.
- `backface-visibility` accepts `visible` or `hidden`. Type `StyleBackfaceVisibility`.

Transforms compose left to right. Multiple functions in one declaration
chain. `transform: translate(10px, 0) rotate(45deg) scale(1.2)` first
translates, then rotates around the origin, then scales.

## Flexbox and grid

The flexbox and grid syntax is standard CSS. The full reference will live
in [Layout](../layout.md). Quick lookup:

- `flex` (shorthand) accepts `<grow> <shrink> <basis>`. Shorthand for the four flex properties.
- `flex-direction` accepts `row`, `row-reverse`, `column`, or `column-reverse`. Type `LayoutFlexDirection`.
- `flex-wrap` accepts `nowrap`, `wrap`, or `wrap-reverse`. Type `LayoutFlexWrap`.
- `flex-grow`, `flex-shrink` accept a float. Types `LayoutFlexGrow` and `LayoutFlexShrink`.
- `flex-basis` accepts a length, `auto`, or `content`. Type `LayoutFlexBasis`.
- `align-items`, `align-self`, `align-content` accept `start`, `end`, `center`, `stretch`, `baseline`, `flex-start`, `flex-end`, `space-between`, `space-around`, or `space-evenly`. Types `LayoutAlignItems`, `LayoutAlignSelf`, `LayoutAlignContent`.
- `justify-content`, `justify-items`, `justify-self` accept the same values as align. Types `LayoutJustifyContent`, `LayoutJustifyItems`, `LayoutJustifySelf`.
- `gap`, `row-gap`, `column-gap` accept a length pair. Type `LayoutGap`.
- `grid-template-rows`, `grid-template-columns` accept a track list with `fr`, `auto`, or length. Grid track types.
- `grid-row`, `grid-column` accept `<start> / <end>`. Grid placement types.
- `grid-area` accepts a shorthand or named area. Grid placement types.

`fr` units use a 100x internal multiplier. See the layout guide for
intrinsic-sizing rules.

## Effects

- `opacity` accepts `0.0`-`1.0` or a percentage. Type `StyleOpacity`.
- `visibility` accepts `visible`, `hidden`, or `collapse`. Type `StyleVisibility`.
- `cursor` accepts `default`, `pointer`, `text`, `move`, `crosshair`, `wait`, `help`, `grab`, `grabbing`, `not-allowed`, `progress`, `cell`, `col-resize`, `row-resize`, `e-resize`, `n-resize`, `s-resize`, `w-resize`, `se-resize`, and so on. Type `StyleCursor`.
- `object-fit` accepts `fill`, `contain`, `cover`, `none`, or `scale-down`. Type `StyleObjectFit`.
- `object-position` accepts a length pair. Type `StyleObjectPosition`.
- `aspect-ratio` accepts `<n>` or `<n> / <m>`. Type `StyleAspectRatio`.
- `pointer-events` accepts `auto` or `none`. Parses, type not exposed.

`StyleCursor` covers every common system cursor on each platform.

## Lists

- `list-style-type` accepts `none`, `disc`, `circle`, `square`, `decimal`, `decimal-leading-zero`, `lower-roman`, `upper-roman`, `lower-alpha`, `upper-alpha`, `lower-greek`, or `upper-greek`. Type `StyleListStyleType`.
- `list-style-position` accepts `inside` or `outside`. Type `StyleListStylePosition`.

## Scrolling

- `scroll-behavior` accepts `auto` or `smooth`. Parses, type not exposed.
- `overscroll-behavior`, `overscroll-behavior-x`, `overscroll-behavior-y` accept `auto`, `contain`, or `none`. Parses, type not exposed.
- `scrollbar-width` accepts `auto`, `thin`, `none`, or a length. Type `LayoutScrollbarWidth`.
- `scrollbar-color` accepts `<thumb> <track>`. Type `StyleScrollbarColor`.
- `scrollbar-track-color` accepts a color. Parses, type not exposed.
- `scrollbar-thumb-color` accepts a color. Parses, type not exposed.

## Selection appearance (Azul)

- `selection-background-color` accepts a color. Type `SelectionBackgroundColor`.
- `selection-color` accepts a color. Type `SelectionColor`.
- `selection-radius` accepts a length. Type `SelectionRadius`.

These accept the OS-native default automatically (see
[System Themes](themes.md)).

## Generated content

- `content` accepts a string, `none`, `normal`, or a keyword. Type `Content`.
- `counter-reset` accepts a name list. Type `CounterReset`.
- `counter-increment` accepts a name list. Type `CounterIncrement`.
- `string-set` accepts a name plus content. Type `StringSet`.

The runtime support for `::before` and `::after` generated content is
in-progress. The parser accepts the syntax today but the layout engine
doesn't yet emit nodes from it.

## Animation and transitions

The animation runtime isn't yet wired up. See the
[Animations](../animations.md) page. These properties parse and
round-trip but their typed views are still internal:

- `transition` shorthand: property duration timing-function delay.
- `transition-property`: property name list, `all`, `none`.
- `transition-duration`, `transition-delay`: time values.
- `transition-timing-function`: `ease`, `linear`, `ease-in`, `ease-out`,
  `ease-in-out`, `cubic-bezier(...)`.
- `animation` shorthand: name, duration, timing-function, delay,
  iteration-count, direction, fill-mode, play-state.
- The individual `animation-*` longhands: each accepts the matching value
  list.

## Azul-specific properties

- `exclusion-margin`, `exclusion-margin-top`, `exclusion-margin-bottom`, `exclusion-margin-left`, `exclusion-margin-right` accept a length. Type `StyleExclusionMargin`.
- `hyphenation-language` accepts a BCP 47 tag. Type `StyleHyphenationLanguage`.
- `shape-inside`, `shape-outside`, `clip-path` accept `inset(...)`, `circle(...)`, `ellipse(...)`, `polygon(...)`, `path(...)`, `url(...)`, or `none`. Parses, type not exposed.

`exclusion-margin` is azul's CSS Exclusions mode. It lets text flow around
floated or positioned elements with a configurable inset.

## Coming Up Next

- [System Themes](themes.md) — System colors, `@theme`, `@os`, and accessibility queries
- [Styling Text](text-and-fonts.md) — Font family, size, weight, alignment, decoration, and the system font keywords
- [Layout](../layout.md) — Overview of the layout solver
