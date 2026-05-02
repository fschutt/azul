---
slug: fragmentation
title: Fragmentation
language: en
canonical_slug: fragmentation
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: [code-organization, layout-solver, css-properties]
tracked_files:
  - layout/src/fragmentation.rs
  - layout/src/paged.rs
  - layout/src/solver3/pagination.rs
  - layout/src/solver3/paged_layout.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:32:10Z
---

# Fragmentation

> **WIP** — paged layout currently has two parallel implementations. `solver3::pagination` is the active engine used by `layout_document_paged`; `layout/src/fragmentation.rs` is the older break-classification machinery, kept for break-before/after rule resolution and headers/footers. They will eventually merge.

Fragmentation is azul's CSS [css-break-3](https://www.w3.org/TR/css-break-3/) implementation: turning a continuous logical document into one display list per page, column, or region. It runs as part of normal layout — there is no post-hoc splitter. Content's page assignment is computed *during* layout based on its absolute Y position on an "infinite canvas with physical spacers."

The active entry point is [`solver3::paged_layout::layout_document_paged`](../../../../layout/src/solver3/paged_layout.rs) at `solver3/paged_layout.rs:67`. It returns `Vec<DisplayList>` — one per page, with Y coordinates relative to the page's own origin.

```rust,ignore
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
```

## FragmentationContext

[`paged::FragmentationContext`](../../../../layout/src/paged.rs) at `paged.rs:34` selects the fragmentainer geometry:

```rust,ignore
pub enum FragmentationContext {
    Continuous {  // screen rendering, never breaks
        width: f32,
        container: Fragmentainer,
    },
    Paged {       // print / PDF
        page_size: LogicalSize,
        pages: Vec<Fragmentainer>,
    },
    MultiColumn { /* future */ },
    Regions     { /* future */ },
}
```

`Continuous` is the default for screen — the layout solver runs unchanged and produces one display list. `Paged` triggers the paged path. `MultiColumn` and `Regions` are placeholder variants; the runtime is not yet wired up for either.

## Infinite canvas with physical spacers

The active paged engine in [`solver3::pagination`](../../../../layout/src/solver3/pagination.rs) lays out the entire document on a single tall vertical canvas, with "dead zones" where page breaks would land:

```text
0px      ─────────────────────────────
         │ Page 1 Content             │
1000px   ─────────────────────────────
         │ Dead Space (Footer+Margin) │  ← Page break zone
1100px   ─────────────────────────────
         │ Page 2 Content             │
2100px   ─────────────────────────────
         │ Dead Space (Footer+Margin) │
2200px   ─────────────────────────────
```

Each block-flow element computes its Y position normally; the dead-zone spacers nudge content past the page boundary when it would otherwise sit on a break. The advantage: every existing layout algorithm (block, flex, grid, inline, table) keeps working unchanged. Only the post-layout slicer needs to know about pages.

[`PageGeometer`](../../../../layout/src/solver3/pagination.rs) at `pagination.rs:56` owns the page-size + margins + header/footer state:

```rust,ignore
pub struct PageGeometer {
    pub page_size: LogicalSize,
    pub page_margins: PageMargins,
    pub header_height: f32,
    pub footer_height: f32,
    pub current_y: f32,
}
```

`current_y` advances as content is placed. When a block doesn't fit on the current page, the engine emits a spacer to the next page boundary and re-positions the block at the new `current_y`.

## Break behavior classification

[`fragmentation::BoxBreakBehavior`](../../../../layout/src/fragmentation.rs) at `fragmentation.rs:351` is the policy each layout box reports:

```rust,ignore
pub enum BoxBreakBehavior {
    Splittable {                        // paragraphs, generic containers
        min_before_break: f32,           // orphans-like
        min_after_break: f32,            // widows-like
    },
    KeepTogether {                      // headers + following content
        estimated_height: f32,
        priority: KeepTogetherPriority,  // Low | Normal | High | Critical
    },
    Monolithic {                        // images, replaced elements
        height: f32,
    },
}
```

`KeepTogetherPriority::Critical` is reserved for figure+caption pairs and table-header+first-row pairs. `Normal` is the default for `break-inside: avoid`. The classifier walks the DOM once and emits a `BoxBreakBehavior` per box; the slicer uses this when picking which break point to take.

