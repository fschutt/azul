---
slug: inline-text3
title: Inline Layout and Text Shaping
language: en
canonical_slug: inline-text3
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: [code-organization, layout-solver, css-properties]
tracked_files:
  - layout/src/text3/mod.rs
  - layout/src/text3/cache.rs
  - layout/src/text3/default.rs
  - layout/src/text3/glyphs.rs
  - layout/src/text3/knuth_plass.rs
  - layout/src/text3/script.rs
  - layout/src/text3/selection.rs
  - layout/src/text3/edit.rs
  - layout/src/solver3/fc.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:32:10Z
---

# Inline Layout and Text Shaping

> **WIP** — `text3` is the third-generation engine, currently being driven by the box solver via `solver3::fc`. APIs may shift; the cache layout in particular is still tuning for the per-item invalidation path.

`text3` is the inline-formatting-context engine. It owns BiDi reordering, font fallback, shaping (allsorts), Knuth–Plass line breaking, vertical-writing-mode rotation, and final glyph positioning. It lives at [`layout/src/text3/`](../../../../layout/src/text3/) and is reached from `solver3` whenever a node has `FormattingContext::Inline`.

The five-stage pipeline ([`text3/cache.rs:1-16`](../../../../layout/src/text3/cache.rs)):

```
InlineContent
    │  Stage 1: Logical Analysis (BiDi run detection, segmentation)
    ▼
LogicalItem
    │  Stage 2: BiDi Reordering (UAX #9, unicode-bidi crate)
    ▼
VisualItem
    │  Stage 3: Shaping (allsorts GSUB/GPOS, font fallback)
    ▼
ShapedItem
    │  Stage 4: Text Orientation (vertical writing-mode transforms)
    ▼
ShapedItem (rotated)
    │  Stage 5: Flow / Positioning (line breaking + placement)
    ▼
PositionedItem  →  UnifiedLayout
```

Each stage has a separate cache; a paragraph that re-renders unchanged hits all four caches and runs only the Stage-5 positioning step.

## Entry from solver3

`solver3::fc::layout_inline_formatting_context` builds a `UnifiedConstraints` and calls `text3::cache::perform_fragment_layout`. The constraints carry the inline-axis available size, the writing mode, font selectors, justification, line-height, white-space handling — everything the inline engine needs without re-walking the DOM.

```rust,ignore
pub struct UnifiedConstraints {
    pub available: AvailableSpace,
    pub writing_mode: WritingMode,
    pub direction: BidiDirection,
    pub text_align: TextAlign,
    pub justify_content: JustifyContent,
    pub line_height: LineHeight,
    pub white_space: WhiteSpace,
    pub overflow_wrap: OverflowWrap,
    pub word_break: WordBreak,
    pub line_break: LineBreak,
    pub hyphens: Hyphens,
    pub text_indent: f32,
    pub orphans: u32,
    pub widows: u32,
    pub font_chain: FontChainKeyOrRef,
    // … further font, decoration, and shape fields …
}
```

`AvailableSpace` is Taffy-shaped: `Definite(f32) | MinContent | MaxContent`. The default is `MaxContent` ("no width constraint") — never `Definite(0.0)`, which would make every word overflow.

## InlineContent → LogicalItem (Stage 1)

[`create_logical_items`](../../../../layout/src/text3/cache.rs) at `text3/cache.rs:5872` walks the run list and emits one `LogicalItem` per BiDi run / atomic inline / segmentation boundary. The function returns a `Vec<LogicalItem>` keyed by `LogicalItemsKey { content_hash, constraints_hash }` — same input + same constraints → cache hit on `TextShapingCache.logical_items`.

`StyledRun` is the input atom from solver3 ([`text3/cache.rs:1289`](../../../../layout/src/text3/cache.rs)):

```rust,ignore
pub struct StyledRun {
    pub text: String,
    pub style: StyleProperties,
    pub source: ContentIndex,
}
```

Inline images and shapes ride alongside text via `InlineContent::Image(InlineImage)` / `InlineContent::Shape(InlineShape)`. They're treated as monolithic "objects" by the line breaker, never split.

## LogicalItem → VisualItem (Stage 2)

[`reorder_logical_items`](../../../../layout/src/text3/cache.rs) at `text3/cache.rs:6172` runs the UAX #9 BiDi algorithm via the `unicode-bidi` crate. It produces `VisualItem`s in *visual* order (left-to-right on the screen), each tagged with its embedding level so Stage 3 knows which direction to shape.

Mixed-script and mixed-direction paragraphs come out correctly because the Stage-2 result is order-aware: an Arabic phrase in an English paragraph emits visual items right-to-left for the Arabic substring, sandwiched in left-to-right English context.

