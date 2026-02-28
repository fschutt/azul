# Scrollbar Bug Tracker

## scrolling.c Bugs

- [x] **BUG-S1: Scrollbar thumb does not sync with trackpad scroll** *(Phase 2)*
  The scrollbar thumb stays frozen during GPU-only scroll. It only syncs on
  window resize (which triggers a full display list rebuild).
  **Root cause:** `paint_scrollbars()` bakes the thumb position into the display
  list at build time. The GPU-only scroll path (`build_image_only_transaction`)
  skips display list rebuild — it only calls `scroll_all_nodes()` +
  `synchronize_gpu_values()`. The thumb *opacity* is GPU-animated via
  `PropertyBindingKey<f32>`, but the thumb *position* is not.
  **Fix:** Make the scrollbar thumb position GPU-animated via a
  `PropertyBindingKey<LayoutTransform>` (translate the thumb by the scroll
  ratio × track length). On each scroll tick, `synchronize_gpu_values()` pushes
  the updated transform.

- [x] **BUG-S2: Scrollbar track/thumb geometry is wrong** *(Phase 1)*
  The scrollbar track ignores: (a) the corner rect when both scrollbars are
  visible, (b) the space for increment/decrement buttons. The thumb math also
  doesn't account for button space.
  **Fix:** The track's usable height should be:
  `track_height - corner_size - 2 × button_size`. The thumb range is within
  that usable region. The thumb has a minimum height (e.g. `3 × width_px`).
  At scroll=0 the thumb's top edge is at the top of the usable region; at
  scroll=max the thumb's bottom edge is at the bottom of the usable region.
  Position is linearly interpolated between these two bounds.

- [x] **BUG-S3: Scrollbar hit-testing and dragging non-functional** *(Phase 1+2)*
  Clicking and dragging the scrollbar thumb/track has no visible effect.
  **Root cause candidates:**
  (a) Hit test tags may not be generated correctly in compositor2.rs,
  (b) `perform_scrollbar_hit_test` may not find the tags,
  (c) `handle_scrollbar_drag` may calculate wrong deltas,
  (d) The drag result (`ShouldReRenderCurrentWindow`) may not produce a
      visible change because of BUG-S1 (thumb is baked in display list).
  **Fix:** Debug the hit-test → drag → gpu_scroll pipeline end-to-end.
  Needs printlns at each stage.

- [x] **BUG-S4: No overscroll / rubber-banding on trackpad** *(Phase 5)*
  The rubber-banding system exists (`rubber_band_clamp()` in scroll_timer.rs)
  but only applies to `WheelDiscrete` momentum. `TrackpadContinuous` inputs
  are hard-clamped: `position.x.max(0.0).min(info.max_scroll_x)`.
  macOS `ScrollPhysics::macos()` has `overscroll_elasticity: 0.3` but
  it's never used for trackpad inputs.
  **Fix:** For `TrackpadContinuous`, when position exceeds bounds, apply
  `rubber_band_clamp()` instead of hard clamp. Need a spring-back mechanism
  that triggers when the trackpad gesture ends (phase == `Ended`/`Cancelled`).
  Currently the scroll timer self-terminates when velocity is zero and no
  pending inputs — it doesn't know about overscroll spring-back for trackpad.
  Need to detect "trackpad ended while overscrolled" and switch to spring-back
  physics (same as `WheelDiscrete` rubber-banding).
  **Implementation:** Need to pass NSEvent phase (`Began`/`Changed`/`Ended`)
  through as a new `ScrollInputSource::TrackpadEnd` event. When the timer
  sees `TrackpadEnd` and current position is overscrolled, switch the node
  from `pending_positions` to `node_velocities` with zero velocity and
  `is_rubber_banding = true`. The existing spring physics will then pull
  it back.

- [x] **BUG-S5: Scrollbar styling is hardcoded, CSS not wired** *(Phase 3)*
  `paint_scrollbars()` reads `ComputedScrollbarStyle` from CSS, including
  `scrollbar-color` and `scrollbar-width`. But button colors are hardcoded
  to debug green (`ColorU { r: 144, g: 238, b: 144, a: 255 }`).
  **Fix:** Use `scrollbar_style.button_color` instead of hardcoded green.
  Also verify that `-azul-scrollbar-style` custom property is properly
  applied.

- [x] **BUG-S6: Default scrollbar style not OS-specific** *(Phase 3 + CSS properties)*
  The CSS system loads `system_native_macos.rs` styles which include
  `SCROLLBAR_MACOS_LIGHT`/`DARK` presets (8px, transparent track,
  semi-transparent thumb). But the `get_scrollbar_style()` default fallback
  uses 16px width with opaque colors, not the OS-specific style.
  **Fix:** Wire the system style's scrollbar presets as the default in
  `get_scrollbar_style()` when no CSS overrides are set. The macOS preset
  should be 8px overlay, Windows should be classic 16px.

