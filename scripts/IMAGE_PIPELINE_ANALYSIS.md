# Image Loading/Caching Pipeline Analysis

## Executive Summary

The image rendering system is currently broken because images are not being properly passed through the display list to the WebRender backend. This document analyzes the current architecture and proposes a complete redesign.

---

## Current Architecture

### Image Types and Sources

Images can come from multiple sources in Azul:

1. **DOM Content Images** (`NodeType::Image(ImageRef)`)
   - Images embedded directly in the DOM via `<img>` tags or `Dom::image()`
   - Currently **partially working** after recent changes

2. **CSS Background Images** (`StyleBackgroundContent::Image(AzString)`)
   - Background images specified via `background-image: url("...")` in CSS
   - Currently **NOT IMPLEMENTED** - returns `None`

3. **Inline Images** (`InlineContent::Image { source: ImageSource }`)  
   - Images embedded in text flow (e.g., emoji, inline icons)
   - Currently **NOT IMPLEMENTED** - `ImageSource` is an empty stub struct

4. **Icon Images** (from `IconProviderHandle`)
   - Vector icons resolved by name
   - Currently working for resolution, but rendering uses same broken pipeline

---

## Data Flow Analysis

### Current Flow (Broken)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. DOM Creation                                                             │
│    - User creates Dom with NodeType::Image(ImageRef)                        │
│    - CSS parsed with background-image: url("...") → AzString only           │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. Layout Phase                                                             │
│    - Images need intrinsic size for layout calculations                     │
│    - NodeType::Image → can get size from ImageRef.get_data()                │
│    - CSS background-image → ??? No ImageRef available, only AzString!       │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. Display List Generation                                                  │
│    - DisplayListItem::Image { bounds, image: ImageRef }                     │
│    - NodeType::Image → works, stores ImageRef directly                      │
│    - CSS background-image → BROKEN: get_image_ref_for_image_source() = None │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. Resource Collection (build_webrender_transaction)                        │
│    - Scans display list for DisplayListItem::Image                          │
│    - Extracts ImageRef and creates AddImage messages                        │
│    - Registers in currently_registered_images                               │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 5. WebRender Compositing (compositor2.rs)                                   │
│    - Looks up ImageRef.get_hash() in currently_registered_images            │
│    - Calls builder.push_image() with WebRender ImageKey                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Key Problems

### Problem 1: CSS Background Images Have No ImageRef

**Location:** `css/src/props/style/background.rs:67`
```rust
pub enum StyleBackgroundContent {
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    ConicGradient(ConicGradient),
    Image(AzString),        // ← Only stores the CSS url string!
    Color(ColorU),
}
```

The CSS parser only stores the URL string, not the actual image data. We need:
- An `ImageCache` lookup during styling/layout
- A way to resolve `AzString` → `ImageRef`

### Problem 2: ImageSource is a Stub

**Location:** `layout/src/font_traits.rs:204`
```rust
#[derive(Debug, Clone)]
pub struct ImageSource;  // ← Empty struct!
```

This is used for inline images but has no actual implementation.

### Problem 3: No Image Resolution During Layout

**Location:** `layout/src/solver3/display_list.rs:795`
```rust
StyleBackgroundContent::Image(_image_id) => {
    // TODO: Implement image backgrounds
}
```

Background images are completely ignored during display list generation.

### Problem 4: ImageCache Not Passed Through Layout

The `ImageCache` exists but is not accessible during display list generation:

**Location:** `core/src/resources.rs:628`
```rust
pub struct ImageCache {
    pub image_id_map: FastHashMap<AzString, ImageRef>,
}
```

This maps CSS `url("...")` strings to `ImageRef`, but it's not available in `DisplayListContext`.

---

## Proposed Solution

### Phase 1: Pass ImageCache to Display List Generation

1. Add `image_cache: &ImageCache` to `LayoutContext`
2. Add `image_cache: &ImageCache` to `DisplayListContext`
3. Modify `push_backgrounds_and_border()` to look up background images

### Phase 2: Resolve CSS Background Images

```rust
// In display_list.rs
StyleBackgroundContent::Image(image_id) => {
    if let Some(image_ref) = self.image_cache.get_css_image_id(image_id) {
        self.push_image(bounds, image_ref.clone());
    }
}
```

### Phase 3: Implement ImageSource for Inline Images

Replace the stub `ImageSource` with a real type:

```rust
pub enum ImageSource {
    /// Direct reference to decoded image
    Ref(ImageRef),
    /// CSS url reference (needs ImageCache lookup)  
    Url(AzString),
    /// Generated image (e.g., SVG icon)
    Generated { 
        width: u32, 
        height: u32, 
        data: Vec<u8> 
    },
}
```

### Phase 4: Image Intrinsic Size Resolution

For layout, we need image dimensions before rendering:

```rust
impl ImageSource {
    pub fn get_intrinsic_size(&self, image_cache: &ImageCache) -> Option<(u32, u32)> {
        match self {
            ImageSource::Ref(img) => img.get_dimensions(),
            ImageSource::Url(url) => {
                image_cache.get_css_image_id(url)
                    .and_then(|img| img.get_dimensions())
            }
            ImageSource::Generated { width, height, .. } => Some((*width, *height)),
        }
    }
}
```

---

## Related TODO Items

### High Priority (Blocking Image Rendering)

| Location | TODO | Impact |
|----------|------|--------|
| `display_list.rs:795` | Implement image backgrounds | CSS background-image completely broken |
| `display_list.rs:846` | Implement image backgrounds for inline text | Inline background-image broken |
| `display_list.rs:2776` | ImageSource needs to contain ImageRef | Inline images completely broken |
| `cpurender.rs:935` | Implement actual image blitting | CPU rendering shows placeholder only |
| `window.rs:1228` | Scan styled_dom for image references | Images not pre-loaded |

### Medium Priority (Rendering Quality)

| Location | TODO | Impact |
|----------|------|--------|
| `compositor2.rs:1647` | Implement proper WebRender box shadow | Box shadows not rendered |
| `compositor2.rs:1686` | Implement proper WebRender filter stacking | CSS filters don't work |
| `compositor2.rs:1710` | Implement proper WebRender backdrop filter | backdrop-filter broken |
| `compositor2.rs:1734` | Implement proper WebRender opacity stacking | Opacity layers broken |
| `cpurender.rs:450` | Implement proper gradient rendering | Gradients show placeholder |
| `cpurender.rs:540` | Implement proper box shadow rendering | CPU shadows not rendered |

### Low Priority (Edge Cases)

| Location | TODO | Impact |
|----------|------|--------|
| `menu_renderer.rs:423` | Render image icon | Menu icons not shown |
| `display_list.rs:2383-2387` | Text decorations/shadows/overflow | Text styling incomplete |
| `fc.rs:3635-3636` | colspan/rowspan from CSS | Table layout incomplete |
| `shape_parser.rs:289` | Handle em, rem, vh, vw | Relative units in shapes broken |

---

## Implementation Checklist

### Immediate Fixes

- [ ] Add `image_cache: &ImageCache` parameter to `LayoutContext`
- [ ] Add `image_cache: &ImageCache` parameter to `DisplayListContext`
- [ ] Implement `StyleBackgroundContent::Image` handling in `push_backgrounds_and_border()`
- [ ] Implement `StyleBackgroundContent::Image` handling in `push_inline_backgrounds_and_border()`

### Short-term Improvements

- [ ] Replace `ImageSource` stub with real enum
- [ ] Implement `get_image_ref_for_image_source()` function
- [ ] Add image dimension getter to `DecodedImage`
- [ ] Pre-load images referenced in CSS during styling phase

### Long-term Enhancements

- [ ] Lazy image loading with placeholder
- [ ] Image caching across frames (already partially implemented)
- [ ] Async image decoding
- [ ] Image format conversion (RGBA, etc.)
- [ ] SVG/vector image support through ImageSource

---

## Files to Modify

1. **core/src/resources.rs**
   - Add `get_dimensions()` to `ImageRef`/`DecodedImage`

2. **layout/src/font_traits.rs**
   - Replace `ImageSource` stub with real implementation

3. **layout/src/solver3/display_list.rs**
   - Add `image_cache` field to `DisplayListContext`
   - Implement `StyleBackgroundContent::Image` handling
   - Implement `get_image_ref_for_image_source()`

4. **layout/src/window.rs**
   - Pass `ImageCache` through to layout functions

5. **dll/src/desktop/shell2/common/layout_v2.rs**
   - Pass `ImageCache` to `layout_and_generate_display_list()`

---

## Testing Plan

1. **Unit Test:** CSS background-image parsing → ImageRef resolution
2. **Integration Test:** DOM with `<img>` tag renders correctly
3. **Integration Test:** CSS `background-image: url(...)` renders correctly
4. **Visual Test:** Multiple background layers render in correct order
5. **Performance Test:** Image caching prevents duplicate registrations
