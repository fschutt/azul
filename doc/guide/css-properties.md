# Azul CSS Properties Reference

This document lists all CSS properties supported by Azul. Properties prefixed with `-azul-` are Azul-specific extensions.

> **Note:** Azul does **not** support `calc()` or `var()` (CSS custom properties). Use concrete values instead.

## Table of Contents

- [Layout Properties](#layout-properties)
- [Sizing Properties](#sizing-properties)
- [Spacing Properties](#spacing-properties)
- [Flexbox Properties](#flexbox-properties)
- [Grid Properties](#grid-properties)
- [Positioning Properties](#positioning-properties)
- [Typography Properties](#typography-properties)
- [Background Properties](#background-properties)
- [Border Properties](#border-properties)
- [Visual Effects](#visual-effects)
- [Scrollbar Properties](#scrollbar-properties)
- [Text Selection Properties](#text-selection-properties)
- [Multi-Column Layout](#multi-column-layout)
- [CSS Regions](#css-regions)
- [Fragmentation](#fragmentation)
- [Lists and Counters](#lists-and-counters)
- [Length Units](#length-units)
- [Color Formats](#color-formats)

---

## Layout Properties

| Property | Values | Description |
|----------|--------|-------------|
| `display` | `flex`, `block`, `inline`, `inline-block`, `none`, `grid` | Display mode |
| `float` | `left`, `right`, `none` | Float positioning |
| `box-sizing` | `content-box`, `border-box` | Box model calculation |
| `clear` | `left`, `right`, `both`, `none` | Clear floats |
| `visibility` | `visible`, `hidden`, `collapse` | Element visibility |
| `writing-mode` | `horizontal-tb`, `vertical-rl`, `vertical-lr` | Text direction |

---

## Sizing Properties

| Property | Values | Description |
|----------|--------|-------------|
| `width` | `<length>`, `<percentage>`, `auto`, `min-content`, `max-content` | Element width |
| `height` | `<length>`, `<percentage>`, `auto`, `min-content`, `max-content` | Element height |
| `min-width` | `<length>`, `<percentage>`, `auto` | Minimum width |
| `min-height` | `<length>`, `<percentage>`, `auto` | Minimum height |
| `max-width` | `<length>`, `<percentage>`, `none` | Maximum width |
| `max-height` | `<length>`, `<percentage>`, `none` | Maximum height |

---

## Spacing Properties

| Property | Values | Description |
|----------|--------|-------------|
| `margin` | `<length>`, `<percentage>`, `auto` | Outer spacing (shorthand) |
| `margin-top` | `<length>`, `<percentage>`, `auto` | Top margin |
| `margin-right` | `<length>`, `<percentage>`, `auto` | Right margin |
| `margin-bottom` | `<length>`, `<percentage>`, `auto` | Bottom margin |
| `margin-left` | `<length>`, `<percentage>`, `auto` | Left margin |
| `padding` | `<length>`, `<percentage>` | Inner spacing (shorthand) |
| `padding-top` | `<length>`, `<percentage>` | Top padding |
| `padding-right` | `<length>`, `<percentage>` | Right padding |
| `padding-bottom` | `<length>`, `<percentage>` | Bottom padding |
| `padding-left` | `<length>`, `<percentage>` | Left padding |
| `padding-inline-start` | `<length>`, `<percentage>` | Logical start padding |
| `padding-inline-end` | `<length>`, `<percentage>` | Logical end padding |

---

## Flexbox Properties

| Property | Values | Description |
|----------|--------|-------------|
| `flex` | `<grow> <shrink> <basis>` | Flex shorthand |
| `flex-direction` | `row`, `column`, `row-reverse`, `column-reverse` | Main axis direction |
| `flex-wrap` | `nowrap`, `wrap`, `wrap-reverse` | Line wrapping |
| `flex-grow` | `<number>` | Grow factor |
| `flex-shrink` | `<number>` | Shrink factor |
| `flex-basis` | `<length>`, `<percentage>`, `auto`, `content` | Initial main size |
| `justify-content` | `flex-start`, `flex-end`, `center`, `space-between`, `space-around`, `space-evenly` | Main axis alignment |
| `align-items` | `flex-start`, `flex-end`, `center`, `stretch`, `baseline` | Cross axis alignment |
| `align-content` | `flex-start`, `flex-end`, `center`, `stretch`, `space-between`, `space-around` | Multi-line alignment |
| `align-self` | `auto`, `flex-start`, `flex-end`, `center`, `stretch`, `baseline` | Individual cross-axis |
| `gap` | `<length>` | Gap between items |
| `row-gap` | `<length>` | Row gap |
| `column-gap` | `<length>` | Column gap |

---

## Grid Properties

| Property | Values | Description |
|----------|--------|-------------|
| `grid` | Complex shorthand | Grid layout shorthand |
| `grid-template-columns` | `<track-list>`, `none` | Column track sizing |
| `grid-template-rows` | `<track-list>`, `none` | Row track sizing |
| `grid-auto-columns` | `<track-size>` | Implicit column sizing |
| `grid-auto-rows` | `<track-size>` | Implicit row sizing |
| `grid-auto-flow` | `row`, `column`, `dense` | Auto-placement algorithm |
| `grid-column` | `<line> / <line>` | Column placement |
| `grid-row` | `<line> / <line>` | Row placement |
| `grid-gap` | `<length>` | Gap between cells (legacy) |
| `justify-self` | `start`, `end`, `center`, `stretch` | Justify within cell |
| `justify-items` | `start`, `end`, `center`, `stretch` | Default justify for items |

---

## Positioning Properties

| Property | Values | Description |
|----------|--------|-------------|
| `position` | `static`, `relative`, `absolute`, `fixed` | Positioning scheme |
| `top` | `<length>`, `<percentage>`, `auto` | Top offset |
| `right` | `<length>`, `<percentage>`, `auto` | Right offset |
| `bottom` | `<length>`, `<percentage>`, `auto` | Bottom offset |
| `left` | `<length>`, `<percentage>`, `auto` | Left offset |
| `z-index` | `<integer>`, `auto` | Stack order |

---

## Typography Properties

| Property | Values | Description |
|----------|--------|-------------|
| `color` | `<color>`, `system:text` | Text color |
| `font` | Complex shorthand | Font shorthand |
| `font-family` | `<family-name>`, `serif`, `sans-serif`, `monospace` | Font family |
| `font-size` | `<length>`, `<percentage>` | Font size |
| `font-weight` | `normal`, `bold`, `100`-`900` | Font weight |
| `font-style` | `normal`, `italic`, `oblique` | Font style |
| `line-height` | `<number>`, `<length>`, `<percentage>`, `normal` | Line height |
| `text-align` | `left`, `right`, `center`, `justify`, `start`, `end` | Horizontal alignment |
| `text-justify` | `auto`, `inter-word`, `inter-character`, `none` | Justify method |
| `vertical-align` | `baseline`, `top`, `middle`, `bottom`, `sub`, `super` | Vertical alignment |
| `letter-spacing` | `<length>`, `normal` | Letter spacing |
| `word-spacing` | `<length>`, `normal` | Word spacing |
| `text-indent` | `<length>`, `<percentage>` | First line indent |
| `text-decoration` | `none`, `underline`, `line-through`, `overline` | Text decoration |
| `white-space` | `normal`, `nowrap`, `pre`, `pre-wrap`, `pre-line` | Whitespace handling |
| `hyphens` | `none`, `manual`, `auto` | Hyphenation |
| `direction` | `ltr`, `rtl` | Text direction |
| `user-select` | `none`, `auto`, `text`, `all` | Text selection |
| `tab-size` | `<integer>`, `<length>` | Tab character width |
| `initial-letter` | `<number>`, `normal` | Drop caps |
| `line-clamp` | `<integer>`, `none` | Line clamping |
| `hanging-punctuation` | `none`, `first`, `last`, `force-end` | Punctuation hanging |
| `text-combine-upright` | `none`, `all`, `digits` | Vertical text combining |
| `-azul-hyphenation-language` | `<language-tag>` | Hyphenation dictionary |
| `text-shadow` | `<x> <y> <blur> <color>` | Text shadow |

---

## Background Properties

| Property | Values | Description |
|----------|--------|-------------|
| `background` | Complex shorthand | Background shorthand |
| `background-color` | `<color>`, `system:background` | Background color |
| `background-image` | `url()`, `linear-gradient()`, `radial-gradient()`, `none` | Background image |
| `background-position` | `<position>` | Image position |
| `background-size` | `<length>`, `cover`, `contain`, `auto` | Image size |
| `background-repeat` | `repeat`, `no-repeat`, `repeat-x`, `repeat-y` | Image repeat |

---

## Border Properties

| Property | Values | Description |
|----------|--------|-------------|
| `border` | `<width> <style> <color>` | Border shorthand |
| `border-top/right/bottom/left` | `<width> <style> <color>` | Side-specific border |
| `border-width` | `<length>` | Border width (all sides) |
| `border-top-width` | `<length>` | Top border width |
| `border-right-width` | `<length>` | Right border width |
| `border-bottom-width` | `<length>` | Bottom border width |
| `border-left-width` | `<length>` | Left border width |
| `border-style` | `solid`, `dashed`, `dotted`, `double`, `none` | Border style |
| `border-top-style` | Same as `border-style` | Top border style |
| `border-right-style` | Same as `border-style` | Right border style |
| `border-bottom-style` | Same as `border-style` | Bottom border style |
| `border-left-style` | Same as `border-style` | Left border style |
| `border-color` | `<color>` | Border color (all sides) |
| `border-top-color` | `<color>` | Top border color |
| `border-right-color` | `<color>` | Right border color |
| `border-bottom-color` | `<color>` | Bottom border color |
| `border-left-color` | `<color>` | Left border color |
| `border-radius` | `<length>`, `<percentage>` | Corner radius |
| `border-top-left-radius` | `<length>`, `<percentage>` | Top-left corner |
| `border-top-right-radius` | `<length>`, `<percentage>` | Top-right corner |
| `border-bottom-left-radius` | `<length>`, `<percentage>` | Bottom-left corner |
| `border-bottom-right-radius` | `<length>`, `<percentage>` | Bottom-right corner |

---

## Visual Effects

| Property | Values | Description |
|----------|--------|-------------|
| `opacity` | `0.0` - `1.0` | Element opacity |
| `box-shadow` | `<x> <y> <blur> <spread> <color>` | Box shadow |
| `-azul-box-shadow-top` | `<shadow>` | Top shadow only |
| `-azul-box-shadow-right` | `<shadow>` | Right shadow only |
| `-azul-box-shadow-bottom` | `<shadow>` | Bottom shadow only |
| `-azul-box-shadow-left` | `<shadow>` | Left shadow only |
| `filter` | `blur()`, `brightness()`, `contrast()`, `grayscale()`, etc. | Visual filters |
| `backdrop-filter` | Same as `filter` | Backdrop filters |
| `mix-blend-mode` | `normal`, `multiply`, `screen`, `overlay`, etc. | Blend mode |
| `transform` | `translate()`, `rotate()`, `scale()`, `skew()`, etc. | Transformations |
| `transform-origin` | `<position>` | Transform origin |
| `perspective-origin` | `<position>` | Perspective origin |
| `backface-visibility` | `visible`, `hidden` | 3D backface |
| `clip-path` | `<shape>`, `url()` | Clipping path |

---

## Scrollbar Properties

| Property | Values | Description |
|----------|--------|-------------|
| `overflow` | `visible`, `hidden`, `scroll`, `auto` | Overflow (shorthand) |
| `overflow-x` | `visible`, `hidden`, `scroll`, `auto` | Horizontal overflow |
| `overflow-y` | `visible`, `hidden`, `scroll`, `auto` | Vertical overflow |
| `scrollbar-width` | `auto`, `thin`, `none` | Scrollbar width |
| `scrollbar-color` | `<thumb-color> <track-color>` | Scrollbar colors |
| `-azul-scrollbar-style` | Complex | Custom scrollbar styling |

---

## Text Selection Properties

Azul-specific properties for customizing text selection appearance:

| Property | Values | Description |
|----------|--------|-------------|
| `-azul-selection-background-color` | `<color>`, `system:selection-background` | Selection background |
| `-azul-selection-color` | `<color>`, `system:selection-text` | Selected text color |
| `-azul-selection-radius` | `<length>` | Selection corner radius |

---

## Caret Properties

| Property | Values | Description |
|----------|--------|-------------|
| `caret-color` | `<color>`, `auto` | Text cursor color |
| `caret-animation-duration` | `<time>` | Caret blink duration |
| `-azul-caret-width` | `<length>` | Caret width |

---

## Multi-Column Layout

| Property | Values | Description |
|----------|--------|-------------|
| `columns` | `<count> <width>` | Columns shorthand |
| `column-count` | `<integer>`, `auto` | Number of columns |
| `column-width` | `<length>`, `auto` | Column width |
| `column-gap` | `<length>`, `normal` | Gap between columns |
| `column-span` | `none`, `all` | Span across columns |
| `column-fill` | `auto`, `balance` | Column filling |
| `column-rule` | `<width> <style> <color>` | Column divider |
| `column-rule-width` | `<length>` | Divider width |
| `column-rule-style` | `solid`, `dashed`, etc. | Divider style |
| `column-rule-color` | `<color>` | Divider color |

---

## CSS Regions

| Property | Values | Description |
|----------|--------|-------------|
| `flow-into` | `<name>`, `none` | Flow content into region |
| `flow-from` | `<name>`, `none` | Pull content from flow |
| `shape-outside` | `<shape>`, `none` | Text wrap shape |
| `shape-inside` | `<shape>`, `none` | Inside text shape |
| `shape-margin` | `<length>` | Shape margin |
| `shape-image-threshold` | `0.0` - `1.0` | Shape alpha threshold |

---

## Fragmentation

Properties for page and column breaks:

| Property | Values | Description |
|----------|--------|-------------|
| `break-before` | `auto`, `avoid`, `page`, `column`, `region` | Break before element |
| `break-after` | `auto`, `avoid`, `page`, `column`, `region` | Break after element |
| `break-inside` | `auto`, `avoid`, `avoid-page`, `avoid-column` | Break inside element |
| `page-break-before` | Legacy alias for `break-before` | |
| `page-break-after` | Legacy alias for `break-after` | |
| `page-break-inside` | Legacy alias for `break-inside` | |
| `orphans` | `<integer>` | Min lines at bottom |
| `widows` | `<integer>` | Min lines at top |
| `box-decoration-break` | `slice`, `clone` | Box decoration handling |

---

## Lists and Counters

| Property | Values | Description |
|----------|--------|-------------|
| `list-style-type` | `disc`, `circle`, `square`, `decimal`, `lower-alpha`, etc. | Marker type |
| `list-style-position` | `inside`, `outside` | Marker position |
| `content` | `<string>`, `counter()`, `attr()`, `none` | Generated content |
| `counter-reset` | `<name> <value>` | Reset counter |
| `counter-increment` | `<name> <value>` | Increment counter |
| `string-set` | `<name> <content>` | Named string |

---

## Cursor

| Property | Values | Description |
|----------|--------|-------------|
| `cursor` | `default`, `pointer`, `text`, `move`, `not-allowed`, `grab`, `grabbing`, `crosshair`, `help`, `wait`, `progress`, `none` | Cursor style |

---

## Length Units

| Unit | Description | Example |
|------|-------------|---------|
| `px` | Pixels (absolute) | `16px` |
| `%` | Percentage of parent | `50%` |
| `em` | Relative to parent font size | `1.5em` |
| `rem` | Relative to root font size | `1rem` |
| `vh` | Viewport height percentage | `100vh` |
| `vw` | Viewport width percentage | `100vw` |
| `vmin` | Smaller of vh/vw | `50vmin` |
| `vmax` | Larger of vh/vw | `50vmax` |
| `ch` | Width of "0" character | `40ch` |
| `ex` | Height of "x" character | `2ex` |
| `pt` | Points (1/72 inch) | `12pt` |
| `pc` | Picas (12 points) | `1pc` |
| `in` | Inches | `1in` |
| `cm` | Centimeters | `2.54cm` |
| `mm` | Millimeters | `25.4mm` |

---

## Color Formats

### Standard CSS Colors

```css
/* Named colors */
color: red;
color: transparent;
color: rebeccapurple;

/* Hexadecimal */
color: #ff0000;        /* RGB */
color: #f00;           /* Short RGB */
color: #ff0000ff;      /* RGBA */
color: #f00f;          /* Short RGBA */

/* Functional notation */
color: rgb(255, 0, 0);
color: rgba(255, 0, 0, 0.5);
color: hsl(0, 100%, 50%);
color: hsla(0, 100%, 50%, 0.5);
```

### System Colors (Azul Extension)

Lazily-evaluated colors that adapt to the user's OS theme:

```css
color: system:text;                  /* System text color */
color: system:background;            /* System background */
background: system:accent;           /* User's accent color */
color: system:accent-text;           /* Text on accent background */
background: system:button-face;      /* Button background */
color: system:button-text;           /* Button text */
background: system:window-background; /* Window background */
background: system:selection-background; /* Selection highlight */
color: system:selection-text;        /* Selected text color */
```

---

## Pseudo-Classes

| Selector | Description |
|----------|-------------|
| `:hover` | Mouse over element |
| `:active` | Element being clicked |
| `:focus` | Element has keyboard focus |
| `:first-child` | First child of parent |
| `:last-child` | Last child of parent |
| `:nth-child(n)` | Nth child (1-indexed) |
| `:nth-child(odd)` | Odd children |
| `:nth-child(even)` | Even children |
| `:disabled` | Disabled form elements |
| `:checked` | Checked checkboxes/radios |

> **Note:** In nested CSS, use `:hover` directly without the `&` prefix (unlike standard CSS nesting).

---

## Not Supported

The following CSS features are **not** supported in Azul:

- `calc()` - Use concrete values
- `var()` - CSS custom properties not available
- `@keyframes` / `animation` - Use Rust animation API
- `@import` - Include styles via Rust code
- `@font-face` - Load fonts via Rust API

---

[Back to Styling System](styling-system.md) | [Back to Guide](https://azul.rs/guide)
