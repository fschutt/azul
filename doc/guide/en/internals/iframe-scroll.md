---
slug: internals/iframe-scroll
title: IFrame Scroll and Display Lists
language: en
canonical_slug: internals/iframe-scroll
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Iframe nodes — independent scroll regions, display-list embedding, and coordinate translation across iframe boundaries.
prerequisites: []
tracked_files:
  - core/src/callbacks.rs
  - core/src/hit_test.rs
  - layout/src/managers/virtual_view.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:52Z
---

# IFrame Scroll and Display Lists

> WIP: the design doc `scripts/IFRAME_SCROLL_DISPLAY_LIST_ARCHITECTURE.md` is from when "IFrame" was a separate display-list concept. The clean architecture it proposed has been implemented and renamed: `DisplayListItem::VirtualView` replaces the old `IFrame` item, `DisplayListItem::VirtualViewPlaceholder` replaces the post-hoc scroll-frame scan, and the `VirtualViewCallback` ABI subsumes the old `IFrameCallback`. This page describes the implementation as it stands; read the design doc for the historical motivation.

A VirtualView is a child DOM rendered inside a parent's display list. Its bounds, clip rect, and scroll offset are owned by the parent's layout pass; the child DOM is built (and rebuilt) by a `VirtualViewCallback` invoked when the parent layout decides one is due. Scrolling is **not** routed through WebRender's APZ for VirtualViews — `ScrollManager` is the single source of truth, and the callback receives the live `scroll_offset` so it can decide what slice to render.

## The display-list contract

`generate_display_list` ([`layout/src/solver3/display_list.rs`](../../../../layout/src/solver3/display_list.rs)) emits two items for each `NodeType::VirtualView`:

```rust,ignore
PushClip { bounds: container, border_radius: container_radius }
  VirtualViewPlaceholder { node_id, bounds, clip_rect }
PopClip
```

Crucially **no `PushScrollFrame` / `PopScrollFrame`** wraps the placeholder. Regular `overflow: scroll` containers do get a scroll frame pair so WebRender's spatial tree can transform their contents; VirtualView nodes do not, because the scroll offset reaches the rendered slice via the callback's return value, not via WebRender transforms.

`VirtualViewPlaceholder` ([`layout/src/solver3/display_list.rs:715`](../../../../layout/src/solver3/display_list.rs)):

```rust,ignore
VirtualViewPlaceholder {
    node_id: NodeId,             // node in the parent DOM
    bounds: WindowLogicalRect,   // window-absolute container rect
    clip_rect: WindowLogicalRect,
}
```

`WindowLogicalRect` is the newtype that flags the rect as window-absolute (see the coordinate-space invariant in [Hit Testing and Scrolling](hit-testing.md)). The compositor converts to scroll-frame-relative inside `resolve_rect()`.

## Replacement in `LayoutWindow`

After the parent's layout pass completes, `LayoutWindow::scan_for_virtual_views` ([`layout/src/window.rs:1318`](../../../../layout/src/window.rs)) walks the layout tree, picks every node with `NodeType::VirtualView`, and reads the calculated position + used size as `(NodeId, LogicalRect)`. For each entry the runtime invokes the VirtualView callback (see [VirtualView Lazy Loading](virtual-view.md)) and, if the callback produced a child DOM, swaps the placeholder for the real item:

```rust,ignore
for item in display_list.items.iter_mut() {
    if let DisplayListItem::VirtualViewPlaceholder { node_id: nid, bounds, clip_rect, .. } = item {
        if *nid == target_node_id {
            *item = DisplayListItem::VirtualView {
                child_dom_id,
                bounds: *bounds,
                clip_rect: *clip_rect,
            };
            break;
        }
    }
}
```

[`layout/src/window.rs:1262`](../../../../layout/src/window.rs). The placeholder is emitted at the right structural position (between `PushClip` and `PopClip`, outside any scroll frame), so the replacement is positional, not scan-based — no depth-counted `PushScrollFrame`/`PopScrollFrame` walk is needed. The fallback path that appends at the end of the display list ([`window.rs:1284`](../../../../layout/src/window.rs)) only fires when the placeholder cannot be found, which should never happen and exists as a defence against future divergence.

## Why the scroll frame is gone — the historical bug

In the earlier IFrame architecture (described in `scripts/IFRAME_SCROLL_DISPLAY_LIST_ARCHITECTURE.md`), the parent's display list looked like this:

```text
PushClip { bounds }
  PushScrollFrame { scroll_id, content_size }
    IFrame { child_dom_id, bounds, clip_rect }   ← inside the scroll spatial node
  PopScrollFrame
PopClip
```

