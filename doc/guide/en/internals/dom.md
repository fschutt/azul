---
slug: internals/dom
title: DOM Internals
language: en
canonical_slug: internals/dom
audience: contributor
maturity: mature
guide_order: null
topic_only: false
short_desc: How the public `Dom` type is built, flattened, and reconciled
prerequisites: []
tracked_files:
  - core/src/dom.rs
  - core/src/diff.rs
  - core/src/styled_dom.rs
  - core/src/xml.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - Dom
  - NodeData
  - NodeType
  - NodeHierarchyItem
  - NodeFlags
  - AccessibilityInfo
  - AttributeType
  - DomVec
  - RefAny
  - CssPropertyWithConditionsVec
---

# DOM Internals

## Overview

Azul's DOM is a `#[repr(C)]` tree of `NodeData` plus per-subtree `Css` stylesheets. Two representations exist side by side: the recursive `Dom` produced by the public builder API, and `FastDom`, a flat parent/sibling arena built directly by the XML parser and codegen. Both flatten into the same `StyledDom` — parallel arrays of `node_hierarchy`, `node_data`, `styled_nodes`, and a `css_property_cache` — which is what every layout pass actually sees.

The contributor surface for the DOM divides into three concerns: how a single node is shaped (the `NodeData` payload), how a tree of nodes is reconciled across frames (the diff pipeline), and how higher-level structures plug in (component-typed authoring, virtualised slices, nested IFrame-style children). The first two live in `core/src/dom.rs` and `core/src/diff.rs`. The remaining concerns each get a dedicated sub-page.

This page is the orientation. It walks through `NodeData`, the tree-to-arena flatten, and the reconciliation machinery — enough to read code in `core/src/dom.rs` without surprise — and links to the sub-pages where those structures interact with components, virtual views, or nested DOMs.

## NodeData and the hot 32-byte path

`NodeData` is the per-node payload:

```rust,ignore
#[repr(C)]
pub struct NodeData {
    pub node_type: NodeType,
    pub callbacks: CoreCallbackDataVec,
    pub css_props: CssPropertyWithConditionsVec,
    pub flags: NodeFlags,
    pub accessibility: Option<Box<AccessibilityInfo>>,
    extra: Option<Box<NodeDataExt>>,
}
```

The first five fields fit in a 32-byte hot path. Anything rarer — attribute strings, the `RefAny` dataset, the optional `VirtualViewCallback`, embedded SVG, context menus — is heap-indirected through `NodeDataExt`. Around 95 % of nodes never allocate `extra`.

`css_props` is `Vec<CssPropertyWithConditions>`, the inline CSS attached to the node. Each entry carries the property plus the dynamic selector that gates it (`@media`, `@lang`, `@theme`, pseudo-state). Within `css_props`, evaluation is "last wins". `callbacks` are `(On, Callback, RefAny)` triples; the `RefAny` is the data pointer the callback receives at fire time. `Hash for NodeData` deliberately hashes the `RefAny`'s type id rather than its contents, so reconciliation does not see two clones of the same data as different.

`NodeType` covers the HTML5 semantic tags (`Div`, `P`, `A`, `Span`, `Section`, `Article`, ...), media (`Image`, `Audio`, `Video`), inline framing (`IFrame`, `OpenGl`, `VirtualView`), forms, tables, and lists. Most variants are unit-like; only `Text(AzString)`, `Image(ImageRef)`, `IFrame(IFrameNodeData)`, `OpenGl(GlCallbackData)`, and `Input(InputType, ...)` carry data.

`NodeFlags` packs `contenteditable`, the `tab_index` variant, an anonymous-box flag, and the override value into one `u32`:

```text
[31]     contenteditable
[30:29]  tab_index variant: 00=None, 01=Auto, 10=OverrideInParent, 11=NoKeyboardFocus
[28]     is_anonymous (table layout fixup)
[27:0]   OverrideInParent value (0..2^28)
```

Anonymous boxes are inserted by the table-layout fixup pass when a `<table>` ancestor needs to wrap stray inline content in implicit `<tbody>` / `<tr>` / `<td>` boxes. The flag tells the diff and the accessibility tree to ignore them.

