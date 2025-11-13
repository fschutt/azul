# CSS Property Default Values - Architecture Analysis & Design Document

**Date:** 2025-11-13  
**Author:** GitHub Copilot (Analysis)  
**Status:** CRITICAL DESIGN FLAW IDENTIFIED

## Executive Summary

The current implementation has a critical architectural flaw in how CSS properties with "unset" or "auto" values are handled. The use of `.unwrap_or(T::default())` throughout the codebase conflates **"property not set"** with **"property explicitly set to its zero/default value"**, making it impossible to distinguish between:

1. `height: auto` (not set → should use content height)
2. `height: 0px` (explicitly set → should be 0px)

This has cascading effects causing:
- H1 elements to overlay P tags (no block spacing)
- Elements with auto-sizing getting 0px dimensions
- Percentage resolution failures
- Inability to implement proper CSS cascading

---

## Current Problems

### 1. **Immediate Issue: H1 Overlays P Tag**

**Test Case:**
```html
<h1 style="font-size: 32px; font-weight: bold;">Hello, World!</h1>
<p>مرحبا بالعالم - Arabic text</p>
```

**Expected:** H1 above P, with spacing  
**Actual:** H1 renders on top of P at same Y position

**Root Cause:**
- Both H1 and P get rendered at `y=580.04456` 
- No block layout spacing applied
- Height calculations don't account for previous siblings

### 2. **Width/Height Auto-Sizing Bug**

**Before Fix:**
```rust
get_css_property!(get_css_height, get_height, LayoutHeight, LayoutHeight::default());
// LayoutHeight::default() = Px(PixelValue::zero()) = 0px
```

If CSS doesn't specify `height`, `.unwrap_or(default())` returns `Px(0px)` instead of "use content height".

**Current "Fix" (WRONG):**
```rust
get_css_property!(get_css_height, get_height, LayoutHeight, LayoutHeight::MaxContent);
```

