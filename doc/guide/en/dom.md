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
   the tree, the next `layout()` call returns a new `Dom`. The
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

The numbers indicate when virtual views, lazy panels, and other ways
to keep the rendered subtree small start to matter. See
[Virtual Views](dom/virtual-views.md).

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

```rust,ignore
pub struct NodeData {
    pub node_type: NodeType,                          // HTML tag or leaf (Text/Image/Icon/VirtualView)
    pub callbacks: CoreCallbackDataVec,               // event handlers; empty for ~80% of nodes
    pub style: Css,                                   // inline CSS with implicit :scope, INLINE priority
    pub flags: NodeFlags,                             // tab index, contenteditable, anonymous
    pub accessibility: Option<Box<AccessibilityInfo>>, // ARIA, only on accessible nodes
    pub extra: Option<Box<NodeDataExt>>,              // attributes, dataset, menus, virtual view, ...
}
```

`style: Css` is the same struct the cascade uses everywhere else;
inline rules carry their conditions (`:hover`, `:focus`, `@theme dark`,
`@os macos`) directly. The two `Option<Box<...>>` fields keep the
common case small — a typical paragraph or div pays nothing for the
accessibility or extras boxes. About 95% of nodes never trigger the
`extra` allocation.

## A function of state

It helps to remember what the browser DOM was originally for. In the
1990s, web pages arrived over slow modems as a stream of HTML, and
the browser had to render *while* the document was still being
received. `document.write` injected new nodes mid-parse;
`appendChild`, `removeChild`, and the rest of the mutation API let
scripts patch the tree as more bytes arrived. Mutability wasn't a
design goal — it was a constraint of streaming over a 14.4k modem.

When SPAs took over, the streaming use case mostly went away, but the
mutation API stayed. React's contribution was to talk users out of
using it: model the UI as a function of state, render the whole tree
on every change, and let a reconciler diff old against new. Vue,
Solid, Svelte, and Elm all converged on the same shape. The browser's
imperative DOM became an implementation detail the framework hid.

Azul has no streaming parser to support and no legacy mutation API to
preserve, so it makes "UI is a function of state" the rule from the
start. The `Dom` returned from `layout()` becomes the framework's
copy:

1. A callback returns `Update::RefreshDom`.
2. The framework re-invokes the layout function.
3. A fresh `Dom` is built from the application data.
4. The framework diffs the new tree against the previous one and
   migrates focus, scroll, dataset, and merge-callback state across
   matched nodes.

There is no handle to the live tree, no `insertChild` /
`setAttribute` / mutation observer surface. Removing the mutation API
has two payoffs: half the bugs that show up in any non-trivial UI
come from "this listener saw stale state because something else
mutated the tree first," and a tree the framework *owns* is far
easier to lay out incrementally than a tree the application can
change at any time.

The reconciliation algorithm — what counts as "matching" old and new
nodes, what migrates, what fires lifecycle events — is documented in
[Reconciliation](dom/reconciliation.md).

State that has to survive a tree rebuild (a video decoder, a GL
texture, the cursor inside a focused input) doesn't live in the tree
shape. It hangs off the node as a dataset. See
[Datasets](dom/datasets.md) and [Merge Callbacks](dom/merge-callbacks.md).

## Building DOMs

### The recursive Dom value

`Dom` is the form actually constructed in user code:

```rust,ignore
pub struct Dom {
    pub root: NodeData,
    pub children: DomVec,
    pub css: CssVec,
    pub estimated_total_children: usize,
}
```

A `Dom` is a subtree: a root `NodeData`, its children, and any
component-level stylesheets attached via `.with_component_css(Css)`.
The framework
flattens the recursive form into the parallel `NodeHierarchyItem` /
`NodeData` arrays once, at the start of the cascade. Every builder
method on `Dom` (`with_class`, `with_callback`, `with_css`) is a
shorthand that delegates to the same method on `self.root`.

