---
slug: internals/hit-testing
title: Hit Testing and Scrolling
language: en
canonical_slug: internals/hit-testing
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - core/src/hit_test.rs
  - core/src/hit_test_tag.rs
  - core/src/drag.rs
  - core/src/selection.rs
  - layout/src/hit_test.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:31:55Z
---

# Hit Testing and Scrolling

> WIP: the tag namespaces and hit-test result types are stable; the type-safe `HitTestTag` wrapper described in [`core/src/hit_test_tag.rs`](../../../../core/src/hit_test_tag.rs) is not yet wired through the rest of the codebase — call sites still manipulate raw `(u64, u16)` pairs. Migration is planned but not in tree.

Hit-testing maps a viewport pixel to four parallel result sets at once: the DOM nodes underneath the cursor, the scroll containers underneath them, the cursor icon to display, and the text-selection regions for selection drags. WebRender returns hit results in front-to-back z-order; azul disambiguates result kinds by tagging each hittable display item with a 16-bit namespace marker. The tag scheme lives in [`core/src/hit_test_tag.rs`](../../../../core/src/hit_test_tag.rs); the result types in [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs); the cursor-resolution algorithm in [`layout/src/hit_test.rs`](../../../../layout/src/hit_test.rs).

## WebRender `ItemTag` namespace layout

Display items are pushed with `ItemTag = (u64, u16)`. The upper byte of `tag.1` selects the namespace:

| Marker (`tag.1 & 0xFF00`) | Const | Meaning | `tag.0` payload |
|---|---|---|---|
| `0x0100` | `TAG_TYPE_DOM_NODE` | Regular interactive DOM nodes (callbacks, focus, hover) | `TagId.inner` (sequential counter from styling) |
| `0x0200` | `TAG_TYPE_SCROLLBAR` | Scrollbar track/thumb hit regions | `(DomId << 32) \| NodeId` of the scroll container; component in `tag.1 & 0xFF` |
| `0x0300` | `TAG_TYPE_SELECTION` | Text selection hit regions per text run | `(DomId << 48) \| (NodeId << 16) \| text_run_index` |
| `0x0400` | `TAG_TYPE_CURSOR` | CSS `cursor` regions on text runs | `(DomId << 32) \| NodeId`; cursor icon in `tag.1 & 0xFF` |
| `0x0500` | `TAG_TYPE_SCROLL_CONTAINER` | Wheel/trackpad target for scroll containers | as above |
| `0` | (legacy) | Treated as `DomNode` for backwards compatibility | `TagId` |

Each namespace is its own depth-sorted bucket, so a selection hit and a DOM-node hit at the same point produce two separate results — the dispatcher does not have to invent priority rules between scroll wheels and click handlers.

## `HitTestTag`

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

[`core/src/hit_test_tag.rs:141`](../../../../core/src/hit_test_tag.rs). Round-trips encode/decode are covered by tests in the same file. `from_item_tag` accepts `tag.1 == 0` as a legacy DOM-node tag so older display lists still hit-test correctly.

`ScrollbarComponent` ([`hit_test_tag.rs:88`](../../../../core/src/hit_test_tag.rs)) packs `VerticalTrack=0`, `VerticalThumb=1`, `HorizontalTrack=2`, `HorizontalThumb=3` into the lower byte of `tag.1`. `CursorType` ([`hit_test_tag.rs:195`](../../../../core/src/hit_test_tag.rs)) packs the 21 cursor variants into the same byte. The `Selection` variant is unusual: it sacrifices `DomId` precision (16 bits, asserted at encode time) and `NodeId` precision (32 bits) to fit the `text_run_index` into `tag.0`.

The intent is for display-list construction to use `HitTestTag::to_item_tag()` and the dispatch path to use `HitTestTag::from_item_tag()`. In practice the codebase still uses raw bit operations; treat `HitTestTag` as the authoritative reference for the encoding, not as a wrapper to plug into.

## `HitTestItem` and `HitTest`

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

