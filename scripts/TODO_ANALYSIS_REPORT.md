# TODO Analysis Report

## Summary

This report analyzes all TODOs from `TODO_LIST.md` to identify which ones are:
1. **FIXABLE NOW** - Can be fixed with a simple code change
2. **NEEDS INVESTIGATION** - Requires understanding context before fixing
3. **LEAVE AS-IS** - Intentional placeholder for future work or complex feature
4. **DOCUMENTATION ONLY** - Comment clarification needed, not code fix

---

## FIXABLE NOW (Easy Wins)

### 1. ✅ FIXED: `core/src/ua_css.rs` - TEXT_DECORATION_UNDERLINE

**Location**: Lines 466-469
**Status**: Already fixed in this session

The TODO said "Uncomment when TextDecoration is implemented" but TextDecoration IS implemented. Fixed by:
- Adding `text::StyleTextDecoration` import
- Uncommenting the constant
- Enabling underline for `<a>` and `<u>` elements

---

### 2. ✅ FIXED: `css/src/dynamic_selector.rs` - is_layout_affecting

**Location**: Lines 702-708
**Status**: Already fixed in this session

The TODO said "Implement when CssProperty has this method" but `CssPropertyType::can_trigger_relayout()` already exists and does exactly what was needed.

**Fix Applied**:
```rust
pub fn is_layout_affecting(&self) -> bool {
    self.property.get_type().can_trigger_relayout()
}
```

---

### 3. `core/src/events.rs` - ApplicationEventFilter TODO

**Location**: Line 1968
**Current Code**:
```rust
pub enum ApplicationEventFilter {
    DeviceConnected,
    DeviceDisconnected,
    // ... TODO: more events
}
```

**Analysis**: This is a **documentation/placeholder TODO**, not a lazy fix. The enum is extensible and events can be added as needed. The TODO is just noting that more events could be added in the future.

**Recommendation**: LEAVE AS-IS - This is intentional API design placeholder.

---

### 4. `core/src/gl.rs` - Epoch Overflow

**Location**: Line 763
**Current Code**:
```rust
pub fn gl_textures_remove_epochs_from_pipeline(document_id: &DocumentId, epoch: Epoch) {
    // TODO: Handle overflow of Epochs correctly (low priority)
```

**Analysis**: Epoch is likely a `u32` or similar. Overflow would only occur after billions of frames. This is marked "low priority" and is a valid deferred optimization.

**Recommendation**: LEAVE AS-IS - Low priority, unlikely to cause issues.

---

### 5. `core/src/prop_cache.rs` - No variable support

**Location**: Line 513
**Current Code**:
```rust
CssDeclaration::Static(s) => Some(s),
CssDeclaration::Dynamic(_d) => None, // TODO: No variable support yet!
```

**Analysis**: CSS variables (custom properties like `--color: red`) require a full variable resolution system. This is a **feature gap**, not a lazy TODO.

**Recommendation**: LEAVE AS-IS - This is a significant feature that requires proper implementation.

---

### 6. `core/src/svg.rs` - get_bounds error handling

**Location**: Line 437
**Current Code**:
```rust
None => return SvgRect::default(), // TODO: error?
```

**Analysis**: This is returning a default rect when the polygon has no rings. The question is whether to return an error or a default. For SVG bounds calculation, returning a zero-sized default rect is actually reasonable behavior for empty shapes.

**Recommendation**: Change to clearer code:
```rust
// Empty polygon has zero-sized bounds at origin
None => return SvgRect::default(),
```
Remove the TODO as the current behavior is acceptable.

---

### 7. `core/src/style.rs` - rule_ends_with

**Location**: Lines 317-318
**Current Code**:
```rust
/// TODO: This is wrong, but it's fast
#[inline]
pub fn rule_ends_with(path: &CssPath, target: Option<CssPathPseudoSelector>) -> bool {
```

**Analysis**: Reading the implementation and the comment at line 522-525 in prop_cache.rs:
```rust
// NOTE: This is wrong, but fast
// ...
// NOTE: This won't work correctly for paths with `.blah:hover > #thing`
// but that can be fixed later
```

The TODO is acknowledging a known limitation: the function only checks the **last** selector in a path, not handling compound selectors like `.blah:hover > #thing`. This would require rewriting the CSS matching algorithm.

**Recommendation**: LEAVE AS-IS - This is a known limitation documented for future work. The comment could be improved to explain what's "wrong":
```rust
/// Checks if a CSS path ends with the specified pseudo-selector.
/// 
/// LIMITATION: Only checks the final selector - compound selectors 
/// like `.foo:hover > .bar` won't match correctly on intermediate nodes.
```

---

### 8. `core/src/gpu.rs` - Parent size for % transforms

**Location**: Lines 140-141
**Current Code**:
```rust
.map(|t| {
    // TODO: look up the parent nodes size properly to resolve animation of
    // transforms with %
    let parent_size_width = 0.0;
    let parent_size_height = 0.0;
```

**Analysis**: This is a real limitation affecting `transform: translateX(50%)` animations. To fix properly would require:
1. Access to the layout tree at this point
2. Looking up parent node's computed size
3. Passing layout context into GPU animation code

