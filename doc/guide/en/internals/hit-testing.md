---
slug: internals/hit-testing
title: Hit Testing
language: en
canonical_slug: internals/hit-testing
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Hit-test tag generation and cursor-to-node routing
prerequisites: []
tracked_files:
  - core/src/hit_test.rs
  - core/src/hit_test_tag.rs
  - core/src/drag.rs
  - core/src/selection.rs
  - layout/src/hit_test.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:52Z
---

# Hit Testing and Scrolling

> WIP: the tag namespaces and hit-test result types are stable. The type-safe `HitTestTag` wrapper described in [`core/src/hit_test_tag.rs`](../../../../core/src/hit_test_tag.rs) is not yet wired through the rest of the codebase. Call sites still manipulate raw `(u64, u16)` pairs in `dll/src/desktop/wr_translate2.rs` and `layout/src/solver3/display_list.rs`. The design doc `scripts/HIT_TEST_TAG_ANALYSIS.md` lays out the migration plan. Treat the enum here as the authoritative encoding reference.

Hit-testing maps a viewport pixel to four parallel result sets at once: the DOM nodes underneath the cursor, the scroll containers underneath them, the cursor icon to display, and the text-selection regions for selection drags. WebRender returns hit results in front-to-back z-order; azul disambiguates result kinds by tagging each hittable display item with a 16-bit namespace marker. The tag scheme lives in [`core/src/hit_test_tag.rs`](../../../../core/src/hit_test_tag.rs); the result types in [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs); the cursor-resolution algorithm in [`layout/src/hit_test.rs`](../../../../layout/src/hit_test.rs).

## WebRender ItemTag namespace layout

Display items are pushed with `ItemTag = (u64, u16)`. The upper byte of `tag.1` selects the namespace:

- **DOM node (`0x0100`).** `TAG_TYPE_DOM_NODE` covers regular interactive DOM nodes for callbacks, focus, and hover. The `tag.0` payload is `TagId.inner`, a sequential counter from styling.
- **Scrollbar (`0x0200`).** `TAG_TYPE_SCROLLBAR` covers scrollbar track and thumb hit regions. The `tag.0` payload is `(DomId << 32) | NodeId` and the component lives in `tag.1 & 0xFF`.
- **Selection (`0x0300`).** `TAG_TYPE_SELECTION` covers text selection hit regions per text run. The `tag.0` payload is `(DomId << 48) | (NodeId << 16) | text_run_index`.
- **Cursor (`0x0400`).** `TAG_TYPE_CURSOR` covers CSS `cursor` regions on text runs. The `tag.0` payload is `(DomId << 32) | NodeId` and the cursor icon lives in `tag.1 & 0xFF`.
- **Scroll container (`0x0500`).** `TAG_TYPE_SCROLL_CONTAINER` is the wheel/trackpad target for scroll containers. The `tag.0` payload is the same as the scrollbar namespace.
- **Legacy (`0`).** Treated as `DomNode` for backwards compatibility, with a `TagId` payload.

Each namespace is its own depth-sorted bucket, so a selection hit and a DOM-node hit at the same point produce two separate results. The dispatcher doesn't have to invent priority rules between scroll wheels and click handlers.

## Why namespaces matter: the legacy bug

Before namespace markers, every push went out as `(tag_value, 0u16)`. WebRender returns small, sequential `tag_value`s for normal DOM nodes (1, 2, 3, ...). The compositor's scrollbar decoder in `dll/src/desktop/wr_translate2.rs` read `(tag_value >> 62) & 0x3` to recover the scrollbar component. For a tag value of 673 that expression is `0`, the same encoding the decoder uses for `VerticalTrack`. Every normal click was misclassified as a scrollbar hit and the button callback never ran. The history is in `scripts/HIT_TEST_TAG_ANALYSIS.md`; the namespace constants in `core/src/hit_test_tag.rs` are the fix.

## HitTestTag

