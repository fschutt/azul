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

# CSS Properties Cheatsheet

The properties azul recognises, grouped by what they style. Each row lists
the syntax and the Rust type the parser produces. The Rust types live under
`azul_css::props::style::*` and `azul_css::props::basic::*` — open the
referenced source file when you need the exact set of variants.

## Value primitives

| Kind | Syntax | Type |
|---|---|---|
| Length (px/em/rem/...) | `12px`, `1.5em`, `100%` | `PixelValue` |
| Length (no percent) | `4px`, `2em` | `PixelValueNoPercent` |
| Percentage | `50%` | `PercentageValue` |
| Angle | `90deg`, `0.5turn`, `1.57rad`, `100grad` | `AngleValue` |
| Time | `200ms`, `0.3s` | `CssDuration` |
| Color | `#fff`, `#1a2b3c`, `rgb(...)`, `rgba(...)`, `hsl(...)`, `hsla(...)`, named, `currentcolor` | `ColorU` |
| System color | `system:accent`, `system:text`, `system:background`, ... | `ColorOrSystem` |
| System font | `system:ui`, `system:monospace`, `system:title`, ... | `StyleFontFamily::System` |
| Float | `1.5`, `0` | `FloatValue` |

Length units: `px`, `em`, `rem`, `pt`, `pc`, `cm`, `mm`, `in`, `vw`, `vh`,
`vmin`, `vmax`, `%`, `ex`, `ch` (see `SizeMetric` at
`css/src/props/basic/length.rs:208`). Angle units: `deg`, `rad`, `grad`,
`turn`, `%` (`css/src/props/basic/angle.rs:18`).

Bare `0` is accepted for any length without a unit. Bare numbers without
a unit are interpreted as degrees for angle properties.

## CSS-wide keywords

Every property accepts `auto`, `none`, `initial`, `inherit`, `unset`, and
`revert` in addition to its typed value (`CssPropertyValue<T>` at
`css/src/css.rs:374`). Some properties only accept a subset — `border-style`
ignores `auto`, `font-family` rejects `initial`, etc. — but the parser is
permissive and the cascade handles the rest.

## Box model

| Property | Values | Type |
|---|---|---|
| `width`, `height`, `min-width`, `min-height`, `max-width`, `max-height` | length, `auto`, `min-content`, `max-content`, `fit-content` | `LayoutWidth`, `LayoutHeight`, ... |
| `padding`, `padding-top`, `padding-right`, `padding-bottom`, `padding-left` | length | `LayoutPadding*` |
| `margin`, `margin-top`, `margin-right`, `margin-bottom`, `margin-left` | length, `auto` | `LayoutMargin*` |
| `box-sizing` | `content-box`, `border-box` | `LayoutBoxSizing` |
| `display` | `block`, `inline`, `inline-block`, `flex`, `inline-flex`, `grid`, `inline-grid`, `none`, `inherit` | `LayoutDisplay` |
| `position` | `static`, `relative`, `absolute`, `fixed`, `sticky` | `LayoutPosition` |
| `top`, `right`, `bottom`, `left` | length, `auto` | `LayoutTop`, ... |
| `overflow`, `overflow-x`, `overflow-y` | `visible`, `hidden`, `scroll`, `auto`, `clip` | `LayoutOverflow` |
| `z-index` | integer | `LayoutZIndex` |

`width: auto` and `height: auto` defer to the layout algorithm. `min-content`
/ `max-content` / `fit-content` derive from intrinsic sizing.

## Background

