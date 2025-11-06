# OpenGL Texture Integration Analysis

## Status: 🟡 Partial Implementation

### 1. GL Context Management ✅ IMPLEMENTED

**Finding:** GL context IS properly managed in the rendering pipeline.

**Evidence:**
- `dll/src/desktop/shell2/linux/x11/mod.rs:437` - `gl_context.make_current()`
- `dll/src/desktop/shell2/linux/wayland/mod.rs:1051` - `gl_context.make_current()`

**GL State Restoration:** ✅ IMPLEMENTED
```rust
// From portedfromcore2.rs:3814-3818
// Reset the framebuffer and SRGB color target to 0
if let Some(gl) = gl_context.as_ref() {
    gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
    gl.disable(gl::FRAMEBUFFER_SRGB);
    gl.disable(gl::MULTISAMPLE);
}
```

### 2. Image Callback Rendering Pipeline 🟡 PARTIALLY IMPLEMENTED

**Current Flow (from old code in REFACTORING/todo3/portedfromcore2.rs:3750-3850):**

```
1. Layout completes
2. scan_for_gltexture_callbacks() finds Image nodes with callbacks
3. For each callback node:
   - Create RenderImageCallbackInfo with bounds, gl_context, etc.
   - Invoke callback: (callback.cb)(&mut data, &mut callback_info)
   - Callback returns ImageRef::gl_texture(texture)
   - Reset GL state (framebuffer, SRGB, multisample)
4. Process returned textures:
   - Extract Texture from ImageRef
   - Call insert_into_active_gl_textures(document_id, epoch, texture)
   - Get ExternalImageId back
   - Store in GlTextureCache.solved_textures
5. Create AddImage ResourceUpdate for WebRender
6. Add to transaction
```

**Problem:** This code is in REFACTORING/, not integrated into current dll/src/

**What's Missing:**
- ❌ No active integration of image callback rendering in current dll/src/
- ❌ CallbackChangeResult.image_callbacks_changed is collected but not processed
- ❌ No function to re-invoke callbacks on resize/animation

### 3. Display List Integration 🟡 NEEDS IMPLEMENTATION

**Current State:**
- ✅ `build_add_image_resource_updates()` creates AddImage messages
- ✅ `collect_image_resource_updates()` scans DOMs for images
- ✅ Images are added to WrTransaction via `add_image()`
- ❌ GL textures from callbacks are NOT automatically registered

**What Needs to Happen:**

```rust
// In dll/src/desktop/wr_translate2.rs or similar

pub fn process_image_callback_updates(
    layout_window: &mut LayoutWindow,
    callback_changes: &CallbackChangeResult,
) -> Vec<UpdateImageResult> {
    let mut updates = Vec::new();
    
    for (dom_id, node_ids) in &callback_changes.image_callbacks_changed {
        for node_id in node_ids {
            // 1. Get the ImageRef::callback from the DOM
            let layout_result = layout_window.layout_results.get_mut(dom_id)?;
            let node_data = layout_result.styled_dom.node_data.get_mut(*node_id)?;
            
            // 2. Create RenderImageCallbackInfo
            let callback_info = RenderImageCallbackInfo::new(...);
            
            // 3. Invoke callback
            let new_image_ref = invoke_callback(&mut node_data, &mut callback_info);
            
            // 4. Extract Texture
            let texture = new_image_ref.into_gl_texture()?;
            
            // 5. Update texture in cache
            let external_image_id = layout_window.gl_texture_cache.update_texture(
                *dom_id,
                *node_id,
                layout_window.document_id,
                layout_window.epoch,
                texture,
                &gl_texture_integration::insert_into_active_gl_textures,
            )?;
            
            // 6. Create UpdateImage ResourceUpdate
            updates.push(UpdateImageResult {
                key_to_update: existing_key,
                new_descriptor: texture.get_descriptor(),
                new_image_data: ImageData::External(ExternalImageData {
                    id: external_image_id,
                    channel_index: 0,
                    image_type: ExternalImageType::TextureHandle(ImageBufferKind::Texture2D),
                }),
            });
        }
    }
    
    updates
}
```

### 4. SVG Tessellation & GPU Upload ✅ FULLY IMPLEMENTED

**Finding:** SVG GPU functionality is COMPLETE and READY TO USE!

**Available APIs:**
```rust
// Tessellate SVG paths on CPU
let tessellated = TessellatedSvgNode::from_nodes(...);

// Upload to GPU
let gpu_node = TessellatedGPUSvgNode::new(&tessellated, gl_context);

// Draw to texture
gpu_node.draw(
    &mut texture,
    target_size,
    color,
    transforms,
);
```

**Implementation Location:** `core/src/svg.rs:814-914`

**Features:**
- ✅ Vertex/index buffer creation
- ✅ GPU upload via VertexBuffer::new()
- ✅ Draw with transforms (translation, rotation, scale)
- ✅ Custom color per draw call
- ✅ Matrix transforms (column-major for OpenGL)
- ✅ Proper shader uniforms (vBboxSize, fDrawColor, vTransformMatrix)

**Answer: YES, opengl.rs demo SHOULD work!**

Issues:
- Need to ensure GL context is active during callback
- Need to integrate callback rendering pipeline
- Demo already has correct API usage (tessellation + GPU upload + draw)

### 5. Anti-Aliasing 🟢 CAN BE ADDED

**Current State:**
- Demo renders to RGBA8 texture without AA
- Texture is then displayed via WebRender

**Solution Options:**

