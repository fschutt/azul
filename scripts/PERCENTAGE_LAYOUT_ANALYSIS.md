# Percentage Values in Azul Layout Solver - Analysis Report

## Executive Summary

The Azul layout solver uses **Taffy** as its underlying flexbox/grid engine. Taffy **fully supports percentages** via `Dimension::percent()` and `LengthPercentage::percent()`. However, the current codebase has several issues preventing percentages from working correctly:

1. **ProgressBar widget uses a hack** instead of proper percentages
2. **Some code paths return `None` for `SizeMetric::Percent`** instead of passing through to Taffy
3. **Inconsistent handling** between `taffy_bridge.rs` (correct) and `sizing.rs` (has issues)

## Current Architecture

### CSS Value Types

```
SizeMetric (css/src/props/basic/length.rs):
├── Px, Pt, Em, Rem, In, Cm, Mm  (absolute units)
├── Percent                       (percentage)
└── Vw, Vh, Vmin, Vmax           (viewport units)

PixelValue (css/src/props/basic/pixel.rs):
├── metric: SizeMetric
└── number: FloatValue
└── to_percent() -> Option<NormalizedPercentage>  ← Extracts % as 0.0-1.0

LayoutWidth / LayoutHeight (css/src/props/layout/dimensions.rs):
├── Auto
├── Px(PixelValue)  ← Can contain SizeMetric::Percent!
├── MinContent
└── MaxContent
```

### Layout Solvers

There are **two paths** for layout:

1. **Taffy Bridge** (`layout/src/solver3/taffy_bridge.rs`)
   - Used for flexbox and grid layouts
   - **Correctly handles percentages** via `from_layout_width()` / `from_layout_height()`
   
2. **Sizing Module** (`layout/src/solver3/sizing.rs`)
   - Used for block-level sizing calculations  
   - **Has issues with percentage handling** in some code paths

## Files Affected

### 1. `layout/src/solver3/taffy_bridge.rs` ✅ CORRECT

```rust
fn from_layout_width(val: LayoutWidth) -> Dimension {
    match val {
        LayoutWidth::Auto => Dimension::auto(),
        LayoutWidth::Px(px) => {
            match pixel_value_to_pixels_fallback(&px) {
                Some(pixels) => Dimension::length(pixels),
                None => match px.to_percent() {
                    Some(p) => Dimension::percent(p.get()),  // ✅ Correct!
                    None => Dimension::auto(),
                },
            }
        }
        ...
    }
}
```

This correctly converts `width: 50%` to `Dimension::percent(0.5)` for Taffy.

### 2. `layout/src/solver3/sizing.rs` ⚠️ HAS ISSUES

Lines 815, 863, 988, 1026, 1088, 1126:
```rust
SizeMetric::Percent => None,  // Returns None first...
...
None => match px.to_percent() {  // ...then handles it here
    Some(p) => resolve_percentage_with_box_model(...)
}
```

This is **redundant** - it first returns `None` for `Percent`, then immediately handles `to_percent()`. The code works but is confusing. The issue is that when `to_percent()` returns `None` (for vw/vh/etc), it falls back to intrinsic sizing.

### 3. `layout/src/widgets/progressbar.rs` ⚠️ USES HACK

```rust
// NOTE: This is a hack, but a quite effective one:
// since the layout solver doesn't support percentages in relation to the parent,
// this widget uses the flex-grow property to achieve the same effect

let flex_grow_bar = 10000000.0 / 100.0 * percent_done;       // 25% = 2,500,000
let flex_grow_remaining = 10000000.0 / 100.0 * (100.0 - percent_done);  // 75% = 7,500,000
```

**Problem**: This comment is outdated! The layout solver now supports percentages. The hack is also potentially problematic because:
- Large flex-grow values (10,000,000) may cause floating point precision issues
- It relies on flex behavior which may not work in all contexts

## Proposed Solution

### Phase 1: Fix ProgressBar Widget

Replace the flex-grow hack with proper percentage widths:

```rust
pub fn dom(self) -> Dom {
    let percent_done = self.progressbar_state.percent_done.max(0.0).min(100.0);
    
    Dom::create_div()
        .with_css_props(/* container styles */)
        .with_children(DomVec::from_vec(vec![
            // Green bar - use width: XX%
            Dom::create_div()
                .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                    CssPropertyWithConditions::simple(CssProperty::Width(
                        LayoutWidthValue::Exact(LayoutWidth::Px(
                            PixelValue::percent(percent_done)
                        )),
                    )),
                    // ... other styles
                ])),
            // Remaining space - use width: (100-XX)% or just let it fill
        ]))
}
```

### Phase 2: Clean Up Sizing Module (Optional)

The `sizing.rs` code is functional but could be cleaner:

```rust
// Before (current):
let pixels_opt = match px.metric {
    SizeMetric::Percent => None,  // Redundant
    ...
};
match pixels_opt {
    None => match px.to_percent() { ... }  // Handles Percent here
}

// After (cleaner):
let pixels_opt = match px.metric {
    SizeMetric::Percent => {
        // Handle percentage directly here
        let p = NormalizedPercentage::from_unnormalized(px.number.get());
        return resolve_percentage_with_box_model(...);
    }
    ...
};
```

### Phase 3: Add Viewport Units Support (Future)

Currently `Vw, Vh, Vmin, Vmax` return `None`. These could be resolved if viewport size is passed to the layout context.

## Test Cases

After fixing, verify:

1. `ProgressBar::create(0.0).dom()` → green bar has 0 width
2. `ProgressBar::create(50.0).dom()` → green bar has 50% width  
3. `ProgressBar::create(100.0).dom()` → green bar has 100% width
4. Parent resize → progress bar resizes proportionally

## Conclusion

The **hack in ProgressBar is unnecessary** because Taffy fully supports percentages. The fix is straightforward:

1. Change ProgressBar to use `PixelValue::percent(x)` instead of flex-grow hack
2. Verify Taffy receives `Dimension::percent(x)` correctly
3. Test that the visual output is correct

The `sizing.rs` code works correctly despite the confusing structure - it's just not as clean as it could be.
