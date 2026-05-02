---
slug: widgets/structural
title: Structural Widgets
language: en
canonical_slug: widgets/structural
audience: external
maturity: wip
guide_order: 142
topic_only: false
short_desc: Container widgets — panels, splitters, tab views, list views, and tree views.
prerequisites: [widgets]
tracked_files:
  - layout/src/widgets/list_view.rs
  - layout/src/widgets/node_graph.rs
  - layout/src/widgets/ribbon.rs
  - layout/src/widgets/tabs.rs
  - layout/src/widgets/tree_view.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:43Z
---

# Structural Widgets

> **WIP** — these widgets render today, but several have known gaps:
> `ListView` does not wire its column or row callbacks into the DOM,
> `NodeGraph` does not draw connection curves, and `Ribbon` has no example
> call site. Treat their APIs as provisional. Plain `Tabs` and `TreeView`
> are usable as documented.

The structural widgets are containers — they take other widgets or DOMs
as content and lay them out in tabbed, hierarchical, or tabular forms.
All live in `layout::widgets`.

## Tabs

A tab UI is two cooperating widgets: `TabHeader` (the clickable bar of
labels) and `TabContent` (the panel for the active tab). Compose them
yourself in a column flexbox so the header stays a fixed height and the
content fills the rest:

```rust,no_run
# use azul::prelude::*;
# use azul::vec::StringVec;
# use azul::widgets::tabs::{TabContent, TabHeader, TabHeaderState};
extern "C" fn on_tab(
    _d: RefAny, _i: CallbackInfo, _s: TabHeaderState,
) -> Update {
    Update::RefreshDom
}

let labels = StringVec::from_vec(vec!["General".into(), "Advanced".into()]);
let dom = Dom::create_div()
    .with_inline_css("display: flex; flex-direction: column;")
    .with_child(
        TabHeader::create(labels)
            .with_active_tab(0)
            .with_on_click(RefAny::new(()), on_tab)
            .dom()
    )
    .with_child(
        TabContent::new(Dom::create_text("Settings go here."))
            .with_padding(true)
            .dom()
    );
```

`TabHeader` (`layout/src/widgets/tabs.rs:1155`) keeps `active_tab: usize`
and emits `TabHeaderState { active_tab: idx }` on click. The active state
is **driven externally** — the widget does not toggle itself; your callback
must rebuild the DOM with a new `active_tab`. `TabContent::with_padding`
adds the standard 7 px frame; pass `false` for a tab that hosts its own
chrome.

## TreeView

`TreeView` (`layout/src/widgets/tree_view.rs:204`) renders a hierarchy
where each node has a label, an optional list of children, and per-node
expanded/selected flags:

```rust,ignore
pub struct TreeViewNode {
    pub label:       AzString,
    pub children:    TreeViewNodeVec,
    pub is_expanded: bool,
    pub is_selected: bool,
}
```

Build the data with the chained `with_*` builders and convert in one
call:

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::tree_view::{TreeView, TreeViewNode};
extern "C" fn on_click(_d: RefAny, _i: CallbackInfo, _idx: usize) -> Update {
    Update::RefreshDom
}

let root = TreeViewNode::new("Project")
    .with_expanded(true)
    .with_child(TreeViewNode::new("src").with_expanded(true)
        .with_child(TreeViewNode::new("main.rs"))
        .with_child(TreeViewNode::new("lib.rs")))
    .with_child(TreeViewNode::new("Cargo.toml"));

let dom = TreeView::new(root)
    .with_on_node_click(RefAny::new(()), on_click)
    .dom();
