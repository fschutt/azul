---
slug: dom
title: Document Object Model
language: en
canonical_slug: dom
audience: external
maturity: mature
guide_order: 30
topic_only: false
short_desc: The Dom tree - node types, hierarchy, and CSS
prerequisites: [architecture/understanding-refany]
tracked_files:
  - core/src/dom.rs
  - core/src/styled_dom.rs
  - core/src/compact_cache_builder.rs
  - core/src/xml.rs
  - css/src/compact_cache.rs
  - dll/src/desktop/shell2/common/debug_server.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Document Object Model

Azul's DOM is data-oriented. Internally it lives as parallel arrays. There's
a `NodeHierarchyItemVec` (each entry is a `NodeHierarchyItem` with `parent`,
`previous_sibling`, `next_sibling`, and `last_child` indices) and a parallel
`NodeDataVec` of `NodeData`. That's the format the layout engine actually
consumes.

The DOM is also frozen the moment you return it from `layout()`. There's no
`appendChild`. There's no `setAttribute`. There are no mutation observers.
State change goes through the next `layout()` call. A callback returns
`Update::RefreshDom`, the framework calls your layout function again, you
build a fresh `Dom` from your application data, and the previous tree is
diffed against the new one. See
[Reconciliation, Diffing, and Lazy Paint](dom/reconciliation.md).

You build the tree in one of two shapes. `Dom` is a recursive value: a
`NodeData` root plus a `DomVec` of children. It's easy to write by hand.
`FastDom` is the flat-arena form, the same parallel arrays the framework
uses internally. It skips the tree-to-arena conversion. The XML parser
emits this form.

State that has to survive a tree rebuild (a video decoder, a GL texture,
the cursor inside a focused input) doesn't live in the tree. It hangs off
the node as a dataset. See [Datasets](dom/datasets.md) and
[Merge Callbacks](dom/merge-callbacks.md).

```rust,no_run
# use azul::prelude::*;
let dom: Dom = Dom::create_body()
    .with_child(Dom::h1("Hello"))
    .with_child(Dom::p_with_text("A paragraph."));
```

This page covers the type definitions, both build shapes, how CSS is
attached, when it actually applies, XML loading, clipping, and the live
debugger. Reusable fragments are covered separately in
[Components](dom/components.md).

## `Dom` and `NodeData`

`Dom` (in `core/src/dom.rs`) is the recursive form:

```rust,ignore
pub struct Dom {
    pub root: NodeData,
    pub children: DomVec,
    pub css: azul_css::css::CssVec,
    pub estimated_total_children: usize,
}
```

A `Dom` is a subtree. Its root is a `NodeData`. `NodeData` (also in
`core/src/dom.rs`) is the per-node payload:

- `node_type: NodeType`. The HTML tag this node represents.
- `callbacks: CoreCallbackDataVec`. Event handlers attached to the node.
- `css_props: CssPropertyWithConditionsVec`. Inline CSS, including
  conditional rules like `:hover` and `@theme dark`.
- `flags: NodeFlags`. Packed bits for tab index, contenteditable, anonymous.
- `accessibility: Option<Box<AccessibilityInfo>>`. ATK/MSAA payload, optional.
- `extra: Option<Box<NodeDataExt>>`. Less-common state, boxed so the common
  case stays small.

`NodeDataExt` is where attributes, dataset, virtual-view payload, menus,
SVG data, key, merge callback, and component-origin marker all live. About
95% of nodes don't have any of these, so the box stays unallocated.

Every builder method on `Dom` (like `with_class` or `with_callback`) is a
shorthand that delegates to the same method on `self.root`.

## `FastDom`: the flat-arena form

`FastDom` (also in `core/src/dom.rs`) holds the arena form directly:

```rust,ignore
pub struct FastDom {
    pub node_hierarchy: NodeHierarchyItemVec,
    pub node_data: NodeDataVec,
    pub css: CssWithNodeIdVec,
}
```

The two vectors are parallel. Index `i` in `node_hierarchy` describes the
parent and sibling links for the node whose data is at index `i` in
`node_data`. The `css` field holds stylesheets keyed by node id, so a
`<style>` block found inside `<head>` can be associated with the right
scope.

Use `Dom` for hand-written UI. Use `FastDom` when the source is already
flat. XML and JSON inputs are typical cases. Going via the recursive `Dom`
would just allocate a tree that immediately gets flattened.
`StyledDom::create_from_fast_dom()` (in `core/src/styled_dom.rs`) consumes
the arena directly. `FastDom::into_dom()` goes the other way if you need
to splice the result into a hand-written tree.

## Node constructors

Each HTML element has a `Dom::create_<tag>()` constructor. Most are `const
fn` and don't allocate until you add a child:

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

Text-bearing constructors take a string and wrap a `Text` child inside
the element:

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

