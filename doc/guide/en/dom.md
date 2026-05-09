---
slug: dom
title: Document Object Model
language: en
canonical_slug: dom
audience: external
maturity: mature
guide_order: 30
topic_only: false
short_desc: Node types, hierarchy, and CSS scoping
prerequisites: [architecture/understanding-refany]
tracked_files:
  - core/src/dom.rs
  - core/src/styled_dom.rs
  - core/src/xml.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
default-search-keys:
  - Dom
  - NodeData
  - NodeType
  - DomVec
  - NodeHierarchyItem
  - AttributeType
  - Css
  - CssPropertyWithConditionsVec
  - SmallAriaInfo
  - AccessibilityInfo
  - CallbackInfo
  - RefAny
  - EventFilter
  - Update
---

# Document Object Model

## Introduction

If you've worked with the browser DOM, the surface here looks familiar.
You build a tree of nodes, each carries a tag, classes, attributes,
inline CSS, callbacks. What's different is the shape underneath, and a
small number of rules the framework imposes on you.

## Four design choices

Azul's DOM differs from a browser DOM in four places:

1. **Hierarchy lives separately from node data.** The relationships
   (`parent`, `prev_sibling`, `next_sibling`, `last_child`) are in one
   array. The content (`tag`, `class`, inline CSS, callbacks) is in a
   parallel array. They're indexed by the same node id.
2. **Both arrays are flat `Vec`s in DOM tree order.** Parent before
   children. So a slice `data[self..self.last_child + 1]` is the
   subtree rooted at `self`. No pointer-chasing to walk a subtree.
3. **The DOM is frozen after `layout()` returns.** There is no
   `insertChild`, no `setAttribute`, no mutation observers. To change
   the tree you return a new `Dom` from the next `layout()` call. The
   framework diffs old against new and migrates state across.
4. **CSS is stored in a compact, layout-hot cache.** Common enum
   properties (`display`, `position`, `float`, `overflow`) are
   bit-packed into a single `u64` per node. The numbers and the cold
   paint properties live in two more arrays. The point is to make the
   per-node working set small enough that the layout pass stays in L2
   instead of round-tripping to RAM. The compact-cache implementation
   itself is documented separately, in
   [internals/styling/compact-cache.md](internals/styling/compact-cache.md).

The result is a tree like this:

```text
         hierarchy[0..5]                  data[0..5]
       ┌───────────────┐               ┌──────────────────────┐
   0   │ parent: -     │ <body>        │ NodeData { ... }     │
   1   │ parent: 0     │   <div>       │ NodeData { ... }     │
   2   │ parent: 1     │     <span>    │ NodeData { ... }     │
   3   │ parent: 0     │   <p>         │ NodeData { ... }     │
   4   │ parent: 3     │     "text"    │ NodeData { ... }     │
       └───────────────┘               └──────────────────────┘
```

Indices into both arrays match. The layout engine traverses by index,
not by pointer, and reads from compact arrays whose memory layout it
controls.

## Cache hierarchy

Layout itself isn't algorithmically hard. It's a tree walk plus a lot
of if/else. The expensive part isn't the math; it's pulling each
node's properties out of memory.

A modern CPU has a tiered memory hierarchy. Cycle counts are
approximate but the order of magnitude is right:

- **L1 data cache** — 32 to 128 KB per core. ~4 cycles to read.
- **L2** — 256 KB to several MB per core. ~12 cycles.
- **L3** — 4 to 32 MB shared. ~30 to 60 cycles. Doesn't exist on
  embedded targets.
- **Main RAM** — gigabytes. ~100 to 300 cycles. A full miss costs
  more than running 100 instructions.

Layout reads the same per-node fields once per relayout pass. If the
working set fits in L2, the second pass is essentially free. If it
spills to RAM, every node fetch stalls the pipeline.

The relevant per-node sizes in the layout hot path:

```text
NodeHierarchyItem            32 B    parent + 3 sibling/child indices
StyledNodeState              10 B    :hover / :focus / :active per node
NodeFlags                     4 B    contenteditable, tab index, anonymous
compact-cache tier 1          8 B    display/position/float/etc bit-packed
compact-cache tier 2 (hot)   68 B    width, height, margin, padding, ...
compact-cache tier 2 (cold)  28 B    paint-only properties (color, opacity)
compact-cache tier 2b (text) 24 B    text-related layout

per-node total (hot)        ~150 B
per-node total (warm)       ~170 B   add cold + text tiers
NodeData (cold during layout) 152 B  read once for inline CSS, classes
```

