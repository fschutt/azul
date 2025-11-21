# Inline Layout Analysis: Display List Generation Bug

**Date:** 2025-11-21  
**Component:** azul-layout `solver3` module  
**Issue:** Missing text in PDF rendering due to inline spans being skipped during display list generation

---

## Executive Summary

The azul-layout engine has a critical bug in `collect_and_measure_inline_content()` (fc.rs:2905) where **inline spans with `display: inline`** are completely skipped during inline content collection. This causes their text content to be lost, resulting in missing text in the final rendered output.

### Impact
- **Severity:** HIGH
- **Affected:** All inline spans (`<span>`, `<em>`, `<strong>`, etc.) with explicit or implicit `display: inline`
- **Symptoms:** 
  - Text inside inline spans disappears from rendered output
  - Background colors on inline spans show as 0×0 rectangles
  - PDF generation missing large portions of text

---

## Bug Location

**File:** `/Users/fschutt/Development/azul/layout/src/solver3/fc.rs`  
**Function:** `collect_and_measure_inline_content()`  
**Lines:** 3025-3160 (approximately)

### Current Buggy Code

```rust
for (item_idx, &dom_child_id) in dom_children.iter().enumerate() {
    let node_data = &ctx.styled_dom.node_data.as_container()[dom_child_id];

    // ✓ CASE 1: Text nodes - handled correctly
    if let NodeType::Text(ref text_content) = node_data.get_node_type() {
        eprintln!("[collect] ✓ Found text node: '{}'", text_content.as_str());
        content.push(InlineContent::Text(StyledRun {
            text: text_content.to_string(),
            style: Arc::new(get_style_properties(ctx.styled_dom, ifc_root_dom_id)),
            logical_start_byte: 0,
        }));
        continue;  // ✓ Correct
    }

    // Find layout tree node for this DOM child
    let Some(child_index) = children.iter().find(/*...*/) else {
        eprintln!("[collect] WARNING: DOM child {:?} has no layout node", dom_child_id);
        continue;
    };

    let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
    let dom_id = child_node.dom_node_id.unwrap();

    // ✓ CASE 2: Non-inline elements (inline-block, etc.) - handled correctly
    if get_display_property(ctx.styled_dom, Some(dom_id)).unwrap_or_default() != LayoutDisplay::Inline {
        // Handle inline-block: measure it, add as Shape
        let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
        let width = intrinsic_size.max_content_width;
        // ... layout the inline-block ...
        content.push(InlineContent::Shape(/*...*/));
        child_map.insert(content_index, child_index);
    } 
    // ✓ CASE 3: Images - handled correctly
    else if let NodeType::Image(image_data) = /*...*/ {
        content.push(InlineContent::Image(/*...*/));
        child_map.insert(content_index, child_index);
    }
    
    // ❌ BUG: What about CASE 4: display:inline spans with text children?
    // They fall through here and are SILENTLY SKIPPED!
    // No recursion, no text extraction, NOTHING!
}
```

---

## Root Cause Analysis

### 1. **Missing Recursion for Inline Elements**

The function iterates over **direct DOM children** of the IFC root, but:

- **Text nodes** (`NodeType::Text`) are added directly ✓
- **Inline-block** elements are measured and added as shapes ✓  
- **Images** are added as inline images ✓
- **Inline spans** (`display: inline`) → **FALL THROUGH AND ARE IGNORED** ❌

### 2. **Style Inheritance Not Implemented**

For inline spans with different styles (e.g., `<span style="color: red">`), the code should:
1. Recursively descend into the inline span's children
2. Collect text nodes with the span's inherited style
3. Build styled runs that reflect style boundaries

**Current behavior:** The span and all its children are skipped entirely.

### 3. **Background Rendering Disconnected**