**Recommendation**: LEAVE AS-IS - Requires architectural changes to pass layout context.

---

### 9. `core/src/transform.rs` - SIMD optimization

**Location**: Line 277
**Current Code**:
```rust
// TODO: use correct SIMD optimization!
let mut matrix = Self::IDENTITY;
let use_avx = INITIALIZED.load(...) && USE_AVX.load(...);
```

**Analysis**: Looking at the code, SIMD IS actually implemented! The function checks for AVX and SSE support and uses them. The TODO is outdated.

**Recommendation**: REMOVE TODO - SIMD is already implemented.

---

### 10. `core/src/icon.rs` - Full subtree splicing

**Location**: Lines 528-534
**Current Code**:
```rust
if replacement_len > 1 {
    // TODO: Full subtree splicing requires inserting nodes into arrays
    #[cfg(debug_assertions)]
    eprintln!(...)
}
```

**Analysis**: This is a known limitation - icon replacement currently only supports single-node replacements. Multi-node icons would require modifying the arena-based DOM arrays.

**Recommendation**: LEAVE AS-IS - This is a feature limitation that requires significant work.

---

### 11. `core/src/dom.rs` - is_focusable analysis

**Location**: Lines 3010-3011
**Current Code**:
```rust
pub fn is_focusable(&self) -> bool {
    // TODO: do some better analysis of next / first / item
    self.get_tab_index().is_some() || ...
```

**Analysis**: The TODO comment doesn't make sense ("next / first / item" is unclear). Looking at the implementation, `is_focusable` correctly checks for `tab_index` or focus callbacks. The comment seems to be a copy-paste error or incomplete thought.

**Recommendation**: REMOVE the confusing TODO comment - the implementation is correct.

---

### 12. `core/src/dom_table.rs` - Anonymous table elements

**Location**: Lines 42-60
**Analysis**: This is a placeholder for implementing CSS 2.2 Section 17.2.1 (anonymous table box generation). This is a complex feature requiring tree manipulation.

**Recommendation**: LEAVE AS-IS - Complex feature, properly documented.

---

## CSS Module TODOs

### 13. `css/src/dynamic_selector.rs` - OS version detection

**Location**: Line 389
**Current Code**:
```rust
os_version: AzString::from_const_str("0.0"), // TODO: Version detection
```

**Analysis**: Detecting OS version requires platform-specific code. This should be implemented in the shell layer (dll/src/desktop/shell2/) where platform APIs are available.

**Recommendation**: LEAVE AS-IS - Requires platform-specific implementation.

---

### 14. `css/src/dynamic_selector.rs` - Accessibility preferences

**Location**: Lines 398-399
```rust
prefers_reduced_motion: BoolCondition::False, // TODO: Accessibility
prefers_high_contrast: BoolCondition::False,
```

**Analysis**: These require platform accessibility APIs to detect user preferences.

**Recommendation**: LEAVE AS-IS - Requires platform-specific implementation.

---

### 15. `css/src/shape_parser.rs` - Handle em, rem, vh, vw

**Location**: Lines 289-290
**Analysis**: Percentage and relative units (em, rem, vh, vw) require layout context for resolution. The parser can't resolve these without knowing container/viewport size.

**Recommendation**: LEAVE AS-IS - Architectural limitation, properly documented.

---

## Layout Module TODOs

### 16. `layout/src/solver3/display_list.rs` - Text decoration handling

**Location**: Lines 2660-2664
```rust
// TODO: Handle text decorations (underline, strikethrough, etc.)
```

**Analysis**: Text decorations ARE already implemented (see lines 2771-2820). This TODO is OUTDATED.

**Recommendation**: REMOVE or UPDATE TODO - Basic text decorations are implemented. Could update to note that skip-ink is not yet implemented.

---

### 17. `layout/src/solver3/display_list.rs` - Image backgrounds

**Location**: Lines 819-820, 870-871
```rust
StyleBackgroundContent::Image(_image_id) => {
    // TODO: Implement image backgrounds
}
```

**Analysis**: This is a real missing feature - CSS `background-image` is not rendered.

**Recommendation**: LEAVE AS-IS - Feature to be implemented.

---

## Summary Statistics

| Category | Count |
|----------|-------|
| Already Fixed | 2 |
| Should Remove/Update TODO | 4 |
| Leave As-Is (Complex Feature) | 10+ |
| Documentation Only | 2 |

## Recommended Immediate Actions

1. ✅ **DONE**: Enable TEXT_DECORATION_UNDERLINE in ua_css.rs
2. ✅ **DONE**: Implement is_layout_affecting() in dynamic_selector.rs
3. **TODO**: Remove outdated SIMD TODO in transform.rs
4. **TODO**: Remove confusing comment in dom.rs is_focusable
5. **TODO**: Update text decoration TODO in display_list.rs (mark as partially done)
6. **TODO**: Clarify svg.rs get_bounds behavior (remove question mark)

## Not Fixable Without Major Work

These TODOs represent real feature gaps or architectural limitations:
- CSS Variables support
- Parent size resolution for % transforms
- Multi-node icon replacement
- Anonymous table element generation
- OS version detection
- Accessibility preference detection
- Relative unit resolution in shape parser
- Image backgrounds
