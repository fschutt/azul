# Margin Collapsing Architecture Analysis & Design

**Author:** AI Assistant  
**Date:** 2025-11-18  
**Status:** Architecture Planning Phase  
**Target:** CSS 2.1 Section 8.3.1 Compliant Implementation

---

## Executive Summary

Margin collapsing is arguably **the most complex feature of CSS block layout**. The current implementation in `azul-layout` handles basic sibling collapsing but lacks three critical features:

1. **Empty Block Collapse-Through** - Empty blocks' top+bottom margins collapse with each other
2. **Parent-Child Margin Escape** - First/last child margins can "escape" parent's box
3. **Nested Margin Propagation** - Margins propagate through multiple nesting levels

This document analyzes the current architecture, identifies fundamental design challenges, and proposes a robust solution.

---

## Part 1: Current Implementation Analysis

### 1.1 What Works ✅

**File:** `src/solver3/fc.rs::layout_bfc()` (lines 400-545)

**Successful Features:**
- ✅ Basic sibling margin collapsing between adjacent blocks
- ✅ Border/padding blocker detection
- ✅ Positive/negative/mixed margin math via `collapse_margins()`
- ✅ Two-pass layout (sizing then positioning)

**Evidence from Tests:**
```
Test 1 (Empty Block): 
  Box1→Empty: 20px gap ✓ (max(20, 10) = 20)
  Empty→Box2: 30px gap ✓ (max(30, 15) = 30)
  → Sibling collapsing works correctly
```

**Key Code:**
```rust
// Lines 489-493: Sibling collapse
else {
    // Normal case: collapse previous bottom margin with current top margin
    advance_pen_with_margin_collapse(&mut main_pen, last_margin_bottom, child_margin_top);
}
```

### 1.2 What's Broken ❌

**Test Results Summary:**

| Test Case | Expected Gap | Actual Gap | Error |
|-----------|-------------|-----------|--------|
| Empty Block Through | 30px | 50px | +66% |
| Parent-Child Top | 30px | 50px | +66% |
| Parent-Child Bottom | 30px | 50px | +66% |
| Triple Nested | 30px | 60px | +100% |
| Multiple Empty | 30px | 75px | +150% |

**Critical Failures:**

1. **Empty blocks don't collapse through themselves**
   - Current: Empty block treated as normal sibling
   - Problem: Top (10px) and bottom (30px) margins not collapsed to 30px
   - Result: Both margins participate separately in sibling collapse

2. **Parent-child margins don't escape**
   - Current: First child's margin always added to pen
   - Problem: Margin stays "inside" parent's box
   - Result: Parent margin + child margin = double spacing

3. **No nested margin propagation**
   - Current: Each nesting level adds its own margin
   - Problem: No mechanism to propagate margins upward
   - Result: Deeply nested elements have massive spacing

---

## Part 2: The Fundamental Problem

### 2.1 The Architectural Challenge

**The Core Issue:** Margin collapsing is **non-local**.

```
Layout Flow (current):
  1. Size children (recursive, depth-first)
  2. Position children (iterative, left-to-right)
  3. Each child positioned independently
  
Problem:
  - Positioning child N requires info about child N-1 (sibling collapse) ✅
  - Positioning child N requires info about child N+1 (empty block lookahead) ❌
  - Positioning child N affects PARENT's position (margin escape) ❌
  - Child's margins can affect GRANDPARENT (nested propagation) ❌
```

### 2.2 Why Current Design Fails

**Current State Machine:**
```rust
for each child {
    1. Check if first child → handle specially
    2. Check blockers (border/padding)
    3. Collapse with previous sibling
    4. Position at pen
    5. Advance pen by child height
    6. Save child's bottom margin
}
```

**Missing State:**
- ❌ No tracking of "collapsed margins waiting to escape"
- ❌ No mechanism to "pull back" pen position
- ❌ No way to modify parent's position after the fact
- ❌ No lookahead to detect empty blocks

