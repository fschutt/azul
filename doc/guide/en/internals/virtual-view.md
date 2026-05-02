---
slug: internals/virtual-view
title: VirtualView Lazy Loading
language: en
canonical_slug: internals/virtual-view
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: The virtual view layer — what it caches, how it survives across layouts, and how it talks to the windowing back-end.
prerequisites: []
tracked_files:
  - core/src/callbacks.rs
  - layout/src/managers/virtual_view.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:52Z
---

# VirtualView Lazy Loading

> WIP: the manager and callback contract are stable; the cap on `EDGE_THRESHOLD`, the absence of left/top edge triggers in `check_reinvoke_condition`, and the manual reset semantics are likely to change.

A `VirtualViewCallback` returns a partial DOM that represents only the slice of content currently in (or near) the viewport, plus a virtual size that drives the scrollbar as if all content were rendered. The runtime calls the callback only when something happens that could require new content: initial render, parent DOM rebuild, container expansion past the rendered slice, or scrolling within `EDGE_THRESHOLD` (200 px) of an unfetched edge. Coordination lives in [`layout/src/managers/virtual_view.rs`](../../../../layout/src/managers/virtual_view.rs); the callback ABI is defined in [`core/src/callbacks.rs`](../../../../core/src/callbacks.rs).

## The callback signature

```rust,ignore
pub type VirtualViewCallbackType =
    extern "C" fn(RefAny, VirtualViewCallbackInfo) -> VirtualViewReturn;

pub struct VirtualViewCallback {
    pub cb: VirtualViewCallbackType,
    pub ctx: OptionRefAny,  // foreign callable for FFI; None for native Rust
}
```

`VirtualViewCallbackType` at [`core/src/callbacks.rs:154`](../../../../core/src/callbacks.rs); `VirtualViewCallback` struct at [`callbacks.rs:159`](../../../../core/src/callbacks.rs). Native Rust callers construct the wrapper via `VirtualViewCallback::create(fn_ptr)`; FFI bindings store the user's foreign callable in `ctx` and a trampoline in `cb` that extracts both `RefAny`s and dispatches.

The two `RefAny`s a foreign callable receives are: the user's data (held in the wrapping `RefAny` that bound this callback to the DOM) and the foreign-language callable itself (extracted from `ctx`). Native Rust closures live entirely in the `cb` function pointer.

## `VirtualViewCallbackInfo`

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

[`core/src/callbacks.rs:206`](../../../../core/src/callbacks.rs). The two raw pointers (`system_fonts`, `image_cache`) are accessed through internal helper methods that re-borrow them with the callback-info lifetime; the unsafe deref is centralised so user code does not see it.

`bounds.get_physical_size()` ([`core/src/callbacks.rs:475`](../../../../core/src/callbacks.rs)) gives the size in physical pixels accounting for DPI. `get_image(image_id)` and `get_system_fonts()` are convenience accessors that walk `image_cache` and `system_fonts` respectively.

`set_callable_ptr(&OptionRefAny)` and `get_ctx()` are the FFI hooks for binding the foreign callable through the info struct (parallel to the `LayoutCallbackInfo` mechanism). Native Rust code does not call them; the trampoline in foreign bindings does.

## Why the callback is invoked: `VirtualViewCallbackReason`

```rust,ignore
pub enum VirtualViewCallbackReason {
    InitialRender,
    DomRecreated,
    BoundsExpanded,
    EdgeScrolled(EdgeType),       // EdgeType: Top | Bottom | Left | Right
    ScrollBeyondContent,
}
```

[`core/src/callbacks.rs:181`](../../../../core/src/callbacks.rs). Today `VirtualViewManager::check_reinvoke` produces `InitialRender`, `BoundsExpanded`, and `EdgeScrolled(Bottom|Right)`. `DomRecreated` is set when the parent DOM rebuilds and the runtime has to re-prime the manager via `reset_all_invocation_flags`; the resulting `check_reinvoke` call returns `InitialRender` rather than `DomRecreated`, since the per-state flag was cleared. `ScrollBeyondContent` is reserved for the future case of programmatic scroll past `virtual_scroll_size`; the predicate is not implemented yet.

