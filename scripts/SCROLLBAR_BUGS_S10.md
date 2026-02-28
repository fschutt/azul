# Scrollbar Bug Report S10 — Resize Crash, Clipping, Overlay Space, Hit-Testing

**Date:** 2026-02-27  
**Branch:** `optimize-opengl-texture-swap`  
**Test binary:** `examples/c/scrolling.c` → `scrolling.bin`

---

## Overview

Four issues remain after the opacity-key and Taffy box-sizing fixes:

| # | Symptom | Severity | Root Cause |
|---|---------|----------|------------|
| 1 | **Crash on window resize** (SIGABRT in WRSceneBuilder) | P0 | `transform_keys` map shared between CSS transforms and scrollbar thumbs → duplicate `SpatialTreeItemKey` |
| 2 | **Scrollbar thumb not visible** (8 px reserved strip shows only background) | P1 | Thumb is drawn but opacity = 0 on first render; CSS-transform reference frame wraps the container on second frame, breaking coordinate space |
| 3 | **8 px space reserved for overlay scrollbar** | P2 | `get_layout_scrollbar_width_px` returns 0 for overlay, but system may be in Legacy mode; visual width and reserve width are conflated |
| 4 | **Hit-testing on scroll thumb doesn't work** | P2 | Thumb rect is inside a spurious reference frame (same root cause as 1) and opacity=0 makes it invisible → untestable |
| 5 | **Scroll buttons / corner rect not configurable** | P3 | Overlay mode should have no buttons / corner; these should be independently toggleable |

---

## Issue 1 — Crash on Resize (P0)

### Symptom

```
Thread 11 Crashed:: WRSceneBuilder#0
  webrender::spatial_tree::SceneSpatialTree::add_spatial_node  ← assert!(spatial_nodes_set.insert(uid))
  webrender::spatial_tree::SceneSpatialTree::add_reference_frame
  webrender::scene_building::SceneBuilder::build_reference_frame
```

Panic: **"duplicate key"** — two reference frames with the same `SpatialTreeItemKey` are
submitted to WebRender in the same scene build.

### Root Cause

**`GpuValueCache.transform_keys` is shared between CSS-transform reference frames
and scrollbar-thumb reference frames.**

Call chain on first render:

1. `layout_and_generate_display_list` (window.rs):  
   a. GPU cache is empty → display list builder finds nothing in `transform_keys`.  
   b. Scrollbar code calls `TransformKey::unique()` → e.g. TransformKey(1).  
   c. **Back-registration** (window.rs ~L815): stores `transform_keys[nid] = TransformKey(1)` and `current_transform_values[nid] = thumb_transform`.

2. `build_webrender_transaction` (wr_translate2.rs):  
   a. Step 1.8 `update_scrollbar_transforms` → updates `transform_keys[nid]` (same entry).  
   b. Step 2 translates the **already-built** display list → only one reference frame → OK.

Call chain on second render (resize):

1. `layout_and_generate_display_list`:  
   a. GPU cache now contains `transform_keys[nid] = TransformKey(1)` (from back-registration).  
   b. **CSS-transform check** (display_list.rs ~L1782):  
      ```rust
      let has_reference_frame = node.dom_node_id.and_then(|dom_id| {
          self.gpu_value_cache.and_then(|cache| {
              let key = cache.transform_keys.get(&dom_id)?;       // ← finds scrollbar key!
              let transform = cache.current_transform_values.get(&dom_id)?; // ← finds scrollbar value!
              Some((*key, *transform))
          })
      });
      ```
      This returns `Some((TransformKey(1), thumb_transform))` even though the node
      has **no CSS transform**. A `PushReferenceFrame` is emitted.  
   c. Scrollbar code (display_list.rs ~L3063) also uses `transform_keys.get(&nid)` → same TransformKey(1).  
   d. A `ScrollBarStyled` with `thumb_transform_key = TransformKey(1)` is emitted.

2. `translate_displaylist_to_wr` (compositor2.rs):  
   a. `PushReferenceFrame` → `builder.push_reference_frame(SpatialTreeItemKey(1, 0))`.  
   b. `ScrollBarStyled` → `builder.push_reference_frame(SpatialTreeItemKey(1, 0))`.  
   c. WebRender: duplicate UID → **panic**.

