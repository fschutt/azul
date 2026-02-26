# Scroll Container Coordinate Architecture

## Executive Summary

This session fixed 5 related bugs in scroll container rendering. The root cause
across all of them is **a single architectural weakness: the display list uses
absolute window-space coordinates everywhere, and the conversion to
scroll-frame-relative coordinates happens ad-hoc per display-list-item type in
the compositor**. This means every new `DisplayListItem` variant must remember
to call `apply_offset()`, and forgetting creates a silent, hard-to-detect bug.

**Can we permanently prevent these bugs?** Yes, via one of two approaches:
1. **Type-level enforcement** (compile-time guarantee, higher refactor cost)
2. **Centralized offset application** (runtime, lower cost, easier to audit)

Both are described below. Approach 2 is recommended as the pragmatic next step.

---

## Bugs Fixed This Session

| # | Bug | Root Cause | File |
|---|-----|-----------|------|
| 1 | Flex containers never got `scrollbar_info` | Taffy's flex algorithm bypasses Azul's scrollbar detection | `taffy_bridge.rs` |
| 2 | Text invisible inside scroll frames | `DisplayListItem::Text` missing `apply_offset()` | `compositor2.rs` |
| 3 | Image mispositioned in scroll frames | `DisplayListItem::Image` missing `apply_offset()` | `compositor2.rs` |
| 4 | IFrame scroll not detected by hit-test | Child pipeline occludes parent scroll nodes | `wr_translate2.rs` |
| 5 | IFrame display list ordering wrong | IFrame appended at end instead of placeholder position | `window.rs` |

Bug #1 is a layout/Taffy integration issue. Bugs #2-3 are the core
coordinate-space bug. Bugs #4-5 are IFrame-specific integration issues.

---

## The Coordinate Space Problem

### Current Architecture

```
Layout Engine → Display List → Compositor → WebRender
  (Window)       (Window)      (convert)     (ScrollFrame)
```

1. **Layout engine** computes all positions in **Window space** (absolute from
   window top-left).
2. **Display list** stores items with Window-space coordinates.
3. **Compositor** (`compositor2.rs`) iterates items and pushes them to
   WebRender. When inside a scroll frame, it must subtract the scroll frame's
   origin from each item's coordinates via `apply_offset()`.
4. **WebRender** expects scroll-frame-relative coordinates for items inside
   `define_scroll_frame`.

The `offset_stack` in compositor2.rs tracks nested scroll frame origins:
```rust
let mut offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];
// PushScrollFrame: offset_stack.push(frame_origin)
// PopScrollFrame:  offset_stack.pop()
// Items: rect = apply_offset(raw_rect, current_offset!())
```

### The Bug Pattern

Every `DisplayListItem::*` match arm must independently remember to:
1. Scale bounds from logical to physical pixels (`scale_bounds_to_layout_rect`)
2. Get the current offset (`current_offset!()`)
3. Apply the offset (`apply_offset(raw_rect, offset)`)

**There is no compiler enforcement.** If step 2-3 are forgotten, the item
renders at its absolute window position instead of relative to the scroll frame.
This is invisible when `offset_stack` is `[(0.0, 0.0)]` (no scroll frames), so
it only manifests when scroll containers are actually used.

### Audit Results (post-fix)

After this session's fixes, the status is:

| Item | `apply_offset`? | Notes |
|------|-----------------|-------|
| Rect | ✅ | Was already correct |
| SelectionRect | ✅ | Fixed this session |
| CursorRect | ✅ | Fixed this session |
| Border | ✅ | Fixed this session |
| ScrollBar | ✅ | Was already correct |
| ScrollBarStyled | ✅ | Was already correct |
| HitTestArea | ✅ | Fixed this session |
| Underline | ✅ | Fixed this session |
| Strikethrough | ✅ | Fixed this session |
| Overline | ✅ | Fixed this session |
| Text | ✅ | Fixed this session |
| Image | ✅ | Fixed this session |
| LinearGradient | ✅ | Was already correct |
| RadialGradient | ✅ | Was already correct |
| ConicGradient | ✅ | Was already correct |
| BoxShadow | ✅ | Was already correct |
| PushClip | ⚠️ | Defines clip in current spatial — may need offset |
| PushScrollFrame | ✅ | Correctly applies offset to frame rect |
| PushStackingContext | ⚠️ | Creates new context — offset semantics unclear |
| PushReferenceFrame | N/A | Uses zero origin by design |
| PushFilter | ⚠️ | Creates filter context |
| PushBackdropFilter | ⚠️ | Creates filter context |
| PushOpacity | ⚠️ | Creates opacity context |
| PushTextShadow | N/A | Shadow offset is text-relative |
| IFrame | N/A | Has own pipeline/coordinate system |

The ⚠️ items (Push* context items) need further investigation. They create new
spatial/clip contexts where the offset semantics are different — the offset
might need to be applied to the context's origin rather than to a rect.

---

## Prevention Strategies

### Approach 1: Type-Level Enforcement (Compile-Time)

Replace `LogicalRect` in `DisplayListItem` with a newtype that carries the
coordinate space:

```rust
/// A rectangle in absolute window coordinates (layout output).
pub struct WindowRect(pub LogicalRect);

/// A rectangle relative to the current scroll/reference frame.
pub struct FrameRelativeRect(pub LogicalRect);

enum DisplayListItem {
    Rect { bounds: WindowRect, ... },
    Text { clip_rect: WindowRect, ... },
    ...
}
```

The compositor would then have:
```rust
fn to_frame_relative(rect: &WindowRect, offset: (f32, f32)) -> FrameRelativeRect {
    FrameRelativeRect(apply_offset(rect.0, offset))
}
```

WebRender push functions would only accept `FrameRelativeRect`, making it a
**compile error** to pass unconverted coordinates.

**Pros:**
- Compile-time guarantee — impossible to forget the conversion
- Self-documenting — type names make coordinate spaces explicit
- Catches bugs when adding new DisplayListItem variants

**Cons:**
- Significant refactoring effort (~50 files touch LogicalRect)
- Some items legitimately use different spaces (IFrame, PushReferenceFrame)
- Glyph positions are `f32` pairs, not rects — needs separate handling

### Approach 2: Centralized Offset Application (Runtime)

Refactor the compositor loop to apply the offset **once, centrally**, rather
than per-item:

```rust
// Before the match: extract bounds from any item that has spatial bounds
let offset = current_offset!();

// Centralized helper that ALL items use
let resolve_rect = |bounds: &LogicalRect| -> LayoutRect {
    apply_offset(scale_bounds_to_layout_rect(bounds, dpi_scale), offset)
};

match item {
    DisplayListItem::Rect { bounds, .. } => {
        let rect = resolve_rect(bounds);
        ...
    }
    DisplayListItem::Text { clip_rect, .. } => {
        let rect = resolve_rect(clip_rect);
        ...
    }
}
```

Better yet, define a trait or method on `DisplayListItem`:
```rust
impl DisplayListItem {
    /// Returns the spatial bounds of this item (if any).
    /// Items that define coordinate contexts (PushClip, PushScrollFrame, etc.)
    /// return None — they handle coordinates differently.
    fn bounds(&self) -> Option<&LogicalRect> { ... }
}
```

Then the compositor does:
```rust
let resolved_bounds = item.bounds().map(|b| resolve_rect(b));
```

**Pros:**
- One place to get right, one place to audit
- Easy to add new item types — just implement `bounds()`
- Can add a `#[cfg(debug_assertions)]` assert that no raw bounds escape

**Cons:**
- Runtime, not compile-time enforcement
- Some items have multiple rects (e.g., ScrollBarStyled has track, thumb, buttons)
- Glyph positions still need separate handling

### Approach 3: Tests (Complementary)

Regardless of approach 1 or 2, add integration tests:

```rust
#[test]
fn test_scroll_frame_offset_all_items() {
    // Create a display list with every item type inside a scroll frame
    // Render to a mock WebRender builder that records coordinates
    // Assert all coordinates are frame-relative, not window-absolute
}
```

This is complementary — it catches regressions but doesn't prevent the initial
bug when adding a new variant.

---

## The Taffy Integration Problem (Bug #1)

Separate from coordinate spaces, the Taffy integration has a design tension:

**Taffy owns flex/grid layout** but **Azul owns scrollbar detection**. When
Taffy lays out a flex container, it doesn't know that `overflow: auto` should
constrain the container's size and trigger scrollbar creation. The fix
(`compute_child_layout()` in `taffy_bridge.rs`) runs Azul's scrollbar check
after Taffy returns, using the CSS-specified height vs. Taffy's content height.

This is inherently fragile because:
1. It depends on correctly extracting CSS height from the styled DOM
2. It only handles `px` heights (not `%`, `vh`, `calc()`)
3. It duplicates logic from `compute_scrollbar_info()` in `cache.rs`

**Recommendation:** The scrollbar check in `compute_child_layout()` should be
unified with `compute_scrollbar_info()`. Both should use the same function. The
Taffy overflow property (`Style::overflow = Scroll`) is already set correctly
and tells Taffy to use `min-size: 0` for the automatic minimum, but it doesn't
constrain the final size — that's by CSS spec design. The container size
constraint must come from `known_dimensions` passed by the parent flex
algorithm, which already works correctly for the PerformLayout pass.

---

## Recommendation

**Implemented: Approach 1 (Type-Level Enforcement).**

`WindowLogicalRect` newtype wraps `LogicalRect` in all `DisplayListItem`
variants and `ScrollbarDrawInfo` fields. The compositor accesses the inner
rect via `.inner()` or `.0`, making every coordinate-space conversion explicit.
A centralized `resolve_rect()` helper combines DPI scaling + offset subtraction
in a single call.

Files changed:
- `layout/src/solver3/display_list.rs` — `WindowLogicalRect` definition, enum, helpers
- `dll/src/desktop/compositor2.rs` — `resolve_rect()`, `.inner()` at all match arms
- `dll/src/desktop/shell2/common/debug_server.rs` — `.0.` field access
- `layout/src/cpurender.rs` — `.inner()` at function boundaries
- `layout/src/window.rs` — `.into()` wrapping

**Future work:**
- Migrate compositor match arms to use `resolve_rect()` instead of the two-step
  `scale_bounds_to_layout_rect` + `apply_offset` pattern.
- Add integration tests (Approach 3) for scroll frame offset correctness.
