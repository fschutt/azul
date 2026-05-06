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

A `Dom` is a tree of `NodeData`. Your `LayoutCallback` returns one each time the
framework needs a fresh view of the application. Construction is the only
thing you do with it — it is write-only from your side; the framework consumes
it, runs the cascade, and produces a `StyledDom` for layout.

**The `Dom` is frozen the moment you return it from `layout()`.** Unlike the
browser DOM, there is no `appendChild` / `removeChild` / `setAttribute` once
the framework has the tree — no live nodes, no mutation observers, no
reflows triggered by JavaScript. State changes always go through the
*next* `layout()` call: a callback returns `Update::RefreshDom`, the
framework re-invokes the layout callback, you build a fresh `Dom` from
your application data, and the framework reconciles the new tree against
the previous one (see [Reconciliation, Diffing, and Lazy Paint](dom/reconciliation.md)).
Anything that needs to *survive* a tree rebuild — a video decoder, a GL
texture, the typing buffer of a focused input — sits on the **node** as a
dataset, not in the tree shape.

```rust,no_run
# use azul::prelude::*;
let dom: Dom = Dom::create_body()
    .with_child(Dom::h1("Hello"))
    .with_child(Dom::p_with_text("A paragraph."));
```

This page covers the shape of a `Dom`, the two ways to build one, how CSS
attaches and when it gets applied, clipping, XML loading, and what the
debugger sees on the other side. The component model — and how reusable
fragments hand themselves to the live debugger — is one level deeper, in
[Components and Component Packs](dom/components.md).

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

## Two shapes: tree-`Dom` vs flat-`FastDom`

The framework consumes nodes in flat-arena form (parallel arrays of hierarchy
and node data). There are two ways to get there:

- **`Dom`** — recursive tree. Easiest to build by hand because parents own
  their children directly. Must be flattened before the cascade runs.
- **`FastDom`** (`core/src/dom.rs:3291`) — flat arena up front. Two parallel
  vectors (`node_hierarchy: NodeHierarchyItemVec` and `node_data:
  NodeDataVec`) plus a `CssWithNodeIdVec` of stylesheets keyed by node id.
  Skips the tree → arena conversion entirely.

```rust,ignore
pub struct FastDom {
    pub node_hierarchy: NodeHierarchyItemVec,
    pub node_data: NodeDataVec,
    pub css: CssWithNodeIdVec,
}
```

Use `Dom` for hand-written UI; use `FastDom` when you are constructing a DOM
from a flat source — XML, JSON, a wire format — and the intermediate tree of
`Dom` values would just turn into garbage. The XML parser produces `FastDom`
directly; `StyledDom::create_from_fast_dom()`
([`core/src/styled_dom.rs:961`](../../core/src/styled_dom.rs)) consumes it
without a tree round-trip. `FastDom::into_dom()` goes the other way when you
need to splice the result into a hand-written tree.

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

## Adding children — and what it costs

Three ways to attach children, in increasing batch size:

```rust,no_run
# use azul::prelude::*;
// 1. Append one at a time. Each call grows .children by one.
let a = Dom::create_div()
    .with_child(Dom::h2("Title"))
    .with_child(Dom::p_with_text("Body"));

// 2. Replace the child vec wholesale with a pre-built DomVec.
let kids: DomVec = vec![Dom::span("x"), Dom::span("y"), Dom::span("z")].into();
let b = Dom::create_div().with_children(kids);

// 3. Collect from an iterator into a parent.
let c: Dom = (0..3).map(|i| Dom::li_with_text(format!("Item {}", i))).collect();
// Produces a `Dom` of NodeType::Div with three <li> children.
```

`with_child` (`core/src/dom.rs:4920`) calls `add_child`, which swaps
`self.children` out, pushes the child onto the underlying `Vec`, and swaps it
back. That's a single `Vec::push` per call — amortised O(1) but with the
realloc cost of growing the vector. For a fixed-size set of children,
**`with_children(DomVec)`** is one allocation total: build the `Vec`,
move it into a `DomVec`, hand the whole thing over.

