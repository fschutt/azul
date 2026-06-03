# Incremental / Damage-Based Rendering — Architecture & Plan

Goal (user, 2026-06-03): **cursor blink, scroll, and window resize must each repaint only the
changed region** — on **CPU (tiny-skia) and GPU (WebRender)**, across **Wayland, X11, macOS, Windows** —
and the **system compositor must receive correct dirty rects**. Compiled from two code-investigation
agents. File:line refs are absolute to `/home/fs/Development/azul`.

## Corrected premises (don't re-derive)
- WebRender = vendored 0.62 fork at `webrender/`. `wr_translate2.rs:218-222` already sets
  `CompositorConfig::Draw { max_partial_present_rects: 1, draw_previous_partial_present_regions: false,
  partial_present: None }`. So WR **computes** per-frame dirty rects (`RenderResults.dirty_rects`,
  `renderer/mod.rs:5516`) and **all four platforms already consume them** into a `gpu_damage_rects` field.
- WebRender 0.62 **always** uses picture/tile caching → only re-rasters dirty tiles regardless of OS
  partial-present. So GPU raster is already incremental; OS partial-present only saves compositor work.
- `partial_present: None` is **correct** — the `PartialPresentCompositor` trait is only for EGL
  `KHR_partial_update` buffer-age; enabling it without a real buffer-age query forces a full-frame
  fallback (worse). Keep it None. Do NOT set `draw_previous_partial_present_regions: true`.

## Current behavior matrix (the bugs)
| Trigger | CPU (all 4 platforms) | GPU (WebRender) |
|---|---|---|
| **Cursor blink** | **FULL re-raster** every 530 ms: caret is a DL item gated on `cursor_is_visible`, so toggling changes the DL **item count** → `compute_display_list_damage` hits its `len != len` guard (`cpurender.rs:2322`) → returns `None` → full raster. | **macOS:** full **scene rebuild** + `flush_scene_builder` every blink (`macos/mod.rs:5400`). **Wayland/X11/Windows:** `ShouldUpdateDisplayListCurrentWindow` → lightweight path that **never pushes the regenerated DL** → blink likely invisible until another event forces a rebuild. |
| **Scroll** | **FROZEN content** on all 4: offset isn't in the DL → scroll-only frame = byte-identical DL → empty damage → empty-damage gate re-blits stale `retained_pixmap`. | **Cheap & correct:** `build_image_only_transaction` → `skip_scene_builder` + `set_scroll_offsets` (`wr_translate2.rs:2743/1842`). (WR's dirty rects are "very large" during scroll — minor.) |
| **Resize** | **realloc + FULL raster** everywhere (`acquire_pixmap` discards on dim mismatch, `cpurender.rs:2204`). | relayout + full scene rebuild (inherent); coalesced by the frame-callback gate. OK. |

## Root causes & the fixes
### Caret (shared, highest value — fixes all platforms at once)
- Emission: `layout/src/solver3/display_list.rs:1988` `paint_cursor` early-returns (emits NO item) when
  `!cursor_is_visible`; `push_cursor_rect` (`:1464`) also skips when `color.a == 0`. Blink driven by
  `cursor_blink_timer_callback` (`layout/src/window.rs:205`) → `SetCursorVisibility`
  (`common/event.rs:1851`) → `regenerate_display_list_for_dom` (`window.rs:5765`).
- **CPU fix:** always emit the `CursorRect` with a stable item count + an `is_visible`/alpha flag that
  `is_visually_equal` (`display_list.rs:886`) compares. Then a blink toggles exactly one item →
  damage = caret rect → `render_display_list_damaged` repaints ~2×16 px. **No backend change.**
- **GPU fix (proper):** make the caret a **GPU-opacity-animated** primitive bound to a WR property key;
  toggle it in `synchronize_gpu_values` (`wr_translate2.rs:1854`) from `should_draw_cursor()`
  (`window.rs:998`). Then blink = one `generate_frame` w/ a single opacity value on ALL platforms (no
  scene rebuild on macOS; visible on the others). Drop the `regenerate_display_list_for_dom` for
  `SetCursorVisibility`. (Code already anticipates this — comment at `event.rs:1860`.)

