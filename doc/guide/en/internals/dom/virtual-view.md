---
slug: internals/dom/virtual-view
title: VirtualView Lazy Loading
language: en
canonical_slug: internals/dom/virtual-view
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: What the virtual view layer caches and how it survives layouts
prerequisites: []
tracked_files:
  - core/src/callbacks.rs
  - layout/src/managers/virtual_view.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:52Z
---

# VirtualView Lazy Loading

## Overview

A `VirtualViewCallback` returns a partial DOM that represents only the slice of content currently in (or near) the viewport, plus a virtual size that drives the scrollbar as if all content were rendered. *WIP — the manager and callback contract are stable; the cap on `EDGE_THRESHOLD`, the absence of left / top edge triggers, and the manual reset semantics are likely to change.*

The runtime calls the callback only when something happens that could require new content: the initial render, a parent DOM rebuild, the container expanding past the rendered slice, or the user scrolling within `EDGE_THRESHOLD` (200 px) of an unfetched edge. Coordination lives in `layout/src/managers/virtual_view.rs`. The callback ABI is in `core/src/callbacks.rs`.

This page documents the callback signature and the manager that decides when to fire it. For how the resulting child DOM is composited into the parent's display list and pipelined through WebRender, see [IFrame Scroll and Display Lists](iframe-scroll.md).

## The callback signature

```rust,ignore
pub type VirtualViewCallbackType =
    extern "C" fn(RefAny, VirtualViewCallbackInfo) -> VirtualViewReturn;

pub struct VirtualViewCallback {
    pub cb: VirtualViewCallbackType,
    pub ctx: OptionRefAny,  // foreign callable for FFI; None for native Rust
}
```

Native Rust callers construct the wrapper via `VirtualViewCallback::create(fn_ptr)`. FFI bindings store the user's foreign callable in `ctx` and a trampoline in `cb` that extracts both `RefAny`s and dispatches.

The two `RefAny`s a foreign callable receives are the user's data (held in the wrapping `RefAny` that bound this callback to the DOM) and the foreign-language callable itself (extracted from `ctx`). Native Rust closures live entirely in the `cb` function pointer.

## VirtualViewCallbackInfo

```rust,ignore
pub struct VirtualViewCallbackInfo {
    pub reason: VirtualViewCallbackReason,
    pub system_fonts: *const FcFontCache,
    pub image_cache:  *const ImageCache,
    pub window_theme: WindowTheme,
    pub bounds: HidpiAdjustedBounds,            // logical_size + DPI factor
    pub scroll_size:           LogicalSize,     // currently rendered content size
    pub scroll_offset:         LogicalPosition, // origin of rendered content in virtual space
    pub virtual_scroll_size:   LogicalSize,     // size the scrollbar pretends content has
    pub virtual_scroll_offset: LogicalPosition, // origin of virtual space (usually zero)
    callable_ptr: *const OptionRefAny,
    _abi_mut: *mut c_void,
}
```

The two raw pointers (`system_fonts`, `image_cache`) are accessed through internal helper methods that re-borrow them with the callback-info lifetime. The unsafe deref is centralised so user code doesn't see it.

`bounds.get_physical_size()` gives the size in physical pixels accounting for DPI. `get_image(image_id)` and `get_system_fonts()` are convenience accessors that walk `image_cache` and `system_fonts` respectively. `set_callable_ptr(&OptionRefAny)` and `get_ctx()` are the FFI hooks for binding the foreign callable through the info struct (parallel to the `LayoutCallbackInfo` mechanism). Native Rust code doesn't call them; the trampoline in foreign bindings does.

## Why the callback is invoked: VirtualViewCallbackReason

```rust,ignore
pub enum VirtualViewCallbackReason {
    InitialRender,
    DomRecreated,
    BoundsExpanded,
    EdgeScrolled(EdgeType),       // EdgeType: Top | Bottom | Left | Right
    ScrollBeyondContent,
}
```

Today `VirtualViewManager::check_reinvoke` produces `InitialRender`, `BoundsExpanded`, and `EdgeScrolled(Bottom|Right)`. `DomRecreated` is set when the parent DOM rebuilds and the runtime has to re-prime the manager via `reset_all_invocation_flags`. The resulting `check_reinvoke` call returns `InitialRender` rather than `DomRecreated`, since the per-state flag was cleared. `ScrollBeyondContent` is reserved for the future case of programmatic scroll past `virtual_scroll_size`. The predicate isn't implemented yet.

The reason lets the callback short-circuit: an `InitialRender` may build a different fallback DOM than an `EdgeScrolled(Bottom)` extension fetch, and a `DomRecreated` callback usually re-emits the existing slice without re-querying the data source.

## Return value: VirtualViewReturn

