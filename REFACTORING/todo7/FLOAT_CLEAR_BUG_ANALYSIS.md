# Float and Clear Layout Bug Analysis Report

## Executive Summary

Analysis of the `block-float-clear-complex-001.xht` test reveals critical bugs in the CSS float and clear implementation that violate CSS 2.2 § 9.5.2 specifications:

1. **Missing Elements (orange `.clear-left` and violet `.clear-right`)**: Elements with `clear` property are being positioned at (0, 0) instead of their correct layout positions
2. **Incorrect Clearance Calculation**: The clearance offset uses content-box instead of margin-box (outer edge) as required by spec
3. **Missing Code Flow**: After computing clearance, elements are not being positioned in the normal flow

## CSS 2.2 Specification Summary

### § 9.5.2 Clear Property Behavior

**Key Requirement**: "This property indicates which sides of an element's box(es) may not be adjacent to an earlier floating box."

**Values for non-floating block-level boxes:**
- `left`: Top border edge must be below the **bottom outer edge** of any left-floating boxes from earlier elements
- `right`: Top border edge must be below the **bottom outer edge** of any right-floating boxes from earlier elements  
- `both`: Top border edge must be below the **bottom outer edge** of any floats from earlier elements
- `none`: No constraint

**Critical Specification Points:**

1. **Outer Edge Requirement** (repeated 3 times in spec):
   > "Requires that the top border edge of the box be below the bottom **outer edge** of any left-floating boxes"
   
   The "outer edge" means the **margin box**, not content box or border box.

2. **Clearance Definition**:
   > "Clearance inhibits margin collapsing and acts as spacing above the margin-top of an element. It is used to push the element vertically past the float."

3. **Clearance Calculation Algorithm**:
   - First determine hypothetical position (as if clear=none)
   - If hypothetical position is not past relevant floats, introduce clearance
   - Set clearance to amount necessary to place border edge even with bottom **outer edge** of lowest float
   - Clearance can be negative or zero

4. **Margin Collapsing Inhibition**:
   > "Values other than 'none' potentially introduce clearance. Clearance inhibits margin collapsing"
   
   When clearance is introduced, margins no longer collapse with preceding siblings.

### § 9.5.1 Float Positioning Rules

**Rule 8**: "A floating box must be placed as high as possible"

**Outer Edge Rules**: Rules 1-3 explicitly state that float positioning is determined by "outer edge" (margin box), not content box

## Test Case Structure

### HTML Structure (block-float-clear-complex-001.xht)
```html
<div class="container">
    1. <div class="float-left"></div>      <!-- Pink, 200x150 + 10px margin -->
    2. <div class="float-right"></div>     <!-- Blue, 200x150 + 10px margin -->
    3. <div class="content"></div>         <!-- Green, 100px min-height + 15px padding + 10px margin -->
    4. <div class="float-left"></div>      <!-- Pink, 200x150 + 10px margin -->
    5. <div class="content"></div>         <!-- Green, 100px min-height + 15px padding + 10px margin -->
    6. <div class="clear-left"></div>      <!-- ORANGE, 60px height + 10px margin, clear: left -->
    7. <div class="float-right"></div>     <!-- Blue, 200x150 + 10px margin -->
    8. <div class="content"></div>         <!-- Green, 100px min-height + 15px padding + 10px margin -->
    9. <div class="clear-both"></div>      <!-- Green (darker), 80px height + 10px margin, clear: both -->
   10. <div class="float-left"></div>      <!-- Pink, 200x150 + 10px margin -->
   11. <div class="float-right"></div>     <!-- Blue, 200x150 + 10px margin -->
   12. <div class="clear-right"></div>     <!-- VIOLET, 60px height + 10px margin, clear: right -->
</div>
```