### 2.3 The Empty Block Problem

**CSS Spec Rule:**
> If a block element is empty (no border, padding, inline content, or height),
> its top and bottom margins collapse with each other, then collapse with
> adjacent margins.

**Why It's Hard:**
```
Box1 (margin-bottom: 20px)
  ↓
EmptyDiv (margin-top: 10px, margin-bottom: 30px)
  ↓
Box2 (margin-top: 15px)

Step-by-step collapse:
1. EmptyDiv's own margins: max(10, 30) = 30px
2. This 30px collapses with Box1's 20px: max(20, 30) = 30px
3. This 30px collapses with Box2's 15px: max(30, 15) = 30px
Total gap: 30px

Current implementation:
1. Box1→EmptyDiv: max(20, 10) = 20px ✓
2. EmptyDiv→Box2: max(30, 15) = 30px ✓
Total gap: 50px (20 + 0 + 30) ✗
```

**The Issue:** Empty blocks need **TWO collapses** (self-collapse, then sibling-collapse), but current code only does one.

### 2.4 The Parent-Child Escape Problem

**CSS Spec Rule:**
> The top margin of an element collapses with the top margin of its first
> in-flow child if the element has no top border, padding, or clearance.

**Why It's Hard:**
```
Body (padding: 20px)
  ↓
Parent (margin-top: 20px, no border/padding)
  ↓  
Child (margin-top: 30px)

Expected:
  - Child's 30px "escapes" parent's box
  - Collapses with parent's 20px
  - Result: max(20, 30) = 30px from body to child content

Current:
  - Parent positioned at Y=20 (its margin)
  - Child positioned at Y=30 (relative to parent)
  - Absolute: Y=50 (20+30) ✗
```

**The Issue:** Child's margin is used to position child **inside** parent, but should position parent **relative to grandparent**.

### 2.5 The Nested Propagation Problem

**CSS Spec Rule:**
> Margins can propagate through multiple nesting levels if no blockers exist.

**Why It's Hardest:**
```
Outer (margin-top: 10px, no blocker)
  ↓
Middle (margin-top: 20px, no blocker)
  ↓
Inner (margin-top: 30px)

Expected:
  - All three margins collapse: max(10, 20, 30) = 30px
  - Inner positioned 30px from grandgrandparent

Current:
  - Outer at Y=10
  - Middle at Y=20 (relative) = absolute 30
  - Inner at Y=30 (relative) = absolute 60 ✗
```

**The Issue:** Each layout call is independent. Middle's layout doesn't know about Outer's margin, can't propagate Inner's margin upward.

---

## Part 3: CSS 2.1 Specification Deep Dive

### 3.1 The Complete Ruleset

From CSS 2.1 Section 8.3.1 "Collapsing margins":

**Margins That Collapse:**
1. Adjacent vertical margins of block boxes in normal flow
2. Top margin of first child with parent's top margin (if no separator)
3. Bottom margin of last child with parent's bottom margin (if no separator)
4. Empty block's top and bottom margins with each other

**Margins That DON'T Collapse:**
1. Margins separated by border
2. Margins separated by padding  
3. Margins separated by clearance
4. Root element's margins
5. Margins of elements that establish new BFC (overflow, float, etc.)
6. Margins with inline content between them

**Collapse Calculation:**
- Both positive → max(a, b)
- Both negative → min(a, b)
- Mixed signs → a + b

### 3.2 Edge Cases & Gotchas

**Case 1: Multiple Empty Blocks**
```html
<div margin="20px">Content</div>
<div margin="10,25px"></div>  <!-- Empty -->
<div margin="15,30px"></div>  <!-- Empty -->
<div margin="12px">Content</div>

Collapse sequence:
1. Empty1: max(10, 25) = 25px
2. Empty2: max(15, 30) = 30px
3. All together: max(20, 25, 30, 12) = 30px
Total gap: 30px (not 20+10+25+15+30+12=112px)
```

