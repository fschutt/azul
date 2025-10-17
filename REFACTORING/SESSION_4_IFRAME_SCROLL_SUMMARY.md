# Session 4: IFrame Scroll Management - Planning Complete

## What Was Accomplished

### 1. Comprehensive Planning Document Created

Created `IFRAME_SCROLL_MANAGEMENT_PLAN.md` - a detailed architectural document covering:

- **Dual Size Model** - Explanation of actual vs virtual scroll bounds
- **Conditional Re-invocation Rules** - When and why IFrame callbacks should be triggered
- **State Management** - IFrameState structure and tracking
- **Integration with ScrollManager** - How scrolling triggers lazy loading
- **Implementation Phases** - Step-by-step guide for implementation
- **Testing Strategy** - Unit and integration test scenarios
- **Performance Considerations** - Preventing infinite loops and repeated invocations
- **Migration Path** - How to handle API changes

### 2. Enhanced Documentation

Added **extensive documentation** to `IFrameCallbackReturn` structure in:

#### `core/src/callbacks.rs`
- Detailed explanation of dual size model
- All 5 conditional re-invocation rules with examples
- Window resize behavior with frame-by-frame breakdown
- Scroll near edge behavior with threshold calculations
- Two complete examples:
  * Basic IFrame usage
  * Virtualized table with lazy loading (1000 rows, renders 20)
- How layout engine uses the values

#### `api/rust/src/lib.rs` (C API wrapper)
- Summary documentation for C API consumers
- Reference to full documentation in core module
- Field-level documentation for each member

## Key Architectural Decisions

### Dual Size Model

**Why two size/offset pairs?**

1. **Actual Content** (`scroll_size` + `scroll_offset`):
   - What's currently rendered in the DOM
   - Example: 20 visible rows of a table

2. **Virtual Content** (`virtual_scroll_size` + `virtual_scroll_offset`):
   - What the IFrame pretends to have
   - Example: All 1000 rows for scrollbar sizing

**Benefits**:
- ✅ Lazy loading - Only render visible content
- ✅ Memory efficient - 1000-row table uses memory of 20 rows
- ✅ Smooth scrolling - Scrollbar represents full dataset
- ✅ Progressive rendering - Load more on demand

### Conditional Re-invocation Rules

The callback is invoked **only when necessary** to prevent performance issues:

#### Rule 1: Initial Render
- **Trigger**: IFrame first appears
- **Reason**: Need initial content

#### Rule 2: Parent DOM Recreated
- **Trigger**: Parent DOM rebuilt (not just re-laid-out)
- **Reason**: Layout cache invalidated

#### Rule 3: Window Resize (Expansion Only)
- **Trigger**: Window grows AND IFrame bounds exceed `scroll_size`
- **Once**: Only invoked ONCE per expansion
- **Not Triggered**: Window shrinking (content just clips)
- **Not Triggered**: Expanded area still within `scroll_size`

**Example Flow**:
```
Frame 0: bounds=800×600, scroll_size=800×600 ✅ Covered
Frame 1: Window → 1000×700
  bounds=1000×700 > scroll_size=800×600 ❌ NOT covered
  → INVOKE callback (once)
Frame 2: Window → 900×650 (smaller)
  → Do NOT invoke (content clips)
```

#### Rule 4: Scroll Near Edge
- **Trigger**: Scroll within threshold (200px) of content edge
- **Once**: Only invoked ONCE per edge approach
- **Reset**: Flag resets when scroll moves away OR callback expands content

**Example Flow**:
```
scroll_size = 1000×2000
Container = 800×600
Scroll to y=1500:
  → Bottom at 1500 + 600 = 2100
  → Distance from edge: 2100 - 2000 = 100px < 200px
  → INVOKE callback
  → Returns scroll_size = 1000×4000
  → Continue scrolling without invoke until near new edge
```