```rust,ignore
pub enum HitTestTag {
    DomNode  { tag_id: TagId },
    Scrollbar { dom_id: DomId, node_id: NodeId, component: ScrollbarComponent },
    Cursor    { dom_id: DomId, node_id: NodeId, cursor_type: CursorType },
    Selection { dom_id: DomId, container_node_id: NodeId, text_run_index: u16 },
}

impl HitTestTag {
    pub fn to_item_tag(&self) -> (u64, u16);
    pub fn from_item_tag(tag: (u64, u16)) -> Option<Self>;
}
```

[`core/src/hit_test_tag.rs::HitTestTag`](../../../../core/src/hit_test_tag.rs). Round-trip encode/decode is covered by tests in the same file. `from_item_tag` accepts `tag.1 == 0` as a legacy DOM-node tag so older display lists still hit-test correctly.

`ScrollbarComponent` ([`core/src/hit_test_tag.rs`](../../../../core/src/hit_test_tag.rs)) packs `VerticalTrack=0`, `VerticalThumb=1`, `HorizontalTrack=2`, `HorizontalThumb=3` into the lower byte of `tag.1`. `CursorType` (same file) packs the 21 cursor variants into the same byte. The `Selection` variant is unusual: it sacrifices `DomId` precision (16 bits, asserted at encode time) and uses the middle 32 bits for `NodeId` so the `text_run_index` fits in the lower 16 bits of `tag.0`.

The intent is for display-list construction to use `HitTestTag::to_item_tag()` and the dispatch path to use `HitTestTag::from_item_tag()`. In practice the codebase still uses raw bit operations. Treat `HitTestTag` as the authoritative reference for the encoding, not as a wrapper to plug into.

## HitTestItem and HitTest

```rust,ignore
pub struct HitTestItem {
    pub point_in_viewport: LogicalPosition,
    pub point_relative_to_item: LogicalPosition,
    pub is_focusable: bool,
    pub is_virtual_view_hit: Option<(DomId, LogicalPosition)>,
    pub hit_depth: u32,        // 0 = frontmost
}

pub struct HitTest {
    pub regular_hit_test_nodes:    BTreeMap<NodeId, HitTestItem>,
    pub scroll_hit_test_nodes:     BTreeMap<NodeId, ScrollHitTestItem>,
    pub scrollbar_hit_test_nodes:  BTreeMap<ScrollbarHitId, ScrollbarHitTestItem>,
    pub cursor_hit_test_nodes:     BTreeMap<NodeId, CursorHitTestItem>,
}
```

`HitTest` and `HitTestItem` are defined at [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs). Each map corresponds to one of the tag namespaces. `hit_depth` is preserved across all four so frontmost-wins logic can reason about the relationship between a button (DomNode tag) and the text inside it (Cursor tag).

`is_virtual_view_hit` is set when the node belongs to a nested DOM produced by a `VirtualViewCallback`. The tuple is `(parent_dom_id, virtual_view_origin)` so dispatchers can translate viewport coordinates into the virtual-view local frame. See [VirtualView Lazy Loading](virtual-view.md) for how nested DOMs are registered.

`ScrollHitTestItem` ([`core/src/hit_test.rs`](../../../../core/src/hit_test.rs)) carries an `OverflowingScrollNode`:

```rust,ignore
pub struct OverflowingScrollNode {
    pub parent_rect: LogicalRect,
    pub child_rect: LogicalRect,
    pub virtual_child_rect: LogicalRect,
    pub parent_external_scroll_id: ExternalScrollId,
    pub parent_dom_hash: DomNodeHash,
    pub scroll_tag_id: ScrollTagId,
}
```

[`core/src/hit_test.rs::OverflowingScrollNode`](../../../../core/src/hit_test.rs). `ExternalScrollId(u64, PipelineId)` is the renderer-side identity of a scroll frame. `parent_dom_hash` survives DOM rebuilds so scroll positions can be migrated by content rather than by `NodeId`.

`ScrollbarHitId` in the same file keys scrollbar-component results by `(DomId, NodeId)` plus the orientation/component encoded into the variant (`VerticalTrack`, `VerticalThumb`, `HorizontalTrack`, `HorizontalThumb`).

## FullHitTest

```rust,ignore
pub struct FullHitTest {
    pub hovered_nodes: BTreeMap<DomId, HitTest>,
    pub focused_node: OptionDomNodeId,
}
```

