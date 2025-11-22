# Float/Clear Bug Fix Implementation Plan

## Step-by-Step Implementation Guide

### STEP 1: Fix clearance_offset (5 minutes, LOW RISK)

**File**: `azul/layout/src/solver3/fc.rs`
**Line**: ~245

**Current Code**:
```rust
if should_clear_this_float {
    let float_main_end = float.rect.origin.main(wm) + float.rect.size.main(wm);
    max_end_offset = max_end_offset.max(float_main_end);
}
```

**New Code**:
```rust
if should_clear_this_float {
    // CSS 2.2 § 9.5.2: Use bottom outer edge (margin-box)
    let float_margin_box_end = float.rect.origin.main(wm) 
                             + float.rect.size.main(wm) 
                             + float.margin.main_end(wm);
    max_end_offset = max_end_offset.max(float_margin_box_end);
}
```

**Test**: Compile and run. Verify no crashes. Elements still won't appear (that's Step 2).

---

### STEP 2: Identify Missing Code Path (10 minutes, INVESTIGATION)

**File**: `azul/layout/src/solver3/fc.rs`
**Function**: `layout_bfc`
**Line**: ~660-900

**Action**: Find where the code flow breaks for clear elements.

**Search Strategy**:
```bash
cd /Users/fschutt/Development/azul/layout/src/solver3
grep -n "get_clear_property" fc.rs
grep -n "NORMAL FLOW BLOCK POSITIONED" fc.rs
```

**Hypothesis**: After clear check (line ~799), there's likely:
1. A `continue` statement that skips to next child, OR
2. A conditional that only runs positioning for non-clear elements, OR  
3. The clear check is inside a block that exits early

**What to Look For**:
```rust
// Pattern 1: Early continue
if clear_val != LayoutClear::None {
    // apply clearance
    continue;  // ❌ BAD - skips positioning
}

// Pattern 2: Incorrect scoping
if something {
    // clear check here
} else {
    // positioning code here  ❌ BAD - clear elements never reach this
}

// Pattern 3: Missing positioning code
if clear_val != LayoutClear::None {
    // apply clearance
}
// ❌ Code just ends here, nothing positions the element
```

---

### STEP 3: Fix Code Flow (30 minutes, MEDIUM RISK)

**Goal**: Ensure clear elements continue to normal flow positioning after clearance.

**Template**:
```rust
// Around line 799 in layout_bfc
for &child_index in &node.children {
    // ... absolute/fixed check ...
    
    // Float handling (out of flow)
    if is_float {
        // ... float positioning code ...
        continue;  // ✅ CORRECT - floats skip normal flow
    }
    
    // --- ALL CODE BELOW THIS LINE RUNS FOR IN-FLOW ELEMENTS ---
    // This includes elements with clear property
    
    // Track first/last child for margin collapse
    if first_child_index.is_none() {
        first_child_index = Some(child_index);
    }
    last_child_index = Some(child_index);
    
    // Get child properties
    let child_size = child_node.used_size.unwrap_or_default();
    let child_margin = &child_node.box_props.margin;
    
    // STEP A: Apply clearance if needed
    let clear_val = get_clear_property(ctx.styled_dom, child_dom_id);
    let clearance_breaks_collapse = match clear_val {
        crate::solver3::getters::MultiValue::Exact(clear_val) 
            if clear_val != LayoutClear::None => {
            
            let cleared_offset = float_context.clearance_offset(
                clear_val,
                main_pen,
                writing_mode,
            );
            
            if cleared_offset > main_pen {
                eprintln!(
                    "[layout_bfc] Applying clearance: child={}, clear={:?}, \
                     old_pen={}, new_pen={}",
                    child_index, clear_val, main_pen, cleared_offset
                );
                main_pen = cleared_offset;
                true  // Clearance introduced - breaks collapse
            } else {
                false  // No clearance needed
            }
        }
        _ => false
    };
    
    // STEP B: Calculate margin collapse
    let (margin_top, margin_bottom) = if clearance_breaks_collapse {
        // CSS 2.2 § 9.5.2: Clearance inhibits margin collapsing
        (child_margin.main_start(wm), child_margin.main_end(wm))
    } else {
        // Normal margin collapse logic
        let collapsed_top = collapse_with_previous(
            child_margin.main_start(wm),
            last_margin_bottom,
            // ... other params ...
        );
        (collapsed_top, child_margin.main_end(wm))
    };
    
    // STEP C: Position element
    let final_y = main_pen + margin_top;
    let final_pos = LogicalPosition::from_main_cross(
        final_y,
        cross_start + child_margin.cross_start(writing_mode),
        writing_mode
    );
    
    output.positions.insert(child_index, final_pos);
    
    eprintln!(
        "[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child={}, \
         final_pos={:?}, main_pen={}, clear={:?}",
        child_index, final_pos, main_pen, clear_val
    );
    
    // STEP D: Advance main_pen
    main_pen = final_y + child_size.main(writing_mode);
    last_margin_bottom = margin_bottom;
    
    if clearance_breaks_collapse {
        // After introducing clearance, reset for next sibling
        last_margin_bottom = 0.0;
    }
}
```

