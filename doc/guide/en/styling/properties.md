---
slug: styling/properties
title: CSS Properties Cheatsheet
language: en
canonical_slug: styling/properties
audience: external
maturity: mature
guide_order: 41
topic_only: false
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
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T17:30:00Z
---

# CSS Properties Cheatsheet

Every property name below is the CSS keyword you write in a stylesheet. The
typed Rust enum behind each property lives in `css/src/props/style/` (visual
properties) or `css/src/props/layout/` (layout properties).

## Box model

| Property | Values |
|---|---|
| `width`, `height` | `<length>` ∣ `<percentage>` ∣ `auto` ∣ `min-content` ∣ `max-content` ∣ `fit-content` |
| `min-width`, `min-height` | same as above, default `0` |
| `max-width`, `max-height` | same as above, default `none` |
| `padding-top`, `padding-right`, `padding-bottom`, `padding-left` | `<length>` ∣ `<percentage>` |
| `padding` | 1 to 4 values: `top right bottom left` |
| `padding-inline-start`, `padding-inline-end` | logical-direction padding (LTR / RTL aware) |
| `margin-top`, `margin-right`, `margin-bottom`, `margin-left` | `<length>` ∣ `<percentage>` ∣ `auto` |
| `margin` | 1 to 4 values |
| `box-sizing` | `content-box` (default) ∣ `border-box` |
| `gap`, `row-gap`, `column-gap` | `<length>` (used by `flex` and `grid`) |

Lengths accept `px`, `em`, `rem`, `pt`, `vw`, `vh`, `%`, plus `0` without a
unit.

## Borders

```css
border-top-width: 2px;
border-top-style: solid;
border-top-color: #888;
border-radius: 8px;
```

| Property | Values |
|---|---|
| `border-top-width`, `border-right-width`, `border-bottom-width`, `border-left-width` | `<length>` |
| `border-top-style`, `border-right-style`, `border-bottom-style`, `border-left-style` | `none` ∣ `solid` ∣ `dashed` ∣ `dotted` ∣ `double` ∣ `hidden` ∣ `groove` ∣ `ridge` ∣ `inset` ∣ `outset` |
| `border-top-color`, `border-right-color`, `border-bottom-color`, `border-left-color` | `<color>` |
| `border-top-left-radius`, `border-top-right-radius`, `border-bottom-left-radius`, `border-bottom-right-radius` | `<length>` ∣ `<percentage>` |
| `border-radius` | shorthand: 1 to 4 values for the four corners |
| `border` | shorthand: `<width> <style> <color>` |

```azul-render screenshot=props-borders width=400 height=120 subtitle="border-style sample"
<body style="padding: 20px;">
  <div style="display: flex; gap: 12px;">
    <div style="width: 60px; height: 60px; border: 2px solid #333;"></div>
    <div style="width: 60px; height: 60px; border: 2px dashed #333;"></div>
    <div style="width: 60px; height: 60px; border: 2px dotted #333;"></div>
    <div style="width: 60px; height: 60px; border: 4px double #333;"></div>
    <div style="width: 60px; height: 60px; border: 2px solid #333; border-radius: 12px;"></div>
  </div>
</body>
```

## Box shadow

```css
box-shadow: 2px 4px 12px rgba(0, 0, 0, 0.2);
box-shadow: inset 0 0 8px #444;
text-shadow: 1px 1px 2px #000;
```

Internally the shorthand expands to `box-shadow-top`, `box-shadow-right`,
`box-shadow-bottom`, `box-shadow-left`. Each `StyleBoxShadow` has
`offset`, `color`, `blur_radius`, `spread_radius`, `clip_mode`
(`Outset` ∣ `Inset`). `text-shadow` shares the same value type.

## Background