- [ ] **BUG-S7: macOS scrollbars should be overlay and fade out** *(Phase 4)*
  `ScrollbarVisibility::WhenScrolling` is correctly detected from
  `NSScroller.preferredScrollerStyle`. Layout reservation returns `0.0` for
  overlay mode. But the fade-in/fade-out animation needs to work:
  - Show scrollbar (opacity 1.0) when scroll starts
  - Keep visible during scroll
  - Fade to 0.0 after ~1.5s of inactivity
  The `scrollbar_v_opacity_keys` / `scrollbar_h_opacity_keys` in gpu_cache
  exist for GPU-animated opacity. The animation needs to be driven by the
  scroll activity timestamp (`AnimatedScrollState::last_activity`).
  **Fix:** In `synchronize_gpu_values()`, check each scrollbar's
  `last_activity` vs current time. If within fade window → opacity 1.0.
  If past fade window → lerp to 0.0. This requires the compositor to always
  emit the scrollbar even when "invisible" (opacity=0), so the GPU key
  exists for animation.

- [x] **BUG-S8: ~100px padding at end of scroll frame** *(fixed: padding-box coordinate alignment)*
  Extra whitespace at the bottom of the scroll content that shouldn't be there.
  **Root cause candidates:**
  (a) `content_size` includes padding/border that shouldn't be counted,
  (b) `max_scroll` calculation adds extra padding,
  (c) The layout engine's `overflow_content_size` is too large.
  **Fix:** Debug by printing `content_size`, `container_size`, `max_scroll`
  values during layout and comparing with expected.

- [x] **BUG-S9: Footer pushed off-screen (layout bug)** *(Phase 8)*
  The bottom footer is barely visible and pushed off-screen. This is a layout
  bug, not scroll-related. Likely the scroll container's flex grow/shrink
  is consuming all available space leaving no room for the footer.
  **Fix:** Investigate the DOM structure of scrolling.c to confirm whether
  the footer is a sibling of the scroll container. If so, the scroll container
  needs `flex-shrink: 1` and `overflow: auto` with a constrained height,
  while the footer needs `flex-shrink: 0`.

- [ ] **BUG-S10: Text selection + scroll-to-follow not implemented**
  Standard browser behavior: when selecting text and dragging beyond the
  visible area, the content should auto-scroll to follow the selection.
  Also needs to work when the cursor goes outside the window.
  **Implementation:** This is a new feature. During a text selection drag:
  - Track cursor position relative to scroll container bounds
  - If cursor is below container: scroll down at a rate proportional to
    the distance beyond the edge
  - If cursor is above: scroll up similarly
  - Continue even when cursor is outside the window (requires mouse capture)
  **Priority:** Medium — depends on text selection being fixed first.

- [ ] **BUG-S11: Scrollbar drag must work outside window** *(Phase 6)*
  When dragging the scrollbar thumb, the drag must continue working even
  when the cursor moves outside the window. The drag should only release
  when the mouse button is released.
  **Implementation:** On macOS, this requires not releasing the mouse capture
  during scrollbar drag. The `handle_scrollbar_drag()` is already called on
  mouse move events — just need to ensure mouse events are still delivered
  when cursor exits the window.

## infinity.c Bugs

- [ ] **BUG-I1: IFrame not showing scrollbar** *(Phase 7)* (partially fixed)
  The IFrame callback is invoked but the IFrame container doesn't show a
  scrollbar. The `virtual_scroll_size` needs to be set by the IFrame callback
  to indicate total scrollable content size, and the scroll manager must use
  that for `is_node_scrollable()` and scrollbar geometry.

- [x] **BUG-I2: IFrame scroll is "blocky" (low priority)** *(Phase 7)*
  Scroll used to update only when a full row (30px) was scrolled. Should
  be smooth per-pixel scrolling.

---

## Architecture Analysis

### The Core Problem: Scrollbar Thumb is Baked in Display List

The display list is built once and cached. During GPU-only scroll
(`build_image_only_transaction`), only WebRender scroll offsets and
GPU-animated properties (opacity, transform) are updated. The scrollbar
thumb *position* is computed in `paint_scrollbars()` at display list build
time and stored as a fixed `LogicalRect` in `ScrollbarDrawInfo.thumb_bounds`.

**Two viable solutions:**

#### Option A: GPU-Animated Scrollbar Thumb (Recommended)

Make the thumb position a GPU-animated transform:

1. In `paint_scrollbars()`, place the thumb at position `scroll_ratio = 0`
   (top of track) and assign a `PropertyBindingKey<LayoutTransform>`.
2. In `synchronize_gpu_values()`, compute the current scroll ratio and push
   a translate transform: `translate(0, scroll_ratio * usable_track_height)`.