**Case 2: Nested Escape with Blocker**
```html
<div margin="20px" border-top="1px">  <!-- Blocker! -->
  <div margin="30px">Content</div>
</div>

Result: 20px + 1px + 30px = 51px gap (no collapse)
```

**Case 3: Empty Block with Blocker**
```html
<div margin="20px">Content</div>
<div margin="10,30px" padding="1px"></div>  <!-- Not empty! -->
<div margin="15px">Content</div>

Result: 20px + 10px + 0px + 30px + 15px = 75px
(Empty block's padding makes it "non-empty", so no self-collapse)
```

**Case 4: Negative Margin Escape**
```html
<div margin-top="20px">
  <div margin-top="-10px">Content</div>
</div>

Expected: max(20, -10) = 10px (signed collapse)
```

---

## Part 4: Proposed Architecture

### 4.1 Key Insight: Margin Context

**Core Idea:** Track a "margin context" that flows through layout.

```rust
struct MarginCollapseContext {
    /// Margins waiting to collapse (can be multiple for nested cases)
    pending_top_margins: Vec<f32>,
    
    /// Whether the next margin can participate in collapsing
    can_collapse_top: bool,
    
    /// Margins from previous elements/children
    pending_bottom_margins: Vec<f32>,
    
    /// Whether a blocker has been encountered
    has_blocker: bool,
    
    /// Depth of nesting (for debugging)
    nesting_level: usize,
}
```

### 4.2 The Three-Phase Algorithm

**Phase 1: Accumulation (Top-Down)**
```rust
fn layout_bfc_with_context(
    node: &LayoutNode,
    parent_context: &MarginCollapseContext,
) -> (LayoutOutput, MarginCollapseContext) {
    let mut context = MarginCollapseContext::new();
    
    // Inherit parent's pending top margins if this is first child
    if is_first_child && !has_top_blocker(node) {
        context.pending_top_margins.extend(&parent_context.pending_top_margins);
        context.pending_top_margins.push(node.margin_top);
    }
    
    // Layout children...
}
```

**Phase 2: Collapse (At Content)**
```rust
// When we hit actual content (non-empty block or text):
fn resolve_pending_top_margins(context: &mut MarginCollapseContext) -> f32 {
    if context.pending_top_margins.is_empty() {
        return 0.0;
    }
    
    // Collapse all pending margins
    let mut result = context.pending_top_margins[0];
    for &margin in &context.pending_top_margins[1..] {
        result = collapse_margins(result, margin);
    }
    
    context.pending_top_margins.clear();
    result
}
```

**Phase 3: Propagation (Bottom-Up)**
```rust
// After laying out all children:
fn finish_bfc_layout(context: &MarginCollapseContext) -> MarginCollapseContext {
    let mut return_context = MarginCollapseContext::new();
    
    // If last child and no bottom blocker, propagate margins upward
    if !has_bottom_blocker() {
        return_context.pending_bottom_margins = context.pending_bottom_margins.clone();
        return_context.pending_bottom_margins.push(node.margin_bottom);
    }
    
    return_context
}
```

### 4.3 Algorithm Walkthrough: Empty Block

```rust
// Box1 (height=50, margin-bottom=20)
// EmptyDiv (margin-top=10, margin-bottom=30)
// Box2 (height=50, margin-top=15)

// === Box1 Layout ===
pen = 0
resolve_top_margins([]) → 0
pen += 50 (content)
pending_bottom = [20]

// === EmptyDiv Layout ===
is_empty_block = true
// Self-collapse: max(10, 30) = 30
self_collapsed = collapse_margins(10, 30) = 30

// Sibling collapse with Box1
collapsed_margin = collapse_margins(20, 30) = 30
pen += 30
pending_bottom = [30]  // Use self-collapsed margin

// === Box2 Layout ===
collapsed_margin = collapse_margins(30, 15) = 30
pen += 30
resolve_top_margins([30]) → 30
pen += 50 (content)

// Result: Box1 at 0, EmptyDiv at 50+30=80, Box2 at 80+30=110
// Gap Box1→Box2 = 110 - 50 = 60... wait, that's wrong too!
```