## VisualItem → ShapedItem (Stage 3)

`text3` ships two shaping entry points:

- [`shape_visual_items_with_per_item_cache`](../../../../layout/src/text3/cache.rs) at `cache.rs:6346` — caches each item independently. Editing one word reshapes only that word, not the paragraph.
- [`shape_visual_items`](../../../../layout/src/text3/cache.rs) at `cache.rs:6591` — monolithic cache, used for read-only layouts.

Both delegate to the concrete shaper in [`text3::default::shape_text_internal`](../../../../layout/src/text3/default.rs), which calls allsorts' GSUB/GPOS pipeline. Shaping is per-script — a single visual item never crosses a script boundary thanks to the segmentation done in Stage 1.

`ShapedItem` is the per-cluster shaped record:

```rust,ignore
pub enum ShapedItem {
    Cluster(ShapedCluster),
    InlineObject(InlineObject),
    LineBreak,
    SoftBreak,
    Hyphen,
}

pub struct ShapedCluster {
    pub glyphs: ShapedGlyphVec,   // SmallVec<[ShapedGlyph; 1]>
    pub advance: f32,
    pub source: ContentIndex,
    pub bidi_level: BidiLevel,
    pub script: Script,
    pub style: StyleProperties,
    // …
}
```

`ShapedGlyphVec` is a `SmallVec<[ShapedGlyph; 1]>` — Latin's 1-glyph clusters stay inline; ligatures and combining marks spill to heap.

## Font fallback

[`FontManager`](../../../../layout/src/text3/cache.rs) is the per-window font handle. It holds a `FcFontCache` (rust-fontconfig) plus a chain cache:

```rust,ignore
pub enum FontChainKeyOrRef {
    Chain(FontChainKey),     // resolved via fontconfig with fallback
    Ref(usize),              // direct FontRef (e.g. embedded icon font)
}

pub struct FontChainKey {
    pub font_families: Vec<String>,
    pub weight: FcWeight,
    pub italic: bool,
    pub oblique: bool,
}
```

`FontChainKey::from_selectors` builds a key from a CSS `font-family: A, B, sans-serif` declaration. The chain is resolved on first use and cached; subsequent paragraphs with the same key skip the fontconfig walk.

The font chain is a `Vec<FontRef>` — when a glyph is missing in font N, the shaper falls through to N+1 and continues. `solver3::getters::collect_and_resolve_font_chains_with_registration` (called from `LayoutWindow::layout_dom_recursive` at `layout/src/window.rs:880`) seeds the cache before the layout pass starts so shaping never blocks on font I/O.

## Stage 4: vertical writing-mode rotation

CSS `writing-mode: vertical-rl` and `vertical-lr` rotate glyphs 90°. `text-orientation: mixed | upright | sideways` controls per-cluster rotation. The transform is applied in-place on the `ShapedCluster` before line breaking, so Stage 5 sees pre-rotated advances and offsets.

## Stage 5: line breaking and positioning

Two breakers, selected by CSS `text-wrap`:

| algorithm | function | use case |
|---|---|---|
| greedy | [`break_one_line`](../../../../layout/src/text3/cache.rs) at `cache.rs:8030` | default, browser-compatible |
| Knuth–Plass | [`kp_layout`](../../../../layout/src/text3/knuth_plass.rs) at `knuth_plass.rs:72` | `text-wrap: balance`, high-quality typesetting |

The greedy path also handles `overflow-wrap: anywhere`, `word-break: break-all`, and emergency hyphenation breaks. Knuth–Plass currently supports only horizontal text; vertical writing modes fall back to greedy.

Knuth–Plass converts the `ShapedItem` stream to a sequence of `Box | Glue | Penalty` nodes and runs the dynamic-programming optimum-fit search from "Breaking Paragraphs into Lines" (Knuth & Plass, 1981). Demerits factor in line-fit ratio (stretch vs shrink), penalty cost at hyphen-break points, and badness against an `INFINITY_BADNESS = 10000.0` ceiling.

[`position_one_line`](../../../../layout/src/text3/cache.rs) at `cache.rs:8456` then walks the broken lines, applies `text-align`, distributes justification glue, and emits one `PositionedItem` per cluster:

```rust,ignore
pub struct PositionedItem {
    pub item: ShapedItem,
    pub position: Point,
    pub line_index: usize,
}

pub struct UnifiedLayout {
    pub items: Vec<PositionedItem>,
    pub overflow: OverflowInfo,
}
```

