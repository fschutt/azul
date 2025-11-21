# CSS `padding-inline-start` Research & Implementation Plan

**Date**: 2025-11-21  
**Specification**: CSS Logical Properties and Values Level 1  
**Related Specs**: CSS Lists Module Level 3, CSS Writing Modes Level 4

---

## Executive Summary

`padding-inline-start` ist eine **logische Property**, die je nach `writing-mode`, `direction` und `text-orientation` des Elements auf unterschiedliche physikalische Properties mappt. Sie ist KEINE Alias für `padding-left`.

**Critical Insight**: In Browser-UA-Stylesheets wird `padding-inline-start: 40px` auf `<ul>`/`<ol>` **Container** angewendet, NICHT auf `<li>` Items. Dies schafft den "Gutter-Space" für `::marker` Pseudo-Elemente bei `list-style-position: outside`.

---

## 1. Property Specification

### 1.1 Formal Definition

| Aspect | Value |
|--------|-------|
| **Name** | `padding-inline-start` |
| **Value** | `<length>` \| `<percentage>` |
| **Initial** | `0` |
| **Applies to** | All elements |
| **Inherited** | No |
| **Percentages** | Relative to inline size (width in horizontal-tb) of containing block |
| **Computed Value** | As `<length>` (percentages resolved) |
| **Animation Type** | By computed value (length) |

### 1.2 Mapping Logic

Das Mapping hängt vom **Element selbst** ab (nicht vom Parent):

```
writing-mode: horizontal-tb + direction: ltr
    → padding-inline-start = padding-left

writing-mode: horizontal-tb + direction: rtl
    → padding-inline-start = padding-right

writing-mode: vertical-rl
    → padding-inline-start = padding-top

writing-mode: vertical-lr
    → padding-inline-start = padding-bottom
```

**Key Point**: `text-orientation` beeinflusst das Mapping nur bei vertikalen writing-modes in Kombination mit mixed/upright Orientierung.

---

## 2. CSS Lists Module Integration

### 2.1 Browser UA Stylesheet (Chrome/Firefox/Safari)

```css
/* Spec: CSS Lists Module Level 3, Appendix A */
ol, ul {
  display: block;
  margin-block: 1em;
  padding-inline-start: 40px;  /* ← Hier auf Container! */
}

li {
  display: list-item;
  text-align: match-parent;
  /* KEIN padding-inline-start hier! */
}
```

### 2.2 Warum auf Container, nicht auf `<li>`?

**Problem 1: Marker-Positionierung bei `list-style-position: outside`**

```
┌─── <ul> mit padding-inline-start: 40px ─────────────────┐
│                                                          │
│  [::marker "1."]  ← Im 40px Gutter   <li> Content       │
│                      (outside)                           │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

Der `::marker` wird **außerhalb** der `<li>` principal box positioniert, aber **innerhalb** des Container-Paddings. Dies ist nur möglich, wenn das Padding auf dem Container ist.

**Problem 2: Text Available Width**

Wenn `padding-inline-start: 40px` auf `<li>` wäre:
- Available width für Text = Container-Width - 40px (links) - 40px (rechts)
- Text würde zu früh umbrechen (wie im Bug beschrieben: "is", "an" passen nicht)

Mit Padding auf Container:
- Available width für `<li>` Content = Container-Width - 40px (nur von einer Seite)
- Korrekte Textbreite

**Problem 3: Nested Lists**

```html
<ul>
  <li>Level 1
    <ul>
      <li>Level 2
        <ul>
          <li>Level 3</li>
        </ul>
      </li>
    </ul>
  </li>