**Ah! The Issue:** Empty blocks shouldn't advance the pen at all!

### 4.4 Correct Empty Block Handling

```rust
if is_empty_block(child) {
    // Empty block: collapse its own margins
    let self_collapsed = collapse_margins(child_margin_top, child_margin_bottom);
    
    // Collapse with previous margin
    let total_collapsed = collapse_margins(last_margin_bottom, self_collapsed);
    
    // DON'T advance pen (no content!)
    // Just update the pending margin
    last_margin_bottom = total_collapsed;
    
    // SKIP positioning (empty block has no visual presence)
    continue;
}
```

### 4.5 Correct Parent-Child Escape

```rust
// At start of layout_bfc:
let parent_margin_top = node.box_props.margin.main_start(writing_mode);
let mut accumulated_top_margin = parent_margin_top;

// For first child:
if is_first_child && !parent_has_top_blocker && !child_has_top_blocker {
    // Accumulate margins (don't position yet)
    accumulated_top_margin = collapse_margins(accumulated_top_margin, child_margin_top);
    
    // Position child at 0 (margin "escapes" to parent's parent)
    child_main_pos = 0.0;
    
    // IMPORTANT: Return accumulated_top_margin to parent
    // So parent's parent can apply it
} else {
    // Normal case: resolve accumulated margin, position child
    main_pen += accumulated_top_margin;
    accumulated_top_margin = 0.0;
    main_pen += child_margin_top;
    child_main_pos = main_pen;
}
```

### 4.6 Handling Nested Propagation

**Key Insight:** Use return values to propagate margins.

```rust
struct BfcLayoutResult {
    positions: BTreeMap<usize, LogicalPosition>,
    overflow_size: LogicalSize,
    
    // NEW: Escaped margins
    escaped_top_margin: Option<f32>,
    escaped_bottom_margin: Option<f32>,
}

fn layout_bfc(...) -> Result<BfcLayoutResult> {
    // ... layout children ...
    
    let mut result = BfcLayoutResult::default();
    
    // If first child has escaped margin and we have no top blocker
    if let Some(child_escaped) = first_child_result.escaped_top_margin {
        if !parent_has_top_blocker {
            // Propagate upward, collapsing with our own margin
            result.escaped_top_margin = Some(
                collapse_margins(node.margin_top, child_escaped)
            );
        }
    }
    
    // Similar for bottom margin...
    
    Ok(result)
}
```

---

## Part 5: Implementation Strategy

### 5.1 Incremental Rollout (Phase-by-Phase)

**Phase 1: Empty Block Self-Collapse (EASIEST)**
- ✅ Detection already works (`is_empty_block()`)
- ➕ Add self-collapse logic before sibling collapse
- ➕ Skip pen advancement for empty blocks
- ⚠️ Risk: Low - localized change

**Phase 2: Parent-Child Top Escape (MEDIUM)**
- ➕ Add `accumulated_top_margin` tracking
- ➕ Modify first-child handling
- ➕ Add `escaped_top_margin` return value
- ⚠️ Risk: Medium - affects positioning logic

**Phase 3: Parent-Child Bottom Escape (MEDIUM)**
- ➕ Track last child
- ➕ Defer bottom margin resolution
- ➕ Add `escaped_bottom_margin` return value
- ⚠️ Risk: Medium - affects size calculation

**Phase 4: Nested Propagation (HARDEST)**
- ➕ Thread `MarginCollapseContext` through recursive calls
- ➕ Modify all `calculate_layout_for_subtree` call sites
- ➕ Handle context merging for multiple children
- ⚠️ Risk: High - pervasive architectural change

### 5.2 Testing Strategy

**Test Pyramid:**
```
Level 3: Integration Tests (6 comprehensive PDFs) ← Current
Level 2: Component Tests (27 unit tests) ← Stubbed
Level 1: Core Function Tests (8 tests) ← Passing
```

