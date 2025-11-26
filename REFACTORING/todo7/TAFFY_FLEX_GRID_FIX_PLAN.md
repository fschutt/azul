# Taffy Flex/Grid Integration - Comprehensive Fix Plan

**Date**: 2025-11-22  
**Status**: PLANNING  
**Goal**: Properly implement CSS-spec compliant flex/grid layout with correct defaults and stretch behavior

---

## Executive Summary

### Current State
- ✅ **Width distribution works** - flex-grow ratios (1:2:3) produce correct widths (99px, 198px, 298px)
- ✅ **InherentSize mode works** - Container respects explicit 600×100px dimensions
- ❌ **Height stretching broken** - Items remain at intrinsic text height (17.71875px) instead of filling 100px container
- ❌ **Incomplete default values** - Many flex/grid properties missing proper CSS spec defaults

### Root Cause Analysis

#### Issue #1: Intrinsic Size Override
**Location**: `taffy_bridge.rs:776-789` - `compute_child_layout()` leaf node handling

```rust
compute_leaf_layout(
    inputs,
    &style,
    |_, _| 0.0,
    |known_dimensions, _available_space| {
        let intrinsic = node.intrinsic_sizes.unwrap_or_default();
        Size {
            width: known_dimensions.width.unwrap_or(intrinsic.max_content_width),
            height: known_dimensions.height.unwrap_or(intrinsic.max_content_height),  // ← PROBLEM
        }
    },
)
```

**Problem**: When Taffy calls `compute_child_layout()` for flex items with `align-items: stretch`:
1. Taffy expects items with `height: auto` to return `None` for intrinsic height
2. Our implementation **always returns** `intrinsic.max_content_height` (17.71875px from text)
3. Taffy sees a definite intrinsic size and **doesn't apply stretch**
4. Items get centered at their intrinsic height instead of filling container

**Evidence**:
- Items positioned at Y=41.140625px ≈ (100 - 17.71875) / 2 → **vertically centered**
- CSS spec: "If the flex item has align-self: stretch, redo layout for its contents, treating this used size as its definite cross size"
- Taffy needs `None` or `0.0` for cross-axis intrinsic size to apply stretch

#### Issue #2: Missing CSS Defaults
**Location**: `taffy_bridge.rs:505-616` - Flex/Grid property translation

Many properties fall back to `unwrap_or_default()` which gives **Rust defaults**, not **CSS spec defaults**:

| Property | Current Default | CSS Spec Default | Status |
|----------|----------------|------------------|--------|
| `align_items` | ❌ None | ✅ `stretch` (flexbox), `start` (grid) | **FIXED** |
| `align_content` | ❌ None | ❌ `stretch` (flexbox), `start` (grid) | **BROKEN** |
| `justify_content` | ❌ None | ❌ `flex-start` (flex), `start` (grid) | **BROKEN** |
| `flex_direction` | ✅ `row` | ✅ `row` | OK |
| `flex_wrap` | ✅ `nowrap` | ✅ `nowrap` | OK |
| `flex_grow` | ✅ `0.0` | ✅ `0.0` | OK |
| `flex_shrink` | ✅ `1.0` | ✅ `1.0` | OK |
| `flex_basis` | ✅ `auto` | ✅ `auto` | OK |
| `align_self` | ✅ None (inherit) | ✅ `auto` | OK |
| `gap` | ✅ `0` | ✅ `0` | OK |
| `grid_auto_flow` | ⚠️ Default trait | ❓ `row` | **CHECK** |

