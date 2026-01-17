# Icon System Issues Report

Based on testing `icons.c`, four issues were identified. This report analyzes each problem and proposes solutions.

---

## Issue 1: ~~Favicon Appended Instead of Replaced~~ (NOT A BUG)

### Observed Behavior
The favicon icon shows the Azul logo image correctly. The label "favicon" appears **below** the icon as designed.

### Clarification

Looking at `icons.c`:
```c
// Card structure:
AzDom card = AzDom_createDiv();  // column flex container
  AzDom favicon_icon = AzDom_createIcon(az_str("favicon"));  // Child 1: Icon → replaced with Image
  AzDom label = AzDom_createText(az_str("favicon"));          // Child 2: Label text
```

This is **correct behavior**: 
- The Icon node is replaced with an Image node
- The sibling label "favicon" is preserved (as it should be)
- Both appear in the column flex container

### Code Verification

In `core/src/icon.rs::apply_single_node_replacement()`:
```rust
// This correctly replaces the NodeType and css_props of the icon node
// at node_idx, preserving all siblings
let node_data = styled_dom.node_data.as_mut();
if let Some(node) = node_data.get_mut(node_idx) {
    node.set_node_type(replacement_node_type);  // Icon → Image
    node.css_props = replacement_root.get_css_props().clone();
}
```

### Status: ✅ Working as Expected

---

## Issue 2: Material Icons Font Glyphs Not Visible (REAL BUG)

### Observed Behavior
The debug output shows correct Unicode codepoints:
```
[ICON RESOLVE]   - Replacement at node 8: 1 nodes, root type: Text("\u{e88a}")
[ICON RESOLVE]   - Replacement at node 11: 1 nodes, root type: Text("\u{e8b8}")
[ICON RESOLVE]   - Replacement at node 14: 1 nodes, root type: Text("\u{e8b6}")
```

But visually, the icons appear as empty/invisible. The label texts "home", "settings", "search" are visible but those are the **sibling label nodes**, not the icon nodes.

### Root Cause Analysis

The Text nodes contain correct Unicode codepoints (`\u{e88a}` = home icon). The `font-family` CSS property IS being set correctly in `layout/src/icon.rs`:

```rust
fn create_font_icon_from_original(font_icon: &FontIconData, ...) -> StyledDom {
    let mut dom = Dom::create_text(font_icon.icon_char.clone());
    
    // Font family is correctly set!
    let font_prop = CssPropertyWithConditions::simple(
        CssProperty::font_family(StyleFontFamilyVec::from_vec(vec![
            StyleFontFamily::Ref(font_icon.font.clone())
        ]))
    );
    
    dom.root.set_css_props(CssPropertyWithConditionsVec::from_vec(props_vec));
    
    StyledDom::create(&mut dom, Css::empty())
}
```

The CSS property IS then copied in `apply_single_node_replacement()`:
```rust
node.css_props = replacement_root.get_css_props().clone();
```

### The Real Issue: `StyleFontFamily::Ref` Not Handled in Font Resolution

**ROOT CAUSE FOUND!**

In `layout/src/solver3/getters.rs::collect_font_stacks_from_styled_dom()`:

```rust
for i in 0..font_families.len() {
    font_stack.push(FontSelector {
        family: font_families.get(i).unwrap().as_string(),  // <-- BUG HERE!
        weight: fc_weight,
        style: fc_style,
        unicode_ranges: Vec::new(),
    });
}
```

The `as_string()` method for `StyleFontFamily::Ref` returns (from `css/src/props/basic/font.rs:320`):

```rust
StyleFontFamily::Ref(s) => format!("font-ref(0x{:x})", s.parsed as usize),
```

So fontconfig receives `"font-ref(0x12345678)"` as a font family name. Fontconfig can't find this "font", so it falls back to the generic fallbacks (sans-serif, serif, monospace).

The Material Icons glyphs (`\u{e88a}` etc.) don't exist in sans-serif, so they render as invisible/tofu.

### Solution: Handle `StyleFontFamily::Ref` Directly

Similar to how we handle `ImageRef` for CSS backgrounds, we need to:

1. **Detect `StyleFontFamily::Ref` in font collection** - Don't try to resolve via fontconfig
2. **Register FontRef directly with FontManager** - The FontRef already contains parsed font data
3. **Use FontRef in text shaping** - Skip fontconfig resolution for Ref fonts

**Implementation in `collect_font_stacks_from_styled_dom()`:**

```rust
for i in 0..font_families.len() {
    let family = font_families.get(i).unwrap();
    
    match family {
        StyleFontFamily::Ref(font_ref) => {
            // FontRef already contains parsed font data
            // Register it directly with the font manager
            // Don't go through fontconfig
            collected_font_refs.push(font_ref.clone());
        }
        other => {
            // System/File fonts go through fontconfig as before
            font_stack.push(FontSelector {
                family: other.as_string(),
                weight: fc_weight,
                style: fc_style,
                unicode_ranges: Vec::new(),
            });
        }
    }
}
```

**In FontManager, add method to register FontRef directly:**

```rust
impl FontManager {
    pub fn register_font_ref(&mut self, font_ref: FontRef) {
        // Generate a unique FontId for this FontRef
        // Add to loaded fonts map
        // The font data is already parsed in FontRef.parsed
    }
}
```

