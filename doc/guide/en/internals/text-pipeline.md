---
slug: text-pipeline
title: Text Pipeline
language: en
canonical_slug: text-pipeline
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: How a styled text run becomes glyphs — font fallback, OpenType shaping, fragmentation across line boxes.
prerequisites: [layout-solver, compact-cache]
tracked_files:
  - layout/src/lib.rs
  - layout/src/window.rs
  - layout/src/font.rs
  - layout/src/font_traits.rs
  - layout/src/text3/cache.rs
  - layout/src/text3/default.rs
  - layout/src/text3/glyphs.rs
  - layout/src/text3/script.rs
  - layout/src/glyph_cache.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:55:41Z
---

# Text Pipeline

> **WIP** — text loading and font invalidation logic shifts often. The font cache uses a per-node dirty list backed by `compact_cache.font_dirty_nodes`; the older XOR-based `font_stacks_hash` field is kept as a fallback signal but is no longer authoritative.

This page covers the *resource* side of text: how font requirements are detected on a `StyledDom`, how families are resolved to OS fonts, how the bytes get parsed into `ParsedFont`, and how the result feeds back into [Inline Layout](inline-text3.md). The actual shaping pipeline (5 stages, `TextShapingCache`) lives on the inline-layout page.

## File map

| File | Purpose |
|---|---|
| `layout/src/font.rs` | `font::loading::build_font_cache`, `font::parsed::ParsedFont` (allsorts wrapper), `font::mock::MockFont` |
| `layout/src/font_traits.rs` | `ParsedFontTrait`, `FontLoaderTrait` — abstraction so `text3` can be tested with `MockFont` |
| `layout/src/text3/default.rs` | `PathLoader` — disk-based `FontLoaderTrait` impl |
| `layout/src/text3/cache.rs` | `FontContext`, `FontManager<T>`, `LoadedFonts<T>`, `FontChainKey` |
| `layout/src/text3/glyphs.rs` | `ShapedGlyph`, glyph-instance conversion |
| `layout/src/text3/script.rs` | `Script`, `Language`, `script_to_language`, script detection |
| `layout/src/glyph_cache.rs` | CPU-renderer glyph-path and cell cache (separate from shaping) |
| `layout/src/window.rs` | `LayoutWindow.layout_dom_recursive` runs the 5-step pipeline |

## The 5-step font resolution pipeline

`LayoutWindow::layout_dom_recursive` (`window.rs:854`) runs the full sequence before each `solver3::layout_document` call:

```
StyledDom
   │
   ▼ Step 0: collect_font_stacks_from_styled_dom (solver3/getters.rs)
   │   walk per-node CSS, collect StyleFontFamilyVec per node
   │
   ▼ Step 1: collect_and_resolve_font_chains_with_registration
   │   resolve_font_chains: (font-family, weight, style) → FontFallbackChain
   │   Unicode-fallback fonts are pruned to scripts present in the DOM
   │
   ▼ Step 2: collect_font_ids_from_chains
   │   diff against font_manager.parsed_fonts to find missing FontIds
   │
   ▼ Step 3: PathLoader::load_font_shared (text3/default.rs)
   │   read disk bytes, parse via ParsedFont::from_bytes
   │
   ▼ Step 4: FontManager::set_font_chain_cache_with_sig
       installs new font_chain_cache, stashes prev_font_hashes signature
```

`text_cache.layout_flow` then reads `font_manager.font_chain_cache` (per-stack chains) + `font_manager.parsed_fonts` (loaded faces) without holding any locks during shaping.

## Per-node `font_family_hash` and the `font_dirty_nodes` set

`build_compact_cache` (`core/src/compact_cache_builder.rs`) computes a per-node `font_family_hash`:

```rust,ignore
let mut hasher = DefaultHasher::new();
families.hash(&mut hasher);
let h = hasher.finish();
result.tier2b_text[i].font_family_hash = if h == 0 { 1 } else { h };
```

`0` is reserved as an "unset" sentinel. `compact_cache.font_dirty_nodes: Vec<NodeId>` is the set of nodes whose `font_family_hash` differs from `compact_cache.prev_font_hashes[i]`. `prev_font_hashes` is the snapshot from the previous frame.

`LayoutWindow::layout_dom_recursive` reads `font_dirty_count = compact_cache.font_dirty_nodes.len()` and the polynomial rolling hash over `prev_font_hashes`:

```rust,ignore
let font_stacks_sig = compact_cache_ref.map(|cc| {
    let mut h: u64 = 0xcbf29ce484222325;
    for &fh in cc.prev_font_hashes.iter() {
        h = h.rotate_left(13) ^ fh;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
});

let font_requirements_unchanged = (font_dirty_count == 0
    && !self.font_manager.font_chain_cache.is_empty())
    || (font_stacks_sig.is_some()
        && font_stacks_sig == self.font_manager.last_resolved_font_stacks_sig
        && !self.font_manager.font_chain_cache.is_empty());
```