For 1,000 nodes the layout-hot working set is ~150 KB. That fits in L2
on every desktop chip and most embedded ones. For 10,000 nodes it's
~1.5 MB, still L2 on a modern Apple/Intel core. For 100,000 nodes it's
~15 MB, which is L3 on desktop and main memory on embedded.

The numbers tell you when you have to start thinking about virtual
views, lazy panels, and other ways to keep the rendered subtree
small. See [Virtual Views](dom/virtual-views.md).

## What's in a node

Each node is split across the two parallel arrays.

`NodeHierarchyItem` carries four indices into the same array — the
node's parent, its previous and next siblings, and its last
descendant:

```rust,ignore
pub struct NodeHierarchyItem {
    pub parent: usize,            // 0 means "no parent"
    pub previous_sibling: usize,
    pub next_sibling: usize,
    pub last_child: usize,        // index of last descendant
}
```

Because children sit contiguously after their parent in tree order,
`data[self_idx ..= last_child]` is the whole subtree rooted at
`self_idx`. No pointer-chasing, no recursion needed to copy a subtree.

`NodeData` carries everything that defines a single node:

- `node_type: NodeType` — the HTML tag (Div, P, Button, ...) or one of
  the four leaves (Text, Image, Icon, VirtualView).
- `callbacks: CoreCallbackDataVec` — event handlers attached to the
  node. Empty for ~80% of nodes.
- `style: Css` — inline CSS as a `Css` value with implicit `:scope`.
  Same struct the cascade uses elsewhere. Carries conditional rules
  (`:hover`, `:focus`, `@theme dark`, `@os macos`) directly. Inline
  rules are tagged `rule_priority::INLINE` so they override author
  CSS.
- `flags: NodeFlags` — packed bits for tab index, contenteditable,
  anonymous.
- `accessibility: Option<Box<AccessibilityInfo>>` — ARIA payload, only
  allocated on accessible nodes.
- `extra: Option<Box<NodeDataExt>>` — boxed bag of less-common state
  (attributes, dataset, virtual-view payload, menus, merge callback,
  SVG data). About 95% of nodes never trigger this allocation.

The two `Option<Box<...>>` fields keep the common case small. A
typical paragraph or div pays nothing for the accessibility or
extras boxes.

## Frozen after creation

The DOM you return from `layout()` is the framework's, not yours
anymore. You don't keep a handle, you don't mutate it, you don't get
notified when something inside it changes. State change goes through
the next `layout()` call:

1. A callback returns `Update::RefreshDom`.
2. The framework re-invokes your layout function.
3. You build a fresh `Dom` from your application data.
4. The framework diffs the new tree against the previous one and
   migrates focus, scroll, dataset, and merge-callback state across
   matched nodes.

This rule exists because every JS framework worth using already
discourages mutation: React, Vue, Solid, etc. all model UI as a
function of state. Azul only makes it the rule, not the convention. 
A mutable DOM is the cause of half the bugs in any non-trivial web
app, and it's also what makes browser layout engines so hard to
optimise. Removing the mutation surface lets the framework treat the
tree as data.

The reconciliation algorithm — what counts as "matching" old and new
nodes, what migrates, what fires lifecycle events — is documented in
[Reconciliation](dom/reconciliation.md).

State that has to survive a tree rebuild (a video decoder, a GL
texture, the cursor inside a focused input) doesn't live in the tree
shape. It hangs off the node as a dataset. See
[Datasets](dom/datasets.md) and [Merge Callbacks](dom/merge-callbacks.md).

## Building a tree

```rust,no_run
use azul::prelude::*;
let dom: Dom = Dom::create_body()
    .with_child(Dom::create_h1_with_text("Hello"))
    .with_child(Dom::create_p_with_text("A paragraph."));
```

You build the tree as a recursive `Dom` value: a `NodeData` root plus
a `DomVec` of children. The framework flattens this into the parallel
arrays explained at the beginning when it starts to do the CSS cascade 
and calculate inherited CSS properties.

The rest of this page covers the node-data layout in detail, how to
attach CSS, the accessibility soft-force pattern, XML loading, and
the live-debugger hooks. Reusable fragments are covered separately
in [Components](dom/components.md).

## The Dom builder

`Dom` is the recursive form you actually construct:

```rust,ignore
pub struct Dom {
    pub root: NodeData,
    pub children: DomVec,
    pub css: CssVec,
    pub estimated_total_children: usize,
}
```

A `Dom` is a subtree: a root `NodeData` plus its children plus any
component-level stylesheets attached via `.style(Css)`. The framework
flattens the recursive form into the parallel arrays once, at the
start of the cascade. Every builder method on `Dom` (like `with_class`
or `with_callback`) is a shorthand that delegates to the same method
on `self.root`.

