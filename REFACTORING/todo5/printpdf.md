# printpdf - HTML-to-PDF Critical Issues

**Date:** 2025-11-13  
**Status:** CRITICAL - Blocks Release  
**Related Docs:**
- Architecture analysis: `REFACTORING/todo5/CSS_PROPERTY_DEFAULT_VALUES_ARCHITECTURE.md`
- Implementation plan: `REFACTORING/todo5/defaultvalues.md`

---

## Critical Issues (P0 - Blocks Release)

### P0-1: Block Layout Broken - Elements Overlap

**Description:** H1 and P tags render at the same Y position (580.04456), causing them to overlap instead of stacking vertically.

**Root Cause:** Block Formatting Context (BFC) implementation in `layout/src/solver3/fc.rs` doesn't accumulate Y offsets for siblings. The layout pass doesn't maintain a "pen" position that advances after each child.

**Current Behavior:**
```
H1: y=580.04456
P:  y=580.04456  ← WRONG: Same Y coordinate!
```

**Expected Behavior:**
```
H1: y=0
P:  y=40 (H1 height + margins)
```

**Fix Required:**
- File: `azul/layout/src/solver3/fc.rs`
- Function: `layout_bfc()`
- Add `main_pen` tracking for vertical positioning
- Accumulate: `main_pen += child.height + collapsed_margins`

**Implementation:** See Step 2 in `defaultvalues.md` - complete code provided

**Test Case:**
```html
<h1>Header</h1>
<p>Paragraph</p>
```
Expected: P below H1, not overlapping

---

### P0-2: Width/Height Auto-Sizing Broken

**Description:** The layout engine cannot distinguish between `height: auto` (use content height) and `height: 0px` (explicitly zero). Current "fix" using `MaxContent` as default is semantically wrong.

**Root Cause:** Type system design flaw - `LayoutWidth` and `LayoutHeight` enums have no `Auto` variant. The `Default` implementation returns `Px(0)`, and `.unwrap_or(default)` conflates "not set" with "explicitly 0px".

**Current Wrong Fix:**
```rust
// azul/layout/src/solver3/getters.rs:46-57
get_css_property!(get_css_width, get_width, LayoutWidth, LayoutWidth::MaxContent);  // WRONG
get_css_property!(get_css_height, get_height, LayoutHeight, LayoutHeight::MaxContent);  // WRONG
```

**Problems with Current Fix:**
1. Treats ALL unset heights as `max-content` 
2. Breaks explicit `height: 0px` - can't distinguish from unset
3. Block elements size incorrectly
4. Doesn't respect CSS initial values per spec

**Fix Required:**
1. Add `Auto` variant to `LayoutWidth` and `LayoutHeight` enums
2. Change `Default::default()` to return `Auto` instead of `Px(0)`
3. Update sizing logic to handle `Auto` based on display type
4. Update Taffy bridge to convert `Auto` to `Dimension::auto()`

**Implementation:** See Step 1 in `defaultvalues.md` - complete code provided

**Files to Modify:**
- `azul/css/src/props/layout/dimensions.rs` - Add Auto variant
- `azul/layout/src/solver3/getters.rs` - Fix default values
- `azul/layout/src/solver3/sizing.rs` - Handle Auto in sizing logic
- `azul/layout/src/solver3/taffy_bridge.rs` - Convert Auto to Taffy's auto

**Test Cases:**
```html
<!-- Should use content height, NOT 0px -->
<div style="height: auto;">Content</div>

<!-- Should be 0px, NOT auto -->
<div style="height: 0px;">Hidden</div>
```

**Estimated Work:** 2-3 days (per architecture doc)

---

## High Priority Issues (P1)

### P1-1: Font-Size from CSS Stylesheets Not Applied

**Description:** Font-size specified in `<style>` blocks is ignored. Only inline styles work. Test shows H1 rendered at 16px despite CSS rule `h1 { font-size: 32px; }`.

**Current Behavior:**
```
SetFontSize: size=Pt(16.0)  ← H1 uses default, not 32px from CSS
```

**Root Cause:** CSS cascade broken - `getters.rs` uses `.unwrap_or(16.0)` which bypasses proper cascade resolution including:
1. CSS rules from `<style>` blocks
2. User-agent stylesheets
3. Inherited values
4. Specificity calculation