</ul>
```

Mit Padding auf `<li>`: Jede Ebene addiert 40px, wird unkontrollierbar  
Mit Padding auf `<ul>`: Jeder Container schafft seinen eigenen Gutter, sauber geschachtelt

---

## 3. Box Model & Available Width Calculation

### 3.1 Padding wirkt nur in eine Richtung

**Kritischer Unterschied zu `padding-left`**:

Wenn `padding-left: 40px` gesetzt ist (physikalisch):
- Wird vom linken UND rechten Rand der available width abgezogen (typisches CSS Box Model)
- Available width = Container Width - 40px - 40px

Wenn `padding-inline-start: 40px` gesetzt ist (logisch):
- Wird NUR von der inline-start Seite abgezogen
- Available width = Container Width - 40px
- Die inline-end Seite bleibt unberührt

**Warum?** 
Logische Properties sind unidirektional. `padding-inline-start` und `padding-inline-end` sind separate Properties.

### 3.2 Percentage Calculation

```css
ul {
  width: 500px;
  padding-inline-start: 10%;  /* 10% of 500px = 50px */
}
```

Die Prozentbasis ist die **inline size** des containing blocks:
- In `horizontal-tb`: width
- In `vertical-rl/lr`: height

---

## 4. Text Orientation Interaction

### 4.1 Vertical Writing Modes

```css
/* Beispiel 1: Japanischer Vertikaltext */
.vertical-ja {
  writing-mode: vertical-rl;      /* Rechts nach links, top-to-bottom */
  text-orientation: upright;       /* Zeichen aufrecht */
  padding-inline-start: 20px;      /* = padding-top */
}

/* Beispiel 2: Mongolischer Vertikaltext */
.vertical-mn {
  writing-mode: vertical-lr;       /* Links nach rechts, top-to-bottom */
  text-orientation: sideways;      /* Zeichen gedreht */
  padding-inline-start: 20px;      /* = padding-bottom */
}
```

### 4.2 Mixed Orientation

Bei `text-orientation: mixed` (Standard bei vertical modes):
- Lateinische Buchstaben werden gedreht (sideways)
- CJK Zeichen bleiben aufrecht
- Das Mapping von `padding-inline-start` bleibt gleich (basiert auf writing-mode, nicht text-orientation)

**Wichtig**: `text-orientation` beeinflusst **Glyph-Rendering**, nicht Property-Mapping.

---

## 5. Implementation Architecture

### 5.1 Data Flow

```
CSS Parser (props/layout/spacing.rs)
    ↓
LayoutPaddingInlineStart struct with PixelValue
    ↓
RectStyle in styled_dom.rs
    ↓
FC Solver (solver3/fc.rs)
    ↓
Map to Physical Property based on:
    - writing_mode (from parent or element)
    - direction (from element)
    - text_orientation (from element, for vertical modes)
    ↓
PhysicalPadding { left, right, top, bottom }
    ↓
Box Layout Calculation
```

### 5.2 Mapping Function Design

```rust
pub fn resolve_logical_padding(
    logical: &LogicalPadding,
    writing_mode: WritingMode,
    direction: Direction,
    text_orientation: TextOrientation,
) -> PhysicalPadding {
    use WritingMode::*;
    use Direction::*;
    use TextOrientation::*;
    
    match writing_mode {
        HorizontalTb => {
            PhysicalPadding {
                left: if direction == Ltr { logical.inline_start } else { logical.inline_end },
                right: if direction == Ltr { logical.inline_end } else { logical.inline_start },
                top: logical.block_start,
                bottom: logical.block_end,
            }
        }
        VerticalRl => {
            PhysicalPadding {
                top: logical.inline_start,
                bottom: logical.inline_end,
                right: logical.block_start,
                left: logical.block_end,
            }
        }
        VerticalLr => {
            PhysicalPadding {
                bottom: logical.inline_start,
                top: logical.inline_end,
                left: logical.block_start,
                right: logical.block_end,
            }
        }
        SidewaysRl => {
            // Special handling for sideways modes
            PhysicalPadding {
                top: logical.inline_start,
                bottom: logical.inline_end,
                right: logical.block_start,
                left: logical.block_end,
            }
        }
        SidewaysLr => {
            PhysicalPadding {
                bottom: logical.inline_start,
                top: logical.inline_end,
                left: logical.block_start,
                right: logical.block_end,
            }
        }
    }
}
```

---

## 6. UA Stylesheet Implementation

### 6.1 Required Constants

```rust
// azul/core/src/ua_css.rs