### Node constructors

Each HTML element has a `Dom::create_<tag>()` constructor. Most are
`const fn` and don't allocate until a child is added:

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

Text-bearing constructors take a string and wrap a `Text` child
inside the element:

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

`NodeType` (in `core/src/dom.rs`) lists every variant. The set covers
all HTML elements plus the SVG subset plus four leaf types: `Text`,
`Image`, `Icon`, and `VirtualView`.

For elements with non-trivial accessibility surface, the primary
constructor takes an a11y struct as a required argument. There's a
matching `_no_a11y` variant that opts out explicitly. The longer
name on the opt-out is the point: it signals that a11y was skipped
on purpose, and it makes the absence visible during code review —
the *soft-force* pattern.

```rust
use azul::prelude::*;

// Primary form: a11y is part of the call signature.
let save = Dom::create_button("Save", SmallAriaInfo::label("Save document"));

// Explicit opt-out, longer name.
let ok = Dom::create_button_no_a11y("OK".into());
```

Most interactive elements use the generic `SmallAriaInfo` (label,
role, description). A few (`<progress>`, `<meter>`, `<dialog>`)
have type-specific structs because their a11y surface needs more
than that. Static, non-interactive elements (`div`, `span`, `p`,
the headings, inline text formatters) don't take a11y info — their
role is implicit from the element type.

See [Accessibility](accessibility.md) for the full list of elements
that follow the soft-force pattern, the type-specific aria structs,
and how the framework translates them into the platform-specific
accessibility trees (UIA, AT-SPI, NSAccessibility).

### IDs, classes, attributes

```rust,no_run
use azul::prelude::*;

let _ = Dom::create_div()
    .with_id("sidebar".into())
    .with_class("panel".into())
    .with_class("scrollable".into())
    .with_attribute(AttributeType::AriaLabel("notification banner".into()))
    .with_attribute(AttributeType::Lang("en".into()));
```

IDs and classes aren't separate fields. They're stored as
`AttributeType::Id` and `AttributeType::Class` entries in the node's
attribute list. The selector `.panel { ... }` matches every node
whose attribute list contains `Class("panel")`.

`AttributeType` (in `core/src/dom.rs`) is a strongly-typed enum:
`Href`, `Src`, `Alt`, `AriaLabel`, `Required`, `MaxLength(i32)`,
`ContentEditable(bool)`, and so on. There's a `Custom` fallback for
arbitrary `name="value"` pairs. Attributes aren't inline CSS — they
feed accessibility, attribute selectors like `[lang="en"]`, and
HTML/XML serialization.

### Adding children

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

`with_child` calls `add_child`, which pushes onto the underlying
`Vec` and updates `estimated_total_children`. That's amortised O(1)
per call. `with_children(DomVec)` is one allocation total.

`estimated_total_children` is maintained by every `add_child` and
`set_children` call. The framework reads it to pre-size the flat
arena during conversion. If `children` is mutated directly, call
`fixup_children_estimated()` before returning.

### Defining a clipping path

Two public mechanisms cover the common cases:

- `with_clip_mask(ImageMask)` takes a raster alpha mask. Use it for
  irregular shapes that already exist as image data.
- `with_css("clip-path: ...;")` parses the CSS property into the
  node's inline-CSS list. Applied during the cascade.

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

### Inline CSS

The primary way to attach CSS is `.with_css(...)` on the node itself.
The method takes a string, parses it through the same pipeline the
cascade uses elsewhere, and stores the result in
`NodeData::style: Css`.

```rust,no_run
use azul::prelude::*;

let item = Dom::create_div().with_css("
    color: blue;
    font-size: 14px;
    :hover { color: red; }
    @theme dark { color: white; background: #222; }
");
```