## Node constructors

Each HTML element has a `Dom::create_<tag>()` constructor. Most are `const
fn` and don't allocate until you add a child:

```rust,no_run
use azul::prelude::*;

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
use azul::prelude::*;

let _ = Dom::create_h1_with_text("Title");
let _ = Dom::create_h2_with_text("Section");
let _ = Dom::create_p_with_text("A paragraph.");
let _ = Dom::create_span_with_text("inline");
let _ = Dom::create_strong_with_text("important");
let _ = Dom::create_code_with_text("println!()");
let _ = Dom::create_text("standalone text node");
```

`NodeType` (in `core/src/dom.rs`) lists every variant. The set covers all
HTML elements plus the SVG subset plus four leaf types: `Text`, `Image`,
`Icon`, and `VirtualView`.

## Accessibility

For elements with non-trivial accessibility surface, the primary
constructor takes an a11y struct as a required argument. There's a
matching `_no_a11y` variant that opts out explicitly. The longer name
on the opt-out is the point. It signals that you skipped a11y on
purpose, and it makes the absence visible during code review.

```rust
use azul::prelude::*;

// Primary form: a11y is part of the call signature.
let save = Dom::create_button("Save", SmallAriaInfo::label("Save document"));

// Explicit opt-out, longer name.
let ok = Dom::create_button_no_a11y("OK".into());
```

Most interactive elements use the generic `SmallAriaInfo` (label, role,
description). A few have type-specific structs because their a11y
surface needs more than that:

- `<progress>` uses `ProgressAriaInfo` (label, current value, max,
  indeterminate).
- `<meter>` uses `MeterAriaInfo` (label, current value, min, max,
  optional low/high/optimum).
- `<dialog>` uses `DialogAriaInfo` (label, modal flag, described-by
  reference).

Example with a type-specific struct:

```rust
use azul::prelude::*;

let upload = Dom::create_progress(
    ProgressAriaInfo::create("File upload".into())
        .with_current_value(0.6)
        .with_max(1.0),
);
```

Elements that follow the pattern: `a`, `area`, `audio`, `button`,
`canvas`, `datalist`, `details`, `dialog`, `fieldset`, `form`, `input`,
`label`, `legend`, `menu`, `menuitem`, `meter`, `optgroup`, `option`,
`output`, `progress`, `select`, `summary`, `table`, `textarea`, `video`.

Static, non-interactive elements (`div`, `span`, `p`, the headings,
inline text formatters) don't take a11y info. Their role is implicit
from the element type.

See [Accessibility](accessibility.md) for the full a11y model and how
the framework translates these structs into platform-specific
accessibility trees (UIA, AT-SPI, NSAccessibility).

## Adding children

Three ways to attach children:

```rust,no_run
use azul::prelude::*;

// 1. One at a time. Each call grows .children by one.
let a = Dom::create_div()
    .with_child(Dom::create_h2_with_text("Title"))
    .with_child(Dom::create_p_with_text("Body"));

// 2. Replace the child vec wholesale.
let kids: DomVec = vec![Dom::create_span_with_text("x"), Dom::create_span_with_text("y"), Dom::create_span_with_text("z")].into();
let b = Dom::create_div().with_children(kids);

// 3. Collect from an iterator into a parent.
let c: Dom = (0..3).map(|i| Dom::create_li_with_text(format!("Item {}", i))).collect();
// Produces a NodeType::Div containing three <li> children.
```

`with_child` calls `add_child`, which pushes onto the underlying `Vec` and
updates `estimated_total_children`. That's amortised O(1) per call.
`with_children(DomVec)` is one allocation total.

`estimated_total_children` is maintained by every `add_child` and
`set_children` call. The framework reads it to pre-size the flat arena
during conversion. If you mutate `children` directly, call
`fixup_children_estimated()` before returning.

## IDs, classes, attributes

```rust,no_run
use azul::prelude::*;

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

Two public mechanisms cover the common cases:

- `with_clip_mask(ImageMask)` takes a raster alpha mask. Use it for
  irregular shapes that already exist as image data.
- `with_css("clip-path: ...;")` parses the CSS property into the node's
  inline-CSS list. Applied during the cascade.

```rust,no_run
use azul::prelude::*;

