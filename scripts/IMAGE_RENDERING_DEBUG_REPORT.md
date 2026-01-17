# Image Rendering Debug Report - Root Cause Found

## Summary

**Images are NOT being added to the display list at all.** The issue is in the inline layout flow where images are converted to `InlineContent::Image` with an **empty placeholder** `ImageSource`.

---

## Root Cause Analysis

### Problem Location: fc.rs:5323

```rust
// In collect_and_measure_inline_content()
} else if let NodeType::Image(image_data) = ... {
    content.push(InlineContent::Image(InlineImage {
        source: ImageSource::Url(String::new()), // ← EMPTY PLACEHOLDER!
        intrinsic_size: ...,
        ...
    }));
}
```

The `image_data` (which is an `ImageRef` containing the actual decoded image) is **DISCARDED** and replaced with an empty `ImageSource::Url("")`.

### Problem Location: display_list.rs:2551-2554

```rust
InlineContent::Image(image) => {
    if let Some(image_ref) = get_image_ref_for_image_source(&image.source) {
        builder.push_image(object_bounds, image_ref);
    }
}
```

Since `image.source` is `ImageSource::Url("")` and `get_image_ref_for_image_source()` returns `None`, the image is **never pushed to the display list**.

### Problem Location: font_traits.rs:204

```rust
pub struct ImageSource;  // Empty stub - no way to store ImageRef!
```

or with text_layout feature:

```rust
pub enum ImageSource {
    Url(String),  // Only stores URL, not ImageRef
}
```

---

## Data Flow Trace

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. DOM Creation                                                             │
│    User creates: Dom::image(ImageRef)                                       │
│    NodeType::Image(ImageRef) ✓ Contains actual image data                   │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. Inline Layout (fc.rs:5307-5337)                                          │
│    Image node is treated as inline content                                  │
│    ImageRef is DISCARDED, replaced with ImageSource::Url("")                │
│    ✗ BROKEN: image_data (ImageRef) is lost here!                            │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. Display List Generation (display_list.rs:2551)                           │
│    paint_inline_object() calls get_image_ref_for_image_source("")           │
│    Returns None → push_image() is NEVER called                              │
│    ✗ BROKEN: No DisplayListItem::Image added!                               │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. Resource Collection (wr_translate2.rs:725-734)                           │
│    Scans display list for DisplayListItem::Image                            │
│    Finds ZERO images → generates ZERO AddImage updates                      │
│    ✓ Code is correct, but receives empty input                              │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 5. WebRender                                                                │
│    Receives ZERO image resources                                            │
│    Receives display list with ZERO image items                              │
│    → Nothing to render                                                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Why Display List Code Path is Never Reached

There's a secondary code path in `display_list.rs:2104-2114` that WOULD work:

```rust
} else if let Some(dom_id) = node.dom_node_id {
    // This node might be a simple replaced element, like an <img> tag.
    let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
    if let NodeType::Image(image_ref) = node_data.get_node_type() {
        builder.push_image(paint_rect, image_ref.clone());  // ✓ This would work!
    }
}
```

**But this is an `else if`!** The condition is:

```rust
if let Some(cached_layout) = &node.inline_layout_result {
    // ... inline content path (images become InlineContent::Image with empty source)
} else if let Some(dom_id) = node.dom_node_id {
    // ... replaced element path (would work, but never reached for images!)
}
```

Since images are processed through the inline layout system (they're "inline replaced elements" in CSS terms), they get an `inline_layout_result`, so the `else if` branch is **NEVER reached**.

---

## Immediate Fix Required

### Option A: Store ImageRef in ImageSource (Recommended)

**File: layout/src/font_traits.rs**

```rust
#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Direct reference to decoded image
    Ref(ImageRef),
    /// CSS url reference (needs cache lookup later)
    Url(String),
}
```

**File: layout/src/solver3/fc.rs:5323**

```rust
content.push(InlineContent::Image(InlineImage {
    source: ImageSource::Ref(image_data.clone()),  // Store the actual ImageRef!
    intrinsic_size: ...,
    ...
}));
```

**File: layout/src/solver3/display_list.rs**

```rust
fn get_image_ref_for_image_source(source: &ImageSource) -> Option<ImageRef> {
    match source {
        ImageSource::Ref(image_ref) => Some(image_ref.clone()),
        ImageSource::Url(url) => {
            // TODO: Look up in ImageCache
            None
        }
    }
}
```

### Option B: Bypass Inline Layout for Images (Quick Hack)

Treat images as block-level replaced elements instead of inline content:

**File: layout/src/solver3/fc.rs:5307**

```rust
// Don't add images to inline content at all
// Let them be rendered via the "replaced element" path in display_list.rs
} else if let NodeType::Image(_) = ... {
    // Skip - will be handled in display_list.rs paint_node()
}
```

**File: layout/src/solver3/display_list.rs:2104**

```rust
// Change from "else if" to separate check
if let Some(cached_layout) = &node.inline_layout_result {
    self.paint_inline_content(builder, content_box.rect(), inline_layout)?;
}

// Always check for replaced elements (images), not just when no inline_layout
if let Some(dom_id) = node.dom_node_id {
    let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
    if let NodeType::Image(image_ref) = node_data.get_node_type() {
        builder.push_image(paint_rect, image_ref.clone());
    }
}
```

---

## Debug Output Analysis

Based on previous debug runs, we saw:
- `[IMAGE DEBUG] Found Image in display list with hash ...` - **NEVER printed**
- `[IMAGE DEBUG] Total images in display lists: 0` - **Confirms zero images**
- `[IMAGE DEBUG] build_add_image_resource_updates returned 0 updates` - **No images to register**

This confirms the display list contains **zero** `DisplayListItem::Image` items.

---

## Verification Steps

After implementing the fix:

1. **Display list should contain images:**
   ```
   [IMAGE DEBUG] Found Image in display list with hash ImageRefHash { inner: 123456 }
   [IMAGE DEBUG] Total images in display lists: 1
   ```

2. **AddImage updates should be generated:**
   ```
   [IMAGE DEBUG] build_add_image_resource_updates returned 1 updates
   [IMAGE DEBUG] Adding 1 images to WebRender transaction!
   ```

3. **Compositor should find the image:**
   ```
   [COMPOSITOR2 IMAGE] Looking up ImageRefHash { inner: 123456 }
   [COMPOSITOR2 IMAGE] currently_registered_images has 1 entries
   [COMPOSITOR2 IMAGE]   - registered: ImageRefHash { inner: 123456 }
   ```

---

## Files to Modify

| File | Change |
|------|--------|
| `layout/src/font_traits.rs` | Add `ImageRef` variant to `ImageSource` |
| `layout/src/solver3/fc.rs` | Store `ImageRef` in `ImageSource::Ref()` |
| `layout/src/solver3/display_list.rs` | Implement `get_image_ref_for_image_source()` |

---

## Related: ImageSource Definition

### Current (Broken)

```rust
// With text_layout feature
pub enum ImageSource {
    Url(String),
}

// Without text_layout feature
pub struct ImageSource;
```

### Proposed Fix

```rust
use azul_core::resources::ImageRef;

#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Direct reference to decoded image (from DOM)
    Ref(ImageRef),
    /// CSS url reference (from background-image, needs ImageCache lookup)
    Url(String),
}
```

This allows:
- Images from DOM nodes to carry their `ImageRef` directly
- CSS background-images to store the URL for later cache lookup