#### Why `GpuValueCache::synchronize()` doesn't help

The `synchronize()` method (gpu.rs ~L112) that would detect the stale entry and remove it
is **never called anywhere in the codebase** — it is dead code. The `transform_keys` map
is only mutated by:
- Back-registration (window.rs ~L815) — inserts scrollbar keys
- `update_transform_key` (gpu_state.rs ~L339) — inserts/updates scrollbar keys  
- `synchronize()` — would remove stale entries, **but is never called**

### Fix

**Option A (recommended):** Add separate maps to `GpuValueCache` for scrollbar thumb transforms:

```rust
// gpu.rs — GpuValueCache
pub scroll_v_transform_keys: BTreeMap<NodeId, TransformKey>,          // NEW
pub scroll_v_current_transform_values: BTreeMap<NodeId, ComputedTransform3D>,  // NEW
```

Update all scrollbar-related code to use the new maps:
- `display_list.rs:3063` → `cache.scroll_v_transform_keys.get(&nid)`
- `window.rs:815` → `gpu_cache.scroll_v_transform_keys.insert(…)`
- `gpu_state.rs:update_transform_key` → `gpu_cache.scroll_v_transform_keys`
- `wr_translate2.rs:synchronize_gpu_values` → iterate `scroll_v_transform_keys`

Keep `transform_keys` exclusively for CSS transforms (currently unused since
`synchronize()` is never called — will be needed if CSS transform animation is implemented).

**Option A is preferred** because it cleanly separates namespaces and prevents future
collisions when CSS transform animation is implemented.

### Files to Change

| File | Change |
|------|--------|
| `core/src/gpu.rs` | Add `scroll_v_transform_keys`, `scroll_v_current_transform_values` |
| `layout/src/solver3/display_list.rs:3063` | Read from `scroll_v_transform_keys` instead of `transform_keys` |
| `layout/src/window.rs:815` | Back-register into `scroll_v_transform_keys` |
| `layout/src/managers/gpu_state.rs:339` | Write to `scroll_v_transform_keys` |
| `dll/src/desktop/wr_translate2.rs` | `synchronize_gpu_values` iterates `scroll_v_transform_keys` |

---

## Issue 2 — Scrollbar Thumb Not Visible (P1)

### Symptom

An 8 px strip is visible at the right edge of the scroll container (background color shows
through), but the scrollbar thumb is not visible or only very faintly visible.

### Root Cause (compound)

Two contributing factors:

**A. Opacity initialization:**  
The back-registration (window.rs ~L847) initializes scrollbar opacity to `0.0`:
```rust
gpu_cache.scrollbar_v_opacity_values.insert(key, 0.0);
```
The compositor (compositor2.rs ~L482) creates the opacity binding with default `1.0`:
```rust
PropertyBinding::Binding(key, 1.0)  // initial: fully visible
```
But `synchronize_gpu_values` then pushes `0.0` from the cache, making the thumb invisible.
The opacity only becomes non-zero after a scroll event triggers `synchronize_scrollbar_opacity`.

**Before first scroll:** opacity = 0.0 → thumb invisible.  
**During/after scroll:** opacity fades in → thumb briefly visible, then fades out.

For **legacy scrollbars** (Always mode), the thumb should be visible at all times (opacity=1.0).

**B. Spurious reference frame (Issue 1 side-effect):**  
On the second render, the CSS-transform code creates a `PushReferenceFrame` wrapping the
entire scroll container node. This shifts all child coordinates into a new spatial space.
The scrollbar, painted after `pop_node_clips`, is still inside this reference frame.
With an identity transform, this appears harmless, but it can cause subtle coordinate
mismatches if the initial transform isn't exactly identity (the back-registration stores
the scrollbar's thumb translate as the "CSS transform").

### Fix

1. **For overlay mode:** Keep opacity = 0.0 initially; fade in on scroll. (Current behavior is correct for overlay, just needs the synchronize_scrollbar_opacity cycle to work.)

2. **For legacy/always mode:** Initialize opacity to 1.0 in the GPU cache, or skip the opacity wrapper entirely when `visibility == Always`. In `paint_scrollbars` (display_list.rs), check `scrollbar_style.visibility`:
   ```rust
   let opacity_key = if scrollbar_style.visibility == ScrollbarVisibilityMode::Always {
       None  // No opacity animation needed — always visible
   } else {
       node_id.map(|nid| { /* existing GPU cache lookup */ })
   };
   ```