**Current Code:**
```rust
// azul/layout/src/solver3/getters.rs:463
let font_size = cache
    .get_font_size(node_data, &dom_id, node_state)
    .and_then(|v| v.get_property().cloned())
    .map(|v| v.inner.to_pixels(16.0))
    .unwrap_or(16.0);  // <-- Always 16.0 if not found!
```

**Fix Required:**
1. Create `layout/src/solver3/cascade.rs` with `get_resolved_font_size()`
2. Walk up layout tree to find inherited font-size values
3. Check CSS rules from style blocks before falling back to default
4. Implement proper cascade: inline > style block > user-agent > inherited > initial

**Implementation:** See Step 3 in `defaultvalues.md` - complete code provided

**Files to Modify:**
- Create `azul/layout/src/solver3/cascade.rs` - New file for inheritance
- `azul/layout/src/solver3/fc.rs` - Use context-aware property getters
- `azul/layout/src/solver3/getters.rs` - Remove broken get_style_properties()

**Test Case:**
```html
<style>
  h1 { font-size: 32px; }
</style>
<h1>Should be 32px</h1>
<p>Should be 16px (default)</p>
```

**Estimated Work:** 1-2 days

---

## Medium Priority Issues (P2)

### P2-1: Wrong Default Font

**Description:** HTML elements without explicit `font-family` use Helvetica Neue (sans-serif) instead of Times New Roman (serif) as required by HTML spec.

**Current Behavior:**
```
Font: hash=13650587470105888389, path=/System/Library/Fonts/HelveticaNeue.ttc
```

**Expected:** Times New Roman or other serif font as default per HTML spec

**Investigation Needed:**
- Where does Azul set default fonts in FontManager?
- Is there a user-agent stylesheet that should set `body { font-family: serif; }`?
- Check `azul/layout/src/text3/cache.rs` font resolution logic

**Estimated Work:** 1 day

---

### P2-2: No Arabic Font Fallback

**Description:** Arabic text renders with Helvetica Neue, which doesn't have Arabic glyphs. Font system should automatically fall back to fonts with Arabic support when encountering missing glyphs.

**Current Behavior:**
- Arabic Unicode characters used: U+0645, U+0631, U+062D, U+0628, U+0627, etc.
- Font used: Helvetica Neue (no Arabic glyphs)
- Result: Missing glyph boxes or incorrect rendering

**Expected Behavior:**
- Detect missing glyphs for Arabic Unicode ranges (U+0600-U+06FF, U+0750-U+077F)
- Query FcFontCache for fonts with Arabic support
- Fall back to: DejaVu Sans, Noto Sans Arabic, Arial Unicode MS, etc.

**Investigation Needed:**
- Check `azul/layout/src/text3/cache.rs` - glyph_run_to_pdf_ops()
- How does FontManager handle missing glyphs?
- Is FcFontCache consulted for fallback fonts?

**Estimated Work:** 2 days

---

## Architectural Context

### Type System Design Flaw

The root cause of P0-2 and related issues is that CSS properties use `.unwrap_or(T::default())` pattern which cannot distinguish:

| Scenario | CSS | Current Behavior | Expected Behavior |
|----------|-----|------------------|-------------------|
| Not set | `<div>` | Returns `Px(0)` | Should be `Auto` |
| Explicit auto | `height: auto` | N/A (can't parse) | Should be `Auto` |
| Explicit zero | `height: 0px` | Returns `Px(0)` | Should be `Px(0)` |

**Solution:** Add `Auto` variant to distinguish these cases.

### CSS Property States

CSS properties have multiple states that must be represented:

| State | CSS Keyword | Meaning |
|-------|------------|---------|
| Unset | (none) | Use inherited or initial value |
| Initial | `initial` | Use CSS spec default |
| Inherit | `inherit` | Use parent's computed value |
| Auto | `auto` | Algorithm-specific (varies by property) |
| Explicit | `10px`, `50%` | User-specified value |

Current implementation only handles "Explicit", conflating all other states.

---

## Implementation Plan

Following the plan in `defaultvalues.md`:

### Step 1: Add Auto Variant (P0-2) ✅ Code Ready
- Modify `css/src/props/layout/dimensions.rs`
- Update `layout/src/solver3/getters.rs`
- Update `layout/src/solver3/taffy_bridge.rs`
- **Status:** Complete code provided in defaultvalues.md

### Step 2: Fix Block Formatting Context (P0-1) ✅ Code Ready
- Modify `layout/src/solver3/sizing.rs`
- Modify `layout/src/solver3/fc.rs` - Add main_pen tracking
- **Status:** Complete code provided in defaultvalues.md

### Step 3: Fix Font-Size Cascade (P1-1) ✅ Code Ready
- Create `layout/src/solver3/cascade.rs`
- Refactor `layout/src/solver3/fc.rs`
- Update `layout/src/solver3/getters.rs`
- **Status:** Complete code provided in defaultvalues.md

### Step 4: Add Regression Tests ✅ Code Ready
- Create `layout/src/solver3/tests.rs`
- Tests: auto-sizing, explicit zero, block layout spacing, font inheritance
- **Status:** Complete code provided in defaultvalues.md

### Step 5: Fix Default Font (P2-1)
- Investigation required
- **Status:** TODO

### Step 6: Implement Arabic Fallback (P2-2)
- Investigation required
- **Status:** TODO

---

## Testing Strategy

### Regression Tests Required

1. **Auto-Sizing Test:** Verify `height: auto` uses content height, not 0px
2. **Explicit Zero Test:** Verify `height: 0px` is respected
3. **Block Layout Test:** Verify H1 and P don't overlap
4. **Font Inheritance Test:** Verify font-size from `<style>` blocks works

All test code provided in `defaultvalues.md` Step 3.

### Integration Test

Current test case in `printpdf/tests/integration.rs`:
```rust
#[test]
fn test_html_to_document() {
    let html = r#"
<style>
h1 { font-size: 32px; font-weight: bold; }
</style>
<h1 style="font-size: 32px; font-weight: bold;">Hello, World!</h1>
<p>مرحبا بالعالم - Arabic text</p>
"#;
    // ...
}
```

**Current Results:**
- ✅ H1 renders 13 glyphs (was 0 before fix)
- ❌ H1 and P overlap at y=580
- ❌ Font-size shows 16px instead of 32px
- ❌ Uses Helvetica instead of serif
- ❌ Arabic text has wrong font

**Expected After Fixes:**
- ✅ H1 renders 13 glyphs
- ✅ P renders below H1 (y > 580)
- ✅ Font-size shows 32px for H1, 16px for P
- ✅ Uses Times or serif font by default
- ✅ Arabic text uses font with Arabic support

---

## Risk Assessment

| Issue | Risk Level | Impact | Workaround Available? |
|-------|-----------|--------|---------------------|
| P0-1 Block Layout | **CRITICAL** | All multi-element layouts broken | No |
| P0-2 Auto-Sizing | **CRITICAL** | Most layouts broken | No |
| P1-1 Font-Size | **HIGH** | Typography wrong | Use inline styles only |
| P2-1 Default Font | **MEDIUM** | Cosmetic issue | Specify font-family explicitly |
| P2-2 Arabic Fallback | **MEDIUM** | Non-Latin text broken | Use font with Unicode support |

---

## Dependencies

- **azul/css** - Core CSS type definitions
- **azul/layout** - Layout engine (solver3)
- **printpdf** - Consumer of layout engine
- **rust_fontconfig** - Font discovery (FcFontCache)

---

## Success Criteria

### Minimum Viable Fix (P0 only)
- [ ] H1 and P don't overlap (proper Y positioning)
- [ ] `height: auto` uses content height, not 0px
- [ ] `height: 0px` is respected as 0px
- [ ] All regression tests pass

### Complete Fix (P0 + P1)
- [ ] Above + font-size from CSS stylesheets works
- [ ] Font inheritance walks layout tree correctly
- [ ] CSS cascade respects specificity

### Full Feature (P0 + P1 + P2)
- [ ] Above + serif default font
- [ ] Above + Arabic font fallback works

---

## References

- **Architecture Doc:** `REFACTORING/todo5/CSS_PROPERTY_DEFAULT_VALUES_ARCHITECTURE.md`
- **Implementation Plan:** `REFACTORING/todo5/defaultvalues.md`
- **Test Case:** `printpdf/tests/integration.rs::test_html_to_document`
- **CSS Spec:** https://www.w3.org/TR/CSS2/visudet.html (width/height computation)