```css
background: #1976d2;
background-image: linear-gradient(180deg, #ff6e7f 0%, #bfe9ff 100%);
background: radial-gradient(circle at top left, white, #888);
background: conic-gradient(from 90deg, red, yellow, green, blue, red);
background-image: url("logo.png");
background-position: center center;
background-size: cover;
background-repeat: no-repeat;
```

| Property | Values |
|---|---|
| `background`, `background-color`, `background-image` | one or more of: `<color>`, `linear-gradient(...)`, `radial-gradient(...)`, `conic-gradient(...)`, `url("path")`, `repeating-*-gradient(...)` |
| `background-position` | keyword pair (`top` / `right` / `bottom` / `left` / `center`) or `<length>` / `<percentage>` |
| `background-size` | `auto` ∣ `cover` ∣ `contain` ∣ `<length>` ∣ `<percentage>` ∣ `<length> <length>` |
| `background-repeat` | `repeat` ∣ `repeat-x` ∣ `repeat-y` ∣ `no-repeat` |

Multiple background layers are written comma-separated; the first listed
paints on top.

```azul-render screenshot=props-gradients width=400 height=160 subtitle="linear, radial, and conic gradients"
<body style="padding: 16px;">
  <div style="display: flex; gap: 12px;">
    <div style="width: 100px; height: 100px; background: linear-gradient(180deg, #ff6e7f, #bfe9ff);"></div>
    <div style="width: 100px; height: 100px; background: radial-gradient(circle, white, #444); border-radius: 50%;"></div>
    <div style="width: 100px; height: 100px; background: conic-gradient(from 0deg, red, yellow, green, blue, red);"></div>
  </div>
</body>
```

## Color

Color values accept:

- Named colors: `red`, `green`, `blue`, `transparent`, plus the full CSS
  named-color set.
- Hex: `#rgb`, `#rrggbb`, `#rgba`, `#rrggbbaa`.
- `rgb(r, g, b)`, `rgba(r, g, b, a)` — `r/g/b` are 0–255 or `0%–100%`,
  `a` is 0.0–1.0.
- `hsl(h, s%, l%)`, `hsla(h, s%, l%, a)` — `h` is `<angle>`, `s`/`l` are
  percentages.
- System color references — see [System Themes](themes.md).

## Text

```css
color: #222;
font-family: "Inter", "Segoe UI", sans-serif;
font-size: 14px;
font-weight: 600;          /* 100..900 ∣ normal ∣ bold */
font-style: italic;        /* normal ∣ italic ∣ oblique */
line-height: 1.4;          /* unitless multiplier OR <length> */
letter-spacing: 0.02em;
word-spacing: 0.1em;
text-align: center;        /* left ∣ right ∣ center ∣ justify ∣ start ∣ end */
text-decoration: underline;
white-space: nowrap;       /* normal ∣ nowrap ∣ pre ∣ pre-wrap ∣ pre-line ∣ break-spaces */
word-break: break-all;     /* normal ∣ break-all ∣ keep-all ∣ break-word */
overflow-wrap: anywhere;   /* normal ∣ anywhere ∣ break-word */
hyphens: auto;             /* none ∣ manual ∣ auto */
direction: rtl;            /* ltr ∣ rtl */
text-transform: uppercase;
text-indent: 2em;
tab-size: 4;
vertical-align: middle;    /* baseline ∣ top ∣ middle ∣ bottom ∣ <length> */
```

`font-family` accepts a comma-separated list of families. Quoted names
preserve spaces. Generic families: `serif`, `sans-serif`, `monospace`,
`cursive`, `fantasy`. System-font keywords (`-azul-system-ui`,
`-azul-system-monospace`) resolve at runtime — see
[System Themes](themes.md).

`font-weight`: `100`, `200`, `300`, `400` (`normal`), `500`, `600`,
`700` (`bold`), `800`, `900`.

## Layout

Covered in detail in [Layout](../layout.md). Shorthand reference:

| Property | Values |
|---|---|
| `display` | `block` ∣ `inline` ∣ `inline-block` ∣ `flex` ∣ `inline-flex` ∣ `grid` ∣ `inline-grid` ∣ `none` ∣ `table` ∣ `table-row` ∣ `table-cell` |
| `position` | `static` ∣ `relative` ∣ `absolute` ∣ `fixed` ∣ `sticky` |
| `top`, `right`, `bottom`, `left` | `<length>` ∣ `<percentage>` ∣ `auto` |
| `z-index` | `<integer>` ∣ `auto` |
| `float`, `clear` | `left` ∣ `right` ∣ `none` ∣ `both` |
| `flex-direction` | `row` ∣ `row-reverse` ∣ `column` ∣ `column-reverse` |
| `flex-wrap` | `nowrap` ∣ `wrap` ∣ `wrap-reverse` |
| `flex-grow`, `flex-shrink` | `<number>` |
| `flex-basis` | `<length>` ∣ `auto` |
| `justify-content`, `align-items`, `align-content`, `justify-self`, `align-self`, `justify-items` | `flex-start` ∣ `flex-end` ∣ `center` ∣ `space-between` ∣ `space-around` ∣ `space-evenly` ∣ `stretch` ∣ `start` ∣ `end` |
| `grid-template-columns`, `grid-template-rows` | track list with `<length>`, `<percentage>`, `<flex>` (`1fr`), `min-content`, `max-content`, `repeat()` |
| `grid-column`, `grid-row` | `<line>` ∣ `<line> / <line>` |
| `gap`, `row-gap`, `column-gap` | `<length>` |
| `writing-mode` | `horizontal-tb` ∣ `vertical-rl` ∣ `vertical-lr` |

## Effects

| Property | Values |
|---|---|
| `opacity` | `0`–`1` ∣ `0%`–`100%` |
| `visibility` | `visible` ∣ `hidden` ∣ `collapse` |
| `cursor` | `default` ∣ `pointer` ∣ `text` ∣ `wait` ∣ `help` ∣ `move` ∣ `not-allowed` ∣ `crosshair` ∣ `cell` ∣ `grab` ∣ `grabbing` ∣ `progress` ∣ `zoom-in` ∣ `zoom-out` ∣ `n-resize` ∣ `s-resize` ∣ `e-resize` ∣ `w-resize` ∣ `ne-resize` ∣ `nw-resize` ∣ `se-resize` ∣ `sw-resize` ∣ `ns-resize` ∣ `ew-resize` ∣ `nesw-resize` ∣ `nwse-resize` ∣ `col-resize` ∣ `row-resize` ∣ `all-scroll` ∣ `vertical-text` ∣ `context-menu` ∣ `alias` ∣ `copy` |
| `mix-blend-mode` | `normal` ∣ `multiply` ∣ `screen` ∣ `overlay` ∣ `darken` ∣ `lighten` ∣ `color-dodge` ∣ `color-burn` ∣ `hard-light` ∣ `soft-light` ∣ `difference` ∣ `exclusion` ∣ `hue` ∣ `saturation` ∣ `color` ∣ `luminosity` |
| `aspect-ratio` | `<number>` ∣ `<number> / <number>` ∣ `auto` |
| `object-fit` | `fill` ∣ `contain` ∣ `cover` ∣ `none` ∣ `scale-down` |
| `object-position` | same syntax as `background-position` |

## Filters

```css
filter: blur(4px);
filter: brightness(0.8) contrast(1.2);
filter: grayscale(50%);
filter: drop-shadow(2px 2px 4px black);
backdrop-filter: blur(8px);
```

Functions: `blur(<length>)`, `brightness(<percentage>)`,
`contrast(<percentage>)`, `grayscale(<percentage>)`, `hue-rotate(<angle>)`,
`invert(<percentage>)`, `opacity(<percentage>)`, `saturate(<percentage>)`,
`sepia(<percentage>)`, `drop-shadow(<offset-x> <offset-y> <blur> <color>)`.
Multiple filters chain left-to-right. `backdrop-filter` runs the same
function set against what's behind the element.

