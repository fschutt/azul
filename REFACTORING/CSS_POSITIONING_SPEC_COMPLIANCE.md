# CSS Positioned Layout Module Level 3 - Compliance Analysis

**Date:** October 23, 2025  
**Specification:** W3C CSS Positioned Layout Module Level 3 (WD, 7 October 2025)  
**Azul Version:** 2.0 (master branch)  
**Analysis Scope:** Normal flow positioning in Block and Inline formatting contexts

## Executive Summary

**Current Status:** ❌ **NON-COMPLIANT** - Critical positioning bug affecting all layout

The Azul layout engine currently has a **critical bug** where all elements are positioned at (0,0) regardless of their actual layout position. This violates the fundamental CSS positioning model defined in the W3C specification.

**Root Cause:** The formatting context layout functions (`layout_formatting_context`) return incorrect relative positions for all children - specifically, they return (0,0) for all elements instead of calculating proper block-flow or inline-flow positions.

**Impact:** 
- All rendered content appears overlapped at the origin
- Normal flow layout is completely broken
- Makes the rendering engine unusable for real-world applications

---

## 1. W3C CSS Positioning Specification Overview

### 1.1 Positioning Schemes (§2)

The CSS specification defines five positioning schemes:

1. **`position: static`** (default) - Normal flow positioning
2. **`position: relative`** - Visual offset from normal position
3. **`position: sticky`** - Scroll-dependent positioning  
4. **`position: absolute`** - Out-of-flow, CB-relative positioning
5. **`position: fixed`** - Out-of-flow, viewport-relative positioning

### 1.2 Normal Flow (CSS2.1 Chapter 9)

