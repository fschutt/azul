# Session 8L: CPU Rendering — Memory & Performance Analysis

## Findings

### 1. NSBitmapImageRep + NSImage Created Every Frame

**File:** `dll/src/desktop/shell2/macos/mod.rs:963-986`

Every `drawRect` call (every frame) allocates:
- `NSBitmapImageRep::alloc()` — bitmap wrapper
- `NSImage::alloc()` — image wrapper
- `std::ptr::copy_nonoverlapping` — full framebuffer copy to bitmap

These are autoreleased by objc2's `Retained<T>` semantics, so **not a leak**, but
the allocation churn is significant: at 60fps with an 800x600 window, that's
~1.8MB allocated+freed per frame.

**Fix:** Cache the `NSBitmapImageRep` and `NSImage` across frames. Only
reallocate on resize. Reuse the bitmap's data pointer for copy.

### 2. No Damage Rect Optimization

**File:** `dll/src/desktop/shell2/macos/mod.rs:946-951`

```rust
if !macos_window.common.display_list_initialized
    || macos_window.common.frame_needs_regeneration
    || macos_window.common.display_list_dirty
{
    let _ = macos_window.render_and_present_in_draw_rect();
}
```

When any flag is set, a **full** re-render happens via
`render_with_font_manager_and_scroll()`. The `_dirty_rect` parameter from
macOS's `drawRect:` is completely ignored — there's no partial update path.

Even when NO re-render is needed (flags are false), the `drawRect` still:
1. Borrows the full framebuffer
2. Creates NSBitmapImageRep (full frame)
3. Copies the entire framebuffer to bitmap
4. Draws the full image to the window

**Fix:** The headless backend ALREADY does this correctly:
- `headless/mod.rs:227-233` uses `compute_display_list_damage()`
- `headless/mod.rs:275-281` uses `render_display_list_damaged()` for incremental
- `cpurender.rs:1938` has `acquire_pixmap()` for buffer reuse
- `cpurender.rs` has `render_display_list_damaged()` accepting damage_rects

Port the headless damage rect logic to the macOS CPU path. The functions
already exist — they're just not called from `macos/mod.rs:5310-5362`.
Also use macOS's `_dirty_rect` parameter to limit the blit region.

### 3. Mouse Move Triggers drawRect

On macOS, `setNeedsDisplay(true)` is called from `update_framebuffer()`,
which is called from `render_and_present_in_draw_rect()`. But mouse move
events may trigger drawRect indirectly through:
- Hover state changes → `display_list_dirty = true` → re-render
- Cursor blink timer → periodic `setNeedsDisplay`
- Hit test updates

Even hovering over the window (no clicks) likely causes:
1. MouseOver event → hover state change
2. `display_list_dirty = true`
3. Next `drawRect` → full re-render + full blit

**Fix:** Only set `display_list_dirty` when hover state actually CHANGES
(node entered/exited), not on every mouse move within the same node.

### 4. CPU Compositor Stub

**File:** `dll/src/desktop/shell2/common/cpu_compositor.rs`

The `CpuCompositor` is a **complete stub** — `rasterize()` just clears to white.
The actual CPU rendering goes through `azul_layout::cpurender::render_with_font_manager_and_scroll()` which uses agg-rust. The stub compositor is unused for actual rendering.

The real renderer in `layout/src/cpurender.rs` creates a `tiny_skia::Pixmap`
per frame. This is allocated/dropped each time — potential for reuse.

### 5. Glyph Cache on MacOSWindow

**File:** `dll/src/desktop/shell2/macos/mod.rs:5338`

```rust
&mut self.glyph_cache,
```

The `glyph_cache` is on the window and persists across frames. This is correct —
glyphs don't need re-rasterization. But if glyphs are added but never evicted
(e.g., when CJK fonts add thousands of glyphs), this could grow unbounded.

---

## Profiling Instructions

### Quick CPU Profile (mouse wiggle test)

```bash
# Build
cargo build -p azul-dll --release --features build-dll

# Run with CPU backend
AZ_BACKEND=cpu AZUL_DEBUG=8765 ./tests/e2e/contenteditable_test &
PID=$!

# Wait for window, then wiggle mouse
sleep 2

# Sample CPU for 10 seconds
sample $PID 10 -file /tmp/azul_cpu_profile.txt

kill $PID
cat /tmp/azul_cpu_profile.txt | head -100
```

### Memory Leak Check

```bash
MallocStackLogging=1 AZ_BACKEND=cpu ./tests/e2e/contenteditable_test &
PID=$!
sleep 5

# Check for leaks
leaks $PID --outputGraph=/tmp/azul_leaks
kill $PID
```

### Automated Render Stress Test via Debug API

```bash
AZ_BACKEND=cpu AZUL_DEBUG=8765 ./tests/e2e/contenteditable_test &
PID=$!
sleep 2

# Rapid mouse moves to trigger re-renders
for i in $(seq 1 100); do
  curl -s -X POST http://localhost:8765/ \
    -d "{\"op\": \"mouse_move\", \"x\": $((100 + i)), \"y\": 100}" > /dev/null
done

# Check memory
leaks $PID 2>/dev/null | head -5
kill $PID
```

---

## Recommended Fixes (by impact)

| Priority | Issue | Expected Impact |
|----------|-------|-----------------|
| **P0** | Cache NSBitmapImageRep across frames | Eliminates ~1.8MB/frame alloc churn |
| **P0** | Only dirty hover when node changes | Eliminates full re-render on mouse move |
| **P1** | Reuse tiny_skia::Pixmap across frames | Avoids framebuffer realloc per render |
| **P1** | Use damage rects for partial blit | Only blit changed region to window |
| **P1** | Port damage rects from headless to macOS | Functions exist, just not wired up |
| **P2** | Glyph cache eviction | Prevent unbounded memory growth |
| **P2** | Dead `NSData::with_bytes` at line 961 | Unused, potential use-after-free |