[`core/src/hit_test.rs::FullHitTest`](../../../../core/src/hit_test.rs). The shell calls `HoverManager::push_hit_test(InputPointId::Mouse, hit_test)` after every cursor move. Downstream consumers (`dispatch_events_propagated`, `CursorTypeHitTest::new`, the input interpreter) read from this snapshot.

`is_empty()` reports `hovered_nodes.is_empty()` only. A `FullHitTest` with no hovered nodes but a focused node still counts as empty. `focused_node` is the authoritative focus state for the hit-test snapshot, typically `FocusManager::focused_node` at the moment the cursor moved.

## Cursor resolution: CursorTypeHitTest

```rust,ignore
pub struct CursorTypeHitTest {
    pub cursor_node: Option<(DomId, NodeId)>,
    pub cursor_icon: MouseCursorType,
}

impl CursorTypeHitTest {
    pub fn new(hit_test: &FullHitTest, layout_window: &LayoutWindow) -> Self;
}
```

[`layout/src/hit_test.rs::CursorTypeHitTest::new`](../../../../layout/src/hit_test.rs). Two independent passes find the frontmost `cursor_node`:

1. Walk `cursor_hit_test_nodes`. These are tag-encoded cursor types from text runs (no CSS lookup). A non-`Default` cursor at a smaller `hit_depth` than the running best replaces it.
2. Walk `regular_hit_test_nodes`. Query the styled DOM's `CssPropertyCache::get_cursor` for each node. An explicit cursor property at a smaller depth replaces the running best.

The frontmost wins. `best_depth` is initialised to `u32::MAX` and replaced by any candidate whose `hit_depth` is strictly smaller. A `cursor: pointer` button on top of a `cursor: text` paragraph displays the pointer cursor. If neither pass finds a non-default cursor, `cursor_icon` stays `MouseCursorType::Default`.

The current logic intentionally inverts an earlier buggy iteration (documented in `scripts/CURSOR_HIT_TEST_ARCHITECTURE_REPORT.md`) where `best_depth` started at 0 and was compared with `>=`, picking the *backmost* node. A separate text-child detection hack in this same function tried to work around the inversion. The hack is gone, and the depth comparison is the only mechanism.

`translate_cursor_type` and `translate_cursor` ([`layout/src/hit_test.rs`](../../../../layout/src/hit_test.rs)) map the tag-encoded `CursorType` and the CSS `StyleCursor` enum to `MouseCursorType` for the platform.

## Scrollbar hit-testing

`ScrollbarHitTestItem` ([`core/src/hit_test.rs`](../../../../core/src/hit_test.rs)) records `point_in_viewport`, `point_relative_to_item`, and `orientation` for each scrollbar component hit. The interpreter uses the local position to decide:

- Click on track (`VerticalTrack` or `HorizontalTrack`): page-scroll one viewport in the direction of the click.
- Click on thumb (`VerticalThumb` or `HorizontalThumb`): begin a `DragContext::scrollbar_thumb(...)` session.
- Drag updates: `DragContext::calculate_scrollbar_scroll_offset()` ([`core/src/drag.rs::calculate_scrollbar_scroll_offset`](../../../../core/src/drag.rs)) converts the mouse delta to a scroll offset using `track_length_px`, `content_length_px`, and `viewport_length_px`.

The thumb-length formula (`viewport / content * track`) and the scrollable-track derivation (`track - thumb`) match the standard proportional scrollbar math. The interpreter passes the result back to `ScrollManager::set_scroll_position` (see [`layout/src/managers/scroll_state.rs`](../../../../layout/src/managers/scroll_state.rs)).

## ScrollState and ScrollStates

```rust,ignore
pub struct ScrollState  { pub scroll_position: LogicalPosition }
pub struct ScrollStates(pub OrderedMap<ExternalScrollId, ScrollState>);

impl ScrollState {
    pub fn add(&mut self, x: f32, y: f32, child_rect: &LogicalRect);
    pub fn set(&mut self, x: f32, y: f32, child_rect: &LogicalRect);
}
```