## Break points

[`BreakPoint`](../../../../layout/src/fragmentation.rs) at `fragmentation.rs:387` records every legal break opportunity:

```rust,ignore
pub struct BreakPoint {
    pub y_position: f32,
    pub break_class: BreakClass,         // ClassA | ClassB | ClassC
    pub break_before: PageBreak,
    pub break_after: PageBreak,
    pub ancestor_avoid_depth: usize,     // > 0 → break-inside: avoid in scope
    pub preceding_node: Option<NodeId>,
    pub following_node: Option<NodeId>,
}
```

The three classes follow CSS-Break-3 §3.1:

- **Class A** — between sibling block-level boxes
- **Class B** — between line boxes inside a block container
- **Class C** — between content edge and child margin edge

`is_allowed()` enforces the precedence rules: forced breaks (`page`, `always`) always allowed, `avoid` blocks the break, ancestor `break-inside: avoid` propagates downward via `ancestor_avoid_depth`. Orphans/widows are checked at a higher level by the slicer, not at the break-point level.

## Page templates

[`PageTemplate`](../../../../layout/src/fragmentation.rs) at `fragmentation.rs:223` describes headers, footers, and running content:

```rust,ignore
pub struct PageTemplate {
    pub header_height: f32,
    pub footer_height: f32,
    pub slots: Vec<PageSlot>,
    pub header_on_first_page: bool,
    pub footer_on_first_page: bool,
    pub left_page_slots: Option<Vec<PageSlot>>,   // even pages
    pub right_page_slots: Option<Vec<PageSlot>>,  // odd pages
}

pub struct PageSlot {
    pub position: PageSlotPosition,  // TopLeft | TopCenter | TopRight | Bottom*
    pub content: PageSlotContent,
    pub font_size_pt: Option<f32>,
    pub color: Option<ColorU>,
}

pub enum PageSlotContent {
    Text(String),
    PageNumber(PageNumberStyle),     // Decimal | Lower/UpperRoman | Lower/UpperAlpha
    PageOfTotal,
    RunningHeader(String),
    Dynamic(Arc<DynamicSlotContentFn>),
}
```

Builder helpers cover the common cases:

```rust,ignore
let template = PageTemplate::new()
    .with_book_header("Chapter 1".to_string(), 30.0)
    .with_page_number_footer(20.0);
```

`PageTemplate::content_area_height(page_height, page_number)` returns the usable height after subtracting header + footer. `slots_for_page(page_number)` honors the left/right alternation.

## Page counters

[`PageCounter`](../../../../layout/src/fragmentation.rs) at `fragmentation.rs:56` tracks the running page number, optional total, optional chapter number, and a `BTreeMap<String, i32>` of named counters (CSS `counter()` function):

```rust,ignore
pub struct PageCounter {
    pub page_number: usize,
    pub total_pages: Option<usize>,
    pub chapter: Option<usize>,
    pub named_counters: BTreeMap<String, i32>,
}
```

`format_page_number(PageNumberStyle::LowerRoman)` produces `"iii"` for page 3. `format_page_of_total()` produces `"Page 3 of 12"` when `total_pages.is_some()`, or `"Page 3"` otherwise (the total is unknown during the first pass).

`PageNumberStyle::LowerAlpha` and `UpperAlpha` go past `z` / `Z` with `aa`, `ab`, … (spreadsheet column scheme).

## FragmentationLayoutContext

[`FragmentationLayoutContext`](../../../../layout/src/fragmentation.rs) at `fragmentation.rs:464` is the per-pass scratch state passed alongside `LayoutContext` when fragmentation is active:

```rust,ignore
pub struct FragmentationLayoutContext {
    pub page_size: LogicalSize,
    pub margins: PageMargins,
    pub template: PageTemplate,
    pub current_page: usize,
    pub current_y: f32,
    pub available_height: f32,
    pub page_content_height: f32,
    pub break_inside_avoid_depth: usize,
    pub orphans: u32,
    pub widows: u32,
    pub fragments: Vec<PageFragment>,
    pub counter: PageCounter,
    pub defaults: FragmentationDefaults,
    pub break_points: Vec<BreakPoint>,
    pub avoid_break_before_next: bool,
}
```

