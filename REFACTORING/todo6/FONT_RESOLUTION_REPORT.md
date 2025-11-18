# Font Resolution Architecture Report
**Date:** November 18, 2025  
**Issue:** `font-weight: bold` CSS property not reaching `FcFontCache` lookups  
**Impact:** Bold variants of fonts (e.g., "Helvetica Bold") are not being selected

---

## Executive Summary

The font resolution system in Azul has **7 distinct layers** with multiple format conversions and data structures. The current architecture results in CSS font properties (particularly `font-weight: bold`) being **lost in translation** between the CSS layer and the font cache lookup layer.

**Root Cause:** A hardcoded `FcWeight::Normal` stub at line 1024 of `azul/layout/src/solver3/getters.rs` prevents `font-weight` CSS values from propagating to the font selection system.

**Current State:** 
- ✅ User-Agent CSS correctly defines `font-weight: bold` for `<h1>` and `<th>` elements
- ✅ CSS parser correctly handles `font-weight` property  
- ❌ Font weight is **discarded** during StyleProperties construction
- ❌ FcFontCache always receives `FcWeight::Normal` regardless of actual CSS value

---

## Complete Font Resolution Pipeline

### Layer 1: CSS Definition → User-Agent Defaults
**Location:** `azul/core/src/ua_css.rs`

```rust
// Line 273-275: Define bold weight constant
static FONT_WEIGHT_BOLD: CssProperty = CssProperty::FontWeight(
    CssPropertyValue::Exact(StyleFontWeight::Bold)
);

// Line 524: H1 gets bold
(NT::H1, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),

// Line 573: TH gets bold  
(NT::Th, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
```

**Status:** ✅ **Working correctly**  
**Data Type:** `CssProperty::FontWeight(CssPropertyValue<StyleFontWeight>)`

---

### Layer 2: CSS Property Storage → StyledDom
**Location:** `azul/core/src/styled_dom.rs`

CSS properties are stored in the `CssPropertyCache` which is queried by node ID and state.

**Key Method:**
```rust
cache.get_font_weight(node_data, &dom_id, node_state)
    .and_then(|v| v.get_property().cloned())
```

**Status:** ⚠️ **Partially working** - Properties are stored but not being retrieved in font resolution  
**Data Type:** `CssPropertyValue<StyleFontWeight>`

---