`ScrollState` and `ScrollStates` are defined at [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs). The `add` and `set` impls clamp to `0.0 .. child_rect.size.{width,height}`. This clamps to the full child size, not to `max(0, child_size - parent_size)`, so callers must pass the *overflow delta* as `child_rect`, not the unmodified child rectangle, or scroll positions can run past the end of the visible content. The live scroll math is in `ScrollManager::scroll_by` and `ScrollManager::set_scroll_position` in the layout crate. This `ScrollState` type is the renderer-facing representation kept in step via `ScrollStates`.

`ScrollManager` ([`layout/src/managers/scroll_state.rs`](../../../../layout/src/managers/scroll_state.rs)) owns the live state per `(DomId, NodeId)`, including the `AnimatedScrollState` (current offset, smooth-scroll animation, container/content rects, virtual-view sizes, overscroll behaviour). Hit-testing only consumes its `get_current_offset` snapshot.

## Drag operations driven by hit-testing

```rust,ignore
pub enum ActiveDragType {
    TextSelection(TextSelectionDrag),
    ScrollbarThumb(ScrollbarThumbDrag),
    Node(NodeDrag),
    WindowMove(WindowMoveDrag),
    WindowResize(WindowResizeDrag),
    FileDrop(FileDropDrag),
}

pub struct DragContext {
    pub drag_type: ActiveDragType,
    pub session_id: u64,        // links to GestureManager
    pub cancelled: bool,        // flipped on Escape
}
```

Defined in [`core/src/drag.rs`](../../../../core/src/drag.rs). The hit-test result determines which constructor the interpreter chooses:

- **Node drag.** A `regular_hit_test_nodes` hit on a draggable node plus mousedown picks `DragContext::node_drag`. `NodeDrag.drag_data: DragData` carries MIME-typed payloads (HTML5 `DataTransfer`).
- **Scrollbar thumb.** A `scrollbar_hit_test_nodes` thumb component picks `DragContext::scrollbar_thumb`.
- **Text selection.** A text-run hit in the selection namespace plus mousedown picks `DragContext::text_selection`. The anchor is stored as `TextCursor` from [`core/src/selection.rs`](../../../../core/src/selection.rs).
- **Window move.** A titlebar drag region plus mousedown picks `DragContext::window_move`. It uses initial window position to compute deltas.
- **File drop.** OS file-drag-over picks `DragContext::file_drop`. `FileDropDrag.files: StringVec` is populated by the platform shell.

`DragContext::update_position(p)` ([`core/src/drag.rs::update_position`](../../../../core/src/drag.rs)) rewrites the active variant's mouse position uniformly. `start_position()` and `current_position()` abstract over the per-variant field names. `as_*` and `is_*` accessors in the same file provide pattern-free read access.

After a DOM rebuild, `DragContext::remap_node_ids(dom_id, mapping)` ([`core/src/drag.rs::remap_node_ids`](../../../../core/src/drag.rs)) rewrites stored `NodeId`s using the lifecycle reconciliation map. If a critical node was unmounted the function returns `false` and the interpreter cancels the drag.

`DropEffect` (`None`/`Copy`/`Link`/`Move`) is the drop target's choice. `DragEffect` (the source's `effect_allowed`) is its strict superset (`CopyLink`, `CopyMove`, `LinkMove`, `All`, plus the `Uninitialized` sentinel). The drop only succeeds when the target's `DropEffect` is a member of the source's `DragEffect` set.

## Selection hit-testing

The `Selection` tag namespace exists so that text selection drags do not interfere with click handlers on the same node. Each text run pushes one `Selection { dom_id, container_node_id, text_run_index }` tag covering its rasterised glyph rect. On a hit, the interpreter:

1. Decodes the tag back via `HitTestTag::from_item_tag`.
2. Looks up the IFC root's `UnifiedLayout` in the layout result.
3. Uses `point_relative_to_item` to convert pixel coordinates into a `TextCursor { cluster_id, affinity }` ([`core/src/selection.rs`](../../../../core/src/selection.rs)).
4. On mousedown, builds a `SelectionAnchor` capturing the IFC node, cursor, character bounds, and mouse position.
5. On mousemove during a `TextSelection` drag, builds a `SelectionFocus` and recomputes `TextSelection.affected_nodes`.

