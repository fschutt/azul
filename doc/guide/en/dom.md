---
slug: dom
title: Document Object Model
language: en
canonical_slug: dom
audience: external
maturity: mature
guide_order: 50
topic_only: false
short_desc: Node types, hierarchy, and CSS scoping
tracked_files:
  - core/src/dom.rs
  - core/src/styled_dom.rs
  - core/src/xml.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
default-search-keys:
  - Dom
  - FastDom
  - VirtualView
  - NodeData
  - contenteditable
---

# Document Object Model

To create the structure of a UI, you need to construct a `Dom` object and return it to the framework. The semantics are roughly the same as on the web, although due to Azuls orientation towards making the UI a derivation of your application state, there are some differences to the web:

1. **The DOM is frozen after \`layout()\` returns.** There is no \`insertChild\`, no \`setAttribute\`, and no mutation observers. The DOM internally is entirely immutable and not editable by your code after creation. To change the tree, your next \`layout()\` call returns a new \`Dom\`. The framework diffs the old against the new and migrates state (like focus and cursor position) across.
2. Node hierarchy is separated from node data. Internally, the framework transforms the `Dom` into two flat `Vec<NodeHierarchy>` and `Vec<NodeData>` structs, and also "compress-encodes" Css values. The reason for this is that it heavily reduces layouting time and memory usage because of better cache access. While this is less noticable, it's the reason why all the `CallbackInfo` APIs hand you a `(DomId, NodeId)` pair to look up information instead of a reference-counted handle such as in a web browser.

## Construction

Azul offers three main ways to construct a `Dom`: using the `FastDom` (directly constructing the two `Vec` arrays mentioned above), using recursive builders (nicer to read but less efficient) or by parsing XML / SVG.

### Recursive Construction

You can build a DOM programmatically by composing nodes - you already saw this API in the "Hello World" guide. There is however still another FastDom API - compare:



```rust,no_run
let my_dom = Dom::div()
    .with_id("container")
    .with_class("panel")
    .with_child(Dom::label("Hello World"))
    .with_child(Dom::button("Click Me"));
```

with the `FastDom` approach - which is much more performant for large lists or tables:

### Parsing

Additionally to programmatic construction, a `Dom`can also be constructed from XML and the XML can be parsed from a string - whereas the XML parser is lenient enough to also parse SVG files.

```rust,no_run
let xml_text = "<html><body><div class=\"panel\">Hello World</div></body></html>";
let parsed = Xml::from_str(xml_text.into()).unwrap();
let dom: Dom = Dom::create_from_parsed_xml(parsed);
```

XML construction is especially powerful because it natively parses `<style>` tags directly into CSS scoped to the DOM, and supports the custom components (like `<shadcn:my-button/>`). that you saw previously in the guide about [Components](../architecture/components.md). The XML parser is intentionally lenient: while not a full HTML5 parser, it does have some HTML5 features such as auto-closing tags.

## Node Attributes

The `Dom` node itself exposes various setters that allow you to customize the behavior, appearance, interactivity and accessibility. The `Dom` and its `NodeData` offer various methods:

- **`.with_id(id)`** / **`.with_class(class)`**: Standard ID and class attributes, which are familiar for any web developer. Used for matching to CSS.
- **`.with_marker(string)`**: Adds a unique "marker" to this node, which does not affect Node equality - usually `string` will be a `Uuid`, so that callbacks can navigate to the resulting `NodeId` if they know the `Uuid` string attached to the node.
- **`.with_css(css)`**: Pushes a new CSS stylesheet onto this subtree. Note that this is a full stylesheet (you can use `@media`, `:hover` and `.class` selectors here), but only applies to the subtree of this node, not to the global UI (for global styling, you'd use `with_css` on the `<body/>` root node. Stylesheets "stack" on top of each other (last stylesheet rules override earlier ones, parent can override styles of children, which makes theming possible).
- **`.with_callback(filter, data, callback)`**: Adds callbacks Attaches an event listener (like a click or hover) with user data.
- **`.with_dataset(data)`**: Attaches a `RefAny` data object that can even survive a `Dom` rebuild (if `.with_merge_callback(cb)` is also set).
- **`.with_tab_index(index)`**: Configures this node for receiving focus events and sets up tab order withing the UI.

## Text Editing

Azul allows any text node to become a text input simply by adding the `contenteditable=true` attribute. In fact, this is how the `<input>` text input itself is constructed: Azul replaces the `<input>` node with a `<div class="__azul-text-input"></div>` internally. By setting a node as contenteditable, the framework automatically handles cursor rendering, text selection, backspace handling, IME and clipboard integration. However, please note: the raw `text` object is always inline, but normally browsers automatically wrap a raw `::text` node in a `<p>`, `<span>` or similar node, which then receives and cascades the styling. A raw `::text` node cannot itself receive any styling, which is why the API has the scary `createTextDoNotUseWithoutBlockLevelWrapper` and a `<p>Raw Text</p>` UI is created via `createPWithText` (i.e. a `<p>` block item with a `::text` node as a direct child containing the actual text).

```rust,no_run
let input = Dom::label("Edit this text").with_contenteditable(true);
```

When the user types, the framework manages the internal text buffer. By default, the edit is "accepted", but a callback can also "inspect" the changeset and "block" its application (for example, only allow numeric input to go through). You can read the updated text back during events or on the next layout pass. (See the text input documentation for deeper details on binding editable text to your application state).

## Virtual Views

The obvious question of the `f(UI) -> Dom` idiom is "how will this ever be performant for massive datasets?". The solution here is simple: "never render much more than what's on-screen right now". In classical OOP toolkits you'd have to create a lot of objects at program startup, so that their lifetimes will stay around and you can hand their references to other UI components. In Azul however, the idea is that we can "re-generate" the UI from data just-in-time when it is then actually needed. A regular UI, even a complex one rarely goes above 5000 nodes - and with the `FastDom` approach listed above you can (in theory) optimize your UI to just two `Vec` memory allocations.

However, for large datasets, you need "virtualization", i.e. the `Dom` now contains only a callback to continuously call once the user scrolls. The node reserves its bounds in the layout tree, but its contents are filled by a invoking separate callback that runs on-demand. The nice benefit of this is that this `VirtualView` node can also be used to "swap out" only parts of a `Dom` without requiring a full reconstruction. The `ProgressBar` component for example uses this technique: 



```rust,no_run
// Creates a virtual view that updates its visual representation on demand
let progress = Dom::virtual_view(my_progress_callback);
```

The callback receives a VirtualViewCallbackReason from the framework, indicating exactly why it was called (e.g., InitialRender, BoundsExpanded, etc.) and what the current user scroll position is.
