# CSS Styling

Azul uses CSS for styling UI elements. This guide covers supported properties and patterns.

## Inline Styles

Apply styles directly to elements:

```rust
Dom::div()
    .with_inline_style("
        padding: 20px;
        margin: 10px;
        background: #f5f5f5;
        border-radius: 8px;
        color: #333;
    ")
```

## Flexbox Layout

Azul supports Flexbox for layout:

```rust
// Horizontal layout
Dom::div()
    .with_inline_style("display: flex; flex-direction: row; gap: 10px;")
    .with_children(vec![
        Dom::text("Item 1"),
        Dom::text("Item 2"),
        Dom::text("Item 3"),
    ].into())

// Vertical layout
Dom::div()
    .with_inline_style("display: flex; flex-direction: column; gap: 10px;")
```

### Flexbox Properties

| Property | Values |
|----------|--------|
| `display` | `flex`, `block`, `inline`, `none` |
| `flex-direction` | `row`, `column`, `row-reverse`, `column-reverse` |
| `flex-wrap` | `nowrap`, `wrap`, `wrap-reverse` |
| `justify-content` | `flex-start`, `flex-end`, `center`, `space-between`, `space-around` |
| `align-items` | `flex-start`, `flex-end`, `center`, `stretch`, `baseline` |
| `align-content` | `flex-start`, `flex-end`, `center`, `stretch`, `space-between`, `space-around` |
| `gap` | `<length>` |

## Box Model

```rust
Dom::div()
    .with_inline_style("
        width: 200px;
        height: 100px;
        padding: 10px 20px;
        margin: 5px;
        border: 2px solid #ccc;
        border-radius: 8px;
        box-sizing: border-box;
    ")
```

## Typography

Azul supports both explicit font families and semantic system font types that resolve to platform-appropriate fonts at runtime.

### Explicit Font Families

```rust
Dom::div()
    .with_inline_style("
        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        font-size: 16px;
        font-weight: bold;
        font-style: italic;
        line-height: 1.5;
        text-align: center;
        text-decoration: underline;
        color: #333;
    ")
```

### System Font Types

Use `system:` prefix to get platform-native fonts that automatically resolve based on the OS:

```css
/* UI font for buttons, labels, menus */
font-family: system:ui;
font-family: system:ui:bold;

/* Monospace font for code */
font-family: system:monospace;
font-family: system:monospace:bold;
font-family: system:monospace:italic;

/* Title font for headings */
font-family: system:title;
font-family: system:title:bold;

/* Menu font */
font-family: system:menu;

/* Small/caption font */
font-family: system:small;

/* Serif font for reading content */
font-family: system:serif;
font-family: system:serif:bold;
```

These resolve to platform-specific fonts:

| System Type | macOS | Windows | Linux |
|-------------|-------|---------|-------|
| `system:ui` | SF Pro | Segoe UI Variable | Cantarell, Ubuntu |
| `system:monospace` | SF Mono, Menlo | Cascadia Mono, Consolas | Ubuntu Mono, DejaVu Sans Mono |
| `system:title` | SF Pro Display | Segoe UI Variable Display | Cantarell |
| `system:serif` | New York | Cambria, Georgia | Noto Serif, DejaVu Serif |

### System Fonts with Fallbacks

Combine system fonts with explicit fallbacks:

```css
font-family: system:ui, Arial, sans-serif;
font-family: system:monospace:bold, Consolas, monospace;
```

## Colors

### Explicit Colors

Supported formats:

```css
color: #ff0000;           /* Hex */
color: #f00;              /* Short hex */
color: rgb(255, 0, 0);    /* RGB */
color: rgba(255, 0, 0, 0.5); /* RGBA */
background: red;          /* Named colors */
```

### System Colors

Use `system:` prefix for OS-native semantic colors that adapt to light/dark themes and user preferences:

```css
/* Primary semantic colors */
color: system:text;                    /* Primary text color */
color: system:secondary-text;          /* Less prominent text */
color: system:disabled-text;           /* Disabled elements */
background: system:background;         /* Content background */

/* Accent colors */
background: system:accent;             /* User-selected accent color */
color: system:accent-text;             /* Text on accent backgrounds */

/* Control colors */
background: system:button-face;        /* Button background */
color: system:button-text;             /* Button text */

/* Window colors */
background: system:window-background;  /* Window/panel background */

/* Selection colors */
background: system:selection-background; /* Selected text background */
color: system:selection-text;            /* Selected text color */

/* Additional semantic colors */
color: system:link;                    /* Hyperlink color */
border-color: system:separator;        /* Divider/separator lines */
border-color: system:grid;             /* Table/grid lines */
background: system:find-highlight;     /* Search highlight */
```

These colors automatically adapt to the user's theme (light/dark mode) and accessibility settings.

### The :backdrop Pseudo-Selector

Style elements differently when the window is not focused:

```css
.selected-item {
    background: system:selection-background;
    color: system:selection-text;
}

/* When window loses focus, use inactive selection colors */
.selected-item:backdrop {
    background: system:selection-background-inactive;
    color: system:selection-text-inactive;
}
```

## Backgrounds

```rust
Dom::div()
    .with_inline_style("
        border: 1px solid #ccc;
        border-top: 2px solid #4a90e2;
        border-radius: 8px;
        border-top-left-radius: 4px;
    ")
```

## Shadows

```rust
Dom::div()
    .with_inline_style("
        box-shadow: 0 2px 4px rgba(0,0,0,0.1);
    ")
```

## Overflow and Scrolling

```rust
Dom::div()
    .with_inline_style("
        height: 200px;
        overflow: auto;      /* Scroll when needed */
        overflow-x: hidden;  /* Hide horizontal */
        overflow-y: scroll;  /* Always show vertical */
    ")
```

## Positioning

```rust
// Relative positioning
Dom::div()
    .with_inline_style("position: relative; top: 10px; left: 20px;")

// Absolute positioning
Dom::div()
    .with_inline_style("position: absolute; top: 0; right: 0;")
```

## Hover and Active States

Add dynamic styles using `add_hover_css_property` and `add_active_css_property`:

```rust
use azul_css::props::{
    basic::color::ColorU,
    property::CssProperty,
    style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
};

const HOVER_BG: [StyleBackgroundContent; 1] = [StyleBackgroundContent::Color(ColorU {
    r: 200, g: 200, b: 200, a: 255,
})];

let mut button = Dom::div()
    .with_inline_style("background: #e0e0e0; cursor: pointer;")
    .with_child(Dom::text("Hover me"));

button.root.add_hover_css_property(CssProperty::BackgroundContent(
    StyleBackgroundContentVec::from_const_slice(&HOVER_BG).into(),
));
```

## Cursor Styles

```rust
Dom::div()
    .with_inline_style("cursor: pointer;")  // Hand cursor
    .with_inline_style("cursor: text;")     // Text cursor
    .with_inline_style("cursor: move;")     // Move cursor
```

## Units

| Unit | Description |
|------|-------------|
| `px` | Pixels |
| `%` | Percentage of parent |
| `em` | Relative to font size |
| `rem` | Relative to root font size |
| `vh` | Viewport height |
| `vw` | Viewport width |

## calc() Function

```rust
Dom::div()
    .with_inline_style("
        width: calc(100% - 40px);
        height: calc(100vh - 60px);
    ")
```

## Supported Properties Reference

### Layout
- `display`, `position`, `top`, `right`, `bottom`, `left`
- `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height`
- `margin`, `padding` (and all variants)
- `flex`, `flex-direction`, `flex-wrap`, `flex-grow`, `flex-shrink`, `flex-basis`
- `justify-content`, `align-items`, `align-self`, `align-content`
- `grid-template-columns`, `grid-template-rows`, `grid-column`, `grid-row`, `gap`

### Visual
- `background`, `background-color`, `background-image`
- `border`, `border-radius`, `box-shadow`
- `color`, `opacity`
- `overflow`, `overflow-x`, `overflow-y`

### Typography
- `font-family`, `font-size`, `font-weight`, `font-style`
- `line-height`, `text-align`, `text-decoration`

### Interaction
- `cursor`, `user-select`

[Back to overview](https://azul.rs/guide)