`background` is a shorthand for `background-color` and `background-image`.
Multiple comma-separated layers stack from front to back.

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    background: linear-gradient(to right, #1976d2, #42a5f5);
    background: radial-gradient(circle at 30% 30%, #fff, #999);
    background: conic-gradient(from 0deg, red, yellow, green, blue, red);
    background: url('chrome.png'), #f0f0f0;
");
```

| Property | Values | Type |
|---|---|---|
| `background` (shorthand) | color, gradient, image, multiple layers | `StyleBackgroundContentVec` |
| `background-color` | color | `ColorU` |
| `background-image` | `linear-gradient(...)`, `radial-gradient(...)`, `conic-gradient(...)`, `url(...)`, `none` | `StyleBackgroundContent` |
| `background-position` | length pair, `top`/`bottom`/`left`/`right`/`center` | `StyleBackgroundPosition` |
| `background-size` | length pair, `cover`, `contain`, `auto` | `StyleBackgroundSize` |
| `background-repeat` | `repeat`, `repeat-x`, `repeat-y`, `no-repeat`, `round`, `space` | `StyleBackgroundRepeat` |

Gradient direction accepts angles (`90deg`), `to <side>` syntax (`to top right`),
or corner directions. See `Direction` at `css/src/props/basic/direction.rs`.

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

| Property | Values | Type |
|---|---|---|
| `border` (shorthand) | width style color | sets all four sides |
| `border-width`, `border-{top,right,bottom,left}-width` | `thin`, `medium`, `thick`, length | `StyleBorder*Width` |
| `border-style`, `border-{top,right,bottom,left}-style` | `none`, `solid`, `double`, `dotted`, `dashed`, `hidden`, `groove`, `ridge`, `inset`, `outset` | `BorderStyle` |
| `border-color`, `border-{top,right,bottom,left}-color` | color | `StyleBorder*Color` |
| `border-radius`, `border-{top-left,top-right,bottom-left,bottom-right}-radius` | length | `StyleBorder*Radius` |

`border` shorthand expands to all four sides; `border-top` etc. expand to
the three properties of one side.

## Box shadow and filter

| Property | Values | Type |
|---|---|---|
| `box-shadow` | `<x> <y> [blur] [spread] [color] [inset]` | `StyleBoxShadow` |
| `text-shadow` | `<x> <y> [blur] [color]` | `StyleBoxShadow` |
| `filter` | `blur(<len>)`, `brightness(<%>)`, `contrast(<%>)`, `grayscale(<%>)`, `hue-rotate(<angle>)`, `invert(<%>)`, `opacity(<%>)`, `saturate(<%>)`, `sepia(<%>)`, `drop-shadow(...)` | `StyleFilterVec` |
| `backdrop-filter` | same as `filter` | `StyleFilterVec` |
| `mix-blend-mode` | `normal`, `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity` | `StyleMixBlendMode` |

`box-shadow` accepts multiple comma-separated shadows. The first one is on
top. See `StyleBoxShadow` at `css/src/props/style/box_shadow.rs:39`.

## Text

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    font-family: 'Inter', sans-serif;
    font-size: 16px;
    font-weight: 600;
    line-height: 1.5;
    color: #222;
    text-align: justify;
    letter-spacing: 0.02em;
    text-decoration: underline;
");
```

| Property | Values | Type |
|---|---|---|
| `color` | color | `StyleTextColor` |
| `font-family` | comma-separated names, generic family (`sans-serif`, `serif`, `monospace`, `cursive`, `fantasy`), `system:ui` | `StyleFontFamilyVec` |
| `font-size` | length, percentage | `StyleFontSize` |
| `font-weight` | `100`–`900`, `normal`, `bold`, `bolder`, `lighter` | `StyleFontWeight` |
| `font-style` | `normal`, `italic`, `oblique` | `StyleFontStyle` |
| `line-height` | unitless number, length, percentage | `StyleLineHeight` |
| `letter-spacing` | length | `StyleLetterSpacing` |
| `word-spacing` | length | `StyleWordSpacing` |
| `tab-size` | integer, length | `StyleTabWidth` |
| `text-align` | `left`, `right`, `center`, `justify`, `start`, `end` | `StyleTextAlign` |
| `text-decoration` | `none`, `underline`, `overline`, `line-through`, combinations | `StyleTextDecoration` |
| `text-transform` | `none`, `uppercase`, `lowercase`, `capitalize` | `StyleTextTransform` |
| `text-orientation` | `mixed`, `upright`, `sideways` | `StyleTextOrientation` |
| `direction` | `ltr`, `rtl` | `LayoutDirection` |
| `white-space` | `normal`, `nowrap`, `pre`, `pre-wrap`, `pre-line` | `LayoutWhiteSpace` |
| `word-break` | `normal`, `break-all`, `keep-all` | `LayoutWordBreak` |
| `overflow-wrap` | `normal`, `break-word`, `anywhere` | `LayoutOverflowWrap` |
| `hyphens` | `none`, `manual`, `auto` | `LayoutHyphens` |
| `hyphenation-language` (Azul) | BCP 47 tag | `StyleHyphenationLanguage` |

System font keywords resolve to platform defaults (`SF Pro`, `Segoe UI`,
`Cantarell`, ...) through `SystemFontType` at `css/src/system.rs:172`. See
[System Themes](themes.md).

## Transforms and 3D

| Property | Values | Type |
|---|---|---|
| `transform` | `translate(<len>, <len>)`, `translateX/Y/Z`, `scale(<n>, <n>)`, `scaleX/Y/Z`, `rotate(<angle>)`, `rotateX/Y/Z`, `skew(<angle>, <angle>)`, `matrix(...)`, `matrix3d(...)`, `perspective(<len>)` | `StyleTransformVec` |
| `transform-origin` | length pair | `StyleTransformOrigin` (default: `50% 50%`) |
| `transform-style` | `flat`, `preserve-3d` | `StyleTransformStyle` |
| `perspective` | length, `none` | `StylePerspective` |
| `perspective-origin` | length pair | `StylePerspectiveOrigin` |
| `backface-visibility` | `visible`, `hidden` | `StyleBackfaceVisibility` |

Transforms compose left to right. Multiple functions in one declaration
chain — `transform: translate(10px, 0) rotate(45deg) scale(1.2)` first
translates, then rotates around the origin, then scales.

## Flexbox and grid

The flexbox/grid syntax is standard CSS. The full reference will live in
[Layout](../layout.md). Quick lookup:

| Property | Values | Type |
|---|---|---|
| `flex` (shorthand) | `<grow> <shrink> <basis>` | `LayoutFlex` |
| `flex-direction` | `row`, `row-reverse`, `column`, `column-reverse` | `LayoutFlexDirection` |
| `flex-wrap` | `nowrap`, `wrap`, `wrap-reverse` | `LayoutFlexWrap` |
| `flex-grow`, `flex-shrink` | float | `LayoutFlexGrow`, `LayoutFlexShrink` |
| `flex-basis` | length, `auto`, `content` | `LayoutFlexBasis` |
| `align-items`, `align-self`, `align-content` | `start`, `end`, `center`, `stretch`, `baseline`, `flex-start`, `flex-end`, `space-between`, `space-around`, `space-evenly` | `LayoutAlign*` |
| `justify-content`, `justify-items`, `justify-self` | same as align | `LayoutJustify*` |
| `gap`, `row-gap`, `column-gap` | length pair | `LayoutGap` |
| `grid-template-rows`, `grid-template-columns` | track list with `fr`, `auto`, length | `LayoutGridTemplate` |
| `grid-row`, `grid-column` | `<start> / <end>` | `LayoutGrid*` |
| `grid-area` | shorthand or named area | `LayoutGridArea` |

`fr` units have a 100× internal multiplier — see the layout guide for
intrinsic-sizing rules.

## Effects

| Property | Values | Type |
|---|---|---|
| `opacity` | `0.0`–`1.0`, percentage | `StyleOpacity` |
| `visibility` | `visible`, `hidden`, `collapse` | `StyleVisibility` |
| `cursor` | `default`, `pointer`, `text`, `move`, `crosshair`, `wait`, `help`, `grab`, `grabbing`, `not-allowed`, `progress`, `cell`, `col-resize`, `row-resize`, `e-resize`, `n-resize`, `s-resize`, `w-resize`, `se-resize`, ... | `StyleCursor` |
| `object-fit` | `fill`, `contain`, `cover`, `none`, `scale-down` | `StyleObjectFit` |
| `object-position` | length pair | `StyleObjectPosition` |
| `aspect-ratio` | `<n>` or `<n> / <m>` | `StyleAspectRatio` |
| `pointer-events` | `auto`, `none` | `LayoutPointerEvents` |

The full cursor enum (`StyleCursor` at `css/src/props/style/effects.rs:135`)
covers every common system cursor on each platform.

## Lists

| Property | Values | Type |
|---|---|---|
| `list-style-type` | `none`, `disc`, `circle`, `square`, `decimal`, `decimal-leading-zero`, `lower-roman`, `upper-roman`, `lower-alpha`, `upper-alpha`, `lower-greek`, `upper-greek` | `StyleListStyleType` |
| `list-style-position` | `inside`, `outside` | `StyleListStylePosition` |

## Scrolling

| Property | Values | Type |
|---|---|---|
| `scroll-behavior` | `auto`, `smooth` | `ScrollBehavior` |
| `overscroll-behavior`, `overscroll-behavior-x`, `overscroll-behavior-y` | `auto`, `contain`, `none` | `OverscrollBehavior` |
| `scrollbar-width` | `auto`, `thin`, `none`, length | `StyleScrollbarWidth` |
| `scrollbar-color` | `<thumb> <track>` | `StyleScrollbarColor` |
| `scrollbar-track-color` | color | `StyleScrollbarTrackColor` |
| `scrollbar-thumb-color` | color | `StyleScrollbarThumbColor` |

## Selection appearance (Azul)

| Property | Values | Type |
|---|---|---|
| `selection-background-color` | color | `SelectionBackgroundColor` |
| `selection-color` | color | `SelectionColor` |
| `selection-radius` | length | `SelectionRadius` |

These accept the OS-native default automatically (see [System Themes](themes.md)).

## Generated content

| Property | Values | Type |
|---|---|---|
| `content` | string, `none`, `normal`, keyword | `Content` |
| `counter-reset` | name list | `CounterReset` |
| `counter-increment` | name list | `CounterIncrement` |
| `string-set` | name + content | `StringSet` |

The runtime support for `::before` / `::after` generated content is
in-progress — the parser accepts the syntax today but the layout engine
does not yet emit nodes from it.

## Animation and transitions

The animation runtime is not yet wired up — see the
[Animations](../animations.md) page. The properties parse and round-trip:

| Property | Values | Type |
|---|---|---|
| `transition` | shorthand: property duration timing-function delay | `StyleTransition` |
| `transition-property` | property name list, `all`, `none` | `StyleTransitionProperty` |
| `transition-duration` | time | `StyleTransitionDuration` |
| `transition-timing-function` | `ease`, `linear`, `ease-in`, `ease-out`, `ease-in-out`, `cubic-bezier(...)` | `AnimationInterpolationFunction` |
| `transition-delay` | time | `StyleTransitionDelay` |
| `animation` | shorthand: name duration timing-function delay iteration-count direction fill-mode play-state | `StyleAnimation` |
| `animation-name`, `animation-duration`, `animation-timing-function`, `animation-delay`, `animation-iteration-count`, `animation-direction`, `animation-fill-mode`, `animation-play-state` | individual values | per-property types |

## Azul-specific properties

| Property | Values | Type |
|---|---|---|
| `exclusion-margin`, `exclusion-margin-top`, `exclusion-margin-bottom`, `exclusion-margin-left`, `exclusion-margin-right` | length | `StyleExclusionMargin` |
| `hyphenation-language` | BCP 47 tag | `StyleHyphenationLanguage` |
| `shape-inside`, `shape-outside`, `clip-path` | `inset(...)`, `circle(...)`, `ellipse(...)`, `polygon(...)`, `path(...)`, `url(...)`, `none` | `StyleShape` |

`exclusion-margin` is azul's CSS Exclusions mode — it lets text flow around
floated/positioned elements with a configurable inset.

## Where to read the source

- `css/src/props/style/background.rs:62` — `StyleBackgroundContent`
- `css/src/props/style/border.rs:28` — `BorderStyle`
- `css/src/props/style/box_shadow.rs:39` — `StyleBoxShadow`
- `css/src/props/style/effects.rs:22` — `StyleOpacity`
- `css/src/props/style/effects.rs:135` — `StyleCursor`
- `css/src/props/style/filter.rs:47` — `StyleFilter`
- `css/src/props/style/text.rs:32` — `StyleTextColor`
- `css/src/props/style/text.rs:61` — `StyleTextAlign`
- `css/src/props/style/transform.rs:64` — `StyleTransformOrigin`
- `css/src/props/style/lists.rs:15` — `StyleListStyleType`
- `css/src/props/style/scrollbar.rs:26` — `ScrollBehavior`
- `css/src/props/style/selection.rs` — selection types
- `css/src/props/property.rs` — `CssProperty` and `CssPropertyType` (the master enum)
