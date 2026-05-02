---
slug: dom
title: The DOM
language: en
canonical_slug: dom
audience: external
maturity: mature
guide_order: 30
topic_only: false
short_desc: Building UIs from the Dom tree — node types, hierarchy, and the layout callback contract.
prerequisites: [understanding-refany]
tracked_files:
  - core/src/dom.rs
  - core/src/xml.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# The DOM

A `Dom` is a tree of `NodeData`. Your `LayoutCallback` returns one each time the
framework needs a fresh view of the application. Construction is the only thing
you do with it — it is write-only from your side; the framework consumes it,
runs the cascade, and produces a `StyledDom` for layout.

```rust,no_run
# use azul::prelude::*;
let dom: Dom = Dom::create_body()
    .with_child(Dom::h1("Hello"))
    .with_child(Dom::p_with_text("A paragraph."));
```

## `Dom` and `NodeData`

`Dom` is a recursive node value defined in
[`core/src/dom.rs:3248`](../../core/src/dom.rs):

```rust,ignore
pub struct Dom {
    pub root: NodeData,
    pub children: DomVec,
    pub css: azul_css::css::CssVec,
    pub estimated_total_children: usize,
}
```

`NodeData` (`core/src/dom.rs:1511`) holds everything about a single node: its
`NodeType` (the HTML tag it represents), its callbacks, its inline CSS
properties, packed flags (tab index, contenteditable, anonymous), optional
accessibility info, and a boxed `extra` field for less-common state
(attributes, dataset, virtual-view payload, menus, key, SVG data,
component-origin marker).

The two roles split cleanly: `NodeData` describes the *node*; `Dom` is a
*subtree* whose root is a `NodeData`. Every builder method on `Dom` is a
shorthand that delegates to the same method on `self.root`.

## Node constructors

Each HTML element has a `Dom::create_<tag>()` constructor. Most are `const fn`
and allocate nothing until you add a child:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div();
let _ = Dom::create_section();
let _ = Dom::create_article();
let _ = Dom::create_main();
let _ = Dom::create_nav();
let _ = Dom::create_header();
let _ = Dom::create_footer();
```

Text-bearing constructors take a string and wrap a `Text` child inside the
element:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::h1("Title");
let _ = Dom::h2("Section");
let _ = Dom::p_with_text("A paragraph.");
let _ = Dom::span("inline");
let _ = Dom::strong("important");
let _ = Dom::code("println!()");
let _ = Dom::create_text("standalone text node");
```

A11y-aware constructors require a `SmallAriaInfo` so the produced node has an
accessible name; the `_no_a11y` variants are deliberate escape hatches:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_button("Save", SmallAriaInfo::label("Save document"));
let _ = Dom::create_button_no_a11y("OK".into()); // skips accessibility info
let _ = Dom::create_a("https://example.com", "Example", SmallAriaInfo::default());
let _ = Dom::create_input("text", "username", "Username", SmallAriaInfo::default());
```

The full set of `NodeType` variants — every HTML element plus the SVG subset
plus `Text`, `Image`, `Icon`, and `VirtualView` — is in `core/src/dom.rs:239`.

## Adding children

`with_child` appends one subtree, `with_children` replaces the child vec, and
`Dom` implements `FromIterator<Dom>` so you can collect into a parent:

```rust,no_run
# use azul::prelude::*;
let list: Dom = (0..3)
    .map(|i| Dom::li_with_text(format!("Item {}", i)))
    .collect::<Dom>();
// `list` is a div whose children are the three <li> nodes.

let nav = Dom::create_nav()
    .with_child(Dom::create_a("/", "Home", SmallAriaInfo::default()))
    .with_child(Dom::create_a("/about", "About", SmallAriaInfo::default()));
```

`estimated_total_children` is updated by every `add_child` call; the framework
uses it to pre-size the flat node arena when it converts the `Dom` to its
internal `CompactDom`. There is nothing you need to do — but if you mutate
`children` directly, call `fixup_children_estimated()` before returning.

## IDs and classes

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div()
    .with_id("sidebar".into())
    .with_class("panel".into())
    .with_class("scrollable".into());
```

Internally, IDs and classes are stored as `AttributeType::Id` and
`AttributeType::Class` entries in the node's attribute list. They drive CSS
selector matching: a stylesheet rule `.panel { ... }` selects every node whose
attribute list contains `Class("panel")`.

