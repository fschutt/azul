# OpenGL Texture Swap Optimization Plan

## Problem Statement

The OpenGL demo (`examples/c/opengl.c`, `examples/rust/src/opengl.rs`) 
hammers the CPU at ~50% because every animation frame triggers a **full DOM 
rebuild** instead of just swapping the GPU texture and compositing.

The 16ms animation timer returns `Update::RefreshDom`, which causes:

1. Full `layout()` callback invocation → rebuilds entire DOM tree
2. Full CSS styling pass
3. Full flexbox layout pass  
4. Full display list rebuild
5. Render image callback invocation (the only actually needed step)
6. WebRender scene build + composite

Only step 5 (re-invoke the render image callback to get a new GL texture) 
and step 6 (composite) are actually needed. Steps 1–4 are pure waste since 
the DOM structure hasn't changed — only `rotation_deg` changed.

## Root Cause Analysis

### Current Flow (wasteful)

```
Timer fires (every 16ms)
  → animate() increments rotation_deg
  → returns TimerCallbackReturn { should_update: Update::RefreshDom, ... }
  
Shell event loop sees RefreshDom
  → sets frame_needs_regeneration = true
  → calls setNeedsDisplay (macOS) / InvalidateRect (Windows) / expose (X11)

drawRect fires
  → render_and_present_in_draw_rect()
    → regenerate_layout()          ← EXPENSIVE: calls layout(), re-styles, re-layouts
      → calls user's layout() fn  ← rebuilds entire DOM from scratch
      → CSS cascade
      → flexbox solver
      → display list generation
    → build_webrender_transaction()
      → process_image_callback_updates()  ← invokes render_my_texture()
      → translate display lists to WR
    → send transaction to WebRender
    → renderer.render() + present
```

### Ideal Flow (optimized)

```
Timer fires (every 16ms)
  → animate() increments rotation_deg
  → calls info.callback_info.update_image_callback(dom_id, node_id)
  → returns TimerCallbackReturn { should_update: Update::RefreshImageCallbacks, ... }

Shell event loop sees RefreshImageCallbacks  
  → calls setNeedsDisplay (macOS) / etc.
  → does NOT set frame_needs_regeneration

drawRect fires
  → render_and_present_in_draw_rect()
    → frame_needs_regeneration is false → skip regenerate_layout() entirely
    → build_webrender_transaction()
      → process_image_callback_updates()  ← invokes render_my_texture() (only changed nodes)
      → display list unchanged → skip scene builder
    → send transaction to WebRender
    → renderer.render() + present
```

## Architecture Findings

### Key Types and Paths

| Component | Location | Role |
|-----------|----------|------|
| `Update` enum | `core/src/callbacks.rs:57` | `DoNothing`, `RefreshDom`, `RefreshDomAllWindows` |
| `ProcessEventResult` enum | `core/src/events.rs:61` | Internal: has `ShouldReRenderCurrentWindow` (unused by public API) |
| `TimerCallbackReturn` | `core/src/callbacks.rs` / `azul.h:5570` | `{ should_update: Update, should_terminate: TerminateTimer }` |
| `CallCallbacksResult` | `layout/src/callbacks.rs:3793` | Has `image_callbacks_changed` field (already exists!) |
| `CallbackChange::UpdateImageCallback` | `layout/src/callbacks.rs:220` | Already exists, marks specific node for re-render |
| `CallbackInfo::update_image_callback()` | `layout/src/callbacks.rs:876` | Already exists, pushes `UpdateImageCallback` change |
| `AzCallbackInfo_updateImageCallback` | `azul.h:29953` | Already exposed in C API |
| `TimerCallbackInfo` | `layout/src/timer.rs:275` | Has `callback_info: CallbackInfo` field (Derefs to it) |
| `process_image_callback_updates()` | `dll/src/desktop/wr_translate2.rs:2546` | Invokes render callbacks, updates GL textures |
| `regenerate_layout()` | `dll/src/desktop/shell2/common/layout_v2.rs:55` | Full DOM rebuild (the expensive part) |
| `render_and_present_in_draw_rect()` | `dll/src/desktop/shell2/macos/mod.rs:4683` | Main render entry point |

### What Already Exists

1. **`CallbackInfo::update_image_callback(dom_id, node_id)`** — marks a 
   specific image callback node for re-invocation. Already in the public C API.

2. **`CallCallbacksResult::image_callbacks_changed`** — field that tracks 
   which nodes need their image callback re-invoked.

3. **`process_image_callback_updates()`** in `wr_translate2.rs` — already 
   iterates all callback images and re-invokes them each frame. Currently 
   unconditional (always re-invokes ALL callbacks).

4. **`ProcessEventResult::ShouldReRenderCurrentWindow`** — internal enum 
   variant that means "re-render without DOM rebuild". Not exposed to users.

### What's Missing