3. Hit-testing needs the *current* thumb position, not the display-list
   position. Update `ScrollbarState.track_rect` in `calculate_scrollbar_states()`
   which already runs on every scroll event.

This keeps the GPU-only scroll path intact (no display list rebuild per frame).

#### Option B: Lightweight Display List Rebuild on Scroll

Change `CallbackChange::ScrollTo` to return
`ShouldUpdateDisplayListCurrentWindow` instead of `ShouldReRenderCurrentWindow`.
This forces a full display list rebuild on every scroll frame.

**Pros:** Simple, no new GPU animation infrastructure needed.
**Cons:** Defeats the purpose of GPU-only scrolling. CPU cost per scroll frame.

### Refactorings Needed

1. **Scrollbar geometry centralization**: Both `paint_scrollbars()` in
   display_list.rs and `calculate_scrollbar_states()` in scroll_state.rs
   compute thumb geometry independently with slightly different formulas.
   These must be unified into a single `ScrollbarGeometry` struct computed
   once and shared.

2. **GPU transform keys for scrollbar thumb**: The gpu_value_cache already
   has `scrollbar_v_opacity_keys` and `scrollbar_h_opacity_keys`. Need to add
   `scrollbar_v_transform_keys` and `scrollbar_h_transform_keys` for thumb
   position animation.

3. **Overscroll for trackpad**: The scroll timer needs TrackpadEnd events
   and spring-back physics for overscrolled trackpad positions.

4. **Corner rect**: When both scrollbars are visible, the bottom-right corner
   (width×width square) should be reserved and drawn as a neutral background.
   Neither scrollbar should extend into that corner.

5. **Button space**: The increment/decrement buttons are drawn but their
   space is not subtracted from the track's usable area for thumb positioning.

---

## Holistic Bug Analysis

### Bug Dependency Graph

Many of these bugs share root causes. Fixing them individually would be wasteful;
instead, we should identify the shared root causes and fix those.

```
                 ┌────────────────────────────┐
                 │   ROOT CAUSE 1:            │
                 │   Thumb position baked in   │
                 │   display list at build     │
                 │   time, not GPU-animated    │
                 ├────────────────────────────┤
                 │ S1: Thumb frozen on scroll  │
                 │ S3: Hit-test uses stale pos │
                 │ S7: Fade opacity needs GPU  │
                 └─────────┬──────────────────┘
                           │
                 ┌─────────▼──────────────────┐
                 │   ROOT CAUSE 2:            │
                 │   Three independent thumb   │
                 │   geometry calculations     │
                 │   with different formulas   │
                 ├────────────────────────────┤
                 │ S2: Wrong geometry          │
                 │ S3: Hit-test misaligned     │
                 │ S8: Wrong content_size      │
                 └─────────┬──────────────────┘
                           │
                 ┌─────────▼──────────────────┐
                 │   ROOT CAUSE 3:            │
                 │   Hardcoded styling /       │
                 │   missing OS defaults       │
                 ├────────────────────────────┤
                 │ S5: Hardcoded button colors │
                 │ S6: Default not OS-specific │
                 │ S7: Overlay not fading      │
                 └────────────────────────────┘

                 ┌────────────────────────────┐
                 │   ROOT CAUSE 4:            │
                 │   TrackpadContinuous has    │
                 │   no phase awareness        │
                 ├────────────────────────────┤
                 │ S4: No overscroll / rubber  │
                 │     banding on trackpad     │
                 └────────────────────────────┘

                 ┌────────────────────────────┐
                 │   ROOT CAUSE 5:            │
                 │   Missing mouse capture     │
                 │   during drag               │
                 ├────────────────────────────┤
                 │ S10: Text sel + auto-scroll │
                 │ S11: Drag outside window    │
                 └────────────────────────────┘

                 ┌────────────────────────────┐
                 │   ROOT CAUSE 6:            │
                 │   IFrame scroll integration │
                 │   not wired                 │
                 ├────────────────────────────┤
                 │ I1: No scrollbar for iframe │
                 │ I2: Blocky scroll           │
                 └────────────────────────────┘

                 ┌────────────────────────────┐
                 │   INDEPENDENT:             │
                 │   S9: Footer pushed off     │
                 │   (pure CSS layout bug)     │
                 └────────────────────────────┘
```

### Root Cause 1: Three-Code-Path Geometry Divergence (S1, S2, S3, S8)

**The single most impactful problem.** Scrollbar thumb geometry is computed
in THREE independent places, each with different formulas:

#### Location 1: `paint_scrollbars()` in display_list.rs (Lines ~3060-3100)
Used at display list build time. Computes `thumb_offset_y` as:
```
viewport_height = inner_rect.size.height
thumb_ratio = viewport_height / content_size.height
thumb_height = track_height * thumb_ratio
max_scroll = content_size.height - viewport_height
scroll_ratio = scroll_offset_y.abs() / max_scroll
thumb_offset_y = (track_height - thumb_height) * scroll_ratio
```
Uses `get_content_size()` on the layout tree node. Gets `track_height` by
subtracting horizontal scrollbar width (if present) from `inner_rect.size.height`.
Does NOT subtract button sizes from track. Thumb position is baked into the
display list.