### CSS Classes
- `.float-left`: Pink (#ee0979), 200x150, margin 10px, float: left
- `.float-right`: Blue (#667eea), 200x150, margin 10px, float: right
- `.content`: Green (#00cf27), padding 15px, margin 10px 0, min-height 100px
- `.clear-left`: **Orange (#f59e0b)**, 60px height, margin 10px 0, clear: left
- `.clear-right`: **Violet (#8b5cf6)**, 60px height, margin 10px 0, clear: right
- `.clear-both`: Dark green (#10b981), 80px height, margin 10px 0, clear: both

## Current Output Analysis

### Display List (from `cargo run --example html_full`)
```
[2] Rect: bounds=840x640 @ (0, 0)          # Body (dark background)
[4] Rect: bounds=800x470 @ (20, 20)        # Container (white)
[6] Rect: bounds=760x130 @ (40, 50)        # Content 1 (green)
[8] Rect: bounds=760x130 @ (40, 190)       # Content 2 (green)
[10] Rect: bounds=760x60 @ (0, 0)          # clear-left (ORANGE) - WRONG POSITION!
[12] Rect: bounds=760x130 @ (40, 330)      # Content 3 (green)
[14] Rect: bounds=760x80 @ (0, 0)          # clear-both (green) - WRONG POSITION!
[16] Rect: bounds=760x60 @ (0, 0)          # clear-right (VIOLET) - WRONG POSITION!
[18] Rect: bounds=200x150 @ (50, 50)       # Float 1 (pink left)
[20] Rect: bounds=200x150 @ (590, 50)      # Float 2 (blue right)
[22] Rect: bounds=200x150 @ (260, 200)     # Float 3 (pink left)
[24] Rect: bounds=200x150 @ (590, 340)     # Float 4 (blue right)
[26] Rect: bounds=200x150 @ (50, 480)      # Float 5 (pink left)
[28] Rect: bounds=200x150 @ (380, 480)     # Float 6 (blue right)
```

### Layout Log Analysis
```
[layout_bfc] Child 8 margin from box_props: top=10, right=0, bottom=10, left=0
[layout_bfc] Positioning float: index=9, type=Right, size=200x150, at Y=290 (main_pen=280 + last_margin=10)
[layout_bfc] Child 10 margin from box_props: top=10, right=0, bottom=10, left=0
[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child=10, final_pos=(0, 290), main_pen=290, establishes_bfc=false
[layout_bfc] Child 11 margin from box_props: top=10, right=0, bottom=10, left=0
[layout_bfc] Positioning float: index=12, type=Left, size=200x150, at Y=430 (main_pen=420 + last_margin=10)
[layout_bfc] Positioning float: index=13, type=Right, size=200x150, at Y=430 (main_pen=420 + last_margin=10)
[layout_bfc] Child 14 margin from box_props: top=10, right=0, bottom=10, left=0
[apply_content_based_height] node=2: old_main=40.00 (Phase 1 with min-height), content=430.00, padding+border=40.00, new_content=470.00, final=470.00
```

**Key Observations:**
1. Child 8 (`.clear-left`, orange) is NOT logged in layout positioning
2. Child 11 (`.clear-both`, dark green) is NOT logged in layout positioning  
3. Child 14 (`.clear-right`, violet) is NOT logged in layout positioning
4. These elements have `bounds @ (0, 0)` in display list → they exist but have invalid positions
5. No clearance calculations are being logged for these elements

## Bug #1: Missing Position Calculation for Clear Elements

### Root Cause (Specification Violation)

Elements with the `clear` property are not being positioned in the layout flow, violating CSS 2.2 § 9.5.2 which requires that clear elements be positioned like normal block-level boxes, just with an adjusted vertical position.

**Current Code Flow** (in `fc.rs` around line 799):
```rust
// Check for clear property and apply clearance
let clear_val = get_clear_property(ctx.styled_dom, child_dom_id);
match clear_val {
    crate::solver3::getters::MultiValue::Exact(clear_val) if clear_val != LayoutClear::None => {
        let cleared_offset = float_context.clearance_offset(
            clear_val,
            main_pen,
            writing_mode,
        );
        if cleared_offset > main_pen {
            eprintln!("[layout_bfc] Applying clearance: ...");
            main_pen = cleared_offset;
            last_margin_bottom = 0.0;  // Clearance breaks margin collapse
        }
    }
    _ => {}
}
// ❌ MISSING: Continue to normal flow positioning after this point
```

**Problem**: After the match statement, the code needs to **continue** to the normal flow positioning logic. Currently, there appears to be an early return or skip that prevents clear elements from being positioned.

**Specification Requirement**: CSS 2.2 § 9.5.2 does NOT say clear elements are removed from flow or positioned differently. They are normal flow block-level boxes with clearance applied.

### Current Flow
```
1. Check if child is float → position float → continue (skip rest)
2. Check clear property → adjust main_pen → THEN WHAT?
3. ???
4. Element never gets positioned
```

### Expected Flow
```
1. Check if child is float → position float → continue (skip rest)
2. Check clear property → adjust main_pen
3. Calculate margin collapse with adjusted main_pen
4. Position element at final Y coordinate
5. Advance main_pen past element
6. Store last_margin_bottom
```

## Bug #2: Incorrect Float Clearance Calculation (Spec Violation)

### Root Cause: Using Content-Box Instead of Margin-Box

The `clearance_offset` function (line 227-252) violates CSS 2.2 § 9.5.2 by using **content-box bottom** instead of **margin-box bottom (outer edge)**.

**Current Implementation**:
```rust
pub fn clearance_offset(
    &self,
    clear: LayoutClear,
    current_main_offset: f32,
    wm: LayoutWritingMode,
) -> f32 {
    // ...
    for float in &self.floats {
        if should_clear_this_float {
            // ❌ SPEC VIOLATION: Uses content-box, not outer edge (margin-box)
            let float_main_end = float.rect.origin.main(wm) + float.rect.size.main(wm);
            max_end_offset = max_end_offset.max(float_main_end);
        }
    }
    // ...
}
```

**CSS 2.2 § 9.5.2 Requirement** (emphasis added):
> "Requires that the top border edge of the box be below the bottom **outer edge** of any left-floating boxes"

The specification uses "outer edge" **three times** in the clear property definition, making it unambiguous that the margin box must be used.

**What is "Outer Edge"?** CSS 2.2 § 8.1 defines:
- Content edge: Inner boundary of padding
- Padding edge: Outer boundary of padding, inner boundary of border  
- Border edge: Outer boundary of border, inner boundary of margin
- **Margin edge (outer edge)**: Outer boundary of margin

**Correct Calculation**:
```rust
// Use margin-box bottom (outer edge) per CSS 2.2 § 9.5.2
let float_margin_box_end = float.rect.origin.main(wm)   // top of content-box
                          + float.rect.size.main(wm)     // + height = bottom of content-box
                          + float.margin.main_end(wm);   // + bottom margin = outer edge
```

### Impact Example

For Float 2 (pink left) in our test case:
- Content-box: origin.y=150, size.height=150 → bottom at **300**
- Margin: bottom=10px
- Margin-box (outer edge): **310**

When child 6 (`.clear-left`) clears left floats:
- **Current (wrong)**: Clears to Y=300 (content-box)
- **Correct (spec)**: Should clear to Y=310 (margin-box/outer edge)
- **Difference**: 10px error per float with bottom margin

### Expected Behavior (from Chrome)

Looking at the Chrome rendering:

**Child 6 (`.clear-left`, orange):**
- Should appear BELOW Float 3 (pink left at Y=200, height=150 → ends at 350)
- With Float 3's bottom margin (10px): clear to Y=360
- Plus element's own top margin (10px): position at Y=370
- Current: positioned at (0, 0) ❌

**Child 11 (`.clear-both`, dark green):**
- Should appear BELOW both Float 4 (blue right) AND last content
- Float 4 is at Y=340, height=150, ends at 490 + margin 10 = 500
- Content 3 ends at Y=330 + 130 + margin 10 = 470
- Clear to max(500, 470) = 500
- Plus element's own top margin: position at Y=510
- Current: positioned at (0, 0) ❌

**Child 14 (`.clear-right`, violet):**
- Should appear BELOW Float 6 (blue right) and Float 5 (pink left)
- Float 5 at Y=480, height=150, margin=10 → margin-box ends at 640
- Float 6 at Y=480, height=150, margin=10 → margin-box ends at 640  
- But child has `clear: right` so only clears Float 6
- Clear to Y=640, plus margin: position at Y=650
- Current: positioned at (0, 0) ❌

## Architectural Improvements & Implementation Plan

### Phase 1: Fix clearance_offset (Immediate, Simple)

**Location**: `fc.rs` line 227-252

**Change**: Use margin-box (outer edge) instead of content-box

```rust
pub fn clearance_offset(
    &self,
    clear: LayoutClear,
    current_main_offset: f32,
    wm: LayoutWritingMode,
) -> f32 {
    let mut max_end_offset = 0.0_f32;

    let check_left = clear == LayoutClear::Left || clear == LayoutClear::Both;
    let check_right = clear == LayoutClear::Right || clear == LayoutClear::Both;

    for float in &self.floats {
        let should_clear_this_float = (check_left && float.kind == LayoutFloat::Left)
            || (check_right && float.kind == LayoutFloat::Right);

        if should_clear_this_float {
            // CSS 2.2 § 9.5.2: Use bottom outer edge (margin-box bottom)
            let float_margin_box_end = float.rect.origin.main(wm) 
                                     + float.rect.size.main(wm) 
                                     + float.margin.main_end(wm);
            max_end_offset = max_end_offset.max(float_margin_box_end);
        }
    }

    if max_end_offset > current_main_offset {
        max_end_offset
    } else {
        current_main_offset
    }
}
```

**Testing**: Verify that clear elements now clear to the correct Y position (margin-box bottom, not content-box bottom)

### Phase 2: Fix Normal Flow Positioning (Critical, Medium Complexity)

**Location**: `fc.rs` around line 660-900 (layout_bfc function)

**Problem**: The current code flow is approximately:

```
for each child:
    1. Skip if absolute/fixed positioned
    2. If float → position float, continue (skip to next child)
    3. Check clear property → adjust main_pen → ??? (unclear what happens next)
    4. Calculate margins, collapse
    5. Position element
    6. Advance main_pen
```

**Issue**: After step 3 (clear check), the code may not reach steps 4-6 for clear elements.

**Required Architecture**:

```rust
for &child_index in &node.children {
    // Skip absolutely positioned
    if position_type == absolute/fixed { continue; }
    
    // Handle floats separately (they're out of flow)
    if is_float {
        position_float(...);
        continue;  // ✅ Skip normal flow for floats
    }
    
    // All remaining elements are IN-FLOW (including those with clear)
    
    // Step 1: Check clear property and adjust main_pen
    let clear_val = get_clear_property(...);
    if clear_val != LayoutClear::None {
        let cleared_offset = float_context.clearance_offset(...);
        if cleared_offset > main_pen {
            // Clearance introduced - breaks margin collapse
            main_pen = cleared_offset;
            last_margin_bottom = 0.0;  // CSS 2.2 § 9.5.2
        }
    }
    
    // Step 2: Calculate margin collapse (with potentially adjusted main_pen)
    let margin_top = calculate_margin_with_collapse(...);
    
    // Step 3: Position element at main_pen + margin_top
    let final_y = main_pen + margin_top;
    output.positions.insert(child_index, LogicalPosition::from_main_cross(final_y, ...));
    
    // Step 4: Advance main_pen
    main_pen = final_y + child_size.main(wm);
    last_margin_bottom = child_margin.main_end(wm);
}
```

**Key Insight**: Clear elements are **normal flow block-level boxes**. The only difference is that clearance may be introduced before margin calculation, which:
1. Pushes main_pen down
2. Breaks margin collapsing with previous siblings

### Phase 3: Code Reorganization (Refactoring, Low Priority)

### Phase 3: Code Reorganization (Refactoring, Low Priority)

Consider extracting clear logic into a helper function:

```rust
/// Applies clearance for an element with the clear property set.
/// Returns the adjusted main_pen and whether margin collapse should be broken.
fn apply_clearance(
    clear: LayoutClear,
    current_main_pen: f32,
    float_context: &FloatingContext,
    wm: LayoutWritingMode,
) -> (f32, bool) {
    if clear == LayoutClear::None {
        return (current_main_pen, false);
    }
    
    let cleared_offset = float_context.clearance_offset(clear, current_main_pen, wm);
    
    if cleared_offset > current_main_pen {
        // Clearance introduced - CSS 2.2 § 9.5.2: breaks margin collapse
        (cleared_offset, true)
    } else {
        // No clearance needed
        (current_main_pen, false)
    }
}
```

Usage in main loop:
```rust
// Apply clearance if needed
let (main_pen, break_collapse) = apply_clearance(
    get_clear_property(...),
    main_pen,
    &float_context,
    writing_mode
);
if break_collapse {
    last_margin_bottom = 0.0;
}
```

### Phase 4: Comprehensive Testing

**Unit Tests** (in `fc.rs` or separate test file):

```rust
#[test]
fn test_clearance_uses_margin_box() {
    let mut fc = FloatingContext::new();
    
    // Add a float with margin
    fc.add_float(
        LayoutFloat::Left,
        LogicalRect::new(
            LogicalPosition::from_main_cross(10.0, 10.0, horizontal_tb),
            LogicalSize::from_main_cross(150.0, 200.0, horizontal_tb),
        ),
        EdgeSizes { top: 5.0, right: 5.0, bottom: 10.0, left: 5.0 }
    );
    
    // Clear left should return margin-box bottom, not content-box bottom
    // Content-box bottom: 10 + 150 = 160
    // Margin-box bottom: 160 + 10 = 170
    let result = fc.clearance_offset(LayoutClear::Left, 0.0, horizontal_tb);
    assert_eq!(result, 170.0, "Should use margin-box bottom (outer edge)");
}

#[test]
fn test_clear_both_picks_lowest_float() {
    let mut fc = FloatingContext::new();
    
    // Left float ends at Y=100
    fc.add_float(LayoutFloat::Left, LogicalRect::new(..., 100.0, ...), EdgeSizes::zero());
    
    // Right float ends at Y=150
    fc.add_float(LayoutFloat::Right, LogicalRect::new(..., 150.0, ...), EdgeSizes::zero());
    
    // Clear both should return the maximum (lowest in layout)
    let result = fc.clearance_offset(LayoutClear::Both, 0.0, horizontal_tb);
    assert_eq!(result, 150.0, "Should clear to lowest float");
}

#[test]
fn test_clear_element_positioned_in_flow() {
    // Integration test: Create a layout with clear element
    // Verify it appears in output.positions
    // Verify it has non-zero coordinates
    // Verify it's positioned after applying clearance
}
```

**Integration Tests** (using existing HTML test infrastructure):

1. **block-float-clear-complex-001.xht** (current failing test)
   - Verify orange `.clear-left` appears
   - Verify violet `.clear-right` appears
   - Verify dark green `.clear-both` appears
   - Check exact Y coordinates match Chrome/Firefox

2. **Simple clear tests** (from CSS 2.2 test suite):
   - `clear-001.xht`: Basic clear:left
   - `clear-002.xht`: Basic clear:right
   - `clear-003.xht`: Basic clear:both
   - `clear-float-004.xht` through `clear-float-008.xht`

3. **Edge cases**:
   - Clear with no preceding floats → should not affect position
   - Clear with float that already passed → should not affect position
   - Clear with negative margins
   - Clear combined with margin collapsing

## Current State Analysis (from Logs)

### What's Working ✅
- Float positioning respects last_margin_bottom (fixed in previous commit)
- Float margins don't collapse with in-flow elements
- Container height calculation doesn't expand for floats unnecessarily

### What's Broken ❌

**Evidence from logs**:
```
[layout_bfc] Child 8 margin from box_props: top=10, right=0, bottom=10, left=0
[layout_bfc] Positioning float: index=9, type=Right, size=200x150, at Y=290
[layout_bfc] Child 10 margin from box_props: top=10, right=0, bottom=10, left=0
[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child=10, final_pos=(0, 290), establishes_bfc=false
[layout_bfc] Child 11 margin from box_props: top=10, right=0, bottom=10, left=0
```

**Analysis**:
1. Child 8 (`.clear-left`) has margins logged but NO positioning log → **not positioned**
2. Float 9 positioned at Y=290
3. Child 10 (content) positioned at Y=290
4. Child 11 (`.clear-both`) has margins logged but NO "NORMAL FLOW BLOCK POSITIONED" log → **not positioned**

**Display List Evidence**:
```
[10] Rect: bounds=760x60 @ (0, 0)   # clear-left (orange) - WRONG!
[14] Rect: bounds=760x80 @ (0, 0)   # clear-both (green) - WRONG!
[16] Rect: bounds=760x60 @ (0, 0)   # clear-right (violet) - WRONG!
```

All three clear elements are at (0, 0) → they exist in display list but were never positioned.

## Expected Results After Fix

```
Display List (corrected):
[10] Rect: bounds=760x60 @ (40, 370)      # clear-left (ORANGE) - below pink float
[14] Rect: bounds=760x80 @ (40, 510)      # clear-both (green) - below all floats
[16] Rect: bounds=760x60 @ (40, 650)      # clear-right (VIOLET) - below blue floats
```

Container height should expand from 470px to ~720px to contain all cleared elements.

## Code Locations

- **Clear property check**: `fc.rs` line ~799
- **Clearance offset calculation**: `fc.rs` line 227-252  
- **Float positioning**: `fc.rs` line 680-710
- **Element positioning logic**: `fc.rs` line 720-900 (approximate)
- **Clear property getter**: `fc.rs` line 4017

## Priority

**HIGH** - This breaks basic CSS layout functionality and causes missing/mispositioned content in real-world HTML documents.

## References

- CSS 2.2 § 9.5.2: Controlling flow next to floats - the 'clear' property
- Test case: `/Users/fschutt/Development/printpdf/examples/assets/html/tests/block-float-clear-complex-001.xht`
- Log output: `/Users/fschutt/Development/printpdf/out.txt`
