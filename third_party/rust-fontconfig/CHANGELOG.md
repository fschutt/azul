# Changelog

All notable changes to this project will be documented in this file.

## [4.4.3] - 2026-06-06

### Fixed

- **Bundled in-memory fonts were unusable on caches with no system fonts**
  (headless / wasm / embedder-bundled-font setups). A font registered via
  `FcFontCache::with_memory_fonts` with a naive pattern (the font bytes plus
  a name, but an empty `unicode_ranges`) could never be selected to shape
  any character. Two independent root causes, both fixed:

  1. `with_memory_fonts` / `with_memory_font_with_id` stored the empty
     `unicode_ranges` verbatim, and `FontFallbackChain::resolve_char`
     deliberately skips any font that reports no coverage. They now
     auto-populate `unicode_ranges` from the font's cmap/OS2 tables when the
     caller leaves them empty, reusing the exact pipeline the on-disk
     builder uses (`FcParseFontBytes` → `parse_font_faces`). This requires
     the `parsing` feature; without it the caller-supplied pattern is stored
     unchanged and the caller must populate `unicode_ranges` themselves.

  2. Generic CSS families (`serif` / `sans-serif` / `monospace`) were
     expanded to a hardcoded list of real per-OS font names and the generic
     name itself was dropped, so a registered memory font (whatever its
     family name) was never reached. The chain builder now falls back to a
     generic `name: None` query for the originally-requested generic
     family **only when the expanded OS-specific stack matched nothing**, so
     systems with real fonts are unaffected and any fallback match comes
     after real matches.

## [4.4.0] - 2026-05-23

### Changed

- Bumped `allsorts-azul` 0.16.2 → 0.16.4 (semver-compatible patch bump
  within the 0.16 line).

### Fixed

- **WASM builds**: `FontBytes::Mmapped(mmapio::Mmap)` was gated only on
  `std`, but `mmapio` is excluded on `target_family = "wasm"`, so the
  variant referenced a crate that isn't linked there. The variant and
  its match arms are now gated on `not(target_family = "wasm")`; WASM
  targets fall back to `FontBytes::Owned`.
- **C bindings (`ffi`) + examples**: realigned to the v4.x shared-cache
  API — `FontSource` → `OwnedFontSource` and
  `FcFontRegistry::into_fc_font_cache()` → `shared_cache()`, and the
  `&FcPattern` borrow now expected by `calculate_style_score`. The
  exported C ABI is unchanged.
- `--all-features` dead-code warning for `pattern_from_filename`: gated
  to match its sole caller, `build_from_filenames`
  (`std` + `not(parsing)`).
- `test_operating_system_font_expansion` assertions updated to match the
  macOS/iOS serif + sans-serif expansion lists shipped in 4.3.0.

## [4.2.1] - 2026-04-18

### Fixed

- **`FcFontRegistry::request_fonts`: build_queue leak after `build_complete`**.
  Promote the existing `cache_loaded` fast-path at the top of the function
  into a joint `cache_loaded || build_complete` short-circuit. Once
  either is true, the pattern map is fully settled; walking `known_paths`
  to compute "missing" / "incomplete" family lists and pushing
  `FcBuildJob` items into `build_queue` is wasted work whose only
  observable effect is a steady leak of ~13 KiB per call — the builder
  threads have shut down and nothing drains the queue.

  Discovered via per-phase heap probes in a downstream resize-loop
  regression (~100 MiB RSS growth across a 5-second interactive
  session). Each call was retaining ~158 `FcBuildJob` items, one per
  permanently-missing system family (Arabic / CJK / etc.).

### Added

- Optional fine-grained heap probes inside `request_fonts`, gated
  behind `AZ_PROFILE=heap,jsonl,detail` + `AZ_PROFILE_OUT=<path>`.
  Permanent diagnostic infrastructure for future memory
  investigations; inert unless both env vars are set.

- `FcFontCache::chain_cache_len()` — cheap accessor returning the
  current number of cached resolved chains.

## [3.3.0] - 2026-04-14

### Added