**Per-Phase Validation:**
1. Run all existing tests (regression check)
2. Enable specific stubbed tests for that phase
3. Generate PDF and measure gaps
4. Compare with browser rendering

### 5.3 Rollback Strategy

**Safety Net:**
- ✅ Existing tests pass (baseline)
- ✅ Feature flag: `ENABLE_ADVANCED_MARGIN_COLLAPSE`
- ✅ Keep old code path for comparison
- ✅ Comprehensive logging with `DEBUG_MARGIN_COLLAPSE`

---

## Part 6: Alternative Approaches Considered

### 6.1 Post-Layout Adjustment (REJECTED)

**Idea:** Position everything normally, then scan for escaped margins and adjust.

**Pros:**
- Non-invasive to layout algorithm
- Easy to understand

**Cons:**
- ❌ Requires multiple passes (performance)
- ❌ Complex tree traversal to find what to adjust
- ❌ Hard to handle nested escapes correctly
- ❌ Doesn't match browser implementation mental model

### 6.2 Separate Margin Resolution Pass (REJECTED)

**Idea:** Three passes: (1) size, (2) resolve margins, (3) position.

**Pros:**
- Clean separation of concerns
- Matches some browser implementations

**Cons:**
- ❌ Requires storing intermediate state
- ❌ Hard to integrate with existing two-pass system
- ❌ Unclear how to handle position-dependent margins (floats, etc.)

### 6.3 Inline Context Propagation (SELECTED)

**Idea:** Thread a `MarginCollapseContext` through layout calls.

**Pros:**
- ✅ Matches CSS spec mental model
- ✅ Incremental implementation possible
- ✅ Explicit state tracking (easier to debug)
- ✅ Natural fit for recursive layout algorithm

**Cons:**
- ⚠️ Requires refactoring function signatures
- ⚠️ More complex state management

---

## Part 7: Open Questions & Future Work

### 7.1 Unresolved Issues

**Q1: Clearance and Margin Collapsing**
> CSS spec: "Clearance prevents margin collapsing"
> How to detect clearance? When does it apply?

**A1:** Check for `clear` property and preceding float. If clearance is inserted, set `has_blocker = true`.

**Q2: BFC Establishment**
> Elements with `overflow: hidden`, `float`, `position: absolute` establish new BFC.
> Do their margins participate in collapsing?

**A2:** No. Add `establishes_bfc()` check, treat as blocker.

**Q3: Writing Modes**
> Current code uses `main_axis` for block direction.
> Does margin collapsing work for `writing-mode: vertical-*`?

**A3:** Should work since we use logical properties. Needs testing.

**Q4: Min/Max Height Constraints**
> If a parent has `min-height` but children don't fill it, do bottom margins escape?

**A4:** Per spec, no. Empty space counts as "content" preventing escape.

### 7.2 Performance Considerations

**Current:** O(n) single pass per BFC
**Proposed:** O(n) but with more complex bookkeeping

**Potential Optimizations:**
1. Cache `is_empty_block` results
2. Pre-compute blocker flags during cascade
3. Use small-vec for pending margins (most cases have ≤3)
4. Short-circuit when no escape possible (top blocker detected)

**Profiling Needed:**
- Measure impact of Vec allocations
- Compare with browser implementations (Firefox, Chromium)

### 7.3 Future Enhancements

**CSS Level 3+ Features:**
1. `margin-trim` property (Chrome 117+)
2. Logical properties (margin-block-start, etc.)
3. Container queries affecting margin collapse (?)

---

## Part 8: Recommendations

### 8.1 Immediate Next Steps (This Sprint)

**Priority 1: Empty Block Self-Collapse**
- Estimated effort: 2-4 hours
- High impact, low risk
- Enables Test 1 and Test 5 to pass