## Transform

```css
transform: translate(20px, 0);
transform: rotate(45deg);
transform: scale(1.2);
transform: matrix(1, 0, 0, 1, 30, 0);
transform: translate3d(20px, 0, 0) rotate3d(0, 1, 0, 30deg);
transform-origin: center center;     /* 1 or 2 values: keyword or <length> */
perspective-origin: center center;
backface-visibility: hidden;          /* visible ∣ hidden */
```

Functions: `translate`, `translateX`, `translateY`, `translate3d`,
`scale`, `scaleX`, `scaleY`, `scale3d`, `rotate`, `rotate3d`,
`rotateX`, `rotateY`, `rotateZ`, `skew`, `skewX`, `skewY`, `matrix`,
`matrix3d`, `perspective`. Multiple functions in one declaration apply
right-to-left.

## Scrollbar

| Property | Values |
|---|---|
| `overflow-x`, `overflow-y` | `visible` ∣ `hidden` ∣ `scroll` ∣ `auto` |
| `scrollbar-width` | `auto` ∣ `thin` ∣ `none` ∣ `<length>` |
| `scrollbar-color` | `<color> <color>` (thumb, track) |
| `scrollbar-track`, `scrollbar-thumb`, `scrollbar-button`, `scrollbar-corner`, `scrollbar-resizer` | any `<background>` value (color, gradient, image) |
| `scrollbar-visibility` | `auto` ∣ `always` ∣ `never` ∣ `overlay` |
| `scrollbar-fade-delay`, `scrollbar-fade-duration` | `<time>` |

## Selection

Azul-specific properties for styling selected text:

| Property | Values |
|---|---|
| `selection-background-color` | `<color>` |
| `selection-color` | `<color>` |
| `selection-radius` | `<length>` (rounded corners on the selection rectangles) |

## Lists

```css
list-style-type: decimal;     /* none ∣ disc ∣ circle ∣ square ∣ decimal ∣ lower-roman ∣ upper-roman ∣ lower-alpha ∣ upper-alpha */
list-style-position: outside; /* inside ∣ outside */
list-style-image: url("dot.png");
```

## Generated content

```css
.note::before { content: "Note: "; }
counter-reset: section 0;
counter-increment: section;
string-set: header content();
```

`Content` and `StringSet` parse but the runtime hookup is partial — see the
review note in `css/src/props/style/content.rs`.

## Animation timing

```css
transition: opacity 200ms ease-out, transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
animation: fade-in 0.5s ease-in;
caret-animation-duration: 1s;
```

Timing functions: `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`,
`cubic-bezier(x1, y1, x2, y2)`. Time units: `s`, `ms`. The animation
runtime is partly wired — see [Animations](../animations.md).

## Azul-specific properties

| Property | Purpose |
|---|---|
| `-azul-exclusion-margin` | Padding the layout solver leaves around an element when wrapping flowed text around it. |
| `-azul-hyphenation-language` | BCP 47 tag passed to the hyphenation engine; defaults to the document language. |
| `caret-color` | Caret color in editable text fields. |
| `caret-width` | Caret stroke width (`<length>`). |
| `caret-animation-duration` | Caret blink period (`<time>`). |

## Logical properties (writing-mode aware)

`padding-inline-start`, `padding-inline-end`, `margin-inline-start`,
`margin-inline-end`, `inset-inline-start`, `inset-inline-end`,
`overflow-block`, `overflow-inline`. These map to physical sides based on
the resolved `direction` and `writing-mode`.

## Shorthand expansion

The parser expands shorthands at parse time, so the `CssDeclaration` list
contains only longhand properties. `border: 1px solid red` produces three
declarations: `border-*-width`, `border-*-style`, `border-*-color` for each
of the four sides. `padding: 4px 8px` expands to four declarations.