- **`FcFontCache::get_font_bytes_arc`**: Returns font bytes as a
  shared `Arc<[u8]>`. Multiple `FontId`s backed by the same file
  content (every face of a `.ttc`, or two paths holding identical
  bytes) now return the *same* `Arc`, so downstream parsers that
  hold the bytes no longer duplicate them per face. The existing
  `get_font_bytes -> Vec<u8>` is kept as a thin wrapper.

- **`FcFontPath::bytes_hash: u64`**: Deterministic 64-bit content
  hash of the file's byte contents, computed once per file at
  parse time. Used as the key for the Arc-sharing cache. A value
  of `0` means "not computed" (e.g. built from a filename-only
  scan, or loaded from a legacy v1 disk cache) and callers should
  treat it as opaque.

- **`DEFAULT_UNICODE_FALLBACK_SCRIPTS`** (pub const): The 7 script
  blocks `resolve_font_chain` pulls in by default (Cyrillic, Arabic,
  Devanagari, Hiragana, Katakana, CJK Unified, Hangul).

- **`FcFontCache::resolve_font_chain_with_scripts`**: New primary
  entry point for fallback-chain resolution. Accepts
  `scripts_hint: Option<&[UnicodeRange]>`:
  - `None` → current behaviour (all 7 default scripts).
  - `Some(&[])` → no Unicode fallbacks attached (for ASCII-only
    documents this avoids dragging Arial Unicode MS and CJK
    fonts into the resolved chain).
  - `Some(&[CJK])` → only CJK fallback attached.

  The chain cache is keyed so a no-scripts-hint resolution can't
  be served from a slot filled by an all-scripts resolution.

- `utils::content_hash_u64` — stable-across-runs 64-bit byte hash.

### Changed

- Disk-cache `FontManifest::CURRENT_VERSION` bumped from `1` → `2`
  to persist `bytes_hash` per file. Existing v1 caches are
  invalidated on load (triggers a clean re-scan).

- `FontCacheEntry` now has a `bytes_hash: u64` field
  (`#[serde(default)]` for forward-compat).

### Unchanged / Back-compat

- `resolve_font_chain` / `resolve_font_chain_with_os` keep their
  signatures and their "default 7 scripts" behaviour.
- `get_font_bytes` keeps its `Option<Vec<u8>>` signature; it now
  just clones from `get_font_bytes_arc` internally.

## [2.0.0] - 2026-02-14

### Breaking Changes

- **`FontId` now uses atomic counter instead of `SystemTime`**: Font IDs are now
  assigned via a global atomic counter (`AtomicU128`), making them deterministic
  and reproducible across runs. Code that compared `FontId` values across sessions
  or relied on their magnitude encoding time will break.

### Added

- **`FcFontRegistry`**: New async font registry with background scanning and
  on-demand font loading. Requires the `async-registry` feature.
  - `FcFontRegistry::new()` — creates a new registry (returns `Arc<Self>`)
  - `register_memory_fonts()` — register in-memory fonts with priority
  - `spawn_scout_and_builders()` — start background directory scanning + font parsing
  - `request_fonts()` — request specific font families (prioritized loading)
  - `into_fc_font_cache()` — convert to `FcFontCache` for compatibility
  - `shutdown()`, `is_scan_complete()`, `is_build_complete()`, `progress()`

- **Disk cache** (`cache` feature): Serializes parsed font metadata to disk via
  `bincode`/`serde`, dramatically speeding up subsequent launches.
  - `FcFontRegistry::load_from_disk_cache()` / `save_to_disk_cache()`
  - `FontManifest`, `FontCacheEntry`, `FontIndexEntry` structs

- **`FcFontCache::build_with_families()`**: Build a cache that only scans and
  parses fonts matching specific family names, much faster than `build()` when
  you know which fonts you need.

- **`Debug` impl for `FcFontRegistry`**: Shows registry state (scan progress,
  font counts, memory fonts).

### Fixed

- **Italic font race condition**: `FcFontRegistry` now waits for all font file
  variants (regular, bold, italic, etc.) to be parsed before resolving font
  queries, preventing cases where italic variants were missing from results.