```rust,ignore
pub struct VirtualViewReturn {
    pub dom: OptionDom,                   // None = keep current DOM, only update bounds
    pub scroll_size:           LogicalSize,
    pub scroll_offset:         LogicalPosition,
    pub virtual_scroll_size:   LogicalSize,
    pub virtual_scroll_offset: LogicalPosition,
}

impl VirtualViewReturn {
    pub fn with_dom(dom: Dom, scroll_size, scroll_offset,
                    virtual_scroll_size, virtual_scroll_offset) -> Self;
    pub fn keep_current(scroll_size, scroll_offset,
                        virtual_scroll_size, virtual_scroll_offset) -> Self;
}
```

The two size pairs encode the virtualisation contract:

- `scroll_size` and `scroll_offset` are the size and position of the actually-rendered DOM slice. For a table showing rows 10 to 30 at 30 px per row, `scroll_size = (full_width, 600)` and `scroll_offset = (0, 300)`.
- `virtual_scroll_size` and `virtual_scroll_offset` are the size the scrollbar represents. For a 1000-row table that's `(full_width, 30000)` regardless of which slice is rendered. `virtual_scroll_offset` is normally `(0, 0)` unless the virtual space starts at a non-zero origin.

Returning `dom: OptionDom::None` (`keep_current(...)`) is the optimisation path. The rendered slice is still adequate for the current scroll position, and only the scroll bounds need updating. The runtime won't rebuild the nested DOM.

## VirtualViewManager

```rust,ignore
pub struct VirtualViewManager {
    states:        BTreeMap<(DomId, NodeId), VirtualViewState>,
    pipeline_ids:  BTreeMap<(DomId, NodeId), PipelineId>,
    next_dom_id:   usize,        // starts at 1; 0 is the root DOM
}
```

One state per `(parent DomId, NodeId of the virtualised element)`. The manager owns:

- `get_or_create_nested_dom_id(parent_dom, node_id) -> DomId` allocates the child DOM identifier the callback's returned `Dom` will live under.
- `get_or_create_pipeline_id(parent_dom, node_id) -> PipelineId` assigns the WebRender pipeline so the nested DOM has its own scroll frame. It's encoded as `PipelineId(dom_id.inner as u32, node_id.index() as u32)`. The encoding is deterministic, not counter-allocated, so the same VirtualView gets the same pipeline across rebuilds.
- `get_scroll_size`, `get_virtual_scroll_size`, and `was_virtual_view_invoked` are accessors used during display-list generation and hit-testing.
- `update_virtual_view_info(parent_dom, node_id, scroll_size, virtual_scroll_size)` is called after the callback returns to record the reported sizes. If the new `scroll_size` exceeds the old one, the `invoked_for_current_expansion` latch is reset so the next layout pass can request more.
- `mark_invoked(parent_dom, node_id, reason)` flips the per-reason latches (`virtual_view_was_invoked`, `invoked_for_current_expansion`, `invoked_for_current_edge`, `last_edge_triggered`).

The `next_dom_id` counter starts at 1 because `DomId { inner: 0 }` is reserved for the root window DOM.

## Re-invocation logic: check_reinvoke

```rust,ignore
pub fn check_reinvoke(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    scroll_manager: &ScrollManager,
    layout_bounds: LogicalRect,
) -> Option<VirtualViewCallbackReason>;
```

The decision tree is:

1. **Never invoked?** Return `Some(InitialRender)`. This also runs after `reset_all_invocation_flags`, so a parent DOM rebuild yields `InitialRender` rather than `DomRecreated` today.
2. **Container grew?** If `layout_bounds.size.{width,height}` is larger than the previously recorded `last_bounds`, clear the `invoked_for_current_expansion` latch.
3. **Update `last_bounds`** to the current frame's value.
4. **Compute** `scroll_offset = scroll_manager.get_current_offset(dom_id, node_id)`.
5. Delegate to `VirtualViewState::check_reinvoke_condition`.

The state-level predicate returns:

- `Some(BoundsExpanded)` when the container is wider or taller than the recorded `scroll_size` and the expansion latch hasn't yet fired.
- `Some(EdgeScrolled(Bottom))` when bottom-edge scrolling is possible (`scroll_size.height > container_size.height`), the cursor is within `EDGE_THRESHOLD` (200 px) of the bottom, the edge latch is clear, and `last_edge_triggered.bottom` is `false`.
- `Some(EdgeScrolled(Right))` similarly for the right edge.
- `None` otherwise.

Top and left edge variants are computed (`current_edges.top`, `current_edges.left`) but aren't currently emitted. Only bottom and right trigger callbacks. This is intentional for the common infinite-scroll case but means top-anchored "load more" isn't yet supported through this path.

## Re-invocation lifecycle