/// padding-inline-start: 40px for list containers
/// Creates gutter space for ::marker pseudo-elements
static PADDING_INLINE_START_40PX: CssProperty = CssProperty::PaddingInlineStart(
    CssPropertyValue::Exact(LayoutPaddingInlineStart {
        inner: PixelValue::const_px(40),
    }),
);

/// padding-inline-end: 40px for symmetry (if needed)
static PADDING_INLINE_END_40PX: CssProperty = CssProperty::PaddingInlineEnd(
    CssPropertyValue::Exact(LayoutPaddingInlineEnd {
        inner: PixelValue::const_px(40),
    }),
);
```

### 6.2 Property Assignment

```rust
match (node_type, property_type) {
    // Apply to list containers, NOT to list items
    (NT::Ul, PT::PaddingInlineStart) => Some(&PADDING_INLINE_START_40PX),
    (NT::Ol, PT::PaddingInlineStart) => Some(&PADDING_INLINE_START_40PX),
    
    // Remove any old padding-left assignments on ul/ol
    // (NT::Ul, PT::PaddingLeft) => None,  // Don't apply physical padding
    // (NT::Ol, PT::PaddingLeft) => None,
    
    // List items get NO padding from UA stylesheet
    // Their padding is inherited from the container's content box
    
    // ... rest of matches
}
```

---

## 7. Parser Implementation

### 7.1 Property Enum Extension

```rust
// azul/css/src/props/property.rs (code-generated)

pub enum CssProperty {
    // ... existing properties
    PaddingInlineStart(CssPropertyValue<LayoutPaddingInlineStart>),
    PaddingInlineEnd(CssPropertyValue<LayoutPaddingInlineEnd>),
    // ...
}

pub enum CssPropertyType {
    // ... existing types
    PaddingInlineStart,
    PaddingInlineEnd,
    // ...
}
```

### 7.2 Parser Functions

```rust
// azul/css/src/props/layout/spacing.rs

impl LayoutPaddingInlineStart {
    pub fn parse(input: &str) -> Result<Self, CssParseError> {
        let pixel_value = PixelValue::parse(input)?;
        Ok(LayoutPaddingInlineStart {
            inner: pixel_value,
        })
    }
}

impl LayoutPaddingInlineEnd {
    pub fn parse(input: &str) -> Result<Self, CssParseError> {
        let pixel_value = PixelValue::parse(input)?;
        Ok(LayoutPaddingInlineEnd {
            inner: pixel_value,
        })
    }
}
```

### 7.3 CSS String Parsing

```rust
// In CSS parser (css_parser.rs)

"padding-inline-start" => {
    let value = parse_property_value(value_str)?;
    CssProperty::PaddingInlineStart(value)
}

"padding-inline-end" => {
    let value = parse_property_value(value_str)?;
    CssProperty::PaddingInlineEnd(value)
}
```

---

## 8. RectStyle Integration

### 8.1 Current Structure

```rust
// azul/azul-css/src/rect_style.rs (circa line 30-70)

pub struct RectStyle {
    // Physical properties (existing)
    pub padding_left: Option<LayoutPaddingLeft>,
    pub padding_right: Option<LayoutPaddingRight>,
    pub padding_top: Option<LayoutPaddingTop>,
    pub padding_bottom: Option<LayoutPaddingBottom>,
    
    // Border properties...
    // Margin properties...
}
```

### 8.2 Extended Structure

```rust
pub struct RectStyle {
    // Physical properties
    pub padding_left: Option<LayoutPaddingLeft>,
    pub padding_right: Option<LayoutPaddingRight>,
    pub padding_top: Option<LayoutPaddingTop>,
    pub padding_bottom: Option<LayoutPaddingBottom>,
    
