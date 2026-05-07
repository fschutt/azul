---
slug: dom/virtual-views
title: Virtual Views
language: en
canonical_slug: dom/virtual-views
audience: external
maturity: mature
guide_order: 34
topic_only: false
short_desc: A node that materialises lazily, for infinite lists and embedded sub-DOMs
prerequisites: [dom]
tracked_files:
  - core/src/dom.rs
  - core/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Virtual Views

A `VirtualView` is a single node. Its inner content comes from a separate
callback. That callback only runs when needed.

It's azul's iframe-equivalent. Use it when the inner content would be too
expensive to build every frame. Common cases:

- An infinite list.
- A 100,000-row table.
- An editor with heavy decoration layers.
- A panel that's logically independent from its surroundings.

The outer `Dom` containing the `VirtualView` is built inside `layout()` like
any other node. The inside isn't built until the framework calls the
callback.

```rust,no_run
# use azul::prelude::*;
struct ListData { items: Vec<String> }

extern "C" fn render_list(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let d = match data.downcast_ref::<ListData>() { Some(d) => d, None => return VirtualViewReturn::default() };
    let dom: Dom = d.items.iter().map(|s| Dom::create_li_with_text(s.clone())).collect();
    let row_h = 24.0_f32;
    let total = LogicalSize::create(info.bounds.logical_size.width, row_h * d.items.len() as f32);
    VirtualViewReturn::with_dom(dom, total, LogicalPosition::zero(), total, LogicalPosition::zero())
}

let list_state = RefAny::new(ListData { items: vec!["A".into(), "B".into()] });
let cb = VirtualViewCallback { cb: render_list, ctx: OptionRefAny::None };
let _ = Dom::create_virtual_view(list_state, cb);
```

## Two coordinate systems: rendered vs virtual

The key idea is that the rendered size and the virtual size are separate
numbers. That's what lets a scrollbar pretend to span 30,000 rows while
only 30 rows actually live in the DOM.

`VirtualViewReturn` carries two pairs:

- `scroll_size` and `scroll_offset` describe the actual rendered content.
  This is the box of DOM you're handing back. `scroll_offset` is where it
  sits in virtual coordinates.

- `virtual_scroll_size` and `virtual_scroll_offset` describe what the view
  pretends to be. `virtual_scroll_size` is the size the scrollbar
  represents. `virtual_scroll_offset` is usually `LogicalPosition::zero()`.

If every row is materialised, the rendered values match the virtual values.
The abstraction collapses to a plain scrollable subtree.

For a virtualised slice, say rows 10..30 of a million-row table:

- `scroll_size` is `(width, 20 * row_height)`. That's the 20 actual rows.
- `scroll_offset` is `(0, 10 * row_height)`. The rendered rows start at
  y = 10 × row_height in virtual coordinates.
- `virtual_scroll_size` is `(width, 1_000_000 * row_height)`. The
  scrollbar represents the whole table.
- `virtual_scroll_offset` is `(0, 0)`.

The framework paints the scrollbar from `virtual_scroll_size`. It clips the
rendered DOM to a viewport at `scroll_offset` of size `scroll_size`. If the
user scrolls outside the rendered window, it re-invokes the callback.

## Why the callback was invoked

Each invocation carries a `VirtualViewCallbackReason`. The variants are
defined in `core/src/callbacks.rs`:

- `InitialRender`. The first time the `VirtualView` appears. You always
  return content here.

- `DomRecreated`. The parent tree was rebuilt from scratch. The cached
  subtree was thrown away. The callback has to rebuild.

- `BoundsExpanded`. The window grew. The view's bounds now exceed its
  previous `scroll_size`. Time to enlarge the rendered content. This fires
  once per expansion, not on every resize tick.

- `EdgeScrolled(EdgeType)`. The user scrolled within approximately
  200 px of one of the four edges of the rendered content. Time to
  lazy-load more rows. `EdgeType` is one of `Top`, `Bottom`, `Left`,
  `Right`. Fires once per edge approach. The flag clears once the scroll
  moves away.