**Problems with Current Fix:**
1. Treats ALL unset heights as `max-content` (flow layout)
2. Breaks explicit `height: 0px` (can't distinguish from unset)
3. Doesn't respect CSS initial values per spec
4. Block elements now size incorrectly

### 3. **Font Size Not Applied**

Test shows `SetFontSize: size=Pt(16.0)` for both H1 and P, despite CSS having `font-size: 32px` for H1.

**Likely Cause:**
```rust
// azul/layout/src/solver3/getters.rs:463
let font_size = cache
    .get_font_size(node_data, &dom_id, node_state)
    .and_then(|v| v.get_property().cloned())
    .map(|v| v.inner.to_pixels(16.0))
    .unwrap_or(16.0);  // <-- Always 16.0 if not found!
```

The font-size from the CSS `<style>` block in the `<head>` is not being applied, only inline styles work.

---

## Architectural Analysis

### Root Cause: Type System Design Flaw

CSS properties in browsers have multiple "states":

| State | CSS Keyword | Meaning |
|-------|------------|---------|
| Unset | (none) | Use inherited or initial value |
| Initial | `initial` | Use CSS spec default |
| Inherit | `inherit` | Use parent's computed value |
| Auto | `auto` | Algorithm-specific (width: fit-content, height: content-based, margins: 0) |
| Explicit | `10px`, `50%` | User-specified value |

**Current Azul Design:**
```rust
pub enum LayoutHeight {
    Px(PixelValue),     // Can be px, %, em, etc.
    MinContent,
    MaxContent,
}

impl Default for LayoutHeight {
    fn default() -> Self {
        LayoutHeight::Px(PixelValue::zero())  // WRONG: 0px ≠ unset!
    }
}
```

**Problem:** No way to represent "unset" or "auto" state!

### Impact on get_css_property! Macro

**Current Macro:**
```rust
macro_rules! get_css_property {
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $default:expr) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> $return_type {
            styled_dom
                .css_property_cache
                .ptr
                .$cache_method(...)
                .and_then(|v| v.get_property().copied())
                .unwrap_or($default)  // <-- LOSES "not set" information!
        }
    };
}
```

**All Current Uses:**

| Property | Current Default | Correct Behavior |
|----------|----------------|------------------|
| `width` | ~~`Px(0)`~~ → `MaxContent` (temp fix) | Should be `auto` (varies by display type) |
| `height` | ~~`Px(0)`~~ → `MaxContent` (temp fix) | Should be `auto` (content-based for blocks) |
| `writing_mode` | `HorizontalTb` | ✅ OK (has proper default) |
| `flex_wrap` | `NoWrap` | ✅ OK |
| `justify_content` | `Start` | ✅ OK |
| `text_align` | `Start` | ✅ OK |
| `float` | `None` | ✅ OK |
| `overflow_x` | `Visible` | ✅ OK |
| `overflow_y` | `Visible` | ✅ OK |
| `position` | `Static` | ✅ OK |

**Status:** Only `width` and `height` are broken because they're the only ones where "unset" has different semantics than the zero value.

### Other .unwrap_or_default() Uses

**In `taffy_bridge.rs`** (lines 352-472):
```rust
// PROBLEM: Loses distinction between "not set" and "0px"
.and_then(|p| {
    if let CssProperty::MinWidth(v) = p {
        Some(v.get_property_or_default().unwrap_or_default().inner)
    } else {
        None
    }
})
```

**In `getters.rs`** (line 467):
```rust
// PROBLEM: font-size always defaults to 16.0, ignoring CSS rules
let font_size = cache
    .get_font_size(node_data, &dom_id, node_state)
    .and_then(|v| v.get_property().cloned())
    .map(|v| v.inner.to_pixels(16.0))
    .unwrap_or(16.0);  // <-- WRONG!
```

Should check:
1. Inline style
2. CSS rules from `<style>` block
3. User-agent stylesheet
4. Inherited value (for font-size)
5. Initial value (16px for root)

**In `getters.rs`** (line 189+):
```rust
// Multiple uses with border-radius
.unwrap_or_default();  // OK for border-radius (0 = no radius)
```

---

## Design Solutions

### Solution 1: Add "Auto" Variant (RECOMMENDED)

**Changes Required:**

**1. Extend LayoutWidth/LayoutHeight:**
```rust
pub enum LayoutWidth {
    Auto,              // NEW: Represents unset/auto
    Px(PixelValue),
    MinContent,
    MaxContent,
}

impl Default for LayoutWidth {
    fn default() -> Self {
        LayoutWidth::Auto  // FIXED: Now distinct from 0px
    }
}

pub enum LayoutHeight {
    Auto,              // NEW
    Px(PixelValue),
    MinContent,
    MaxContent,
}

impl Default for LayoutHeight {
    fn default() -> Self {
        LayoutHeight::Auto  // FIXED
    }
}
```

**2. Update Sizing Logic:**
```rust
// azul/layout/src/solver3/sizing.rs
pub fn calculate_used_size_for_node(...) -> Result<LogicalSize> {
    let resolved_width = match css_width {
        LayoutWidth::Auto => {
            // Use algorithm based on display type:
            // - block: containing block width
            // - inline: max-content
            // - flex-item: based on flex-grow/flex-basis
            match display {
                LayoutDisplay::Block => containing_block_size.width,
                LayoutDisplay::Inline => intrinsic.max_content_width,
                // ...
            }
        }
        LayoutWidth::Px(px) => {
            match px.to_pixels_no_percent() {
                Some(pixels) => pixels,
                None => match px.to_percent() {
                    Some(p) => p.resolve(containing_block_size.width),
                    None => intrinsic.max_content_width,
                },
            }
        }
        LayoutWidth::MinContent => intrinsic.min_content_width,
        LayoutWidth::MaxContent => intrinsic.max_content_width,
    };

    let resolved_height = match css_height {
        LayoutHeight::Auto => {
            // For blocks: use content height (calculated after layout)
            // For replaced elements: intrinsic height
            // Will be updated in apply_content_based_height()
            intrinsic.max_content_height
        }
        LayoutHeight::Px(px) => {
            match px.to_pixels_no_percent() {
                Some(pixels) => pixels,
                None => match px.to_percent() {
                    Some(p) => p.resolve(containing_block_size.height),
                    None => intrinsic.max_content_height,
                },
            }
        }
        LayoutHeight::MinContent => intrinsic.min_content_height,
        LayoutHeight::MaxContent => intrinsic.max_content_height,
    };
    
    // ...
}
```

**3. Update get_css_property! Macro:**
```rust
get_css_property!(
    get_css_width,
    get_width,
    LayoutWidth,
    LayoutWidth::Auto  // Now semantically correct!
);
get_css_property!(
    get_css_height,
    get_height,
    LayoutHeight,
    LayoutHeight::Auto  // Now semantically correct!
);
```

**4. Update Taffy Bridge:**
```rust
fn from_layout_width(val: LayoutWidth) -> Dimension {
    match val {
        LayoutWidth::Auto => Dimension::auto(),  // NEW
        LayoutWidth::Px(px) => {
            match px.to_pixels_no_percent() {
                Some(pixels) => Dimension::length(pixels),
                None => match px.to_percent() {
                    Some(p) => Dimension::percent(p.get()),
                    None => Dimension::auto(),
                },
            }
        }
        LayoutWidth::MinContent | LayoutWidth::MaxContent => Dimension::auto(),
    }
}
```

**Pros:**
- ✅ Semantically correct
- ✅ Distinguishes "not set" from "0px"
- ✅ Enables proper percentage resolution
- ✅ Matches CSS spec behavior
- ✅ Enables proper cascading

**Cons:**
- ❌ Requires updating all match arms (breaking change)
- ❌ Needs careful migration of existing code

**Estimated Work:** 2-3 days
- Day 1: Add variants, update core types
- Day 2: Update sizing.rs, taffy_bridge.rs
- Day 3: Update tests, fix regressions

---

### Solution 2: Use Option<LayoutWidth> (ALTERNATIVE)

**Changes Required:**

```rust
// Change getter signature
pub fn get_css_width(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<LayoutWidth> {  // Returns None if not set
    styled_dom
        .css_property_cache
        .ptr
        .get_width(...)
        .and_then(|v| v.get_property().copied())
    // No .unwrap_or()!
}

// Update sizing logic
pub fn calculate_used_size_for_node(...) -> Result<LogicalSize> {
    let css_width = get_css_width(styled_dom, id, node_state);
    
    let resolved_width = match css_width {
        None => {
            // Not set -> use auto behavior
            match display {
                LayoutDisplay::Block => containing_block_size.width,
                _ => intrinsic.max_content_width,
            }
        }
        Some(LayoutWidth::Px(px)) => {
            // Explicitly set
            // ...
        }
        // ...
    };
}
```

**Pros:**
- ✅ Distinguishes "not set" from "set to value"
- ✅ Rust idiomatic (Option for optional values)
- ✅ No need to extend enums

**Cons:**
- ❌ Still can't distinguish "auto" keyword from "not set"
- ❌ Requires updating all call sites (100+ locations)
- ❌ More verbose

**Estimated Work:** 3-4 days

---

### Solution 3: Full CssPropertyValue Integration (COMPREHENSIVE)

The CSS module already has a proper `CssPropertyValue` type:

```rust
pub enum CssPropertyValue<T> {
    None,
    Initial,
    Inherit,
    Auto,
    Exact(T),
}
```

**Changes Required:**

1. Stop unwrapping in getters - return `Option<&CssPropertyValue<LayoutWidth>>`
2. Handle all cases at call sites
3. Implement proper inheritance for applicable properties
4. Implement proper initial value resolution per CSS spec

**Pros:**
- ✅ Fully CSS-compliant
- ✅ Enables proper inheritance
- ✅ Enables proper `auto`/`initial`/`inherit` keywords
- ✅ Future-proof

**Cons:**
- ❌ Massive refactoring (4-6 weeks)
- ❌ Touches every file in layout engine
- ❌ High risk of regressions

---

## Recommended Approach

**Phase 1: Quick Fix (1 day) - FOR CURRENT RELEASE**

Revert the `MaxContent` changes and use a more targeted fix:

```rust
pub fn get_css_width_for_used_size(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
    display: LayoutDisplay,
) -> LayoutWidth {
    match styled_dom
        .css_property_cache
        .ptr
        .get_width(...)
        .and_then(|v| v.get_property().copied())
    {
        Some(w) => w,
        None => {
            // Not set -> use auto behavior based on display type
            match display {
                LayoutDisplay::Block => LayoutWidth::MaxContent, // Will be constrained by container
                LayoutDisplay::Inline => LayoutWidth::MaxContent,
                _ => LayoutWidth::MaxContent,
            }
        }
    }
}
```

**Phase 2: Proper Fix (2-3 weeks) - NEXT SPRINT**

Implement Solution 1 (Add Auto Variant) with:
1. Add `Auto` variant to `LayoutWidth` and `LayoutHeight`
2. Update `Default` implementations
3. Update all `match` statements in `sizing.rs`
4. Update `taffy_bridge.rs` conversions
5. Add regression tests
6. Update documentation

**Phase 3: Full Solution (Future)**

Plan migration to Solution 3 (Full CssPropertyValue Integration) for next major version.

---

## Test Cases Needed

### 1. Auto-Sizing
```html
<div style="width: auto; height: auto;">Should fit content</div>
```

### 2. Explicit Zero
```html
<div style="width: 0px; height: 0px;">Should be invisible</div>
```

### 3. Percentage Resolution
```html
<div style="width: 100%; height: 50%;">Should be 100% × 50% of parent</div>
```

### 4. Block Layout Spacing
```html
<h1>Header</h1>
<p>Should be below header, not overlapping</p>
```

### 5. Font Inheritance
```html
<style>
  h1 { font-size: 32px; }
</style>
<h1>Should be 32px</h1>
<p>Should be 16px (default)</p>
```

---

## Block Layout Issue (Separate Problem)

The H1/P overlay issue is NOT just about height calculation - it's about **block formatting context**:

**Expected Block Layout:**
```
+-------------------+
| H1: y=0           |
| height=40px       |
+-------------------+
| P: y=40           |  <-- Should start AFTER H1
| height=20px       |
+-------------------+
```

**Current Broken Layout:**
```
+-------------------+
| H1: y=580         |
+-------------------+
| P: y=580          |  <-- WRONG: Same Y!
+-------------------+
```

**Root Cause:** Block layout pass doesn't calculate cumulative Y offsets for siblings.

**Fix Needed in:** `azul/layout/src/solver3/cache.rs` - `layout_formatting_context()`

Must accumulate Y positions:
```rust
let mut current_y = 0.0;
for child in block_children {
    position_child_at_y(child, current_y);
    current_y += child.height + child.margin_bottom + next_child.margin_top;
}
```

---

## Font-Size Issue

**Problem:** `<style>` CSS rules not applied, only inline styles work.

**Investigation Needed:**
1. Check `CssPropertyCache::restyle()` - is it parsing `<style>` blocks?
2. Check CSS specificity calculation
3. Check if font-size is in `css_normal_props` map

**Likely Fix:** Ensure `<style>` block CSS is passed to `restyle()` during `str_to_dom()`.

---

## Priority Ranking

| Issue | Priority | Estimated Fix Time | Impact |
|-------|----------|-------------------|--------|
| H1/P overlap (block layout) | **P0** | 1 day | Blocks all multi-element layouts |
| Width/Height auto-sizing | **P0** | 3 days (Solution 1) | Affects all auto-sized elements |
| Font-size not applied | **P1** | 1 day | Affects typography |
| Default font (Helvetica → Times) | **P2** | 1 day | Cosmetic issue |
| Arabic font fallback | **P2** | 2 days | Affects non-Latin text |

---

## Conclusion

The current `.unwrap_or(T::default())` pattern is fundamentally incompatible with CSS semantics. We need to:

1. **Immediately:** Fix block layout Y positioning (P0)
2. **Short-term:** Add `Auto` variant to `LayoutWidth`/`LayoutHeight` (P0)
3. **Medium-term:** Fix font-size cascade from `<style>` blocks (P1)
4. **Long-term:** Migrate to full `CssPropertyValue` integration

Without these fixes, the HTML-to-PDF feature will remain broken for all but the simplest layouts.