```

The `usize` argument to the click callback is the depth-first preorder
index of the clicked node — not a node ID. Map it back to your data by
walking the tree in the same order.

`is_expanded` / `is_selected` are read-only outputs of the widget. To
toggle, store the tree in your `RefAny`, mutate it inside the callback,
and return `Update::RefreshDom`.

## ListView

`ListView` (`layout/src/widgets/list_view.rs:1459`) renders a tabular list
with column headers and rows. The data shape:

```rust,ignore
pub struct ListView {
    pub columns:        StringVec,
    pub rows:           ListViewRowVec,
    pub sorted_by:      OptionUsize,
    pub scroll_offset:  PixelValueNoPercent,
    pub content_height: OptionPixelValueNoPercent,
    pub column_context_menu: OptionMenu,
    pub on_lazy_load_scroll: OptionListViewOnLazyLoadScroll,
    pub on_column_click:     OptionListViewOnColumnClick,
    pub on_row_click:        OptionListViewOnRowClick,
}
```

The widget supports lazy-loading: pass an explicit `content_height` to
size the scroll region for unloaded rows, and use `on_lazy_load_scroll` to
fetch more data as the user scrolls.

> **Known limitation:** the current `dom()` implementation does not yet
> attach the `on_column_click`, `on_row_click`, `on_lazy_load_scroll`, or
> `column_context_menu` to event filters in the produced DOM. Configure
> them now to keep your call sites stable, but plan to handle clicks via
> external event filters until the widget wires them through.

## Ribbon

`Ribbon` (`layout/src/widgets/ribbon.rs:130`) is an Office-style ribbon:
a tab bar where each tab contains one or more titled `RibbonSection`s.
Sections separate themselves with vertical dividers and stack their
title below the content.

```rust,no_run
# use azul::prelude::*;
# use azul::vec::RibbonTabVec;
# use azul::widgets::ribbon::{Ribbon, RibbonSection, RibbonTab};
let home = RibbonTab::new("Home".into())
    .with_section(RibbonSection::new("Clipboard".into(), Dom::create_div()))
    .with_section(RibbonSection::new("Font".into(), Dom::create_div()));

let view = RibbonTab::new("View".into())
    .with_section(RibbonSection::new("Zoom".into(), Dom::create_div()));

let dom = Ribbon::new(RibbonTabVec::from_vec(vec![home, view]))
    .dom();
```

`set_active_tab(idx)` clamps to the last valid index. The
`on_tab_click(data, fn)` callback receives the clicked tab index — like
`Tabs`, the widget is externally driven; rebuild your DOM with the new
active tab to switch.

## NodeGraph

> **Known limitation:** `NodeGraph` (`layout/src/widgets/node_graph.rs:55`)
> renders nodes and ports correctly, but **connection curves between nodes
> are not currently drawn**. The `draw_connection` function returns a null
> image pending image-rendering callback support inside the widget. Use it
> for tooling demos and structural editors today; production-quality node
> editors should expect to draw their own connection overlay.

`NodeGraph` is a graph editor: typed input/output ports on rectangular
nodes, drag-to-position, drag-to-connect, and per-node editable fields
(text, number, checkbox, color, file path).

```rust,ignore
pub struct NodeGraph {
    pub node_types:        NodeTypeIdInfoMapVec,
    pub input_output_types: InputOutputTypeIdInfoMapVec,
    pub nodes:             NodeIdNodeMapVec,
    pub allow_multiple_root_nodes: bool,
    pub offset:            LogicalPosition,
    pub callbacks:         NodeGraphCallbacks,
    pub add_node_str:      AzString,
    pub scale_factor:      f32,
    // ...
}
```

A graph is described in three layers:

1. **Type catalog** — register every node type (`NodeTypeId` →
   `NodeTypeInfo` with name, inputs, outputs) and every port data type
   (`InputOutputTypeId` → `InputOutputInfo` with display name and color).
2. **Instances** — `Node` values keyed by `NodeGraphNodeId`, each with a
   position, a list of `NodeTypeField`s for editable values, and
   `connect_in` / `connect_out` lists.
3. **Callbacks** — `NodeGraphCallbacks` carries eight slots for graph
   events (`on_node_added`, `on_node_removed`, `on_node_dragged`,
   `on_node_graph_dragged`, `on_node_connected`,
   `on_node_input_disconnected`, `on_node_output_disconnected`,
   `on_node_field_edited`).

Field values are typed via `NodeTypeFieldValue`:

| Variant | Renders as |
|---|---|
| `TextInput(AzString)` | a `TextInput` widget |
| `NumberInput(f32)` | a `NumberInput` widget |
| `CheckBox(bool)` | a `CheckBox` widget |
| `ColorInput(ColorU)` | a `ColorInput` widget |
| `FileInput(OptionString)` | a `FileInput` widget |

Editing any of them invokes the graph's `on_node_field_edited` callback
with the originating node ID, the field index, the node type, and the
new value. The widget delegates to the right input type automatically;
you only handle the high-level event.

## Picking the right container

| You want… | Use |
|---|---|
| flat tabs of independent panels | `TabHeader` + `TabContent` |
| ribbon-style tabs with named subsections | `Ribbon` |
| a hierarchical sidebar | `TreeView` |
| a tabular data grid (columns + rows) | `ListView` |
| a visual graph editor | `NodeGraph` |

For ad-hoc layouts, plain `Dom::create_div` with flex CSS is almost always
shorter than wiring a widget; reach for these when you want the
platform-native look or the widget's specific event surface.
