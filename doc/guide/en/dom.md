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

Azul's DOM differs from a browser DOM in two key ways:

1. **The DOM is frozen after \`layout()\` returns.** There is no \`insertChild\`, no \`setAttribute\`, and no mutation observers. The DOM internally is entirely immutable and not editable by your code after creation. To change the tree, your next \`layout()\` call returns a new \`Dom\`. The framework diffs the old against the new and migrates state (like focus and cursor position) across.
2. **Hierarchy lives separately from node data.** The relationships (`parent`, `prev_sibling`, `next_sibling`, `last_child`) are in one flat array, while the content (`tag`, `class`, inline CSS, callbacks) is in a parallel flat array. This makes layout exceptionally fast since it avoids pointer-chasing.

## Constructing a DOM

Azul offers two main ways to construct a DOM tree: using recursive builders, or by parsing XML.

### Recursive Construction (Dom vs FastDom)

You can build a DOM programmatically by composing nodes. There are two primary APIs for this: `Dom` and `FastDom`.

`Dom` is the most flexible and idiomatic way. It allocates heap memory as you build it and allows you to attach callbacks, classes, and IDs seamlessly:

```rust,no_run
let my_dom = Dom::div()
    .with_id("container")
    .with_class("panel")
    .with_child(Dom::label("Hello World"))
    .with_child(Dom::button("Click Me"));
```

`FastDom`, on the other hand, is a zero-allocation, arena-based alternative. It is highly optimized for performance and is ideal for constructing massive lists or tables. Because it doesn't allocate dynamically, its lifetime is tied to the arena, making it slightly more complex to use but significantly faster when re-rendering thousands of nodes.

### Parsing from XHTML

`Dom::create_from_parsed_xml` is the public entry point for building a DOM from a string of XHTML. Given an `Xml` value, it parses tags and components directly into a `Dom` tree.

```rust,no_run
let xml_text = "<html><body><div class=\"panel\">Hello World</div></body></html>";
let parsed = Xml::from_str(xml_text.into()).unwrap();
let dom: Dom = Dom::create_from_parsed_xml(parsed);
```

XML construction is especially powerful because it natively parses `<style>` tags directly into CSS scoped to the DOM, and supports custom components natively (like `<my-button/>`).

## Node Setters

A DOM node exposes various setters that allow you to customize its behavior, appearance, and interactivity before it is returned to the framework:

- **`.with_id(id)`**: Sets the unique identifier for CSS `#id` targeting.
- **`.with_class(class)`**: Adds a CSS class.
- **`.with_css_override(css)`**: Overrides specific CSS properties inline.
- **`.with_callback(event, data, callback)`**: Attaches an event listener (like a click or hover) with user data.
- **`.with_dataset(data)`**: Attaches arbitrary data (a `RefAny`) that can be retrieved during events.
- **`.with_tab_index(index)`**: Makes the node focusable via the keyboard.

## Text Editing (contenteditable)

Azul allows any text node to become a text input simply by flagging it. By setting a node as `contenteditable`, the framework automatically handles cursor rendering, text selection, backspace handling, and clipboard integration without needing complex custom logic.

```rust,no_run
let input = Dom::label("Edit this text").with_contenteditable(true);
```

When the user types, the framework manages the internal text buffer. You can read the updated text back during events or on the next layout pass. (See the text input documentation for deeper details on binding editable text to your application state).

## Virtual Views

If your application has components that update very frequently (like a video player, an audio waveform, or a fast-updating `ProgressBar`), returning a new `Dom` and re-running the layout engine on every frame would be wasteful.

For these cases, Azul provides **Virtual Views**. A `VirtualView` allows you to "swap out" parts of the DOM directly without triggering a full UI layout. The node reserves its bounds in the layout tree, but its contents are fulfilled by a separate callback that runs on-demand.

```rust,no_run
// Creates a virtual view that updates its visual representation on demand
let progress = Dom::virtual_view(my_progress_callback);
```

The callback receives a `VirtualViewCallbackReason` indicating exactly why it was called (e.g., `InitialRender`, `BoundsExpanded`, etc.), allowing you to skip rendering if the component hasn't actually changed size or state.