`UnifiedLayout` is what `solver3::fc` reads back to know the IFC's content size and baseline.

## Floats and shape-aware wrapping

`solver3::fc::FloatingContext::available_line_box_space` (covered in [Layout Solver](layout-solver.md)) gives `text3` per-y `(start, end)` pairs that account for floats clipping into the line box. The breaker queries this when starting each line and shrinks the available width accordingly. Lines that don't fit in the gap step down by the line height and try again.

`shape-inside` and `shape-outside` plug into the same hook via `ShapeBoundary` ([`text3/cache.rs:2855`](../../../../layout/src/text3/cache.rs)). Circle, ellipse, polygon, and path are converted from CSS shapes via `ShapeBoundary::from_css_shape`, then queried by `available_line_box_space` like any float exclusion.

## Hyphenation

The `text_layout_hyphenation` feature pulls in the `hyphenation` crate. [`find_all_hyphenation_breaks`](../../../../layout/src/text3/cache.rs) at `cache.rs:8224` runs Liang-Knuth pattern matching per language (`Language` derived from script via `script_to_language`). The breakpoints become low-cost penalty nodes that K–P can pick when they reduce paragraph badness.

When the feature is off the module ships a stub `Standard` type that returns no breaks, so calling code compiles either way.

## Editing fast path

[`edit.rs`](../../../../layout/src/text3/edit.rs) and [`try_incremental_relayout`](../../../../layout/src/text3/cache.rs) at `cache.rs:5192` together implement the per-keystroke fast path. Three outcomes:

| result | behavior | trigger |
|---|---|---|
| `GlyphSwap` | reuse positions, replace glyphs | edit didn't change advance widths |
| `LineShift { affected_item, delta }` | shift items right/left of the edit | line still fits |
| `PartialReflow { reflow_from_line }` | rerun line breaker from this line | edit overflows the line |
| `FullRelayout` | bail out | edit crossed line-cache boundaries |

Inputs are `dirty_item_indices` (which items changed), `old_advances` and `new_advances` (per-item advance widths), and `CachedLineBreaks` (line ranges + widths from the previous layout). The function is pure — the actual shifting/reflowing is done by the caller in `LayoutWindow::apply_text_changeset`.

## Selection and cursors

[`text3::selection`](../../../../layout/src/text3/selection.rs) and `core::selection::TextCursor` together drive selection rendering. The solver passes `cursor_locations: Vec<(DomId, NodeId, TextCursor)>` and `text_selections: &BTreeMap<DomId, TextSelection>` into `LayoutContext`. The display-list generator emits caret rectangles and selection-fill rectangles per IFC.

Cursor placement uses `ContentIndex` (the `(run_index, byte_offset)` pair) — the same coordinate system as `StyledRun.source`, so a cursor at content-index 7 in a paragraph reliably maps back to the same logical position across re-layouts even if line breaks shift.

## Cache structure

[`TextShapingCache`](../../../../layout/src/text3/cache.rs) at `cache.rs:5255` holds four caches:

```rust,ignore
pub struct TextShapingCache {
    logical_items: HashMap<CacheId, Arc<Vec<LogicalItem>>>,    // Stage 1
    visual_items: HashMap<CacheId, Arc<Vec<VisualItem>>>,      // Stage 2
    shaped_items: HashMap<CacheId, Arc<Vec<ShapedItem>>>,      // Stage 3 (monolithic)
    per_item_shaped: HashMap<u64, Arc<PerItemShapedEntry>>,    // Stage 3 (per-item)
    per_item_accessed: HashSet<u64>,
    generation: u64,
}
```

`generation` advances on each layout pass. `per_item_accessed` records which keys were touched this generation; entries not in the set are evicted at the end of the pass — a generation-based LRU without per-entry timestamps.

The cache is owned by `LayoutWindow` and threaded through `layout_document` as `text_cache: &mut TextLayoutCache`. `TextLayoutCache` is a back-compat alias for `TextShapingCache` (see `layout/src/lib.rs:223`).

## Mock fonts for tests

[`font::mock::MockFont`](../../../../layout/src/font.rs) is a `ParsedFontTrait` impl with hand-fed metrics, advance widths, and glyph indices. It exists so unit tests can exercise the inline pipeline without loading real OpenType data — useful for testing line breaking, BiDi, and selection logic deterministically.

## See also

- [Layout Solver (Flex/Grid)](layout-solver.md) — how `solver3::fc` calls into here
- [Text Pipeline](text-pipeline.md) — font parsing, glyph cache, CPU rasterization
- [Fragmentation](fragmentation.md) — orphans/widows hooks that feed back into the breaker