#### Option A: MSAA (Multisample Anti-Aliasing)
```rust
// In Texture::allocate_rgba8()
pub fn allocate_rgba8_msaa(
    gl_context: GlContextPtr,
    size: PhysicalSizeU32,
    clear_color: ColorU,
    samples: u32,  // 2, 4, 8, 16
) -> Self {
    // Create MSAA renderbuffer
    let mut rbo = 0;
    gl.gen_renderbuffers(1, &mut rbo);
    gl.bind_renderbuffer(gl::RENDERBUFFER, rbo);
    gl.renderbuffer_storage_multisample(
        gl::RENDERBUFFER,
        samples as i32,
        gl::RGBA8,
        size.width as i32,
        size.height as i32,
    );
    
    // Attach to framebuffer
    gl.framebuffer_renderbuffer(
        gl::FRAMEBUFFER,
        gl::COLOR_ATTACHMENT0,
        gl::RENDERBUFFER,
        rbo,
    );
    
    // After drawing, resolve to regular texture
    gl.blit_framebuffer(...);
}
```

#### Option B: Custom AA Shader (Simpler)
```glsl
// Fragment shader for basic FXAA-style edge smoothing
#version 330 core

in vec2 vTexCoord;
out vec4 FragColor;

uniform sampler2D uTexture;
uniform vec2 uTexelSize;  // 1.0 / texture_size

void main() {
    // Sample 3x3 neighborhood
    vec4 c = texture(uTexture, vTexCoord);
    
    vec4 n  = texture(uTexture, vTexCoord + vec2( 0.0, -1.0) * uTexelSize);
    vec4 s  = texture(uTexture, vTexCoord + vec2( 0.0,  1.0) * uTexelSize);
    vec4 e  = texture(uTexture, vTexCoord + vec2( 1.0,  0.0) * uTexelSize);
    vec4 w  = texture(uTexture, vTexCoord + vec2(-1.0,  0.0) * uTexelSize);
    
    vec4 ne = texture(uTexture, vTexCoord + vec2( 1.0, -1.0) * uTexelSize);
    vec4 nw = texture(uTexture, vTexCoord + vec2(-1.0, -1.0) * uTexelSize);
    vec4 se = texture(uTexture, vTexCoord + vec2( 1.0,  1.0) * uTexelSize);
    vec4 sw = texture(uTexture, vTexCoord + vec2(-1.0,  1.0) * uTexelSize);
    
    // Simple box blur for AA
    vec4 avg = (c + n + s + e + w + ne + nw + se + sw) / 9.0;
    
    // Edge detection
    float lum_c = dot(c.rgb, vec3(0.299, 0.587, 0.114));
    float lum_n = dot(n.rgb, vec3(0.299, 0.587, 0.114));
    float lum_s = dot(s.rgb, vec3(0.299, 0.587, 0.114));
    float lum_e = dot(e.rgb, vec3(0.299, 0.587, 0.114));
    float lum_w = dot(w.rgb, vec3(0.299, 0.587, 0.114));
    
    float edge = abs(lum_c - lum_n) + abs(lum_c - lum_s) + 
                 abs(lum_c - lum_e) + abs(lum_c - lum_w);
    
    // Blend based on edge strength
    float aa_strength = smoothstep(0.1, 0.3, edge);
    FragColor = mix(c, avg, aa_strength * 0.5);
}
```

#### Option C: Supersample & Downsample
```rust
// Render at 2x resolution, then downsample
let render_size = PhysicalSizeU32::new(width * 2, height * 2);
let mut large_texture = Texture::allocate_rgba8(gl_context, render_size, clear_color);

// Render SVG at 2x
gpu_node.draw(&mut large_texture, render_size, color, transforms);

// Downsample to final size (simple averaging)
let final_texture = large_texture.downsample_to(PhysicalSizeU32::new(width, height));
```

**Recommendation:** Start with Option B (custom shader) as it's:
- ✅ Simple to implement
- ✅ Low overhead
- ✅ Good quality for UI rendering
- ✅ Can be toggled on/off

## Action Items

### High Priority
1. ✅ **GL Context Management** - Already working
2. ❌ **Integrate Image Callback Rendering** - Port code from REFACTORING/ to dll/src/
3. ❌ **Process image_callbacks_changed** - Implement in rendering pipeline
4. ❌ **UpdateImage dispatch** - Send to WebRender transaction

### Medium Priority
5. ⚠️ **Test opengl.rs demo** - Should work after #2-4
6. ⚠️ **Add anti-aliasing shader** - Optional enhancement

### Low Priority
7. ⚠️ **MSAA support** - For higher quality (more complex)
8. ⚠️ **Supersample option** - For ultra-high quality (expensive)

## Code Locations

### Working Code (Reference):
- `REFACTORING/todo3/portedfromcore2.rs:3750-3850` - Image callback rendering
- `REFACTORING/todo3/very_old_resources.rs:985-1004` - update_texture()

### Integration Points (Need Implementation):
- `dll/src/desktop/wr_translate2.rs` - Add process_image_callback_updates()
- `dll/src/desktop/shell2/*/mod.rs` - Call after apply_callback_changes()
- `layout/src/window.rs` - Expose image callback invocation API

### Already Implemented:
- `dll/src/desktop/gl_texture_cache.rs` - Texture storage ✅
- `dll/src/desktop/gl_texture_integration.rs` - API wrappers ✅
- `layout/src/callbacks.rs` - CallbackChange::UpdateImageCallback ✅
- `layout/src/timer.rs` - TimerCallbackInfo with Deref ✅
- `core/src/svg.rs:814-914` - SVG GPU rendering ✅
- `core/src/resources.rs:985-1004` - GlTextureCache::update_texture() ✅