Interactive constructors require a `SmallAriaInfo` so the resulting node
has an accessible name. The `_no_a11y` variants exist as deliberate escape
hatches:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_button("Save", SmallAriaInfo::label("Save document"));
let _ = Dom::create_button_no_a11y("OK".into());
let _ = Dom::create_a("https://example.com", "Example", SmallAriaInfo::default());
let _ = Dom::create_input("text", "username", "Username", SmallAriaInfo::default());
```

`NodeType` (in `core/src/dom.rs`) lists every variant. The set covers all
HTML elements plus the SVG subset plus four leaf types: `Text`, `Image`,
`Icon`, and `VirtualView`.

## Adding children

Three ways to attach children:

```rust,no_run
# use azul::prelude::*;
// 1. One at a time. Each call grows .children by one.
let a = Dom::create_div()
    .with_child(Dom::h2("Title"))
    .with_child(Dom::p_with_text("Body"));

// 2. Replace the child vec wholesale.
let kids: DomVec = vec![Dom::span("x"), Dom::span("y"), Dom::span("z")].into();
let b = Dom::create_div().with_children(kids);

// 3. Collect from an iterator into a parent.
let c: Dom = (0..3).map(|i| Dom::li_with_text(format!("Item {}", i))).collect();
// Produces a NodeType::Div containing three <li> children.
```

`with_child` calls `add_child`, which pushes onto the underlying `Vec` and
updates `estimated_total_children`. That's amortised O(1) per call.
`with_children(DomVec)` is one allocation total. For really wide trees
built from a flat data source, build a `FastDom` instead. One big
allocation beats N small ones.

`estimated_total_children` is maintained by every `add_child` and
`set_children` call. The framework reads it to pre-size the flat arena
during conversion. If you mutate `children` directly, call
`fixup_children_estimated()` before returning.

## IDs, classes, attributes

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div()
    .with_id("sidebar".into())
    .with_class("panel".into())
    .with_class("scrollable".into())
    .with_attribute(AttributeType::AriaLabel("notification banner".into()))
    .with_attribute(AttributeType::Lang("en".into()));
```

IDs and classes aren't separate fields. They're stored as `AttributeType::Id`
and `AttributeType::Class` entries in the node's attribute list. The selector
`.panel { ... }` matches every node whose attribute list contains
`Class("panel")`.

`AttributeType` (in `core/src/dom.rs`) is a strongly-typed enum: `Href`,
`Src`, `Alt`, `AriaLabel`, `Required`, `MaxLength(i32)`,
`ContentEditable(bool)`, and so on. There's a `Custom` fallback for
arbitrary `name="value"` pairs. Attributes aren't inline CSS. They feed
accessibility, attribute selectors like `[lang="en"]`, and HTML/XML
serialization.

## Clipping a node

Three mechanisms, one for each shape of clip region:

- `with_clip_mask(ImageMask)` takes a raster alpha mask. Use it for
  irregular shapes that already exist as image data.
- `with_svg_clip_path(SvgMultiPolygon)` takes a vector clip path. Same
  geometry the SVG renderer uses for `<clipPath>`.
- `with_css("clip-path: ...;")` parses the CSS property into the node's
  inline-CSS list. Applied during the cascade.

```rust,no_run
# use azul::prelude::*;
# use azul::svg::SvgMultiPolygon;
# fn build(mask: ImageMask, polygon: SvgMultiPolygon) -> Dom {
let raster = Dom::create_image(ImageRef::null_image(0, 0, RawImageFormat::R8, U8VecRef::from(&[][..])))
    .with_clip_mask(mask);

let vector = Dom::create_div()
    .with_svg_clip_path(polygon);

let css_form = Dom::create_div()
    .with_css("clip-path: circle(40px at 50% 50%);");

# Dom::create_body().with_child(raster).with_child(vector).with_child(css_form)
# }
```

All three end up at the same place inside the renderer. `with_clip_mask`
stores an `ImageMask` on the SVG-data slot. `with_svg_clip_path` stores
an `SvgNodeData::Path` on the same slot. CSS `clip-path` parses to a
`ClipPathValue` during the cascade. The display-list builder reads the
slot when it builds the clip stack for the subtree, so a `clip-path` set
on a parent applies to every descendant.

`with_svg_clip_path` is the seam to the SVG side. Parsed `<svg>` content
arrives as `SvgMultiPolygon` values (in `core/src/svg.rs`), and the same
value can drive either the SVG renderer or a regular DOM node's clip
region.

## CSS: per-node and per-subtree

There are two attachment points with two different semantics.

