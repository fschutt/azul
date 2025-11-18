# Font Weight Fix - Implementation Guide

## Quick Fix (Phase 1)

### Step 1: Make conversion functions public

**File:** `azul/layout/src/solver3/fc.rs`

**Line 270:** Change `fn convert_font_style` to `pub(crate) fn convert_font_style`
```rust
/// Helper: Convert StyleFontStyle to text3::cache::FontStyle
pub(crate) fn convert_font_style(style: azul_css::props::basic::font::StyleFontStyle) -> crate::text3::cache::FontStyle {
    use azul_css::props::basic::font::StyleFontStyle;
    match style {
        StyleFontStyle::Normal => crate::text3::cache::FontStyle::Normal,
        StyleFontStyle::Italic => crate::text3::cache::FontStyle::Italic,
        StyleFontStyle::Oblique => crate::text3::cache::FontStyle::Oblique,
    }
}
```

**Line 279:** Change `fn convert_font_weight` to `pub(crate) fn convert_font_weight`
```rust
/// Helper: Convert StyleFontWeight to rust_fontconfig::FcWeight
pub(crate) fn convert_font_weight(weight: azul_css::props::basic::font::StyleFontWeight) -> rust_fontconfig::FcWeight {
    use azul_css::props::basic::font::StyleFontWeight;
    match weight {
        StyleFontWeight::W100 => rust_fontconfig::FcWeight::Thin,
        StyleFontWeight::W200 => rust_fontconfig::FcWeight::ExtraLight,
        StyleFontWeight::W300 | StyleFontWeight::Lighter => rust_fontconfig::FcWeight::Light,
        StyleFontWeight::Normal => rust_fontconfig::FcWeight::Normal,
        StyleFontWeight::W500 => rust_fontconfig::FcWeight::Medium,
        StyleFontWeight::W600 => rust_fontconfig::FcWeight::SemiBold,
        StyleFontWeight::Bold => rust_fontconfig::FcWeight::Bold,
        StyleFontWeight::W800 => rust_fontconfig::FcWeight::ExtraBold,
        StyleFontWeight::W900 | StyleFontWeight::Bolder => rust_fontconfig::FcWeight::Black,
    }
}
```

---

### Step 2: Fix get_style_properties function

**File:** `azul/layout/src/solver3/getters.rs`

**Location:** Function `get_style_properties`, around line 1000-1024

**BEFORE (broken):**
```rust
pub fn get_style_properties(styled_dom: &StyledDom, dom_id: NodeId) -> StyleProperties {
    // ... existing code for font_family_name, font_size, color, line_height ...
    
    let properties = StyleProperties {
        font_selector: crate::text3::cache::FontSelector {
            family: font_family_name,
            weight: rust_fontconfig::FcWeight::Normal, // ❌ HARDCODED
            style: crate::text3::cache::FontStyle::Normal, // ❌ HARDCODED
            unicode_ranges: Vec::new(),
        },
        font_size_px: font_size,
        color,
        line_height,
        ..Default::default()
    };
    
    properties
}
```