### Layer 3: CSS Value Type → StyleFontWeight Enum
**Location:** `azul/css/src/props/basic/font.rs`

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum StyleFontWeight {
    Lighter = 0,
    W100 = 100,
    W200 = 200,
    W300 = 300,
    Normal = 400,
    W500 = 500,
    W600 = 600,
    Bold = 700,      // ← This value should be used
    W800 = 800,
    W900 = 900,
    Bolder = 1000,
}
```

**Conversion Methods:**
- CSS parser: `parse_font_weight("bold")` → `StyleFontWeight::Bold`
- Fontconfig: `StyleFontWeight::Bold.to_fc_weight()` → `200` (FC_WEIGHT_BOLD)

**Status:** ✅ **Working correctly**  
**Data Type:** `StyleFontWeight` enum with numeric values

---

### Layer 4: StyledDom Query → StyleProperties Construction
**Location:** `azul/layout/src/solver3/getters.rs:947-1041`

**❌ CRITICAL BUG - LINE 1024:**

```rust
pub fn get_style_properties(styled_dom: &StyledDom, dom_id: NodeId) -> StyleProperties {
    // ... font_family_name extraction works ...
    // ... font_size extraction works ...
    // ... color extraction works ...
    
    let properties = StyleProperties {
        font_selector: crate::text3::cache::FontSelector {
            family: font_family_name,
            weight: rust_fontconfig::FcWeight::Normal, // ❌ HARDCODED STUB
            style: crate::text3::cache::FontStyle::Normal, // ❌ HARDCODED STUB
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

**What Should Happen:**
```rust
// Query the CSS cache for font-weight
let font_weight = cache
    .get_font_weight(node_data, &dom_id, node_state)
    .and_then(|v| v.get_property().cloned())
    .map(|v| v.inner) // Extract StyleFontWeight
    .unwrap_or(StyleFontWeight::Normal);

// Convert StyleFontWeight → FcWeight
let fc_weight = convert_font_weight(font_weight);

// Query the CSS cache for font-style
let font_style = cache
    .get_font_style(node_data, &dom_id, node_state)
    .and_then(|v| v.get_property().cloned())
    .map(|v| v.inner)
    .unwrap_or(StyleFontStyle::Normal);

// Convert StyleFontStyle → FontStyle
let fs = convert_font_style(font_style);

let properties = StyleProperties {
    font_selector: crate::text3::cache::FontSelector {
        family: font_family_name,
        weight: fc_weight,        // ✅ Use actual CSS value
        style: fs,                // ✅ Use actual CSS value
        unicode_ranges: Vec::new(),
    },
    // ... rest of properties ...
};
```

**Status:** ❌ **BROKEN - Font weight and style are discarded here**  
**Data Type IN:** `CssPropertyValue<StyleFontWeight>` from cache  
**Data Type OUT:** Hardcoded `rust_fontconfig::FcWeight::Normal`

---

### Layer 5: StyleProperties → FontSelector
**Location:** `azul/layout/src/text3/cache.rs:492-509`

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontSelector {
    pub family: String,
    pub weight: FcWeight,        // ← Should receive FcWeight::Bold
    pub style: FontStyle,
    pub unicode_ranges: Vec<UnicodeRange>,
}
```

**Status:** ✅ **Data structure is correct** - just receives wrong values from Layer 4  
**Data Type:** `rust_fontconfig::FcWeight` enum

---

### Layer 6: FontSelector → FcFontCache Query
**Location:** `azul/layout/src/text3/cache.rs:177-268`

```rust
impl<T: ParsedFontTrait, Q: FontLoaderTrait<T>> FontProviderTrait<T> for FontManager<T, Q> {
    fn load_font(&self, font_selector: &FontSelector) -> Result<T, LayoutError> {
        // Check cache first
        if let Ok(c) = self.font_selector_to_id_cache.lock() {
            if let Some(cached_id) = c.get(font_selector) {
                // ... return cached font ...
            }
        }

        // Query fontconfig with the FontSelector
        let pattern = FcPattern {
            name: Some(font_selector.family.clone()),
            weight: font_selector.weight,    // ← Always receives Normal
            italic: if font_selector.style == FontStyle::Italic {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            // ...
        };

        let fc_match = self.fc_cache.query(&pattern, &mut trace);
        // ...
    }
}
```

**Status:** ⚠️ **Working correctly** - but receives wrong input from Layer 4  
**Data Type:** `rust_fontconfig::FcPattern` with weight field

---

### Layer 7: FcFontCache → System Font Lookup
**Location:** `rust-fontconfig` crate (external dependency)

The `FcFontCache` uses fontconfig's pattern matching to find the best matching font file on the system:

```
Pattern: { family: "Helvetica", weight: Normal }
↓
Fontconfig Query
↓
Match: /System/Library/Fonts/Helvetica.ttc (regular variant)

vs. what SHOULD happen:

Pattern: { family: "Helvetica", weight: Bold }
↓
Fontconfig Query  
↓
Match: /System/Library/Fonts/Helvetica.ttc (bold variant)
  or   /System/Library/Fonts/Helvetica-Bold.ttf
```

**Status:** ✅ **Working correctly** - but receives wrong query pattern  
**Data Type:** Platform-specific font file paths

---

## Type Conversion Chain

```
CSS Text → StyleFontWeight → FcWeight → FontId → ParsedFont
"bold"    → Bold (700)     → 200      → UUID   → Font bytes

CURRENT (BROKEN):
CSS cache: StyleFontWeight::Bold
    ↓ ❌ LOST
get_style_properties: FcWeight::Normal (hardcoded)
    ↓
FcFontCache: Matches regular weight fonts
    ↓
Result: Wrong font variant loaded

CORRECT FLOW:
CSS cache: StyleFontWeight::Bold  
    ↓ ✅ Query cache.get_font_weight()
get_style_properties: StyleFontWeight::Bold
    ↓ ✅ convert_font_weight()
FontSelector: FcWeight::Bold (200)
    ↓ ✅ Pattern matching
FcFontCache: Matches bold weight fonts
    ↓
Result: Correct font variant loaded
```

---

## Conversion Helper Functions

The conversion functions exist and are correct:

### StyleFontWeight → FcWeight
**Location:** `azul/layout/src/solver3/fc.rs:280-295`

```rust
fn convert_font_weight(weight: StyleFontWeight) -> FcWeight {
    match weight {
        StyleFontWeight::W100 => FcWeight::Thin,
        StyleFontWeight::W200 => FcWeight::ExtraLight,
        StyleFontWeight::W300 | StyleFontWeight::Lighter => FcWeight::Light,
        StyleFontWeight::Normal => FcWeight::Normal,
        StyleFontWeight::W500 => FcWeight::Medium,
        StyleFontWeight::W600 => FcWeight::SemiBold,
        StyleFontWeight::Bold => FcWeight::Bold,        // ← Correct mapping
        StyleFontWeight::W800 => FcWeight::ExtraBold,
        StyleFontWeight::W900 | StyleFontWeight::Bolder => FcWeight::Black,
    }
}
```

**Status:** ✅ **Already implemented and correct** - just not being used!

### StyleFontStyle → FontStyle  
**Location:** `azul/layout/src/solver3/fc.rs:270-277`

```rust
fn convert_font_style(style: StyleFontStyle) -> crate::text3::cache::FontStyle {
    match style {
        StyleFontStyle::Normal => FontStyle::Normal,
        StyleFontStyle::Italic => FontStyle::Italic,
        StyleFontStyle::Oblique => FontStyle::Oblique,
    }
}
```

**Status:** ✅ **Already implemented and correct** - just not being used!

---

## Architecture Problems

### 1. **Scattered Responsibilities**
Font resolution logic is spread across 7 files in 3 different crates:
- `azul-core`: UA CSS definitions
- `azul-css`: Type definitions and conversions
- `azul-layout`: Font loading, caching, and resolution

### 2. **Multiple Data Format Conversions**
```
CssProperty → CssPropertyValue → StyleFontWeight → 
FcWeight → FcPattern → FontId → ParsedFont
```

Each conversion is a potential point of failure.

### 3. **Implicit Dependencies**
The `get_style_properties` function in `solver3/getters.rs` needs to:
1. Know about CSS property cache structure
2. Know about font conversion functions (in different module)
3. Know about text layout data structures
4. Construct complex nested structures

### 4. **No Type Safety Between Layers**
The `FontSelector` receives `FcWeight` but the construction site uses `StyleFontWeight`. The type mismatch is "solved" by hardcoding values instead of converting them.

### 5. **Testing Difficulty**
Each layer has different mock/test infrastructure, making end-to-end testing of font resolution nearly impossible.

---

## Immediate Fix

**File:** `azul/layout/src/solver3/getters.rs`  
**Function:** `get_style_properties` (lines 947-1041)  
**Lines to change:** 1024-1025

### Current Code:
```rust
let properties = StyleProperties {
    font_selector: crate::text3::cache::FontSelector {
        family: font_family_name,
        weight: rust_fontconfig::FcWeight::Normal, // ❌ HARDCODED
        style: crate::text3::cache::FontStyle::Normal, // ❌ HARDCODED
        unicode_ranges: Vec::new(),
    },
    // ...
};
```

### Fixed Code:
```rust
// Query CSS cache for font-weight
let font_weight = cache
    .get_font_weight(node_data, &dom_id, node_state)
    .and_then(|v| v.get_property().cloned())
    .map(|v| v.inner)
    .unwrap_or(azul_css::props::basic::font::StyleFontWeight::Normal);

// Query CSS cache for font-style
let font_style = cache
    .get_font_style(node_data, &dom_id, node_state)
    .and_then(|v| v.get_property().cloned())
    .map(|v| v.inner)
    .unwrap_or(azul_css::props::basic::font::StyleFontStyle::Normal);

// Convert using existing helper functions
let fc_weight = super::fc::convert_font_weight(font_weight);
let fc_style = super::fc::convert_font_style(font_style);

let properties = StyleProperties {
    font_selector: crate::text3::cache::FontSelector {
        family: font_family_name,
        weight: fc_weight,        // ✅ Use actual CSS value
        style: fc_style,          // ✅ Use actual CSS value
        unicode_ranges: Vec::new(),
    },
    // ... rest unchanged ...
};
```

**Note:** The `convert_font_weight` and `convert_font_style` functions need to be made `pub(crate)` or moved to a shared module.

---

## Long-Term Architectural Improvements

### Proposal 1: Unified Font Descriptor
Create a single type that encapsulates all font selection criteria:

```rust
// New file: azul/layout/src/font/descriptor.rs

pub struct FontDescriptor {
    family: String,
    weight: FcWeight,
    style: FontStyle,
    size_px: f32,
    unicode_ranges: Vec<UnicodeRange>,
}

impl FontDescriptor {
    /// Construct directly from CSS property cache
    pub fn from_css_cache(
        cache: &CssPropertyCache,
        node_data: &NodeData,
        dom_id: NodeId,
        node_state: &StyledNodeState,
    ) -> Self {
        // All CSS querying logic in one place
        // All type conversions in one place
        // Returns ready-to-use descriptor
    }
    
    /// Convert to FontSelector for font cache lookup
    pub fn to_selector(&self) -> FontSelector {
        FontSelector {
            family: self.family.clone(),
            weight: self.weight,
            style: self.style,
            unicode_ranges: self.unicode_ranges.clone(),
        }
    }
}
```

**Benefits:**
- Single source of truth for font resolution
- Type-safe conversions
- Easy to test
- Clear API boundary

### Proposal 2: Font Resolution Service
Encapsulate the entire font resolution pipeline:

```rust
// New file: azul/layout/src/font/resolver.rs

pub struct FontResolver<'a, T, Q> {
    css_cache: &'a CssPropertyCache,
    font_manager: &'a FontManager<T, Q>,
}

impl<'a, T: ParsedFontTrait, Q: FontLoaderTrait<T>> FontResolver<'a, T, Q> {
    /// Resolve font for a DOM node
    /// Returns the loaded ParsedFont ready for text shaping
    pub fn resolve_font(
        &self,
        styled_dom: &StyledDom,
        dom_id: NodeId,
    ) -> Result<T, LayoutError> {
        // 1. Query CSS properties
        // 2. Apply inheritance and UA defaults
        // 3. Convert to FontDescriptor
        // 4. Query font cache
        // 5. Load font if needed
        // 6. Return ParsedFont
    }
}
```

**Benefits:**
- Encapsulates entire pipeline
- Single entry point for font resolution
- Easy to mock for testing
- Clear error propagation

### Proposal 3: Type-Safe Conversion Pipeline
Use newtype pattern to enforce correct conversions:

```rust
// Prevent accidental mixing of weight types

#[derive(Copy, Clone)]
pub struct CssWeight(StyleFontWeight);

#[derive(Copy, Clone)]  
pub struct FontconfigWeight(FcWeight);

impl From<CssWeight> for FontconfigWeight {
    fn from(w: CssWeight) -> Self {
        FontconfigWeight(match w.0 {
            StyleFontWeight::Bold => FcWeight::Bold,
            // ... all conversions explicit ...
        })
    }
}
```

**Benefits:**
- Compiler-enforced correctness
- No accidental type mixing
- Self-documenting code

---

## Testing Strategy

### Unit Tests Needed:

1. **CSS Property Extraction**
   ```rust
   #[test]
   fn test_h1_has_bold_weight() {
       let styled_dom = create_dom_with_h1();
       let weight = get_font_weight(&styled_dom, h1_node_id);
       assert_eq!(weight, StyleFontWeight::Bold);
   }
   ```

2. **Type Conversions**
   ```rust
   #[test]
   fn test_style_weight_to_fc_weight() {
       assert_eq!(
           convert_font_weight(StyleFontWeight::Bold),
           FcWeight::Bold
       );
   }
   ```

3. **FontSelector Construction**
   ```rust
   #[test]
   fn test_font_selector_preserves_weight() {
       let props = get_style_properties(&styled_dom, node_id);
       assert_eq!(props.font_selector.weight, FcWeight::Bold);
   }
   ```

4. **End-to-End**
   ```rust
   #[test]
   fn test_bold_h1_loads_bold_font() {
       let html = "<h1>Bold Text</h1>";
       let pdf = render_html_to_pdf(html);
       // Verify that the font used has weight 700
       assert!(pdf.uses_font_with_weight(700));
   }
   ```

### Integration Tests Needed:

1. Verify fontconfig receives correct pattern
2. Verify correct font file is loaded for bold text
3. Verify PDF output uses bold font variant
4. Verify inheritance works (bold parent → bold child)

---

## Performance Impact

### Current Architecture:
- Multiple hash map lookups per property
- Repeated CSS cache queries
- String allocations for font family names
- Multiple Arc clones for font data

### With Improvements:
- Single CSS query per node
- Cached FontDescriptor per node
- Reuse FontSelector across multiple lookups
- Fewer allocations

**Estimated improvement:** 20-30% faster text layout for documents with varied typography

---

## Migration Path

### Phase 1: Minimal Fix (Immediate)
- Fix the hardcoded stubs in `get_style_properties`
- Add unit tests for the fix
- **Time estimate:** 2 hours

### Phase 2: Refactor FontSelector Construction
- Create `FontDescriptor::from_css_cache()` method
- Move conversion functions to shared module
- Update `get_style_properties` to use new API
- **Time estimate:** 1 day

### Phase 3: Font Resolution Service  
- Create `FontResolver` service
- Migrate all font loading code
- Update solver3 to use resolver
- **Time estimate:** 3 days

### Phase 4: Optimization
- Cache FontDescriptors per node
- Reduce allocations
- Profile and optimize hot paths
- **Time estimate:** 2 days

---

## Related Files

### Files That Need Changes:
1. ✏️ `azul/layout/src/solver3/getters.rs` - Fix hardcoded stubs
2. ✏️ `azul/layout/src/solver3/fc.rs` - Make conversion functions public
3. ✏️ `azul/layout/src/solver3/mod.rs` - Export conversion functions
4. 📝 `azul/layout/src/font/descriptor.rs` - NEW FILE (Phase 2)
5. 📝 `azul/layout/src/font/resolver.rs` - NEW FILE (Phase 3)

### Files That Work Correctly (No Changes):
- ✅ `azul/core/src/ua_css.rs` - UA defaults correct
- ✅ `azul/css/src/props/basic/font.rs` - Type definitions correct
- ✅ `azul/layout/src/text3/cache.rs` - Font manager correct
- ✅ `azul/layout/src/font/loading.rs` - Font loading correct

---

## Conclusion

The font resolution system has the right pieces but they're not connected properly. The immediate fix is trivial (remove 2 hardcoded stubs), but the long-term solution requires consolidating the scattered font resolution logic into a cohesive service with clear boundaries and type safety.

**Priority:** 🔴 **HIGH** - This affects all bold text rendering in HTML→PDF conversion

**Complexity:** 
- Immediate fix: 🟢 **LOW**
- Long-term refactor: 🟡 **MEDIUM**

**Risk:**
- Immediate fix: 🟢 **LOW** - Only changes value source, not logic
- Long-term refactor: 🟡 **MEDIUM** - Touches multiple subsystems

---

## Appendix: Complete Call Stack

```
printpdf::html::xml_to_pdf_pages()
  ↓
printpdf::html::inline_css_in_xml()  [Inlines <style> into style=""]
  ↓
azul_layout::xml::parse_xml_string()
  ↓
azul_core::xml::str_to_dom()
  ↓
azul_layout::LayoutWindow::layout_and_generate_display_list()
  ↓
azul_layout::solver3::layout_formatting_context()
  ↓
azul_layout::solver3::fc::layout_ifc()  [Inline Formatting Context]
  ↓
azul_layout::solver3::getters::get_style_properties()  ❌ BUG HERE
  ↓
azul_layout::text3::cache::FontManager::load_font()
  ↓
rust_fontconfig::FcFontCache::query()
  ↓
System fontconfig library
  ↓
Font file on disk
```

**The bug is at the 7th step from the top**, right before font cache lookup.