The reason lets the callback short-circuit: an `InitialRender` may build a different fallback DOM than an `EdgeScrolled(Bottom)` extension fetch, and a `DomRecreated` callback usually re-emits the existing slice without re-querying the data source.

## Return value: `VirtualViewReturn`

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

[`core/src/callbacks.rs:280`](../../../../core/src/callbacks.rs). The two size pairs encode the virtualisation contract:

- `scroll_size` / `scroll_offset` — the size and position of the actually-rendered DOM slice. For a table showing rows 10–30 at 30 px per row, `scroll_size = (full_width, 600)` and `scroll_offset = (0, 300)`.
- `virtual_scroll_size` / `virtual_scroll_offset` — the size the scrollbar represents. For a 1000-row table that is `(full_width, 30000)` regardless of which slice is rendered. `virtual_scroll_offset` is normally `(0, 0)` unless the virtual space starts at a non-zero origin.

Returning `dom: OptionDom::None` (`keep_current(...)`) is the optimisation path: the rendered slice is still adequate for the current scroll position, only the scroll bounds need updating. The runtime will not rebuild the nested DOM.

## `VirtualViewManager`

```rust,ignore
pub struct VirtualViewManager {
    states:        BTreeMap<(DomId, NodeId), VirtualViewState>,
    pipeline_ids:  BTreeMap<(DomId, NodeId), PipelineId>,
    next_dom_id:   usize,        // starts at 1; 0 is the root DOM
}
```

[`layout/src/managers/virtual_view.rs:28`](../../../../layout/src/managers/virtual_view.rs). One state per `(parent DomId, NodeId of the virtualised element)`. The manager owns:

- `get_or_create_nested_dom_id(parent_dom, node_id) -> DomId` ([`virtual_view.rs:100`](../../../../layout/src/managers/virtual_view.rs)) — allocates the child DOM identifier the callback's returned `Dom` will live under.
- `get_or_create_pipeline_id(parent_dom, node_id) -> PipelineId` ([`virtual_view.rs:126`](../../../../layout/src/managers/virtual_view.rs)) — assigns the WebRender pipeline so the nested DOM has its own scroll frame. Encoded as `PipelineId(dom_id.inner as u32, node_id.index() as u32)` — deterministic, not counter-allocated, so the same VirtualView gets the same pipeline across rebuilds.
- `get_scroll_size`, `get_virtual_scroll_size`, `was_virtual_view_invoked` — accessors used during display-list generation and hit-testing.
- `update_virtual_view_info(parent_dom, node_id, scroll_size, virtual_scroll_size)` ([`virtual_view.rs:160`](../../../../layout/src/managers/virtual_view.rs)) — called after the callback returns to record the reported sizes; if the new `scroll_size` exceeds the old one, the `invoked_for_current_expansion` latch is reset so the next layout pass can request more.
- `mark_invoked(parent_dom, node_id, reason)` ([`virtual_view.rs:185`](../../../../layout/src/managers/virtual_view.rs)) — flips the per-reason latches (`virtual_view_was_invoked`, `invoked_for_current_expansion`, `invoked_for_current_edge`, `last_edge_triggered`).

The `next_dom_id` counter starts at 1 because `DomId { inner: 0 }` is reserved for the root window DOM.

## Re-invocation logic: `check_reinvoke`

```rust,ignore
pub fn check_reinvoke(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    scroll_manager: &ScrollManager,
    layout_bounds: LogicalRect,
) -> Option<VirtualViewCallbackReason>;
```

[`layout/src/managers/virtual_view.rs:244`](../../../../layout/src/managers/virtual_view.rs). The decision tree is:

1. **Never invoked?** Return `Some(InitialRender)`. (This also runs after `reset_all_invocation_flags`, so a parent DOM rebuild yields `InitialRender` rather than `DomRecreated` today.)
2. **Container grew?** If `layout_bounds.size.{width,height}` is larger than the previously recorded `last_bounds`, clear the `invoked_for_current_expansion` latch.
3. **Update `last_bounds`** to the current frame's value.
4. **Compute** `scroll_offset = scroll_manager.get_current_offset(dom_id, node_id)`.
5. Delegate to `VirtualViewState::check_reinvoke_condition`.

The state-level predicate ([`virtual_view.rs:341`](../../../../layout/src/managers/virtual_view.rs)) returns:

- `Some(BoundsExpanded)` when the container is wider or taller than the recorded `scroll_size` and the expansion latch has not yet fired.
- `Some(EdgeScrolled(Bottom))` when bottom-edge scrolling is possible (`scroll_size.height > container_size.height`), the cursor is within `EDGE_THRESHOLD` (200 px) of the bottom, the edge latch is clear, and `last_edge_triggered.bottom` is `false`.
- `Some(EdgeScrolled(Right))` similarly for the right edge.
- `None` otherwise.

Top and left edge variants are computed (`current_edges.top`, `current_edges.left`) but are not currently emitted — only bottom and right trigger callbacks. This is intentional for the common infinite-scroll case but means top-anchored "load more" is not yet supported through this path.

## Re-invocation lifecycle

```text
┌──────────────────────────────────────────────────────────────────────┐
│  Frame N: layout pass for a node with a VirtualViewCallback           │
│                                                                       │
│   1. VirtualViewManager.check_reinvoke(parent, node, scroll, bounds)  │
│        ├── never invoked            → Some(InitialRender)             │
│        ├── bounds grew              → Some(BoundsExpanded)            │
│        ├── scroll near edge         → Some(EdgeScrolled(Bottom|Right))│
│        └── otherwise                → None  → reuse last DOM          │
│                                                                       │
│   2. If Some(reason):                                                 │
│        a. Build VirtualViewCallbackInfo with reason + bounds + scroll │
│        b. Invoke callback → VirtualViewReturn { dom, sizes... }       │
│        c. update_virtual_view_info(scroll_size, virtual_scroll_size)  │
│        d. mark_invoked(reason)                                        │
│        e. If dom is Some, replace VirtualViewPlaceholder with         │
│           VirtualView { child_dom_id, bounds, clip_rect } in the      │
│           parent's display list (window.rs:1262)                      │
│                                                                       │
│   3. ScrollManager records virtual_scroll_size / virtual_scroll_offset│
│      so scrollbar geometry reflects the virtual content rectangle.    │
└──────────────────────────────────────────────────────────────────────┘
```

The placeholder/replacement mechanism is documented in [IFrame Scroll and Display Lists](iframe-scroll.md).

## Latch reset rules

The latches in `VirtualViewState` ([`virtual_view.rs:42`](../../../../layout/src/managers/virtual_view.rs)) prevent the same condition from firing every frame. The reset rules are:

| Latch | Reset when |
|---|---|
| `virtual_view_was_invoked` | `force_reinvoke()` or `reset_all_invocation_flags()` |
| `invoked_for_current_expansion` | container bounds grew (in `check_reinvoke`) or `scroll_size` grew (in `update_virtual_view_info`) |
| `invoked_for_current_edge` | `force_reinvoke()` or `reset_all_invocation_flags()` |
| `last_edge_triggered.{top,bottom,left,right}` | `force_reinvoke()` or `reset_all_invocation_flags()` |

