# IFrame Scroll & Display List Architecture Analysis

## 1. What the Patch Fixed

### The Bug

When an IFrame node had `overflow: scroll` / `overflow: auto`, the user could not scroll
the IFrame content. Scrolling caused the IFrame viewport itself to move off-screen instead
of updating the content offset passed to the IFrame callback.

### Root Cause

The display list for an IFrame's parent DOM was structured like this **before** the patch:

```
PushClip { bounds: [IFrame container] }           ← (1) clips to container
  PushScrollFrame { scroll_id: 42, ... }           ← (2) starts scroll spatial node
    IFrame { child_dom_id: 1, bounds, clip_rect }  ← (3) IFrame item INSIDE scroll frame
  PopScrollFrame                                   ← (4) ends scroll spatial node
PopClip                                            ← (5) ends clip
```

The `IFrame` display list item was inserted at position (3), *inside* the
`PushScrollFrame`/`PopScrollFrame` pair.  When WebRender processed a scroll event,
it applied the scroll offset to **all children** of the scroll spatial node — including
the IFrame's `bounds` and `clip_rect`.  This shifted the IFrame viewport itself, making
it disappear off the visible area.

### What the Patch Changed

The patch in `window.rs` moves the `IFrame` item insertion to **after** `PopScrollFrame`:

```
PushClip { bounds: [IFrame container] }           ← (1) clips to container
  PushScrollFrame { scroll_id: 42, ... }           ← (2) starts scroll spatial node
  PopScrollFrame                                   ← (3) ends scroll spatial node
  IFrame { child_dom_id: 1, bounds, clip_rect }    ← (4) IFrame item OUTSIDE scroll frame
PopClip                                            ← (5) ends clip
```

This keeps the IFrame viewport **stationary** in window coordinates (it's still clipped by
the parent `PushClip` but not moved by the scroll frame).  The actual content offset is
communicated to the IFrame callback via `scroll_offset` in `IFrameCallbackInfo`, where the
callback can decide which rows/columns to render.

The implementation correctly handles nested scroll frames by tracking `PushScrollFrame` /
`PopScrollFrame` depth to find the matching `PopScrollFrame`:

```rust
let mut depth = 1usize;
for j in (push_idx + 1)..display_list.items.len() {
    match &display_list.items[j] {
        PushScrollFrame { .. } => depth += 1,
        PopScrollFrame => {
            depth -= 1;
            if depth == 0 { pop_idx = Some(j); break; }
        }
        _ => {}
    }
}
let insert_at = pop_idx.map(|j| j + 1).unwrap_or(push_idx + 1);
```

## 2. Why the Current Architecture Is Not Clean

The fix is correct but highlights deeper structural problems:

### Problem 1: Post-Hoc Display List Mutation

`generate_display_list()` in `solver3/display_list.rs` emits a complete display list for
the parent DOM **without IFrame items**.  Then `layout_and_generate_display_list()` in
`window.rs` scans the finished display list, finds `PushScrollFrame` by `scroll_id`,
mutates `content_size`, and inserts the `IFrame` item at an inferred position.

This is fragile because:
- The position is inferred by scanning for matching `PushScrollFrame` / `PopScrollFrame` pairs
  rather than being structurally defined
- If the display list generation order changes (e.g. scroll frames become nested differently,
  or an optimization reorders items), the scan silently breaks
- The `content_size` mutation is a side-channel update to a value that `generate_display_list`
  already set once, just to a placeholder value

### Problem 2: Scroll Frame Doesn't Render Anything

For IFrame nodes, the `PushScrollFrame` / `PopScrollFrame` pair in the parent DOM's display
list is semantically incorrect. Its purpose is to tell WebRender to create a **scroll spatial
node** with the IFrame's virtual content size.  But:

1. **The scroll frame is empty** — there are no drawing primitives inside it (the IFrame
   item is now outside it after the fix)
2. **Azul manages scrolling** — the `ScrollManager` tracks offsets and passes them to the
   IFrame callback via `IFrameCallbackInfo.scroll_offset`. WebRender's APZ (Async Pan/Zoom)
   is not the scrolling authority for IFrame content.
3. **The content_size is absurd** — an infinite scroll list might declare 120 million pixels
   of virtual scroll height, causing WebRender to allocate spatial tracking for a content
   area it never renders into.

The scroll frame's only useful purpose is providing a **hit-test target** so that scroll
wheel/trackpad events on the IFrame container are routed to the `ScrollManager`.

### Problem 3: Two Scroll Tracking Systems

Scrolling for IFrame containers is tracked in two places:
1. **WebRender's APZ** — via the `PushScrollFrame` / `define_scroll_frame()` in compositor2.rs
2. **Azul's `ScrollManager`** — via `register_or_update_scroll_node()` and `get_current_offset()`

The `IFrameCallbackInfo.scroll_offset` comes from `ScrollManager`, but WebRender also
applies its own scroll offset to anything inside the scroll spatial node.  The fix works
by moving the IFrame item *outside* the scroll spatial node, effectively disabling
WebRender's scroll transform for the IFrame viewport while keeping the hit-test alive.

