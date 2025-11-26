# CSS Property Inheritance Problem Report

**Date:** November 15, 2025  
**Author:** GitHub Copilot  
**Status:** Analysis & Design Proposal

## Executive Summary

The current CSS property resolution system has a critical architectural limitation: **inherited properties (like `font-weight: bold`) set on parent elements are not properly propagated to anonymous text nodes**. This breaks fundamental CSS inheritance as specified in CSS 2.1 Section 6.2.

### Problem Scenario

```html
<p style="font-weight: bold">This should be bold</p>
```

**Current behavior:**
- The `<p>` element has `font-weight: bold` in its CSS property cache
- The anonymous `p::text` node (containing "This should be bold") **does not** see this property
- Text renders with `font-weight: normal` (default)
- Result: **Text is not bold** ❌

**Expected behavior:**
- The `p::text` node should inherit `font-weight: bold` from its `<p>` parent
- Text should render bold ✅

---

## Root Cause Analysis

### 1. Current Architecture

The `CssPropertyCache` in `/Users/fschutt/Development/azul/core/src/prop_cache.rs` implements property resolution via:

```rust
pub fn get_property<'a>(
    &'a self,
    node_data: &'a NodeData,
    node_id: &NodeId,
    node_state: &StyledNodeState,
) -> Option<&'a CssProperty>
```

This method:
1. Checks if the property is explicitly set on the node (inline styles, CSS rules)
2. Returns `None` if not found
3. **Does NOT walk up the tree for inherited properties**

### 2. Why Text Nodes Fail

