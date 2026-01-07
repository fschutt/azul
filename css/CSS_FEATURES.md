# Azul CSS Features

This guide shows how to use Azul's CSS features for styling your application.

## Table of Contents

- [Language-Specific Styling](#language-specific-styling)
- [Media Queries](#media-queries)
- [Pseudo-States](#pseudo-states)
- [OS-Specific Styling](#os-specific-styling)
- [Theme-Specific Styling](#theme-specific-styling)

---

## Language-Specific Styling

Style elements differently based on the user's language using the `:lang()` pseudo-class.

### CSS Examples

```css
/* German typography */
p:lang(de) {
    font-family: "Fira Sans", sans-serif;
    hyphens: auto;
}

/* German quotation marks */
blockquote:lang(de) {
    quotes: "„" """ "‚" "'";
}

/* Japanese font stack */
body:lang(ja) {
    font-family: "Hiragino Kaku Gothic", "MS Gothic", sans-serif;
}

/* English defaults */
p:lang(en) {
    font-family: "Georgia", serif;
}

/* Right-to-left languages */
div:lang(ar) {
    direction: rtl;
    text-align: right;
}
```

### Rust API Examples

```rust
use azul::prelude::*;
use azul::css::Css;

fn build_ui(data: &mut AppData) -> Dom {
    // CSS with language-specific styles
    let css = Css::from_str(r#"
        .greeting:lang(de) { content: "Guten Tag"; }
        .greeting:lang(en) { content: "Hello"; }
        .greeting:lang(fr) { content: "Bonjour"; }
    "#);
    
    Dom::body()
        .with_child(
            Dom::div()
                .with_class("greeting")
        )
        .style(css)
}
```

### Language Detection

Azul automatically detects the system language:
- **macOS**: From system preferences
- **Windows**: From user locale settings  
- **Linux**: From `$LANG` environment variable

The detected language (e.g., `"de-DE"`, `"en-US"`) is used for `:lang()` matching.

### Language Tag Matching

| CSS Selector | Matches |
|--------------|---------|
| `:lang(de)` | `de`, `de-DE`, `de-AT`, `de-CH` |
| `:lang(de-DE)` | `de-DE`, `de-DE-formal` |
| `:lang(en)` | `en`, `en-US`, `en-GB`, `en-AU` |
| `:lang(zh-Hans)` | `zh-Hans`, `zh-Hans-CN` |

---

## Media Queries

Apply styles based on screen type.

### CSS Examples

```css
/* Screen-only styles */
@media screen {
    .sidebar {
        display: flex;
    }
}

/* Print styles */
@media print {
    .sidebar {
        display: none;
    }
    
    body {
        font-size: 12pt;
        color: black;
    }
}

/* Universal styles */
@media all {
    .container {
        max-width: 1200px;
    }
}
```

### Rust API Examples

```rust
use azul::prelude::*;
use azul::css::Css;

fn build_ui(data: &mut AppData) -> Dom {
    let css = Css::from_str(r#"
        @media screen {
            .content { background: white; }
        }
        
        @media print {
            .no-print { display: none; }
        }
    "#);
    
    Dom::body()
        .with_child(
            Dom::div()
                .with_class("content")
                .with_child(
                    Dom::div()
                        .with_class("no-print")
                        .with_text("This won't print")
                )
        )
        .style(css)
}
```

---

## Pseudo-States

Style elements based on user interaction.

### CSS Examples

```css
/* Hover effect */
button:hover {
    background-color: #0066cc;
}

/* Active/pressed state */
button:active {
    background-color: #004499;
}

/* Focused element */
input:focus {
    border-color: #0066cc;
    outline: 2px solid rgba(0, 102, 204, 0.3);
}

/* Disabled state */
button:disabled {
    background-color: #cccccc;
    cursor: not-allowed;
}

/* Combine with :lang() */
a:lang(de):hover {
    text-decoration: underline;
}
```

### Rust API Examples

```rust
use azul::prelude::*;
use azul::css::*;

fn build_button() -> Dom {
    Dom::div()
        .with_class("button")
        // Normal state
        .with_inline_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::simple(
                CssProperty::BackgroundColor(ColorU::rgb(240, 240, 240))
            ),
            CssPropertyWithConditions::simple(
                CssProperty::Cursor(StyleCursor::Pointer)
            ),
        ]))
        // Hover state
        .with_inline_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::on_hover(
                CssProperty::BackgroundColor(ColorU::rgb(220, 220, 220))
            ),
        ]))
        // Active state
        .with_inline_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::on_active(
                CssProperty::BackgroundColor(ColorU::rgb(200, 200, 200))
            ),
        ]))
}
```

### Using Helper Methods

```rust
use azul::prelude::*;
use azul::css::*;

fn styled_button(label: &str) -> Dom {
    Dom::div()
        .with_text(label)
        // Use convenience methods for common states
        .with_normal_css_property(CssProperty::BackgroundColor(ColorU::WHITE))
        .with_hover_css_property(CssProperty::BackgroundColor(ColorU::rgb(230, 230, 230)))
        .with_active_css_property(CssProperty::BackgroundColor(ColorU::rgb(200, 200, 200)))
        .with_focus_css_property(CssProperty::BorderColor(ColorU::BLUE))
}
```

---

## OS-Specific Styling

Apply different styles based on the operating system.

### Rust API Examples

```rust
use azul::prelude::*;
use azul::css::*;
use azul::css::dynamic_selector::*;

fn platform_button() -> Dom {
    Dom::div()
        .with_class("button")
        .with_inline_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            // macOS: Rounded corners, subtle shadow
            CssPropertyWithConditions::on_macos(
                CssProperty::BorderRadius(LayoutBorderRadius::px(6.0))
            ),
            CssPropertyWithConditions::on_macos(
                CssProperty::BoxShadow(/* macOS-style shadow */)
            ),
            
            // Windows: Square corners, accent color
            CssPropertyWithConditions::on_windows(
                CssProperty::BorderRadius(LayoutBorderRadius::px(0.0))
            ),
            
            // Linux: GNOME-style rounded corners
            CssPropertyWithConditions::on_linux(
                CssProperty::BorderRadius(LayoutBorderRadius::px(4.0))
            ),
        ]))
}
```

### Available OS Conditions

```rust
// Target specific platforms
CssPropertyWithConditions::on_windows(property)
CssPropertyWithConditions::on_macos(property)
CssPropertyWithConditions::on_linux(property)

// Or use the generic method
CssPropertyWithConditions::with_condition(
    property,
    DynamicSelector::Os(OsCondition::MacOS)
)
```

---

## Theme-Specific Styling

Style differently for light and dark themes.

### Rust API Examples

```rust
use azul::prelude::*;
use azul::css::*;
use azul::css::dynamic_selector::*;

fn themed_container() -> Dom {
    Dom::div()
        .with_class("container")
        .with_inline_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            // Light theme
            CssPropertyWithConditions::light_theme(
                CssProperty::BackgroundColor(ColorU::WHITE)
            ),
            CssPropertyWithConditions::light_theme(
                CssProperty::TextColor(ColorU::BLACK)
            ),
            
            // Dark theme
            CssPropertyWithConditions::dark_theme(
                CssProperty::BackgroundColor(ColorU::rgb(30, 30, 30))
            ),
            CssPropertyWithConditions::dark_theme(
                CssProperty::TextColor(ColorU::WHITE)
            ),
        ]))
}
```

---

## Combining Conditions

You can combine multiple conditions - all must match for the style to apply.

### Rust API Examples

```rust
use azul::prelude::*;
use azul::css::*;
use azul::css::dynamic_selector::*;

fn complex_button() -> Dom {
    Dom::div()
        .with_inline_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            // macOS + Dark theme + Hover
            CssPropertyWithConditions::with_conditions(
                CssProperty::BackgroundColor(ColorU::rgb(60, 60, 60)),
                DynamicSelectorVec::from_vec(vec![
                    DynamicSelector::Os(OsCondition::MacOS),
                    DynamicSelector::Theme(ThemeCondition::Dark),
                    DynamicSelector::PseudoState(PseudoStateType::Hover),
                ])
            ),
            
            // Windows + Light theme
            CssPropertyWithConditions::with_conditions(
                CssProperty::BackgroundColor(ColorU::rgb(0, 120, 212)),
                DynamicSelectorVec::from_vec(vec![
                    DynamicSelector::Os(OsCondition::Windows),
                    DynamicSelector::Theme(ThemeCondition::Light),
                ])
            ),
        ]))
}
```

---

## Quick Reference

### CssPropertyWithConditions Helper Methods

| Method | Description |
|--------|-------------|
| `simple(prop)` | Always applies |
| `on_hover(prop)` | Applies on mouse hover |
| `on_active(prop)` | Applies when pressed |
| `on_focus(prop)` | Applies when focused |
| `when_disabled(prop)` | Applies when disabled |
| `on_windows(prop)` | Windows only |
| `on_macos(prop)` | macOS only |
| `on_linux(prop)` | Linux only |
| `dark_theme(prop)` | Dark theme only |
| `light_theme(prop)` | Light theme only |
| `with_condition(prop, cond)` | Custom single condition |
| `with_conditions(prop, conds)` | Multiple conditions (AND) |

### Supported CSS Pseudo-Classes

| Pseudo-Class | Description |
|--------------|-------------|
| `:hover` | Mouse over element |
| `:active` | Element being clicked |
| `:focus` | Element has keyboard focus |
| `:disabled` | Element is disabled |
| `:checked` | Checkbox/radio is selected |
| `:first` | First child |
| `:last` | Last child |
| `:nth-child(n)` | Nth child element |
| `:lang(tag)` | Matches language tag |