### Scroll (CPU)
`scroll_dirty` flag exists: `ScrollManager.has_pending_scroll_changes()` / `clear_scroll_dirty()`
(`scroll_state.rs:419/424`), set in `set_scroll_position[_unclamped]`, cleared only on full regen
(`window.rs:1306`). **Design 1 (minimal, do first):** on a scroll-only frame, damage = the scrolled
`PushScrollFrame.clip_bounds` (the scroll viewport) → `render_display_list_damaged(.., &[viewport])`;
`render_state.scroll_offsets` carries the live offset so items re-raster at the new position. Needs a
per-window `last_scroll_offsets` map (or reuse the `scroll_changed` flag). **Design 2 (optimal, later):**
persistent `CompositorState` + `scroll_layer` pixel-shift (`cpurender.rs:477`) + `compute_exposed_rects`
(`cpurender.rs:751`) — memmove the pixmap by the delta, raster only the exposed strip. Bigger refactor.
(Wayland CPU arm already computes `scroll_changed` at ~`mod.rs:3156` but currently doesn't use it.)

### Resize (CPU)
On grow: `retained_pixmap.resize_grow_only(pw,ph,..)` (`cpurender.rs:1076`) + `compute_resize_damage`
(`cpurender.rs:2406`) → right/bottom strips → `render_display_list_damaged`. Mirror the headless backend
(`headless/mod.rs:217-229`), which already does this. Shrink: realloc + full raster is fine. Must bypass
`render_with_font_manager_and_scroll_retained` (it `fill`s white + discards on mismatch).

### Wayland GPU damage ordering bug
`wayland/mod.rs:3084-3122`: `swap_buffers()` (=`eglSwapBuffers`, which attaches+commits) runs BEFORE the
`wl_surface_damage` calls and there's no `wl_surface_commit` after → GPU damage hints are dropped. **Fix:**
use `eglSwapBuffersWithDamageKHR(dpy, surf, rects, n)` (load via `eglGetProcAddress`; rects are
**device-px, bottom-left** `{x,y,w,h}`), drop the manual `wl_surface_damage`. Fallback to bare swap if the
ext is absent.

## Per-platform compositor sub-rect damage (CPU path)
| Platform | Blit | Damage API | Change |
|---|---|---|---|
| Wayland | shm + RGBA→ARGB copy (`mod.rs:3232`) | unconditional `wl_surface_damage(0,0,W,H)` (`:3262`); `present()` has an unused per-rect path via `cpu_state.damage_rects` (`:567`) | populate `damage_rects`; prefer `wl_surface_damage_buffer` per rect |
| X11 | `XPutImage(..,0,0,0,0,pw,ph)` whole (`mod.rs:2224`) | the `XPutImage` extent IS the update | pass damage rect to dst params `(dx,dy,w,h)` |
| macOS | `update_framebuffer`→`drawRect:` whole, ignores `_dirty_rect` (`mod.rs:989`) | `setNeedsDisplay(true)` whole (`:1895`); `setNeedsDisplayInRect:` path exists (`:5988`) but CPU never populates damage | populate damage + honor `dirty_rect` in `drawRect:` |
| Windows | `StretchDIBits(0,0,pw,ph,0,0,pw,ph)` whole (`mod.rs:895`); present OUTSIDE `BeginPaint` (no `rcPaint`) | per-rect `InvalidateRect` exists (`:3805`) but CPU never populates `gpu_damage_rects`; all callers pass NULL = whole window | populate damage + dst-rect on `StretchDIBits`. WGL native (no ANGLE/EGL, no DXGI) → GPU OS-damage not available; `InvalidateRect` is the lever |

## Implementation order (Wayland-first = the only one testable on this machine; GPU SIGSEGVs here → CPU)
- **Phase 0 — shared caret fix** (CPU: always-emit + visible flag in `display_list.rs`). Fixes blink
  full-raster on ALL CPU platforms; zero backend change. **Start here.**
- **Phase 1 — Wayland CPU**: scroll (Design 1) + resize (grow-only) + compositor sub-rect damage
  (`damage_rects` + per-rect `wl_surface_damage_buffer`). Verify with a temporary damage-rect log.
- **Phase 2 — ports**: X11, macOS, Windows CPU arms (near-copies) + their compositor damage.
- **Phase 3 — GPU**: caret GPU-opacity property (all platforms) + Wayland `eglSwapBuffersWithDamageKHR`
  ordering fix. (Hard to verify here — GPU crashes on this nouveau box.)
- **Phase 4 — optional**: Design 2 scroll pixel-shift; `RenderReasons` tagging; resize debounce.

## Verification note
Incremental repaint isn't visually distinguishable from full repaint (the caret blinks either way). Verify
by **logging the damage rects** (temporary `eprintln`, like the `[ADDR]` probes): caret→caret rect,
scroll→viewport rect, resize→edge strips — not full-window. Then remove the logs before commit.

## Key files
- CPU: `layout/src/cpurender.rs` (`compute_display_list_damage:2317`, `render_display_list_damaged:2686`,
  `render_with_font_manager_and_scroll_retained:2272`, `acquire_pixmap:2202`, `scroll_layer:477`,
  `compute_exposed_rects:751`, `compute_resize_damage:2406`, `resize_grow_only:1076`); headless reference
  `dll/src/desktop/shell2/headless/mod.rs:200-312`.
- Caret/DL: `layout/src/solver3/display_list.rs:886,1464,1982-2083`; `layout/src/window.rs:205,998,5765`;
  `dll/src/desktop/shell2/common/event.rs:1851`.
- GPU/WR: `dll/src/desktop/wr_translate2.rs:218,1771,1842,1854,2743`; `webrender/core/src/{composite.rs:270,
  renderer/mod.rs:4324(calculate_dirty_rects),5516(RenderResults)}`.
- CPU render arms: Wayland `linux/wayland/mod.rs:3136`; X11 `linux/x11/mod.rs:2069`; macOS
  `macos/mod.rs:5520`; Windows `windows/mod.rs:757`. Per-platform present+damage refs in the table above.