`HitTest` is defined at [`core/src/hit_test.rs:24`](../../../../core/src/hit_test.rs); `HitTestItem` at [`core/src/hit_test.rs:192`](../../../../core/src/hit_test.rs). Each map corresponds to one of the tag namespaces. `hit_depth` is preserved across all three so frontmost-wins logic can reason about the relationship between a button (DomNode tag) and the text inside it (Cursor tag).

`is_virtual_view_hit` is set when the node belongs to a nested DOM produced by a `VirtualViewCallback`; the tuple is `(parent_dom_id, virtual_view_origin)` so dispatchers can translate viewport coordinates into the virtual-view local frame. See [VirtualView Lazy Loading](virtual-view.md) for how nested DOMs are registered.

`ScrollHitTestItem` carries an `OverflowingScrollNode` ([`core/src/hit_test.rs:98`](../../../../core/src/hit_test.rs)):

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

`ExternalScrollId(u64, PipelineId)` is the renderer-side identity of a scroll frame; `parent_dom_hash` survives DOM rebuilds so scroll positions can be migrated by content rather than by `NodeId`.

`ScrollbarHitId` ([`core/src/hit_test.rs:63`](../../../../core/src/hit_test.rs)) keys scrollbar-component results by `(DomId, NodeId)` plus the orientation/component encoded into the variant (`VerticalTrack`, `VerticalThumb`, `HorizontalTrack`, `HorizontalThumb`).

## `FullHitTest`

```rust,ignore
pub struct FullHitTest {
    pub hovered_nodes: BTreeMap<DomId, HitTest>,
    pub focused_node: OptionDomNodeId,
}
```

[`core/src/hit_test.rs:313`](../../../../core/src/hit_test.rs). The shell calls `HoverManager::push_hit_test(InputPointId::Mouse, hit_test)` after every cursor move; downstream consumers (`dispatch_events_propagated`, `CursorTypeHitTest::new`, the input interpreter) read from this snapshot.

`is_empty()` reports `hovered_nodes.is_empty()` only; a `FullHitTest` with no hovered nodes but a focused node still counts as empty. `focused_node` is the authoritative focus state for the hit-test snapshot — typically `FocusManager::focused_node` at the moment the cursor moved.

## Cursor resolution: `CursorTypeHitTest`

```rust,ignore
pub struct CursorTypeHitTest {
    pub cursor_node: Option<(DomId, NodeId)>,
    pub cursor_icon: MouseCursorType,
}

impl CursorTypeHitTest {
    pub fn new(hit_test: &FullHitTest, layout_window: &LayoutWindow) -> Self;
}
```

[`layout/src/hit_test.rs:42`](../../../../layout/src/hit_test.rs). Two independent passes find the frontmost `cursor_node`:

1. Walk `cursor_hit_test_nodes` — these are tag-encoded cursor types from text runs (no CSS lookup). A non-`Default` cursor at a smaller `hit_depth` than the running best replaces it.
2. Walk `regular_hit_test_nodes` — query the styled DOM's `CssPropertyCache::get_cursor` for each node; an explicit cursor property at a smaller depth replaces the running best.

The frontmost wins: a `cursor: pointer` button on top of a `cursor: text` paragraph displays the pointer cursor. If neither pass finds a non-default cursor, `cursor_icon` stays `MouseCursorType::Default`.

`translate_cursor_type` ([`layout/src/hit_test.rs:130`](../../../../layout/src/hit_test.rs)) and `translate_cursor` ([`layout/src/hit_test.rs:159`](../../../../layout/src/hit_test.rs)) map the tag-encoded `CursorType` and the CSS `StyleCursor` enum to `MouseCursorType` for the platform. A second copy of `translate_cursor` exists in [`core/src/window.rs`](../../../../core/src/window.rs) and is currently unused — the layout copy is the live one.

## Scrollbar hit-testing

`ScrollbarHitTestItem` ([`core/src/hit_test.rs:73`](../../../../core/src/hit_test.rs)) records `point_in_viewport`, `point_relative_to_item`, and `orientation` for each scrollbar component hit. The interpreter uses the local position to decide:

- Click on track (component is `VerticalTrack` or `HorizontalTrack`): page scroll one viewport in the direction of the click.
- Click on thumb (`VerticalThumb` / `HorizontalThumb`): begin a `DragContext::scrollbar_thumb(...)` session.
- Drag updates: `DragContext::calculate_scrollbar_scroll_offset()` ([`core/src/drag.rs:629`](../../../../core/src/drag.rs)) converts the mouse delta to a scroll offset using `track_length_px`, `content_length_px`, and `viewport_length_px`.

The thumb-length formula (`viewport / content × track`) and the scrollable-track derivation (`track − thumb`) match the standard proportional scrollbar math; the interpreter passes the result back to `ScrollManager::set_scroll_offset` (see [`layout/src/managers/scroll_state.rs`](../../../../layout/src/managers/scroll_state.rs)).

## `ScrollState` and `ScrollStates`

```rust,ignore
pub struct ScrollState  { pub scroll_position: LogicalPosition }
pub struct ScrollStates(pub OrderedMap<ExternalScrollId, ScrollState>);

impl ScrollState {
    pub fn add(&mut self, x: f32, y: f32, child_rect: &LogicalRect);
    pub fn set(&mut self, x: f32, y: f32, child_rect: &LogicalRect);
}
```

`ScrollState` is defined at [`core/src/hit_test.rs:269`](../../../../core/src/hit_test.rs); `ScrollStates` at [`core/src/hit_test.rs:226`](../../../../core/src/hit_test.rs). The `add`/`set` impls ([`hit_test.rs:280`](../../../../core/src/hit_test.rs)) clamp to `0.0 .. child_rect.size.{width,height}`. Note: this clamps to the full child size, not to `max(0, child_size − parent_size)`, so callers must pass the *overflow delta* as `child_rect`, not the unmodified child rectangle, or scroll positions can run off the end of the visible content. The live scroll math is in `ScrollManager::scroll_by` and `ScrollManager::set_scroll_offset` in the layout crate; this `ScrollState` type is the renderer-facing representation kept in step via `ScrollStates`.

`ScrollManager` ([`layout/src/managers/scroll_state.rs:297`](../../../../layout/src/managers/scroll_state.rs)) owns the live state per `(DomId, NodeId)`, including the `AnimatedScrollState` (current offset, smooth-scroll animation, container/content rects, virtual-view sizes, overscroll behaviour). Hit-testing only consumes its `get_current_offset` snapshot.

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

| Hit | Constructor | Notes |
|---|---|---|
| `regular_hit_test_nodes` on a draggable node + mousedown | `DragContext::node_drag` | `NodeDrag.drag_data: DragData` carries MIME-typed payloads (HTML5 `DataTransfer`) |
| `scrollbar_hit_test_nodes` thumb component | `DragContext::scrollbar_thumb` | See above |
| Text-run hit (selection namespace) + mousedown | `DragContext::text_selection` | Anchor stored as `TextCursor` from [`core/src/selection.rs`](../../../../core/src/selection.rs) |
| Titlebar drag region + mousedown | `DragContext::window_move` | Uses initial window position to compute deltas |
| OS file-drag-over | `DragContext::file_drop` | `FileDropDrag.files: StringVec` populated by the platform shell |

`DragContext::update_position(p)` rewrites the active variant's mouse position uniformly. `start_position()` / `current_position()` abstract over the per-variant field names. `as_*` and `is_*` accessors at [`core/src/drag.rs:530`–630](../../../../core/src/drag.rs) provide pattern-free read access.

After a DOM rebuild, `DragContext::remap_node_ids(dom_id, mapping)` ([`core/src/drag.rs:697`](../../../../core/src/drag.rs)) rewrites stored `NodeId`s using the lifecycle reconciliation map; if a critical node was unmounted the function returns `false` and the interpreter cancels the drag.

`DropEffect` (`None`/`Copy`/`Link`/`Move`) is the drop target's choice; `DragEffect` (the source's `effect_allowed`) is its strict superset (`CopyLink`, `CopyMove`, `LinkMove`, `All`, plus the `Uninitialized` sentinel). The drop only succeeds when the target's `DropEffect` is a member of the source's `DragEffect` set.