This dual-tracking is confusing and creates unnecessary overhead.

## 3. Proposed Clean Architecture

### Display List Generation

For IFrame nodes, `generate_display_list()` should emit a dedicated structure instead of
abusing scroll frames:

```
PushClip { bounds: [IFrame container] }
  HitTestArea { bounds, tag: iframe_scroll_tag }     ← for scroll wheel events
  IFramePlaceholder { node_id, bounds, clip_rect }    ← replaced by window.rs
PopClip
```

**No `PushScrollFrame` / `PopScrollFrame`** should be emitted for the IFrame node's overflow.
The IFrame placeholder is a sentinel that `window.rs` replaces with the real `IFrame { child_dom_id, ... }` item after invoking the callback.

This eliminates:
- The need to scan for matching `PushScrollFrame` by `scroll_id`
- The `content_size` mutation
- The depth-tracking `PopScrollFrame` search
- WebRender creating a scroll spatial node for content it never renders

### window.rs Integration

```rust
// After invoking IFrame callbacks:
for (node_id, bounds) in iframes {
    if let Some(child_dom_id) = self.invoke_iframe_callback_with_dom(...) {
        // Simply replace the placeholder
        for item in &mut display_list.items {
            if let DisplayListItem::IFramePlaceholder { node_id: nid, .. } = item {
                if *nid == node_id {
                    *item = DisplayListItem::IFrame { child_dom_id, bounds, clip_rect: bounds };
                    break;
                }
            }
        }
    }
}
```

### Scroll Hit-Test Routing

The `HitTestArea` with a special tag type (e.g. `TAG_TYPE_IFRAME_SCROLL = 0x0600`) would
be handled by the event system's hit-test dispatch.  When a scroll event hits this tag,
the `ScrollManager` is updated directly — no WebRender scroll frame involved.

This is how it effectively works *already* after the fix (the scroll frame is empty and
the `ScrollManager` is the true scroll authority), but the clean version makes it explicit
and removes the vestigial WebRender scroll frame.

### compositor2.rs Changes

The compositor would handle `IFramePlaceholder` as a no-op (it should have been replaced
already) and `IFrame` items use the **parent's** spatial/clip context directly since there's
no scroll frame to enter/exit:

```rust
DisplayListItem::IFrame { child_dom_id, bounds, clip_rect } => {
    let space_and_clip = SpaceAndClipInfo {
        spatial_id: current_spatial!(),       // parent's spatial node
        clip_chain_id: current_clip!(),       // parent's PushClip
    };
    builder.push_iframe(wr_bounds, wr_clip_rect, &space_and_clip, child_pipeline_id, false);
}
```

This is simpler, more correct, and eliminates the empty scroll frame overhead.

### Scroll Manager as Single Source of Truth

With no WebRender scroll frame for IFrames, `ScrollManager` becomes the unambiguous
scroll authority.  This is already true in practice — `invoke_iframe_callback_impl()`
reads from `scroll_manager.get_current_offset()` to populate `IFrameCallbackInfo.scroll_offset`.

The flow becomes:

```
scroll wheel event on IFrame container
  → hit-test identifies iframe_scroll_tag
  → ScrollManager.handle_scroll_event(dom_id, node_id, delta)
  → ScrollManager updates current_offset
  → IFrameManager.check_reinvoke() detects edge scroll
  → IFrame callback re-invoked with new scroll_offset
  → child DOM re-laid out, display list updated
```

## 4. Migration Path

1. **Add `IFramePlaceholder` variant** to `DisplayListItem`
2. **Modify `push_node_clips()`** in `display_list.rs` to detect IFrame nodes
   (via a flag or node type check) and skip `PushScrollFrame` for them, emitting
   `IFramePlaceholder` + `HitTestArea` instead
3. **Simplify `window.rs`** IFrame insertion to a placeholder replacement (no more
   scan-for-scroll-frame logic)
4. **Remove `content_size` mutation** — scroll size is only tracked in `ScrollManager`
   and `IFrameManager`
5. **Update compositor2.rs** to handle `IFramePlaceholder` as a no-op
6. **Add `TAG_TYPE_IFRAME_SCROLL`** hit-test tag type to event dispatch

The refactor is isolable: it changes the IFrame code path only and doesn't affect
non-IFrame scroll frames which continue to work through WebRender's APZ as before.

## 5. Summary

| Aspect | Current (after fix) | Proposed clean |
|--------|-------------------|----------------|
| IFrame position in DL | After `PopScrollFrame` (post-hoc insert) | Replaces `IFramePlaceholder` (in-place) |
| Scroll frame for IFrame | Empty `PushScrollFrame`/`PopScrollFrame` pair | None — just `HitTestArea` |
| Content size tracking | Mutated on `PushScrollFrame.content_size` | Only in `ScrollManager` + `IFrameManager` |
| Scroll authority | Dual (WebRender APZ + `ScrollManager`) | Single (`ScrollManager`) |
| Display list mutation | Scan by `scroll_id`, depth-track `Pop` | Direct replacement by `node_id` |
| WebRender overhead | Scroll spatial node with 120M px content | None |