For really wide trees (hundreds or thousands of nodes from a flat data
source), build the arena directly with `FastDom` instead of growing the tree
node by node — the difference is one big allocation versus N small ones, and
no tree-to-arena conversion at all.

`estimated_total_children` is updated by every `add_child` / `set_children`
call; the framework uses it to pre-size the flat node arena when the tree
gets converted. There is nothing you need to do — but if you mutate
`children` directly, call `fixup_children_estimated()` before returning.

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

IDs and classes are stored as `AttributeType::Id` and `AttributeType::Class`
entries in the node's attribute list and drive CSS selector matching — the
rule `.panel { ... }` selects every node whose attribute list contains
`Class("panel")`. Multiple IDs on one node are legal but unusual; the cascade
treats them as a disjunction.

`AttributeType` (`core/src/dom.rs:1257`) is a strongly-typed enum of every
attribute the framework recognises — `Href`, `Src`, `Alt`, `AriaLabel`,
`Required`, `MaxLength(i32)`, `ContentEditable(bool)`, plus a `Custom`
fallback for arbitrary `name="value"` pairs. Attributes are not the same as
inline CSS — they feed accessibility, attribute selectors (`[lang="en"]`),
and serialization to HTML/XML.

## Clipping a node

Three different mechanisms, each for a different shape of clip region:

- **`with_clip_mask(ImageMask)`** — a raster alpha mask. Use it for
  irregular shapes that already exist as image data: feathered cutouts,
  vignettes, badges with stamped logos.
- **`with_svg_clip_path(SvgMultiPolygon)`** — a vector clip path. The same
  geometry the SVG renderer uses for `<clipPath>` elements; integrates with
  the SVG node story below.
- **`with_css("clip-path: …;")`** — the CSS property, parsed into the
  node's inline-CSS list and applied during the cascade. The same
  `ClipPathValue` accepts CSS-shape syntax.

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

Inside the renderer all three roads meet: `with_clip_mask` stores an
`ImageMask` on the node's SVG-data slot; `with_svg_clip_path` stores a
`SvgNodeData::Path` on the same slot; CSS `clip-path` parses to a
`ClipPathValue` in the cascade. The display-list builder reads the slot when
it is preparing the clip stack for the subtree, so a `clip-path` set on a
parent applies to every descendant the same way a CSS scope would.

`with_svg_clip_path` is the seam to the SVG side: parsed `<svg>` content
arrives as `SvgMultiPolygon` values (see [`core/src/svg.rs`](../../core/src/svg.rs)),
and you can hand the same value either to the SVG renderer or to a regular
DOM node as its clip region.

## CSS on a `Dom` — per-node and per-subtree

Two attachment points, two semantics:

```rust,no_run
# use azul::prelude::*;
# use azul::css::Css;
// 1. with_css(...) parses a CSS string into the *node's* inline-property vec.
let item = Dom::create_div().with_css("
    color: blue;
    font-size: 14px;
    :hover { color: red; }
    @theme dark { color: white; background: #222; }
");

// 2. style(Css) attaches a parsed stylesheet to the *subtree*. Multiple
//    .style() calls stack last-wins.
let theme   = Css::from_string("body { font-family: sans-serif; }".into());
let widgets = Css::from_string(".panel { padding: 8px; }".into());
let body = Dom::create_body()
    .style(theme)
    .style(widgets)
    .with_child(Dom::create_div().with_class("panel".into()));
```

Per-node `with_css` ends up as a `CssPropertyWithConditions` vec on the node
itself, including the `:hover` / `:active` / `@os` / `@theme` blocks —
conditions evaluate per frame, so an `@theme dark { … }` adapts without a
re-layout.

Per-subtree `style(Css)` is the closer cousin of a Shadow DOM: the
stylesheet is *scoped* to the subtree in the sense that you typically attach
it to a component root and ship them together, and the cascade walks it
during the same single pass. It is not Shadow-DOM-strict: the rules still
match across the whole document tree, including descendant selectors that
cross the subtree boundary, because the cascade flattens everything before
matching. If you want strict scoping, hand-write selectors that include the
component's marker class.