**Implementation:**
```rust
// In layout_bfc positioning loop:
if is_empty_block(child_node) && !child_has_top_blocker {
    let self_collapsed = collapse_margins(child_margin_top, child_margin_bottom);
    let with_prev = collapse_margins(last_margin_bottom, self_collapsed);
    last_margin_bottom = with_prev;
    continue; // Skip positioning, skip pen advance
}
```

**Success Criteria:**
- ✅ Test 1 gap: 50px → 30px
- ✅ Test 5 gap: 75px → 30px
- ✅ No regression in other tests

### 8.2 Medium-Term Goals (Next 2-3 Sprints)

**Priority 2: Parent-Child Escape**
- Estimated effort: 1-2 weeks
- Medium risk (affects core positioning)
- Requires careful design of return value propagation

**Priority 3: Nested Propagation**
- Estimated effort: 2-3 weeks
- High risk (pervasive changes)
- Should be done after escape is solid

### 8.3 Long-Term Vision

**Goal:** Feature-complete CSS 2.1 block layout

**Remaining Features:**
- ✅ Margin collapsing (this document)
- ⏳ Float positioning (partially done)
- ⏳ Clearance (stubbed)
- ❌ Min/max size constraints (done?)
- ❌ Baseline alignment in inline context
- ❌ Text-align: justify

---

## Part 9: Code Locations Reference

### 9.1 Files to Modify

**Primary:**
- `src/solver3/fc.rs::layout_bfc()` (lines 400-545)
  - Main positioning loop
  - Current margin collapse implementation

**Supporting:**
- `src/solver3/fc.rs::collapse_margins()` (line 3103)
  - Core collapse math (already correct)
  
- `src/solver3/fc.rs::advance_pen_with_margin_collapse()` (line 3140)
  - Helper for sibling collapse
  
- `src/solver3/fc.rs::is_empty_block()` (line 3205)
  - Empty block detection
  
- `src/solver3/fc.rs::has_margin_collapse_blocker()` (line 3170)
  - Border/padding blocker check

**Testing:**
- `tests/margin_collapsing.rs`
  - 27 unit tests (8 passing, 19 stubbed)

### 9.2 Function Signature Changes Needed

**Current:**
```rust
fn layout_bfc<T, Q>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree,
    text_cache: &mut BTreeMap<NodeId, InlineCache>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput, LayoutError>
```

**Proposed (Phase 4):**
```rust
fn layout_bfc<T, Q>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree,
    text_cache: &mut BTreeMap<NodeId, InlineCache>,
    node_index: usize,
    constraints: &LayoutConstraints,
    margin_context: &MarginCollapseContext,  // NEW
) -> Result<BfcLayoutResult, LayoutError>  // CHANGED

struct BfcLayoutResult {
    output: LayoutOutput,
    escaped_top_margin: Option<f32>,     // NEW
    escaped_bottom_margin: Option<f32>,  // NEW
}
```

---

## Part 10: Conclusion

Margin collapsing is complex because it's **inherently non-local**. The current architecture works for the 80% case (sibling collapsing) but fails for the hard 20%:

1. **Empty blocks** - Need TWO collapse operations (self, then sibling)
2. **Parent-child escape** - Need UPWARD propagation of margins
3. **Nested propagation** - Need margins to flow through MULTIPLE levels

The proposed solution uses **context threading** to track pending margins as they flow through layout. This matches the CSS spec's mental model and enables incremental implementation.

**Recommended Approach:**
1. ✅ Start with empty blocks (low risk, high impact)
2. ⏳ Add parent-child escape (medium risk)
3. ⏳ Implement nested propagation (high complexity)

With careful testing and incremental rollout, we can achieve full CSS 2.1 compliance without destabilizing the existing layout engine.

---

**End of Document**

*This architecture is based on analysis of:*
- *CSS 2.1 Specification Section 8.3.1*
- *Current azul-layout implementation (solver3)*
- *Test results from 6 comprehensive test cases*
- *Browser implementations (Firefox, Chromium) behavior*