#### Location 2: `compute_vertical_thumb_transform()` in gpu_state.rs (Lines ~176-196)
Used during GPU-only scroll updates. Computes identically except:
```
track_height = container_size.height - scrollbar_info.scrollbar_height
thumb_height = (container_size.height / content_size.height) * track_height
```
- Uses `container_size.height` (= `used_size`) instead of `inner_rect.size.height`.
  `inner_rect` equals `paint_rect minus borders` in display_list.rs;
  `container_size` equals `used_size` which may or may not include borders.
  **THIS IS A MISMATCH.** If the node has borders, these will differ.
- Uses `scrollbar_info.scrollbar_height` (from `ScrollbarRequirements`) vs
  display_list.rs uses `scrollbar_style.width_px` (from CSS).
  These SHOULD be the same but go through different code paths.
- Gets `content_size` from `inline_layout_result.bounds()` if available,
  falling back to `container_size`.
  **ANOTHER MISMATCH.** `paint_scrollbars()` uses `node.get_content_size()`
  which returns `overflow_content_size` from the positioned tree node.

#### Location 3: `calculate_scrollbar_states()` in scroll_state.rs (Lines ~860-890)
Used for hit-testing on mouse click/drag. Computes:
```
SCROLLBAR_WIDTH = 16.0 (HARDCODED constant)
thumb_size_ratio = container_height / content_height
thumb_position_ratio = current_offset.y / max_scroll
```
- Uses `AnimatedScrollState.container_rect.size.height` and
  `AnimatedScrollState.content_rect.size.height`.
- Hardcodes `SCROLLBAR_WIDTH = 16.0` instead of reading CSS.
- Computes `track_rect` at right edge of container — but `container_rect`
  is set during `register_or_update_scroll_node()` which may store the
  content-box or border-box depending on caller.
- `hit_test_component()` subtracts 2×button_height from track, but
  `paint_scrollbars()` doesn't subtract button heights from thumb range.

**Impact:** When the user drags the scrollbar thumb (using Location 3 geometry
for hit-test), the converted scroll delta is wrong because the geometry
(track size, button offset) doesn't match what was actually rendered
(Location 1) or what the GPU transform expects (Location 2).

### Root Cause 2: GPU Transform Pipeline Incomplete (S1, S7)

The GPU transform for the scrollbar thumb position has been partially
implemented but has several issues:

1. **Transform key registration ordering.** In window.rs:803-828, after
   `layout_document()`, we scan the display list for `ScrollBarStyled` items
   and register their `thumb_transform_key` in the GPU cache. But
   `update_scrollbar_transforms()` is also called there (line ~830), which
   looks up `scrollbar_info` from `layout_tree.nodes[].scrollbar_info` —
   this field might be `None` for nodes that have scrollbars painted by
   `paint_scrollbars()` but whose `scrollbar_info` in the LayoutTree was
   never populated (it's only set during BFC/Taffy layout, not during
   display list generation).

2. **The LayoutTree `scrollbar_info` vs display list `get_scrollbar_info_from_layout()`.**
   `update_scrollbar_transforms()` in gpu_state.rs iterates
   `layout_tree.nodes` looking for `scrollbar_info: Some(...)`. But
   `paint_scrollbars()` calls `get_scrollbar_info_from_layout(node)` which
   reads `node.overflow_x/overflow_y` and computes scrollbar presence from
   content vs used sizes. The LayoutTree `scrollbar_info` field might not be
   set if the node was NOT processed by compute_child_layout or
   compute_taffy_scrollbar_info. **This means update_scrollbar_transforms
   silently skips nodes it should be updating.**

3. **Horizontal transform TODO.** `paint_scrollbars()` sets
   `thumb_transform_key = None` for horizontal scrollbars. So horizontal
   thumb will never be GPU-animated.

4. **`content_size` mismatch.** `update_scrollbar_transforms` uses
   `inline_layout_result.bounds()` for content_size. `paint_scrollbars()`
   uses `node.get_content_size()`. Different values → different thumb
   positions → visual jump when switching between the two paths.

### Root Cause 3: Style/Defaults Wiring (S5, S6, S7)

These are all wiring issues where infrastructure exists but isn't connected:

- `scrollbar_style.button_color` exists in CSS but `paint_scrollbars()` uses
  `debug_button_color` (hardcoded green).
- macOS native scrollbar presets exist (`SCROLLBAR_MACOS_LIGHT/DARK` in
  system_native_macos.rs) but `get_scrollbar_style()` doesn't use OS defaults.