- `ScrollBeyondContent`. A programmatic scroll (e.g. `set_scroll_position`)
  jumped the offset past the rendered `scroll_size`.

The callback does not fire on small resizes that stay inside the rendered
`scroll_size`. It does not fire when the window shrinks. It does not fire
on parent re-renders that don't recreate the parent DOM. That's the
optimisation.

## Returning OptionDom::None

Sometimes the existing DOM is fine. The user scrolled, but stayed inside
the already-rendered area. Or the parent re-rendered without invalidating
the subtree.

Set `dom: OptionDom::None` in the return value. The previous DOM stays in
place. Only the scroll bounds are updated.

```rust,ignore
fn render_table(data: &mut TableData, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    if data.already_rendered_area_covers(info.scroll_offset, info.bounds.logical_size) {
        return VirtualViewReturn {
            dom: OptionDom::None,                  // keep current DOM
            scroll_size:  data.current_scroll_size,
            scroll_offset: data.current_scroll_offset,
            virtual_scroll_size: data.virtual_size,
            virtual_scroll_offset: LogicalPosition::zero(),
        };
    }
    let new_dom = data.render_more_rows(info.scroll_offset, info.bounds.logical_size);
    /* ... */
    # VirtualViewReturn::default()
}
```

`VirtualViewReturn::keep_current(...)` is a shortcut for the same return
shape.

This is the cheapest re-invocation. The most expensive ones are
`InitialRender` and `DomRecreated`, since both rebuild from nothing.

## A virtualised table, end to end

```rust,ignore
struct TableData {
    total_rows: usize,
    row_height: f32,
    visible_rows: Vec<Row>,        // currently rendered
    first_visible_row: usize,
}

extern "C" fn table_render(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let mut tdata = match data.downcast_mut::<TableData>() {
        Some(d) => d, None => return VirtualViewReturn::default(),
    };
    let container_h = info.bounds.logical_size.height;
    let scroll_y = info.scroll_offset.y;

    // Which rows should we render?
    let first = (scroll_y / tdata.row_height) as usize;
    let count = (container_h / tdata.row_height).ceil() as usize + 2; // +2 buffer

    tdata.visible_rows = tdata.fetch_rows(first, count);
    tdata.first_visible_row = first;

    let dom: Dom = tdata.visible_rows.iter().map(|r| {
        Dom::create_div()
            .with_child(Dom::create_text(r.text.clone()))
            .with_css(format!("height: {}px;", tdata.row_height))
    }).collect();

    VirtualViewReturn {
        dom: OptionDom::Some(dom),
        scroll_size: LogicalSize::create(
            info.bounds.logical_size.width,
            tdata.visible_rows.len() as f32 * tdata.row_height,
        ),
        scroll_offset: LogicalPosition::create(
            0.0, first as f32 * tdata.row_height,
        ),
        virtual_scroll_size: LogicalSize::create(
            info.bounds.logical_size.width,
            tdata.total_rows as f32 * tdata.row_height,
        ),
        virtual_scroll_offset: LogicalPosition::zero(),
    }
}
```

The user sees a scrollbar that represents all million rows. The DOM holds
about 25 row nodes. The callback runs only when the user scrolls near an
edge, the parent rebuilds, or the window grows. It doesn't run every
frame.

## Pairing with merge callbacks

The data parameter on a `VirtualView` is a `RefAny`. The framework keeps
it alive across re-renders of the parent DOM. The inner `Dom` returned by
the callback is rebuilt each time the callback fires.

If the inner content owns expensive resources (per-row decoders, GL
textures), pair each row's root with [`with_dataset(...)`](datasets.md)
and a [merge callback](merge-callbacks.md). That way the resources
transfer when the table is re-rendered.

The outer `RefAny` is also a good place to keep live state: what was
rendered last time, the cached fetcher, in-flight requests. The callback
can look at that state before deciding whether to return `OptionDom::None`.


## Coming Up Next

- [Components](components.md) — Reusable UI fragments - named functions of (args) -> Dom
- [Scrolling](../scrolling-and-drag.md) — Scroll containers, drag-and-drop, hit testing
- [Layout](../layout.md) — Overview of the layout solver