#### Rule 5: Programmatic Scroll
- **Trigger**: `set_scroll_position()` scrolls beyond `scroll_size`
- **Same constraints** as Rule 4

### Why These Rules?

**Efficiency**: Prevents calling callback on every frame during:
- Window resize animations
- Smooth scrolling
- Multiple scroll events

**User Experience**: Only loads new content when user would actually see it:
- Near edge of rendered content
- New visible area from window expansion

**Performance**: Avoids:
- Infinite callback loops
- Repeated DOM rebuilds
- Unnecessary layout passes

## Implementation Roadmap

### Phase 1: Core Type Updates ⏳ Not Started
- [ ] Add `OptionStyledDom` to `core/src/dom.rs`
- [ ] Change `IFrameCallbackReturn.dom` from `StyledDom` to `OptionStyledDom`
- [ ] Add convenience methods: `with_dom()`, `keep_current()`
- [ ] Update C API wrappers

### Phase 2: IFrameState Tracking ⏳ Not Started
- [ ] Enhance `IFrameState` in `layout/src/window.rs`
- [ ] Add fields: `current_scroll_size`, `cached_dom`, invocation flags
- [ ] Implement `should_reinvoke_iframe()` method
- [ ] Implement `update_iframe_state()` method

### Phase 3: ScrollManager Integration ⏳ Not Started
- [ ] Add `check_iframe_scroll_edges()` to `ScrollManager`
- [ ] Enhance `ScrollState.is_scrolled_near_edge()` with `EdgeFlags`
- [ ] Update `tick()` to return IFrames needing updates
- [ ] Add `update_node_bounds()` calls after IFrame layout

### Phase 4: Layout Loop Updates ⏳ Not Started
- [ ] Add IFrame re-invocation check in `layout_document()`
- [ ] Handle `OptionStyledDom::None` (keep current DOM)
- [ ] Handle `OptionStyledDom::Some` (update DOM, mark dirty)
- [ ] Implement reflow loop for IFrame updates

### Phase 5: Testing ⏳ Not Started
- [ ] Unit tests for bounds expansion detection
- [ ] Unit tests for scroll edge detection  
- [ ] Unit tests for `OptionStyledDom` handling
- [ ] Integration test: Large virtualized table
- [ ] Integration test: Multiple independent IFrames
- [ ] Integration test: Nested IFrames

### Phase 6: Documentation ⏳ Not Started
- [ ] Update `IFRAME_LAYOUT_BEHAVIOR.md` with scroll behavior
- [ ] Add migration guide in CHANGELOG
- [ ] Update examples to use new API
- [ ] Add virtualization example to docs

## Current Status

### ✅ Completed
1. **Architecture designed** - Dual size model and re-invocation rules defined
2. **Documentation written** - Comprehensive doc comments added to core types
3. **Planning document created** - Full implementation guide available
4. **API designed** - `OptionStyledDom` approach for "no update" signals

### 🔄 In Progress
- Planning phase complete, ready to begin implementation

### ⏳ Next Steps
1. **Create `OptionStyledDom` type** in core module
2. **Update `IFrameCallbackReturn`** to use `OptionStyledDom`
3. **Implement state tracking** in `LayoutWindow`

## Integration with Existing Systems

### Works With Current IFrame Layout ✅
- **100% width/height defaults** - Already implemented
- **Taffy integration** - Works with percentage sizing
- **Tests passing** - Both IFrame tests green

### Builds On ScrollManager Architecture
- **ScrollManager exists** - From addscrolling.md planning
- **Scroll state tracking** - Already manages scroll positions
- **Tick system** - Already updates scroll animations

### Extends Solver3 Layout Engine
- **Incremental layout** - Re-invocation integrates with dirty tracking
- **Cache system** - IFrame DOMs stored in `layout_results`
- **Display list generation** - IFrame content merged into parent display list

## Example Use Case: Virtualized Table