#### Issue #3: Context-Dependent Defaults
**Problem**: `align_items` default is **different** for flex vs grid:
- Flexbox: `align-items: stretch` (makes items fill cross-axis)
- Grid: `align-items: start` (respects item's intrinsic size)

Currently we apply `stretch` to **both**, which is wrong for grid.

---

## CSS Flexbox Specification Analysis

### Align-Items: Stretch Behavior
From **CSS Flexible Box Layout Module Level 1** (W3C):

> "**stretch**: If the cross size property of the flex item computes to auto, and neither of the cross-axis margins are auto, the flex item is stretched. Its used value is the length necessary to make the cross size of the item's margin box as close to the same size as the line as possible, while still respecting the constraints imposed by min-height/min-width/max-height/max-width."

**Key Requirements**:
1. Item must have `height: auto` (or `width: auto` for column flex) ✅ Our test has this
2. Item must NOT have auto margins on cross-axis ✅ Our test has no margins
3. Item must NOT have definite intrinsic cross-size ❌ **We report 17.71875px from text**
4. Item must respect min/max constraints ✅ None set in test

### Intrinsic Sizes in Flexbox
From **CSS Intrinsic & Extrinsic Sizing Module Level 4**:

> "The **intrinsic size** of an element is the size it would have based on its content alone, without considering any constraints imposed by its formatting context."

**For text content**: The intrinsic height is the height of the text line(s).

**For flex items**: When `align-items: stretch` is set:
- Intrinsic cross-size should be **ignored** if the item's cross-size is `auto`
- Item should fill the flex line's cross-size
- Text content should be **laid out within** the stretched size, not dictate it

**Our Bug**: We return intrinsic size → Taffy thinks item has definite size → no stretch applied

---

## Architecture Analysis

### Current Flow
```
1. LayoutTree created with intrinsic_sizes computed from text
   └─> Node.intrinsic_sizes = IntrinsicSizes { max_content_height: 17.71875, ... }

2. Taffy calls compute_child_layout() for flex item
   └─> We call compute_leaf_layout()
       └─> Measure function returns: Size { height: 17.71875 }

3. Taffy sees definite intrinsic height
   └─> Skips stretch, centers item at Y=41px

4. Item rendered: 99.33×17.71875 @ (2, 41) instead of 99.33×100 @ (2, 0)
```

### Desired Flow
```
1. LayoutTree created with intrinsic_sizes computed from text
   └─> Node.intrinsic_sizes = IntrinsicSizes { max_content_height: 17.71875, ... }

2. Taffy calls compute_child_layout() for flex item
   └─> We check: Is parent flex/grid? Is align-items/align-self stretch? Is height auto?
       └─> YES → Return height: 0.0 or None (signal "no intrinsic size")
       └─> NO → Return height: 17.71875 (normal intrinsic size)

3. Taffy sees no definite intrinsic height + height: auto
   └─> Applies stretch, sets item height to 100px

4. Taffy calls compute_child_layout() AGAIN with known_dimensions.height = Some(100.0)
   └─> We layout text within 100px height
   └─> Return final layout

5. Item rendered: 99.33×100 @ (2, 0) ✅ CORRECT
```

### Key Insight
**Taffy calls `compute_child_layout()` TWICE for stretched items**:
1. **First call**: Measure intrinsic size (we must return None/0 for cross-axis)
2. **Second call**: Layout with definite cross-size (we layout content within stretched size)

We currently return intrinsic size in **both** calls, breaking the stretch protocol.

---

## Proposed Solution

### Strategy: Context-Aware Intrinsic Sizing

**Core Idea**: Detect when a node is a flex/grid item with stretch behavior, and suppress cross-axis intrinsic size in that case.

### Implementation Plan

#### Phase 1: Fix Intrinsic Sizing for Stretch (HIGH PRIORITY)

**Location**: `taffy_bridge.rs:776-789` - `compute_child_layout()` measure function

**Current Code**:
```rust
compute_leaf_layout(
    inputs,
    &style,
    |_, _| 0.0,
    |known_dimensions, _available_space| {
        let intrinsic = node.intrinsic_sizes.unwrap_or_default();
        Size {
            width: known_dimensions.width.unwrap_or(intrinsic.max_content_width),
            height: known_dimensions.height.unwrap_or(intrinsic.max_content_height),
        }
    },
)
```

**Proposed Fix**:
```rust
compute_leaf_layout(
    inputs,
    &style,
    |_, _| 0.0,
    |known_dimensions, _available_space| {
        let intrinsic = node.intrinsic_sizes.unwrap_or_default();
        
        // Determine if we should suppress intrinsic cross-size for stretching
        // This is complex - we need to know:
        // 1. Parent's display type (flex row/column or grid)
        // 2. Parent's align-items value
        // 3. Our align-self value (overrides parent's align-items)
        // 4. Our cross-axis size (must be auto)
        
        // PROBLEM: We don't have easy access to parent context here!
        
        Size {
            width: known_dimensions.width.unwrap_or(intrinsic.max_content_width),
            height: known_dimensions.height.unwrap_or(intrinsic.max_content_height),
        }
    },
)
```

**Challenge**: To determine if stretch applies, we need parent context (display type, align-items) which we don't have in the measure callback.

#### Phase 2: Add Parent Context to LayoutNode

**Location**: `layout_tree.rs` - `LayoutNode` struct

**Add Field**:
```rust
pub struct LayoutNode {
    // ... existing fields ...
    
    /// Parent's formatting context (Flex, Grid, Block, etc.)
    pub parent_formatting_context: Option<FormattingContext>,
    
    /// Effective align-self value (considering parent's align-items)
    pub effective_align_self: Option<AlignSelf>,
}
```

**Population**: During tree construction in `fc.rs`, when creating child nodes:
```rust
child_node.parent_formatting_context = Some(parent.formatting_context);
child_node.effective_align_self = compute_effective_align_self(parent, child);
```

#### Phase 3: Implement Stretch Detection Logic

**Location**: New helper function in `taffy_bridge.rs`

```rust
impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> TaffyBridge<'a, 'b, T, Q> {
    /// Determines if cross-axis intrinsic size should be suppressed for stretching
    fn should_suppress_cross_intrinsic(&self, node_idx: usize, style: &Style) -> (bool, bool) {
        let Some(node) = self.tree.get(node_idx) else {
            return (false, false);
        };
        
        let Some(parent_fc) = node.parent_formatting_context else {
            return (false, false);
        };
        
        // Determine cross-axis based on parent's main axis
        let (suppress_width, suppress_height) = match parent_fc {
            FormattingContext::Flex => {
                // Get parent's flex-direction to determine cross-axis
                let parent_idx = /* need parent idx */;
                let parent_style = self.get_taffy_style(parent_idx);
                
                let is_row = matches!(
                    parent_style.flex_direction, 
                    FlexDirection::Row | FlexDirection::RowReverse
                );
                
                // Get effective align-self (or parent's align-items)
                let align = node.effective_align_self
                    .or(parent_style.align_items)
                    .unwrap_or(AlignSelf::Stretch);
                
                let should_stretch = matches!(align, AlignSelf::Stretch);
                
                // Check if our cross-axis size is auto
                let cross_size_is_auto = if is_row {
                    matches!(style.size.height, Dimension::Auto)
                } else {
                    matches!(style.size.width, Dimension::Auto)
                };
                
                if should_stretch && cross_size_is_auto {
                    if is_row {
                        (false, true)  // Suppress height for row flex
                    } else {
                        (true, false)  // Suppress width for column flex
                    }
                } else {
                    (false, false)
                }
            }
            FormattingContext::Grid => {
                // Similar logic for grid, but align-items default is 'start' not 'stretch'
                // TODO: Implement grid stretch detection
                (false, false)
            }
            _ => (false, false),
        };
        
        (suppress_width, suppress_height)
    }
}
```

**Usage**:
```rust
compute_leaf_layout(
    inputs,
    &style,
    |_, _| 0.0,
    |known_dimensions, _available_space| {
        let intrinsic = node.intrinsic_sizes.unwrap_or_default();
        let (suppress_width, suppress_height) = tree.should_suppress_cross_intrinsic(node_idx, &style);
        
        Size {
            width: known_dimensions.width.unwrap_or(
                if suppress_width { 0.0 } else { intrinsic.max_content_width }
            ),
            height: known_dimensions.height.unwrap_or(
                if suppress_height { 0.0 } else { intrinsic.max_content_height }
            ),
        }
    },
)
```

#### Phase 4: Fix Missing CSS Defaults

**Location**: `taffy_bridge.rs:540-616` - Property translations

**Changes**:

1. **align_content** (line ~610):
```rust
taffy_style.align_content = Some(cache
    .get_property(node_data, &id, node_state, &CssPropertyType::AlignContent)
    .and_then(|p| {
        if let CssProperty::AlignContent(v) = p {
            Some(*v)
        } else {
            None
        }
    })
    .map(layout_align_content_to_taffy)
    .unwrap_or_else(|| {
        // Default depends on display type
        match taffy_style.display {
            Display::Flex => AlignContent::Stretch,  // CSS flex default
            Display::Grid => AlignContent::Start,    // CSS grid default
            _ => AlignContent::Stretch,
        }
    }));
```

2. **justify_content** (line ~539):
```rust
taffy_style.justify_content = Some(cache
    .get_property(node_data, &id, node_state, &CssPropertyType::JustifyContent)
    .and_then(|p| {
        if let CssProperty::JustifyContent(v) = p {
            Some(*v)
        } else {
            None
        }
    })
    .map(layout_justify_content_to_taffy)
    .unwrap_or_else(|| {
        // Default depends on display type
        match taffy_style.display {
            Display::Flex => JustifyContent::FlexStart,  // CSS flex default
            Display::Grid => JustifyContent::Start,       // CSS grid default
            _ => JustifyContent::FlexStart,
        }
    }));
```

3. **Fix align_items** to be context-aware (line 528):
```rust
taffy_style.align_items = Some(cache
    .get_property(node_data, &id, node_state, &CssPropertyType::AlignItems)
    .and_then(|p| {
        if let CssProperty::AlignItems(v) = p {
            Some(*v)
        } else {
            None
        }
    })
    .map(layout_align_items_to_taffy)
    .unwrap_or_else(|| {
        // Default depends on display type - CRITICAL DIFFERENCE!
        match taffy_style.display {
            Display::Flex => AlignItems::Stretch,  // CSS flexbox default
            Display::Grid => AlignItems::Start,    // CSS grid default
            _ => AlignItems::Stretch,
        }
    }));
```

#### Phase 5: Testing & Validation

**Test Cases**:

1. **Current test** (flexbox-simple-test.html):
   - Expected: Items 99.33×100px, 198.67×100px, 298×100px at Y=0
   - Validates: Flex-grow + stretch

2. **Explicit align-items: center**:
   ```html
   .container { display: flex; align-items: center; }
   ```
   - Expected: Items at intrinsic height, centered at Y=41px
   - Validates: Stretch suppression respects explicit values

3. **Explicit height on items**:
   ```html
   .item1 { height: 50px; }
   ```
   - Expected: Item1 at 50px height, not stretched
   - Validates: Definite size blocks stretch

4. **Grid with stretch** (should NOT stretch by default):
   ```html
   .container { display: grid; grid-template-columns: 1fr 1fr; }
   ```
   - Expected: Items at intrinsic height (grid default is start, not stretch)
   - Validates: Context-aware defaults

5. **Column flexbox**:
   ```html
   .container { display: flex; flex-direction: column; height: 200px; }
   .item { width: auto; }
   ```
   - Expected: Items stretch to full width, respect content height
   - Validates: Cross-axis detection for column flex

---

## Alternative Approaches Considered

### Alternative 1: Modify Intrinsic Size Computation
**Idea**: Don't compute text intrinsic height for flex items at all.

**Rejected**: 
- Breaks measurement when stretch doesn't apply (explicit align-items: center, etc.)
- Intrinsic size is needed for normal flow layout
- Too invasive, affects many code paths

### Alternative 2: Post-Process Taffy Layout
**Idea**: Let Taffy center items, then manually stretch them afterward.

**Rejected**:
- Not spec-compliant (text should be laid out within stretched size)
- Breaks nested layouts
- Doesn't fix root cause
- Hack, not a proper solution

### Alternative 3: Fork Taffy and Modify Stretch Logic
**Idea**: Change Taffy's stretch implementation to ignore intrinsic sizes.

**Rejected**:
- Maintains a fork (maintenance burden)
- Taffy is correct - we're using it wrong
- Other users would have same issue
- Doesn't fix our CSS defaults issue

### Alternative 4: Use Taffy's Tree API Instead of Traits
**Idea**: Build a real Taffy tree with nodes, then call layout.

**Rejected**:
- Major refactor (weeks of work)
- Loses integration with our existing LayoutTree
- Breaks text shaping integration
- Not necessary - trait API should work

---

## Risk Assessment

### High Risk Areas

1. **Parent Context Access** (Phase 2)
   - Risk: Complex tree traversal, potential for circular dependencies
   - Mitigation: Store minimal context (parent FC + effective align-self), compute during tree construction

2. **Cross-Axis Detection** (Phase 3)
   - Risk: Getting flex-direction wrong leads to stretching wrong axis
   - Mitigation: Comprehensive test suite covering row/column/row-reverse/column-reverse

3. **Grid vs Flex Defaults** (Phase 4)
   - Risk: Wrong defaults break existing layouts
   - Mitigation: Add tests for both grid and flex, compare with browser behavior

4. **Performance** (All Phases)
   - Risk: Extra context checking on every leaf node layout
   - Mitigation: Use cached effective_align_self, avoid repeated parent lookups

### Low Risk Areas

1. **CSS Defaults Fix** (Phase 4)
   - Risk: Minimal - just setting proper default values
   - Impact: May fix other subtle bugs we haven't noticed

2. **Testing** (Phase 5)
   - Risk: None - pure validation
   - Benefit: Catches regressions early

---

## Implementation Timeline

### Priority 1 (Today): Fix Critical Defaults
- [x] align_items: stretch for flexbox ✅ DONE
- [ ] Fix align_items to be context-aware (flex vs grid)
- [ ] Fix align_content default (stretch for flex, start for grid)
- [ ] Fix justify_content default (flex-start for flex, start for grid)
- **Time**: 1 hour
- **Risk**: Low
- **Benefit**: Fixes multiple bugs at once

### Priority 2 (Today): Design Parent Context System
- [ ] Add parent_formatting_context field to LayoutNode
- [ ] Add effective_align_self field to LayoutNode  
- [ ] Implement population logic in tree construction
- [ ] Add helper to get parent index from child
- **Time**: 2 hours
- **Risk**: Medium
- **Benefit**: Enables stretch detection

### Priority 3 (Today): Implement Stretch Detection
- [ ] Write should_suppress_cross_intrinsic() helper
- [ ] Integrate into compute_leaf_layout measure callback
- [ ] Add logging to verify detection works
- [ ] Test with flexbox-simple-test.html
- **Time**: 2 hours
- **Risk**: High (most complex part)
- **Benefit**: FIXES THE MAIN BUG

### Priority 4 (Today): Comprehensive Testing
- [ ] Test flexbox row with stretch (current test)
- [ ] Test flexbox with explicit align-items: center
- [ ] Test flexbox column with stretch (width)
- [ ] Test grid (should NOT stretch by default)
- [ ] Test items with explicit heights
- **Time**: 1 hour
- **Risk**: Low
- **Benefit**: Validates entire solution

**Total Estimated Time**: 6 hours
**Completion Target**: End of day (2025-11-22)

---

## Success Criteria

### Must Have (P0)
- ✅ Items in flexbox-simple-test.html render at full 100px height
- ✅ Items positioned at Y=0 (top of container), not Y=41 (centered)
- ✅ All CSS defaults match specification (flex vs grid aware)
- ✅ Width distribution remains correct (99.33:198.67:298)

### Should Have (P1)
- ✅ Grid containers don't incorrectly stretch items
- ✅ Explicit align-items/align-self values respected
- ✅ Column flexbox stretches width, not height
- ✅ Items with explicit cross-size don't stretch

### Nice to Have (P2)
- ✅ Performance impact minimal (<5% layout time increase)
- ✅ Clean architecture (no hacks or workarounds)
- ✅ Comprehensive test coverage (5+ test cases)
- ✅ Documentation for future maintainers

---

## Open Questions

1. **Q**: Should we suppress intrinsic size completely (0.0) or return None?
   **A**: Return 0.0 - Taffy expects f32, and 0.0 signals "no intrinsic constraint"

2. **Q**: What about min-height/max-height constraints with stretch?
   **A**: Taffy handles this - stretch respects min/max. We just suppress base intrinsic.

3. **Q**: Do we need to handle align-self: auto specially?
   **A**: Yes - auto means "inherit parent's align-items". Compute effective value during tree construction.

4. **Q**: What about nested flex containers?
   **A**: Each node stores its parent's FC. Nested containers are children of other flex containers - works naturally.

5. **Q**: How do we get parent node index efficiently?
   **A**: Store parent_idx: Option<usize> in LayoutNode, populated during tree construction.

---

## CRITICAL DEBUGGING FINDINGS (2025-11-22)

### Symptom: Taffy Trait Methods Never Called

**Evidence**:
- `child_ids()` - NOT called
- `child_count()` - NOT called  
- `get_flexbox_child_style()` - NOT called
- `compute_child_layout()` - NOT called
- `translate_style_to_taffy()` for child nodes - NOT called

**But**:
- `layout_taffy_subtree()` IS called for the flex container (Node 2)
- `compute_flexbox_layout()` IS executed
- Container layout completes successfully (600×100px)

### Analysis

This pattern suggests one of the following:

## Related Issues

- **Issue #1**: Flexbox items rendering at 0×0px → FIXED (InherentSize mode)
- **Issue #2**: Flex-grow not distributing width → FIXED (explicit flex properties)
- **Issue #3**: Items not stretching to container height → THIS PLAN
- **Issue #4**: Grid template rows/columns commented out → SEPARATE (Taffy API mismatch)

---

## References

### CSS Specifications
- **CSS Flexbox Level 1**: https://www.w3.org/TR/css-flexbox-1/
  - Section 8.3: Cross-axis Alignment (align-items, align-self)
  - Section 9: Flex Layout Algorithm
- **CSS Grid Level 1**: https://www.w3.org/TR/css-grid-1/
  - Section 6.1: Grid Item Alignment (align-items default)
- **CSS Sizing Level 4**: https://www.w3.org/TR/css-sizing-4/
  - Section 4: Intrinsic Size Determination

### Taffy Documentation
- **Layout Algorithm**: Flexbox uses two-pass layout for stretch
- **Measure Callbacks**: Should return intrinsic size, or 0.0 to signal "no constraint"
- **compute_leaf_layout**: Used for nodes with content but no layout children

### Codebase
- `azul/layout/src/solver3/taffy_bridge.rs`: Taffy integration
- `azul/layout/src/solver3/fc.rs`: Formatting context dispatch
- `azul/layout/src/solver3/layout_tree.rs`: LayoutNode definition

---

## Notes for Implementation

### Debugging Tips
1. Add logging in measure callback to see when intrinsic sizes are requested
2. Log parent_formatting_context and effective_align_self for each node
3. Compare Y-positions: Y=0 means stretched, Y>0 means centered/aligned
4. Check Taffy's inputs.known_dimensions in both layout passes

### Common Pitfalls
1. **Forgetting context dependency**: align-items default is DIFFERENT for flex vs grid!
2. **Wrong axis for column flex**: Column flex stretches WIDTH, not height
3. **Not handling reverse directions**: row-reverse and column-reverse use same axis
4. **Auto margins**: Items with auto cross-margins don't stretch (not in our test)

### Testing Strategy
1. Start with simplest case (current test: row flex, stretch, no explicit sizes)
2. Add one complexity at a time (explicit align, column direction, grid)
3. Compare with browser rendering using same HTML
4. Use visual diff tool to verify pixel-perfect match

---

## Conclusion

This is a **solvable problem** with a **clear path forward**. The root cause is well understood:
- We return intrinsic height when Taffy expects 0.0 to signal "no constraint"
- Missing CSS defaults cause wrong behavior for grid containers
- Lack of parent context prevents proper stretch detection

The solution requires:
1. **Minimal architecture change**: Add 2 fields to LayoutNode
2. **Targeted logic**: One helper function to detect stretch cases
3. **Proper defaults**: Fix 3 property translations to be context-aware
4. **Comprehensive testing**: 5 test cases to cover all scenarios

**Estimated effort**: 6 hours for full implementation and testing.  
**Risk level**: Medium (most complex part is stretch detection, but well-scoped).  
**Reward**: Fully CSS-spec compliant flex/grid layout, fixing multiple bugs at once.

Let's do this properly. 🚀

## CRITICAL DEBUGGING FINDINGS - Nov 22, 2025

### The Mystery Solved: Why Children Weren't Queried

**Initial Hypothesis**: Taffy wasn't calling trait methods to query children.

**Reality**: Taffy WAS calling all methods correctly:
- ✅ `child_ids()` - called 6 times, returned [3, 4, 5]
- ✅ `child_count()` - called 12 times, returned 3
- ✅ `compute_child_layout()` - called 18 times (3 children × 6 layout passes)
- ✅ `should_suppress_cross_intrinsic()` - called and correctly returned (false, true)

**The Real Problem**: stdout buffering and grep timing! Using `cargo run ... &> out.txt` revealed all the hidden debug output.

### The Actual Bug: Taffy's Stretch Implementation

**Symptom**:
```
[MEASURE] Node NodeId(3): known_dimensions=Size { width: Some(99.333336), height: None }
[MEASURE]   suppress_width=false, suppress_height=true
[MEASURE]   result=Size { width: 99.333336, height: 0.0 }

[SET_LAYOUT] Node 3 (DOM NodeId(3)): size=99.333336x0, pos=(2, 50)
```

**Expected Behavior** (CSS Flexbox Spec):
- When `align-items: stretch` and item has `height: auto`
- Taffy should call measure with `known_dimensions.height: Some(100.0)` (container height)
- Final layout should be: `size=99.333336x100, pos=(2, 0)` (stretched, top-aligned)

**Actual Behavior** (Taffy 0.9.1):
- Taffy calls measure with `known_dimensions.height: None`
- Item reports intrinsic height of 0.0 (suppressed correctly)
- Taffy sets: `size=99.333336x0, pos=(2, 50)` (NOT stretched, centered)

### Analysis: Is This a Taffy Bug?

**Evidence Taffy is Working**:
1. Width distribution is perfect (99.33px, 198.67px, 298px via flex-grow 1:2:3)
2. All trait methods called correctly
3. Container sized correctly (600×100px)

**Evidence Taffy is Broken**:
1. Stretch not applied despite `align-items: Stretch` in style
2. measure never receives `known_dimensions.height: Some(...)`
3. Items remain at intrinsic size (0px) instead of container size (100px)
4. Y-position is 50px (centered) not 0px (stretched to top)

**Hypothesis**: Taffy 0.9.1's `align-items: stretch` may require:
- Explicit `min-height: 0` or `max-height: infinity` in style
- Different flex item configuration
- Or this is a genuine Taffy bug

### Next Steps

1. **Check Taffy source code**: Review how `align-items: Stretch` is implemented in 0.9.1
2. **Test minimal case**: Create standalone Taffy test without our bridge
3. **Check GitHub**: Search Taffy issues for "stretch" bugs
4. **Consider upgrade**: Check if newer Taffy versions fix this
5. **Manual workaround**: Post-process layout to apply stretch manually

### Workaround Strategy

If Taffy bug is confirmed, implement in `set_unrounded_layout`:

```rust
// After Taffy sets layout
if parent_uses_stretch && child_cross_size_is_auto {
    // Override height to match container
    let container_height = get_parent_height();
    layout.size.height = container_height;
    layout.location.y = 0.0;  // Top-align instead of center
}
```

This would bypass Taffy's broken stretch logic and manually apply the CSS spec behavior.