**Key Changes**:
1. ✅ Clear check happens WITHIN the normal flow loop, not before it
2. ✅ After clear check, code continues to positioning
3. ✅ Clearance only affects whether margins collapse
4. ✅ All in-flow elements follow the same positioning path

---

### STEP 4: Handle Edge Cases (15 minutes)

**Case 1: Clear with First Child**
```rust
// If element is first child and has clear, there's no previous margin to collapse
if first_child_index == Some(child_index) {
    // First child - no previous margin to collapse anyway
    // But clearance can still push it down past floats
}
```

**Case 2: Clear with Parent Padding**
```rust
// Container has padding → blocks margin escape
// Clear element's margin doesn't collapse with container
// This should work automatically if clearance_breaks_collapse is handled correctly
```

**Case 3: Multiple Clear Elements in Sequence**
```rust
// Element 1: clear:left → positioned at Y1
// Element 2: clear:left → positioned at Y2
// Y2 should be based on Element 1's position, not repeat float clearance
// This should work automatically with last_margin_bottom tracking
```

---

### STEP 5: Testing & Validation (30 minutes)

**Compile & Run**:
```bash
cd /Users/fschutt/Development/printpdf
cargo build --example html_full
cargo run --example html_full 2>&1 | tee /tmp/output.txt
```

**Check Logs**:
```bash
# Should see positioning logs for ALL children, including clear elements
grep "NORMAL FLOW BLOCK POSITIONED" /tmp/output.txt

# Should see clearance application
grep "Applying clearance" /tmp/output.txt

# Check final display list
grep "Rect: bounds=" /tmp/output.txt | head -20
```

**Expected Output**:
```
[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child=8, final_pos=(0, 370), clear=Exact(Left)
[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child=11, final_pos=(0, 510), clear=Exact(Both)
[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child=14, final_pos=(0, 650), clear=Exact(Right)
```

**Display List**:
```
[10] Rect: bounds=760x60 @ (40, 370)   # clear-left (orange) - NOT @ (0,0)!
[14] Rect: bounds=760x80 @ (40, 510)   # clear-both (green)
[16] Rect: bounds=760x60 @ (40, 650)   # clear-right (violet)
```

---

### STEP 6: Visual Verification (5 minutes)

```bash
cd /Users/fschutt/Development/printpdf
open html_full_test.pdf
```

**Check**:
- ✅ Orange box appears below pink float
- ✅ Dark green box appears below all floats
- ✅ Violet box appears at bottom
- ✅ No elements at (0, 0)
- ✅ Container height includes all clear elements

**Compare** with Chrome rendering (user provided screenshot)

---

## Rollback Plan

If Step 3 causes crashes or incorrect layout:

1. **Revert** the code changes to Step 3
2. **Keep** Step 1 (clearance_offset fix) - it's safe
3. **Re-analyze** the code flow with more detailed logging
4. **Ask user** to provide more context about the code structure

## Success Criteria

✅ All three clear elements appear in PDF (not at 0,0)
✅ Orange box Y > 300 (below float 2 margin-box)
✅ Green box appears below both floats
✅ Violet box appears at bottom
✅ No compilation errors
✅ No layout regressions on other tests

## Estimated Time

- Step 1: 5 min
- Step 2: 10 min  
- Step 3: 30 min
- Step 4: 15 min
- Step 5: 30 min
- Step 6: 5 min

**Total**: ~1.5 hours

## Next Steps After Fix

1. Add unit tests for clearance_offset
2. Add integration tests for clear property
3. Run CSS 2.2 test suite clear tests
4. Document the fix in CHANGELOG
5. Consider refactoring for clarity