If `font_requirements_unchanged`, all 5 steps are skipped — saves ~1.5 ms cold / ~0.9 ms warm.

The signature catches the case where `build_compact_cache` did not re-run between layouts but the font stacks are identical to what is already cached. Without this fallback, every layout call that bypasses the compact cache rebuild (e.g., scroll-only, hover-only) would hit the resolver pointlessly.

## Why XOR is not enough

The original design used a single `font_stacks_hash: u64` field on `LayoutWindow`, computed as the XOR of every node's `font_family_hash`. The intent was: if XOR is unchanged, the multiset of font requirements is unchanged.

Two failure modes:

1. **XOR collision.** XOR(a, b, a, b) == 0. Adding font A to one node and removing A from another in the same frame leaves XOR unchanged. The font is still in the DOM, but the snapshot looks "clean".
2. **Granularity.** A single per-window XOR cannot tell which *node* changed fonts. The whole pipeline runs even when one heading switched family.

The current design fixes (1) by checking `font_dirty_nodes.len()`, which is computed by per-node comparison, not XOR. (2) is partially fixed: the resolver still runs over the full DOM, but the per-node `font_dirty_nodes` list is the substrate for future incremental resolution. The legacy `font_stacks_hash` field on `LayoutWindow` (`window.rs:481`) is still present but only as a backup signal.

The full discussion is in `scripts/FONT_INVALIDATION_AND_MEMORY_LAYOUT_ANALYSIS.md` § Part 1.

## `FontChainKey` and the chain cache

`FontChainKey` (defined in `text3/cache.rs`) keys a fallback chain by `(font_family_hash, weight, style)`. Two font stacks with the same hash but different weights resolve to different chains.

```rust,ignore
pub struct FontFallbackChain {
    pub primary: FontId,
    pub fallbacks: Vec<FontId>,                  // unicode-fallback ladder
    pub coverage: BTreeMap<UnicodeRange, FontId>, // per-range overrides
}
```

`FontId` is a stable identifier into `font_manager.parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>`. The chain is read-only during shaping; shapers walk `primary → fallbacks` checking codepoint coverage.

`FontFallbackChain::resolve_char(c)` is the per-codepoint hot path. Coverage is precomputed per font during `ParsedFont::from_bytes`.

## `ParsedFont` (allsorts-backed, lazy)

`font/parsed/ParsedFont` (`font.rs:292`) is the parsed in-memory font. It holds:

- `hash: u64` — cache key
- `font_metrics: LayoutFontMetrics` — ascent, descent, line gap (for `LineHeight::Normal` resolution)
- `pdf_font_metrics: PdfFontMetrics` — detailed metrics for PDF embedding
- `num_glyphs: u16`, `hhea_table`, `vhea_table`, `maxp_table` — shaping/layout tables
- `gsub_bytes: Option<Vec<u8>>` + `gsub_cache_lazy: OnceLock<Option<GsubCache>>` — GSUB is parsed on first shape call, not at `from_bytes` time
- `loca_glyf: LocaGlyfState` — `Loaded(Option<...>)` for eager/CFF, `Deferred { bytes, font_index, loaded }` for lazy decode (re-parseable after eviction)
- `last_used: AtomicU64` — monotonic-clock nanos for LRU eviction

Lazy parsing is critical for memory: parsing a 20 MiB CJK font's GSUB table takes ~50 MiB resident; deferring it until first shape call drops cold-startup memory by that amount when the DOM never needs CJK shaping.

`from_bytes(bytes, index, warnings)` is the entry point. It parses headers + cmap eagerly and populates `LocaGlyfState::Deferred` for the outline tables. `get_or_decode_glyph(glyph_id)` triggers full decode on first miss, with two-step double-check inside a `Mutex<Option<...>>` to keep the expensive `LocaGlyf::load` outside the critical section.

## `LocaGlyfState` — eviction-aware lazy decode

```rust,ignore
pub(crate) enum LocaGlyfState {
    Loaded(Option<Arc<Mutex<LocaGlyf>>>),
    Deferred {
        bytes: Arc<rust_fontconfig::FontBytes>,
        font_index: usize,
        loaded: Arc<Mutex<Option<Arc<Mutex<LocaGlyf>>>>>,
    },
}
```

`Loaded(None)` means CFF or no outline data — cannot be evicted because there are no source bytes to re-decode from. The eager `from_bytes` path (tests, PDF generation via `with_source_bytes`) produces this variant.