Multiple IDs on one node are legal but unusual; the cascade treats them as a
disjunction (any matches).

## HTML attributes

`AttributeType` (`core/src/dom.rs:1257`) is a strongly-typed enum of every
attribute the framework recognises — `Href`, `Src`, `Alt`, `AriaLabel`,
`Required`, `MaxLength(i32)`, `ContentEditable(bool)`, plus a `Custom`
fallback for arbitrary `name="value"` pairs.

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div()
    .with_attribute(AttributeType::AriaLabel("notification banner".into()))
    .with_attribute(AttributeType::Lang("en".into()))
    .with_attribute(AttributeType::Hidden);
```

Attributes are not the same as inline CSS. They feed accessibility, attribute
selectors (`[lang="en"]`), and serialization to HTML/XML.

## Inline CSS

`with_css` parses a CSS string into the node's `css_props` vector and supports
the full inline-style dialect — bare properties, pseudo-state blocks, and
`@os` / `@theme` conditional rules:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    color: blue;
    font-size: 14px;
    :hover { color: red; }
    :active { color: green; }
    @os linux { font-size: 13px; }
    @theme dark { color: white; background: #222; }
");
```

Properties are evaluated last-wins at cascade time. Each entry is a
`CssPropertyWithConditions` (in `azul_css::dynamic_selector`); the runtime
evaluates conditions per frame, so `@os` and `@theme` adapt without a re-layout.

## Stylesheets attached to a subtree

`Dom::style(css)` pushes a parsed `Css` onto the subtree's `css` vec. Multiple
stylesheets stack in push order, with later ones overriding earlier ones:

```rust,no_run
# use azul::prelude::*;
# use azul::css::Css;
let theme_css = Css::from_string("body { font-family: sans-serif; }".into());
let component_css = Css::from_string(".panel { padding: 8px; }".into());

let _ = Dom::create_body()
    .style(theme_css)
    .style(component_css)
    .with_child(Dom::create_div().with_class("panel"));
```

`with_css` is for one node; `style` scopes a whole stylesheet to the subtree.
Components typically ship a `style()` call on their root so callers don't have
to wire CSS by hand — see [Components](dom/components.md).

## Callbacks

`with_callback(filter, data, callback)` attaches a `RefAny` and a function
pointer to fire when the matching event reaches this node:

```rust,no_run
# use azul::prelude::*;
struct Counter { value: i64 }

extern "C" fn on_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    let mut c = match data.downcast_mut::<Counter>() { Some(c) => c, None => return Update::DoNothing };
    c.value += 1;
    Update::RefreshDom
}

# fn build(state: RefAny) -> Dom {
Dom::create_button_no_a11y("+1".into())
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), state, on_click)
# }
```

`with_callback` takes a fully-qualified `EventFilter`. The `On` shorthand
(`core/src/dom.rs:1124`) converts to one via `From<On>` —
`On::MouseUp.into()` is the same as `EventFilter::Hover(HoverEventFilter::MouseUp)`.
Event filtering, propagation order, and the `CallbackInfo` API are covered
in [Events and Input](events.md).

## Stable keys for reconciliation

`with_key(k)` stamps a node with a hashable key so the diffing pass can match
it to the prior frame's node even if its position in the parent's child vec
changed. Without keys, reconciliation falls back to a structural-hash match,
which is correct but loses cursor position, focus, and dataset state when
sibling order changes.

```rust,no_run
# use azul::prelude::*;
# struct Item { id: u64, label: String }
# fn render(items: &[Item]) -> Dom {
items.iter()
    .map(|item| {
        Dom::create_li()
            .with_key(item.id)
            .with_child(Dom::create_text(item.label.as_str()))
    })
    .collect::<Dom>()
# }
```

Use a key whenever a list can reorder, insert at the front, or remove from
the middle. For static lists the key is unnecessary.

## Datasets

`with_dataset(OptionRefAny)` attaches arbitrary user data to a node, queryable
in callbacks via `CallbackInfo::get_dataset`. The dataset is what makes the
backreference pattern work: the dataset on a node carries the `RefAny` chain
back to the higher-level component that owns the logic.

```rust,no_run
# use azul::prelude::*;
# struct PortDataset { port_id: u32, parent: RefAny }
# fn make_port(node: RefAny, port_id: u32) -> Dom {
let dataset = RefAny::new(PortDataset { port_id, parent: node });
Dom::create_div()
    .with_class("port")
    .with_dataset(OptionRefAny::Some(dataset))
# }
```

