# Azul Styling System

This document explains Azul's styling architecture, available CSS values, and how the system avoids the "m×n" complexity problem found in traditional CSS engines.

## The M×N Problem

Traditional CSS engines suffer from an "m×n" complexity problem during style resolution:

- **m**: Number of CSS selectors in stylesheets
- **n**: Number of DOM nodes in the document

For every DOM node, the engine must check every CSS selector to determine which rules apply. This results in **O(m × n)** complexity, which becomes a performance bottleneck for large documents with complex stylesheets.

### Why This Matters

```
Document with 10,000 DOM nodes
Stylesheet with 5,000 selectors
= 50,000,000 selector matches to evaluate
```

Even with optimizations like Bloom filters and selector indexing, this fundamental complexity limits performance.

## Azul's Solution: Compiled CSS

Azul solves this by compiling CSS at parse time into efficient lookup structures:

```
┌─────────────────────────────────────────────────────────────┐
│                    CSS Stylesheet                            │
│  .button { ... }                                            │
│  .card .title { ... }                                       │
│  #header nav a:hover { ... }                                │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼ (compile once at parse time)
┌─────────────────────────────────────────────────────────────┐
│              Compiled Selector Index                         │
│  - Hash maps by class, id, tag                              │
│  - Pre-computed specificity                                  │
│  - Optimized matching order                                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼ (O(1) lookup per node)
┌─────────────────────────────────────────────────────────────┐
│                    DOM Node                                  │
│  - Direct property lookup                                    │
│  - No selector iteration                                     │
└─────────────────────────────────────────────────────────────┘
```

With this approach:
- Style resolution is **O(n)** instead of **O(m × n)**
- CSS parsing cost is amortized over many frames
- Hot path (rendering) is optimized for speed

## SystemStyle: Automatic Platform Detection

Azul detects the current platform and provides appropriate styling:

```rust
use azul::system::SystemStyle;

// Automatically detects:
// - OS (Windows, macOS, Linux)
// - OS version (Windows 11 22H2, macOS Sonoma, etc.)
// - Desktop environment (GNOME, KDE, etc.)
// - Theme (light/dark)
// - Accessibility settings (reduced motion, high contrast)
let style = SystemStyle::detect();

println!("Platform: {:?}", style.platform);
println!("Theme: {:?}", style.theme);
println!("OS Version: {:?}", style.os_version);
println!("Reduced Motion: {:?}", style.prefers_reduced_motion);
println!("High Contrast: {:?}", style.prefers_high_contrast);
```

### Detected Properties

| Property | Description |
|----------|-------------|
| `platform` | OS family: `Windows`, `MacOs`, `Linux(Gnome)`, `Linux(Kde)`, `Android`, `Ios` |
| `theme` | `Light` or `Dark` based on system settings |
| `os_version` | Specific OS version (e.g., `WIN_11_23H2`, `MACOS_SONOMA`, `LINUX_6_0`) |
| `colors` | System accent color, text color, background color, etc. |
| `fonts` | UI font, monospace font, font sizes |
| `metrics` | Corner radius, border width, spacing |
| `prefers_reduced_motion` | User accessibility setting |
| `prefers_high_contrast` | User accessibility setting |
| `language` | System locale in BCP 47 format (e.g., "en-US", "de-DE") |

## Dynamic CSS Selectors

Azul supports CSS selectors that adapt to runtime conditions:

### OS-Based Selectors

```css
/* Target specific platforms */
@supports (os: windows) {
    button {
        font-family: "Segoe UI", sans-serif;
    }
}

@supports (os: macos) {
    button {
        font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    }
}

@supports (os: linux) {
    button {
        font-family: "Cantarell", "Noto Sans", sans-serif;
    }
}
```

### Version-Based Selectors

```css
/* Windows 11 specific styles */
@supports (os-version: >= windows-11) {
    .card {
        border-radius: 8px;
    }
}

/* Fallback for older Windows */
@supports (os-version: < windows-11) {
    .card {
        border-radius: 0;
    }
}
```

### Theme Selectors