## Building a Dom

The builder API is method-chained. `Dom::create_div()`, `Dom::create_p()`, `Dom::create_text(s)` are `const fn` constructors; `with_child`, `with_id`, `with_class`, `with_callback` consume `self` and return it:

```rust,ignore
use azul::dom::{Dom, On};
use azul::callbacks::{Callback, RefAny};

let dom = Dom::create_div()
    .with_id("root".into())
    .with_class("container".into())
    .with_child(
        Dom::create_p()
            .with_child(Dom::create_text("Hello"))
    )
    .with_child(
        Dom::create_button()
            .with_callback(On::MouseUp.into(), my_data, my_callback)
    );
```

`with_child` updates `estimated_total_children` so the tree-to-arena flatten step can pre-size its allocations; `add_child` is the `&mut self` variant. `Dom` also carries a per-subtree `CssVec` — `Dom::create_div().with_css(my_stylesheet)` attaches a `<style>`-scoped sheet that applies only to this subtree.

`IdOrClass` is the CSS attribute primitive:

```rust,ignore
pub enum IdOrClass {
    Id(AzString),
    Class(AzString),
}
```

These live in `NodeData::attributes()` (off the `NodeDataExt` heap path). `as_id()` and `as_class()` are the accessors used by the cascade and by the diff's reconciliation key. A node can carry any number of classes; CSS specificity sums them in the matching pass. The id is hashed first because it's the cheapest stable key for diffing.

## Tree to arena: from Dom to StyledDom

The recursive `Dom` is a builder representation. Layout never sees it. `StyledDom::from_dom(...)` flattens the tree into parallel arrays:

- `node_hierarchy: NodeHierarchyItemVec` — parent/sibling pointers indexed by `NodeId`.
- `node_data: NodeDataVec` — `NodeData` per node, same indexing.
- `styled_nodes: StyledNodeVec` — pseudo-state bits per node.
- `css_property_cache: CssPropertyCachePtr` — see [Cascade, Inheritance, Restyle](styling/cascade.md).

Pre-order indexing is invariant: parent index < child index. Every cascade pass walks `0..node_count` forwards and trusts that any value it reads from a parent is already resolved. Inheritance, font-size resolution, and `compute_inherited_values` all depend on this. Reordering the arena, or building bottom-up, breaks the cascade silently — values from later siblings can leak into earlier ones.

For bulk construction (XML parsing, code-generated DOM, the component-typed pipeline), `FastDom` skips the recursive intermediate and populates the flat vectors directly. `StyledDom::create_from_fast_dom()` consumes it without conversion.

## Hashes and fingerprints

Three hashes appear on every node, each with a different job:

- **Content hash** — `NodeData::calculate_node_data_hash() -> DomNodeHash`. Includes `node_type`, attributes, flags, callback event/function/refany-type, and CSS props by discriminant. Used as the Tier 2 reconciliation match.
- **Structural hash** — `NodeData::calculate_structural_hash()`. Like the content hash, but ignores text content, so it catches text edits inside otherwise-identical nodes.
- **Reconciliation key** — see the next section.

The `RefAny` type id is part of the content hash; its contents are not. This is what lets a stateful component preserve its identity when its internal state changes.

`NodeDataFingerprint` is a 6×u64 struct that fingerprints a node's fields independently:

```rust,ignore
pub struct NodeDataFingerprint {
    pub content_hash: u64,      // node_type
    pub state_hash: u64,        // hover/focus/active bits
    pub inline_css_hash: u64,   // css_props
    pub ids_classes_hash: u64,
    pub callbacks_hash: u64,
    pub attrs_hash: u64,        // contenteditable, tab_index, dataset
}
```

`NodeDataFingerprint::compute(node, styled_state)` is O(1) per node. Comparing two fingerprints tells you which categories changed without running the full `compute_node_changes` walk. The reconciliation pipeline uses this as Tier 1: if all six hashes match, the node didn't change at all and `compute_node_changes` is skipped entirely.

## Reconciliation key

