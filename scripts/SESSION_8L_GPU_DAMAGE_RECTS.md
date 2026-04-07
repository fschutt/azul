# GPU Damage Rects — Wiring Analysis

## Current State: 90% Built, Final Connection Missing

WebRender computes per-frame damage rects internally and returns them
via `RenderResults::dirty_rects`. All 4 desktop platforms have
`gpu_damage_rects` fields and `request_redraw()` code that consumes
them for per-rect invalidation. **The only missing piece is copying
`render()` results into `gpu_damage_rects`.**

```
renderer.render(device_size, buffer_age)
    │
    ├── returns RenderResults { dirty_rects: Vec<DeviceIntRect>, ... }
    │
    ├── ALL PLATFORMS: results IGNORED or only stats logged
    │
    └── gap: dirty_rects never stored in gpu_damage_rects
```

---

## What WebRender Provides

### RenderResults (webrender/core/src/renderer/mod.rs:5517-5534)

```rust
pub struct RenderResults {
    pub stats: RendererStats,
    pub dirty_rects: Vec<DeviceIntRect>,  // ← THE DAMAGE OUTPUT
    pub picture_cache_debug: PictureCacheDebugInfo,
}
```

After `renderer.render()`, `dirty_rects` contains device-pixel
rectangles of framebuffer regions that changed. Typically 1 rect
(the union), but can be multiple.

### BufferDamageTracker (webrender/core/src/renderer/mod.rs:713-754)

4-frame circular buffer tracking damage history. Used for
`buffer_age`-based backbuffer validity computation:

- `push_dirty_rect(rect)` — records current frame's damage
- `get_damage_rect(buffer_age)` — returns union of damage for
  the backbuffer's staleness

### buffer_age Parameter

`renderer.render(device_size, buffer_age)` — the `buffer_age`
tells WebRender how stale the current backbuffer is:

- `0` = new/unknown buffer → full repaint (ALL PLATFORMS DO THIS TODAY)
- `1` = was previous frame → minimal damage
- `n` = union of last n−1 frames of damage

**Critical issue:** All platforms currently pass `0`, defeating
WebRender's internal damage optimization. Fixing this alone would
reduce GPU work significantly.

---

## Per-Platform Status

### Where renderer.render() is called (results ignored)

| Platform | File:Line | render() call | What happens to results |
|----------|-----------|---------------|------------------------|
| macOS    | macos/mod.rs:5529 | `renderer.render(device_size, 0)` | Only stats logged |
| X11      | x11/mod.rs:1895 | `renderer.render(framebuffer_size, 0)` | `_results` — dropped |
| Wayland  | wayland/mod.rs:2693 | `renderer.render(device_size, 0)` | Result discarded |
| Win32    | windows/mod.rs:1043 | `renderer.render(framebuffer_size, 0)` | `.map_err()` — only error checked |

### request_redraw() — per-rect invalidation (ready, awaiting rects)

| Platform | API Used | Status |
|----------|----------|--------|
| macOS    | `setNeedsDisplayInRect:` (Cocoa) | Code ready, rects never populated |
| X11      | `XSendEvent` with per-rect `Expose` | Code ready, rects never populated |
| Win32    | `InvalidateRect(hwnd, &rect, 0)` | Code ready, rects never populated |
| Wayland  | `wl_surface_damage()` | Minimal impl, no per-rect path |

### Buffer swap (always full today)

| Platform | Swap Call | Partial Alternative |
|----------|-----------|---------------------|
| macOS    | `gl_context.flushBuffer()` | None — macOS compositor handles partial |
| X11      | `eglSwapBuffers()` | `eglSwapBuffersWithDamageKHR` (not loaded) |
| Wayland  | `eglSwapBuffers()` | `eglSwapBuffersWithDamageKHR` (not loaded) |
| Win32    | `SwapBuffers(hdc)` | None — DWM handles partial compositing |

---

## Implementation Plan

### Phase 1: Wire RenderResults → gpu_damage_rects (Low Risk)

After each `renderer.render()` call, convert `dirty_rects` from
device-pixel coordinates to logical coordinates and store them:

```rust
// After renderer.render():
match renderer.render(device_size, 0) {
    Ok(results) => {
        let dpi = ws.size.dpi as f32 / 96.0;
        self.gpu_damage_rects = results.dirty_rects.iter().map(|dr| {
            LogicalRect {
                origin: LogicalPosition {
                    x: dr.min.x as f32 / dpi,
                    y: dr.min.y as f32 / dpi,
                },
                size: LogicalSize {
                    width: dr.width() as f32 / dpi,
                    height: dr.height() as f32 / dpi,
                },
            }
        }).collect();
    }
    Err(e) => { ... }
}
```

Apply to: macOS (line 5529), X11 (line 1895), Win32 (line 1043),
Wayland (line 2693).

**Expected impact:** `request_redraw()` starts sending per-rect
invalidation to the OS compositor. The compositor can skip
recompositing unchanged regions (major win for cursor blink,
scrollbar animations, etc.).

### Phase 2: Pass Correct buffer_age (Medium Risk)

Replace `buffer_age: 0` with the actual backbuffer age. This
requires platform-specific queries:

| Platform | How to Get buffer_age |
|----------|-----------------------|
| EGL (X11/Wayland) | `eglQuerySurface(display, surface, EGL_BUFFER_AGE_KHR, &age)` — requires loading `EGL_KHR_get_all_proc_addresses` or `EGL_EXT_buffer_age` |
| macOS | Not available via NSOpenGLContext. Always pass 1 (double-buffered: backbuffer was the previous frame). |
| Win32 | Not available via WGL. Always pass 1 (double-buffered). |