## Selection hit-testing

The `Selection` tag namespace exists so that text selection drags do not interfere with click handlers on the same node. Each text run pushes one `Selection { dom_id, container_node_id, text_run_index }` tag covering its rasterised glyph rect. On a hit, the interpreter:

1. Decodes the tag back via `HitTestTag::from_item_tag`.
2. Looks up the IFC root's `UnifiedLayout` in the layout result.
3. Uses `point_relative_to_item` to convert pixel coordinates into a `TextCursor { cluster_id, affinity }` ([`core/src/selection.rs:93`](../../../../core/src/selection.rs)).
4. On mousedown, builds a `SelectionAnchor` ([`core/src/selection.rs:534`](../../../../core/src/selection.rs)) capturing the IFC node, cursor, character bounds, and mouse position.
5. On mousemove during a `TextSelection` drag, builds a `SelectionFocus` ([`core/src/selection.rs:554`](../../../../core/src/selection.rs)) and recomputes `TextSelection.affected_nodes`.

`TextSelection.affected_nodes: BTreeMap<NodeId, SelectionRange>` keys per IFC root; this enables O(log N) lookup during render so each `<p>` only has to ask the selection for its own range. The selection can span multiple IFC roots — anchor and focus carry their own `ifc_root_node_id`.

`MultiCursorState` ([`core/src/selection.rs:255`](../../../../core/src/selection.rs)) is the Sublime-style multi-cursor variant used by `TextEditManager` for editable elements; it maintains the same sorted/non-overlapping invariant as `SelectionState` ([`core/src/selection.rs:154`](../../../../core/src/selection.rs)) but with stable `SelectionId`s and proper `merge_overlapping`. `SelectionState` is the FFI-friendly form used by the C API.

## Producing the hit test

The actual hit-test request goes through the WebRender API hook in the desktop compositor (`dll/src/desktop/wr_translate2.rs`). It pushes the cursor coordinates, receives the front-to-back result list, decodes each `(u64, u16)` tag, and bins the results into the four `HitTest` maps by namespace. The output is then wrapped in a `FullHitTest` together with the current focused node and handed to the shell.

For the CPU-only renderer path, the same pipeline runs against the layout result directly (no WebRender involved); the bin discipline is identical because the tag namespaces are part of the display-list contract, not part of WebRender.

## Where the pieces live

| Concern | File |
|---|---|
| Tag encoding/decoding (`HitTestTag`, `ScrollbarComponent`, `CursorType`, namespace constants) | [`core/src/hit_test_tag.rs`](../../../../core/src/hit_test_tag.rs) |
| Result types (`HitTest`, `HitTestItem`, `ScrollHitTestItem`, `ScrollbarHitTestItem`, `CursorHitTestItem`, `FullHitTest`) | [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs) |
| Scroll identity (`ExternalScrollId`, `OverflowingScrollNode`, `ScrollState`, `ScrollStates`) | [`core/src/hit_test.rs`](../../../../core/src/hit_test.rs) |
| Cursor resolution (`CursorTypeHitTest::new`, CSS↔platform cursor maps) | [`layout/src/hit_test.rs`](../../../../layout/src/hit_test.rs) |
| Drag dispatch (`DragContext`, `ActiveDragType`, scrollbar math, NodeId remapping) | [`core/src/drag.rs`](../../../../core/src/drag.rs) |
| Selection model (`TextCursor`, `SelectionRange`, `TextSelection`, `MultiCursorState`) | [`core/src/selection.rs`](../../../../core/src/selection.rs) |
| Live scroll state (`ScrollManager`, `AnimatedScrollState`, scroll input queue) | [`layout/src/managers/scroll_state.rs`](../../../../layout/src/managers/scroll_state.rs) |

For how hit-test results enter the dispatch loop, see [Event System Internals](event-system.md). For how nested DOMs from `VirtualViewCallback`s register their hit areas, see [VirtualView Lazy Loading](virtual-view.md).
