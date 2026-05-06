---
slug: inline-text3
title: Inline Layout and Text Shaping
language: en
canonical_slug: inline-text3
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: The text3 engine - shaping, line breaking, BiDi, hyphenation
prerequisites: [layout-solver, text-pipeline]
tracked_files:
  - layout/src/lib.rs
  - layout/src/window.rs
  - layout/src/text3/cache.rs
  - layout/src/text3/edit.rs
  - layout/src/text3/glyphs.rs
  - layout/src/text3/knuth_plass.rs
  - layout/src/text3/script.rs
  - layout/src/text3/selection.rs
  - layout/src/text3/default.rs
  - layout/src/solver3/fc.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:55:41Z
---

# Inline Layout and Text Shaping

> **WIP** — `text3` is the live engine; the older `text2` path has been removed. Some features (`initial-letter`, `text-box-trim`, ruby) are partially implemented.

The text engine lives at `layout/src/text3/`. It owns shaping, line breaking, BiDi reordering, vertical writing modes, hyphenation, selection, and editing. Its central type is the [`TextShapingCache`](https://github.com/maps4print/azul/blob/master/layout/src/text3/cache.rs) (re-exported as `TextLayoutCache` for backward compatibility from `layout/src/lib.rs`).

```rust,ignore
pub use text3::cache::TextShapingCache as TextLayoutCache;
```

## File map

| File | Purpose |
|---|---|
| `text3/cache.rs` | `TextShapingCache`, `FontManager`, `FontContext`, `UnifiedConstraints`, `UnifiedLayout`, the 5-stage pipeline (`layout_flow`) |
| `text3/glyphs.rs` | Glyph storage primitives (`ShapedGlyph`), glyph instance conversion |
| `text3/script.rs` | Script detection, language mapping, `script_to_language` |
| `text3/knuth_plass.rs` | Knuth–Plass total-fit line breaking |
| `text3/edit.rs` | Per-character edit operations against `UnifiedLayout`'s `items` vec |
| `text3/selection.rs` | Cursor-pixel mapping, selection range expansion |
| `text3/default.rs` | `PathLoader` — disk-based font loader for the IO side |
| `text3/mod.rs` | `pub use` barrel |

The IFC entry point on the layout side is `solver3/fc.rs:layout_ifc` (`fc.rs:2373`).

## The 5-stage pipeline

`TextShapingCache::layout_flow` (`cache.rs:5569`) is the top-level entry. Each stage is independently cached:

```
InlineContent ──Stage 1─▶ LogicalItem
                          (per-char attribution)
                │
                ▼ Stage 2
            VisualItem  (BiDi reorder, UAX #9)
                │
                ▼ Stage 3
            ShapedItem  (HarfBuzz/allsorts; per-item cache)
                │
                ▼ Stage 4
            ShapedItem' (text-orientation rotate for vertical-rl/lr)
                │
                ▼ Stage 5
            PositionedItem in UnifiedLayout
            (Knuth–Plass lines + final placement)
```

Stages 1–4 are independent of geometry; stage 5 takes a `flow_chain: &[LayoutFragment]` so the same shaped content can be re-flowed across columns or pages without re-shaping.

```rust,ignore
pub fn layout_flow<T: ParsedFontTrait>(
    &mut self,
    content: &[InlineContent],
    style_overrides: &[StyleOverride],
    flow_chain: &[LayoutFragment],
    font_chain_cache: &HashMap<FontChainKey, FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<FlowLayout, LayoutError>;
```

## Caching architecture

`TextShapingCache` (`cache.rs:5255`) holds four maps:

| Field | Key | Value | What it caches |
|---|---|---|---|
| `logical_items` | `CacheId = u64` of `&[InlineContent]` | `Arc<Vec<LogicalItem>>` | Stage 1 |
| `visual_items` | `(logical_items_id, base_direction)` → CacheId | `Arc<Vec<VisualItem>>` | Stage 2 |
| `shaped_items` | `(visual_items_id, style_hash)` → CacheId | `Arc<Vec<ShapedItem>>` | Stage 3 (monolithic) |
| `per_item_shaped` | `hash(text, bidi_level, script, style.layout_hash())` | `Arc<PerItemShapedEntry>` | Stage 3 (incremental) |

Stage 3 has two levels: a fast monolithic cache hit returns the entire `Vec<ShapedItem>` if the visual-items + style hashes match. On a miss, `shape_visual_items_with_per_item_cache` reuses individual cached items per-key (keyed on text + bidi level + script + layout-affecting style) and only re-shapes new items. Eviction runs every layout pass via `begin_generation`:

```rust,ignore
pub fn begin_generation(&mut self) {
    if self.generation > 0 && !self.per_item_accessed.is_empty() {
        let accessed = &self.per_item_accessed;
        self.per_item_shaped.retain(|k, _| accessed.contains(k));
    }
    self.per_item_accessed.clear();
    self.generation += 1;
}
```

The cap is `PER_ITEM_CACHE_MAX = 4096`; exceeding it forces a generation flush early.

## `InlineContent` and `LogicalItem`

`InlineContent` (defined in `azul-core` and re-exported through `text3`) is the externally-visible inline-level "atom":

```rust,ignore
pub enum InlineContent {
    Text(StyledRun),
    Image(InlineImage),
    Space(SpaceConfig),
    LineBreak(LineBreakConfig),
    Tab { style: Arc<StyleProperties> },
    Marker { run: StyledRun, position_outside: bool },
    Shape(InlineShape),
    Ruby { base: Vec<InlineContent>, text: Vec<InlineContent>, style: Arc<StyleProperties> },
}
```

`StyledRun` carries a `String` plus an `Arc<StyleProperties>` (font selectors, size, weight, decoration, color). `Arc` makes per-item cache entries cheap to share between similar runs.

Stage 1 (`create_logical_items`) splits `Text` runs by script boundaries, applies `style_overrides` (per-character style changes for selection, IME preedit, search highlighting), and tags each `LogicalItem` with the source span and style.

## BiDi (Stage 2)

`reorder_logical_items` runs Unicode BiDi (UAX #9) using the `unicode-bidi` crate. The base direction comes from CSS `direction`, except when `unicode-bidi: plaintext` is set:

```rust,ignore
let base_direction = if unicode_bidi_val == UnicodeBidi::Plaintext {
    let has_strong = logical_items.iter().any(|item| {
        if let LogicalItem::Text { text, .. } = item {
            matches!(unicode_bidi::get_base_direction(text.as_str()),
                Direction::Ltr | Direction::Rtl)
        } else { false }
    });
    if has_strong { get_base_direction_from_logical(&logical_items) }
    else { first_constraints.direction.unwrap_or(BidiDirection::Ltr) }
} else {
    first_constraints.direction.unwrap_or(BidiDirection::Ltr)
};
```

CSS Writing Modes § 8.3: `plaintext` auto-detects from the first strong character; empty paragraphs fall back to the containing block's direction.

## Shaping (Stage 3)

`shape_visual_items` and `shape_visual_items_with_per_item_cache` (`cache.rs:6346`, `cache.rs:6591`) drive the shaper through the `ParsedFontTrait` abstraction. The default implementation uses [allsorts](https://github.com/yeslogic/allsorts) (re-exported via `crate::font::parsed::ParsedFont`) for OpenType shaping with HarfBuzz-equivalent ligatures, kerning, contextual forms, and complex script support.

Font fallback: shaping a cluster goes through a `FontFallbackChain` resolved from the cluster's script + style. Each fallback level is checked for codepoint coverage; the first font that covers all codepoints in the cluster wins. The fallback chain is built once per `(font-family, weight, style)` stack by `collect_and_resolve_font_chains_with_registration` (in `solver3/getters.rs`) and cached on `FontManager.font_chain_cache`.

`ShapedItem` variants:

```rust,ignore
pub enum ShapedItem {
    Cluster(ShapedCluster),       // a single grapheme cluster + glyphs
    Object { ... },                // inline-block, image
    CombinedBlock { ... },         // text-combine-upright run
    Tab { ... },
    Break { ... },                 // soft/hard break opportunity
}
```

`ShapedCluster.source_node_id: Option<NodeId>` lets selection and editing map glyph runs back to their source DOM node. `Object` and other generated items lack a direct `source_node_id`; the IFC's `ContentIndex` mapping recovers it.

## Text-orientation transform (Stage 4)

For `writing-mode: vertical-rl`/`vertical-lr` and `text-orientation: upright | sideways | mixed`, glyph clusters are rotated and offset before line breaking. The transform uses constraints from the *first* fragment only — multi-fragment flows with mixed writing modes are noted as a TODO in `cache.rs:5543`.

## Line breaking and flow (Stage 5)

`text3/knuth_plass.rs` implements Knuth–Plass total-fit line breaking. The breaker walks `ShapedItem`s, accumulating "boxes" (clusters) and "glue" (spaces), then minimises a total-badness metric across all line-break combinations. Tightness, looseness, and `text-wrap: balance` are all knobs in the badness function.

`perform_fragment_layout` runs once per `LayoutFragment` (one fragment per column or per page). A `BreakCursor` tracks where the previous fragment stopped; the next fragment picks up from that cursor. This is how multi-column and paged inline layout works without re-shaping.

`UnifiedLayout` is the output:

```rust,ignore
pub struct UnifiedLayout {
    pub items: Vec<PositionedItem>,   // one per glyph cluster / object
    pub bounds: LogicalRect,
    pub line_count: usize,
    pub baseline_offsets: Vec<f32>,
    // ...
}

pub struct PositionedItem {
    pub item: ShapedItem,
    pub position: LogicalPosition,    // top-left of the cluster's bounding box
    pub line_index: u32,
    pub bidi_level: u8,
    // ...
}
```

`UnifiedLayout` is wrapped in `Arc` and stored on the IFC root's `LayoutNode.warm.inline_layout_result: Option<Arc<CachedInlineLayout>>` (see [Layout Solver](layout-solver.md)).

## `UnifiedConstraints`

`cache.rs:1047`. The full per-IFC layout input. Built by `solver3/fc.rs:layout_ifc` from CSS getters on the IFC root:

```rust,ignore
pub struct UnifiedConstraints {
    pub shape_boundaries: Vec<ShapeBoundary>,    // CSS Shapes
    pub shape_exclusions: Vec<ShapeBoundary>,
    pub available_width: AvailableSpace,         // Definite | MinContent | MaxContent
    pub available_height: Option<f32>,
    pub writing_mode: Option<WritingMode>,
    pub direction: Option<BidiDirection>,
    pub text_orientation: TextOrientation,
    pub text_align: TextAlign,
    pub text_justify: JustifyContent,
    pub line_height: LineHeight,
    pub vertical_align: VerticalAlign,
    pub strut_ascent: f32,
    pub strut_descent: f32,
    pub strut_x_height: f32,
    pub ch_width: f32,
    pub overflow: OverflowBehavior,
    pub segment_alignment: SegmentAlignment,
    pub text_combine_upright: Option<TextCombineUpright>,
    pub exclusion_margin: f32,
    pub hyphenation: Hyphens,
    pub hyphenation_language: Option<Language>,
    pub text_indent: f32,
    pub text_indent_each_line: bool,
    pub text_indent_hanging: bool,
    pub initial_letter: Option<InitialLetter>,
    pub line_clamp: Option<NonZeroUsize>,
    pub text_wrap: TextWrap,
    pub columns: u32,
    pub column_gap: f32,
    pub hanging_punctuation: bool,
    pub overflow_wrap: OverflowWrap,
    pub text_align_last: TextAlign,
    pub word_break: WordBreak,
    pub white_space_mode: WhiteSpaceMode,
    pub line_break: LineBreakStrictness,
    pub unicode_bidi: UnicodeBidi,
}
```

`available_width: AvailableSpace` is the cache-validity key. A layout shaped under `MinContent` cannot be reused for `Definite(actual_column_width)` — the line breaks would be at the wrong positions. This was the root cause of the table-cell width bug fixed by storing `constraints` alongside the layout in `CachedInlineLayout`.

`AvailableSpace::default()` returns `MaxContent`, never `Definite(0.0)` — a zero-width container would make every word overflow to its own line.

`PartialEq` on `UnifiedConstraints` uses `round_eq` for floats so jitter from CSS recomputation does not invalidate the cache. `Hash` uses `f.round() as usize` for the same reason.

## `FontManager` and the font chain cache

`cache.rs:678`. `FontManager<T>` is parameterised over the parsed-font type (`FontRef` for production, `MockFont` for tests).

```rust,ignore
pub struct FontManager<T> {
    pub fc_cache: FcFontCache,                                    // shared via Arc<RwLock>
    pub parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>,             // shared pool
    pub font_chain_cache: HashMap<FontChainKey, FontFallbackChain>,
    pub embedded_fonts: Mutex<HashMap<u64, FontRef>>,
    pub font_hash_to_families: HashMap<u64, StyleFontFamilyVec>,
    pub registry: Option<Arc<FcFontRegistry>>,
    pub last_resolved_font_stacks_sig: Option<u64>,
}
```

`fc_cache` is a `rust-fontconfig` v4.1 shared handle (internally `Arc<RwLock>`); cloning is cheap and builder-thread writes are immediately visible. No more snapshot-refresh dance.

`registry` is the optional scout-on-demand path: when present, chain resolution calls `FcFontRegistry::request_and_resolve_with_scripts` which lazy-parses families the DOM needs, dropping peak RSS by the common-stack metadata size (~15 MiB on macOS) for headless renders that don't touch every system font.

`last_resolved_font_stacks_sig` is the rolling-hash signature of `compact_cache.prev_font_hashes` at the moment the chain cache was last populated. `LayoutWindow.layout_dom_recursive` (`window.rs:887`) reads this to skip the resolver when the DOM's font stacks haven't changed since the last successful resolution. See [Layout Solver](layout-solver.md) for the skip logic.

## `FontContext` vs `FontManager`

`FontContext` (`cache.rs:533`) is the *application-wide* shared font state — owned by `App`. `FontManager` is the *per-window* one — owned by `LayoutWindow`. They share the same `parsed_fonts` Arc; `FontContext::to_font_manager` clones into a `FontManager` while keeping the parsed-fonts pool shared.

`FontContext::pre_resolve_chains_for_dom` is the warmup hook: a headless renderer or PDF generator can pre-resolve all font chains for a DOM before the first layout, avoiding a layout-time spike. The function uses `scripts_present_in_styled_dom` (in `solver3/getters.rs`) to limit Unicode-fallback fonts to the scripts actually present — for an ASCII-only page, this skips the ~300 MiB Arial-Unicode / CJK / Arabic pull-in entirely.

## Hyphenation

Behind `feature = "text_layout_hyphenation"`. Uses the [`hyphenation`](https://crates.io/crates/hyphenation) crate with TeX patterns. Languages are loaded lazily; each `UnifiedConstraints` carries `hyphenation: Hyphens` (`Auto`/`None`/`Manual`) and `hyphenation_language: Option<Language>`. Stage 5 inserts soft-hyphen break opportunities into the Knuth–Plass break list before line breaking.

When the feature is off, `text3::cache::Standard` becomes a no-op stub (`cache.rs:122`) returning empty `breaks`, so the rest of the pipeline compiles unchanged.

## Selection

`text3/selection.rs` plus types in `azul-core`:

- `TextCursor { cluster_id: GraphemeClusterId, affinity: CursorAffinity }` — locates a cursor between two grapheme clusters, with affinity choosing the visual side at line breaks.
- `SelectionRange { anchor, focus }` — same `TextCursor` type at both ends.
- `ContentIndex` — a `(run_index, cluster_offset)` pair indexed against a `UnifiedLayout`. Maps cleanly to a `(NodeId, byte_offset)` via `ShapedCluster.source_node_id`.

`hit_test_cursor_position(layout, point)` returns the `TextCursor` at a screen position. `cursor_to_pixel_position(layout, cursor)` is the inverse, used to draw the caret. Both walk `layout.items` in source order.

## Editing

`text3/edit.rs` operates directly against `UnifiedLayout`:

- `apply_text_changeset(&mut layout, changeset)` mutates the `items` vec for a stream of inserts/deletes given as cluster-indexed operations.
- `recompute_line_breaks(&mut layout, available_width)` reruns Knuth–Plass over the modified items without re-shaping unaffected clusters.

This is the fast path used by `LayoutWindow::try_incremental_text_relayout` for keystroke-by-keystroke text edits. It bypasses `solver3::layout_document` entirely when the IFC's height does not change. If the height changes (e.g. the line wraps), the path falls back to a normal `layout_document` call so the BFC parent can reposition siblings.

`window.rs:DirtyTextNode` (`window.rs:347`) holds the in-progress `Vec<InlineContent>` for an edited text node before it's committed back into the DOM:

```rust,ignore
pub struct DirtyTextNode {
    pub content: Vec<InlineContent>,
    pub cursor: Option<TextCursor>,
    pub needs_ancestor_relayout: bool,
}
```

`needs_ancestor_relayout = true` means the IFC's height changed and the parent BFC needs to re-flow.

## IME preedit injection

`LayoutWindow.pre_preedit_content: Option<Vec<InlineContent>>` (`window.rs:485`) stores a snapshot of the pre-edit inline content. When IME preedit text changes (e.g., during CJK composition), the renderer injects the preedit text into a clean copy of the original content, preserving the user's existing input. Without the snapshot, repeated `setMarkedText` calls would accumulate stale preedits.

`LayoutContext.preedit_text: Option<String>` is the per-render preedit string. `cursor_locations: Vec<(DomId, NodeId, TextCursor)>` carries multi-cursor positions for both visible cursors and preedit anchors.

## Layout-vs-render style equivalence

`StyleProperties::layout_eq` compares only the fields that affect glyph positions (font, size, letter-spacing, word-spacing). Color, decoration, background, and shadow are *not* compared. `TextShapingCache::use_old_layout` uses this to decide whether a cached layout can be reused when constraints + content match but rendering-only properties changed:

```rust,ignore
pub fn use_old_layout(
    old_constraints: &UnifiedConstraints,
    new_constraints: &UnifiedConstraints,
    old_content: &[InlineContent],
    new_content: &[InlineContent],
) -> bool;
```

A pure color change on a paragraph thus keeps the same `UnifiedLayout` and only triggers display-list regeneration.

## `solver3/fc.rs:layout_ifc` — the call site

`fc.rs:2373`. `layout_ifc` is the bridge from box layout to text layout. It:

1. Resolves the IFC root's DOM ID (anonymous boxes inherit from parent or first child with a DOM id).
2. Walks the IFC tree to collect `Vec<InlineContent>` and a `child_map: BTreeMap<NodeIndex, ContentRange>` (so glyph clusters can be mapped back to layout nodes for hit-testing).
3. Checks for a cached `CachedInlineLayout` with matching `constraints`. If present and `available_width` + `has_floats` match, return it without re-running stages 1–5.
4. Builds `UnifiedConstraints` from CSS and `LayoutConstraints`.
5. Calls `text_cache.layout_flow(content, overrides, &[fragment], font_chain_cache, fc_cache, loaded_fonts, debug_messages)`.
6. Builds `CachedInlineLayout::new_with_constraints` and stores it on the IFC root's `warm.inline_layout_result`.
7. Returns a `LayoutOutput` with the IFC's bounds and per-child positions for inline-blocks.

The first ~80 lines of `layout_ifc` (Phase 2d incremental relayout) are the cache-hit fast path; full execution starts at `Phase 1: Collect and measure all inline-level children`.

## Known gaps vs CSS Inline Layout Module Level 3

From `cache.rs:5551`:

- § 3.3 **initial-letter** (drop caps) — types in place, layout not wired.
- § 4 **vertical-align** — only baseline supported. `top`, `middle`, `bottom`, `text-top`, `text-bottom`, `super`, `sub` use approximate offsets; full table-cell/inline-block alignment is incomplete.
- § 6 **text-box-trim / leading-trim** — not implemented.
- Multi-fragment text orientation (mixed writing modes across columns) uses constraints from the first fragment only.
- Ruby layout: `Ruby` variant exists but baseline alignment of base+text is approximate.