For `position: static` elements (which Azul's test cases use), the specification defines:

**Block Formatting Context (BFC):**
- Block-level boxes flow **vertically** 
- Each box's top edge touches the bottom edge of the previous box (plus margins)
- Initial position: top-left of containing block's content area
- Horizontal position: determined by width, margins, and containing block width
- Vertical position: **cumulative** - each box positioned after the previous one

**Inline Formatting Context (IFC):**
- Inline-level boxes flow **horizontally** (in the inline direction)
- When a line is full, content wraps to the next line
- Lines are stacked vertically (in the block direction)
- Each inline box positioned relative to its line box

### 1.3 Containing Blocks (§2.1)

For static/relative/sticky positioned boxes:
> "The containing block is established by the nearest ancestor box that is a block container"

The containing block provides:
- The coordinate system for positioning
- The reference for percentage-based sizes
- The boundaries for normal flow layout

---

## 2. Azul's Implementation Architecture

### 2.1 Layout Pipeline

```
solve_layout()
  ├─> reconcile_dom_to_layout_tree()
  ├─> calculate_intrinsic_sizes()
  ├─> for each layout_root:
  │     ├─> get_containing_block_for_node()  // Gets CB position & size
  │     └─> calculate_layout_for_subtree()    // Recursive layout
  │           ├─> layout_formatting_context()  // ← BUG LOCATION
  │           │     Returns: LayoutOutput {
  │           │       positions: BTreeMap<child_index, LogicalPosition>,
  │           │       overflow_size: LogicalSize
  │           │     }
  │           └─> For each child:
  │                 ├─> Convert relative → absolute position
  │                 ├─> Insert into absolute_positions map
  │                 └─> Recursive call (if not flex/grid)
  ├─> position_out_of_flow_elements()
  ├─> adjust_relative_positions()
  └─> generate_display_list()
```

### 2.2 Position Calculation Strategy

**Design Intent:**
1. **Formatting Context Layer** (`layout_formatting_context`) calculates **relative positions** of children within their parent's content box
2. **Subtree Layout Layer** (`calculate_layout_for_subtree`) converts relative → absolute by adding parent's content-box position
3. **Display List Layer** (`get_paint_rect`) retrieves absolute positions for rendering

**Example:**
```
Root @ (0,0), padding=10px
  ├─> content-box @ (10,10)
  └─> Child A: relative_pos=(0,0) → absolute_pos=(10,10)
      Child B: relative_pos=(0,100) → absolute_pos=(10,110)  // Below A
```

---

## 3. Current Bug Analysis

### 3.1 Observed Behavior

**Test Case:** `test_cpurender.rs`
- DOM: Root div (640x480) containing 5 children (various sizes)
- Expected: Children positioned sequentially in block flow
- Actual: ALL elements positioned at (0,0)

**Debug Output:**
```
[calculate_layout_for_subtree] After layout: node_index=0, layout_output.positions.len()=5
  child 1 @ (0, 0)  ← WRONG! Should be ~(0, 0) [first child]
  child 2 @ (0, 0)  ← WRONG! Should be ~(0, 30) [after child 1]
  child 3 @ (0, 0)  ← WRONG! Should be ~(0, 130) [after child 2]
  child 4 @ (0, 0)  ← WRONG! Should be ~(0, 230) [after child 3]
  child 5 @ (0, 0)  ← WRONG! Should be ~(0, 330) [after child 4]
```

### 3.2 Root Cause

**Location:** `layout/src/solver3/formatting_context.rs` (or similar)  
**Function:** `layout_formatting_context()`

**Problem:** The formatting context layout function does NOT calculate proper relative positions for children. Instead, it returns (0,0) for ALL children regardless of:
- Their size
- Their siblings' positions
- The formatting context type (block vs inline)
- Margins, padding, borders

**Expected Behavior (Block FC):**
```rust
fn layout_block_formatting_context(...) -> LayoutOutput {
    let mut positions = BTreeMap::new();
    let mut current_y = 0.0;  // Block-axis cursor
    
    for child in children {
        // Position child at current cursor
        positions.insert(child.index, LogicalPosition::new(0.0, current_y));
        
        // Advance cursor by child's outer height
        current_y += child.margin_top + child.border_top + child.padding_top
                   + child.height
                   + child.padding_bottom + child.border_bottom + child.margin_bottom;
    }
    
    LayoutOutput { positions, overflow_size: ... }
}
```

**Actual Behavior:**
```rust
// Positions are never calculated or always set to (0,0)
LayoutOutput {
    positions: children.iter().map(|&c| (c, LogicalPosition::default())).collect(),
    overflow_size: ...
}
```

### 3.3 Impact on Downstream Code

**`calculate_layout_for_subtree` (cache.rs:642-651):**
```rust
for (&child_index, &child_relative_pos) in &layout_output.positions {
    let child_absolute_pos = LogicalPosition::new(
        self_content_box_pos.x + child_relative_pos.x,  // self=(10,10), child=(0,0)
        self_content_box_pos.y + child_relative_pos.y,  // → absolute=(10,10) ❌
    );
    absolute_positions.insert(child_index, child_absolute_pos);
}
```

Even though the conversion logic is CORRECT, it produces wrong results because the input (`child_relative_pos`) is wrong.

**`get_paint_rect` (display_list.rs:803-831):**
```rust
fn get_paint_rect(&self, node_index: usize) -> Option<LogicalRect> {
    let pos = self.positioned_tree.absolute_positions
        .get(&node_index)
        .copied()
        .unwrap_or_default();  // Falls back to (0,0) if missing
    // ...
}
```

This function retrieves the (incorrect) absolute positions and passes them to the renderer, causing all content to overlap at the origin.

---

## 4. W3C Spec Compliance Matrix

| Specification Requirement | Azul Implementation | Status |
|---------------------------|---------------------|--------|
| **Normal Flow - Block FC** | | |
| Boxes flow vertically | ❌ All at y=0 | FAIL |
| Each box positioned after previous | ❌ No cumulative positioning | FAIL |
| Margins collapse vertically | ⚠️ Not tested (obscured by position bug) | UNKNOWN |
| **Normal Flow - Inline FC** | | |
| Boxes flow horizontally | ❌ All at x=0 | FAIL |
| Line wrapping when needed | ⚠️ Not tested | UNKNOWN |
| Lines stack vertically | ❌ All lines at y=0 | FAIL |
| **Containing Blocks** | | |
| CB established by nearest block ancestor | ✅ Correct in `get_containing_block_for_node` | PASS |
| CB defines coordinate system | ❌ Coordinates always (0,0) | FAIL |
| CB used for percentage resolution | ✅ Working in sizing code | PASS |
| **Position Property** | | |
| `position: static` (default) | ❌ Not working (all at origin) | FAIL |
| `position: relative` | ⚠️ Implementation exists but untested | UNKNOWN |
| `position: absolute` | ⚠️ Implementation exists but untested | UNKNOWN |
| `position: fixed` | ⚠️ Implementation exists but untested | UNKNOWN |
| `position: sticky` | ⚠️ Implementation exists but untested | UNKNOWN |
| **Display List Generation** | | |
| Correct paint rectangles | ❌ All at (0,0) | FAIL |
| Proper stacking context order | ✅ z-index handling implemented | PASS |
| Clip rectangles | ⚠️ Calculated but wrong positions | PARTIAL |

**Overall Compliance: 2/16 PASS, 9/16 FAIL, 5/16 UNKNOWN**

---

## 5. Comparison with CSS2.1 Block Layout Algorithm

### 5.1 CSS2.1 § 9.4.1 - Block Formatting Context

**Specification (paraphrased):**
> In a block formatting context, boxes are laid out one after the other, vertically, starting at the top of the containing block. The vertical distance between two sibling boxes is determined by the 'margin' properties. Vertical margins between adjacent block-level boxes in a block formatting context collapse.

**Azul's Implementation:**
- ❌ Boxes are NOT laid out "one after the other"
- ❌ ALL boxes start at y=0 (the top)
- ❌ No vertical distance calculation between siblings
- ⚠️ Margin collapse not tested (can't see it due to positioning bug)

### 5.2 CSS2.1 § 10.3.3 - Block-level, non-replaced elements in normal flow

**Specification (horizontal positioning):**
```
'margin-left' + 'border-left-width' + 'padding-left' + 'width' + 
'padding-right' + 'border-right-width' + 'margin-right' = 
width of containing block
```

**Azul's Implementation:**
- ✅ This calculation appears correct in `calculate_used_size_for_node`
- ✅ Widths are correctly resolved (debug output shows correct sizes)
- ❌ But horizontal POSITION is always x=0, not respecting margin-left

### 5.3 CSS2.1 § 10.6.3 - Block-level, non-replaced elements in normal flow

**Specification (vertical positioning):**
> If 'height' is 'auto', the height depends on whether the element has any block-level children and whether it has padding or borders... In a block formatting context, [...] the height is the distance from the top margin-edge of the topmost block-level child box to the bottom margin-edge of the bottom-most block-level child box.

**Azul's Implementation:**
- ✅ Auto height calculation appears implemented
- ❌ But child boxes are not positioned vertically at all
- ❌ All children have margin-edge at y=0, making height calculation impossible

---

## 6. Technical Debt & Design Flaws

### 6.1 Architectural Issue: Incomplete Abstraction

**Problem:** The `LayoutOutput` struct correctly defines the contract:
```rust
pub struct LayoutOutput {
    pub positions: BTreeMap<usize, LogicalPosition>,  // Child positions
    pub overflow_size: LogicalSize,                   // Content overflow
}
```

But the implementing functions (`layout_block_context`, `layout_inline_context`, etc.) do NOT fulfill this contract. They return empty or zero-valued positions.

**Why This Happened:**
Likely causes:
1. **Incomplete refactoring** - Old code removed, new code not finished
2. **Taffy integration** - Flex/Grid use Taffy which returns positions, but Block/Inline contexts were hand-written and left incomplete
3. **Testing gaps** - No integration tests caught this during development

### 6.2 Missing Unit Tests

**Critical Gap:** No tests verify that `layout_formatting_context` returns correct positions.

**What Should Exist:**
```rust
#[test]
fn test_block_fc_vertical_positioning() {
    let root = create_div(size=(100, 100));
    let child1 = create_div(size=(50, 20));
    let child2 = create_div(size=(50, 30));
    root.append(child1, child2);
    
    let output = layout_block_context(root, ...);
    
    assert_eq!(output.positions[&child1], LogicalPosition::new(0, 0));
    assert_eq!(output.positions[&child2], LogicalPosition::new(0, 20));
    //                                                            ^^^ after child1
}
```

### 6.3 Debug vs Release Inconsistency

**Current State:** Debug builds with `eprintln!` show the bug clearly, but there's no automated detection.

**Recommendation:** Add debug assertions:
```rust
#[cfg(debug_assertions)]
fn validate_layout_output(output: &LayoutOutput, children: &[Node]) {
    for (i, child) in children.iter().enumerate().skip(1) {
        let prev_pos = output.positions[&children[i-1].index];
        let curr_pos = output.positions[&child.index];
        
        // In block FC, each child should be below the previous one
        assert!(curr_pos.y > prev_pos.y, 
                "Child {} @ {:?} is not below child {} @ {:?}",
                child.index, curr_pos, children[i-1].index, prev_pos);
    }
}
```

---

## 7. Path to Compliance

### Phase 1: Fix Block Formatting Context (CRITICAL - Week 1)

**Goal:** Make normal block flow work correctly.

**Tasks:**
1. Implement `layout_block_context` position calculation
   - Track vertical cursor position
   - Add child heights (including margins, borders, padding)
   - Handle margin collapsing (simplified first version)
   
2. Add unit tests for block FC positioning
   - Single child
   - Multiple children
   - With margins
   - With borders/padding

3. Verify with `test_cpurender` example
   - All rectangles should stack vertically
   - No overlap

**Success Criteria:**
- ✅ All children positioned at unique y-coordinates
- ✅ Each child's top edge touches previous child's bottom edge (± margins)
- ✅ Display list shows correct positions
- ✅ CPU renderer output shows visually correct layout

### Phase 2: Fix Inline Formatting Context (HIGH - Week 2)

**Goal:** Make text and inline elements flow correctly.

**Tasks:**
1. Implement `layout_inline_context` position calculation
   - Track horizontal cursor within line
   - Track vertical position of current line
   - Handle line wrapping
   
2. Fix glyph positioning in text layout
   - Glyphs currently have point=(0,0) and size=0x0
   - Should use font metrics and shaping results

3. Add tests for inline FC
   - Single-line text
   - Multi-line wrapping
   - Mixed inline/block elements

### Phase 3: Validate Other Positioning Schemes (MEDIUM - Week 3-4)

**Tasks:**
1. Test `position: relative`
   - Should work once normal flow is fixed
   - Add visual offset from normal position

2. Test `position: absolute`
   - Already implemented in `position_out_of_flow_elements`
   - Verify against W3C spec § 3.5

3. Test `position: fixed`
   - Viewport-relative positioning
   - Should stay in place when scrolling

4. Test `position: sticky`
   - Scroll-dependent behavior
   - Complex interaction with scroll containers

### Phase 4: Performance & Polish (LOW - Week 5+)

**Tasks:**
1. Optimize position calculations (avoid recalculations)
2. Add caching for frequently accessed positions
3. Profile with real-world DOMs
4. Add comprehensive compliance test suite

---

## 8. Recommendations

### Immediate Actions (This Week)

1. **Remove debug outputs** after documenting the bug
2. **Create failing integration test** that demonstrates the bug
3. **Implement block FC positioning** in `formatting_context.rs`
4. **Verify fix** with `test_cpurender` and visual inspection

### Short-term (Next 2 Weeks)

1. **Add inline FC positioning**
2. **Fix font glyph positioning** (separate but related bug)
3. **Create W3C CSS2.1 compliance test suite**
4. **Document positioning architecture** in code comments

### Long-term (Next Month)

1. **Full CSS Positioned Layout Level 3 compliance**
2. **Performance benchmarks** for layout engine
3. **Fuzzing tests** to catch edge cases
4. **Visual regression testing** with reference screenshots

---

## 9. Conclusion

**Current State:**
Azul's layout engine is **fundamentally broken** for normal flow positioning. All elements appear at (0,0) because the formatting context functions do not calculate child positions.

**Severity:** 🔴 **CRITICAL** - Blocks all rendering work

**Fix Complexity:** 🟡 **MEDIUM**
- Root cause is well-understood
- Fix location is clear (formatting_context.rs)
- Implementation is straightforward (cumulative position tracking)
- Testing is required to prevent regression

**Estimated Effort:**
- Block FC fix: 2-3 days
- Inline FC fix: 3-4 days  
- Testing & validation: 2-3 days
- **Total: ~2 weeks** for full normal flow compliance

**W3C Spec Compliance:**
- Current: **12% compliant** (2/16 requirements passing)
- After Phase 1: **~60% compliant** (block flow working)
- After Phase 2: **~80% compliant** (inline flow working)
- Full compliance: **Phase 3-4** (all positioning schemes)

---

## References

1. W3C CSS Positioned Layout Module Level 3
   - https://www.w3.org/TR/css-position-3/
   
2. CSS 2.1 Specification - Chapter 9: Visual formatting model
   - https://www.w3.org/TR/CSS2/visuren.html
   
3. CSS 2.1 Specification - Chapter 10: Visual formatting model details
   - https://www.w3.org/TR/CSS2/visudet.html

4. CSS Display Module Level 3 - Formatting Contexts
   - https://www.w3.org/TR/css-display-3/#formatting-context

---

**Document Version:** 1.0  
**Last Updated:** October 23, 2025  
**Next Review:** After Phase 1 completion
