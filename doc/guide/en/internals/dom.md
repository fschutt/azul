---
slug: dom-internals
title: DOM Internals
language: en
canonical_slug: dom-internals
audience: contributor
maturity: mature
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - core/src/dom.rs
  - core/src/diff.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:43:38Z
---

The DOM is a `#[repr(C)]` tree of `NodeData` plus per-subtree `Css` stylesheets. Two representations exist: the recursive [`Dom`](https://github.com/maps4print/azul/blob/master/core/src/dom.rs) used by the public builder API, and [`FastDom`](https://github.com/maps4print/azul/blob/master/core/src/dom.rs) — a flat parent/sibling arena built directly by the XML parser. Both feed `StyledDom`, which is what the layout solver sees.

## File map

| File | Lines | Purpose |
|---|---|---|
| `core/src/dom.rs:3248` | `Dom` | recursive builder tree |
| `core/src/dom.rs:3291` | `FastDom` | flat arena, used by XML / mass construction |
| `core/src/dom.rs:1511` | `NodeData` | per-node payload (type, callbacks, css_props, flags, accessibility, extra) |
| `core/src/dom.rs:239` | `NodeType` | 50+ HTML element variants plus `Text`, `Image`, `IFrame`, `OpenGl` |
| `core/src/dom.rs:1978` | `NodeFlags` | u32 bitfield: contenteditable, tab_index, anonymous |
| `core/src/dom.rs:1206` | `IdOrClass` | CSS id/class attribute |
| `core/src/dom.rs:1124` | `On` | event types attached to callbacks |
| `core/src/id.rs:174` | `Node`, `NodeHierarchy` | flat arena hierarchy primitive (parent / prev_sibling / next_sibling / last_child) |

## `NodeData`

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

The struct is split into a hot 32-byte fast path (the first five fields) and a heap-allocated `NodeDataExt` (`core/src/dom.rs:1773`) for attributes, dataset (`RefAny`), virtual_view, svg_data, and menus. ~95% of nodes never allocate `extra`.

`css_props` is `Vec<CssPropertyWithConditions>` — inline CSS attached to the node along with the dynamic selector that gates it (`@media`, `@lang`, `@theme`, pseudo-state). Property evaluation is "last wins" within `css_props`.

`callbacks` are tuples of `(On, Callback, RefAny)` where `RefAny` is the data pointer the callback receives. Hashing `NodeData` (`core/src/dom.rs:1541`) intentionally hashes the `RefAny`'s type id rather than its contents, so reconciliation does not see two clones of the same data as different.

## `NodeType` and the 50+ HTML elements

`NodeType` (`core/src/dom.rs:239`) covers all HTML5 semantic tags plus media (`Image`, `Audio`, `Video`), inline framing (`IFrame`, `OpenGl`), forms (`Input`, `Select`, `Textarea`, `Button`, `Label`), tables, lists, and SVG-style virtual nodes. Most variants are unit-like — only `Text(AzString)`, `Image(ImageRef)`, `IFrame(IFrameNodeData)`, `OpenGl(GlCallbackData)`, and `Input(InputType, ...)` carry data.

To add a node type:

1. Add a variant to `NodeType` and update `Hash`/`Ord`/`Display`.
2. Add a `Dom::create_<tag>()` constructor following the existing pattern in `core/src/dom.rs:3405`.
3. Add a `NodeData::create_<tag>()` constructor in `core/src/dom.rs:2172`.
4. Add UA CSS defaults in `core/src/ua_css.rs` if it should match a built-in stylesheet entry.
5. Update the XML parser's tag table in `layout/src/xml/mod.rs` so XML / XHTML round-trips.

## `NodeFlags` packing

Tab index, contenteditable, and anonymous-box are packed into a single u32 (`core/src/dom.rs:1989`):

```text
[31]     contenteditable
[30:29]  tab_index variant: 00=None, 01=Auto, 10=OverrideInParent, 11=NoKeyboardFocus
[28]     is_anonymous (table layout fixup)
[27:0]   OverrideInParent value (0..2^28)
```

Anonymous boxes are inserted by the table-layout fixup pass (`layout/src/solver3/`) when a `<table>` ancestor needs to wrap stray inline content in implicit `<tbody>`/`<tr>`/`<td>` boxes. They are flagged so the diff and accessibility tree can ignore them.

## Building a `Dom`

The builder API is method-chained. `Dom::create_div()`, `Dom::create_p()`, `Dom::create_text()` are `const fn` constructors; `with_child`, `with_id`, `with_class`, `with_callback` consume `self` and return it.

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

`with_child` (`core/src/dom.rs:4920`) updates `estimated_total_children` so the tree→arena flatten step can pre-size its allocations. `add_child` (`core/src/dom.rs:4871`) is the `&mut self` variant.

`Dom` also carries a per-subtree `CssVec`. `Dom::create_div().with_css(my_stylesheet)` attaches a `<style>`-scoped sheet that applies only to this subtree.

## Tree → arena: from `Dom` to `StyledDom`

The recursive `Dom` is a builder representation. Layout never sees it; instead `StyledDom::from_dom(...)` flattens the tree into parallel arrays:

- `node_hierarchy: NodeHierarchyItemVec` — parent/sibling pointers indexed by `NodeId`.
- `node_data: NodeDataVec` — `NodeData` per node, same indexing.
- `styled_nodes: StyledNodeVec` — pseudo-state bits per node.
- `css_property_cache: CssPropertyCachePtr` — see [Cascade, Inheritance, Restyle](cascade.md).

Pre-order indexing is invariant: parent index < child index, which lets every cascade pass walk the array forwards and trust that parent values are already resolved.

For bulk construction (XML parsing, code-generated DOM), `FastDom` skips the recursive intermediate by populating the flat vectors directly. `StyledDom::create_from_fast_dom()` consumes it without conversion.

## `IdOrClass` and CSS attachment

```rust,ignore
pub enum IdOrClass {
    Id(AzString),
    Class(AzString),
}
```

These live in `NodeData::attributes()` (off the `NodeDataExt` heap path). `IdOrClass::as_id()` and `as_class()` are used by the cascade and by the diff's reconciliation key (`core/src/diff.rs:346`).

A node can carry any number of classes; CSS specificity sums them in the matching pass. The id is hashed first because it's the cheapest stable key for diffing.

## Hashing and fingerprints

Three hashes appear in this module:

- **Content hash** — `NodeData::calculate_node_data_hash() -> DomNodeHash` (`core/src/dom.rs:225`). Includes `node_type`, attributes, flags, callback event/function/refany-type, and CSS props by discriminant. Used as the Tier 2 reconciliation match.
- **Structural hash** — `NodeData::calculate_structural_hash()`. Like the content hash, but ignores text content. Catches text edits inside otherwise-identical nodes.
- **Reconciliation key** — `core/src/diff.rs:339`, see below.

The `RefAny` type id is part of the content hash but its contents are not. This avoids treating "same component, new state object" as a different node.

## Reconciliation key

`calculate_reconciliation_key(node_data, hierarchy, node_id) -> u64` at `core/src/diff.rs:339` produces a single `u64` per node by checking three priorities:

1. `node.get_key()` — if `.with_key("foo")` was called, use that hash.
2. CSS id — if the node has an id attribute, hash it.
3. Structural key — `Hash(node_type, classes..., nth_of_type, parent_key)` recursively.

The structural key is the interesting one. `nth_of_type` is the count of preceding siblings *of the same `NodeType`*, not all siblings — so inserting a `<button>` before two `<p>` siblings doesn't shift their keys.

Reconciliation pre-computes the keys for every node up front (`precompute_reconciliation_keys`, `core/src/diff.rs:398`) so the per-node match in `reconcile_dom` is O(1).

## Diff pipeline overview

`reconcile_dom(old_data, new_data, old_hier, new_hier, old_layout, new_layout, dom_id, ts)` at `core/src/diff.rs:459` produces a `DiffResult { events, node_moves }`. Three reconciliation tiers, in priority order:

| Tier | Key | Catches |
|---|---|---|
| 1 | reconciliation key | logical identity (`.with_key()`, CSS id, structural) |
| 2 | content hash (`DomNodeHash`) | pure reorders of anonymous nodes |
| 3 | structural hash | text edits inside an otherwise-identical node |

Each tier indexes the old DOM into a `BTreeMap<Key, VecDeque<NodeId>>` and consumes matches in document order, so two siblings with the same structural key match left-to-right.

If `.with_key()` is set on the new node and finds no match, the node is a `Mount` — Tier 2/3 are skipped. Explicit keys are an opt-in identity contract; falling through to coarser tiers would silently match unrelated nodes.

For each match, `reconcile_dom` fires:

- `Resize` if the old/new layout rect's size differs and the node has a resize callback.
- `Update` if the match was Tier 1 (logical identity preserved) but the content hash changed and the node has an update callback. Tier 2/3 matches are content-identical by definition; text edits are handled separately by `reconcile_cursor_position` (`core/src/diff.rs:939`).

Unmatched old nodes → `Unmount` events. Unmatched new nodes → `Mount` events.

## `compute_node_changes` and `NodeChangeSet`

Once Tier-1/2/3 matched a node pair, `compute_node_changes(old, new, old_state, new_state) -> NodeChangeSet` (`core/src/diff.rs:167`) does field-by-field comparison and returns bitflags:

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

## `ChangeAccumulator` — the unified change stream

`ChangeAccumulator` (`core/src/diff.rs:1065`) is the single source of truth for "what work runs this frame". It merges three input paths:

1. DOM reconciliation (`reconcile_dom_with_changes` → `merge_extended_diff`)
2. CSS restyle on pseudo-state change (`merge_restyle_result`, see `core/src/styled_dom.rs:1604`)
3. Direct runtime edits (`add_text_change`, `add_css_change`, `add_image_change`, `add_mount`, `add_unmount`)

The accumulator tracks `max_scope: RelayoutScope` (None / IfcOnly / SizingOnly / Full — see [Cascade, Inheritance, Restyle](cascade.md)) so layout can be skipped entirely when `max_scope == None`.

`classify_change_scope` (`core/src/diff.rs:1285`) maps `NodeChangeSet` flags to `RelayoutScope`:

- `NODE_TYPE_CHANGED | CHILDREN_CHANGED | IDS_AND_CLASSES` → `Full`
- `INLINE_STYLE_LAYOUT` → walk inline props, take max of `prop.relayout_scope()`
- `TEXT_CONTENT` → `IfcOnly`
- `IMAGE_CHANGED | CONTENTEDITABLE` → `SizingOnly`
- paint-only flags → `None`

## State migration: `transfer_states` and `create_migration_map`

Some state lives outside `NodeData` but is keyed by `NodeId`: scroll offsets, focus, cursor position, drag state. After a diff, those node IDs are stale.

`create_migration_map(node_moves) -> OrderedMap<NodeId, NodeId>` at `core/src/diff.rs:746` flips the `Vec<NodeMove>` into a lookup table; managers (focus, scroll, cursor, hover) walk their state and remap.

`transfer_states(old, new, node_moves)` at `core/src/diff.rs:776` is different — it runs **merge callbacks**. If the new node carries a `dataset_merge_callback` and both old/new have a `dataset` `RefAny`, the callback combines them and stores the result on the new node. This is how a stateful component preserves internal state across a re-render.

Both must run **before** the old DOM is dropped.

## `NodeDataFingerprint` — the fast pre-check

`NodeDataFingerprint` (`core/src/diff.rs:1411`) is a 6×u64 struct that fingerprints a node's fields independently:

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

## Adding a new field to `NodeData`

A checklist for safely extending `NodeData` without breaking the diff or hashing:

1. Add the field. If it's larger than ~8 bytes or rarely set, put it in `NodeDataExt` (`core/src/dom.rs:1773`) instead.
2. Update `Hash for NodeData` (`core/src/dom.rs:1541`) — add the field to the hash, or document why it's intentionally excluded (callbacks-by-pointer pattern).
3. Update `compute_node_changes` (`core/src/diff.rs:167`) — add a compare arm and a new `NodeChangeSet` flag, or fold into an existing flag.
4. Update `NodeDataFingerprint` (`core/src/diff.rs:1411`) — add a `category_hash` if the new field deserves its own diff bucket, else fold into `attrs_hash`.
5. Update `classify_change_scope` (`core/src/diff.rs:1285`) so the new flag maps to a `RelayoutScope`.
6. If the field is FFI-visible, regenerate `api.json` bindings.

## See also

- [CSS Parser](css-parser.md) — how the per-node `css_props` strings become typed `CssProperty` values.
- [Cascade, Inheritance, Restyle](cascade.md) — how `NodeData::css_props` plus stylesheets become a `CssPropertyCache`.
- [Compact Property Cache](compact-cache.md) — the final resolved layout values consumed by the solver.