The parsed rules carry their conditions directly: `:hover`,
`:focus`, `:active`, `@os`, and `@theme` blocks all live inside the
same `Css` value. Conditions are re-evaluated per frame, so
`@theme dark { ... }` flips when the user toggles dark mode without
any re-layout. Inline rules are tagged `rule_priority::INLINE` so
they win the cascade against author CSS.

After the recent unification, the inline store is a regular `Css` —
the legacy `css_props: CssPropertyWithConditionsVec` field is gone.
`with_css_props(vec)` still works as a compatibility shim that maps
each property to a single-declaration rule at INLINE priority.

### Component-level stylesheets

Reusable components ship a parsed stylesheet that travels with the
subtree. Attach it on the component's root with `.with_component_css(Css)`:

```rust,no_run
use azul::prelude::*;

let widgets = Css::from_string(".panel { padding: 8px; }".into());
let panel: Dom = Dom::create_div()
    .with_class("panel".into())
    .with_component_css(widgets);
```

A browser cascades every stylesheet against every node in one global
pass — `.panel { ... }` in one tab can match a `.panel` in any iframe
that imports the same stylesheet, and changes there force a global
restyle. Azul's component CSS travels *with the subtree*. The
framework merges every component-level `Css` together when it
flattens the tree, but the rules a component ships only have a
chance to match the nodes the component itself owns. Anything
outside the component's subtree is invisible to its selectors. This
is a soft scope (the framework doesn't enforce a Shadow-DOM
boundary), but it follows from the way components label their roots
and avoids the cross-component restyle storms a global cascade
produces. For hard scoping, write selectors that nest under the
component's root class.

User-level theming sits at the *outermost* layer: the system
`@theme dark` block, the `system:*` color keywords, and the optional
end-user ricing file all target the framework-wide hooks. Component
CSS doesn't fight user theming because the two layers target
different selectors. See [Components](dom/components.md) for the
component-pack model and [Theming](styling/themes.md) for the full
theming model and the `AZ_RICING` opt-out.

The cascade runs *once*, after the `LayoutCallback` returns.
`NodeData::style: Css` and `Dom::css: CssVec` are opaque state
during `layout()`. The framework collects the rules at the end of
the callback, sorts by `(priority, specificity)`, and walks the tree
once to fill the compact cache. Selector matching, inheritance, and
the compact-cache build all happen there. CSS work inside `layout()`
is cheap because each call is just a parse and a push. For the
internal cache layout that the layout engine reads, see
[internals/styling/compact-cache.md](internals/styling/compact-cache.md).

### Inside the layout callback

A `layout()` callback receives application data and a
`LayoutCallbackInfo` describing the window. Returning a `Dom`
finishes the pass; the framework reconciles, lays out, and renders.

```rust,no_run
use azul::prelude::*;

struct AppModel {
    user_name: String,
    locale: Locale,
}

extern "C" fn layout(data: &mut RefAny, info: LayoutCallbackInfo) -> StyledDom {
    let model = match data.downcast_ref::<AppModel>() {
        Some(m) => m,
        None => return StyledDom::default(),
    };

    let strings = Strings::for_locale(model.locale);

    // Window-aware layout: switch to a single-column layout below 768px.
    let body = if info.window_width_less_than(768.0) {
        Dom::create_body()
            .with_css("display:flex; flex-direction:column;")
            .with_child(navbar_compact(&model.user_name, &strings))
            .with_child(content_area(&strings))
    } else {
        Dom::create_body()
            .with_css("display:grid; grid-template-columns:240px 1fr;")
            .with_child(sidebar(&model.user_name, &strings))
            .with_child(content_area(&strings))
    };

    body.with_component_css(app_stylesheet()).style_dom()
}
```