- `scrollbar_v_opacity_keys` exist in GPU cache but nobody populates them
  at display list build time (only transform keys are registered in
  window.rs:803-828). The fade animation in `GpuStateManager::tick()` is a
  placeholder that returns `GpuTickResult::default()`.

### Root Cause 4: TrackpadEnd Phase Not Propagated (S4)

The macOS event handler converts `NSEvent.scrollingDeltaX/Y` to either
`TrackpadContinuous` (if `hasPreciseScrollingDeltas`) or `WheelDiscrete`.
The scroll timer hard-clamps `TrackpadContinuous` positions:
```rust
position.x.max(0.0).min(info.max_scroll_x)
```
There's no `TrackpadEnd` variant in `ScrollInputSource`. When the user lifts
their fingers, no event fires to trigger spring-back from overscroll.

The rubber-banding infrastructure is fully implemented for `WheelDiscrete`:
`calculate_overshoot()`, `rubber_band_clamp()`, `spring_constant_from_bounce_duration()`.
It just needs to be wired for trackpad too.

### Root Cause 5: No Mouse Capture (S10, S11)

On macOS, mouse events stop being delivered when the cursor exits the window.
`handle_scrollbar_drag()` relies on continuous mouse move events. Without mouse
capture, dragging the scrollbar thumb outside the window silently stops working.

Similarly, text selection with auto-scroll (S10) requires mouse capture plus a
timer to keep scrolling while the cursor is outside the scroll container bounds.

### Root Cause 6: IFrame Scroll (I1, I2)

IFrame nodes set `virtual_scroll_size` to declare their total scrollable area.
The scrollbar visibility check in `paint_scrollbars()` uses
`get_scrollbar_info_from_layout()`, which computes from `overflow_content_size`.
For IFrame nodes, the `overflow_content_size` might not include the virtual
content that exists beyond the currently-rendered viewport slice.

`calculate_scrollbar_states()` in `scroll_state.rs` DOES respect
`virtual_scroll_size` (lines 822-828), so the hit-testing path is already correct
for IFrames. The rendering path (`paint_scrollbars`) is not.

### S9: Footer Bug (Independent, CSS Only)

In scrolling.c, the DOM is:
```
body (flex-column, height:100%)
  ├─ title  (fixed height)
  ├─ container (flex-grow:1, overflow:auto, height:400px)
  └─ footer (fixed height)
```
The `flex-grow:1` on the container consumes all remaining space. But
`height:400px` is also set, creating a conflict. The flex layout resolves by
growing the container to fill all space, pushing the footer off-screen.

**Fix:** Remove `height: 400px` from the container or use `max-height: 400px`
and `flex-shrink: 0` on the footer. This is a test case fix, not an engine bug.

---

## Action Plan

### Phase 1: Unified Scrollbar Geometry (Fixes S2, S3, S8, Prerequisite for S1)

**Goal:** One source of truth for all scrollbar calculations.

**Step 1.1: Create `ScrollbarGeometry` struct**

Location: `layout/src/solver3/scrollbar.rs`

```rust
/// Single source of truth for scrollbar geometry.
/// Computed once, used by: display list painting, GPU transform updates,
/// hit-testing, and drag delta conversion.
pub struct ScrollbarGeometry {
    /// Orientation
    pub orientation: ScrollbarOrientation,
    /// The full track rect (in the container's coordinate space)
    pub track_rect: LogicalRect,
    /// Button heights/widths (top/left and bottom/right)
    pub button_size: f32,
    /// Usable track length = track total - corner - 2×button_size
    pub usable_track_length: f32,
    /// The thumb length (min-clamped)
    pub thumb_length: f32,
    /// Scroll ratio (0.0 at top/left, 1.0 at bottom/right)
    pub scroll_ratio: f32,
    /// Thumb offset from usable track start
    pub thumb_offset: f32,
    /// Max scroll distance in content pixels
    pub max_scroll: f32,
    /// CSS-specified scrollbar width
    pub width_px: f32,
}
```

**Step 1.2: Single compute function**

```rust
pub fn compute_scrollbar_geometry(
    orientation: ScrollbarOrientation,
    inner_rect: LogicalRect,    // content-box of scroll container
    content_size: LogicalSize,   // from get_content_size() or virtual_scroll_size
    scroll_offset: f32,          // current scroll position (y for vertical, x for horizontal)
    scrollbar_width_px: f32,     // CSS scrollbar-width value
    has_other_scrollbar: bool,   // true if perpendicular scrollbar is visible
) -> ScrollbarGeometry
```

This function computes ALL derived values. The button_size = scrollbar_width_px.
The corner is subtracted when `has_other_scrollbar`. The usable_track_length =
track_total - 2*button_size - (corner if applicable). The thumb_length is
clamped to min(3 * width_px, track_total/2). The scroll_ratio and thumb_offset
are linear interpolation within the usable region.