3. **Fix Issue 1** to eliminate the spurious reference frame.

---

## Issue 3 — Overlay Scrollbar Reserves 8 px Layout Space (P2)

### Symptom

The scroll container content area is 8 px narrower than expected, with a visible
8 px background strip at the right edge.

### Analysis

`get_layout_scrollbar_width_px` (getters.rs ~L1876) correctly returns `0.0` for
`ScrollbarVisibility::WhenScrolling` (overlay mode). However:

1. **System mode depends on hardware:** macOS returns `NSScrollerStyleLegacy` (0) when
   a mouse is connected, `NSScrollerStyleOverlay` (1) for trackpad-only. Connecting a mouse
   switches to legacy mode where 8 px IS reserved.

   **Design requirement:** `SystemStyle` must be the **single source of truth** for all
   scrollbar configuration. On app startup (or on "system style reload"), the OS-native
   preferences are read once and stored in `SystemStyle`. From that point on, all scrollbar
   behaviour is derived from `SystemStyle` — **not** from live OS queries. This makes
   scrollbar rendering fully deterministic and allows previewing the look of other
   operating systems / hardware configurations without switching OS or plugging in a mouse.
   The user (or a theme system) can override any `SystemStyle` field at any time.

2. **The visual width and reserve width use a single field:** Both `get_scrollbar_style().width_px`
   (visual) and `get_layout_scrollbar_width_px` (reserve) currently derive from the same
   `width_px` in `ComputedScrollbarStyle`.

   **Design requirement:** `ComputedScrollbarStyle` must have **two separate fields**:
   - `visual_width_px: f32` — width used for rendering the track/thumb (e.g. 8 px)
   - `reserve_width_px: f32` — width subtracted from the content area during layout

   In overlay mode: `visual_width_px = 8`, `reserve_width_px = 0`.  
   In legacy mode: `visual_width_px = 8`, `reserve_width_px = 8`.  
   Both are independently configurable via CSS or `SystemStyle`.

3. **Apparent bug:** If the system is in legacy mode but the CSS specifies
   `-azul-scrollbar-visibility: when-scrolling`, the CSS visibility is applied (via
   `get_scrollbar_style` Step 5) but `get_layout_scrollbar_width_px` only checks the
   **system** preference, not the per-node CSS override. So CSS can request overlay behavior
   but the layout still reserves space.

### Fix

In `get_layout_scrollbar_width_px`, also check the per-node CSS scrollbar visibility:

```rust
pub fn get_layout_scrollbar_width_px<T>(ctx, dom_id, styled_node_state) -> f32 {
    // Check per-node CSS visibility first (takes priority over system default)
    let node_visibility = get_scrollbar_style(ctx.styled_dom, dom_id, styled_node_state, ctx.system_style.as_deref());
    if node_visibility.visibility == ScrollbarVisibilityMode::WhenScrolling {
        return 0.0; // overlay: no layout reservation
    }
    // Check system-level preference
    if let Some(ref sys) = ctx.system_style {
        match sys.scrollbar_preferences.visibility {
            ScrollbarVisibility::WhenScrolling => return 0.0,
            _ => {}
        }
    }
    get_scrollbar_width_px(ctx.styled_dom, dom_id, styled_node_state)
}
```

### Disambiguation: Visual Width vs Reserve Width

`ComputedScrollbarStyle` must carry **two** width fields:

```rust
pub struct ComputedScrollbarStyle {
    pub visual_width_px: f32,   // rendering width of track + thumb
    pub reserve_width_px: f32,  // layout space subtracted from content area
    // … other fields …
}
```

| Property | Overlay | Legacy |
|----------|---------|--------|
| `visual_width_px` (track/thumb rendering) | 8 px | 8 px |
| `reserve_width_px` (layout space) | 0 px | 8 px |

Consumers:
- `compute_scrollbar_geometry` uses `visual_width_px` for painting.
- `check_scrollbar_necessity` / `get_layout_scrollbar_width_px` uses `reserve_width_px`.

Both values are resolved from CSS (per-node) with fallback to `SystemStyle` defaults.

---

