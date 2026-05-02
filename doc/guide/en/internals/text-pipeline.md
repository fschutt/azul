---
slug: text-pipeline
title: Text Pipeline
language: en
canonical_slug: text-pipeline
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: [code-organization, inline-text3]
tracked_files:
  - layout/src/font.rs
  - layout/src/glyph_cache.rs
  - layout/src/text3/cache.rs
  - layout/src/text3/default.rs
  - layout/src/text3/glyphs.rs
  - layout/src/text3/script.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:32:10Z
---

# Text Pipeline

> **WIP** — `font::parsed::ParsedFont` is the active font handle; the lazy `LocaGlyf` decoder and `FontManager` eviction policy are still tuning. CPU rasterization (`glyph_cache`) is feature-gated behind `cpurender`.

The text pipeline turns OpenType bytes into screen pixels: parsing, metrics, fallback chains, allsorts shaping, glyph outline decoding, and the cell-level rasterizer cache for CPU rendering. Inline layout itself is covered in [Inline Layout and Text Shaping](inline-text3.md); this page focuses on the data flowing into and out of that engine.

```
font bytes (Arc<FontBytes>)
        │  font::parsed::ParsedFont::from_bytes
        ▼
ParsedFont (lazy loca/glyf, lazy GSUB/GPOS)
        │  FontManager::resolve_all_font_chains
        ▼
FontFallbackChain (Vec<FontId>)
        │  text3::default::shape_text_internal
        ▼
ShapedItem (glyph IDs + advances)
        │  display_list::generate_display_list
        ▼
PushText { glyphs, font_ref }
        │  cpurender or wr_translate2
        ▼
glyph outline → path → rasterizer cells → pixels
```

## ParsedFont

[`font::parsed::ParsedFont`](../../../../layout/src/font.rs) at `font.rs:292` is azul's OpenType handle. It wraps allsorts' tables but defers the expensive parts (loca+glyf, GSUB, GPOS) until first use:

```rust,ignore
pub struct ParsedFont {
    pub hash: u64,
    pub font_metrics: LayoutFontMetrics,
    pub pdf_font_metrics: PdfFontMetrics,
    pub num_glyphs: u16,
    pub hhea_table: HheaTable,
    pub hmtx_range: (usize, usize),     // offset+len in original_bytes
    pub vmtx_range: (usize, usize),
    pub vhea_table: Option<HheaTable>,
    pub maxp_table: MaxpTable,
    pub(crate) gsub_bytes: Option<Vec<u8>>,
    pub(crate) gsub_cache_lazy: OnceLock<Option<GsubCache>>,
    pub(crate) gpos_bytes: Option<Vec<u8>>,
    pub(crate) gpos_cache_lazy: OnceLock<Option<GposCache>>,
    pub opt_gdef_table: Option<Arc<GDEFTable>>,
    pub opt_kern_table: Option<Arc<KernTable>>,
    pub(crate) last_used: Arc<AtomicU64>,
    pub(crate) is_variable_font: bool,
    pub(crate) glyph_cache: Arc<RwLock<BTreeMap<u16, Arc<OwnedGlyph>>>>,
    pub(crate) loca_glyf: LocaGlyfState,
    pub space_width: Option<usize>,
    // …
}
```

`hash` is a SipHash of the font bytes — used as identity for caches and equality checks. `last_used` is updated by `get_or_decode_glyph`, `gsub()`, and `gpos()`; the `FontManager` evictor reads it to drop deferred faces that haven't been touched.

## Lazy decoding

[`LocaGlyfState`](../../../../layout/src/font.rs) at `font.rs:180` is a three-state enum that controls when the loca+glyf tables are parsed:

```rust,ignore
pub(crate) enum LocaGlyfState {
    Loaded(Option<Arc<Mutex<LocaGlyf>>>),  // ready (or known to lack outlines)
    Deferred {                              // parse on first glyph decode
        bytes: Arc<FontBytes>,
        font_index: usize,
        loaded: Arc<Mutex<Option<Arc<Mutex<LocaGlyf>>>>>,
    },
}
```

The `Deferred` variant is the big win on font-heavy pages: a stylesheet declaring 20 fallback fonts loads 20 `ParsedFont` headers, but the `LocaGlyf` (loca + glyf tables, often hundreds of kilobytes) is only parsed for the few faces actually rasterized.

`resolve_loca_glyf` does a two-step double-check: read the `Mutex<Option<…>>`, drop the lock, parse outside the critical section, take the lock again, store. This keeps the `LocaGlyf::load` cost off the lock so two threads don't serialize on the first decode of a popular font.