**AFTER (fixed):**
```rust
pub fn get_style_properties(styled_dom: &StyledDom, dom_id: NodeId) -> StyleProperties {
    use azul_css::props::basic::PropertyContext, ResolutionContext, PhysicalSize};
    
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
    let cache = &styled_dom.css_property_cache.ptr;

    let font_family_name = cache
        .get_font_family(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .and_then(|v| v.get(0).map(|f| f.as_string()))
        .unwrap_or_else(|| "serif".to_string());

    // Get parent's font-size for proper em resolution in font-size property
    let parent_font_size = styled_dom
        .node_hierarchy
        .as_container()
        .get(dom_id)
        .and_then(|node| {
            use azul_core::id::NodeId as CoreNodeId;
            let parent_id = CoreNodeId::from_usize(node.parent)?;
            // Recursively get parent's font-size
            cache
                .get_font_size(&styled_dom.node_data.as_container()[parent_id], &parent_id, 
                    &styled_dom.styled_nodes.as_container()[parent_id].state)
                .and_then(|v| v.get_property().cloned())
                .map(|v| {
                    use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
                    v.inner.to_pixels_internal(0.0, DEFAULT_FONT_SIZE)
                })
        })
        .unwrap_or(azul_css::props::basic::pixel::DEFAULT_FONT_SIZE);
    
    // Create resolution context for font-size (em refers to parent)
    let font_size_context = ResolutionContext {
        element_font_size: azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
        parent_font_size,
        root_font_size: azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        element_size: None,
        dpi_scale: 1.0,
    };

    let font_size = cache
        .get_font_size(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner.resolve_with_context(&font_size_context, PropertyContext::FontSize))
        .unwrap_or(azul_css::props::basic::pixel::DEFAULT_FONT_SIZE);

    let color = cache
        .get_text_color(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner)
        .unwrap_or_default();

    let line_height = cache
        .get_line_height(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner.normalized() * font_size)
        .unwrap_or(font_size * 1.2);

    // ✅ NEW: Query font-weight from CSS cache
    let font_weight = cache
        .get_font_weight(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner)
        .unwrap_or(azul_css::props::basic::font::StyleFontWeight::Normal);

    // ✅ NEW: Query font-style from CSS cache  
    let font_style = cache
        .get_font_style(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner)
        .unwrap_or(azul_css::props::basic::font::StyleFontStyle::Normal);

    // ✅ NEW: Convert using helper functions
    let fc_weight = super::fc::convert_font_weight(font_weight);
    let fc_style = super::fc::convert_font_style(font_style);

    let properties = StyleProperties {
        font_selector: crate::text3::cache::FontSelector {
            family: font_family_name,
            weight: fc_weight,        // ✅ Use actual CSS value
            style: fc_style,          // ✅ Use actual CSS value
            unicode_ranges: Vec::new(),
        },
        font_size_px: font_size,
        color,
        line_height,
        ..Default::default()
    };
    
    properties
}
```

---

## Testing the Fix

### Manual Test:
```bash
cd /Users/fschutt/Development/printpdf
cargo run --release --example html_full
open html_full_test.pdf
```

**Expected result:** 
- "Table Test" heading should be bold
- Table headers ("Header 1", "Header 2") should be bold

### Verification:
Look for debug output like:
```
[FontManager] Using system fallback for 'Helvetica': FontId(...)
```

Should change to:
```
[FontManager] Font match: Helvetica Bold (weight: Bold)
```

---

## Common Issues

### Issue 1: Conversion functions not found
**Error:** `cannot find function convert_font_weight in super::fc`

**Solution:** Make sure you changed `fn` to `pub(crate) fn` in step 1

---

### Issue 2: Type mismatch in cache.get_font_weight()
**Error:** Method `get_font_weight` not found on `CssPropertyCache`

**Solution:** Check that the CSS property cache actually has this method. If not, you may need to add it following the pattern of `get_font_size`.

---

### Issue 3: Still getting regular fonts
**Possible causes:**
1. Font cache is populated before the fix (clear caches)
2. System doesn't have bold variant of the font
3. CSS cascade is overriding the UA default

**Debug:**
Add print statements:
```rust
println!("[DEBUG] font_weight from CSS: {:?}", font_weight);
println!("[DEBUG] fc_weight after conversion: {:?}", fc_weight);
println!("[DEBUG] font_selector: {:?}", properties.font_selector);
```

---

## Validation Checklist

- [ ] Conversion functions are `pub(crate)` in `fc.rs`
- [ ] `get_style_properties` queries `get_font_weight()`
- [ ] `get_style_properties` queries `get_font_style()`
- [ ] Conversion functions are called before FontSelector construction
- [ ] FontSelector uses converted values (not hardcoded stubs)
- [ ] Code compiles without errors
- [ ] `<h1>` elements render in bold
- [ ] `<th>` elements render in bold
- [ ] Regular `<p>` elements still render in normal weight
- [ ] `<strong>` elements render in bolder weight

---

## Time Estimate

- Code changes: 15 minutes
- Compilation: 5 minutes
- Testing: 10 minutes
- **Total: 30 minutes**

---

## Rollback Plan

If the fix causes issues:

1. Revert `azul/layout/src/solver3/getters.rs`:
   ```bash
   git checkout HEAD -- azul/layout/src/solver3/getters.rs
   ```

2. Revert `azul/layout/src/solver3/fc.rs`:
   ```bash
   git checkout HEAD -- azul/layout/src/solver3/fc.rs
   ```

3. Rebuild:
   ```bash
   cargo clean
   cargo build --release
   ```

---

## Next Steps (After Fix Works)

See FONT_RESOLUTION_REPORT.md for:
- Phase 2: FontDescriptor abstraction
- Phase 3: FontResolver service  
- Phase 4: Performance optimization

Each phase builds on the previous one and can be done incrementally.