1. **`Update::RefreshImageCallbacks`** — a new `Update` variant that 
   triggers a re-render (setNeedsDisplay) WITHOUT setting 
   `frame_needs_regeneration`. This would skip `regenerate_layout()` while 
   still running `build_webrender_transaction()` which invokes 
   `process_image_callback_updates()`.

2. **Timer path for `image_callbacks_changed`** — the timer result processing 
   in the macOS shell (`tick_timers` at mod.rs:430) currently only checks 
   `callbacks_update_screen` for `RefreshDom`. It doesn't process 
   `image_callbacks_changed` from the result or pass it to the render pipeline.

3. **Selective callback invocation** — `process_image_callback_updates()` 
   currently re-invokes ALL callback images unconditionally. It should 
   optionally accept a set of changed node IDs to only re-invoke those.

4. **Node ID lookup from timer** — the `animate()` function needs to know 
   the `DomId` and `NodeId` of the OpenGL image node to call 
   `update_image_callback()`. Since the timer doesn't directly know this, 
   options include:
   - Store the node ID in app state (set during `layout()`)
   - Use `get_node_id_by_id_attribute()` to find the node by an HTML `id`
   - Just use the new `Update::RefreshImageCallbacks` which re-invokes all 
     callbacks (simpler, almost equivalent for single-callback apps)

## Implementation Plan

### Phase 1: Add `Update::RefreshImageCallbacks` variant

**Files to modify:**

1. **`core/src/callbacks.rs`** — Add `RefreshImageCallbacks` to `Update` enum:
   ```rust
   pub enum Update {
       DoNothing,
       RefreshDom,
       RefreshDomAllWindows,
       RefreshImageCallbacks,  // NEW: re-invoke image callbacks without DOM rebuild
   }
   ```
   Update `max_self()` ordering: `DoNothing < RefreshImageCallbacks < RefreshDom < RefreshDomAllWindows`

2. **`api.json`** — Add `RefreshImageCallbacks` variant to the Update enum definition.

3. **`dll/azul.h`** — Regenerate or manually add `AzUpdate_RefreshImageCallbacks` to the enum.

4. **`dll/src/desktop/shell2/common/callback_processing.rs`** — Handle 
   `Update::RefreshImageCallbacks` → map to `ProcessEventResult::ShouldReRenderCurrentWindow`.

5. **`dll/src/desktop/shell2/common/event_v2.rs`** — Handle 
   `Update::RefreshImageCallbacks` in `process_callback_result_v2()`:
   - Do NOT call `mark_frame_needs_regeneration()`
   - Set result to `ShouldReRenderCurrentWindow`

### Phase 2: Update Shell Event Loops

**Files to modify (all platforms):**

6. **macOS: `dll/src/desktop/shell2/macos/mod.rs`**
   - In `tick_timers()` (~line 479): also trigger `setNeedsDisplay` for 
     `RefreshImageCallbacks` (but NOT set `frame_needs_regeneration`)
   - In `render_and_present_in_draw_rect()` (~line 4720): same treatment

7. **Windows: `dll/src/desktop/shell2/windows/mod.rs`**
   - Same pattern: trigger redraw for `RefreshImageCallbacks` without DOM rebuild

8. **X11: `dll/src/desktop/shell2/linux/x11/mod.rs`** and 
   **`x11/events.rs`**
   - Same pattern

9. **Wayland: `dll/src/desktop/shell2/linux/wayland/mod.rs`**
   - Same pattern

### Phase 3: Wire Up Timer → Image Callback Update

The timer callback return already has `CallCallbacksResult::image_callbacks_changed`.
When the timer callback calls `info.callback_info.update_image_callback(dom_id, node_id)`,
the change is recorded. This data needs to flow through to `process_image_callback_updates()`.

10. **`layout/src/window.rs`** — In `process_change_result()` (~line 3553), 
    the `image_callbacks_changed` is already forwarded to the 
    `CallCallbacksResult`. Verify this works for timer paths.

11. **`dll/src/desktop/wr_translate2.rs`** — In 
    `process_image_callback_updates()`, optionally accept a 
    `BTreeMap<DomId, FastBTreeSet<NodeId>>` to only re-invoke specific 
    callbacks instead of all. If empty/None, fall back to invoking all 
    (backwards compatible).

### Phase 4: Add C API Helpers

12. **`dll/azul.h`** — Add:
    ```c
    extern DLLIMPORT AzTimerCallbackReturn AzTimerCallbackReturn_continueAndRefreshImageCallbacks(void);
    ```

13. **`dll/src/lib.rs`** (or wherever the C API is generated) — Implement 
    the new function.

### Phase 5: Update Examples

