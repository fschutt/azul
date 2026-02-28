# Bloaty Analysis: `libazul.dylib`

**Binary:** `/Users/fschutt/Development/azul/target/release/libazul.dylib`
**Build:** Release with debug symbols
**Total size:** 27.2 MB
**Analysis date:** 2026-02-26

---

## Section Breakdown

| Size | % | Section |
|------|---|---------|
| 11.6 MB | 42.5% | `__TEXT,__text` (code) |
| 7.18 MB | 26.4% | `__TEXT,__const` (constants/data) |
| 3.19 MB | 11.7% | String Table |
| 3.04 MB | 11.2% | Symbol Table |
| 1.46 MB | 5.3% | `__DATA_CONST,__const` |
| 216 KB | 0.8% | Code Signature |
| 212 KB | 0.8% | Export Info |
| 135 KB | 0.5% | `__TEXT,__cstring` |

---

## Top Anonymous Constants (Largest Data Blobs)

| Size | Symbol | Identified as |
|------|--------|---------------|
| 2.77 MB | `_anon.66787c…` | **ICU segmenter dictionaries** — Thai/Burmese/CJK word-break data from `icu_segmenter_data` |
| 917 KB | `_anon.798763…` | `encoding_rs` charset lookup tables |
| 650 KB | `_anon.4faf4e…` | **Azul debug server** embedded HTML/CSS/JS strings (E2E test routes) |
| 499 KB | `_anon.b005ec…` | **ICU locale blob** from `icu_provider_blob` (root locale `"und"`) |
| 372 KB | `_anon.77f5508…874` | **`regex_automata` compiled DFA tables** |
| 307 KB | `_anon.eed79d…` | Unknown (likely more encoding/regex data) |
| 299 KB | `_anon.c36a09…` | More debug server strings ("Bad datastore lookup", etc.) |
| 228 KB | `_anon.77f5508…742` | More `regex_automata` NFA tables |
| 148 KB | `_ecp_nistz256_precomputed` | `ring` crypto — P-256 ECDH precomputed table |
| 119 KB | `brotli_decompressor::dictionary::kBrotliDictionary` | Brotli decompression dictionary |

**ICU total: ~3.27 MB** (2.77 MB segmenter + 499 KB locale blob)
**regex tables total: ~600 KB**
**Debug server embedded strings: ~950 KB**

---

## Top Named Code Symbols

| Size | Symbol |
|------|--------|
| 67.9 KB | `azul::desktop::shell2::common::debug_server::process_debug_event` |
| 67.3 KB | `azul::desktop::shell2::macos::gl::GlFunctions::initialize` |
| 67.2 KB | `taffy::compute::grid::track_sizing::resolve_intrinsic_track_sizes` |
| 56.6 KB | `azul_css::CssProperty::clone` |
| 52.2 KB | `azul_layout::widgets::text_input::TEXT_INPUT_CONTAINER_PROPS` |
| 41.0 KB | `webrender::renderer::Renderer::draw_frame` |
| 36.9 KB | `encoding_rs::data::BIG5_LOW_BITS` |
| 23.1 KB | debug server `ExportedLibraryResponse` deserialize visitor |

---

## Dependency Rlib Sizes (unstripped, from `target/release/deps/`)

These are pre-monomorphization `.rlib` sizes; actual contribution to the binary is smaller.

| rlib | Size |
|------|------|
| `libazul_layout` | 131.4 MB |
| `libobjc2_app_kit` | 129.7 MB |
| `libazul` | 119.0 MB |
| `libobjc2_foundation` | 62.5 MB |
| `libwebrender` | 60.8 MB |
| `libazul_core` | 55.2 MB |
| `libazul_css` | 39.5 MB |
| `libicu_datetime` | 36.7 MB |
| `liballsorts` | 32.8 MB |
| `libimage` | 28.3 MB |
| **`libmoxcms`** | **23.4 MB** |
| `librustls` | 21.3 MB |
| `libicu_datetime_data` | 20.3 MB |
| `libicu_segmenter` | 15.8 MB |
| `libregex_automata` | 15.7 MB |
| `librust_fontconfig` | 13.4 MB |
| `libregex_syntax` | 13.1 MB |
| `libwebrender_api` | 12.0 MB |
| `libicu_segmenter_data` | 11.9 MB |
| **`libpxfm`** | **9.96 MB** |

---

## Optimization Opportunities

### High Priority

#### 1. ICU — ~3.3 MB
- `icu_segmenter_data` (2.77 MB) + `icu_provider_blob` (499 KB)
- **Fix:** New `icu_macos` feature uses `NSNumberFormatter` / `NSDateFormatter` / `NSListFormatter` / `NSString.localizedCompare:` instead of bundling ICU4X data blobs. Plural rules use a compact hardcoded CLDR lookup table. `build-dll` now uses `"icu_macos"` instead of `"icu"`.
- **Status:** Done — `layout/src/icu_macos.rs` + cfg-gated `icu.rs`

#### 2. `regex` — ~600 KB
- `regex_automata` DFA/NFA tables compiled for 4 trivial patterns used only in `try_wlr_randr()` in `dll/src/desktop/display.rs`
- **Fix:** Replace 4 regex patterns with string parsing; remove `regex` dep
- **Status:** Done (see PR/commit)

---

### Medium Priority

#### 3. WebP / `moxcms` / `pxfm` — rlib overhead
- Dependency chain: `image` (webp feature) → `image-webp v0.2.4` → `moxcms v0.7.9` → `pxfm v0.1.25`
- `libmoxcms` is 23.4 MB rlib, `libpxfm` is 9.96 MB rlib
- **Fix:** Remove `webp` from `all_img_formats` or from `build-dll` defaults
- **Risk:** WebP is a commonly expected image format; may break apps that use it
- **Status:** Under investigation

#### 4. `backtrace` — small
- `backtrace = { version = "0.3.66", optional = true }` is enabled by `build-dll` and `link-static`
- Not useful in release builds without debug info parsing
- **Fix:** Remove from `build-dll`/`link-static`; enable only in `dev` profile or behind a feature flag
- **Status:** Not yet done

#### 5. Debug server (`spmc`, `strum`) — small
- `spmc` provides an SPMC channel used exclusively in `debug_server.rs` for timer→window thread communication
- `strum` used in `debug_server.rs` for enum iteration over `DebugRequest` variants (also used in `azul-css` so can't fully remove)
- **Fix:** Gate the debug server behind an optional feature (e.g., `debug-server`); `spmc` goes away completely
- **Status:** Not yet done

---

### Low Priority / Keep

#### 6. `tfd` (tiny-file-dialogs)
- Native file open/save dialogs on all platforms
- Deeply integrated; removing would break file dialog API
- **Decision:** Keep

#### 7. ICU (alternative) — localization/date formatting
- `icu` is pulled in by `azul-layout` for proper Unicode segmentation, date formatting, etc.
- **Decision:** Keep as opt-in feature; do NOT include in default `build-dll`

---

## Summary Table

| Item | Actual binary impact | Action | Status |
|------|---------------------|--------|--------|
| ICU segmenter + locale data | ~3.3 MB | macOS Foundation backend (`icu_macos`) | Done |
| `regex` tables | ~600 KB | Replace with string ops | Done |
| `moxcms` + `pxfm` (WebP) | significant rlib overhead | Investigate removing `webp` from defaults | TODO |
| `backtrace` | small | Gate to dev-only | TODO |
| `spmc` / debug server | small | Optional feature gate | TODO |