`break_inside_avoid_depth` increases when entering a subtree with `break-inside: avoid` and decreases on exit. While > 0, no `BreakPoint::is_allowed()` returns `true` (except forced breaks).

`avoid_break_before_next` is set by `break-after: avoid` on the previous sibling — it suppresses the next available break opportunity.

## Defaults — the smart layer

[`FragmentationDefaults`](../../../../layout/src/fragmentation.rs) at `fragmentation.rs:537` controls heuristics that go beyond the spec:

```rust,ignore
pub struct FragmentationDefaults {
    pub keep_headers_with_content: bool,    // h1–h6 stick to following block
    pub min_paragraph_lines: u32,           // <3 lines → KeepTogether
    pub keep_figures_together: bool,
    pub keep_table_headers: bool,
    pub keep_list_markers: bool,
    pub small_block_threshold_lines: u32,   // small block → Monolithic
    pub default_orphans: u32,               // 2
    pub default_widows: u32,                // 2
}
```

Defaults match common book typography: 2/2 orphans+widows, headers stick, figures stay together, small blocks (≤ 3 lines) are treated as monolithic. Override per-document by passing a custom `FragmentationDefaults`.

## PageFragment

[`PageFragment`](../../../../layout/src/fragmentation.rs) at `fragmentation.rs:447` is the per-page output of the slicer:

```rust,ignore
pub struct PageFragment {
    pub page_index: usize,
    pub bounds: LogicalRect,                // page-local
    pub items: Vec<DisplayListItem>,
    pub source_node: Option<NodeId>,
    pub is_continuation: bool,              // continued from previous page
    pub continues_on_next: bool,            // continues on next page
}
```

`is_continuation` and `continues_on_next` flag boxes that span pages. The compositor uses them to suppress border-top on continuations and border-bottom on splits, per CSS `box-decoration-break: slice`.

## Two parallel implementations

There are two pagination modules and they share types but not the runtime:

| module | purpose | status |
|---|---|---|
| [`solver3::pagination`](../../../../layout/src/solver3/pagination.rs) | active page-layout (`PageGeometer`, `FakePageConfig`) | wired to `layout_document_paged` |
| [`fragmentation`](../../../../layout/src/fragmentation.rs) | break classification + page templates + counters | wired into the slicer that consumes `solver3::pagination` output |

`solver3::pagination` exists because the `fragmentation` module's break-during-layout integration ran into ordering problems with the inline engine — page breaks need to land between line boxes (Class B), which means the inline breaker has to be aware of the fragmentainer height. The infinite-canvas approach sidesteps this: the inline breaker doesn't change at all, and the slicer comes after with full Y-coordinate visibility.

The current direction is to keep `fragmentation::BoxBreakBehavior`, `BreakPoint`, `PageTemplate`, `PageCounter` (these are good abstractions) and to migrate the slicer + integration into `solver3::pagination`. Treat the duplication as transitional.

## CSS GCPM-3 status

[`solver3::pagination`](../../../../layout/src/solver3/pagination.rs) declares partial CSS Generated Content for Paged Media (Level 3) support:

| feature | status |
|---|---|
| Page counters (`counter(page)`, `counter(pages)`) | functional |
| Header / footer slot configuration | functional |
| Running elements (`position: running(name)`) | stub |
| Named strings (`string-set`, `content: string(name)`) | stub |
| Page selectors (`@page :first`, `@page :left/:right`) | stub |
| `@page` rule parsing | not implemented — programmatic via `FakePageConfig` |

`FakePageConfig` ([`solver3::pagination::FakePageConfig`](../../../../layout/src/solver3/pagination.rs)) is the temporary programmatic interface for setting page decoration before the full `@page` parser lands. It is *not* user-facing — the public API is `layout_document_paged` with a `FragmentationContext::Paged { page_size, … }`.

## See also

- [Layout Solver (Flex/Grid)](layout-solver.md) — the underlying `layout_document` that paged layout dispatches into
- [Inline Layout and Text Shaping](inline-text3.md) — the breaker that produces line-box break points (Class B)
- [CSS Property Internals](css-properties.md) — `break-before`, `break-after`, `break-inside`, `orphans`, `widows`