```rust,no_run
# use azul::prelude::*;
# use azul::css::Css;
// 1. with_css(...) parses a CSS string into the node's inline-property vec.
let item = Dom::create_div().with_css("
    color: blue;
    font-size: 14px;
    :hover { color: red; }
    @theme dark { color: white; background: #222; }
");

// 2. style(Css) attaches a parsed stylesheet to the subtree.
let theme   = Css::from_string("body { font-family: sans-serif; }".into());
let widgets = Css::from_string(".panel { padding: 8px; }".into());
let body = Dom::create_body()
    .style(theme)
    .style(widgets)
    .with_child(Dom::create_div().with_class("panel".into()));
```

`with_css` ends up as a `CssPropertyWithConditionsVec` on the node itself,
including the `:hover`, `:active`, `@os`, and `@theme` blocks. Conditions
are evaluated per frame, so `@theme dark { ... }` adapts without a
re-layout.

`style(Css)` attaches a parsed stylesheet to the subtree. Multiple
`style()` calls stack in push order. Later entries override earlier ones.
This isn't strict Shadow-DOM scoping. The cascade flattens everything
before matching, so descendant selectors still cross subtree boundaries.
For strict scoping, hand-write selectors that include the component's
marker class.

Components typically ship a `style()` call on their root so callers don't
have to wire CSS by hand. See [Components](dom/components.md).

## When does CSS apply?

Not while your `LayoutCallback` is running. The `Dom` you build carries
CSS as opaque state:

- Per-node: a `CssPropertyWithConditionsVec` (parsed but not cascaded).
- Per-subtree: a `CssVec` of parsed-but-unmerged stylesheets.

The cascade runs once, after the layout callback returns, in
`StyledDom::create_from_dom()` (in `core/src/styled_dom.rs`). The steps
are:

1. Collect every `Dom::style(...)` `Css` from the recursive tree.
2. Strip the now-collected CSS from the nodes so it doesn't apply twice.
3. Flatten the recursive `Dom` into the parallel-array form.
4. Merge the collected stylesheets in push order.
5. Match selectors against the flattened tree, fold in `apply_ua_css` and
   `compute_inherited_values`.
6. Build the compact cache via `build_compact_cache` (in
   `core/src/compact_cache_builder.rs`).

The compact cache is the "CSS compression" pass. A naive cascade output
would be a `BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>`. The
compact cache (in `css/src/compact_cache.rs`) re-encodes the layout-hot
subset into packed tiers:

- Tier 1: `Vec<u64>`. 21 enum properties bit-packed into 8 B per node.
  Includes `display`, `position`, `float`, `overflow_x/y`, `flex_direction`,
  `justify_content`, `align_items`, `font_weight`, `text_align`, and others.
- Tier 2 hot: `Vec<CompactNodeProps>`. Layout-critical numeric dimensions
  in 68 B per node. `width`, `height`, `padding`, `margin`, `border`,
  `flex_basis`, and so on.
- Tier 2 cold: `Vec<CompactNodePropsCold>`. Paint-only properties in 28 B
  per node. Color, opacity, others.
- Tier 2b: `Vec<CompactTextProps>`. Text and IFC properties in 24 B per
  node.

Layout reads the compact cache directly. No map lookups, no enum-tag
dispatch on hot paths. Less common properties (background, box-shadow,
transform) stay in the slow cascade path. The layout engine doesn't need
them.

For you, the API consumer, this means CSS operations inside `layout()`
are cheap. Each one is one parse and one push. The expensive work
(selector matching, inheritance, compact cache) runs once after you
return.

## Loading XML and XHTML

XML/XHTML parsing has two entry points. `str_to_dom_unstyled` (in
`core/src/xml.rs`) returns a `Dom` you can return from `layout()`:

```rust,no_run
# use azul::prelude::*;
# use azul_core::xml::{Xml, ComponentMap, str_to_dom_unstyled};
# use azul_layout::xml::parse_xml;
# let xml_text = "";
let parsed: Xml = parse_xml(xml_text).unwrap();
let components = ComponentMap::default();
let dom: Dom = str_to_dom_unstyled(parsed.root.as_ref(), &components).unwrap();
```

`str_to_dom` is the related entry point that goes through `FastDom`
internally and returns a fully-styled `StyledDom`. Use it for one-shot
rendering outside a window.

The XML parser walks `<html><head><style>...</style></head><body>...</body>`,
parses each `<style>` block into a `Css`, and attaches it scoped to the
node it was found inside. The cascade runs on the next layout pass like
any other DOM.

`<svg>` content embedded in XHTML flows through the same path. The
parser recognises SVG tags. The resulting nodes carry `SvgNodeData` on
their extra-state slot. A `clip-path` attribute on an XHTML element
resolves the same way a CSS `clip-path:` property would.