**Step 1.3: Use ScrollbarGeometry everywhere**

- `paint_scrollbars()`: Call `compute_scrollbar_geometry()`, use results for
  all rendering and hit IDs. Store in display list as `ScrollbarDrawInfo`.
- `compute_vertical_thumb_transform()` in gpu_state.rs: Reuse the SAME function.
  Currently uses different input (container_size vs inner_rect). Must be fixed
  to use inner_rect (= content-box = used_size - borders).
- `calculate_scrollbar_states()` in scroll_state.rs: Reuse the SAME function.
  Delete the separate `calculate_vertical_scrollbar_static` /
  `calculate_horizontal_scrollbar_static` functions. Delete the hardcoded
  `SCROLLBAR_WIDTH = 16.0`.
- `hit_test_component()`: Use geometry from `ScrollbarGeometry` (usable_track_length,
  button_size, thumb_length, thumb_offset).

**Step 1.4: Fix content_size source**

The `get_content_size()` method on the positioned tree node should be the
single source. `update_scrollbar_transforms()` in gpu_state.rs currently
falls back to `inline_layout_result.bounds()`. Change it to store
`content_size` in the LayoutTree node alongside `scrollbar_info`, populated
during layout (not display list gen). Alternatively, store the
`ScrollbarGeometry` itself in the layout tree node.

**Estimated files to change:**
- `layout/src/solver3/scrollbar.rs` (add struct + compute fn)
- `layout/src/solver3/display_list.rs` (use compute fn in paint_scrollbars)
- `layout/src/managers/gpu_state.rs` (use compute fn in update_scrollbar_transforms)
- `layout/src/managers/scroll_state.rs` (use compute fn in calculate_scrollbar_states, hit_test_component)

### Phase 2: GPU Transform Pipeline for Thumb (Fixes S1)

**Goal:** Scrollbar thumb moves smoothly during GPU-only scroll.

**Step 2.1: Ensure scrollbar_info is populated in LayoutTree**

The current partial implementation creates TransformKey in `paint_scrollbars()`
but `update_scrollbar_transforms()` in gpu_state.rs skips nodes where
`layout_tree.nodes[].scrollbar_info` is `None`. Fix: populate `scrollbar_info`
for ALL overflow nodes during layout (BFC + Taffy paths), not just
during display list generation.

**Step 2.2: Store content_size in LayoutTree node**

Add `content_size: Option<LogicalSize>` to `LayoutTreeNode`. Populate during
layout. `update_scrollbar_transforms()` uses this instead of guessing from
`inline_layout_result`.

**Step 2.3: Fix transform formula to match paint formula**

`compute_vertical_thumb_transform()` must use the SAME formula as
`paint_scrollbars()`. After Phase 1 (unified geometry), both call the same
function, so this is automatic.

**Step 2.4: Wire horizontal thumb transform**

Duplicate the vertical pattern for horizontal. Add
`scrollbar_h_transform_keys` to GpuValueCache (analogous to existing
`scrollbar_v_opacity_keys`).

**Step 2.5: Test**

Rebuild and run `scrolling.bin`. The scrollbar thumb should move smoothly
during trackpad scroll without any display list rebuild. Verify with
existing debug printlns that `synchronize_gpu_values` pushes transform
updates on each scroll tick.

**Estimated files to change:**
- `layout/src/solver3/layout_tree.rs` (add content_size field)
- `layout/src/solver3/cache.rs` or `taffy_bridge.rs` (populate scrollbar_info + content_size)
- `layout/src/managers/gpu_state.rs` (use unified geometry, add horizontal support)
- `layout/src/window.rs` (simplify key registration)
- `core/src/gpu.rs` (add h_transform_keys if needed)

### Phase 3: Scrollbar Styling & OS Defaults (Fixes S5, S6)

**Goal:** Scrollbars match the OS default appearance.

**Step 3.1: Wire CSS button_color**

In `paint_scrollbars()`, replace:
```rust
let debug_button_color = ColorU { r: 144, g: 238, b: 144, a: 255 };
```
with:
```rust
let button_color = scrollbar_style.button_color;
```

**Step 3.2: OS-specific defaults in get_scrollbar_style()**

When no CSS override is set, `get_scrollbar_style()` should return:
- macOS: 8px, transparent track, semi-transparent grey thumb (from
  `SCROLLBAR_MACOS_LIGHT`), no buttons
- Windows: 16px, light grey track, dark grey thumb, arrow buttons
- Linux: 16px (GTK-like fallback)

Wire `system_native_macos.rs` presets. This involves making
`get_scrollbar_style()` accept a platform parameter or checking at runtime.

**Estimated files to change:**
- `layout/src/solver3/display_list.rs` (use scrollbar_style.button_color)
- CSS style resolution code (wire OS defaults into get_scrollbar_style)
- Possibly `core/src/window.rs` (pass platform info to layout)