`reset_all_invocation_flags` ([`virtual_view.rs:213`](../../../../layout/src/managers/virtual_view.rs)) is called from `layout_and_generate_display_list` after the layout cache is cleared — the child DOMs no longer exist in `layout_results`, so the callback must run again from scratch. `force_reinvoke(dom_id, node_id)` ([`virtual_view.rs:226`](../../../../layout/src/managers/virtual_view.rs)) is the per-VirtualView equivalent used by `trigger_virtual_view_rerender`.

`last_edge_triggered` is *not* cleared when the user scrolls away from an edge. That is currently a deliberate choice — once you have requested more content for the bottom, you do not want to re-request it every time the user scrolls back to the bottom. The trade-off is that callers must use `force_reinvoke` to allow the same edge to fire again.

## Coordination with `ScrollManager`

The nested DOM has its own scroll frame in `ScrollManager` ([`layout/src/managers/scroll_state.rs:297`](../../../../layout/src/managers/scroll_state.rs)). When a callback returns `virtual_scroll_size` larger than `scroll_size`, the scroll manager's `AnimatedScrollState.virtual_scroll_size` is set to that value via `set_virtual_scroll_size` — clamping logic for the nested DOM then uses it instead of `content_rect.size`, so the scrollbar can scroll past the actually-rendered content into the virtual area. When the user scrolls past `scroll_offset + scroll_size`, the next `check_reinvoke` call reads the new `scroll_offset` and detects the edge condition.

## Hit-testing nested DOMs

A hit on a node inside a nested VirtualView DOM produces a `HitTestItem.is_virtual_view_hit = Some((parent_dom_id, virtual_view_origin))` ([`core/src/hit_test.rs:203`](../../../../core/src/hit_test.rs)). The dispatcher uses `virtual_view_origin` to translate viewport coordinates into the nested DOM's local frame before invoking callbacks. See [Hit Testing and Scrolling](hit-testing.md) for the full hit-test data flow.

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

[`layout/src/managers/virtual_view.rs:282`](../../../../layout/src/managers/virtual_view.rs). `debug_counts` is read by `AZ_E2E_TEST` to assert that the manager's internal maps do not grow unboundedly across resize/scroll loops.

## Constants and tuning

| Constant | Value | Location |
|---|---|---|
| `EDGE_THRESHOLD` | `200.0` (px) | [`virtual_view.rs:20`](../../../../layout/src/managers/virtual_view.rs) |

The threshold is hardcoded; per-VirtualView configuration would require a field on `VirtualViewManager` or a parameter to `check_reinvoke`. Mobile-density displays that want a smaller pre-fetch window currently have no knob.

## Where the pieces live

| Concern | File |
|---|---|
| Callback ABI (`VirtualViewCallback`, `VirtualViewCallbackInfo`, `VirtualViewReturn`, `VirtualViewCallbackReason`, `EdgeType`, `HidpiAdjustedBounds`) | [`core/src/callbacks.rs`](../../../../core/src/callbacks.rs) |
| Manager (`VirtualViewManager`, `VirtualViewState`, `EdgeFlags`, `VirtualViewDebugInfo`, `EDGE_THRESHOLD`) | [`layout/src/managers/virtual_view.rs`](../../../../layout/src/managers/virtual_view.rs) |
| Scroll-state coordination (`AnimatedScrollState.virtual_scroll_size`, `ScrollManager`) | [`layout/src/managers/scroll_state.rs`](../../../../layout/src/managers/scroll_state.rs) |
| Hit-test result `is_virtual_view_hit` | [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs) |
| Display-list placeholder + replacement | [`layout/src/solver3/display_list.rs`](../../../../layout/src/solver3/display_list.rs), [`layout/src/window.rs`](../../../../layout/src/window.rs) |

For how the placeholder is replaced and how the nested DOM is composited into the parent's pipeline, see [IFrame Scroll and Display Lists](iframe-scroll.md). For how the nested DOM enters the event pipeline, see [Event System Internals](event-system.md). For how the hit-test result distinguishes virtual-view children from regular nodes, see [Hit Testing and Scrolling](hit-testing.md).
