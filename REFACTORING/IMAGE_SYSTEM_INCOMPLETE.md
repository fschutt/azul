# Image System Implementation - Critical Gaps

## Current Status: Images DO NOT WORK

The image rendering system has fundamental gaps that prevent images from being rendered at all.

## Critical Missing Pieces

### 1. **Image Key Generation (layout/src/solver3/display_list.rs)**

**Problem**: The functions that should convert `ImageRef` / `ImageRefHash` to `ImageKey` are stubs:

```rust
fn get_image_key_for_src(_src: &ImageRefHash) -> Option<ImageKey> {
    None  // <-- STUB! Always returns None
}

fn get_image_key_for_image_source(_source: &ImageSource) -> Option<ImageKey> {
    None  // <-- STUB! Always returns None
}
```

**Impact**: Images are NEVER added to the display list, even if they exist in the DOM.

**Fix Required**:
- These functions need access to `renderer_resources` to look up existing ImageKeys
- OR they need to create new ImageKeys and track the ImageRef -> ImageKey mapping
- This mapping needs to persist across frames

### 2. **Image Resource Collection (dll/src/desktop/wr_translate2.rs)**

**Problem**: `generate_frame()` only collects font resources, not image resources.

**Current Code**:
```rust
pub fn generate_frame(...) {
    // Collects fonts ✓
    let font_updates = collect_font_resource_updates(...);
    
    // Images are MISSING! ✗
    // Should also collect: let image_updates = collect_image_resource_updates(...);
}
```

**Fix Required**:
1. Collect all `ImageKey`s from display lists (already implemented: `collect_image_keys_from_display_list`)
2. For each ImageKey, look up the corresponding `ImageRef` in `renderer_resources`
3. Call `build_add_image_resource_updates()` to create AddImage messages for new images
4. Add image updates to transaction via `txn.update_resources()`
5. Track images in `renderer_resources.currently_registered_images`

### 3. **ImageRef → ImageKey Mapping**

**Solution Implemented**: Direct conversion without mapping table

**How it works**:
- `ImageRefHash(usize)` is a stable pointer-based hash
- `ImageKey { namespace, key: u32 }` is just a wrapper around a u32
- We can directly convert: `key = hash as u32`

**Implementation**:
```rust
pub fn image_ref_hash_to_image_key(hash: ImageRefHash, namespace: IdNamespace) -> ImageKey {
    ImageKey {
        namespace,
        key: hash.0 as u32,
    }
}
```

**Why this works**:
- ImageRefHash is stable across frames (pointer address)
- No need for `image_ref_to_key` mapping table
- Simpler, faster, less memory overhead
- ImageKey is deterministic: same ImageRef always gets same ImageKey

### 4. **Display List Generation Context**

**Problem**: `DisplayListGenerator` doesn't have access to `renderer_resources`, so it can't look up ImageKeys.

**Current Signature**:
```rust
pub fn generate_display_list<T, Q>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree,
    // ...
) -> Result<DisplayList>
```

**Fix Required**:
- Add `renderer_resources: &RendererResources` parameter
- Pass it through to `get_image_key_for_src` and `get_image_key_for_image_source`
- This allows looking up existing ImageKeys during display list generation

## Implementation Plan

### Phase 1: Use Direct ImageRefHash → ImageKey Conversion (COMPLETE ✓)

1. Added `image_ref_hash_to_image_key()` conversion function:
   ```rust
   pub fn image_ref_hash_to_image_key(hash: ImageRefHash, namespace: IdNamespace) -> ImageKey
   ```

2. Updated `build_add_image_resource_updates()` to use direct conversion:
   ```rust
   let key = image_ref_hash_to_image_key(image_ref_hash, id_namespace);
   ```

3. No `image_ref_to_key` mapping needed - conversion is deterministic!

### Phase 2: Pass RendererResources to Display List Generation

1. Update `generate_display_list()` signature to accept `renderer_resources`

2. Pass it through to `paint_node_content()` and other functions that need it

3. Update `get_image_key_for_src()`:
   ```rust
   fn get_image_key_for_src(
       src: &ImageRefHash,
       namespace: IdNamespace
   ) -> ImageKey {
       image_ref_hash_to_image_key(src, namespace)
   }
   ```

### Phase 3: Collect and Send Image Resources in generate_frame()

1. Collect all ImageKeys from display lists:
   ```rust
   let mut all_image_keys = Vec::new();
   for layout_result in layout_window.layout_results.values() {
       all_image_keys.extend(collect_image_keys_from_display_list(&layout_result.display_list));
   }
   ```

2. Build image resource updates:
   ```rust
   let image_updates = build_add_image_resource_updates(
       &layout_window.renderer_resources,
       id_namespace,
       epoch,
       &layout_window.document_id,
       &all_image_refs,
       insert_into_active_gl_textures,
   );
   ```

3. Add to transaction:
   ```rust
   let wr_image_resources: Vec<webrender::ResourceUpdate> = image_updates
       .into_iter()
       .map(|(hash, add_msg)| {
           // No need to update image_ref_to_key - conversion is deterministic!
           translate_add_image(add_msg.0)
       })
       .filter_map(|x| x)
       .map(|add_img| webrender::ResourceUpdate::AddImage(add_img))
       .collect();
   
   txn.update_resources(wr_image_resources);
   ```

### Phase 4: Garbage Collection

Implement image deletion for images no longer in use (similar to font GC).

## Testing Strategy

1. **Unit Test**: ImageRefHash → ImageKey mapping survives across frames
2. **Integration Test**: Simple <img> tag renders correctly
3. **Integration Test**: Image updates when ImageRef changes
4. **Performance Test**: 1000 images don't cause slowdown

## Related Files

- `layout/src/solver3/display_list.rs` - Display list generation (needs renderer_resources)
- `dll/src/desktop/wr_translate2.rs` - Resource management (needs image collection)
- `core/src/resources.rs` - Resource tracking (needs image_ref_to_key map)
- `dll/src/desktop/compositor2.rs` - Image rendering (already implemented)

## Estimated Effort

- Phase 1: 2-3 hours (critical path)
- Phase 2: 2-3 hours (refactoring)
- Phase 3: 1-2 hours (straightforward)
- Phase 4: 1 hour (copy-paste from font GC)

**Total: 6-9 hours of focused work**

## Current Workaround

None. Images simply don't render at all. The compositor code to display them is ready, but they never make it to WebRender because the resource management pipeline is incomplete.