    // Logical properties (NEW)
    pub padding_inline_start: Option<LayoutPaddingInlineStart>,
    pub padding_inline_end: Option<LayoutPaddingInlineEnd>,
    pub padding_block_start: Option<LayoutPaddingBlockStart>,
    pub padding_block_end: Option<LayoutPaddingBlockEnd>,
    
    // ... borders, margins
}
```

### 8.3 Resolution Strategy

Two approaches:

**Option A: Eager Resolution (in FC Solver)**
- Resolve logical → physical at start of layout
- Store only physical values
- Simpler layout code

**Option B: Lazy Resolution (during layout)**
- Keep logical values separate
- Resolve when computing final box dimensions
- More flexible for dynamic writing-mode changes

**Recommendation**: Option A (Eager) for initial implementation.

---

## 9. FC Solver Changes

### 9.1 Padding Resolution Function

```rust
// azul/layout/src/solver3/fc.rs

pub fn resolve_padding(
    rect_style: &RectStyle,
    writing_mode: WritingMode,
    direction: Direction,
) -> ResolvedPadding {
    let mut resolved = ResolvedPadding {
        left: rect_style.padding_left.map(|p| p.inner).unwrap_or(PixelValue::zero()),
        right: rect_style.padding_right.map(|p| p.inner).unwrap_or(PixelValue::zero()),
        top: rect_style.padding_top.map(|p| p.inner).unwrap_or(PixelValue::zero()),
        bottom: rect_style.padding_bottom.map(|p| p.inner).unwrap_or(PixelValue::zero()),
    };
    
    // Apply logical properties (they override physical if both set)
    if let Some(inline_start) = rect_style.padding_inline_start {
        match (writing_mode, direction) {
            (WritingMode::HorizontalTb, Direction::Ltr) => {
                resolved.left = inline_start.inner;
            }
            (WritingMode::HorizontalTb, Direction::Rtl) => {
                resolved.right = inline_start.inner;
            }
            (WritingMode::VerticalRl, _) => {
                resolved.top = inline_start.inner;
            }
            (WritingMode::VerticalLr, _) => {
                resolved.bottom = inline_start.inner;
            }
            // ... other modes
        }
    }
    
    // Similar for inline_end, block_start, block_end
    
    resolved
}
```

### 9.2 Integration Point

```rust
// In compute_flex_item_size() or similar

let writing_mode = node.style.writing_mode.unwrap_or(WritingMode::HorizontalTb);
let direction = node.style.direction.unwrap_or(Direction::Ltr);

let padding = resolve_padding(&node.style, writing_mode, direction);

let content_box_width = border_box_width 
    - padding.left.to_pixels(parent_width)
    - padding.right.to_pixels(parent_width);
```

---

## 10. Testing Strategy

### 10.1 Unit Tests

```rust
#[test]
fn test_padding_inline_start_ltr() {
    let style = RectStyle {
        padding_inline_start: Some(LayoutPaddingInlineStart {
            inner: PixelValue::const_px(40),
        }),
        ..Default::default()
    };
    
    let resolved = resolve_padding(&style, WritingMode::HorizontalTb, Direction::Ltr);
    
    assert_eq!(resolved.left.to_pixels(0.0), 40.0);
    assert_eq!(resolved.right.to_pixels(0.0), 0.0);
}

#[test]
fn test_padding_inline_start_rtl() {
    let style = RectStyle {
        padding_inline_start: Some(LayoutPaddingInlineStart {
            inner: PixelValue::const_px(40),
        }),
        ..Default::default()
    };
    
    let resolved = resolve_padding(&style, WritingMode::HorizontalTb, Direction::Rtl);
    
    assert_eq!(resolved.left.to_pixels(0.0), 0.0);
    assert_eq!(resolved.right.to_pixels(0.0), 40.0);
}

