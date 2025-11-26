# CSS Float and Clear Implementation Analysis
**Date:** 2025-11-21  
**Status:** ❌ Not Working  
**Reference:** [CSS Page Floats Module Level 3](https://www.w3.org/TR/css-page-floats-3/)

## Problem Statement

The current azul-layout implementation has a `FloatingContext` structure but **floats are never actually positioned or integrated into the layout flow**. 

### Current vs Expected Behavior

**Current Output:**
```
┌──────────────────────────────────────┐
│ Float and Clear Test                │
├──────────────────────────────────────┤
│ ┌────────────┐                       │
│ │ Float Left │ (red, 100x100)        │
│ └────────────┘                       │
│ ┌────────────┐                       │
│ │Float Right │ (blue, 100x100)       │
│ └────────────┘                       │
│ ┌──────────────────────────────────┐ │
│ │ Normal Box (yellow, full width)  │ │
│ └──────────────────────────────────┘ │
│ ┌──────────────────────────────────┐ │
│ │ Clear Both (green, full width)   │ │
│ └──────────────────────────────────┘ │
└──────────────────────────────────────┘
```
All boxes are stacked vertically with no wrapping.

**Expected Output (CSS Spec Behavior):**
```
┌──────────────────────────────────────┐
│ Float and Clear Test                │
├──────────────────────────────────────┤
│ ┌─────┐ ┌──────────────┐ ┌────────┐ │
│ │Float│ │ Normal Box   │ │ Float  │ │
│ │Left │ │ (wraps in    │ │ Right  │ │
│ │     │ │  remaining   │ │        │ │
│ │     │ │  space)      │ │        │ │
│ └─────┘ └──────────────┘ └────────┘ │
│ ┌──────────────────────────────────┐ │
│ │ Clear Both (below all floats)    │ │
│ └──────────────────────────────────┘ │
└──────────────────────────────────────┘
```
Normal flow content wraps around floats; clear forces positioning below floats.

## Current Implementation Status

### ✅ What Exists

1. **Data Structures** (`layout/src/solver3/fc.rs`):
   - `FloatingContext` - manages float state
   - `FloatBox` - represents individual floats
   - `available_line_box_space()` - calculates space around floats
   - `clearance_offset()` - calculates clear offset

2. **CSS Property Recognition** (`layout/src/solver3/getters.rs`):
   - `get_float()` - extracts `float: left|right|none`
   - `get_clear()` - extracts `clear: left|right|both|none`

3. **BFC Detection** (`layout/src/solver3/layout_tree.rs:1138-1140`):
   ```rust
   let float = get_float(styled_dom, node_id, &styled_node.state);
   if !float.is_none() {
       return true; // Establishes BFC
   }
   ```

### ❌ What's Missing

1. **Float Positioning Logic**: No code path that:
   - Detects floated elements during BFC layout
   - Removes floats from normal flow
   - Positions floats at the line-left/line-right edge
   - Adds floats to `FloatingContext`

2. **Float-Aware Line Box Constraints**: 
   - `available_line_box_space()` exists but is **never called**
   - Line boxes don't query float context for available width
   - Normal flow doesn't wrap around floats

3. **Clear Implementation**:
   - `clearance_offset()` exists but is **never called**
   - No code advances pen position past floats when `clear` is set

4. **Integration Points**: No connection between:
   - `layout_bfc()` and float detection
   - IFC line breaking and float constraints
   - Margin collapse and float positioning

## CSS Specification Requirements

### From [CSS 2.1 Section 9.5](https://www.w3.org/TR/CSS21/visuren.html#floats)

**Float Positioning Rules:**

1. **Out of Flow**: "A floated box is shifted to the left or right until its outer edge touches the containing block edge or the outer edge of another float."

2. **Main-Axis Constraints**: "The top of a floated box is aligned with the top of the current line box or with the bottom of the preceding block-level box."

3. **Cross-Axis Constraints**: "A line box next to a float is shortened to make room for the margin box of the float."

4. **Stacking**: "Floats are stacked starting from the line-left/line-right edge. If there isn't enough space, the float moves down until it fits."

### From [CSS Page Floats Level 3](https://www.w3.org/TR/css-page-floats-3/)

**Modern Float Extensions:**

1. **Float Reference**: Floats are positioned relative to a "float reference" (typically the containing block).

2. **Float Positioning Areas**: Defines regions where floats accumulate (top, bottom, left, right, inside, outside).

3. **Exclusion Rectangles**: Content flow must respect float exclusion areas.

## Architectural Changes Required

### Phase 1: Float Detection and Removal from Normal Flow

**Location:** `layout/src/solver3/fc.rs::layout_bfc()`

**Changes:**
```rust
fn layout_bfc<T, Q>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree,
    text_cache: &mut LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<BfcLayoutResult> {
    // ...existing setup...
    
    // NEW: Initialize FloatingContext for this BFC
    let mut float_context = FloatingContext::default();
    let mut float_indices = Vec::new();
    
    // Pass 1: Separate floats from normal flow
    for &child_index in &node.children {
        let child_node = tree.get(child_index)?;
        let float_type = get_float(ctx.styled_dom, child_node.dom_node_id);
        
        if float_type != LayoutFloat::None {
            float_indices.push((child_index, float_type));
            continue; // Don't include in normal flow
        }
        
        // ...existing normal flow sizing...
    }
    
    // NEW: Pass 1.5: Layout and position floats
    for (float_index, float_type) in float_indices {
        let float_node = tree.get_mut(float_index)?;
        
        // Size the float (it gets full containing block width)
        calculate_layout_for_subtree(
            ctx, tree, text_cache,
            float_index,
            LogicalPosition::zero(),
            constraints.available_size,
            &mut BTreeMap::new(),
            &mut bool::default(),
        )?;
        
        let float_size = float_node.used_size.unwrap_or_default();
        let float_margin = &float_node.box_props.margin;
        
        // Find available position for this float
        let float_rect = position_float(
            &float_context,
            float_type,
            float_size,
            float_margin,
            main_pen, // Current Y position
            constraints.available_size.width,
            writing_mode,
        );
        
        // Add to float context BEFORE positioning subsequent floats
        float_context.add_float(float_type, float_rect);
        
        // Store final position
        output.positions.insert(float_index, float_rect.origin);
    }
    
    // Pass 2: Position normal flow with float awareness
    for &child_index in &node.children {
        // ...check if float, skip...
        
        // Query available space considering floats
        let (cross_start, cross_end) = float_context.available_line_box_space(
            main_pen,
            main_pen + child_size.main(writing_mode),
            constraints.available_size.cross(writing_mode),
            writing_mode,
        );
        
        let available_width = cross_end - cross_start;
        
        // Re-layout child with constrained width if needed
        if available_width < constraints.available_size.width {
            // Trigger reflow with reduced width
        }
        
        // Check for clear property
        let clear = get_clear(ctx.styled_dom, child_node.dom_node_id);
        if !clear.is_none() {
            main_pen = float_context.clearance_offset(
                clear,
                main_pen,
                writing_mode,
            );
        }
        
        // Position child in available space
        let child_cross_pos = cross_start + child_margin.cross_start(writing_mode);
        // ...rest of positioning...
    }
}
```

**New Helper Function:**
```rust
fn position_float(
    float_ctx: &FloatingContext,
    float_type: LayoutFloat,
    size: LogicalSize,
    margin: &EdgeSizes,
    current_main_offset: f32,
    bfc_cross_size: f32,
    wm: LayoutWritingMode,
) -> LogicalRect {
    // 1. Start at current main-axis position (Y in horizontal-tb)
    let mut main_start = current_main_offset;
    
    // 2. Determine cross-axis position based on float type
    let cross_start = if float_type == LayoutFloat::Left {
        // Try to place at line-left (0), but check for existing floats
        let mut cross = margin.cross_start(wm);
        
        // Check if there's space at this Y position
        loop {
            let (avail_start, avail_end) = float_ctx.available_line_box_space(
                main_start,
                main_start + size.main(wm),
                bfc_cross_size,
                wm,
            );
            
            let needed_width = size.cross(wm) + margin.cross_start(wm) + margin.cross_end(wm);
            
            if avail_start + needed_width <= avail_end {
                cross = avail_start + margin.cross_start(wm);
                break;
            }
            
            // Not enough space, move down to clear lowest overlapping float
            main_start = find_next_clear_position(float_ctx, main_start, wm);
        }
        
        cross
    } else {
        // LayoutFloat::Right - similar logic but from right edge
        let mut cross = bfc_cross_size - size.cross(wm) - margin.cross_end(wm);
        
        loop {
            let (avail_start, avail_end) = float_ctx.available_line_box_space(
                main_start,
                main_start + size.main(wm),
                bfc_cross_size,
                wm,
            );
            
            let needed_width = size.cross(wm) + margin.cross_start(wm) + margin.cross_end(wm);
            
            if avail_end - needed_width >= avail_start {
                cross = avail_end - size.cross(wm) - margin.cross_end(wm);
                break;
            }
            
            main_start = find_next_clear_position(float_ctx, main_start, wm);
        }
        
        cross
    };
    
    LogicalRect {
        origin: LogicalPosition::from_main_cross(main_start, cross_start, wm),
        size,
    }
}
```

### Phase 2: FloatingContext Enhancement

**Location:** `layout/src/solver3/fc.rs::FloatingContext`

**New Methods:**
```rust
impl FloatingContext {
    /// Add a newly positioned float to the context
    pub fn add_float(&mut self, kind: LayoutFloat, rect: LogicalRect) {
        self.floats.push(FloatBox { kind, rect });
    }
    
    /// Find the next main-axis position where a float of given size would fit
    pub fn find_next_fit(
        &self,
        min_main_offset: f32,
        float_size: LogicalSize,
        bfc_cross_size: f32,
        wm: LayoutWritingMode,
    ) -> f32 {
        // Binary search or linear scan through float positions
        // to find first Y where there's enough horizontal space
        let mut candidate_main = min_main_offset;
        
        loop {
            let (avail_start, avail_end) = self.available_line_box_space(
                candidate_main,
                candidate_main + float_size.main(wm),
                bfc_cross_size,
                wm,
            );
            
            if avail_end - avail_start >= float_size.cross(wm) {
                return candidate_main;
            }
            
            // Find next distinct Y position from floats
            candidate_main = self.get_next_main_position(candidate_main, wm)
                .unwrap_or(candidate_main + 1.0);
        }
    }
    
    /// Get the next distinct main-axis position after the given offset
    fn get_next_main_position(&self, current: f32, wm: LayoutWritingMode) -> Option<f32> {
        self.floats.iter()
            .map(|f| f.rect.origin.main(wm) + f.rect.size.main(wm))
            .filter(|&main_end| main_end > current)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
    }
}
```

### Phase 3: IFC Integration

**Location:** `layout/src/solver3/fc.rs::layout_ifc()`

**Changes:**
```rust
fn layout_ifc<T, Q>(
    ctx: &mut LayoutContext<T, Q>,
    text_cache: &mut LayoutCache<T>,
    tree: &LayoutTree,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    // ...existing setup...
    
    // NEW: Pass float context to text3 for line breaking
    let float_ctx = constraints.bfc_state
        .as_ref()
        .map(|bfc| &bfc.floats);
    
    let unified_constraints = UnifiedConstraints {
        available_width: constraints.available_size.width,
        available_height: Some(constraints.available_size.height),
        columns: 1,
        // NEW: Include float context
        float_context: float_ctx,
    };
    
    // text3::perform_fragment_layout needs to:
    // 1. Query float_context for available width at each line's Y position
    // 2. Adjust line boxes to fit in non-floated space
    // 3. Break lines when width is too constrained
}
```

**Text3 Changes Required:**
```rust
// In text3/cache.rs or text3/fragment.rs
fn get_line_constraints(
    y_position: f32,
    line_height: f32,
    base_width: f32,
    float_ctx: Option<&FloatingContext>,
    wm: LayoutWritingMode,
) -> Vec<LineSegment> {
    if let Some(ctx) = float_ctx {
        let (cross_start, cross_end) = ctx.available_line_box_space(
            y_position,
            y_position + line_height,
            base_width,
            wm,
        );
        
        vec![LineSegment {
            start_x: cross_start,
            width: cross_end - cross_start,
            priority: 0,
        }]
    } else {
        vec![LineSegment {
            start_x: 0.0,
            width: base_width,
            priority: 0,
        }]
    }
}
```

### Phase 4: Clear Property Implementation

**Location:** `layout/src/solver3/fc.rs::layout_bfc()` - Pass 2

**Implementation:**
```rust
// In normal flow positioning loop:
let child_clear = get_clear(ctx.styled_dom, child_node.dom_node_id);

if child_clear != LayoutClear::None {
    // Advance pen to clear floats BEFORE positioning this child
    main_pen = float_context.clearance_offset(
        child_clear,
        main_pen,
        writing_mode,
    );
    
    // IMPORTANT: After clearing, no more margin collapse with previous sibling
    // The clearance creates separation
    last_margin_bottom = 0.0;
}

// Then proceed with normal positioning
```

## Implementation Complexity Analysis

### Difficulty: **HIGH** ⚠️

**Reasons:**
1. **Two-Pass Complexity**: Floats must be sized and positioned BEFORE normal flow, but they depend on current layout state (Y position).

2. **Reflow Cascade**: Adding a float can affect:
   - All subsequent float positions
   - All normal flow elements' widths
   - Line breaking in IFC
   - Potentially parent BFC size

3. **Margin Collapse Interaction**: Cleared elements break margin collapse chains - needs careful handling.

4. **Text3 Integration**: Requires passing float context through multiple abstraction layers to line breaking code.

5. **Writing Mode Support**: All float positioning must work in horizontal-tb, vertical-rl, vertical-lr.

6. **Performance**: Checking float intersections on every element/line is O(n*m) where n=elements, m=floats.

## Testing Strategy

### Test Cases Required

1. **Basic Float Left/Right**
   - Single float left with text wrap
   - Single float right with text wrap
   - Both float left and right with text in middle

2. **Float Stacking**
   - Multiple consecutive float: left
   - Multiple consecutive float: right
   - Alternating left/right floats

3. **Clear Property**
   - `clear: left` after float: left
   - `clear: right` after float: right
   - `clear: both` after mixed floats
   - Clear with insufficient space (float drop)

4. **Float Drop**
   - Float wider than available space
   - Multiple floats causing vertical stacking

5. **Nested BFCs**
   - Float in parent, BFC child (should not wrap)
   - Float in child BFC (should not escape)

6. **Edge Cases**
   - Float with negative margins
   - Float taller than content
   - Float in absolutely positioned element
   - Float with `position: relative` offset

## Recommended Implementation Order

1. ✅ **Phase 0: Research & Documentation** (This document)

2. **Phase 1: Float Detection** (1-2 days)
   - Modify `layout_bfc()` to separate floats from normal flow
   - Add basic float positioning (without collision detection)
   - Test: Single float left, single float right

3. **Phase 2: Float Collision** (2-3 days)
   - Implement `position_float()` with collision detection
   - Test: Multiple floats stacking correctly

4. **Phase 3: Normal Flow Integration** (3-4 days)
   - Query float context for available width
   - Adjust normal flow element positions
   - Test: Text wrapping around floats

5. **Phase 4: IFC Integration** (4-5 days)
   - Pass float context to text3
   - Modify line breaking to respect float constraints
   - Test: Complex multi-line text with floats

6. **Phase 5: Clear Property** (1-2 days)
   - Implement clearance offset application
   - Test: All clear property combinations

7. **Phase 6: Edge Cases & Polish** (3-4 days)
   - Float drop behavior
   - Margin collapse with clear
   - Negative margins
   - Performance optimization

**Total Estimate: 14-22 days**

## Alternative: Incremental Implementation

If full implementation is too large:

### Minimal Viable Float (MVF)

1. **Only Float Detection** - Mark floats in tree, don't position
2. **Only Clear** - Implement clear without actual float positioning
3. **Static Float Position** - Position floats but don't affect normal flow
4. **Progressive Enhancement** - Add float awareness to one BFC at a time

## Performance Considerations

### Current Bottlenecks

1. **Float Lookup**: O(m) per element check, where m = number of floats
2. **Line Breaking**: Each line must query float context
3. **Reflow**: Width changes cascade through entire subtree

### Optimizations

1. **Spatial Indexing**: Use R-tree or grid for float position queries
2. **Dirty Marking**: Only reflow elements actually affected by floats
3. **Cached Line Constraints**: Store per-line float intrusions
4. **Early Exit**: Skip float queries when no floats in BFC

## Conclusion

The current azul-layout has the **data structures** for floats but **zero implementation** of the actual float positioning algorithm. This is not a bug fix but a **significant feature addition** requiring:

- ~15-20 days of focused development
- Deep understanding of CSS 2.1 float rules
- Careful integration with existing margin collapse
- Extensive testing across writing modes

**Recommendation:** Implement in phases, starting with basic float-left/float-right in horizontal writing mode, then expand to full specification compliance.

## References

- [CSS 2.1 Section 9.5 - Floats](https://www.w3.org/TR/CSS21/visuren.html#floats)
- [CSS Page Floats Module Level 3](https://www.w3.org/TR/css-page-floats-3/)
- [MDN: Float](https://developer.mozilla.org/en-US/docs/Web/CSS/float)
- [CSS WG: Float Spec Issues](https://github.com/w3c/csswg-drafts/labels/css-page-floats-3)

---

**Next Steps:**
1. Review this analysis with team
2. Prioritize: Full implementation vs MVF
3. Allocate development time
4. Create detailed task breakdown
5. Set up comprehensive test suite