WebRender applied the scroll offset to every child of the scroll spatial node — including the `IFrame { bounds, clip_rect }` itself. Scrolling the IFrame slid the IFrame viewport off-screen instead of changing which content the callback rendered. The fix was to move the IFrame item *after* `PopScrollFrame` so it stayed stationary in window coordinates while the scroll frame still contributed a hit-test target. That left an empty scroll frame whose only job was carrying the wheel/trackpad hit area.

The current architecture eliminates the empty scroll frame entirely. The container's hit-test area still routes wheel events to `ScrollManager::record_scroll_from_hit_test`, but it does so through a `HitTestArea` tagged with the scroll-container namespace (`TAG_TYPE_SCROLL_CONTAINER` = 0x0500 — see [Hit Testing and Scrolling](hit-testing.md)). No WebRender spatial node is reserved for content that will never be rendered into.

## Compositor handling

`translate_displaylist_to_wr` in [`dll/src/desktop/compositor2.rs`](../../../../dll/src/desktop/compositor2.rs) handles `DisplayListItem::VirtualView` by recursing into the child DOM's display list under a fresh WebRender pipeline:

```rust,ignore
DisplayListItem::VirtualView { child_dom_id, bounds, clip_rect } => {
    let child_pipeline_id = wr_translate_pipeline_id(
        AzulPipelineId(child_dom_id.inner as u32, document_id)
    );
    // ... recurse into child layout result ...
    let space_and_clip = SpaceAndClipInfo {
        spatial_id: current_spatial!(),     // parent's spatial node
        clip_chain_id: current_clip!(),     // parent's PushClip
    };
    let wr_bounds    = scale_bounds_to_layout_rect(bounds.inner(),    dpi_scale);
    let wr_clip_rect = scale_bounds_to_layout_rect(clip_rect.inner(), dpi_scale);
    builder.push_iframe(wr_bounds, wr_clip_rect, &space_and_clip,
                        child_pipeline_id, false);
}
```

[`dll/src/desktop/compositor2.rs:1386–1467`](../../../../dll/src/desktop/compositor2.rs). The function still calls WebRender's `push_iframe` because that is the renderer-level mechanism for compositing a child pipeline into a parent — the name is WebRender's, not azul's. The child pipeline is registered via `nested_pipelines.push((child_pipeline_id, child_dl))` so the desktop shell can submit it alongside the parent transaction.

The `space_and_clip` uses the **parent's** spatial node and clip chain, not a fresh scroll frame — the IFrame viewport renders inside the parent's coordinate context, and the child's own scroll frames (if any) are pushed inside the recursed display list.

## Pipeline identity

`PipelineId` for a VirtualView is deterministic, not counter-allocated:

```rust,ignore
pub fn get_or_create_pipeline_id(&mut self, dom_id: DomId, node_id: NodeId) -> PipelineId {
    *self.pipeline_ids.entry((dom_id, node_id))
        .or_insert_with(|| PipelineId(dom_id.inner as u32, node_id.index() as u32))
}
```

[`layout/src/managers/virtual_view.rs:126`](../../../../layout/src/managers/virtual_view.rs). As long as the parent `(DomId, NodeId)` is stable across rebuilds, the same VirtualView gets the same pipeline. The compositor's recursion uses the same encoding (`AzulPipelineId(child_dom_id.inner as u32, document_id)`) so the child pipeline's identity is consistent end-to-end.

## Scroll routing

A wheel/trackpad event over a VirtualView container hits the scroll-container hit area on the parent's layout. The shell decodes the tag, asks `ScrollManager::record_scroll_from_hit_test` to enqueue the input, the scroll-physics timer drains the queue and writes the new offset, and on the next frame:

1. `LayoutWindow::layout_and_generate_display_list` runs again for the parent.
2. The scan picks up the VirtualView nodes.
3. `VirtualViewManager::check_reinvoke` reads the new offset from `ScrollManager::get_current_offset(parent_dom, node_id)`.
4. If the offset is within `EDGE_THRESHOLD` of the bottom/right of the rendered slice, the manager returns `Some(EdgeScrolled(Bottom|Right))` and the callback is invoked with the new `scroll_offset`.
5. The callback returns a fresh slice; the placeholder replacement runs again with the new `child_dom_id`.

`AnimatedScrollState.virtual_scroll_size` ([`layout/src/managers/scroll_state.rs:330`](../../../../layout/src/managers/scroll_state.rs)) is set when the callback's return value reports a `virtual_scroll_size` larger than the rendered `scroll_size`. The scroll manager's clamping switches from `content_rect.size` to `virtual_scroll_size`, so the user can scroll past the rendered slice into the unrendered virtual area, generating the edge events that trigger callback re-invocation.