Components typically ship a `style()` call on their root so callers don't
have to wire CSS by hand — see [Components](dom/components.md).

## When does CSS actually apply? Not until after layout()

A subtle but load-bearing point: while your `LayoutCallback` is running, no
CSS has been applied yet. The `Dom` you build carries CSS as **opaque
state**:

- per-node: a `Vec<CssPropertyWithConditions>` (already parsed but not
  cascaded against the tree)
- per-subtree: a `CssVec` — *parsed but unmerged stylesheets*

The cascade runs **once**, after the layout callback returns, in
`StyledDom::create_from_dom()` ([`core/src/styled_dom.rs:1169`](../../core/src/styled_dom.rs)):

1. **Collect** every `Dom::style(...)` Css from the recursive tree
2. **Strip** the now-collected CSS from the nodes (no double-apply)
3. **Flatten** the recursive `Dom` into a `CompactDom` (parallel arrays)
4. **Merge** the collected stylesheets in push order
5. **Cascade**: match selectors against the flattened tree, fold in
   `apply_ua_css` and `compute_inherited_values`
6. **Build the compact cache** —
   `CssPropertyCache::build_compact_cache(node_data, prev_font_hashes)`
   ([`core/src/compact_cache_builder.rs:35`](../../core/src/compact_cache_builder.rs))

That last step is the "CSS compression" pass. Instead of leaving cascaded
properties as a `BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>` —
which is what a naive cascade output would look like — the compact cache
([`css/src/compact_cache.rs`](../../css/src/compact_cache.rs)) re-encodes
the layout-hot subset of properties in three packed tiers:

- **Tier 1** — `Vec<u64>`: 21 enum properties bit-packed into 8 B per node
  (`display`, `position`, `float`, `overflow_x/y`, `flex_direction`,
  `justify_content`, `align_items`, `font_weight`, `text_align`, …)
- **Tier 2 hot** — `Vec<CompactNodeProps>`: layout-critical numeric
  dimensions in 68 B per node (`width`, `height`, `padding`, `margin`,
  `border`, `flex_basis`, …)
- **Tier 2 cold** — `Vec<CompactNodePropsCold>`: paint-only properties in
  28 B per node (color, opacity, …)
- **Tier 2b** — `Vec<CompactTextProps>`: text/IFC properties in 24 B per
  node

Layout reads the compact cache directly — no map lookups, no enum-tag
dispatch on hot paths. Less common properties (background, box-shadow,
transform) stay in the slow cascade path because the layout engine doesn't
need them.

The reason this matters for you, the API consumer, is that every CSS
operation you do inside `layout()` is "free" in the sense that it costs
exactly one parse and one push onto a `Vec<Css>`. The expensive parts —
selector matching, inheritance, the compact cache — happen once after you
return, and only on the deltas the framework decides need a recascade.

## Loading XML and XHTML