`Deferred` keeps an `Arc<FontBytes>` so `FontManager::evict_unused` can drop the parsed `LocaGlyf` and force re-decode on next access. The `Mutex<Option<...>>` wrapper (rather than `OnceLock`) is what makes idle-eviction possible.

## `MockFont` for testing

`font::mock::MockFont` (`font.rs:71`) implements `ParsedFontTrait` without any allsorts dependency. Tests that exercise text layout without real fonts construct a `MockFont` with explicit per-glyph advance widths and use `FontManager<MockFont>`:

```rust,ignore
pub struct MockFont {
    pub font_metrics: LayoutFontMetrics,
    pub space_width: Option<usize>,
    pub glyph_advances: BTreeMap<u16, u16>,
    pub glyph_sizes: BTreeMap<u16, (i32, i32)>,
    pub glyph_indices: BTreeMap<u32, u16>,
}
```

`MockFont::new(metrics).with_space_width(10).with_glyph_advance(65, 600)` builds a font where 'A' has 600 font-units of advance. Tests assert exact pixel positions, which would be brittle against real font versions.

## `FontLoaderTrait` and `PathLoader`

`font_traits::FontLoaderTrait` defines how `FontManager::load_missing_for_chains` reads bytes. The default `PathLoader` (`text3/default.rs`) is disk-based:

```rust,ignore
pub trait FontLoaderTrait {
    fn load_font_shared(
        &self,
        bytes: Arc<FontBytes>,
        font_index: usize,
    ) -> Result<FontRef, LayoutError>;
}
```

Other loaders exist: in-memory loaders for embedded fonts (Material Icons, see `layout/src/icon.rs`), the headless renderer's loader, and a CPU-renderer-specific loader that pre-warms outline decode. All convert bytes to a `FontRef` (defined in `azul-css`) which wraps a raw pointer + destructor — the FFI-stable handle.

`parse_font_fn` and `parsed_font_to_font_ref` (`layout/src/lib.rs`) are the canonical bytes → `FontRef` adapter:

```rust,ignore
pub fn parse_font_fn(source: LoadedFontSource) -> Option<FontRef> {
    ParsedFont::from_bytes(
        source.data.as_ref(),
        source.index as usize,
        &mut Vec::new(),
    )
    .map(parsed_font_to_font_ref)
}
```

`parsed_font_to_font_ref` boxes the `ParsedFont` and stores it as `*const c_void` with a destructor pointer; `font_ref_to_parsed_font` reverses the cast. The unsafe cast is sound iff every `FontRef` was created by `parsed_font_to_font_ref` — by convention in the codebase.

## `FcFontCache` and rust-fontconfig 4.1

`rust-fontconfig` is the system-fontconfig replacement. Version 4.1 (current) made `FcFontCache` an `Arc<RwLock>` shared handle internally — clone is cheap, and writes from a builder thread are immediately visible to all readers without an explicit refresh dance.

`build_font_cache()` (`font.rs:24`) is the eager builder; `FcFontRegistry` (in `rust-fontconfig`) is the lazy builder that parses fonts on demand. `FontContext::from_registry(registry)` wires the registry into the chain resolver via `FontFallbackChain::request_and_resolve_with_scripts`, which priority-bumps the builder for missing families and waits for them.

The scout-on-demand path matters for headless rendering: parsing every system font eagerly takes ~15 MiB resident on macOS. A headless CPU rasterizer that only needs Arial doesn't need to pay that cost.

Under `cfg(miri)`, `build_font_cache()` returns a default empty cache so Miri test runs don't try to walk the filesystem.

## `LoadedFonts<T>` — the per-window pool

`text3::cache::LoadedFonts<T>` is the pool that shaping reads. It wraps `font_manager.parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>`. The `Arc` is shared across windows that opt into pooling via `LayoutWindow::new_with_shared_fonts(fc_cache, parsed_fonts)`.

Sharing matters when the app opens multiple windows: each window does its own font *resolution* (chains depend on the per-window DOM), but all windows pull parsed faces from the same pool. Without sharing, each window re-parses every font on first use.

## `FontContext` (the app-wide handle)

`FontContext` (`text3/cache.rs:533`) is the `App`-owned warmup handle:

```rust,ignore
pub struct FontContext {
    pub fc_cache: FcFontCache,
    pub parsed_fonts: Arc<Mutex<HashMap<FontId, FontRef>>>,
    pub font_chain_cache: HashMap<FontChainKey, FontFallbackChain>,
    pub embedded_fonts: HashMap<u64, FontRef>,
    pub font_hash_to_families: HashMap<u64, StyleFontFamilyVec>,
    pub registry: Option<Arc<FcFontRegistry>>,
}
```

`FontContext::pre_resolve_chains_for_dom(styled_dom, platform)` runs Steps 0 and 1 of the pipeline ahead of time. PDF generators and headless renderers call this to avoid a layout-time spike; production GUIs typically don't. Use `FontContext::load_fonts_for_chains` to also run Steps 2–3.