14. **`examples/c/opengl.c`** — Update `animate()`:
    ```c
    AzTimerCallbackReturn animate(AzRefAny data, AzTimerCallbackInfo info) {
        OpenGlStateRefMut d = OpenGlStateRefMut_create(&data);
        if (!OpenGlState_downcastMut(&data, &d)) {
            return AzTimerCallbackReturn_terminateUnchanged();
        }
        
        d.ptr->rotation_deg += 1.0f;
        if (d.ptr->rotation_deg >= 360.0f) {
            d.ptr->rotation_deg = 0.0f;
        }
        OpenGlStateRefMut_delete(&d);
        
        // NEW: Only refresh the image callbacks, don't rebuild DOM
        return AzTimerCallbackReturn_continueAndRefreshImageCallbacks();
    }
    ```

15. **`examples/c/opengl_simple.c`** — Same change.

16. **`examples/rust/src/opengl.rs`** — Update `animate()`:
    ```rust
    extern "C" fn animate(mut timer_data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
        TimerCallbackReturn {
            should_terminate: TerminateTimer::Continue,
            should_update: match timer_data.downcast_mut::<OpenGlAppState>() {
                Some(mut s) => {
                    s.rotation_deg += 1.0;
                    Update::RefreshImageCallbacks  // was: Update::RefreshDom
                }
                None => Update::DoNothing,
            },
        }
    }
    ```

17. **C++ and Python examples** — Apply corresponding changes.

## Expected Performance Impact

| Metric | Before | After |
|--------|--------|-------|
| CPU usage (idle animation) | ~50% | ~1–5% |
| Per-frame work | layout() + style + flexbox + display list + render callback + composite | render callback + composite |
| Skipped per frame | — | DOM construction, CSS cascade, flexbox solver, display list diff |
| GPU work | Same (render texture + composite) | Same |

## Risks and Considerations

1. **Display list staleness**: If `RefreshImageCallbacks` skips 
   `regenerate_layout()` entirely, the display list from the previous frame 
   is reused. This is correct as long as no DOM structure or layout 
   properties changed. The current `process_image_callback_updates()` 
   already handles updating the GL texture in the existing display list via 
   WebRender's external image API.

2. **WebRender transaction still needed**: Even though the display list 
   doesn't change, we still need to send a frame generation request to 
   WebRender so it composites the new texture. The `generate_frame()` call 
   in the transaction handles this. We may need `txn.skip_scene_builder()` 
   + `txn.generate_frame()` to avoid unnecessary scene rebuilds.

3. **Multiple concurrent callbacks**: If the app has multiple image 
   callbacks (e.g., two OpenGL viewports), `RefreshImageCallbacks` 
   re-invokes ALL of them. This is fine — it's still O(callbacks) not 
   O(DOM nodes). For fine-grained control, `update_image_callback()` can 
   mark specific nodes.

4. **Backward compatibility**: Adding a new `Update` enum variant is an 
   ABI change. Existing compiled code using the old enum will still work 
   since the new variant has a higher discriminant, but recompilation is 
   recommended. The old `RefreshDom` path continues to work unchanged.

5. **Interaction with other changes**: If a timer both changes CSS 
   properties AND refreshes an image callback, the `max_self()` logic on 
   `Update` ensures `RefreshDom` takes priority over 
   `RefreshImageCallbacks`, so correctness is preserved.

## Testing Strategy

1. Run `opengl.c` and `opengl_simple.c` examples, verify animation still 
   works.
2. Monitor CPU usage — should drop from ~50% to ~1-5%.
3. Resize the window during animation — image should still re-render at 
   correct size (resize triggers `RefreshDom` via the resize event path, 
   which does full relayout including size recalculation).
4. Test with the debug server connected — verify inspection still works.
5. Run on all platforms (macOS, Windows, Linux X11, Linux Wayland).

## Alternative Approaches Considered

### A: Timer calls `update_image_callback()` + returns `DoNothing`

**Problem**: `DoNothing` doesn't trigger `setNeedsDisplay`, so no 
`drawRect` fires and the texture update never reaches the screen. We'd 
need the timer processing to check `image_callbacks_changed` and trigger 
a redraw, effectively reimplementing `RefreshImageCallbacks` implicitly.

### B: Make `process_image_callback_updates()` always run regardless of `frame_needs_regeneration`

**Problem**: This would run image callbacks even when no animation is 
active, wasting resources for static UIs. Also requires `setNeedsDisplay` 
to be called unconditionally, causing unnecessary composites.

### C: Use a direct GL swap without going through WebRender

**Problem**: The texture is composited by WebRender (it's an external 
image). Bypassing WebRender would mean losing the compositor features 
(border-radius clipping, box-shadow, child DOM nodes on top of the 
texture). The current architecture correctly uses WebRender for 
compositing — we just need to avoid the DOM rebuild.

### D: Add `Update::ReRender` (broader than just image callbacks)

This is a superset of the proposed approach. `Update::ReRender` would 
trigger a full re-render (rebuild display list + composite) without DOM 
rebuild. This is more general but also more expensive than just 
refreshing image callbacks. Could be added later if needed.

The chosen approach (`RefreshImageCallbacks`) is the minimal change that 
solves the specific problem with maximum performance benefit.