#[test]
fn test_padding_inline_start_vertical() {
    let style = RectStyle {
        padding_inline_start: Some(LayoutPaddingInlineStart {
            inner: PixelValue::const_px(40),
        }),
        ..Default::default()
    };
    
    let resolved = resolve_padding(&style, WritingMode::VerticalRl, Direction::Ltr);
    
    assert_eq!(resolved.top.to_pixels(0.0), 40.0);
    assert_eq!(resolved.bottom.to_pixels(0.0), 0.0);
}
```

### 10.2 Integration Tests

```html
<!-- Test 1: Basic list with LTR -->
<ul style="writing-mode: horizontal-tb; direction: ltr;">
  <li>Item 1</li>
  <li>Item 2</li>
</ul>

<!-- Test 2: RTL list -->
<ul style="direction: rtl;">
  <li>عنصر 1</li>
  <li>عنصر 2</li>
</ul>

<!-- Test 3: Vertical list -->
<ul style="writing-mode: vertical-rl;">
  <li>項目 1</li>
  <li>項目 2</li>
</ul>

<!-- Test 4: Explicit logical padding -->
<div style="padding-inline-start: 50px; writing-mode: horizontal-tb;">
  Content
</div>
```

### 10.3 Visual Regression Tests

Compare rendered PDFs:
- List marker positioning
- Text wrapping behavior
- Nested list indentation
- Mixed writing modes

---

## 11. Cascade & Specificity

### 11.1 Logical vs Physical Precedence

Per CSS Cascade Spec:
- Both logical AND physical properties participate in cascade
- **Order matters**: Last declaration wins
- No automatic conversion at parse time

```css
ul {
  padding-left: 20px;          /* Physical */
  padding-inline-start: 40px;  /* Logical - wins due to order */
}
```

Result in LTR: `padding-left: 40px`

```css
ul {
  padding-inline-start: 40px;  /* Logical */
  padding-left: 20px;          /* Physical - wins due to order */
}
```

Result in LTR: `padding-left: 20px`

### 11.2 Implementation in RectStyle

```rust
impl RectStyle {
    pub fn merge(&mut self, other: &RectStyle) {
        // Later properties override earlier ones
        // Both logical and physical participate
        
        if other.padding_left.is_some() {
            self.padding_left = other.padding_left;
        }
        if other.padding_inline_start.is_some() {
            self.padding_inline_start = other.padding_inline_start;
        }
        
        // During resolution, check declaration order
        // (requires tracking which was declared last)
    }
}
```

---

## 12. Performance Considerations

### 12.1 Caching Resolved Values

```rust
pub struct ResolvedBoxStyle {
    // Cached physical values
    padding: PhysicalPadding,
    margin: PhysicalMargin,
    border: PhysicalBorder,
    
    // Original logical values (for invalidation)
    logical_padding: Option<LogicalPadding>,
    
    // Context used for resolution
    writing_mode: WritingMode,
    direction: Direction,
}

impl ResolvedBoxStyle {
    pub fn invalidate_if_changed(
        &mut self,
        new_writing_mode: WritingMode,
        new_direction: Direction,
    ) -> bool {
        if self.writing_mode != new_writing_mode || self.direction != new_direction {
            // Re-resolve logical properties
            if let Some(logical) = &self.logical_padding {
                self.padding = resolve_logical_padding(
                    logical,
                    new_writing_mode,
                    new_direction,
                );
            }
            self.writing_mode = new_writing_mode;
            self.direction = new_direction;
            true
        } else {
            false
        }
    }
}
```

### 12.2 Avoiding Re-resolution

- Resolve once at start of layout pass
- Cache in layout tree node
- Only re-resolve if `writing-mode` or `direction` changes dynamically

---

## 13. Edge Cases & Gotchas

### 13.1 Percentage Values

```css
ul {
  width: 500px;
  padding-inline-start: 10%;  /* 10% of WHAT? */
}
```

**Answer**: 10% of the **inline size** of the containing block.
- In `horizontal-tb`: 10% of width
- In `vertical-rl`: 10% of height

**Implementation**: Percentage resolution must know writing-mode.

### 13.2 Inheritance

Logical properties do **NOT** inherit.

```html
<div style="padding-inline-start: 40px;">
  <p><!-- Does NOT inherit padding --></p>