For double-buffered contexts without explicit age queries, `1` is
correct (the backbuffer contains the previous frame's content).

**Expected impact:** WebRender's `BufferDamageTracker` produces
accurate `dirty_rects` that represent only what actually changed,
instead of always returning the full framebuffer rect.

### Phase 3: EGL Swap with Damage (Linux only)

Load and use `eglSwapBuffersWithDamageKHR` on Linux/Wayland to
tell the GPU driver which framebuffer regions need copying:

```rust
// In x11/dlopen.rs and wayland/dlopen.rs, add:
pub eglSwapBuffersWithDamageKHR: Option<
    unsafe extern "C" fn(
        display: EGLDisplay,
        surface: EGLSurface,
        rects: *const EGLint,  // [x, y, w, h, x, y, w, h, ...]
        n_rects: EGLint,
    ) -> EGLBoolean
>,

// Load via eglGetProcAddress (extension may not be available):
let swap_damage = eglGetProcAddress("eglSwapBuffersWithDamageKHR");
```

Then in the swap path, use it instead of `eglSwapBuffers`:

```rust
if let Some(swap_fn) = egl.eglSwapBuffersWithDamageKHR {
    // Convert damage rects to EGLint array [x, y, w, h, ...]
    // Note: EGL damage rects use bottom-left origin
    let mut egl_rects: Vec<i32> = Vec::new();
    for dr in &damage_rects {
        egl_rects.push(dr.origin.x as i32);
        egl_rects.push((fb_height - dr.origin.y - dr.size.height) as i32);
        egl_rects.push(dr.size.width as i32);
        egl_rects.push(dr.size.height as i32);
    }
    swap_fn(display, surface, egl_rects.as_ptr(), damage_rects.len() as i32);
} else {
    eglSwapBuffers(display, surface);
}
```

**Expected impact:** GPU driver can skip copying unchanged framebuffer
regions during the page flip. Significant on tiled renderers (Mali,
Adreno) where this avoids re-reading unchanged tiles from memory.

### Phase 4: glScissor for Selective Clear (Optional)

`glScissor` is already loaded on all platforms. WebRender already uses
it internally for per-tile clears. For the application layer, the main
opportunity is scissored `glClear` before rendering:

```rust
gl.enable(gl::SCISSOR_TEST);
for dr in &damage_rects {
    gl.scissor(dr.x, dr.y, dr.w, dr.h);
    gl.clear(gl::COLOR_BUFFER_BIT);
}
gl.disable(gl::SCISSOR_TEST);
```

This is **lower priority** because WebRender already handles internal
clipping efficiently. The main bottleneck is the swap, not the render.

---

## Cursor Blink Specifics

Cursor blink toggles `blink.is_visible` every 530ms via timer callback.
This returns `Update::RefreshDom` which triggers a full display list
rebuild + WebRender frame.

With damage rects wired:
1. Display list changes only in the cursor's bounds
2. `compute_display_list_damage()` produces a small rect (cursor area)
3. CPU path: only repaints cursor rect (already working after session 8L)
4. GPU path: WebRender's dirty_rect covers only the cursor area
5. `request_redraw()` → `setNeedsDisplayInRect:` for cursor area only
6. OS compositor only recomposites that region

**Net effect:** Cursor blink goes from full-screen GPU render + full
swap to cursor-region-only render + partial compositor update.

---

## Selection Animation Specifics

Selection highlight changes when the user extends/retracts selection:
- SelectionRect items change bounds in the display list
- Old selection area + new selection area both become damage rects
- Coalescing merges adjacent selection rects into a single band

This already works correctly in the CPU path. For GPU, the same
dirty_rects from WebRender would cover the selection change area.

---

## Scroll Offset Specifics

Scroll offset changes don't modify the display list structure — the
same items exist, just rendered at different positions.

- CPU path: handled via pixel-shift + expose-strip damage (existing)
- GPU path: WebRender handles scroll via reference frame transforms.
  The dirty_rect covers the entire scroll viewport (correct, since
  all visible content shifts). No special handling needed.

---

## Files to Modify

Phase 1 (wire results → gpu_damage_rects):
- `dll/src/desktop/shell2/macos/mod.rs` — after line 5529
- `dll/src/desktop/shell2/linux/x11/mod.rs` — after line 1895
- `dll/src/desktop/shell2/linux/wayland/mod.rs` — after line 2693
- `dll/src/desktop/shell2/windows/mod.rs` — after line 1043

Phase 2 (correct buffer_age):
- Same 4 files, change `0` → correct age value
- `dll/src/desktop/shell2/linux/x11/dlopen.rs` — load `eglQuerySurface`
- `dll/src/desktop/shell2/linux/wayland/dlopen.rs` — same

Phase 3 (EGL swap with damage):
- `dll/src/desktop/shell2/linux/x11/dlopen.rs` — load extension
- `dll/src/desktop/shell2/linux/x11/gl.rs` — swap function
- `dll/src/desktop/shell2/linux/wayland/dlopen.rs` — load extension
- `dll/src/desktop/shell2/linux/wayland/gl.rs` — swap function

Phase 4 (glScissor clear):
- Same 4 platform mod.rs files, before `renderer.render()` call

---

## Risk Assessment

| Phase | Risk | Reason |
|-------|------|--------|
| 1 | Low | Only adds data flow, doesn't change rendering |
| 2 | Medium | Wrong buffer_age can cause visual artifacts (missing damage) |
| 3 | Low | Extension may not be available; fallback to normal swap |
| 4 | Low | Scissor is additive optimization, doesn't change output |