## Issue 4 — Scrollbar Hit-Testing Not Working (P2)

### Symptom

Clicking on the scroll thumb area does not initiate drag scrolling.

### Analysis

The hit-testing infrastructure is correctly implemented:

1. **Tag encoding:** `wr_translate_scrollbar_hit_id` (wr_translate2.rs ~L456) encodes
   `(DomId << 32 | NodeId, 0x0200 | component)` — distinguishing vertical/horizontal
   thumb/track.

2. **Hit-test push:** compositor2.rs ~L733 pushes `builder.push_hit_test(thumb_rect, …, tag)`
   for the thumb.

3. **Event dispatch:** `handle_scrollbar_click` (event.rs ~L3881) correctly initiates a
   `ScrollbarDragState` on thumb click.

4. **Drag handling:** `handle_scrollbar_drag` computes scroll delta from mouse position delta
   and applies it via scroll_manager.

### Root Causes (blocking)

1. **Thumb invisible (Issue 2):** opacity=0 means the user can't see where to click.
   However, hit-testing is spatial (not visual) so invisible elements CAN be hit-tested.

2. **Transform key collision (Issue 1):** The thumb is wrapped in a reference frame
   (from the ScrollBarStyled handler) that may use a stale/conflicting transform.
   Additionally, on the second render, a SPURIOUS reference frame wraps the entire
   container node (from the CSS-transform false positive). The hit-test rect for the
   thumb is pushed in the thumb's reference frame spatial space, but the spatial tree
   has a duplicate UID for that frame → crash before hit-testing can be tested.