```text
+----------------------------------------------------------------------+
|  Frame N: layout pass for a node with a VirtualViewCallback          |
|                                                                      |
|   1. VirtualViewManager.check_reinvoke(parent, node, scroll, bounds) |
|        |-- never invoked            -> Some(InitialRender)           |
|        |-- bounds grew              -> Some(BoundsExpanded)          |
|        |-- scroll near edge         -> Some(EdgeScrolled(Bottom|Right))
|        `-- otherwise                -> None  -> reuse last DOM       |
|                                                                      |
|   2. If Some(reason):                                                |
|        a. Build VirtualViewCallbackInfo with reason + bounds + scroll|
|        b. Invoke callback -> VirtualViewReturn { dom, sizes... }     |
|        c. update_virtual_view_info(scroll_size, virtual_scroll_size) |
|        d. mark_invoked(reason)                                       |
|        e. If dom is Some, replace VirtualViewPlaceholder with        |
|           VirtualView { child_dom_id, bounds, clip_rect } in the     |
|           parent's display list                                      |
|                                                                      |
|   3. ScrollManager records virtual_scroll_size / virtual_scroll_offset
|      so scrollbar geometry reflects the virtual content rectangle.   |
+----------------------------------------------------------------------+
```

The placeholder / replacement mechanism is documented in [IFrame Scroll and Display Lists](iframe-scroll.md).

## Latch reset rules

The latches in `VirtualViewState` prevent the same condition from firing every frame. The reset rules are:

- `virtual_view_was_invoked` resets on `force_reinvoke()` or `reset_all_invocation_flags()`.
- `invoked_for_current_expansion` resets when container bounds grew (in `check_reinvoke`) or `scroll_size` grew (in `update_virtual_view_info`).
- `invoked_for_current_edge` resets on `force_reinvoke()` or `reset_all_invocation_flags()`.
- `last_edge_triggered.{top,bottom,left,right}` resets on `force_reinvoke()` or `reset_all_invocation_flags()`.

`reset_all_invocation_flags` is called from `layout_and_generate_display_list` after the layout cache is cleared. The child DOMs no longer exist in `layout_results`, so the callback must run again from scratch. `force_reinvoke(dom_id, node_id)` is the per-VirtualView equivalent used by `trigger_virtual_view_rerender`.

`last_edge_triggered` is *not* cleared when the user scrolls away from an edge. That's currently a deliberate choice. Once you've requested more content for the bottom, you don't want to re-request it every time the user scrolls back to the bottom. The trade-off is that callers must use `force_reinvoke` to allow the same edge to fire again.

## Coordination with ScrollManager

The nested DOM has its own scroll frame in `ScrollManager`. When a callback returns `virtual_scroll_size` larger than `scroll_size`, the scroll manager's `AnimatedScrollState.virtual_scroll_size` is set to that value via `set_virtual_scroll_size`. Clamping logic for the nested DOM then uses it instead of `content_rect.size`, so the scrollbar can scroll past the actually-rendered content into the virtual area. When the user scrolls past `scroll_offset + scroll_size`, the next `check_reinvoke` call reads the new `scroll_offset` and detects the edge condition.

## Hit-testing nested DOMs

A hit on a node inside a nested VirtualView DOM produces a `HitTestItem.is_virtual_view_hit = Some((parent_dom_id, virtual_view_origin))`. The dispatcher uses `virtual_view_origin` to translate viewport coordinates into the nested DOM's local frame before invoking callbacks. See [Event System Internals](../events.md) for the full hit-test data flow.

## Debug introspection

```rust,ignore
pub struct VirtualViewDebugInfo {
    pub parent_dom_id: usize,
    pub parent_node_id: usize,
    pub nested_dom_id: usize,
    pub scroll_size_width: Option<f32>,
    pub scroll_size_height: Option<f32>,
    pub virtual_scroll_size_width: Option<f32>,
    pub virtual_scroll_size_height: Option<f32>,
    pub was_invoked: bool,
    pub last_bounds_x: f32,
    pub last_bounds_y: f32,
    pub last_bounds_width: f32,
    pub last_bounds_height: f32,
}

pub fn get_all_virtual_view_infos(&self) -> Vec<VirtualViewDebugInfo>;
pub fn debug_counts(&self) -> (usize, usize);  // (states.len(), pipeline_ids.len())
```

`debug_counts` is read by `AZ_E2E_TEST` to assert that the manager's internal maps don't grow unboundedly across resize / scroll loops.

## Constants and tuning

`EDGE_THRESHOLD` is `200.0` pixels. The threshold is hardcoded; per-VirtualView configuration would require a field on `VirtualViewManager` or a parameter to `check_reinvoke`. Mobile-density displays that want a smaller pre-fetch window currently have no knob.

## See also

- [IFrame Scroll and Display Lists](iframe-scroll.md) — how the placeholder is replaced and how the nested DOM is composited into the parent's pipeline.
- [Event System Internals](../events.md) — how the nested DOM enters the event pipeline and how the hit-test result distinguishes virtual-view children from regular nodes.
- [DOM Internals](../dom.md) — the `NodeType::VirtualView` variant that drives the placeholder emission.

## Coming Up Next

- [DOM Internals](../dom.md) — How the public `Dom` type is built and stored
- [IFrame Scroll](iframe-scroll.md) — Iframe scroll regions and coordinate translation
- [Rendering Pipeline](../rendering.md) — From `StyledDom` to pixels