The `LayoutCallbackInfo` exposes everything needed to make the
returned `Dom` adapt to the running window. Responsive sizing
(`window_width_less_than`, `window_width_between`, `window_height_*`,
and the raw `get_window_width` / `get_window_height`) covers the
per-tree branch cases (a hamburger menu vs a sidebar) — the
per-property cases (`@media`, `@theme`) are handled by inline CSS.
The framework re-invokes `layout()` whenever the window crosses a
breakpoint, the system theme flips, or a route switch fires, so the
width branch is always re-evaluated against the live window. The
callback can read `info.relayout_reason()` to find out *why* it was
called — `Resize`, `ThemeChange`, `RouteChange`, `RefreshDom`, or
`Initial` — and skip work that doesn't need to repeat (analytics
fetches, locale-pack loading) when the trigger was just a resize.

Other helpers: `get_dpi_factor` returns 1.0 / 2.0 / etc. for asset
selection; `get_active_route()` / `get_route_param(key)` for
router-driven trees; `get_image(name)` for registered images;
`get_system_style()` for the current `SystemStyle` snapshot;
`get_gl_context()` for canvas-backed nodes; `get_system_fonts()` for
font availability checks (CJK / RTL fallbacks).

A worked example covering window-size, DPI, theme, route, and
Fluent localization in a single layout pass:

```rust,no_run
use azul::prelude::*;
use azul::desktop::fluent::{FluentLocalizerHandle, FluentFormatArg};

struct AppModel {
    user_name: String,
    locale: String,                    // BCP-47, e.g. "fr-FR"
    localizer: FluentLocalizerHandle,
    unread_count: u32,
}

extern "C" fn layout(data: &mut RefAny, info: LayoutCallbackInfo) -> StyledDom {
    let model = match data.downcast_ref::<AppModel>() {
        Some(m) => m,
        None => return StyledDom::default(),
    };

    // i18n: a localized greeting + a pluralized inbox count.
    let greeting = model.localizer.translate(
        model.locale.as_str().into(),
        "greeting".into(),
        Some(&[FluentFormatArg::str("name", &model.user_name)].into()),
    );
    let inbox = model.localizer.translate(
        model.locale.as_str().into(),
        "inbox-count".into(),
        Some(&[FluentFormatArg::num("count", model.unread_count as i64)].into()),
    );

    // DPI-aware logo: prefer the @2x variant on Retina/HiDPI screens.
    let logo = if info.get_dpi_factor() >= 1.5 { "logo@2x" } else { "logo" };
    let logo_img = info.get_image(&logo.into())
        .map(Dom::create_image)
        .unwrap_or_else(Dom::create_div);

    // Theme-aware accent color picked outside CSS (for a value the
    // cascade can't reach — e.g. a canvas paint color).
    let accent = match info.theme {
        WindowTheme::DarkMode => "#79b8ff",
        WindowTheme::LightMode => "#0046bf",
    };

    // Route-driven content: /settings vs /inbox vs default.
    let main = match info.get_route_pattern().as_str() {
        "/settings" => settings_page(&model),
        "/inbox" => inbox_page(&model, &inbox),
        _ => home_page(&model, &greeting),
    };

    // Window-size-driven layout: hamburger nav under 768px, sidebar above.
    let shell = if info.window_width_less_than(768.0) {
        Dom::create_body()
            .with_css("display:flex; flex-direction:column;")
            .with_child(top_bar(logo_img, accent))
            .with_child(main)
    } else {
        Dom::create_body()
            .with_css("display:grid; grid-template-columns:240px 1fr;")
            .with_child(sidebar(logo_img, &greeting, accent))
            .with_child(main)
    };

    shell.with_component_css(app_stylesheet()).style_dom()
}
```

The output is a `StyledDom` (`dom.style_dom()` runs the cascade and
returns the framework-owned form). Returning it hands ownership to
the framework, which reconciles against the previous frame and
schedules layout + paint.

## Routing

A multi-page app registers a layout callback per URL pattern on the
`AppConfig` — the framework picks the right one for the active
route and re-runs it on `switch_route`:

```rust,no_run
use azul::prelude::*;

extern "C" fn layout_home(_: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom { todo!() }
extern "C" fn layout_user(_: &mut RefAny, info: LayoutCallbackInfo) -> StyledDom {
    let id = info.get_route_param("id").map(|s| s.as_str()).unwrap_or("");
    Dom::create_h1_with_text(format!("User #{}", id)).style_dom()
}

fn main() {
    let mut config = AppConfig::create();
    config.add_route("/", layout_home);
    config.add_route("/user/:id", layout_user);

    let app = App::create(initial_data, config);
    app.run(WindowCreateOptions::new(layout_home));
}
```

A `:name` segment captures the path component as a parameter
readable via `info.get_route_param("name")`. On desktop the route
is in-memory state; on a web build the same routes also map to
HTTP endpoints with `history.pushState()` integration.

A user callback navigates with `CallbackInfo::switch_route` —
`info.set_route_param(key, value)` modifies a single param in place
without changing the active pattern:

```rust,ignore
extern "C" fn open_user(data: RefAny, mut info: CallbackInfo) -> Update {
    let id = match data.downcast_ref::<u64>() {
        Some(i) => *i,
        None => return Update::DoNothing,
    };
    let params = vec![StringPair {
        key: "id".into(),
        value: id.to_string().into(),
    }].into();
    info.switch_route("/user/:id".into(), params);
    Update::RefreshDom
}
```

The framework swaps the active layout callback on the next frame
and reconciles the new tree against the previous one. See
[Routing](routing.md) for the full pattern syntax, multi-route
layouts, and the web-vs-desktop differences.

## Parsing from XHTML

`Dom::create_from_parsed_xml` is the public entry point. Given an
`Xml` value, it returns a `Dom` ready to return from `layout()`:

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
No extra wiring is required: the parser recognises the SVG namespace,
walks the geometry, and stamps an `SvgNodeData` on each shape. A
clip-mask attribute on an SVG element resolves the same way as a CSS
`clip-path:` (see [Defining a clipping path](#defining-a-clipping-path)
above).

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

The framework's reconciler matches new nodes against old ones when a
fresh tree is returned. Cursor position, focus, and dataset state
migrate across the diff for matched nodes. See
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

Run the binary with `AZ_DEBUG=<port>` set and `App::create` starts
an HTTP debug server on that port. It accepts JSON commands and
returns JSON responses. The inspector sees the same tree the
renderer is about to draw, so a query reflects what's on screen.

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
responses, and the result is an integration test that exercises the
real layout, the real callbacks, and the real reconciliation pass.
The test pattern, the assertion vocabulary, and the CI recipe are
covered in [End-to-End Testing](e2e-testing.md).

For tests that don't need a window (snapshot tests, PDF export, CI
machines without a display server), the same debug API is also
reachable in [Headless Rendering](headless-rendering.md), which
runs the full layout and rendering pipeline into a `Vec<u8>`
framebuffer.

The `get_node_hierarchy` response carries a `component` field for
each node, allowing navigation back to the component that produced
it. The inspector uses it to draw a Component Tree alongside the DOM
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

Picking a node in the DOM Tree surfaces the component that produced
it, and (if its render function lives in a registered library) the
inspector links back to the source. See
[Components](dom/components.md#component-packs) for how a library
wires its components into the registry.

## Coming Up Next

- [Callbacks](callbacks.md) — What `CallbackInfo` exposes, dataset reads, focus/scroll dispatch
- [Routing](routing.md) — URL patterns, route params, and per-route layout callbacks
- [Reconciliation](dom/reconciliation.md) — Diffing, restyle scope, and damage-rect repaint
- [Datasets](dom/datasets.md) — Attaching state to a node for navigation and per-instance state
- [Components](dom/components.md) — Reusable UI fragments — named functions of (args) -> Dom
- [Styling with CSS](styling.md) — Stylesheets, selectors, and the cascade
- [Theming](styling/themes.md) — `@theme dark`, `system:*` colors, and end-user ricing