## Scroll authority — the single source of truth

For non-VirtualView scroll containers, scroll routing is dual: WebRender's APZ owns the spatial transform and `ScrollManager` mirrors the offset for hit-testing and callbacks. For VirtualViews, only `ScrollManager` matters — there is no APZ scroll node to keep in sync, and the callback is the only consumer of the offset. This is why the platform-side audit in `scripts/CALLBACK_INVOCATION_UNIFICATION.md` §9 calls `ScrollManager.scroll_to()` after a scroll-physics timer fires: if `process_callback_result_v2` is skipped (a bug on Windows/X11/Wayland documented there), the offset never reaches `ScrollManager` and the next `check_reinvoke` reads stale data.

The proposed `Update::ScrollOnly` variant in §9.6 of that doc is one way to keep scroll-only updates from forcing a full DOM rebuild every frame. As of this writing the scroll-physics timer still returns `Update::RefreshDom` and the platforms divide on whether they handle it correctly.

## Hit-testing inside a VirtualView

A hit on a node inside a VirtualView's child DOM produces a `HitTestItem.is_virtual_view_hit = Some((parent_dom_id, virtual_view_origin))` ([`core/src/hit_test.rs:203`](../../../../core/src/hit_test.rs)). The dispatcher uses `virtual_view_origin` to translate viewport coordinates into the child DOM's local frame before invoking callbacks. The origin is the placeholder's `bounds.origin` at the time the display list was built — i.e. window-absolute, before scroll frame transforms — so subtracting it from `point_in_viewport` gives the child-local coordinate.

## What's still rough

Items the design doc lists as future work and that are still open:

- The scroll-physics timer returns `Update::RefreshDom`, forcing a full layout pass after every wheel tick. A scroll-only update path that skips display-list rebuild and only re-runs `scroll_all_nodes` + `txn.generate_frame()` would eliminate the per-tick layout overhead.
- The `nodes_scrolled_in_callbacks` field in `CallCallbacksResult` is processed only on macOS today; Windows, X11, and Wayland platform handlers do not include it in their `needs_processing` check (see `scripts/CALLBACK_INVOCATION_UNIFICATION.md` §9.2). The fix is mechanical but cross-platform.
- `EdgeScrolled(Top)` and `EdgeScrolled(Left)` are computed in `VirtualViewState::check_reinvoke_condition` but are never returned ([`layout/src/managers/virtual_view.rs:341`](../../../../layout/src/managers/virtual_view.rs)). Top-anchored "load older messages" lists are not yet supported through this path.
- `EDGE_THRESHOLD` ([`layout/src/managers/virtual_view.rs:20`](../../../../layout/src/managers/virtual_view.rs)) is a hardcoded `200.0`; per-VirtualView tuning is not exposed.

## Where the pieces live

| Concern | File |
|---|---|
| Display-list items (`VirtualView`, `VirtualViewPlaceholder`, `WindowLogicalRect`) | [`layout/src/solver3/display_list.rs`](../../../../layout/src/solver3/display_list.rs) |
| Scan + placeholder replacement (`scan_for_virtual_views`, `invoke_virtual_view_callback_with_dom`) | [`layout/src/window.rs`](../../../../layout/src/window.rs) (~line 1244) |
| Manager (`VirtualViewManager`, `VirtualViewState`, `EdgeFlags`, `EDGE_THRESHOLD`) | [`layout/src/managers/virtual_view.rs`](../../../../layout/src/managers/virtual_view.rs) |
| Callback ABI (`VirtualViewCallback`, `VirtualViewCallbackInfo`, `VirtualViewReturn`, `VirtualViewCallbackReason`) | [`core/src/callbacks.rs`](../../../../core/src/callbacks.rs) |
| Compositor recursion (`translate_displaylist_to_wr`, `push_iframe`) | [`dll/src/desktop/compositor2.rs`](../../../../dll/src/desktop/compositor2.rs) (~line 1386) |
| Scroll authority (`ScrollManager`, `record_scroll_from_hit_test`, `set_virtual_scroll_size`) | [`layout/src/managers/scroll_state.rs`](../../../../layout/src/managers/scroll_state.rs) |
| Hit-test routing (`HitTestItem.is_virtual_view_hit`, `PipelineId`) | [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs) |

For how the callback is invoked and the re-invocation latches, see [VirtualView Lazy Loading](virtual-view.md). For how the hit-test result distinguishes VirtualView children from regular DOM nodes, see [Hit Testing and Scrolling](hit-testing.md). For where scroll inputs feed into the event pipeline, see [Event System Internals](event-system.md).