3. **Hit-test rect may be at wrong coordinates:** The thumb_rect is resolved via
   `resolve_rect` which subtracts `current_offset`. The offset should be 0 for scrollbar
   items (they're outside the scroll frame). But if the spurious CSS-transform reference
   frame shifts the spatial origin, the hit-test coordinates could be wrong.

### Fix

Fix Issue 1 first (eliminate the duplicate reference frame). Then:
1. Verify hit-test rects match the visible thumb position (use AZUL_DEBUG to inspect)
2. Ensure the hit-test is pushed in the correct spatial space (same spatial_id as the thumb rect)
3. Fix Issue 2 so the thumb is visible for manual testing

### Additional Requirement: Hover / Active Visual Feedback

On macOS (and iOS), the overlay scrollbar **grows wider and changes color** when the
cursor hovers over the scrollbar area. This provides a larger drag target and clearer
affordance. This behaviour should be modeled via CSS pseudo-classes in the UA stylesheet:

```css
/* UA stylesheet — overlay scrollbar hover / active states */
.__azul_scrollbar_thumb:hover {
    -azul-scrollbar-visual-width: 12px;    /* grow from 8 → 12 */
    -azul-scrollbar-thumb-color: rgba(0, 0, 0, 0.55);  /* darken */
}
.__azul_scrollbar_thumb:active {
    -azul-scrollbar-visual-width: 12px;
    -azul-scrollbar-thumb-color: rgba(0, 0, 0, 0.70);  /* even darker while dragging */
}
```

This requires:
- The scrollbar thumb to participate in the normal hit-test → hover detection pipeline
  (already partially done via `TAG_TYPE_SCROLLBAR` tags)
- The scrollbar styling resolver (`get_scrollbar_style`) to accept `:hover` / `:active`
  state and resolve the appropriate CSS values
- The compositor to re-resolve scrollbar style when hover state changes (schedule repaint)

---

---

## Issue 5 — Configurable Scroll Buttons and Corner Rect

### Requirement

The legacy scrollbar on most desktop OSes shows **top / bottom arrow buttons** ("scroll one
line" buttons) and a **corner rect** (where vertical and horizontal scrollbars meet). The
overlay scrollbar on macOS has **none of these** — only the fading thumb + track.

In Azul, the presence of scroll buttons and the corner rect must be **independently
configurable**, regardless of whether the scrollbar is in overlay or legacy mode.

### Design

Add to `ComputedScrollbarStyle`:

```rust
pub struct ComputedScrollbarStyle {
    // … existing fields …

    /// Whether to show top/bottom (or left/right) arrow buttons.
    /// When false, no layout space is reserved for them.
    pub show_scroll_buttons: bool,

    /// Height (vertical) or width (horizontal) of each arrow button in px.
    /// Only used when `show_scroll_buttons == true`.
    pub scroll_button_size_px: f32,

    /// Whether to show the corner rect where V and H scrollbars meet.
    pub show_corner_rect: bool,
}
```

Defaults per mode:

| Property | Overlay (macOS trackpad) | Legacy (mouse / Windows) |
|----------|------------------------|--------------------------|
| `show_scroll_buttons` | `false` | `true` |
| `scroll_button_size_px` | — | 17 px (Windows), 0 px (macOS Legacy) |
| `show_corner_rect` | `false` | `true` |

These are resolved from CSS (per-node) with fallback to `SystemStyle`.

CSS properties (examples):
```css
.__azul_scrollbar {
    -azul-scrollbar-show-buttons: false;
    -azul-scrollbar-button-size: 0px;
    -azul-scrollbar-show-corner: false;
}
```

Layout impact:
- When `show_scroll_buttons == false`: the track spans the entire scrollbar length
  (no space reserved for buttons). The thumb travels the full track height.
- When `show_corner_rect == false`: content area extends into the corner.

---

## Dependency Graph

```
Issue 1 (Crash) ──blocks──► Issue 2 (Visibility)
                ──blocks──► Issue 4 (Hit-testing)
Issue 2 (Visibility) ──blocks──► Issue 4 (Hit-testing manual verification)
Issue 3 (Space reservation) is independent — CSS override propagation
Issue 5 (Buttons/corner) is independent — new feature
```

**Priority order:** 1 → 2 → 4 → 3 → 5

---

## Design: `SystemStyle` as Single Source of Truth

### Current State

Several scrollbar properties are queried live from the OS at various call sites:
- `NSScroller.preferredScrollerStyle` → overlay vs legacy (read in `system_native_macos.rs`)
- Scrollbar width, colors, visibility → mixture of CSS, OS defaults, hardcoded values

This makes behaviour hardware-dependent at runtime and impossible to preview cross-platform
styles within a single running application.

### Target Architecture

```
┌──────────────┐   startup / reload   ┌──────────────────────┐
│   OS APIs    │ ─────────────────────►│    SystemStyle       │  (frozen snapshot)
│  (NSScroller,│                       │                      │
│   GTK, Win32)│                       │  .scrollbar_prefs {  │
└──────────────┘                       │    visibility,       │
                                       │    visual_width_px,  │
                                       │    reserve_width_px, │
                                       │    show_buttons,     │
          user / theme override ──────►│    button_size_px,   │
                                       │    show_corner,      │
                                       │    thumb_color,      │
                                       │    track_color,      │
                                       │    fade_delay,       │
                                       │    fade_duration,    │
                                       │  }                   │
                                       └──────────┬───────────┘
                                                  │
                                     all consumers read from here
                                                  │
                      ┌───────────────┬───────────┼───────────────┐
                      ▼               ▼           ▼               ▼
               get_scrollbar_   check_scrollbar  paint_       compute_
               style()          _necessity()     scrollbars() scrollbar_
                                                              geometry()
```

**Rules:**
1. OS is queried **once** at startup (and on explicit "reload system style" event).
2. The result is stored in `SystemStyle.scrollbar_preferences`.
3. All layout, painting, and hit-testing code reads **only** from `SystemStyle`.
4. The user (or a theme engine) can override any field in `SystemStyle` at any time.
5. Per-node CSS properties override `SystemStyle` when present.

This means: on a trackpad-only Mac, you can set `SystemStyle.scrollbar_prefs.visibility = Always`
to get legacy scrollbar behaviour without plugging in a mouse. Or set
`show_scroll_buttons = true` on an overlay scrollbar to get a hybrid look.

---

## Reproduction

```bash
cd /Users/fschutt/Development/azul
cargo build --features build-dll --release
cd examples/c
cc -o scrolling.bin scrolling.c -lazul -L../../target/release -I../../dll
AZUL_DEBUG=8765 DYLD_LIBRARY_PATH=../../target/release ./scrolling.bin
# → Resize the window → crash
```

Debug API queries:
```bash
curl -s -X POST http://localhost:8765/ -d '{"op":"get_display_list"}'
curl -s -X POST http://localhost:8765/ -d '{"op":"get_scroll_states"}'
```