- **Font scoring**: When style is `DontCare`, prefer `Normal` over `Italic`
  variants. This fixes cases where italic fonts were incorrectly chosen as the
  default match.

- **Memory font preference**: `query()` now prefers memory fonts over disk fonts
  when both match equally, ensuring programmatically registered fonts take
  priority.

- **Test fix**: Arial Regular test pattern now explicitly sets `bold: False`
  instead of `DontCare` for correct scoring behavior.

## [1.2.2] - 2025-12-01

### Added

- `FcParseFontBytes`: Parse in-memory font data without building a full cache.

## [1.2.1] - 2025-11-26

### Fixed

- **Issue #15**: Windows font paths no longer assume C: drive. Now uses `SystemRoot`/`WINDIR` environment variable for system fonts and `USERPROFILE` for user fonts, with proper fallbacks.

- **Issue #17**: Removed duplicate `FcFontCache::build()` implementation that caused compilation errors when building without `std` or `parsing` features.

- **Issue #18**: Fixed compilation without `parsing` feature. All `allsorts` imports and dependent functions are now properly guarded with `#[cfg(feature = "parsing")]`.

## [1.2.0] - 2025-06-03

### Breaking Changes

- **`resolve_font_chain()` signature changed**: The `text` parameter has been removed. Font chains are now resolved based on CSS properties only (font-family, weight, italic, oblique), not text content.
  
  Old API:
  ```rust
  cache.resolve_font_chain(&families, text, weight, italic, oblique, &mut trace)
  ```
  
  New API:
  ```rust
  cache.resolve_font_chain(&families, weight, italic, oblique, &mut trace)
  ```

- **`query_all()` method removed**: Use `cache.list()` with filtering instead.
  
  Old API:
  ```rust
  let fonts = cache.query_all(&pattern, &mut trace);
  ```
  
  New API:
  ```rust
  let fonts: Vec<_> = cache.list().into_iter()
      .filter(|(pattern, _id)| /* your filter */)
      .collect();
  ```

- **`query_for_text()` moved to `FontFallbackChain`**: Text-to-font resolution now requires a font chain first.
  
  Old API:
  ```rust
  let fonts = cache.query_for_text(&pattern, text, &mut trace);
  ```
  
  New API:
  ```rust
  let chain = cache.resolve_font_chain(&families, weight, italic, oblique, &mut trace);
  let font_runs = chain.query_for_text(&cache, text);
  ```

### Added

- **`FontFallbackChain::resolve_text()`**: Returns per-character font assignments as `Vec<(char, Option<(FontId, String)>)>` for fine-grained control.

- **`FontFallbackChain::resolve_char()`**: Resolve a single character to its font.

- **`CssFallbackGroup` struct**: Groups fonts by their CSS source name, making it clear which CSS font-family each font came from.

- **Font chain caching**: Identical CSS font-family stacks now share cached font chains, improving performance when the same fonts are used with different text content.

### Changed

- **Architecture**: The new two-step workflow (chain resolution → text querying) better matches CSS/browser font handling semantics and enables better caching.

- **Performance**: Font chains are now cached by CSS properties, avoiding redundant font resolution for the same font-family declarations.

### Rationale

The API was refactored to separate concerns:
1. **Font chain resolution** (`resolve_font_chain`): Determines which fonts to use based on CSS font-family, weight, and style. This is typically done once per CSS declaration.
2. **Text-to-font mapping** (`resolve_text`/`query_for_text`): Maps text content to specific fonts in the chain. This is done per text string to render.

This separation enables:
- Better caching (same CSS fonts can be reused for different text)
- Clearer API semantics matching CSS behavior
- More efficient text layout pipelines

## [1.1.0] - 2025-11-25

### Added

- Better font resolution algorithms
- Performance improvements for font matching

## [1.0.3] - Previous

### Added

- Derive `Hash` on public types

## [1.0.2] - Previous

- Bug fixes and improvements

## [1.0.1] - Previous

- Bug fixes

## [1.0.0] - Previous

- Initial stable release
- Font matching by name, family, and style properties
- Unicode range support
- In-memory font loading
- C API bindings
- Cross-platform support (Windows, macOS, Linux, WASM)