XML/XHTML parsing produces a `FastDom` directly (no recursive `Dom`
intermediate):

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
`<html><head><style>…</style></head><body>…</body>`, parses each `<style>`
block into a `Css` and attaches it to the `FastDom`'s `css:
CssWithNodeIdVec` keyed by the node it was found inside, then converts the
body. The cascade applies on the next layout pass like any other DOM.
`str_to_dom` is a fully-styled variant that returns a `StyledDom` directly,
useful for one-shot rendering outside a window.

`<svg>` content embedded in XHTML flows through the same path: the parser
recognises SVG tags, the resulting nodes carry `SvgNodeData` on their
extra-state slot, and a `clip-path` attribute on an XHTML element resolves
the same way a CSS `clip-path:` property would.

`ComponentMap` is the registry of XML-defined components — see the
component-packs section of [Components](dom/components.md#component-packs)
for how the framework looks up `<card title="…"/>` against a registered
library.

## Callbacks, keys, datasets, `VirtualView`

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
pointer to fire when the matching event reaches this node. The `On`
shorthand (`core/src/dom.rs:1124`) converts to one via `From<On>` —
`On::MouseUp.into()` is the same as
`EventFilter::Hover(HoverEventFilter::MouseUp)`. Event filtering,
propagation order, and the `CallbackInfo` API are covered in
[Events and Input](events.md).

`with_key(k)` stamps a node with a hashable key so reconciliation can match
it to the prior frame's node even when sibling order changed. Without keys,
diffing falls back to a structural-hash match — correct, but loses cursor
position, focus, and dataset state when items reorder.

`with_dataset(OptionRefAny)` attaches arbitrary user data to a node,
queryable in callbacks via `CallbackInfo::get_dataset`. The dataset is
the canonical place to keep *UI-layer* state — the cursor inside an
input, the expansion flag of a tree-view row, a marker struct that
identifies "I am the save button" so a generic callback can dispatch.
See [Datasets and Marker Structs](dom/datasets.md) for the navigation
patterns and the ephemeral-`RefAny` semantics.

For state that must *survive* a subtree replacement (a video decoder, a
GL texture, a websocket), pair the dataset with `with_merge_callback`:
the framework calls it with the old and new `RefAny`s during
reconciliation so heavy resources can move across. See
[Merge Callbacks](dom/merge-callbacks.md) for the full reconcile-style
pattern with a worked FFmpeg example.

A `VirtualView` is azul's iframe-equivalent: a single node whose
contents come from a separate callback that runs *only* when the
framework needs them. It is the mechanism for infinite lists, lazy
panels, and embedded sub-DOMs that own their own scroll math. The
callback receives a `VirtualViewCallbackReason` explaining why it was
called (`InitialRender`, `DomRecreated`, `BoundsExpanded`,
`EdgeScrolled(_)`, `ScrollBeyondContent`); use the reason to skip work
when the call is just a parent re-render. See
[Virtual Views](dom/virtual-views.md) for the rendered-vs-virtual
coordinate model and a virtualised-table walkthrough.

## Inspecting a live tree: the `AZ_DEBUG` server

When `AZ_DEBUG=<port>` is set, `App::create` starts an HTTP debug server on
that port (defined in
[`dll/src/desktop/shell2/common/debug_server.rs`](../../dll/src/desktop/shell2/common/debug_server.rs)).
It accepts JSON commands and returns JSON responses, all serialised by the
timer callback so the inspector sees the same tree the renderer is about to
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

The `get_node_hierarchy` response for each node carries a `component`
field — populated from the node's `ComponentOrigin`
([`core/src/dom.rs:1588`](../../core/src/dom.rs)) — which the inspector
uses to draw a Component Tree alongside the DOM Tree:

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

The inspector clicks-through: pick a node in the DOM Tree, get the
component that produced it, and (if its render function lives in a
registered library) jump back to the source. That round-trip is what makes
the live design-time tools work; see the component-packs section of
[Components](dom/components.md#component-packs) for how a library wires its
components into the registry.

## Where to read the source

- `core/src/dom.rs:1511` — `NodeData` definition
- `core/src/dom.rs:1588` — `ComponentOrigin` (debugger tag)
- `core/src/dom.rs:3248` — `Dom` definition
- `core/src/dom.rs:3291` — `FastDom` definition (flat arena)
- `core/src/dom.rs:239` — `NodeType` variants
- `core/src/dom.rs:1257` — `AttributeType` variants
- `core/src/dom.rs:1124` — `On` event-shorthand enum
- `core/src/dom.rs:5129` — `with_clip_mask` / `with_svg_clip_path`
- `core/src/styled_dom.rs:1169` — `create_from_dom` (collect → cascade → compact cache)
- `core/src/styled_dom.rs:961` — `create_from_fast_dom` (skip the tree round-trip)
- `core/src/compact_cache_builder.rs:35` — `build_compact_cache`
- `css/src/compact_cache.rs` — three-tier numeric encoding
- `core/src/xml.rs:4314` — `str_to_dom_unstyled` entry point
- `dll/src/desktop/shell2/common/debug_server.rs` — `AZ_DEBUG` server
