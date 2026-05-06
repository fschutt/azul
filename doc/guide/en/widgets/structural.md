---
slug: widgets/structural
title: Structural Widgets
language: en
canonical_slug: widgets/structural
audience: external
maturity: wip
guide_order: 142
topic_only: false
short_desc: Panels, splitters, tab views, list views, tree views
prerequisites: [widgets]
tracked_files:
  - layout/src/widgets/list_view.rs
  - layout/src/widgets/ribbon.rs
  - layout/src/widgets/tabs.rs
  - layout/src/widgets/tree_view.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:43Z
---

# Structural Widgets

> **WIP.** These widgets render today, but several have known gaps. `ListView`
> does not yet wire its column or row callbacks into the DOM. Treat their APIs
> as provisional. `TabHeader` and `TreeView` are usable as documented.

The structural widgets are containers. They take other widgets or DOMs as
content and lay them out in tabbed, hierarchical, or tabular forms.

## Tabs

`TabHeader` is the clickable bar of labels. Compose it with your own content
DOM in a column flexbox so the header stays a fixed height and the content
fills the rest.

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_tab(
    _d: RefAny, _i: CallbackInfo, _s: TabHeaderState,
) -> Update {
    Update::RefreshDom
}

let labels = StringVec::from_vec(vec!["General".into(), "Advanced".into()]);
let dom = Dom::create_div()
    .with_child(
        TabHeader::create(labels)
            .with_active_tab(0)
            .with_on_click(RefAny::new(()), on_tab)
            .dom()
    )
    .with_child(Dom::create_text("Settings go here."));
```

`TabHeader` keeps `active_tab: usize` and emits `TabHeaderState { active_tab }`
on click. The active state is driven externally; the widget does not toggle
itself. Your callback rebuilds the DOM with a new `active_tab`.

## TreeView

`TreeView` renders a hierarchy where each node has a label, an optional list
of children, and per-node expanded/selected flags.

```rust,ignore
pub struct TreeViewNode {
    pub label:       AzString,
    pub children:    TreeViewNodeVec,
    pub is_expanded: bool,
    pub is_selected: bool,
}
```

Build the data with the chained `with_*` builders and convert in one call.

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_click(_d: RefAny, _i: CallbackInfo, _idx: usize) -> Update {
    Update::RefreshDom
}

let root = TreeViewNode::new("Project".into())
    .with_expanded(true)
    .with_child(TreeViewNode::new("src".into()).with_expanded(true)
        .with_child(TreeViewNode::new("main.rs".into()))
        .with_child(TreeViewNode::new("lib.rs".into())))
    .with_child(TreeViewNode::new("Cargo.toml".into()));

let dom = TreeView::new(root)
    .with_on_node_click(RefAny::new(()), on_click)
    .dom();
```rust

The `usize` argument to the click callback is the depth-first preorder index
of the clicked node, not a node ID. Map it back to your data by walking the
tree in the same order.

`is_expanded` and `is_selected` are read-only outputs of the widget. To toggle,
store the tree in your `RefAny`, mutate it inside the callback, and return
`Update::RefreshDom`.

## ListView

`ListView` renders a tabular list with column headers and rows. The data
shape:

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

The widget supports lazy-loading. Pass an explicit `content_height` to size the
scroll region for unloaded rows, and use `on_lazy_load_scroll` to fetch more
data as the user scrolls.

> **Known limitation.** The current `dom()` does not yet attach
> `on_column_click`, `on_row_click`, `on_lazy_load_scroll`, or
> `column_context_menu` to event filters in the produced DOM. Configure them
> now to keep your call sites stable, but plan to handle clicks via external
> event filters until the widget wires them through.

## Ribbon

`Ribbon` is an Office-style ribbon: a tab bar where each tab contains one or
more titled `RibbonSection`s. Sections separate themselves with vertical
dividers and stack their title below the content.

```rust,no_run
# use azul::prelude::*;
let home = RibbonTab::new("Home".into())
    .with_section(RibbonSection::new("Clipboard".into(), Dom::create_div()))
    .with_section(RibbonSection::new("Font".into(), Dom::create_div()));

let view = RibbonTab::new("View".into())
    .with_section(RibbonSection::new("Zoom".into(), Dom::create_div()));

let dom = Ribbon::new(RibbonTabVec::from_vec(vec![home, view]))
    .dom();
```

`set_active_tab(idx)` clamps to the last valid index. The
`with_on_tab_click(data, fn)` callback receives the clicked tab index. Like
tabs, the widget is externally driven; rebuild your DOM with the new active
tab to switch.

## Picking the right container

- For flat tabs of independent panels, use `TabHeader` plus a content div.
- For ribbon-style tabs with named subsections, use `Ribbon`.
- For a hierarchical sidebar, use `TreeView`.
- For a tabular data grid (columns + rows), use `ListView`.

For ad-hoc layouts, plain `Dom::create_div` with flex CSS is almost always
shorter than wiring a widget. Reach for these widgets when you want the
platform-native look or the widget's specific event surface.

## Coming Up Next

- [Input Widgets](inputs.md) — Text fields, checkboxes, radios, sliders, dropdowns
- [Layout](../layout.md) — Overview of the layout solver
- [Events](../events.md) — Callbacks, event filters, and how state triggers relayout
