---
slug: events/node-tree
title: Node Tree & Hit Testing
language: en
canonical_slug: events/node-tree
audience: external
maturity: wip
guide_order: 65
topic_only: false
short_desc: Addressing nodes, walking the tree, geometry, and changing a node without a rebuild
prerequisites: [hello-world, events, events/callbacks]
tracked_files:
  - layout/src/callbacks.rs
  - layout/src/hit_test.rs
  - core/src/dom.rs
  - core/src/styled_dom.rs
default-search-keys:
  - CallbackInfo
  - DomNodeId
  - NodeHierarchyItemId
  - get_hit_node
  - get_node_rect
  - get_node_id_by_marker
  - change_node_text
  - override_node_css_properties
---

# Node Tree & Hit Testing

A callback fires with a whole DOM behind it. This page is about reaching
into that tree: finding the node that fired, walking to its relatives,
asking where it is on screen, and changing it without rebuilding
anything.

## Introduction

Everything here hangs off `CallbackInfo`. Nothing on this page rebuilds
the DOM - these are reads against the tree `layout()` already produced,
plus a handful of targeted writes that skip `layout()` entirely. If you
want a new tree, return `Update::RefreshDom` instead and let `layout()`
run.

## Addressing a node

There are two spellings, and which one a method takes is not a matter of
taste - it tells you which layer the method belongs to.

**`DomNodeId`** is the pair `(DomId, NodeId)` and is what callbacks hand
you. `get_hit_node()` returns one. Most read methods take one:

```rust,ignore
let hit: DomNodeId = info.get_hit_node();
let rect = info.get_node_rect(hit);
```

**A split `dom_id` + `node_id`** pair appears on the methods that reach
the node *hierarchy* directly, and those return a
`NodeHierarchyItemId` rather than an `OptionDomNodeId`:

```rust,ignore
let parent = info.get_parent_node(dom_id, node_id);   // NodeHierarchyItemId
let parent = info.get_parent(some_dom_node_id);       // OptionDomNodeId
```

Both reach the same node. The `DomNodeId` form is the one to reach for
in application code; the split form exists for callers that already hold
the two halves and want to avoid rebuilding the pair. A
`NodeHierarchyItemId` is not an option type - it carries its own "no
such node" state, so check it before use.

Most apps have a single root DOM, `DomId::ROOT_ID`. Sub-DOMs come from
`IFrame` nodes and virtual views; `get_dom_ids()` lists them all.

## Which node fired

```rust,ignore
let hit = info.get_hit_node();                  // the node the event landed on
let deepest = info.get_deepest_hovered_node();  // innermost node under the cursor
let hovered = info.get_hovered_nodes();         // every node under the cursor
```

`get_hit_node()` is the node the event was dispatched to, which is not
always the innermost one under the pointer: an event that bubbles is
dispatched to an ancestor while the pointer is over a child.
`get_deepest_hovered_node()` answers the other question.

For a callback attached to many nodes - one "submit" shared by several
forms - the dataset is how you tell instances apart:

```rust,ignore
let me = match info.get_dataset(info.get_hit_node()) {
    Some(d) => d,
    None => return Update::DoNothing,
};
```

To reach a node that did *not* fire, give it an address in `layout()`
and resolve it here. A marker is the cheapest one, because it is
invisible to CSS and does not affect node equality, so minting a fresh
one every frame costs no `Mount` / `Unmount` churn:

```rust,ignore
// layout()
let marker = Uuid::short();
let bar = ProgressBar::create(0.0).dom().with_marker(Some(marker.clone()).into());

// callback
if let Some(node) = info.get_node_id_by_marker(marker).into_option() { /* ... */ }
```

`get_node_id_by_id_attribute(dom_id, id)` does the same for a node
carrying a real `id`.

## Walking the tree

`get_parent`, `get_first_child`, `get_last_child`, `get_next_sibling` and
`get_previous_sibling` take and return `DomNodeId`s.
`get_all_children_nodes(dom_id, node_id)` returns the whole child list at
once, and `get_children_count(dom_id, node_id)` its length.