```css
/* Automatic light/dark mode */
@media (prefers-color-scheme: dark) {
    body {
        background: #1e1e1e;
        color: #e0e0e0;
    }
}

@media (prefers-color-scheme: light) {
    body {
        background: #ffffff;
        color: #333333;
    }
}
```

### Accessibility Selectors

```css
/* Respect reduced motion preferences */
@media (prefers-reduced-motion: reduce) {
    * {
        animation: none !important;
        transition: none !important;
    }
}

/* High contrast mode */
@media (prefers-contrast: more) {
    .button {
        border: 2px solid currentColor;
    }
}
```

### Desktop Environment Selectors

```css
/* GNOME-specific styles */
@supports (desktop-env: gnome) {
    .headerbar {
        background: linear-gradient(to bottom, #f6f5f4, #edebe9);
    }
}

/* KDE-specific styles */
@supports (desktop-env: kde) {
    .headerbar {
        background: #eff0f1;
    }
}
```

## Available CSS Properties

### Layout Properties

| Property | Values | Description |
|----------|--------|-------------|
| `display` | `flex`, `block`, `inline`, `none` | Display mode |
| `position` | `static`, `relative`, `absolute`, `fixed` | Positioning |
| `flex-direction` | `row`, `column`, `row-reverse`, `column-reverse` | Flex axis |
| `flex-wrap` | `nowrap`, `wrap`, `wrap-reverse` | Flex wrapping |
| `justify-content` | `flex-start`, `flex-end`, `center`, `space-between`, `space-around`, `space-evenly` | Main axis alignment |
| `align-items` | `flex-start`, `flex-end`, `center`, `stretch`, `baseline` | Cross axis alignment |
| `align-content` | `flex-start`, `flex-end`, `center`, `stretch`, `space-between`, `space-around` | Multi-line alignment |
| `align-self` | `auto`, `flex-start`, `flex-end`, `center`, `stretch`, `baseline` | Individual alignment |
| `gap` | `<length>` | Gap between flex/grid items |
| `row-gap` | `<length>` | Row gap |
| `column-gap` | `<length>` | Column gap |

### Sizing Properties

| Property | Values | Description |
|----------|--------|-------------|
| `width` | `<length>`, `<percentage>`, `auto`, `min-content`, `max-content` | Element width |
| `height` | `<length>`, `<percentage>`, `auto`, `min-content`, `max-content` | Element height |
| `min-width` | `<length>`, `<percentage>` | Minimum width |
| `max-width` | `<length>`, `<percentage>`, `none` | Maximum width |
| `min-height` | `<length>`, `<percentage>` | Minimum height |
| `max-height` | `<length>`, `<percentage>`, `none` | Maximum height |
| `flex-grow` | `<number>` | Flex grow factor |
| `flex-shrink` | `<number>` | Flex shrink factor |
| `flex-basis` | `<length>`, `<percentage>`, `auto`, `content` | Initial main size |
| `aspect-ratio` | `<number>`, `<ratio>` | Aspect ratio |

### Spacing Properties

| Property | Values | Description |
|----------|--------|-------------|
| `margin` | `<length>`, `<percentage>`, `auto` | Outer spacing (shorthand) |
| `margin-top/right/bottom/left` | `<length>`, `<percentage>`, `auto` | Individual margins |
| `padding` | `<length>`, `<percentage>` | Inner spacing (shorthand) |
| `padding-top/right/bottom/left` | `<length>`, `<percentage>` | Individual padding |

### Border Properties

| Property | Values | Description |
|----------|--------|-------------|
| `border` | `<width> <style> <color>` | Border shorthand |
| `border-width` | `<length>` | Border width |
| `border-style` | `solid`, `dashed`, `dotted`, `double`, `none` | Border style |
| `border-color` | `<color>` | Border color |
| `border-radius` | `<length>`, `<percentage>` | Corner radius |
| `border-top-left-radius` | `<length>`, `<percentage>` | Individual corner |

### Background Properties