Inline span backgrounds are rendered in `paint_node()` in `display_list.rs`, but since the span has no intrinsic size during layout (it's supposed to wrap its content), the background becomes a **0×0 rectangle** that gets skipped by the bridge.

---

## Detailed Flow Analysis

### Example HTML
```html
<p>This is <span class="highlight">important</span> text.</p>
```

### DOM Structure
```
<p> (IFC root, display: block)
├─ #text "This is "
├─ <span class="highlight"> (display: inline)
│  └─ #text "important"
└─ #text " text."
```

### Current (Buggy) Behavior

**collect_and_measure_inline_content() iteration:**

1. **Item 0:** Text node "This is " → ✓ Added as `InlineContent::Text`
2. **Item 1:** `<span>` element → Has layout node, `display: inline`
   - Doesn't match `!= LayoutDisplay::Inline` condition
   - Not an image
   - **Falls through → NOTHING HAPPENS**
3. **Item 2:** Text node " text." → ✓ Added as `InlineContent::Text`

**Result:** Only "This is  text." appears (no "important")

### Display List Output

```
[7] TextLayout: bounds=555.27×18.64 @ (20, 74.88)
[8] Text: 15 glyphs  // "This is  text." (notice double space)
[9] Rect: bounds=0×0 @ (0, 0)  // <span> background, but no size!
```

The `<span>` has a background color, so `paint_node()` adds a Rect to the display list. But since the span has no laid-out size (because its content was never measured), the rect is **0×0** and gets skipped by printpdf's bridge.

---

## Comparison with CSS Specification

### CSS Display Module Level 3

> **Inline-level boxes** participate in an inline formatting context. Inline-level boxes that 
> are not inline boxes (such as replaced elements, inline-block elements, and inline-table 
> elements) are called **atomic inline-level boxes** because they participate in their inline 
> formatting context as a single opaque box.

### Expected Behavior per Spec

1. **Inline boxes** (e.g., `<span>`) should:
   - Be transparent wrappers around their content
   - Inherit styles from parents and apply them to descendants
   - Not establish new formatting contexts
   - Have no intrinsic size (wrap their content)

2. **Text within inline boxes** should:
   - Be collected recursively
   - Carry style information from the inline box
   - Participate in line breaking with surrounding text

3. **Inline-block boxes** should:
   - Be laid out independently as atomic units
   - Have intrinsic size based on their content
   - Not break across lines

### Azul Implementation vs. Spec

| Feature | CSS Spec | Azul Current | Status |
|---------|----------|--------------|--------|
| Text nodes in IFC root | Collected | ✓ Collected | ✓ CORRECT |
| Inline-block layout | Laid out as atomic box | ✓ Recursively laid out | ✓ CORRECT |
| Images in inline context | Atomic inline-level | ✓ Handled | ✓ CORRECT |
| **Inline spans (transparent)** | **Recurse + inherit style** | **❌ SKIPPED** | **❌ BUG** |
| Style inheritance in runs | Per-span styles | ❌ Only IFC root style | ❌ BUG |

---

## Related Code Locations

### 1. Inline Content Collection (`fc.rs`)

**Function:** `collect_and_measure_inline_content()`  
**Lines:** 2905-3160

**Purpose:** Gather all inline content from an IFC into `Vec<InlineContent>` for text3 engine.

**Issues:**
- No recursion for `display: inline` elements
- Style inheritance only from IFC root
- Missing `else` branch for inline spans

### 2. Display List Generation (`display_list.rs`)

**Function:** `paint_inline_content()`  
**Lines:** 1435-1650

**Purpose:** Convert text3's `UnifiedLayout` into display list items (TextLayout, Text, decorations).

**Issues:**
- Assumes all inline content is already collected (doesn't handle missing content)
- Background rendering happens in `paint_node()`, which uses layout tree bounds
- Inline spans with no bounds → 0×0 rects

**Function:** `paint_node()`  
**Lines:** ~800-1100

**Purpose:** Paint backgrounds, borders, and other box-model decorations.

**Issues:**
- Paints backgrounds for inline spans based on `used_size`
- If `used_size` is not set (because content wasn't collected), background is 0×0

### 3. Intrinsic Sizing (`sizing.rs`)

**Function:** `collect_inline_content_for_sizing()`  
**Lines:** 437-600

**Purpose:** Collect inline content for intrinsic size calculation (min/max-content widths).

**Issues:**
- **Has the same bug!** No recursion for inline spans
- This causes inline spans to have 0 intrinsic width, which may cascade to other layout bugs

---

## Float and Clear Interaction

The report was requested to analyze float/clear behavior. Here's the analysis:

### Float Handling in IFC

**Function:** `position_floated_child()` in `fc.rs` (lines 3165-3250)

**How it works:**
1. Floats are positioned in the **BFC** (Block Formatting Context), not the IFC
2. When an IFC encounters a float child, it's positioned in the parent BFC's floating context
3. The IFC's available line box space is reduced by the floating context

**Inline content and floats:**
- Floats **do not** become part of `InlineContent` for text3
- They are positioned separately in the BFC
- Text3 receives adjusted `UnifiedConstraints` with reduced `available_width` based on float intrusions

**Current status:** ✓ Float positioning appears correct (separate from this inline bug)

### Clear Handling

**Location:** `layout_bfc()` in `fc.rs`

**How it works:**
1. When a child has `clear: left/right/both`, the BFC positions it below all relevant floats
2. Clear only affects **block-level** children, not inline content
3. Text wrapping around floats is handled by text3's line breaking algorithm

**Current status:** ✓ Clear appears to work correctly for block children

### Inline-Block and Floats

**Relevant code:** Lines 3073-3125 in `collect_and_measure_inline_content()`

```rust
if get_display_property(...) != LayoutDisplay::Inline {
    // This is an atomic inline-level box (e.g., inline-block, image).
    let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
    let width = intrinsic_size.max_content_width;
    
    // Recursively lay out the inline-block to get its final height and baseline.
    let layout_output = layout_formatting_context(ctx, tree, text_cache, child_index, &child_constraints)?;
    
    // Add as Shape to inline content
    content.push(InlineContent::Shape(InlineShape { /*...*/ }));
}
```

**Analysis:**
- Inline-blocks establish a new BFC (line 3095: `bfc_state: None`)
- This correctly isolates their internal floats from the parent IFC
- Floats inside an inline-block don't affect text outside it ✓

**Current status:** ✓ Inline-block float isolation is correct

---

## Test Cases Demonstrating the Bug

### Test Case 1: Inline Span with Text

**HTML:**
```html
<p>Before <span>inside</span> after</p>
```

**Expected:** "Before inside after"  
**Actual:** "Before  after" (double space, "inside" missing)

### Test Case 2: Nested Inline Spans

**HTML:**
```html
<p>Text <span>outer <span>inner</span> outer2</span> end</p>
```

**Expected:** "Text outer inner outer2 end"  
**Actual:** "Text  end" (all span content missing)

### Test Case 3: Inline Span with Background

**HTML:**
```html
<p>This is <span style="background: yellow">highlighted</span> text</p>
```

**Expected:** "This is highlighted text" with yellow background behind "highlighted"  
**Actual:** "This is  text" with 0×0 yellow rectangle (invisible)

### Test Case 4: Inline-Block (Should Work)

**HTML:**
```html
<p>Text <span style="display: inline-block; width: 50px; height: 20px; background: red;"></span> after</p>
```

**Expected:** "Text [red box] after"  
**Actual:** ✓ "Text [red box] after" (this DOES work, because inline-block is handled)

---

## Proposed Fix

### Strategy

The fix requires modifying `collect_and_measure_inline_content()` to **recursively process inline spans** and their text children with proper style inheritance.

### High-Level Approach

```rust
if display == LayoutDisplay::Inline {
    // This is a transparent inline wrapper (e.g., <span>, <em>, <strong>)
    // We must recursively collect its children with inherited style
    
    let span_style = get_style_properties(ctx.styled_dom, dom_id);
    
    // Recursively collect inline content from this span's children
    collect_inline_span_content(
        ctx,
        tree,
        dom_id,
        span_style,
        &mut content,
    )?;
    
    // Note: The span itself doesn't become an InlineContent item
    // Its children (text nodes, nested spans, etc.) are added directly
}
```

### Detailed Implementation

**Option 1: Helper Function (Recommended)**

Add a new helper function:

```rust
fn collect_inline_span_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    span_dom_id: NodeId,
    inherited_style: StyleProperties,
    content: &mut Vec<InlineContent>,
) -> Result<()> {
    let node_hier = &ctx.styled_dom.node_hierarchy.as_container()[span_dom_id];
    
    // Iterate over span's DOM children
    for child_id in node_hier.first_child..=node_hier.last_child {
        let child_node_data = &ctx.styled_dom.node_data.as_container()[child_id];
        
        match child_node_data.get_node_type() {
            NodeType::Text(ref text) => {
                // Add text with span's style
                content.push(InlineContent::Text(StyledRun {
                    text: text.to_string(),
                    style: Arc::new(inherited_style.clone()),
                    logical_start_byte: 0,
                }));
            }
            _ => {
                // Nested element: check its display type
                let child_display = get_display_property(ctx.styled_dom, Some(child_id))
                    .unwrap_or_default();
                
                if child_display == LayoutDisplay::Inline {
                    // Nested inline span: recurse
                    let child_style = get_style_properties(ctx.styled_dom, child_id);
                    collect_inline_span_content(ctx, tree, child_id, child_style, content)?;
                } else {
                    // Inline-block or other: handle as before
                    // (measure and add as Shape/Image)
                }
            }
        }
    }
    
    Ok(())
}
```

**Option 2: Refactor Main Loop**

Restructure the main loop to handle all cases:

```rust
for (item_idx, &dom_child_id) in dom_children.iter().enumerate() {
    // ... existing setup ...
    
    match node_data.get_node_type() {
        NodeType::Text(ref text_content) => {
            // Existing text handling
            content.push(InlineContent::Text(/*...*/));
        }
        _ => {
            let display = get_display_property(ctx.styled_dom, Some(dom_child_id))
                .unwrap_or_default();
            
            match display {
                LayoutDisplay::Inline => {
                    // NEW: Handle inline spans
                    let span_style = get_style_properties(ctx.styled_dom, dom_child_id);
                    collect_inline_span_content(ctx, tree, dom_child_id, span_style, &mut content)?;
                }
                LayoutDisplay::InlineBlock => {
                    // Existing inline-block handling
                    // ... measure and add as Shape ...
                }
                _ => {
                    // Other display types
                }
            }
        }
    }
}
```

---

## Testing Strategy

### Unit Tests Needed

1. **Test inline span text collection**
   - Verify text inside `<span>` is collected
   - Verify style inheritance works correctly
   - File: `azul/layout/src/solver3/tests/test_inline_span_collection.rs`

2. **Test nested inline spans**
   - Multiple levels: `<span><span><span>text</span></span></span>`
   - Style changes at each level
   - File: Same as above

3. **Test inline span backgrounds**
   - Verify span gets sized based on its content
   - Verify background rect has non-zero bounds
   - File: `azul/layout/src/solver3/tests/test_inline_backgrounds.rs`

4. **Test mixed inline content**
   - Text + inline spans + inline-blocks + more text
   - Verify correct ordering and positioning
   - File: Same as test 1

### Integration Tests

1. **PDF rendering test**
   - HTML with inline spans → PDF
   - Verify all text appears in output
   - File: `printpdf/tests/test_inline_spans.rs`

2. **Display list verification**
   - Check that display list contains all expected glyphs
   - No 0×0 rectangles for non-empty spans
   - File: `azul/layout/tests/test_display_list_inline.rs`

### Regression Tests

**Verify these still work after the fix:**
- Inline-blocks (shouldn't be affected)
- Floats and clears (separate system)
- Line breaking and wrapping
- Text decorations (underline, etc.)

---

## Implementation Checklist

- [ ] Create unit test for inline span collection (RED)
- [ ] Implement `collect_inline_span_content()` helper
- [ ] Modify main loop in `collect_and_measure_inline_content()` to call helper for inline spans
- [ ] Verify unit test passes (GREEN)
- [ ] Create integration test for PDF rendering
- [ ] Test with printpdf html_inline_debug example
- [ ] Verify no regressions in existing tests
- [ ] Update documentation in function comments
- [ ] Consider performance implications (recursion depth)
- [ ] Handle edge cases:
  - [ ] Empty inline spans
  - [ ] Deeply nested spans (recursion limit?)
  - [ ] Inline spans with pseudo-elements (::before, ::after)
  - [ ] Right-to-left text in inline spans

---

## Additional Considerations

### Performance

**Recursion Depth:** Deeply nested inline spans could cause stack overflow. Consider:
- Iterative approach with explicit stack
- Recursion depth limit with error handling
- Typical HTML has <10 levels, should be fine

**Memory:** Style cloning for each span could be expensive. Consider:
- Arc-wrapped styles (already used)
- Style diff tracking (only store changes)
- Style cache/interning

### Style Boundaries

Currently, `InlineContent::Text` has a single `style: Arc<StyleProperties>`. For rich text with multiple style changes, text3 needs to:

1. Split text into styled runs at style boundaries
2. Each run has its own style
3. Line breaking must respect run boundaries (can't break mid-glyph)

**Current text3 support:** Appears to handle this already (StyledRun has logical_start_byte).

### Background Rendering

With proper content collection, inline spans will have:
- Non-zero intrinsic size (width = sum of content widths)
- Proper `used_size` set after layout

Then `paint_node()` will correctly render:
- Background rect with actual dimensions
- Border around the span (if specified)

**Important:** Inline spans can **wrap across lines**. CSS spec requires backgrounds to have separate rects for each line fragment. Current implementation may not handle this yet (needs investigation).

---

## Conclusion

The azul-layout engine has a critical gap in its inline content collection logic. The fix is straightforward (add recursion for inline spans) but requires careful attention to style inheritance and testing. The float/clear systems are independent and appear to be working correctly.

**Priority:** HIGH - This breaks basic text rendering in common HTML patterns.

**Estimated Effort:** 2-4 hours (implementation + tests)

**Complexity:** Medium (recursion logic, style handling)

**Risk:** Low - Well-isolated change, easy to test