For state that must survive subtree replacement (video decoder, GL texture,
network connection), pair the dataset with `with_merge_callback`: the
framework calls it with the old and new `RefAny`s during reconciliation so
your component can move heavy resources across.

## `VirtualView` — embedding a sub-DOM

A `VirtualView` is azul's iframe-equivalent: a node whose contents are
produced by a separate callback that runs *only* when the framework needs the
contents — on first paint, on bounds change, or when scrolling crosses an
edge. It is the mechanism for infinite lists, lazy panels, and embedded
component roots that have their own data.

```rust,no_run
# use azul::prelude::*;
struct ListData { items: Vec<String> }

extern "C" fn render_list(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let d = match data.downcast_ref::<ListData>() { Some(d) => d, None => return VirtualViewReturn::default() };
    let dom: Dom = d.items.iter()
        .map(|s| Dom::li_with_text(s.clone()))
        .collect();
    let row_h = 24.0_f32;
    VirtualViewReturn::with_dom(
        dom,
        LogicalSize::new(info.bounds.logical_size.width, row_h * d.items.len() as f32),
        LogicalPosition::zero(),
        LogicalSize::new(info.bounds.logical_size.width, row_h * d.items.len() as f32),
        LogicalPosition::zero(),
    )
}

let list_state = RefAny::new(ListData { items: vec!["A".into(), "B".into()] });
let _ = Dom::create_virtual_view(list_state, VirtualViewCallback::create(render_list));
```

The callback receives bounds, scroll offset, scroll size, the system-font
cache, and a `VirtualViewCallbackReason` explaining why it was called
(`InitialRender`, `DomRecreated`, `BoundsExpanded`, `EdgeScrolled(_)`,
`ScrollBeyondContent` — `core/src/callbacks.rs:181`). Use the reason to skip
work when the call is just a parent re-render.

## Loading XML and XHTML

`Dom::from_xml(s)` is currently a stub — it returns a text node with the
input length. Real XML/XHTML parsing lives in `core/src/xml.rs` and
`layout/src/xml/mod.rs`. The two-step pipeline:

```rust,no_run
# use azul::prelude::*;
# use azul_core::xml::{Xml, ComponentMap, str_to_dom_unstyled};
# use azul_layout::xml::parse_xml;
# let xml_text = "";
let parsed: Xml = parse_xml(xml_text).unwrap();
let components = ComponentMap::default();
let dom: Dom = str_to_dom_unstyled(parsed.root.as_ref(), &components).unwrap();
```

`str_to_dom_unstyled` (`core/src/xml.rs:4314`) walks
`<html><head><style>…</style></head><body>…</body>`, parses the `<style>`
block into a `Css` and attaches it to the returned `Dom`'s `css` field, then
converts the body. The cascade applies on the next layout pass like any
other DOM. `str_to_dom` is a fully-styled variant that returns a `StyledDom`
directly, useful for one-shot rendering outside a window.

`ComponentMap` is the registry of XML-defined components — see
[Components](dom/components.md) for the component model.

## `FastDom` — the arena form

`FastDom` (`core/src/dom.rs:3291`) is a flat-arena DOM: two parallel vectors
(`node_hierarchy`, `node_data`) plus a `CssWithNodeIdVec` of stylesheets
keyed by node id. It is what the XML parser produces internally and what
`StyledDom::create_from_fast_dom` consumes; the tree-shaped `Dom` is converted
to it as part of the cascade pass.

You almost never construct `FastDom` by hand. Reach for it only when you are
building a DOM from a flat source — XML, JSON, a wire format — and the
intermediate tree of `Dom` values would be wasteful. `FastDom::into_dom()`
converts back to the tree form when needed.

## Where to read the source

- `core/src/dom.rs:1511` — `NodeData` definition
- `core/src/dom.rs:3248` — `Dom` definition
- `core/src/dom.rs:3291` — `FastDom` definition
- `core/src/dom.rs:239` — `NodeType` variants
- `core/src/dom.rs:1257` — `AttributeType` variants
- `core/src/dom.rs:1124` — `On` event-shorthand enum
- `core/src/xml.rs:4314` — `str_to_dom_unstyled` entry point