Anonymous text nodes (e.g., `p::text`) are created during DOM parsing but:
- Have **no CSS rules** targeting them (they're not real DOM elements)
- Have **no inline styles** (they can't have attributes)
- Are **children** of styled elements, but inheritance is not implemented

When the text layout engine calls:
```rust
styled_dom.css_property_cache.get_font_weight(node_data, &text_node_id, node_state)
```

It receives `None` because:
1. No CSS rule matches `p::text`
2. The cache doesn't check the parent `<p>` element
3. Default value (`font-weight: normal`) is used

### 3. Current Workaround Location

In `/Users/fschutt/Development/azul/layout/src/solver3/fc.rs` at line ~270, there's a partial workaround:

```rust
fn get_style_properties_with_context(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    node_index: usize,
) -> Arc<StyleProperties> {
    // Resolve inherited properties by walking the tree
    let font_size = get_resolved_font_size(tree, styled_dom, node_index);
    
    // BUT: Other inherited properties are NOT resolved this way!
    let font_family_name = cache
        .get_font_family(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        // ...
        .unwrap_or_else(|| "sans-serif".to_string()); // Default, not inherited!
}
```

**This is insufficient because:**
- Only `font_size` uses proper inheritance via `get_resolved_font_size()`
- Other properties like `font-weight`, `color`, `font-style` use `.unwrap_or(default)` instead of walking the tree
- It's duplicated logic (should be in the property cache)

---

## CSS 2.1 Specification Reference

### Section 6.2 - Inheritance

> "Some values are inherited by the children of an element in the document tree. Each property defines whether it is inherited or not."

**Inherited properties include:**
- `color`
- `font-family`, `font-size`, `font-style`, `font-weight`, `font-variant`
- `letter-spacing`, `line-height`, `word-spacing`
- `text-align`, `text-indent`, `text-transform`
- `white-space`
- `direction`, `visibility`
- And many others...

### Section 6.1.1 - Specified Values

> "If the cascade results in a value, use it. Except that, if the value is 'inherit', the specified value is defined in 'Inheritance' below. **Otherwise, if the property is inherited, use the parent element's computed value**."

**This is the behavior we're missing!**

---

## Proposed Solutions

### Option A: Cache-Level Inheritance (Recommended) ⭐

**Implementation:** Modify `CssPropertyCache::get_property()` to automatically walk up the tree for inherited properties.

**Advantages:**
- ✅ Centralized logic (DRY principle)
- ✅ All code using the cache benefits automatically
- ✅ Spec-compliant by default
- ✅ No duplicate inheritance logic in multiple places

**Disadvantages:**
- ⚠️ Performance impact (tree traversal on cache miss)
- ⚠️ Requires knowing parent relationships (needs `styled_dom` context)

**Pseudocode:**
```rust
impl CssPropertyCache {
    pub fn get_property_with_inheritance<'a>(
        &'a self,
        styled_dom: &'a StyledDom,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        property_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {
        let node_data = &styled_dom.node_data.as_container()[*node_id];
        
        // 1. Try to get directly from this node
        if let Some(prop) = self.get_property(node_data, node_id, node_state) {
            return Some(prop);
        }
        
        // 2. If property is inherited, walk up the tree
        if property_type.is_inherited() {
            if let Some(parent_id) = styled_dom.get_parent(*node_id) {
                return self.get_property_with_inheritance(
                    styled_dom, 
                    &parent_id, 
                    node_state, // Or parent's state?
                    property_type
                );
            }
        }
        
        None
    }
}
```

**Required changes:**
1. Add `CssPropertyType::is_inherited() -> bool` method
2. Add `get_property_with_inheritance()` method to cache
3. Update all call sites to use the new method
4. Cache parent lookups to avoid repeated traversals

---

### Option B: Layout-Level Inheritance

**Implementation:** Keep inheritance logic in the layout engine, but fix all properties (not just `font-size`).

**Advantages:**
- ✅ No changes to core CSS cache
- ✅ Can optimize based on layout tree structure

**Disadvantages:**
- ❌ Duplicates logic (every layout subsystem needs its own inheritance)
- ❌ Easy to miss properties (current `font-weight` bug proves this)
- ❌ Violates separation of concerns (layout shouldn't know about CSS inheritance)

**Current state:**
- Already partially implemented for `font-size` in `get_resolved_font_size()`
- Needs extension to all inherited properties
- Currently broken for `font-weight`, `font-style`, `color`, etc.

---

### Option C: Computed Value Cache

**Implementation:** Build a separate "computed values" cache that pre-resolves all inherited properties during styling phase.

**Advantages:**
- ✅ Best performance (inheritance resolved once)
- ✅ Clean separation (computed vs. cascaded values)
- ✅ Matches CSS specification terminology

**Disadvantages:**
- ⚠️ Requires full architecture refactor
- ⚠️ Memory overhead (stores computed values for all nodes)
- ⚠️ Complex invalidation on style changes

**Note:** This is the "correct" long-term architecture per CSS specs, but requires significant refactoring.

---

## Recommended Implementation Plan

### Phase 1: Quick Fix (Current Sprint)

**Goal:** Make text nodes work with inherited properties

1. **Extend `get_style_properties_with_context()` in `fc.rs`:**
   - Add proper inheritance for `font-weight`, `font-style`, `color`
   - Similar to existing `get_resolved_font_size()` helper
   - Quick fix, minimal risk

2. **Create helper functions:**
   ```rust
   fn get_resolved_font_weight(tree: &LayoutTree, styled_dom: &StyledDom, node_index: usize) -> StyleFontWeight
   fn get_resolved_color(tree: &LayoutTree, styled_dom: &StyledDom, node_index: usize) -> StyleTextColor
   fn get_resolved_font_style(tree: &LayoutTree, styled_dom: &StyledDom, node_index: usize) -> StyleFontStyle
   // etc.
   ```

3. **Update `get_style_properties_with_context()`** to use these helpers instead of `.unwrap_or(default)`

**Timeline:** 1-2 days  
**Risk:** Low (localized changes)

---

### Phase 2: Cache-Level Inheritance (Next Sprint)

**Goal:** Implement proper inheritance in `CssPropertyCache`

1. **Add `CssPropertyType::is_inherited()` method:**
   ```rust
   impl CssPropertyType {
       pub const fn is_inherited(&self) -> bool {
           match self {
               CssPropertyType::FontFamily
               | CssPropertyType::FontSize
               | CssPropertyType::FontWeight
               | CssPropertyType::FontStyle
               | CssPropertyType::TextColor
               | CssPropertyType::LineHeight
               | CssPropertyType::LetterSpacing
               | CssPropertyType::WordSpacing
               | CssPropertyType::TextAlign
               | CssPropertyType::Direction
               | CssPropertyType::Visibility
               | CssPropertyType::WhiteSpace
               | CssPropertyType::Hyphens
               // ... (see CSS 2.1 spec for full list)
               => true,
               _ => false,
           }
       }
   }
   ```

2. **Add parent relationship access to styled_dom:**
   ```rust
   impl StyledDom {
       pub fn get_parent(&self, node_id: NodeId) -> Option<NodeId> {
           // Use existing node hierarchy data
       }
   }
   ```

3. **Implement `get_property_with_inheritance()` as shown in Option A**

4. **Deprecate layout-level inheritance helpers** (from Phase 1)

5. **Update all call sites** to use new cache method

**Timeline:** 1-2 weeks  
**Risk:** Medium (affects core system)

---

### Phase 3: Full Computed Values Cache (Future)

**Goal:** Implement CSS 2.1 Section 6.1-6.2 properly

This is a major refactor for CSS 2.2/3 compliance. Deferred until layout engine stabilizes.

---

## Immediate Action Items

### For fixing `font-weight: bold` specifically:

1. ✅ **Remove debug borders** (Task #1) - DONE
2. 🔄 **Add missing CSS properties** (Task #2) - IN PROGRESS
3. 🔄 **Fix direction property** (Task #3) - IN PROGRESS
4. 🔄 **Pass CSS to layout** (Task #4) - IN PROGRESS
5. **Fix inheritance** (Task #5 - THIS REPORT):

   **Quick fix code:**
   ```rust
   // In /Users/fschutt/Development/azul/layout/src/solver3/fc.rs
   
   fn get_resolved_font_weight(
       tree: &LayoutTree,
       styled_dom: &StyledDom,
       node_index: usize,
   ) -> StyleFontWeight {
       let mut current_index = node_index;
       
       loop {
           let node = tree.get(current_index);
           if let Some(node) = node {
               if let Some(dom_id) = node.dom_node_id {
                   let node_data = &styled_dom.node_data.as_container()[dom_id];
                   let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
                   
                   if let Some(weight) = styled_dom.css_property_cache.ptr
                       .get_font_weight(node_data, &dom_id, node_state)
                       .and_then(|v| v.get_property().copied())
                   {
                       return weight;
                   }
               }
               
               // Walk up to parent
               if let Some(parent_idx) = node.parent {
                   current_index = parent_idx;
               } else {
                   break;
               }
           } else {
               break;
           }
       }
       
       StyleFontWeight::Normal // Default
   }
   ```

   Then update `get_style_properties_with_context()` to use it:
   ```rust
   let font_weight = get_resolved_font_weight(tree, styled_dom, node_index);
   
   Arc::new(StyleProperties {
       font_selector: crate::text3::cache::FontSelector {
           family: font_family_name,
           weight: font_weight.to_fc_weight(), // Convert StyleFontWeight -> FcWeight
           style: font_style.to_font_style(),
           unicode_ranges: Vec::new(),
       },
       // ...
   })
   ```

---

## Testing Strategy

### Unit Tests Needed

```rust
#[test]
fn test_font_weight_inheritance_simple() {
    // <p style="font-weight: bold">Text</p>
    // Text node should inherit bold
}

#[test]
fn test_font_weight_inheritance_nested() {
    // <div style="font-weight: bold"><p>Text</p></div>
    // Text node should inherit bold through <p>
}

#[test]
fn test_font_weight_inheritance_override() {
    // <div style="font-weight: bold"><p style="font-weight: normal">Text</p></div>
    // Text node should inherit normal (not bold)
}

#[test]
fn test_non_inherited_property() {
    // <div style="border: 1px solid red"><p>Text</p></div>
    // <p> should NOT inherit border
}
```

### Integration Tests

- Test bold text rendering in PDF output
- Verify fontconfig cache lookup with correct weight
- Check that `FcFontCache` receives `FcWeight::Bold` (not `Normal`)

---

## Impact Assessment

### Components Affected

1. **`/Users/fschutt/Development/azul/core/src/prop_cache.rs`**
   - Add inheritance support (Phase 2)

2. **`/Users/fschutt/Development/azul/layout/src/solver3/fc.rs`**
   - Fix `get_style_properties_with_context()` (Phase 1) ⚠️ URGENT
   - Add inheritance helpers (Phase 1)

3. **`/Users/fschutt/Development/azul/layout/src/solver3/getters.rs`**
   - May need updates if using cache directly

4. **`/Users/fschutt/Development/azul/css/src/props/property.rs`**
   - Add `CssPropertyType::is_inherited()` (Phase 2)

5. **Font loading system**
   - Ensure `FontSelector.weight` is correctly populated
   - Verify `FcFontCache` lookup uses inherited weight

### Backwards Compatibility

- ✅ No breaking changes to public API
- ✅ Pure bug fix (makes behavior match CSS spec)
- ⚠️ May change rendering of existing documents (if they relied on incorrect inheritance)

### Performance Considerations

- **Phase 1:** Minimal impact (only text node styling)
- **Phase 2:** Small overhead on cache misses (tree traversal)
- **Optimization:** Cache inheritance chain per node

---

## Success Criteria

1. ✅ `<p style="font-weight: bold">Text</p>` renders bold in PDF
2. ✅ All CSS 2.1 inherited properties work correctly
3. ✅ Unit tests pass for inheritance scenarios
4. ✅ No performance regression (< 5% slowdown acceptable)
5. ✅ Documentation updated with inheritance behavior

---

## References

- **CSS 2.1 Specification:** https://www.w3.org/TR/CSS21/cascade.html#inheritance
- **CSS Property Definition Table:** https://www.w3.org/TR/CSS21/propidx.html (Inherited? column)
- **Current Issue Location:** `/Users/fschutt/Development/azul/layout/src/solver3/fc.rs:270`
- **Property Cache:** `/Users/fschutt/Development/azul/core/src/prop_cache.rs:1236`

---

## Conclusion

The lack of proper CSS inheritance is a **critical architectural gap** that must be addressed. The recommended approach is:

1. **Short-term:** Fix `get_style_properties_with_context()` to inherit all font properties (1-2 days)
2. **Medium-term:** Move inheritance logic into `CssPropertyCache` (1-2 weeks)
3. **Long-term:** Implement full computed values cache per CSS spec (future)

This will make the layout engine spec-compliant and fix rendering bugs in PDF generation and UI display.

---

**Status:** Ready for implementation  
**Priority:** High (blocks correct text rendering)  
**Estimated Effort:** Phase 1: 8 hours, Phase 2: 40 hours