`TextSelection.affected_nodes: BTreeMap<NodeId, SelectionRange>` keys per IFC root. This enables O(log N) lookup during render so each `<p>` only has to ask the selection for its own range. The selection can span multiple IFC roots, since anchor and focus carry their own `ifc_root_node_id`.

`MultiCursorState` is the Sublime-style multi-cursor variant used by `TextEditManager` for editable elements. It maintains the same sorted/non-overlapping invariant as `SelectionState` but with stable `SelectionId`s and a proper `merge_overlapping`. `SelectionState::add` only sorts and dedups exact duplicates and is treated as the FFI/C-API form. Internal Rust code uses `MultiCursorState`.

## Producing the hit test

The actual hit-test request goes through the WebRender API hook in the desktop compositor (`dll/src/desktop/wr_translate2.rs`). It pushes the cursor coordinates, receives the front-to-back result list, decodes each `(u64, u16)` tag, and bins the results into the four `HitTest` maps by namespace. The output is then wrapped in a `FullHitTest` together with the current focused node and handed to the shell.

For the CPU-only renderer path the same pipeline runs against the layout result directly (no WebRender involved); the bin discipline is identical because the tag namespaces are part of the display-list contract, not part of WebRender.

## Coordinate-space invariant

Everything in the display list is emitted in **window-absolute** coordinates by the layout solver. The compositor in `dll/src/desktop/compositor2.rs` is the only component that converts to scroll-frame-relative coordinates, via a `resolve_rect()` helper that combines DPI scaling and offset subtraction. To make this checkable at compile time, `DisplayListItem` variants now wrap their bounds in a `WindowLogicalRect` newtype. See `scripts/SCROLL_COORDINATE_ARCHITECTURE.md` for the history. Every new variant that forgot `apply_offset` produced a silent rendering bug inside scroll containers. When adding a new variant, accept `WindowLogicalRect` and read `.inner()` only inside the compositor's match arm.

## Where the pieces live

- **Tag encoding and decoding.** `HitTestTag`, `ScrollbarComponent`, `CursorType`, and namespace constants.
  - [`core/src/hit_test_tag.rs`](../../../../core/src/hit_test_tag.rs)
- **Result types.** `HitTest`, `HitTestItem`, `ScrollHitTestItem`, `ScrollbarHitTestItem`, `CursorHitTestItem`, `FullHitTest`.
  - [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs)
- **Scroll identity.** `ExternalScrollId`, `OverflowingScrollNode`, `ScrollState`, `ScrollStates`, `PipelineId`.
  - [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs)
- **Cursor resolution.** `CursorTypeHitTest::new` and CSS-to-platform cursor maps.
  - [`layout/src/hit_test.rs`](../../../../layout/src/hit_test.rs)
- **Drag dispatch.** `DragContext`, `ActiveDragType`, scrollbar math, and NodeId remapping.
  - [`core/src/drag.rs`](../../../../core/src/drag.rs)
- **Selection model.** `TextCursor`, `SelectionRange`, `TextSelection`, `MultiCursorState`.
  - [`core/src/selection.rs`](../../../../core/src/selection.rs)
- **Live scroll state.** `ScrollManager`, `AnimatedScrollState`, the scroll input queue.
  - [`layout/src/managers/scroll_state.rs`](../../../../layout/src/managers/scroll_state.rs)

For how hit-test results enter the dispatch loop, see [Event System Internals](event-system.md). For how nested DOMs from `VirtualViewCallback`s register their hit areas, see [VirtualView Lazy Loading](virtual-view.md). For the IFrame-specific scroll routing problem (and why IFrames intentionally sit outside their `PushScrollFrame`/`PopScrollFrame` pair), see [IFrame Scroll and Display Lists](iframe-scroll.md).

## Coming Up Next

- [Event System Internals](event-system.md): Hit-testing, callback invocation, the Update protocol
- [IFrame Scroll](iframe-scroll.md): Iframe scroll regions and coordinate translation
- [Layout Solver (Flex/Grid)](layout-solver.md): Architecture of `solver3/` and how the engines share state
