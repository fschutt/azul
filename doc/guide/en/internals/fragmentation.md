---
slug: fragmentation
title: Fragmentation
language: en
canonical_slug: fragmentation
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Page breaks, widows, orphans, and PDF fragmentation
prerequisites: [layout-solver, inline-text3]
tracked_files:
  - layout/src/lib.rs
  - layout/src/window.rs
  - layout/src/fragmentation.rs
  - layout/src/paged.rs
  - layout/src/solver3/pagination.rs
  - layout/src/solver3/paged_layout.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:55:41Z
---

# Fragmentation

> **WIP** — two parallel pagination paths exist. The active production path is the "infinite-canvas with physical spacers" model in `solver3/pagination.rs` + `solver3/paged_layout.rs`. The original `layout/src/fragmentation.rs` (CSS css-break-3 integrated splitting) has been mostly superseded but its types are still public and re-exported.

CSS Fragmentation Module Level 3 ([css-break-3](https://www.w3.org/TR/css-break-3/)) covers breaking content across pages, columns, and regions. Azul implements paged media for PDF generation; column and region fragmentation are scaffolded but not active.

## Two implementations, briefly

- **`layout/src/fragmentation.rs`.** Partially superseded. CSS-spec-style "decide breaks during layout". Defines `FragmentationLayoutContext`, `BoxBreakBehavior`, and `BreakPoint`.
- **`layout/src/paged.rs`.** Active container model. The `FragmentationContext` enum has `Continuous`, `Paged`, `MultiColumn`, and `Regions` variants. Defines `Fragmentainer` and `FragmentationState`.
- **`layout/src/solver3/pagination.rs`.** Active. `PageGeometer` is the infinite-canvas coordinate model. `FakePageConfig` handles header and footer config without `@page` parsing.
- **`layout/src/solver3/paged_layout.rs`.** Active. `layout_document_paged` drives the paged path. It lays out once on a tall canvas, splits into pages by Y position, and filters the display list.

The note on `fragmentation.rs:23` says explicitly: *"`solver3/pagination.rs` provides an alternative page-layout implementation with its own `PageGeometer`, `PageTemplate`, and `PageMargins`. See that module for the currently active paged-layout pipeline."* The original design proposed integrated mid-layout splitting; the implementation took the simpler post-hoc filter approach (visible in commit history and in `paged_layout.rs:1` "page_index is assigned to nodes DURING layout based on Y position").

## Active path: infinite canvas with physical spacers

`solver3/pagination.rs` lays content out on a single tall canvas, with "dead zones" between pages representing margins, headers, and footers:

```
0px      ─────────────────────────────
         │ Page 1 Content             │
1000px   ─────────────────────────────
         │ Dead Space (Footer+Margin) │   ← page break zone
1100px   ─────────────────────────────
         │ Page 2 Content             │
2100px   ─────────────────────────────
         │ Dead Space (Footer+Margin) │
2200px   ─────────────────────────────
```

The advantage: the existing block/inline solver runs unchanged on the tall canvas. The downside: `break-inside: avoid` and orphans/widows aren't honoured by the layout — the splitter has to do its best after the fact. CSS `@page` rules aren't parsed yet (`FakePageConfig` is the programmatic surrogate).

## `FragmentationContext` (paged.rs)

```rust,ignore
pub enum FragmentationContext {
    Continuous {
        width: f32,
        container: Fragmentainer,         // grows infinitely
    },
    Paged {
        page_size: LogicalSize,
        pages: Vec<Fragmentainer>,        // fixed-size pages
    },
    MultiColumn {
        column_width: f32,
        column_height: f32,
        gap: f32,
        columns: Vec<Fragmentainer>,
    },
    Regions {
        regions: Vec<Fragmentainer>,      // pre-defined; cannot grow
    },
}
```

`MultiColumn` and `Regions` exist as enum variants but are not yet driven by any layout path. `Continuous` and `Paged` are the only ones that production code constructs.

A `Fragmentainer` tracks `size`, `used_block_size`, `is_fixed_size`. `remaining_space()` returns `f32::MAX` for non-fixed (continuous) and `(size.height - used).max(0.0)` for fixed (pages). `advance()` creates the next fragmentainer; for `Continuous` it's a no-op (containers grow), for `Regions` it returns `Err` if no more regions exist.

## `FragmentationState`

`paged.rs:287`. The simpler per-layout-pass tracker for paged mode. Doesn't own fragmentainers; just tracks `current_page`, `current_page_y`, `available_height`, and `page_content_height`.

```rust,ignore
pub struct FragmentationState {
    pub current_page: usize,
    pub current_page_y: f32,
    pub available_height: f32,
    pub page_content_height: f32,
    pub margins_top: f32,
    pub margins_bottom: f32,
    pub total_pages: usize,
}
```

Helpers: `can_fit(height)`, `would_fit_on_empty_page(height)`, `use_space(height)`, `advance_page()`, `page_for_y(y) -> usize`, `page_y_offset(page) -> f32`.

`page_for_y` is the splitter: given the absolute Y position of a display-list item, it computes which page the item belongs on. `paged_layout.rs:layout_document_paged` uses this to build per-page display lists from the single tall layout pass.

## Driving paged layout (`solver3/paged_layout.rs`)

```rust,ignore
#[cfg(feature = "text_layout")]
pub fn layout_document_paged<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    fragmentation_context: FragmentationContext,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &mut FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
    font_loader: F,
    image_cache: &ImageCache,
    get_system_time_fn: GetSystemTimeCallback,
) -> Result<Vec<DisplayList>>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(Arc<FontBytes>, usize) -> std::result::Result<T, LayoutError>;
```

Returns one `DisplayList` per page. Internally:

1. Run normal `layout_document` against a `viewport` whose height is `f32::MAX` (the infinite canvas).
2. Compute `page_size` and per-page geometry from `FragmentationContext::Paged.page_size` plus `FakePageConfig`.
3. Walk `display_list.items` and split by `page_for_y(item.bounds.origin.y)`. Y coordinates are converted to page-relative by subtracting `page_y_offset(page)`.
4. For each page, append header/footer items from `FakePageConfig`.

Re-exports through `layout/src/lib.rs`:

```rust,ignore
pub use solver3::paged_layout::layout_document_paged;
pub use paged::FragmentationState;
pub use fragmentation::{
    BoxBreakBehavior, BreakDecision, FragmentationDefaults, FragmentationLayoutContext,
    KeepTogetherPriority, PageCounter, PageFragment, PageMargins, PageNumberStyle, PageSlot,
    PageSlotContent, PageSlotPosition, PageTemplate,
};
```

## `FakePageConfig` and page templates

`solver3/pagination.rs:FakePageConfig` is the programmatic substitute for unparsed `@page` rules. Configures:

- Page size, margins, header height, footer height
- Per-slot dynamic content via `PageSlotContent` (Text, PageNumber, PageOfTotal, RunningHeader, Dynamic closure)
- Six slot positions (`PageSlotPosition::TopLeft`, `TopCenter`, `TopRight`, `BottomLeft`, `BottomCenter`, `BottomRight`)
- Optional left/right (verso/recto) overrides via `PageTemplate::slots_for_page(page_number)` which selects between `slots`, `left_page_slots`, and `right_page_slots`
- `header_on_first_page` / `footer_on_first_page` toggles for cover-page styling

`PageNumberStyle` covers `Decimal`, `LowerRoman`, `UpperRoman`, `LowerAlpha`, `UpperAlpha`. `PageCounter::format_page_number(style)` produces the string; `format_page_of_total()` renders "Page X of Y".

`PageSlotContent::Dynamic(Arc<DynamicSlotContentFn>)` lets a caller produce per-page content from a closure:

```rust,ignore
let func = DynamicSlotContentFn::new(|counter| {
    format!("Page {}", counter.page_number)
});
let content = PageSlotContent::Dynamic(Arc::new(func));
```

`Send + Sync` is required because the function is `Arc`-shared across pages.

## CSS break properties (defined, partially honoured)

From `azul_css::props::layout::fragmentation`, defined and parseable, but only partially consumed:

- The `break-before` and `break-after` properties accept `PageBreak`. Honoured by `BreakPoint::is_forced()` in `fragmentation.rs`. Not consumed by the paged splitter.
- The `break-inside` property accepts `BreakInside` (`Auto` or `Avoid`). Honoured by `FragmentationLayoutContext::break_inside_avoid_depth` (the counter increments on entry). Not consumed by the paged splitter.
- The `orphans` property accepts a `u32` (default 2). Defined but not enforced by Knuth-Plass.
- The `widows` property accepts a `u32` (default 2). Defined but not enforced by Knuth-Plass.
- The `box-decoration-break` property accepts `BoxDecorationBreak` (`Slice` or `Clone`). Defined but not honoured.

`BoxBreakBehavior` (`fragmentation.rs:351`) classifies a box's break behaviour:

```rust,ignore
pub enum BoxBreakBehavior {
    Splittable {
        min_before_break: f32,
        min_after_break: f32,
    },
    KeepTogether {
        estimated_height: f32,
        priority: KeepTogetherPriority,
    },
    Monolithic {
        height: f32,
    },
}
```

`KeepTogetherPriority`: `Low | Normal | High | Critical`. Headers with following content are `High`, figures with captions are `Critical`. The original design used these to drive break decisions during layout; the active path doesn't read them yet.

## `FragmentationDefaults` (heuristics)

`fragmentation.rs:537`. The "smart" defaults that the integrated splitter would apply when CSS doesn't dictate otherwise:

```rust,ignore
pub struct FragmentationDefaults {
    pub keep_headers_with_content: bool,         // default true
    pub min_paragraph_lines: u32,                 // default 3
    pub keep_figures_together: bool,              // default true
    pub keep_table_headers: bool,                 // default true
    pub keep_list_markers: bool,                  // default true
    pub small_block_threshold_lines: u32,         // default 3
    pub default_orphans: u32,                     // default 2
    pub default_widows: u32,                      // default 2
}
```

These are exposed but unused by `paged_layout.rs`. Implementing them requires switching to integrated splitting (the original design) or layering them on top of the post-hoc splitter.

## Page templates and verso/recto

`PageTemplate::slots_for_page(page_number)` picks the slot list:

```rust,ignore
let override_slots = if page_number % 2 == 0 {
    self.left_page_slots.as_deref()       // verso (even)
} else {
    self.right_page_slots.as_deref()       // recto (odd)
};
override_slots.unwrap_or(&self.slots)
```

`FragmentationLayoutContext::advance_to_left_page` and `advance_to_right_page` insert blank pages as needed to land on an even or odd page (chapter-start convention in print typography).

## Inline content flow across fragments

The text engine's stage-5 line breaker (`text3/cache.rs:layout_flow`) accepts `flow_chain: &[LayoutFragment]`. A `BreakCursor` records where one fragment stopped; the next fragment continues from there. This is how text would flow across columns or pages without re-shaping. Today only the paged splitter uses it indirectly (one big fragment per layout pass, then split by Y); column layout is not wired up.

## Activating fragmentation in `LayoutWindow`

`LayoutWindow::new_paged(fc_cache, page_size)` (`window.rs:702`, behind `feature = "pdf"`) constructs a window with `fragmentation_context: FragmentationContext::new_paged(page_size)`. The paged layout path then calls `layout_document_paged` instead of `layout_document`. The screen path (`new` and `new_with_shared_fonts`) uses `FragmentationContext::new_continuous(800.0)` which makes `paged_layout.rs` a no-op (one page, infinite height).

## Known divergence from the original design

The original design (`layout/src/fragmentation.rs`) called for break decisions made *during* layout: as content is laid out, check `can_fit(height)`, apply `break-before`/`break-after` rules, defer or split `KeepTogether` blocks, enforce orphans/widows. The implementation in `solver3/paged_layout.rs` lays content out continuously and splits afterwards, which is simpler but cannot honour `break-inside: avoid` or orphans/widows correctly. The original design proposed `BreakDecision` and `BreakPoint::is_allowed()` to drive integrated splitting; those types remain public and re-exported from `lib.rs`, but no caller consumes them in the paged path.

If a contributor wants integrated splitting, the path forward is:

1. Wire `FragmentationContext` into `LayoutContext` (already done — `LayoutContext::fragmentation_context: Option<&'a mut FragmentationContext>`).
2. In `solver3/fc.rs:layout_bfc`, before placing each child, check `ctx.fragmentation_context` and call `can_fit(child_height)`. If false, advance the fragmentainer, leave a gap, and re-issue the layout with the child placed at the new page Y.
3. In `solver3/fc.rs:layout_ifc`, plumb the fragment list through to `text_cache.layout_flow(flow_chain: &[LayoutFragment])` so Knuth–Plass produces fragment-aware breaks.

Today neither step is done; `LayoutContext.fragmentation_context` is always `None` in production calls.

## Coming Up Next

- [Text Shaping](inline-text3.md) — The text3 engine - shaping, line breaking, BiDi, hyphenation
- [Layout Solver (Flex/Grid)](layout-solver.md) — Architecture of `solver3/` and how the engines share state
- [Text Pipeline](text-pipeline.md) — How a styled text run becomes glyphs