`get_node_child_index_path(ancestor, node)` gives the child indices from
one node down to another - a stable path you can store and re-resolve
after a rebuild, which raw ids do not survive.

## Geometry

```rust,ignore
let rect  = info.get_node_rect(node);              // position + size, laid out
let pos   = info.get_node_position(node);
let size  = info.get_node_size(node);
let bounds = info.get_node_hit_test_bounds(node);  // what hit testing uses
```

`get_node_rect` is the laid-out box. `get_node_hit_test_bounds` is the
area that actually receives pointer events, which differs wherever a node
was given a larger or smaller hit area than its visual box.
`get_hit_node_rect()` and `get_hit_node_layout_rect()` are shorthands for
the node that fired.

To measure a tree you have *not* mounted, `measure_dom(dom, available)`
lays it out against a bound and returns the size it would take;
`measure_dom_shrink_to_fit(dom, bound)` gives the smallest box that still
fits its content. Both are the right way to size a popup before showing
it.

## Reading content and attributes

`get_node_text_content(node)`, `get_node_text_length(node)`,
`get_node_tag_name(node)`, `get_node_classes(node)`, `get_node_id(node)`
and `get_node_attribute(node, name)` read what the node is. They answer
from the styled DOM, so they see what `layout()` produced, including
anything a previous callback changed in place.

## Changing a node without rebuilding

These write straight into the current tree. No `layout()` call, no diff -
which is why they are the fast path for high-frequency updates such as a
drag or a pressure curve:

```rust,ignore
info.change_node_text(node, "42".into());
info.override_node_css_properties(dom_id, node_id, props);
info.change_node_image(dom_id, node_id, image, UpdateImageType::Background);
```

The cost is that the change is invisible to your data model: the next
`RefreshDom` rebuilds from `layout()` and the override is gone unless the
model produces it too. Use them for transient visual state, and the model
for anything that must survive a rebuild.

`insert_child_node` and `delete_node` change the shape of the tree the
same way. They are deliberately narrow - structural edits are the diff's
job, and doing many of them by hand will lose to a single `RefreshDom`.

## More methods

Everything else in this group, by what it answers.

**Addressing and identity** - `get_dom_ids`, `get_dom_subtree`,
`get_styled_dom_clone`, `get_node_id_by_id_attribute`,
`get_node_id_by_marker`, `get_node_child_index_path`.

**Relatives, split-pair form** - `get_parent_node`,
`get_first_child_node`, `get_last_child_node`, `get_next_sibling_node`,
`get_previous_sibling_node`, `get_all_children_nodes`,
`get_children_count`.

**Hit testing and hover** - `get_hit_node`, `get_hit_node_rect`,
`get_hit_node_layout_rect`, `get_deepest_hovered_node`,
`get_hovered_nodes`, `get_hovered_nodes_frames_ago`,
`get_cursor_relative_to_node`, `request_hit_test_update`.
`get_hovered_nodes_frames_ago(n)` is how a gesture looks back at what the
pointer was over before the current frame.

**Focus** - `is_node_focused`, `is_node_focused_for_seat`,
`get_focused_node`, `get_focused_node_for_seat`, `set_focus_to_node`. The
`_for_seat` forms take a seat id, because one window can have several
independent cursors; see [Pointer, Pen & Touch](pointer.md).

**Scrolling, per node** - `find_scroll_parent`, `get_scroll_node_info`,
`get_scroll_offset_for_node`, `scroll_node_into_view`. See
[Scrolling](scrolling.md).

**Text and selection, per node** - `get_node_cursor_position`,
`get_node_selection_ranges`, `node_has_selection`. See
[Text Selection](text-selection.md).

**Mutation** - `change_node_text`, `change_node_css_properties`,
`override_node_css_properties`, `change_node_image`,
`change_node_image_mask`, `set_node_ids_and_classes`, `insert_child_node`,
`delete_node`.

**Other** - `measure_dom`, `measure_dom_shrink_to_fit`,
`take_screenshot_of_node`, `open_menu_for_node`, `open_menu_for_hit_node`,
`is_node_drag_active`, `get_dragged_node`, `get_dataset`.