When `evict_unused` runs, deferred faces past their idle threshold revert their `loaded` field back to `None` and drop the parsed `LocaGlyf`. The `Arc<FontBytes>` is retained — re-decode on next access is comparatively cheap.

## Parsing entry points

| function | usage |
|---|---|
| [`ParsedFont::from_bytes`](../../../../layout/src/font.rs) at `font.rs:712` | eager parse, owns its own `LocaGlyf` copy — used by tests and PDF callers via `with_source_bytes` |
| [`ParsedFont::from_bytes_shared`](../../../../layout/src/font.rs) at `font.rs:1094` | lazy parse, retains `Arc<FontBytes>` for deferred decode — used by the layout font loader |

The lazy path goes through `Deferred`. The eager path produces `Loaded(Some(arc))` or `Loaded(None)` for fonts where loca+glyf parse failed (e.g. CFF-flavored OpenType, where there's no `glyf` table at all).

`from_bytes_internal` pre-inserts the space glyph (and `.notdef` when present) into `glyph_cache` so the shaper's cmap-miss path always has a fallback shape without racing against an in-flight decode.

## Font metrics

Two metric structs travel alongside `ParsedFont`:

- [`LayoutFontMetrics`](../../../../layout/src/text3/cache.rs) — ascent, descent, line gap, units-per-em. Used by the inline layout to compute `line-height: normal`.
- [`PdfFontMetrics`](../../../../layout/src/font.rs) — full HEAD/HHEA/OS/2 metric block (cap height, x-height, italic angle, weight class). Used by `printpdf` for font embedding.

`LineHeight::resolve_with_metrics` ([`text3/cache.rs:94`](../../../../layout/src/text3/cache.rs)) reads `LayoutFontMetrics` to compute pixel line heights:

```rust,ignore
pub fn resolve_with_metrics(&self, font_size_px: f32, m: &LayoutFontMetrics) -> f32 {
    self.resolve(font_size_px, m.ascent, m.descent, m.line_gap, m.units_per_em)
}
```

`Normal` resolves to `(ascent − descent + line_gap) / units_per_em × font_size_px`. `Px(v)` returns `v` directly (already pre-resolved during CSS parsing).

## FontManager

[`text3::cache::FontManager<T>`](../../../../layout/src/text3/cache.rs) at `cache.rs:678` is the per-window font handle. It owns:

```rust,ignore
pub struct FontManager<T> {
    pub fc_cache: FcFontCache,
    pub parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>,
    pub font_chain_cache: HashMap<FontChainKey, FontFallbackChain>,
    pub embedded_fonts: Mutex<HashMap<u64, FontRef>>,
    pub font_hash_to_families: HashMap<u64, StyleFontFamilyVec>,
    pub registry: Option<Arc<FcFontRegistry>>,
    pub last_resolved_font_stacks_sig: Option<u64>,
}
```

| field | purpose |
|---|---|
| `fc_cache` | rust-fontconfig's font-path cache (system font enumeration) |
| `parsed_fonts` | shared pool of `ParsedFont` keyed by `FontId` — multiple `FontManager`s can share via `Arc::clone` |
| `font_chain_cache` | resolved fallback chains keyed on `FontChainKey` |
| `embedded_fonts` | direct `FontRef`s that bypass fontconfig (e.g. embedded Material Icons) |
| `font_hash_to_families` | reverse map for compact-cache hashes — used by font collection |
| `registry` | optional `FcFontRegistry` for scout-on-demand lazy parsing |
| `last_resolved_font_stacks_sig` | FxHash of `prev_font_hashes` at the last successful chain resolution |

`fc_cache` is itself an `Arc<RwLock<_>>` internally (rust-fontconfig 4.1+), so cloning a `FontManager` is cheap and writes propagate instantly.

## Chain resolution

`solver3::getters::collect_and_resolve_font_chains_with_registration` (called from `LayoutWindow::layout_dom_recursive` at `layout/src/window.rs:880`) walks the styled DOM, collects unique `FontChainKey`s, and resolves them via fontconfig before layout starts.

The `last_resolved_font_stacks_sig` guard catches the common "repeat layout on an unchanged DOM" case: an FxHash of the `prev_font_hashes` slice. Match → no fontconfig call → roughly 1.5 ms (cold) / 0.9 ms (warm) saved per re-layout.

`font_dirty_nodes` (populated in `build_compact_cache`) refines this with per-node tracking when individual fonts change. The pair (signature + dirty list) catches both broad and narrow cases without false positives.

## ParsedFontTrait abstraction

The shaper, layout, and editor traffic in `T: ParsedFontTrait` rather than `ParsedFont` directly. This is what lets `font::mock::MockFont` substitute for tests:

```rust,ignore
pub trait ParsedFontTrait: ShallowClone {
    fn metrics(&self) -> LayoutFontMetrics;
    fn space_width(&self) -> Option<usize>;
    fn glyph_for_codepoint(&self, cp: u32) -> Option<u16>;
    fn glyph_advance(&self, gid: u16) -> Option<u16>;
    fn shape(&self, …) -> Result<Vec<ShapedGlyph>, LayoutError>;
    // …
}
```

Mock fonts hand-feed advance widths and glyph indices to test BiDi, line breaking, and editing without OpenType data.

## OwnedGlyph

[`OwnedGlyph`](../../../../layout/src/font.rs) is the per-glyph decoded outline:

```rust,ignore
pub struct OwnedGlyph {
    pub bbox: OwnedGlyphBoundingBox,
    pub outlines: Vec<GlyphOutline>,        // contour list
    pub horizontal_advance: u16,
    pub left_side_bearing: i16,
    pub vertical_advance: Option<u16>,
    pub vertical_origin_y: Option<i16>,
    // …
}

pub struct GlyphOutline {
    pub operations: GlyphOutlineOpVec,      // MoveTo | LineTo | QuadTo | CubicTo
}
```

`GlyphOutlineCollector` (private, `font.rs:210`) implements allsorts' `OutlineSink` to collect contour commands during `GlyfVisitorContext::visit()`. Composite glyphs and variable-font deltas are resolved automatically by allsorts before the sink sees them.

## Subsetting

`SubsetFont` and the `whole_font` / `subset` re-exports from allsorts come through the `parsed` module. Used by `printpdf` to embed minimal-byte font subsets into PDFs:

```rust,ignore
pub use allsorts::subset::CmapTarget;
pub use crate::font::parsed::{
    SubsetFont, OwnedGlyph, ParsedFont, FontParseWarning, FontType,
};
```

`CmapTarget` controls which cmap subtable variant the subset emits (PDF readers vary in what they accept). The subsetter takes the original font bytes plus a glyph-id list, parses just the needed tables, and emits a fresh OpenType binary with only those glyphs.

## Glyph cache (CPU rendering)

[`glyph_cache::GlyphCache`](../../../../layout/src/glyph_cache.rs) is a two-level cache for the CPU rasterizer (`cpurender` feature). Glyphs go through path construction once, then through cell rasterization once per (scale, sub-pixel position) combo:

```rust,ignore
pub struct GlyphCache {
    paths: HashMap<GlyphPathKey, Option<(PathStorage, bool)>>,   // 8K entries
    cells: HashMap<GlyphCellKey, Option<CachedCells>>,           // 16K entries
}

pub struct GlyphPathKey {
    pub font_hash: u64,
    pub glyph_id: u16,
    pub ppem: u16,                  // 0 = unhinted
}

pub struct GlyphCellKey {
    pub font_hash: u64,
    pub glyph_id: u16,
    pub ppem: u16,
    pub scale_fixed: u32,           // (scale * 65536) for unhinted glyphs
    pub subpx_x: u8,                // 0..3, quantized to 1/4 px
    pub subpx_y: u8,
}
```

The path cache stores AGG `PathStorage` objects built from `OwnedGlyph` outlines. The cell cache stores AGG `CellAa` arrays — the rasterizer's intermediate output, before scanline conversion. Cells are computed at the canonical `(0,0)` origin and offset by integer pixel deltas at draw time, so a glyph rendered at (100.25, 200.75) reuses the same cell array as the same glyph at (300.25, 50.75).

The 1/4-pixel sub-pixel quantization (4 × 4 = 16 cell variants per glyph + scale) is the visual-quality / cache-size sweet spot. Sub-pixel positioning matters for text legibility; quantizing finer doubles the cache without measurable rendering improvement.

`MAX_PATH_ENTRIES = 8192` and `MAX_CELL_ENTRIES = 16384` cap unbounded growth on Latin-heavy pages with extensive font rotation.

## Script and language

[`text3::script::Script`](../../../../layout/src/text3/script.rs) detects Unicode scripts per cluster — needed to choose GSUB/GPOS feature sets and to pick a fallback language for hyphenation. `script_to_language` maps `Script::Latin` → `Language::English` (the default), `Script::Cyrillic` → `Language::Russian`, etc.

The mapping is intentionally rough — when the document language is known via CSS `lang` or HTML `lang`, that overrides script-derived guesses. The script-only path is the no-locale fallback.

## See also

- [Inline Layout and Text Shaping](inline-text3.md) — how shaped glyphs flow through layout
- [Image Pipeline](image-pipeline.md) — the GL texture cache for image content; parallel infrastructure to font caching
- [Layout Solver (Flex/Grid)](layout-solver.md) — `layout_document` orchestration that calls into font resolution
