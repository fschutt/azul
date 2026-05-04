---
slug: dom/virtual-views
title: Virtual Views
language: en
canonical_slug: dom/virtual-views
audience: external
maturity: mature
guide_order: 34
topic_only: false
short_desc: A node whose contents come from a callback that runs only when the framework needs them — the mechanism for infinite lists, lazy panels, and embedded sub-DOMs that own their own scroll math.
prerequisites: [dom]
tracked_files:
  - core/src/dom.rs
  - core/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Virtual Views

A `VirtualView` is azul's iframe-equivalent: a single node whose
contents are produced by a *separate callback* that the framework only
invokes when it actually needs the content. Use it when:

- The full content would be expensive to build every frame — an
  infinite list, a 100,000-row table, an editor with many decoration
  layers.
- The content is genuinely independent from the rest of the layout —
  a settings panel that should not re-render when the surrounding
  toolbar updates.
- You want explicit control over the scroll model — separating the
  *rendered* size from the *virtual* size, so a scrollbar can pretend
  to span 30,000 rows while only 30 are actually in the DOM.

```rust,no_run
# use azul::prelude::*;
struct ListData { items: Vec<String> }

extern "C" fn render_list(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let d = match data.downcast_ref::<ListData>() { Some(d) => d, None => return VirtualViewReturn::default() };
    let dom: Dom = d.items.iter().map(|s| Dom::li_with_text(s.clone())).collect();
    let row_h = 24.0_f32;
    let total = LogicalSize::create(info.bounds.logical_size.width, row_h * d.items.len() as f32);
    VirtualViewReturn::with_dom(dom, total, LogicalPosition::zero(), total, LogicalPosition::zero())
}

let list_state = RefAny::new(ListData { items: vec!["A".into(), "B".into()] });
let _ = Dom::create_virtual_view(list_state, VirtualViewCallback::create(render_list));
```

The outer `Dom` containing the `VirtualView` is built inside `layout()`
like any other tree. The *inside* of the `VirtualView` only materialises
when the framework calls `render_list`.

## When the framework invokes the callback

`VirtualViewCallbackReason` (`core/src/callbacks.rs:181`) tells the
callback why it was called:

- **`InitialRender`** — the first time the `VirtualView` appears in
  layout. You always render content here.
- **`DomRecreated`** — the parent tree was rebuilt from scratch (not
  just re-laid-out). The framework lost the cached subtree, so the
  callback has to rebuild it.
- **`BoundsExpanded`** — the window grew and the `VirtualView`'s bounds
  now exceed its previous `scroll_size`. The callback is invited to
  enlarge its rendered content. This fires *once* per expansion, not on
  every resize tick.
- **`EdgeScrolled(EdgeType)`** — the user scrolled within
  `EDGE_THRESHOLD` (200 px) of one of the four edges of the rendered
  content. Time to lazy-load more rows or fetch the next page. Fires
  *once* per edge approach; the flag clears when the scroll moves away.
- **`ScrollBeyondContent`** — programmatic scroll
  (`set_scroll_position`) jumped the offset past the rendered
  `scroll_size`. Same constraints as the threshold rule.

The callback does *not* fire on small resizes that stay within the
already-rendered `scroll_size`, on shrinking the window, or on parent
re-renders that don't recreate the parent DOM. That's the optimisation
the abstraction is built around.

## Two coordinate systems: rendered vs virtual

The `VirtualViewReturn` value carries two size/offset pairs. The
distinction is what makes "30,000 rows of scrollbar, 30 rows of DOM"
work without the framework needing to know what's behind the
abstraction:

- **`scroll_size`** + **`scroll_offset`** describe the *actual rendered
  content*. This is the box of DOM you're handing back, where it sits
  in virtual coordinates, and how big it is.
- **`virtual_scroll_size`** + **`virtual_scroll_offset`** describe what
  the `VirtualView` *pretends* to have — the size the scrollbar should
  represent, and where the visible window's top-left sits in that
  pretended space.

For a non-virtualised `VirtualView` (every row materialised), the
rendered and virtual values match — the abstraction collapses to a
plain scrollable subtree.

For a virtualised list — say, rows 10..30 of a million-row table — the
shape is:

- `scroll_size` = `(width, 20 * row_height)` — the 20 actual rows.
- `scroll_offset` = `(0, 10 * row_height)` — those 20 rows start at
  y = 10 × row_height in virtual coords.
- `virtual_scroll_size` = `(width, 1_000_000 * row_height)` — what the
  scrollbar represents.
- `virtual_scroll_offset` = `(0, 0)` — usually origin.

Read the framework's interpretation as: "the scrollbar paints based on
`virtual_scroll_size`; the rendered content is clipped to a viewport
that lives at `scroll_offset` and is `scroll_size` big; if the user
scrolls outside the rendered window, re-invoke the callback."

## Skipping unnecessary work — `OptionDom::None`

If the callback determines that the current rendered window is still
fine (the user scrolled, but stayed inside the already-rendered area;
or a parent re-rendered without invalidating this subtree), it can
return `OptionDom::None`. The framework keeps the previous DOM and only
updates the scroll bounds.

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

This is the cheapest re-invocation. The most expensive — `InitialRender`
or `DomRecreated` — has to rebuild from nothing.

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

The user sees a scrollbar that represents all million rows. The DOM
contains ~25 row nodes. The callback fires only when the user scrolls
near an edge, the parent rebuilds, or the window grows — not on every
frame.

## Pairing with merge callbacks

A `VirtualView`'s data parameter is a `RefAny` that the framework keeps
alive across re-renders of the *parent* DOM, but the inner `Dom` returned
by the callback is rebuilt each time the callback fires. If the inner
content owns expensive resources (per-row decoders, GL textures), pair
each row's root with [`with_dataset(...)`](datasets.md) and a
[merge callback](merge-callbacks.md) so the resources transfer when the
table is re-rendered.

The `VirtualView`'s outer `RefAny` (the data parameter) is the natural
place to *also* keep the live state — what's rendered now, the cached
fetcher, in-flight requests — so the callback can look at "what we
rendered last time" before deciding whether to return `OptionDom::None`.

## Where to read the source

- `core/src/dom.rs:2195` — `NodeData::create_virtual_view`
- `core/src/dom.rs:3627` — `Dom::create_virtual_view`
- `core/src/callbacks.rs:181` — `VirtualViewCallbackReason`
- `core/src/callbacks.rs:204` — `VirtualViewCallbackInfo`
- `core/src/callbacks.rs:307` — `VirtualViewReturn`