| Property | Values | Description |
|----------|--------|-------------|
| `background` | `<color>`, `<gradient>`, `<image>` | Background shorthand |
| `background-color` | `<color>` | Background color |
| `background-image` | `url()`, `linear-gradient()`, `radial-gradient()` | Background image |
| `background-position` | `<position>` | Image position |
| `background-size` | `<length>`, `cover`, `contain` | Image size |
| `background-repeat` | `repeat`, `no-repeat`, `repeat-x`, `repeat-y` | Image repeat |

### Typography Properties

| Property | Values | Description |
|----------|--------|-------------|
| `color` | `<color>` | Text color |
| `font-family` | `<family-name>`, `serif`, `sans-serif`, `monospace` | Font family |
| `font-size` | `<length>`, `<percentage>` | Font size |
| `font-weight` | `normal`, `bold`, `100`-`900` | Font weight |
| `font-style` | `normal`, `italic`, `oblique` | Font style |
| `line-height` | `<number>`, `<length>`, `<percentage>` | Line height |
| `text-align` | `left`, `right`, `center`, `justify` | Text alignment |
| `text-decoration` | `none`, `underline`, `line-through`, `overline` | Text decoration |
| `text-transform` | `none`, `uppercase`, `lowercase`, `capitalize` | Text transformation |
| `letter-spacing` | `<length>` | Letter spacing |
| `word-spacing` | `<length>` | Word spacing |
| `white-space` | `normal`, `nowrap`, `pre`, `pre-wrap`, `pre-line` | Whitespace handling |
| `text-overflow` | `clip`, `ellipsis` | Overflow behavior |

### Visual Effects

| Property | Values | Description |
|----------|--------|-------------|
| `opacity` | `0.0` - `1.0` | Element opacity |
| `box-shadow` | `<x> <y> <blur> <spread> <color>` | Box shadow |
| `filter` | `blur()`, `brightness()`, `contrast()`, etc. | Visual filters |
| `transform` | `translate()`, `rotate()`, `scale()`, etc. | Transformations |
| `transition` | `<property> <duration> <timing> <delay>` | Transitions |

### Cursor and Interaction

| Property | Values | Description |
|----------|--------|-------------|
| `cursor` | `default`, `pointer`, `text`, `move`, `not-allowed`, etc. | Cursor style |
| `user-select` | `none`, `auto`, `text`, `all` | Text selection |
| `pointer-events` | `auto`, `none` | Pointer events |

### Overflow and Scrolling

| Property | Values | Description |
|----------|--------|-------------|
| `overflow` | `visible`, `hidden`, `scroll`, `auto` | Overflow handling |
| `overflow-x` | `visible`, `hidden`, `scroll`, `auto` | Horizontal overflow |
| `overflow-y` | `visible`, `hidden`, `scroll`, `auto` | Vertical overflow |
| `scroll-behavior` | `auto`, `smooth` | Scroll behavior |

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

## Color Formats