</div>
```

### 13.3 Shorthand Properties

```css
padding: 10px 20px;  /* Sets physical properties */
padding-inline: 30px; /* Shorthand for inline-start and inline-end */
```

**Question**: What if both are set?

```css
ul {
  padding: 10px;                    /* All sides = 10px */
  padding-inline-start: 40px;       /* inline-start = 40px */
}
```

Result: `padding-inline-start` wins for that side (cascade order).

### 13.4 Zero Values

```css
padding-inline-start: 0;  /* Valid, removes padding */
```

Must be handled correctly (different from `None`/unset).

---

## 14. Migration Path

### Phase 1: Parser & Types ✅ (To be implemented)
- Add `LayoutPaddingInlineStart` / `End` to spacing.rs
- Add enum variants to CssProperty
- Implement parse() methods
- Add to RectStyle

### Phase 2: UA Stylesheet 🔄 (Modify existing)
- Change `PADDING_INLINE_START_40PX` from using `PaddingLeft` to proper property
- Keep on `<ul>` and `<ol>`, NOT on `<li>`

### Phase 3: Resolution Logic ✅ (New code)
- Implement `resolve_padding()` in fc.rs
- Map logical → physical based on writing-mode
- Handle cascade precedence

### Phase 4: Integration ✅ (Modify existing)
- Use resolved padding in box sizing calculations
- Update available width calculations
- Ensure text layout receives correct widths

### Phase 5: Testing 🧪
- Unit tests for all writing-mode combinations
- Integration tests with HTML examples
- Visual regression tests
- PDF output verification

---

## 15. Success Criteria

### Functional Requirements
- ✅ `padding-inline-start` parses correctly
- ✅ Maps to correct physical property based on writing-mode
- ✅ UA stylesheet applies to `<ul>`/`<ol>`, not `<li>`
- ✅ List markers positioned correctly
- ✅ Text wrapping respects padding
- ✅ Nested lists indent properly

### Performance Requirements
- ❌ No measurable slowdown in layout (< 5% regression)
- ❌ Resolved values cached appropriately

### Compatibility Requirements
- ✅ Existing physical properties still work
- ✅ Cascade order respected (logical vs physical)
- ✅ Percentage values resolve correctly

---

## 16. References

### Specifications
- [CSS Logical Properties Level 1](https://www.w3.org/TR/css-logical-1/)
- [CSS Lists Module Level 3](https://www.w3.org/TR/css-lists-3/)
- [CSS Writing Modes Level 4](https://www.w3.org/TR/css-writing-modes-4/)
- [CSS Box Model Level 4](https://drafts.csswg.org/css-box-4/)

### Browser Implementations
- Chrome/Blink: `third_party/blink/renderer/core/css/resolver/style_adjuster.cc`
- Firefox/Gecko: `layout/style/nsComputedDOMStyle.cpp`
- Safari/WebKit: `Source/WebCore/css/CSSComputedStyleDeclaration.cpp`

### Related Issues
- [CSSWG Issue 3029](https://github.com/w3c/csswg-drafts/issues/3029): Inheritance of logical properties
- [CSSWG Issue 3030](https://github.com/w3c/csswg-drafts/issues/3030): Shorthand expansion

---

## Conclusion

`padding-inline-start` ist eine essentielle Property für zukunftssichere, mehrsprachige Layouts. Die korrekte Implementierung erfordert:

1. **Separates Property** (nicht Alias für padding-left)
2. **Anwendung auf List-Container** (`<ul>`/`<ol>`), nicht Items (`<li>`)
3. **Writing-Mode-Bewusstsein** (mapping basiert auf Element-Context)
4. **Korrekte Available-Width-Berechnung** (nur von einer Seite abziehen)

Die vorgeschlagene Implementierung folgt den CSS-Specs und Browser-Verhalten, während sie gut in die bestehende Azul-Architektur integriert wird.