`calculate_reconciliation_key(node_data, hierarchy, node_id) -> u64` produces a single `u64` per node by checking three priorities, in order:

1. `node.get_key()` — if `.with_key("foo")` was called, hash that.
2. CSS id — if the node has an id attribute, hash it.
3. Structural key — `Hash(node_type, classes..., nth_of_type, parent_key)`, recursively.

The structural key is the interesting one. `nth_of_type` counts preceding siblings *of the same `NodeType`*, not all siblings. Inserting a `<button>` before two `<p>` siblings doesn't shift the buttons' keys.

`precompute_reconciliation_keys` runs once over the arena up front so the per-node match in `reconcile_dom` is O(1).

## Diff pipeline

`reconcile_dom(old_data, new_data, old_hier, new_hier, old_layout, new_layout, dom_id, ts)` produces a `DiffResult { events, node_moves }`. Three reconciliation tiers run in priority order:

- **Tier 1.** Reconciliation key catches logical identity from `.with_key()`, CSS id, and the structural fallback.
- **Tier 2.** Content hash catches pure reorders of anonymous nodes.
- **Tier 3.** Structural hash catches text edits inside an otherwise-identical node.

Each tier indexes the old DOM into a `BTreeMap<Key, VecDeque<NodeId>>` and consumes matches in document order, so two siblings with the same structural key match left to right. If `.with_key()` is set on the new node and finds no match, the node is a `Mount` — Tier 2/3 are skipped. Explicit keys are an opt-in identity contract; falling through to coarser tiers would silently match unrelated nodes.

For each match, `reconcile_dom` fires:

- `Resize` if the layout rect's size differs between old and new and the node has a resize callback.
- `Update` if the match was Tier 1 (logical identity preserved) but the content hash changed and the node has an update callback. Tier 2/3 matches are content-identical by definition; text edits inside an editable node are handled separately by `reconcile_cursor_position`.

Unmatched old nodes become `Unmount` events. Unmatched new nodes become `Mount` events.

Once a pair is matched, `compute_node_changes(old, new, old_state, new_state) -> NodeChangeSet` does field-by-field comparison and returns bitflags:

```text
NODE_TYPE_CHANGED       0x0001   Text→Image, etc. — short-circuits everything else
TEXT_CONTENT            0x0002
IDS_AND_CLASSES         0x0004   triggers restyle
INLINE_STYLE_LAYOUT     0x0008
CHILDREN_CHANGED        0x0010
IMAGE_CHANGED           0x0020
CONTENTEDITABLE         0x0040
TAB_INDEX               0x0080
INLINE_STYLE_PAINT      0x0100   (paint-only)
STYLED_STATE            0x0200   (paint-only — hover/focus/active changed)
CALLBACKS               0x0400   (no visual effect)
DATASET                 0x0800
ACCESSIBILITY           0x1000
```

Composite masks: `AFFECTS_LAYOUT` (low 8 bits + `IMAGE_CHANGED`) and `AFFECTS_PAINT` (`INLINE_STYLE_PAINT | STYLED_STATE`). `is_visually_unchanged()` returns true when only `CALLBACKS | DATASET | ACCESSIBILITY` changed.

## ChangeAccumulator: one stream of work per frame

`ChangeAccumulator` is the single source of truth for "what work runs this frame". It merges three input paths:

1. DOM reconciliation (`reconcile_dom_with_changes` → `merge_extended_diff`).
2. CSS restyle on pseudo-state change (`merge_restyle_result`, see `restyle_on_state_change`).
3. Direct runtime edits (`add_text_change`, `add_css_change`, `add_image_change`, `add_mount`, `add_unmount`).

The accumulator tracks `max_scope: RelayoutScope` (None / IfcOnly / SizingOnly / Full — see [Cascade](styling/cascade.md)) so layout can be skipped entirely when `max_scope == None`. `classify_change_scope` maps `NodeChangeSet` flags to `RelayoutScope`:

- `NODE_TYPE_CHANGED | CHILDREN_CHANGED | IDS_AND_CLASSES` → `Full`
- `INLINE_STYLE_LAYOUT` → walk inline props, take max of `prop.relayout_scope()`
- `TEXT_CONTENT` → `IfcOnly`
- `IMAGE_CHANGED | CONTENTEDITABLE` → `SizingOnly`
- paint-only flags → `None`

## State migration across rebuilds

Some state lives outside `NodeData` but is keyed by `NodeId`: scroll offsets, focus, cursor position, drag state, selection ranges. After a diff, those node IDs are stale.

`create_migration_map(node_moves) -> OrderedMap<NodeId, NodeId>` flips the `Vec<NodeMove>` into a lookup table. The managers (focus, scroll, cursor, hover) walk their state and remap.

`transfer_states(old, new, node_moves)` is different. It runs **merge callbacks**: if the new node carries a `dataset_merge_callback` and both old and new have a `dataset` `RefAny`, the callback combines them and stores the result on the new node. This is how a stateful component preserves internal state across a re-render.

Both must run **before** the old DOM is dropped.

## Adding a new node type

1. Add a variant to `NodeType` and update `Hash` / `Ord` / `Display`.
2. Add a `Dom::create_<tag>()` constructor following the existing pattern.
3. Add a `NodeData::create_<tag>()` constructor.
4. Add UA CSS defaults in `core/src/ua_css.rs` if the tag should match a built-in stylesheet entry.
5. Update the XML parser's tag table in `layout/src/xml/mod.rs` so XML / XHTML round-trips.

## Adding a new field to NodeData

A checklist for safely extending `NodeData` without breaking the diff or the hashing contract:

1. Add the field. If it's larger than ~8 bytes or rarely set, put it in `NodeDataExt` instead.
2. Update `Hash for NodeData`. Add the field to the hash, or document why it's intentionally excluded (the callbacks-by-pointer pattern is the canonical reason).
3. Update `compute_node_changes`. Add a compare arm and a new `NodeChangeSet` flag, or fold into an existing flag.
4. Update `NodeDataFingerprint`. Add a `category_hash` if the new field deserves its own diff bucket; otherwise fold into `attrs_hash`.
5. Update `classify_change_scope` so the new flag maps to a `RelayoutScope`.
6. If the field is FFI-visible, regenerate `api.json` bindings.

## See also

The DOM internals tree continues with three companion pages, each focused on one cross-cutting concern that touches `NodeData`:

- [Component Type System](dom/component-types.md) — typed component authoring, the `ComponentDef` / `ComponentDataModel` registry, and how built-in HTML tags map onto `NodeType`.
- [VirtualView Lazy Loading](dom/virtual-view.md) — how a node can declare itself a virtualised viewport that lazily fetches DOM slices via a callback.
- [IFrame Scroll and Display Lists](dom/iframe-scroll.md) — how `NodeType::VirtualView` becomes a placeholder in the parent display list, gets replaced with a child DOM, and is composited into a separate WebRender pipeline.

The DOM is also the input to the styling pipeline. The companion subtree is rooted at:

- [Styling Subsystem](styling.md) — parent overview of cascade, compact cache, parser, and system style.
  - [Cascade, Inheritance, Restyle](styling/cascade.md) — `NodeData::css_props` plus stylesheets become a `CssPropertyCache`.
  - [Compact Property Cache](styling/compact-cache.md) — the resolved layout-hot values consumed by the solver.
  - [CSS Parser](styling/css-parser.md) — how the per-node `css_props` strings become typed `CssProperty` values.

For where the diff results feed into rendering and event handling, see:

- [Layout Solver](layout.md) — what `RelayoutScope` actually controls.
- [Rendering Pipeline](rendering.md) — how the resulting display list reaches the screen.
- [Event System Internals](events.md) — hit-testing, callback invocation, the `Update` protocol.

## Coming Up Next

- [Styling Subsystem](styling.md) — how parsed CSS becomes per-node resolved values
- [Component Type System](dom/component-types.md) — typed component authoring
- [VirtualView Lazy Loading](dom/virtual-view.md) — the virtual view layer
- [IFrame Scroll](dom/iframe-scroll.md) — nested DOMs and display-list compositing