### Phase 4: Opacity Fade Animation (Fixes S7)

**Goal:** macOS overlay scrollbars fade in on scroll, fade out after inactivity.

**Step 4.1: Register opacity keys in display list**

In `paint_scrollbars()`, when `ScrollbarVisibility::WhenScrolling`, create
`OpacityKey::unique()` and store in `ScrollbarDrawInfo.opacity_key`. Register
in gpu_cache (`scrollbar_v_opacity_keys`, `scrollbar_h_opacity_keys`).

The compositor2.rs scrollbar rendering already has opacity support:
`ScrollbarDrawInfo.opacity_key` exists. When set, compositor should wrap
scrollbar in a `PushOpacityBinding`.

**Step 4.2: Implement GpuStateManager::tick() for opacity**

Replace the placeholder `tick()` with real fade logic:
```rust
fn tick(&mut self, now: Instant) -> GpuTickResult {
    for (dom_id, cache) in &mut self.caches {
        for ((d, node_id), opacity_key) in &cache.scrollbar_v_opacity_keys {
            let last_activity = /* look up from scroll_manager */;
            let elapsed = now - last_activity;
            let target = if elapsed < self.fade_delay {
                1.0  // fully visible during activity
            } else if elapsed < self.fade_delay + self.fade_duration {
                // lerp from 1.0 to 0.0
                1.0 - (elapsed - self.fade_delay) / self.fade_duration
            } else {
                0.0  // fully hidden
            };
            cache.scrollbar_v_opacity_values.insert((*d, *node_id), target);
        }
    }
}
```

**Step 4.3: Call tick() in render loop**

Ensure `gpu_state_manager.tick(now)` is called once per frame from the
platform render function. If the result indicates `needs_repaint`, request
another frame (to keep the fade animation running).

**Step 4.4: Always emit scrollbar in display list**

Even when "hidden" (opacity=0), the scrollbar must exist in the display list
with the opacity key so that GPU animation can show it when needed. The
compositor wraps it in `PushOpacityBinding`, so when opacity=0 the GPU
skips rendering it (no CPU cost).

**Estimated files to change:**
- `layout/src/managers/gpu_state.rs` (implement tick())
- `layout/src/solver3/display_list.rs` (always emit, register opacity keys)
- `dll/src/desktop/compositor2.rs` (wrap in PushOpacityBinding)
- `dll/src/desktop/wr_translate2.rs` (call tick, forward needs_repaint)
- Platform render loop (request_redraw if fading)

### Phase 5: Trackpad Rubber-Banding (Fixes S4)

**Goal:** Overscroll elastic bounce on trackpad gestures.

**Step 5.1: Add TrackpadPhase to ScrollInputSource**

```rust
pub enum ScrollInputSource {
    TrackpadContinuous,
    TrackpadEnd,  // NEW: gesture ended, fingers lifted
    WheelDiscrete,
    Programmatic,
}
```

**Step 5.2: Pass NSEvent phase from macOS event handler**

In `macos/events.rs`, check `[nsevent phase]` and `[nsevent momentumPhase]`.
When `phase == NSEventPhaseEnded` or `momentumPhase == NSEventPhaseEnded`,
send `ScrollInputSource::TrackpadEnd`.

**Step 5.3: Handle TrackpadEnd in scroll timer**

```rust
ScrollInputSource::TrackpadEnd => {
    // If currently overscrolled, switch to spring-back
    let pos = physics.pending_positions.remove(&key)
        .or_else(|| timer_info.get_scroll_node_info(...).map(...));
    let overshoot = calculate_overshoot(pos.y, 0.0, info.max_scroll_y);
    if overshoot.abs() > 0.01 {
        let node_phys = physics.node_velocities.entry(key).or_default();
        node_phys.velocity = LogicalPosition::zero();
        node_phys.is_rubber_banding = true;
    }
}
```

**Step 5.4: Allow overscroll in TrackpadContinuous**

Replace the hard clamp:
```rust
position.x.max(0.0).min(info.max_scroll_x)
```
with `rubber_band_clamp()` (the function already exists):
```rust
rubber_band_clamp(position.y, 0.0, info.max_scroll_y,
    max_overscroll_distance, overscroll_elasticity)
```

**Estimated files to change:**
- `layout/src/managers/scroll_state.rs` (add TrackpadEnd variant)
- `dll/src/desktop/shell2/macos/events.rs` (detect phase, emit TrackpadEnd)
- `layout/src/scroll_timer.rs` (handle TrackpadEnd, use rubber_band_clamp for trackpad)

### Phase 6: Mouse Capture for Drag (Fixes S11, Partial S10)

**Goal:** Scrollbar drag continues working when cursor exits window.

**Step 6.1: Mouse capture on drag start**