```css
/* Named colors */
color: red;
color: transparent;

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

## CSS Functions

> **Note:** `calc()` and `var()` (CSS custom properties) are **not supported** in Azul.
> Use concrete values instead.

### Gradients

```css
/* Linear gradients */
background: linear-gradient(to right, #ff0000, #0000ff);
background: linear-gradient(45deg, red, blue);
background: linear-gradient(to bottom, #fff 0%, #eee 100%);

/* Radial gradients */
background: radial-gradient(circle, #fff, #000);
background: radial-gradient(ellipse at center, red, blue);
```

## Pseudo-Classes

| Selector | Description |
|----------|-------------|
| `:hover` | Mouse over element |
| `:active` | Element being clicked |
| `:focus` | Element has keyboard focus |
| `:first-child` | First child of parent |
| `:last-child` | Last child of parent |
| `:nth-child(n)` | Nth child of parent |
| `:nth-child(odd)` | Odd children |
| `:nth-child(even)` | Even children |
| `:disabled` | Disabled form elements |
| `:checked` | Checked checkboxes/radio buttons |

## CSS Nesting

Azul supports CSS nesting syntax. Note that unlike standard CSS nesting, 
Azul uses `:hover` directly without the `&` prefix:

```css
.card {
    background: white;
    padding: 16px;
    
    /* Nested rules */
    .title {
        font-size: 18px;
        font-weight: bold;
    }
    
    .content {
        color: #666;
    }
    
    /* Pseudo-classes - no & prefix needed */
    :hover {
        box-shadow: 0 4px 8px rgba(0,0,0,0.1);
    }
    
    /* Media queries */
    @media (prefers-color-scheme: dark) {
        background: #333;
        color: white;
    }
}
```

## Best Practices

### 1. Use System Colors

Azul provides lazily-evaluated system colors that adapt to the user's OS theme:

```css
/* Use system colors in CSS */
.button {
    background: system:accent;
    color: system:accent-text;
}

.card {
    background: system:window-background;
    color: system:text;
}

::selection {
    background: system:selection-background;
    color: system:selection-text;
}
```

Available system colors:

| CSS Syntax | Description |
|------------|-------------|
| `system:text` | System text color (black/white depending on theme) |
| `system:background` | System background color |
| `system:accent` | User's accent/highlight color |
| `system:accent-text` | Text color for use on accent backgrounds |
| `system:button-face` | Button background color |
| `system:button-text` | Button text color |
| `system:window-background` | Window/panel background |
| `system:selection-background` | Text selection background |
| `system:selection-text` | Text color when selected |

In Rust code:

```rust
let style = SystemStyle::detect();
let accent = style.colors.accent; // System accent color
let selection = style.colors.selection_background;
```

### 2. Respect Accessibility Settings

```css
@media (prefers-reduced-motion: reduce) {
    * {
        animation-duration: 0.01ms !important;
        transition-duration: 0.01ms !important;
    }
}
```

### 3. Use Semantic Elements

```rust
// Good: semantic elements
Dom::create_button("Submit")
Dom::create_p("Hello")

// Avoid: create_text (use create_p, create_span, etc.)
Dom::create_text("Hello")  // Bad: no semantic meaning
Dom::create_div().with_class("button")  // Bad: div soup
```

### 4. Prefer Flexbox

```css
/* Modern, flexible layout */
.container {
    display: flex;
    flex-direction: column;
    gap: 16px;
}
```

## Application Ricing

Azul supports user-customizable stylesheets, allowing end-users to "rice" (customize) 
any Azul application without modifying the application code.

### How It Works

When an Azul application starts, it automatically looks for a user stylesheet at:

| Platform | Location |
|----------|----------|
| **Linux** | `~/.config/azul/styles/<app_name>.css` |
| **macOS** | `~/Library/Application Support/azul/styles/<app_name>.css` |
| **Windows** | `%APPDATA%\azul\styles\<app_name>.css` |

Where `<app_name>` is the name of the executable (e.g., `my-app` for `my-app.exe`).

### Example

If you have an application called `todo-app`, create:

```
~/.config/azul/styles/todo-app.css
```

With content like:

```css
/* Override button styling */
button {
    background: linear-gradient(to bottom, #667eea, #764ba2);
    border-radius: 20px;
    color: white;
}

/* Custom accent color */
.accent {
    background: system:accent;
}

/* Dark theme override */
body {
    background: #1a1a2e;
    color: #eaeaea;
}
```

### Disabling Ricing

Application developers can disable user ricing by setting an environment variable:

```bash
AZUL_DISABLE_RICING=1 ./my-app
```

### Linux "Smoke and Mirrors" Mode

For Linux users with heavily customized ("riced") desktops using tiling window managers,
Azul can detect colors from tools like `pywal`:

```bash
# Enable riced desktop detection (skips GNOME/KDE detection)
AZUL_SMOKE_AND_MIRRORS=1 ./my-app
```

This mode:
- Reads colors from `~/.cache/wal/colors.json` (pywal)
- Parses `~/.config/hypr/hyprland.conf` for border radius and colors
- Works with Hyprland, Sway, i3, and other tiling WMs

[Back to overview](https://azul.rs/guide)