### User Perspective
```
User opens app with 1000-row table
  → Sees first 20 rows immediately
  
User scrolls down
  → Smooth scrolling with accurate scrollbar
  
User reaches row 900
  → Callback invoked, rows 880-920 rendered
  → Seamless transition, no visible loading
```

### Callback Implementation
```rust
fn table_callback(data: &mut TableData, info: &mut IFrameCallbackInfo) -> IFrameCallbackReturn {
    // Calculate visible range
    let first_row = (info.scroll_offset.y / ROW_HEIGHT) as usize;
    let row_count = (info.bounds.size.height / ROW_HEIGHT).ceil() as usize + 2;
    
    // Render only visible rows
    let rows = data.fetch_rows(first_row, row_count);
    let dom = render_rows(rows);
    
    IFrameCallbackReturn {
        dom: dom.style(Css::empty()),
        
        // Actual: ~30 rows rendered (900px)
        scroll_size: LogicalSize::new(800.0, row_count as f32 * ROW_HEIGHT),
        scroll_offset: LogicalPosition::new(0.0, first_row as f32 * ROW_HEIGHT),
        
        // Virtual: All 1000 rows (30,000px)
        virtual_scroll_size: LogicalSize::new(800.0, 1000.0 * ROW_HEIGHT),
        virtual_scroll_offset: LogicalPosition::zero(),
    }
}
```

### What Happens
1. **Initial**: Renders rows 0-29 (900px of content)
2. **Scroll to row 800**: Within 200px of bottom (900px content height)
3. **Callback invoked**: Renders rows 780-820 (1200px of content)
4. **User continues**: Scrolls smoothly through newly rendered content
5. **Reach row 900**: Near edge again, callback invoked for rows 880-920

**Result**: Only ~30 rows in memory at any time, but user experiences seamless scrolling through all 1000 rows!

## Key Insights from Planning

### 1. Why Not Re-invoke on Window Shrink?
- Content is already rendered
- Scrollbars will clip the view naturally
- Re-rendering smaller content wastes CPU
- User can scroll back if they resize to large again

### 2. Why Only Once Per Expansion?
- Prevents callback spam during window resize animations
- User sees smooth resize, callback only after resize settles
- If callback returns content that covers new area, no more invocations needed
- If callback doesn't cover, it will re-invoke (user wants to see content)

### 3. Why 200px Threshold?
- Balance between:
  * Too small: Callback invoked too late, user sees empty space
  * Too large: Callback invoked too early, wastes rendering
- 200px ≈ 2-3 screens of scrolling buffer
- Gives callback time to render before user reaches edge

### 4. Why Track Edge Flags?
- Different content might grow in different directions
- Table might have more rows (vertical) but fixed columns (horizontal)
- Need to track which edges we've already invoked for
- Reset per-edge when content expands in that direction

## Comparison with HTML IFrames

### Standard HTML `<iframe>`
```html
<iframe src="large-table.html" width="800" height="600"></iframe>
```
- Loads entire page upfront
- Full DOM always in memory
- Scroll position managed by nested document
- No lazy loading unless page implements it

### Azul IFrame with Virtualization
```rust
Dom::iframe(table_data, virtualized_table_callback)
```
- Renders only visible content
- 20-30 rows in memory (vs 1000)
- Callback provides new content on demand
- Scrollbar represents full virtual size
- Seamless user experience

**Azul approach is more like:**
- React's `react-window` virtualization
- iOS `UITableView` with cell reuse
- Android `RecyclerView`

## Questions for User (Resolved via Planning)

### ❓ Should we add `OptionStyledDom` for "no update"?
**Decision**: YES
- Callback can signal "current content is fine"
- Avoids rebuilding DOM unnecessarily
- Example: Scroll down but already rendered ahead

### ❓ What threshold for "near edge"?
**Decision**: 200px (configurable)
- Empirically good balance
- Can be tuned per application
- Future: Make it a parameter on `Dom::iframe()`

