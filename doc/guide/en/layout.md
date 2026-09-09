---
slug: layout
title: Layout
language: en
canonical_slug: layout
audience: external
maturity: mature
guide_order: 80
topic_only: false
short_desc: Overview of the layout solver
prerequisites: [styling]
tracked_files:
  - css/src/props/layout/mod.rs
  - css/src/props/layout/display.rs
  - css/src/props/layout/dimensions.rs
  - css/src/props/layout/spacing.rs
  - css/src/props/layout/wrapping.rs
  - css/src/props/layout/fragmentation.rs
  - css/src/props/layout/column.rs
  - css/src/props/layout/table.rs
  - css/src/props/layout/shape.rs
  - css/src/props/layout/flow.rs
  - layout/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:49:28Z
default-search-keys:
  - Dom
  - Css
  - CssProperty
  - StyledDom
  - LayoutCallback
  - LayoutCallbackInfo
---

# Layout

## Overview

Azul lays out content according to the W3C browser specifications, so this page will just give a main overview of the *differences* of Azul vs Google Chrome, which mainly revolves around APIs for measuring / retrieving layout. The layout modes (box, inline, flex, grid, table, ...) are exactly modeled after the W3C specification, any deviation from that can be reported as a bug.

If you already know how CSS values work, then you can skip all sub-pages referenced here and only read this page. The sub-pages only contain examples and descriptions of the various main layout modes, which are:

- **Block** - default, full-width stacked boxes. See
  [Blocks, Sizing, and Positioning](layout/blocks.md).
- **Inline** - text and inline-block runs inside a block. See [Inline Layout](layout/inline.md).
- **Flex** - one-axis containers (rows or columns). See [Flexbox](layout/flex.md).
- **Grid** - two-axis containers (columns and rows). See [Grid](layout/grid.md).

Please also see the way better documentation on HTML layout modes:

- [MDN - Box Model](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Box_model): extensive documentation, however Azul does not support all properties, only common ones
- [W3CSchools - Box Model](https://www.w3schools.com/css/css_boxmodel.asp): simpler, but more approachable documentation
- [Every Layout](https://every-layout.dev/rudiments/boxes/) - explains the box model more visually

Azul does support (with tests) even advanced writing modes such as vertical text in various directions, as well as of course RTL text (which affects properties such as `start` and `end`). Currently, there is however no support of `<ruby>` text.

## Adding stylesheets

In the previous documentation, and even in the hello-world example, you already came across
the single way stylesheets are added to the `Dom`: setting `.with_css("...")`. However, it's
very important to see what that does internally:

```rust
impl Dom {

    pub fn with_css(mut self, style: &str) -> Self {
        self.set_css(style);
        self
    }
    
    pub fn set_css(&mut self, style: &str) {
        self.add_component_css(Css::parse_inline(style));
    }

    pub fn add_component_css(&mut self, css: Css) {
        self.css.push(css);
    }
}
```

Azul does not immediately cascade the CSS and applies it to the nodes, instead it 
"stores" the CSS in an order and then applies / cascades it later in one go. Another
difference is that there is no `<style>` node like in HTML, as the CSS stylesheet can
only cascade within one subtree, not affect anything outside of it. 

This is crucial for components, so you don't have to hack together anything like 
[BEM](https://css-tricks.com/bem-101/) or generate unique IDs for your styling blocks, 
so that they don't collide with other styling rules.

Another difference is that, as seen above, the "inline style" can take a *full* CSS
stylesheet verbatim, including `@media` rules, `:hover`, `:focus` and `:active` rules,
which browsers famously cannot do:

```rust
let dom = Dom::create_div()
    .with_css("
        @media (max-width: 600px) {
          * {
            background-color: lightblue;
          }
        }
    ");
```

So, here this `*` applies only to this div and will style the background 
color as `lightblue` if the width of the window. You can also use 
`@container` to make the rule based on the size of the div instead.

A third difference is that you can "stack" entire stylesheets on top of each 
other within a single subtree: the rule is that the last-applied stylesheet wins:

```rust
// red background gets stacked on top, replaces blue background
let dom = Dom::create_div()
    .with_css("background:blue")
    .with_css("background:red");
```

Notably, this stacking happens *before* cascading happens, which allows designers to 
override the default widget stylesheet configuration before the properties of the 
widget component are applied to the subtree.

## Measuring layout

A big issue when layouting items in a `VirtualView` callback (see [Virtual Views](dom/virtual-views.md)) 
is that you need to know or at least estimage the size of a DOM tree before actually rendering it in a 
layout, and you need to "readjust" the scrolling position once you know the actual size more accurately.
For example, you might want to have a file browser view with 100.000 images (which all have different aspect ratios),
but only materialize 100 images. But to determine the scroll positions, you need to calculate the scroll position,
which is usually only know *after* the layout is finished.

To solve this problem, you can use the ``

## Performance and relayouting

As a general rule of thumb, parsing CSS stylesheets is very fast as it does not use a lot of allocations.
You can