`ComponentMap` is the registry of XML-defined components. See
[Components](dom/components.md#component-packs) for how the framework
looks up `<card title="..."/>` against a registered library.

## Callbacks, keys, datasets, virtual views

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

`with_callback(filter, data, callback)` attaches a `RefAny` and a function
pointer. The handler fires when the matching event reaches the node. The
`On` enum (in `core/src/dom.rs`) is a shorthand: `On::MouseUp.into()` is
the same as `EventFilter::Hover(HoverEventFilter::MouseUp)`. Event
filtering, propagation order, and the `CallbackInfo` API are covered in
[Events and Input](events.md).

`with_key(k)` stamps a node with a hashable key so reconciliation can
match it to the prior frame's node when sibling order changes. Without
keys, diffing falls back to a structural-hash match. That's still
correct, but it loses cursor position, focus, and dataset state when
items reorder.

`with_dataset(OptionRefAny)` attaches arbitrary user data to a node.
Callbacks read it via `CallbackInfo::get_dataset`. The dataset is the
canonical place for UI-layer state. The cursor inside an input. The
expansion flag on a tree-view row. A marker struct that says "I am the
save button" so a generic callback can dispatch. See
[Datasets](dom/datasets.md) for the navigation patterns.

For state that must survive a subtree rebuild, pair the dataset with
`with_merge_callback`. The framework calls it with the old and new
`RefAny` values during reconciliation. Heavy resources can move from
the old node to the new one. See [Merge Callbacks](dom/merge-callbacks.md)
for a worked FFmpeg example.

A `VirtualView` is a node whose contents come from a separate callback
that runs only when the framework needs them. Use it for infinite lists,
lazy panels, and embedded sub-DOMs that own their own scroll math. The
callback receives a `VirtualViewCallbackReason` (`InitialRender`,
`DomRecreated`, `BoundsExpanded`, `EdgeScrolled(_)`,
`ScrollBeyondContent`). Use the reason to skip work when the call is
just a parent re-render. See [Virtual Views](dom/virtual-views.md) for
the rendered-vs-virtual coordinate model and a virtualised-table
walkthrough.

## Inspecting a live tree: `AZ_DEBUG`

When `AZ_DEBUG=<port>` is set, `App::create` starts an HTTP debug server
on that port (defined in
`dll/src/desktop/shell2/common/debug_server.rs`). It accepts JSON
commands and returns JSON responses, all serialised on the timer
callback. The inspector sees the same tree the renderer is about to
draw.

```bash
AZ_DEBUG=8765 cargo run --bin my_app

# Synchronous queries:
curl -X POST http://localhost:8765/ -d '{"type":"get_state"}'
curl -X POST http://localhost:8765/ -d '{"type":"get_dom_tree"}'
curl -X POST http://localhost:8765/ -d '{"type":"get_node_hierarchy"}'
curl -X POST http://localhost:8765/ -d '{"type":"get_node_css_properties", "selector":".panel"}'
curl -X POST http://localhost:8765/ -d '{"type":"get_layout_tree"}'
curl -X POST http://localhost:8765/ -d '{"type":"get_display_list"}'

# Synthesised events (dispatched into the same callback path real input uses):
curl -X POST http://localhost:8765/ -d '{"type":"click", "selector":".button-primary"}'
curl -X POST http://localhost:8765/ -d '{"type":"text_input", "text":"hello"}'
```

The `get_node_hierarchy` response carries a `component` field for each
node. It's populated from the node's `ComponentOrigin` (in
`core/src/dom.rs`). The inspector uses it to draw a Component Tree
alongside the DOM Tree:

```json
{
  "index": 17, "node_type": "div", "tag": "card",
  "id": null, "classes": ["card", "card--info"],
  "parent": 12, "children": [18, 19, 20],
  "component": {
    "component_id": "shadcn:card",
    "data_model": { "title": "First", "body": "alpha" }
  },
  "rect": { "x": 16.0, "y": 16.0, "width": 320.0, "height": 88.0 },
  "events": ["MouseUp"], "tab_index": 0
}
```

Pick a node in the DOM Tree, get the component that produced it, and (if
its render function lives in a registered library) jump back to the
source. See [Components](dom/components.md#component-packs) for how a
library wires its components into the registry.

## Where to read the source

- `core/src/dom.rs`: `Dom`, `FastDom`, `NodeData`, `NodeDataExt`,
  `NodeType`, `AttributeType`, `On`, `ComponentOrigin`, the builder
  methods.
- `core/src/styled_dom.rs`: `NodeHierarchyItem`, `create_from_dom`,
  `create_from_fast_dom`.
- `core/src/compact_cache_builder.rs`: `build_compact_cache`.
- `css/src/compact_cache.rs`: `CompactNodeProps`, `CompactNodePropsCold`,
  `CompactTextProps`, the three-tier numeric encoding.
- `core/src/xml.rs`: `str_to_dom_unstyled`, `str_to_dom`,
  `render_dom_from_body_node_fast`.
- `dll/src/desktop/shell2/common/debug_server.rs`: the `AZ_DEBUG` HTTP
  server.