### ❓ How to prevent infinite callback loops?
**Decision**: State tracking with flags
- `invoked_for_current_expansion` - Reset on new expansion
- `invoked_for_current_edge` - Reset when scroll away or content grows
- Compare bounds/scroll with `last_invoked_*` values

### ❓ Should nested IFrames be supported?
**Decision**: YES, works automatically
- Each IFrame has its own `IFrameState`
- Each IFrame's callback receives its own `info.bounds`
- Scroll states are independent
- No special handling needed

## Files Modified/Created

### Created
- ✅ `/REFACTORING/IFRAME_SCROLL_MANAGEMENT_PLAN.md` - Full architecture plan
- ✅ `/REFACTORING/SESSION_4_IFRAME_SCROLL_SUMMARY.md` - This file

### Modified
- ✅ `/core/src/callbacks.rs` - Added extensive documentation to `IFrameCallbackReturn`
- ✅ `/api/rust/src/lib.rs` - Added documentation to C API wrapper

### To Be Modified (In Implementation Phases)
- ⏳ `/core/src/dom.rs` - Add `OptionStyledDom` type
- ⏳ `/core/src/callbacks.rs` - Change `dom` field type
- ⏳ `/layout/src/window.rs` - Enhance `IFrameState`, add methods
- ⏳ `/layout/src/scroll.rs` - Add IFrame edge detection
- ⏳ `/layout/src/solver3/mod.rs` - Integrate re-invocation checks
- ⏳ `/layout/src/solver3/cache.rs` - Handle `OptionStyledDom`
- ⏳ `/layout/src/solver3/tests.rs` - Add new test scenarios

## Success Criteria

### When Implementation is Complete
- [ ] IFrame callbacks invoked only when necessary (5 rules)
- [ ] Virtualized table test with 10,000 rows passes
- [ ] Multiple IFrames work independently
- [ ] Nested IFrames function correctly
- [ ] Window resize only invokes when bounds exceed content
- [ ] Scroll only invokes when near edge (threshold-based)
- [ ] Callbacks can return "no update" via `OptionStyledDom::None`
- [ ] No infinite callback loops
- [ ] Performance: 60fps scrolling in 10,000-row table

## Next Session Goals

1. **Implement Phase 1**: Core type updates
   - Add `OptionStyledDom`
   - Update `IFrameCallbackReturn`
   - Update all tests to compile

2. **Implement Phase 2**: State tracking
   - Enhance `IFrameState`
   - Add re-invocation check methods

3. **Run tests**: Ensure existing tests still pass

## Notes for Implementation

### Remember
- **Don't break existing tests** - Both IFrame tests must stay green
- **Backward compatibility** - Provide deprecated `new()` method
- **C API compatibility** - Update bindings for `OptionStyledDom`
- **Error handling** - Callback might panic or return invalid data

### Performance Targets
- **Callback latency**: <16ms (one frame @ 60fps)
- **Memory**: O(visible_rows) not O(total_rows)
- **Invocations**: <5 per second during active scrolling

### Edge Cases to Handle
- **Zero-sized IFrame**: Don't invoke callback
- **Hidden IFrame**: Don't invoke until visible
- **Rapid resize**: Debounce or invoke once after resize settles
- **Circular references**: Parent IFrame contains child IFrame containing parent

## Conclusion

The planning phase is **complete**. We have:

✅ **Clear architecture** - Dual size model well-defined
✅ **Precise rules** - When callbacks should (and shouldn't) be invoked
✅ **State management** - IFrameState tracks all necessary information
✅ **Integration points** - ScrollManager and layout loop interactions defined
✅ **Implementation plan** - 6 phases with clear tasks
✅ **Testing strategy** - Unit and integration tests specified
✅ **Documentation** - Extensive examples and explanations

**Ready to begin implementation in next session!**

---

*This document serves as the comprehensive record of Session 4's planning work. It should be referenced during implementation to ensure all architectural decisions are followed.*
