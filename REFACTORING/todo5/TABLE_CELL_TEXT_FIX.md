# Table Cell Text Content - Implementation TODO

## Status: IN PROGRESS

## Goal
Make table cells measure text content via IFC to fix invisible table bug.

## Implementation Plan: Option A - TableCell IFC Integration

### Phase 1: Core Re-architecture ✅ COMPLETE

- [x] 1.1 Modify `layout_cell_for_height` to detect text children via DOM
- [x] 1.2 Call `layout_ifc()` for cells with text content
- [x] 1.3 Extract padding/border calculation into helper function
- [x] 1.4 Handle mixed content (text + block children)

### Phase 2: Integration & Testing

- [ ] 2.1 Test with current simple table (Header 1, Cell 1)
- [ ] 2.2 Verify row heights become > 0
- [ ] 2.3 Verify table height > 0
- [ ] 2.4 Generate PDF and verify table visible

### Phase 3: Edge Cases

- [ ] 3.1 Test table with only block children (no text)
- [ ] 3.2 Test table with mixed content
- [ ] 3.3 Test table with multi-line text wrapping
- [ ] 3.4 Test empty cells
- [ ] 3.5 Test cells with images/other inline content

### Phase 4: Cleanup

- [ ] 4.1 Remove debug eprintln! statements
- [ ] 4.2 Add proper error handling
- [ ] 4.3 Update comments/documentation
- [ ] 4.4 Run layout tests to ensure no regression

### Phase 5: Secondary Issues

- [ ] 5.1 Fix body margin not affecting position
- [ ] 5.2 Verify resize detection still works

## Key Files to Modify

1. **azul/layout/src/solver3/fc.rs**
   - `layout_cell_for_height()` - Main change point
   - Lines ~1951-2008

## Implementation Notes

### Current Broken Flow:
```
TableCell → check tree.children() → empty → height=0
```

### New Fixed Flow:
```
TableCell → check DOM children → has text? → call layout_ifc() → measure height
```

### Critical Points:
- IFC already traverses DOM correctly (lines 2367+)
- Don't break existing cell layout for non-text content
- Preserve padding/border calculations
- Handle writing modes correctly

## Expected Outcome

**Before:**
```
row_heights=[0.0, 0.0, 0.0]
table final_used_size=595.28x0
```

**After:**
```
row_heights=[16.0, 16.0, 16.0]  (or similar based on font size)
table final_used_size=595.28x48.0  (or similar)
```

## References

- Research Report: `/Users/fschutt/Development/azul/REFACTORING/todo5/invisible.md`
- HTML Spec: https://html.spec.whatwg.org/multipage/tables.html
- CSS 2.2 IFC: https://www.w3.org/TR/CSS22/visuren.html#inline-formatting