When `handle_scrollbar_click()` starts a drag (sets `ScrollbarDragState`),
call platform-specific mouse capture:
- macOS: Cocoa delivers events during tracking loops automatically if
  the NSView accepts first responder. Verify `[NSWindow setAcceptsMouseMovedEvents:YES]`
  and mouseDown/mouseDragged continuation is working.
- Windows: `SetCapture(hwnd)`
- Linux: XGrabPointer / XInput2 grab

**Step 6.2: Release capture on drag end**

When mouse button is released and `ScrollbarDragState` is cleared, release
capture.

**Estimated files to change:**
- `dll/src/desktop/shell2/macos/events.rs` (verify mouse capture)
- `dll/src/desktop/shell2/win32/events.rs` (SetCapture/ReleaseCapture)
- `dll/src/desktop/shell2/x11/events.rs` (XGrabPointer)

### Phase 7: IFrame Scroll Integration (Fixes I1, I2)

**Goal:** IFrame nodes show scrollbars and scroll smoothly.

**Step 7.1: Use virtual_scroll_size in paint_scrollbars()**

In `paint_scrollbars()`, after getting `content_size` from
`node.get_content_size()`, check if the node is an IFrame with
`virtual_scroll_size`. If so, use that instead:
```rust
let content_size = node.virtual_scroll_size
    .unwrap_or_else(|| node.get_content_size());
```
This requires the virtual_scroll_size to be accessible from the positioned
tree node.

**Step 7.2: Per-pixel scroll for IFrame**

The "blocky" scroll issue (I2) happens because the IFrame re-invocation
threshold is set too high (one full row). Lower the threshold or make
it configurable. The IFrame should be re-invoked when the scroll offset
changes by any amount, not just full rows.

**Estimated files to change:**
- `layout/src/solver3/display_list.rs` (use virtual_scroll_size)
- `layout/src/window.rs` (lower IFrame reinvoke threshold)

### Phase 8: Test Case Fix (Fixes S9)

Change scrolling.c container style from:
```c
"display: flex; flex-direction: column; flex-grow: 1; overflow: auto;
 background: #ffff00; border: 3px solid #00ff00; margin: 8px; height: 400px;"
```
to:
```c
"display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1;
 overflow: auto; background: #ffff00; border: 3px solid #00ff00;
 margin: 8px; min-height: 0;"
```
And add `flex-shrink: 0;` to the footer style. This ensures the container
shrinks to accommodate the footer within the flex column.

### Implementation Priority

| Priority | Phase | Bugs Fixed | Effort | Dependencies |
|----------|-------|------------|--------|-------------|
| 1        | Phase 1 (Unified Geometry) | S2, S3, S8 | Medium | None |
| 2        | Phase 2 (GPU Transform) | S1 | Medium | Phase 1 |
| 3        | Phase 8 (Test Fix) | S9 | Trivial | None |
| 4        | Phase 3 (Styling) | S5, S6 | Low | None |
| 5        | Phase 4 (Opacity Fade) | S7 | Medium | Phase 2 |
| 6        | Phase 5 (Rubber Band) | S4 | Medium | None |
| 7        | Phase 6 (Mouse Capture) | S11, partial S10 | Low-Medium | Platform-specific |
| 8        | Phase 7 (IFrame) | I1, I2 | Medium | Phase 1 |

### Critical Invariant Checklist

Once all phases are complete, the following invariants MUST hold:

1. **Single geometry function**: `compute_scrollbar_geometry()` is called by
   exactly 3 callers (paint, gpu_update, hit_test) with the same inputs
   → same outputs.

2. **No hardcoded dimensions**: All scrollbar dimensions come from CSS style
   resolution with OS-specific defaults. Zero use of `16.0` or `8.0` magic
   numbers outside the OS default configuration.

3. **GPU transform always in sync**: After every `scroll_all_nodes()` call,
   `update_scrollbar_transforms()` runs before `synchronize_gpu_values()`.
   The transform formula matches the paint formula (guaranteed by shared
   `compute_scrollbar_geometry()`).

4. **content_size is unambiguous**: One method (`get_content_size()` or
   `virtual_scroll_size`) is used everywhere. The value stored in LayoutTree
   matches what `paint_scrollbars()` uses.

5. **Hit-test matches visual**: The thumb rect used for hit-testing
   (`push_hit_test` in compositor2.rs) is the SAME rect the user sees on
   screen (including GPU transform offset). Currently the hit-test rect is
   the at-rest position (top of track); it must be updated to account for
   the transform. Options: (a) make the hit-test area cover the entire track
   and use scroll_ratio to determine component, or (b) update hit-test
   geometry via `update_scrollbar_transforms`.

6. **Opacity keys registered before first frame**: The compositor must be able
   to resolve `PropertyBinding::Binding(key, initial_value)` for opacity on
   the very first frame render, not just after the first scroll event.