`to_font_manager()` clones `FontContext` into a `FontManager<FontRef>` for a window, sharing the `parsed_fonts` Arc.

## `Script` and `Language`

`text3/script.rs:Script` is an enum over Unicode script codes (ISO 15924). `script_to_language(script) -> Option<Language>` maps a script to a default language tag (Latin → English, Hangul → Korean, …). Used by the shaper for OpenType locale-aware features (`locl`, `liga` variants).

Script detection on a string runs `unicode-script` per codepoint and groups runs by script. `LogicalItem::Text` carries `script: Script` after Stage 1; the shaper (Stage 3) selects fallback chains by script.

`scripts_present_in_styled_dom(styled_dom)` (in `solver3/getters.rs`) walks every text node, returning the `BTreeSet<Script>` of scripts that appear. This drives Unicode-fallback pruning during chain resolution: don't pull in CJK fallbacks if no CJK character is present.

## `glyph_cache.rs` — CPU rendering only

Distinct from shaping: this is the cell-and-path cache for the CPU rasterizer (`feature = "cpurender"`). It memoises rasterized glyph cells (small bitmaps) keyed by `(FontId, glyph_id, size, subpixel_position)` and decoded glyph paths for SDF/distance-field rendering. Hardware (WebRender) rendering doesn't use this cache — WebRender has its own glyph-rasterization cache.

The split keeps CPU-rendering memory off the GPU path. Headless tests under `feature = "cpurender"` populate `glyph_cache` and serve from it; production GPU renders never touch it.

## Memory eviction (`evict_unused`)

`FontManager::evict_unused(threshold)` walks `parsed_fonts`, finds `ParsedFont` entries whose `last_used` is older than the threshold, and:

1. Clears `gsub_cache_lazy` (parsed GSUB) — re-parseable from `gsub_bytes`.
2. Transitions `LocaGlyfState::Deferred.loaded` from `Some(arc)` to `None` — re-parseable from `bytes`.
3. Decrements `parsed_fonts` reference if no other window holds the FontId.

`Loaded(None)` and `Loaded(Some(_))` variants from the eager path can't be evicted because there are no `bytes` to re-decode from. Tests that load with `from_bytes` and never set up a `Deferred` state will retain GSUB indefinitely; production fonts loaded via `PathLoader::load_font_shared` start as `Deferred`.

## The CSS getter side

`solver3/getters.rs` carries the cached CSS reads:

| Function | Returns |
|---|---|
| `collect_font_stacks_from_styled_dom(styled_dom, platform)` | `BTreeMap<NodeId, StyleFontFamilyVec>` plus stacks the platform's default font-family in front of `serif` / `sans-serif` / `monospace` keywords |
| `resolve_font_chains(stacks, fc_cache, scripts_hint)` | `ResolvedFontChains { chains: HashMap<FontChainKeyOrRef, FontFallbackChain> }` |
| `collect_used_codepoints(styled_dom)` | `BTreeSet<char>` — used by `prune_chain_to_used_chars` to drop fallback fonts that contribute no codepoints |
| `scripts_present_in_styled_dom(styled_dom)` | `BTreeSet<Script>` |
| `collect_and_resolve_font_chains_with_registration(...)` | High-level wrapper that does all of the above plus registry-driven scout-on-demand |

`platform: &azul_css::system::Platform` is read from `LayoutWindow.system_style.platform`, falling back to `Platform::current()` at compile time. Different platforms map `serif` to different concrete families (Times New Roman on Windows, Times on macOS, Liberation Serif on Linux).

## Summary table

| Concern | Where it lives | Cache scope |
|---|---|---|
| System font path discovery | `rust-fontconfig` `FcFontCache` | App (shared via Arc) |
| (family, weight, style) → `FontFallbackChain` | `font_chain_cache` in `FontManager` | Window |
| Parsed font tables (`ParsedFont`) | `parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>` | Window pool, shareable across windows |
| Lazy GSUB / loca+glyf | `OnceLock` / `Mutex<Option>` inside `ParsedFont` | Per-font face |
| Per-stack `font_family_hash` | `tier2b_text[i].font_family_hash` in `compact_cache` | Per-DOM, rebuilt by `build_compact_cache` |
| Dirty-node list for fonts | `compact_cache.font_dirty_nodes` | Per-DOM, rebuilt by `build_compact_cache` |
| Chain-cache invalidation signature | `FontManager.last_resolved_font_stacks_sig` | Window |
| Shaped glyph runs | `TextShapingCache.shaped_items` + `per_item_shaped` | Window (text engine) |
| Rasterized glyph cells | `glyph_cache.rs` | Window (CPU renderer only) |