### Status: 🔴 Bug - Font Icons Invisible

---

## Issue 3: Blurry Favicon (HiDPI Handling)

### Observed Behavior
The favicon is 64x64 source pixels, rendered at 48x48 CSS pixels. On a Retina display (2x DPI), this means:
- CSS size: 48x48
- Physical pixels needed: 96x96
- Source image: 64x64
- Result: Upscaled from 64 to 96, causing blur

### How Browsers Handle HiDPI Images

#### 1. `srcset` Attribute (HTML)
```html
<img src="icon.png" 
     srcset="icon.png 1x, icon@2x.png 2x, icon@3x.png 3x"
     width="48" height="48">
```

#### 2. `image-set()` CSS Function
```css
.icon {
    background-image: image-set(
        url("icon.png") 1x,
        url("icon@2x.png") 2x
    );
}
```

#### 3. Automatic Downscaling
If source image is larger than needed, browsers render at full resolution then downsample. A 64x64 image displayed at 48x48 CSS on 2x display:
- Render the 64x64 image to a 96x96 physical pixel area
- This is upscaling (64→96), which causes blur

#### 4. Best Practice for Crisp Icons
- Provide images at 2x or 3x the CSS size
- For a 48x48 CSS icon: use 96x96 or 144x144 source image
- Alternatively: use vector (SVG) icons which scale perfectly

### Azul's Current Approach

```rust
// In sizing.rs, images use their intrinsic size
let intrinsic_size = image_ref.get_size(); // 64x64

// CSS constrains to 48x48
// Layout uses 48x48
// WebRender receives the 64x64 image data
// On 2x display: needs 96x96 physical pixels, but only has 64x64
```

### Solution Options

#### Option A: Document Best Practice
- Recommend users provide 2x/3x images for HiDPI
- For 48px CSS icon, provide 96px or 144px image

#### Option B: Support `srcset`-like API
```rust
// Future API
Dom::create_image_responsive(ImageRefSet {
    base: image_1x,
    variants: vec![
        (2.0, image_2x),
        (3.0, image_3x),
    ]
})
```

#### Option C: Use SVG/Vector Icons
- Material Icons can be rendered as vector paths
- Perfect scaling at any DPI
- Already have font support (which IS vector)

### Immediate Fix
For now, the 64x64 favicon should be resized to match CSS size at the current DPI. In `compositor2.rs`, when pushing images:
```rust
// Scale image appropriately for display
// If CSS size is 48x48 and DPI is 2x, render at 96x96 physical
// Downsample the 64x64 to 96x96 (still blur, but less than before)
```

Actually, the cleanest solution: **Use the intrinsic size when no CSS size is specified, OR clamp to the source size when larger than needed**.

---

## Issue 4: Inline Baseline Alignment

### Observed Behavior
The h1 "Icon System Demo" and the description text are vertically aligned by their top edge, not by text baseline.

Screenshot shows:
```
┌─────────────────────────────────────────────────┐
│ Icon System Demo  The favicon icon below...     │ ← Both start at same Y
└─────────────────────────────────────────────────┘
```

Should be:
```
┌─────────────────────────────────────────────────┐
│ Icon System Demo                                │
│                   The favicon icon below...     │ ← Baseline aligned
└─────────────────────────────────────────────────┘
```

Wait, looking at the screenshot more carefully - the H1 and description ARE on the same line, which suggests they're in the same inline formatting context. This is actually a CSS/layout issue in the example, not necessarily a bug.

### Actual Issue
Looking at `icons.c`:
```c
AzDom title = AzDom_createText(az_str("Icon System Demo"));
AzDom_setInlineStyle(&title, az_str("font-size: 24px; font-weight: bold; margin-bottom: 16px;"));

AzDom desc = AzDom_createText(az_str("The favicon icon below..."));
AzDom_setInlineStyle(&desc, az_str("font-size: 14px; color: #666; margin-bottom: 20px;"));
```

Both are added as children of the root div with `flex-direction: column`. In flexbox column layout, items should stack vertically. But they appear on the same line.

### Root Cause
This could be:
1. `flex-direction: column` not being applied correctly
2. Text nodes being treated as inline and merged
3. Display type issue (text nodes default to `display: inline`)

### Solution
Need to wrap text in block-level containers or set `display: block`:
```c
AzDom_setInlineStyle(&title, az_str("display: block; font-size: 24px; ..."));
```

Or the flexbox column layout needs to work correctly for text children.

---

## Summary of Fixes Needed

| Issue | Priority | Fix Location |
|-------|----------|--------------|
| 1. Favicon appending | HIGH | `core/src/icon.rs` - `splice_at_node()` logic |
| 2. Font icons invisible | HIGH | `layout/src/icon.rs` - Apply font-family CSS |
| 3. Blurry images (HiDPI) | MEDIUM | Document best practice, consider vector icons |
| 4. Inline alignment | MEDIUM | `icons.c` example or flex layout for text |

---

## Next Steps

1. **Debug Issue 1**: Add more logging to `splice_at_node()` to see what's happening
2. **Fix Issue 2**: Ensure `FontRef` becomes a CSS `font-family` property
3. **Document Issue 3**: HiDPI best practices
4. **Review Issue 4**: Check if this is example code issue or layout bug