fn build(mask: ImageMask) -> Dom {
  let raster = Dom::create_image(ImageRef::null_image(0, 0, RawImageFormat::R8, U8VecRef::from(&[][..])))
    .with_clip_mask(mask);

  let css_form = Dom::create_div()
    .with_css("clip-path: circle(40px at 50% 50%);");

  Dom::create_body().with_child(raster).with_child(css_form)
}
```

A `clip-path` set on a parent applies to every descendant.

## Inline CSS

The primary way to attach CSS is `.with_css(...)` on the node itself.
You hand it a string; it parses through the same pipeline the cascade
uses elsewhere and stores the result in `NodeData::style: Css`.

```rust,no_run
use azul::prelude::*;

let item = Dom::create_div().with_css("
    color: blue;
    font-size: 14px;
    :hover { color: red; }
    @theme dark { color: white; background: #222; }
");
```

The parsed rules carry their conditions directly: `:hover`, `:focus`,
`:active`, `@os`, and `@theme` blocks all live inside the same `Css`
value. Conditions are re-evaluated per frame, so `@theme dark { ... }`
flips when the user toggles dark mode without any re-layout. Inline
rules are tagged `rule_priority::INLINE` so they win the cascade
against author CSS.

After the recent unification, the inline store is a regular `Css` —
the legacy `css_props: CssPropertyWithConditionsVec` field is gone.
`with_css_props(vec)` still works as a compatibility shim that maps
each property to a single-declaration rule at INLINE priority.

## Component-level stylesheets

Reusable components ship a parsed stylesheet that travels with the
subtree. Attach it on the component's root with `.style(Css)`:

```rust,no_run
use azul::prelude::*;

let widgets = Css::from_string(".panel { padding: 8px; }".into());
let panel: Dom = Dom::create_div()
    .with_class("panel".into())
    .style(widgets);
```

Multiple `.style(...)` calls stack in push order; later entries
override earlier ones at equal specificity. The framework gathers
every component-level `Css` together with the application stylesheet
and runs a single cascade after `layout()` returns.

Component CSS *applies to the subtree where it was attached* because
the rules only have a chance to match nodes that the component owns —
the cascade is global, but the component's marker class (or other
selector that scopes its rules) only exists inside its own subtree.
That's the convention components follow rather than a Shadow-DOM
boundary the framework enforces. For hard scoping, write selectors
that nest under the component's root class.

User-level theming (the system `@theme dark { ... }` block, the
`system:*` color keywords, end-user ricing in
`~/.config/azul/styles/<app>.css`) sits at the *outermost* layer of
the cascade. Component CSS doesn't fight user theming because the
component rules typically target component-internal classes, while
user theming targets the system color and font hooks. See
[Theming](styling/themes.md) for the full theming model and the
`AZ_DISABLE_RICING` opt-out.

## When does CSS apply?

Not while your `LayoutCallback` is running. The `Dom` you build
carries CSS as opaque state — `NodeData::style: Css` for inline
rules, `Dom::css: CssVec` for component stylesheets. The cascade
runs once after your layout callback returns: selector matching,
inheritance, and the compact-cache build all happen there.

CSS work inside `layout()` is cheap because each call is just a parse
and a push. The framework collects the rules, sorts by `(priority,
specificity)`, and walks the tree once to fill the compact cache.

For the internal cache layout that the layout engine reads, see
[internals/styling/compact-cache.md](internals/styling/compact-cache.md).

## Parsing from XHTML

`Dom::create_from_parsed_xml` is the public entry point. Pass it an `Xml`
value and you get a `Dom` back, ready to return from `layout()`:

```rust,no_run
use azul::prelude::*;

let xml_text = "";
let parsed = Xml::from_str(xml_text.into()).unwrap();
let dom: Dom = Dom::create_from_parsed_xml(parsed);
```

The XML parser walks `<html><head><style>...</style></head><body>...</body>`,
parses each `<style>` block into a `Css`, and attaches it scoped to the
node it was found inside. The cascade runs on the next layout pass like
any other DOM.

`ComponentMap` is the registry of XML-defined components. See
[Components](dom/components.md#component-packs) for how the framework
looks up `<card title="..."/>` against a registered library.

## Parsing from SVG

The same XML pipeline accepts `<svg>` tags inside the body and turns
them into vector nodes that render alongside the rest of the Dom.
You don't need extra wiring: the parser recognises the SVG namespace,
walks the geometry, and stamps an `SvgNodeData` on each shape. A
clip-mask attribute on an SVG element resolves the same way as a CSS
`clip-path:` (see [Clipping a node](#clipping-a-node) above).

```rust,no_run
use azul::prelude::*;

let xhtml = r#"
<html>
  <body>
    <svg viewBox="0 0 100 100" width="200" height="200">
      <circle cx="50" cy="50" r="40" fill="#1d4f8b"/>
      <rect x="10" y="10" width="40" height="40" fill="#5dade2"/>
    </svg>
  </body>
</html>
"#;
let parsed = Xml::from_str(xhtml.into()).unwrap();
let dom: Dom = Dom::create_from_parsed_xml(parsed);
```

The same `dom` is renderable without a window. Run the binary with
`AZ_BACKEND=headless` and the framework rasterises into an in-memory
framebuffer instead of opening a window. Combine with
`AZ_DEBUG=<port>` to drive `take_screenshot` from a shell script and
get a base64 PNG back. That's the path snapshot tests, PDF export,
and CI machines without a display server use.

For the full SVG geometry model (paths, tessellation, GPU
tessellated nodes), the standalone `Svg::from_string` parser that
returns a `RawImage`, and the GPU stroke pipeline, see
[SVG](images/svg.md). For the headless backend in detail, see
[Headless Rendering](headless-rendering.md).

## Callbacks

```rust,no_run
use azul::prelude::*;

struct Counter { value: i64 }
extern "C" fn on_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    let mut c = match data.downcast_mut::<Counter>() { Some(c) => c, None => return Update::DoNothing };
    c.value += 1;
    Update::RefreshDom
}

fn build(state: RefAny) -> Dom {
  Dom::create_button_no_a11y("+1".into())
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), state, on_click)
}
```

`with_callback(filter, data, callback)` attaches a `RefAny` and a
function pointer. The handler fires when the matching event reaches
the node. The callback returns an `Update` that tells the framework
whether to re-run layout, re-render, or do nothing.

What the callback can actually *do* — read the dataset, query the
hit-test, dispatch to siblings, focus another node, schedule a
timer, post a thread message — lives in
[Callbacks](callbacks.md). Event filtering and propagation order
are in [Events and Input](events.md).

The framework's reconciler matches new nodes against old ones when you
return a fresh tree. Cursor position, focus, and dataset state migrate
across the diff for matched nodes. See
[Reconciliation](dom/reconciliation.md).

## Datasets

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

## Virtual Views

A `VirtualView` is a node whose contents come from a separate callback
that runs only when the framework needs them. Use it for infinite lists,
lazy panels, and embedded sub-DOMs that own their own scroll math. The
callback receives a `VirtualViewCallbackReason` (`InitialRender`,
`DomRecreated`, `BoundsExpanded`, `EdgeScrolled(_)`,
`ScrollBeyondContent`). Use the reason to skip work when the call is
just a parent re-render. See [Virtual Views](dom/virtual-views.md) for
the rendered-vs-virtual coordinate model and a virtualised-table
walkthrough.

## Debugging

Run your binary with `AZ_DEBUG=<port>` set and `App::create` starts
an HTTP debug server on that port. It accepts JSON commands and
returns JSON responses. The inspector sees the same tree the
renderer is about to draw, so what you query is what's on screen.

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

The synthesised events go through the exact same dispatch path as
real input. A scripted `click` runs the same hit test, the same
event filters, and the same callback as a user mouse click. That
makes the debug server the basis for end-to-end tests: drive the
app from a shell script or a Python harness, assert on the JSON
responses, and you have an integration test that exercises the real
layout, the real callbacks, and the real reconciliation pass. The
test pattern, the assertion vocabulary, and the CI recipe are
covered in [End-to-End Testing](e2e-testing.md).

For tests that don't need a window (snapshot tests, PDF export, CI
machines without a display server), the same debug API is also
reachable in [Headless Rendering](headless-rendering.md), which
runs the full layout and rendering pipeline into a `Vec<u8>`
framebuffer.

The `get_node_hierarchy` response carries a `component` field for
each node so you can navigate to the component that produced it.
The inspector uses it to draw a Component Tree alongside the DOM
Tree:

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

Pick a node in the DOM Tree, get the component that produced it,
and (if its render function lives in a registered library) jump
back to the source. See [Components](dom/components.md#component-packs)
for how a library wires its components into the registry.

## Coming Up Next

- [Callbacks](callbacks.md) — What `CallbackInfo` exposes, dataset reads, focus/scroll dispatch
- [Reconciliation](dom/reconciliation.md) — Diffing, restyle scope, and damage-rect repaint
- [Datasets](dom/datasets.md) — Attaching state to a node for navigation and per-instance state
- [Components](dom/components.md) — Reusable UI fragments — named functions of (args) -> Dom
- [Styling with CSS](styling.md) — Stylesheets, selectors, and the cascade
- [Theming](styling/themes.md) — `@theme dark`, `system:*` colors, and end-user ricing
