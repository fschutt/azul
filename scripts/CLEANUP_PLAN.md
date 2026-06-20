# Azul Cleanup Plan

Investigation-backed checklist from the 2026-06-20 code-review pass. Each item carries the
verified current state (file:line) and a recommended action. Categories:
**REMOVE** / **REFACTOR** / **BUILD-OUT** / **KEEP** (no action, finding corrected) / **INVESTIGATE**.

Effort: 🟢 small · 🟡 medium · 🔴 large.

---

## core/ crate

- [x] **animation.rs — overpromising stub** 🟢 — **DONE:** `UpdateImageType` is not FFI-exported
  (absent from api.json); folded it into `core/src/resources.rs` (the image domain, next to
  `change_node_image` consumers) with an accurate doc, updated the 3 import sites
  (`layout/callbacks.rs`, `window.rs`, `widgets/capture_common.rs`), dropped `pub mod animation` from
  `core/src/lib.rs`, and deleted `core/src/animation.rs`.

- [x] ~~ui_solver.rs — remove~~ → **KEEP** 🟢 — `core/src/ui_solver.rs` (53 lines) defines
  `ResolvedOffsets` + `GlyphInstance`, used by `core/src/resources.rs:48`. Not dead. No action.

- [x] ~~callbacks.rs 822-862 — duplicated docs~~ → **KEEP** 🟢 — Lines 822-834 / 836-854 document
  *different* items (`CoreCallbackType` vs `CoreCallback`); the recurring "usize is actually a fn
  ptr" note is intentional. No true duplication. No action.

- [x] ~~db.rs — unusable~~ → **KEEP** 🟢 — `core/src/db.rs` (165 lines): engine-agnostic SQL POD
  types (`DbValue`/`DbValueVec`/`DbRows`) deliberately in core; the `Db` handle lives in dll behind
  `db-sqlite`. Has 3 tests. Consumers are in dll/FFI. No action.

- [ ] **events.rs — move tests out** 🟢 — `core/src/events.rs` is 3686 lines; `mod tests` spans
  3249-3686 (~437 lines, ~16 tests). **Action:** move to `events/tests.rs` (or `#[path]` include).
  Cuts file ~12%. **⚠ DEFERRED — user is handling this one manually (do NOT touch).** Approach if
  needed: extract body lines 3250-3685 into `core/src/events/tests.rs`, replace the inline module
  with `#[cfg(test)] #[path = "events/tests.rs"] mod tests;` (`use super::*` keeps resolving).

- [ ] **gpu.rs — split `synchronize`** 🟡 — `GpuValueCache::synchronize` is `core/src/gpu.rs:99-347`
  (~248 lines). **Action:** split into per-property-category helpers/iterators.

- [ ] **Merge hit_test.rs + hit_test_tag.rs** 🟡 — `core/src/hit_test.rs` (343) + `hit_test_tag.rs`
  (541) are tightly coupled (hit_test imports `hit_test_tag::CursorType`; tag file is pure
  encoding). **Action:** merge into one module (~884 lines).
- [ ] **⚠ CPU hit tester ignores CSS transforms** 🔴 (correctness, not cleanup) — zero `transform`
  matches in either file. **Action:** account for CSS transforms in hit testing. Tracks with the
  "use CPU hit tester always" decoupling-from-WebRender goal.

- [x] **changeset.rs trailing comment** 🟢 — **DONE:** removed the trailing comment in
  `layout/src/managers/changeset.rs`; the module-level docs (lines 8-11) already cover that the
  `create_*_changeset` helpers were removed and that the payload types are retained/used by
  `window.rs`/`undo_redo.rs`.

- [ ] **CssPropertyCache — macro-generate accessors** 🟡 — `core/src/prop_cache.rs` (5482 lines):
  ~92 of 181 `pub fn get_*` match the mechanical pattern
  `get_xxx() -> Option<&XxxValue> { self.get_property(..., &CssPropertyType::Xxx).and_then(|p| p.as_xxx()) }`.
  **Action:** `get_prop_accessor!(name, type, as_method, ValueType)` macro. Biggest LOC win.

- [ ] **RawImage::into_loaded_image_source — split** 🟡 — `core/src/resources.rs:1781-2202`
  (~420 lines), format-dispatch over pixel layouts. **Action:** split per-`RawImageFormat` arm into
  helpers.

- [x] **udp_framing.rs → deprecate for WebTransport** 🟢 — **DONE (reviewed, keep):** self-contained
  (178 lines, 5 tests) and explicitly conditioned on WebTransport/AzMeet, which is NOT part of this
  cleanup. Removing it now would break the still-live dll `Udp` handle (FFI-exported). KEEP until
  WebTransport lands; remove together with the dll item then. No change.

- [x] ~~core video — duplicated in widget~~ → **KEEP** 🟢 — `core/src/video.rs` (116 lines) is POD
  config/frame types only; widget logic is `layout/src/widgets/video.rs`. Correct FFI layering, not
  duplication. No action.

- [x] ~~clipboard paste/copy flow — verify accuracy~~ → **ACCURATE** 🟢 — documented in
  `layout/src/managers/clipboard.rs:1-27`; matches the stated Paste/Copy/Cut flows exactly. No
  action.

- [ ] **RefAny — add optional on-update / sync callback** 🔴 — `core/src/refany.rs` has no
  observer/notify hook; `clone` is a shallow refcount bump. **Action:** add an optional on-update fn
  on the RefAny inner box, fired on `downcast_mut`. Enables (a) client/server state sync on web and
  (b) the undo/redo deep-copy/snapshot backup. Foundational for two other items below.

---

## layout/ crate

- [x] ~~scroll_into_view.rs misplaced under "layout managers" / move to core~~ → **KEEP** 🟢 — it's
  `layout/src/managers/scroll_into_view.rs` (511 lines), correctly with the other managers, has a
  test file, and depends on `LayoutWindow` geometry (can't go to core). Not superseded. No action.

- [x] ~~Unify layout/hit_test.rs into core CPU hit tester~~ → **KEEP** 🟢 — `layout/src/hit_test.rs`
  (287 lines) is *cursor-type resolution* needing `LayoutWindow`/`StyleCursor`; `core/src/hit_test.rs`
  is data types only. Disjoint responsibilities; can't unify into core. Confusing names, but no
  action. (The real CPU-hit-tester correctness work is the transforms item under core.)

- [x] **headless.rs — trim diagram + verbose docs** 🟢 — **DONE:** replaced the two ASCII box
  diagrams, the GPU/headless comparison table, and the font/image lifecycle ASCII with an ~11-line
  prose summary (CPU path = `LayoutWindow → solver3 DisplayList → cpurender → PNG`; no GL/Renderer/
  RenderApi; `CpuHitTester`; `AZUL_HEADLESS=1`). Doc block went from ~77 lines to ~17.

- [x] **paged.rs — move to core** 🟢 — **DONE:** moved `FragmentationContext` + `PageMargins` to
  `core/src/paged.rs` (`use crate::geom::LogicalSize`), added `pub mod paged` to core lib, and
  replaced layout's `pub mod paged` with `pub use azul_core::paged;` so existing
  `azul_layout::paged::*` / `crate::paged::*` / `pub use paged::PageMargins` paths keep resolving.

- [x] **window_state.rs — small module review** 🟢 — **DONE (reviewed, keep as-is):** the module's
  own header (lines 3-5) documents that `WindowCreateOptions`/`FullWindowState` live here because
  `CallbackInfo` references them and must live in azul-layout. Merging 161 lines into the already
  7605-line `window.rs` would reduce clarity, not improve it; the split is valid. No change.

- [ ] **cpurender.rs — split monolith** 🔴 — `layout/src/cpurender.rs` (6086 lines, 126 fns) mixes
  compositor (`CompositorState`/`Layer`), scroll fast-path, `AzulPixmap` framebuffer, agg-rust
  rasterizer, SVG rasterizer (`render_svg_*` at 5247+), and test helpers. **Action:** split into
  `cpurender/{compositor,raster,svg,pixmap}.rs`.

- [x] **extra.rs — fold and delete** 🟢 — **DONE:** `coloru_from_str` was dead (only an unused
  import in `node_graph.rs`) → removed the import and the fn. `dom_from_parsed_xml` (FFI-used via
  api.json `Dom.create_from_parsed_xml`) intrinsically needs the `xml` feature, so it moved into
  `xml/mod.rs` (gated by `feature = "xml"`, which the dll enables; `extra` does not) and the api.json
  fn_body now reads `azul_layout::xml::dom_from_parsed_xml(xml)`. Dropped `pub mod extra` from the
  layout lib and deleted `extra.rs`. The xml-off stub was unreachable (no internal caller; FFI codegen
  targets xml-on builds) so it was dropped.

- [ ] **ICU — add cross-backend CI parity tests** 🟡 — three backends: `icu.rs` (1850, ICU4X
  default), `icu_macos.rs` (339, Foundation), `icu_windows.rs` (488, Win32 NLS); selected via
  features in `lib.rs:102-112`. **No ICU tests exist anywhere.** **Action:** add a CI parity test
  comparing macOS/Windows backend output against the ICU4X reference (number/date/list/plural).

- [ ] **layout clippy — tighten lints** 🟡 — `layout/src/lib.rs:13-39` blanket-allows
  `dead_code, unused_imports, unused_variables, unused_mut, unused_assignments` (+ more) crate-wide.
  **Action:** remove those 5 worst offenders, let AI fix the resulting errors/warnings
  incrementally.

- [ ] **`az_mark` / `az_mark_read` markers — remove/gate** 🟡 — 123 web-lift volatile-store
  diagnostic markers across the hot solver path (`window.rs`, `solver3/*`, `text3/cache.rs`),
  defined at `lib.rs:53/:61`. web-lift class-B is resolved (per memory). **Action:** remove or
  feature-gate them. (Also: 198 `unsafe` occurrences — heaviest in `callbacks.rs`, `solver3/fc.rs`,
  `text3/cache.rs`; audit candidate.)

- [ ] **SVG — unify on the DOM path** 🔴 — two divergent paths: DOM (`SvgNodeData::Path`) only
  produces a clip mask (can't paint fill/stroke); the working renderer is the direct rasterizer
  `cpurender.rs:5247 render_svg_to_png/_to_imageref/_group`. `widgets/map.rs:1001` documents the
  workaround (rasterize → image node). Types in `core/src/svg.rs` (1464) + `xml/svg.rs` (2352).
  **Action:** give `SvgNodeData::Path` real fill/stroke painting (or have the DOM display-list call
  `render_svg_group`), then remove the map.rs rasterize-to-image workaround.

---

## dll/ crate

### web/

- [x] **eventloop.rs — dangling "M11 plan" comments** 🟢 — **DONE:** rewrote the three doc comments
  that cited the missing M11 plan doc (`:260` Stage B.1 "high risk", `:962` Stage B.1, `:1312` "hard
  direction #4") to be self-contained (kept the technical rationale, dropped the dangling doc
  references). Left the descriptive "Sprint N" section markers as-is per the plan's scope.

- [ ] **html_render.rs — head/title support** 🟡 — body assembled at `:158`; HTML template at
  `:207-230` hardcodes `<title>Azul Web App</title>` (:213) and `<html lang="en">` (:209).
  `NodeType::Head`/`Title`/`Body` are mapped in `node_type_to_html_tag` (:640) but never populate
  the real `<head>` (body walk wraps everything in `<div id="az-body">`). **Action:** thread a
  title+lang through `render_html` and honor `NodeType::Head`/`Title` if present in the DOM.

- [ ] **html_render.rs:640 — incomplete head emission** 🟡 — tie-in with item above; `<head>` is
  never populated from the DOM. **Action:** complete the head/title walk.

- [x] ~~html_render.rs — disallow `_`~~ → **SKIP / low value** 🟢 — ~17 `_` usages are all legitimate
  match wildcards / `Err(_)` over a ~80-variant `NodeType`; no `let _ =` discards. A blanket ban
  would force full enumeration with little benefit. **Action:** none (revisit only if NodeType
  exhaustiveness is independently wanted).

- [ ] **EVENT_PATCH_SCHEMA — track deferred wiring** 🟡 — `dll/src/web/EVENT_PATCH_SCHEMA.md:149-167`
  lists still-unwired (intentional): real CallbackInfo wasm-side (cbs calling `*_change_*` no-op on
  web), MoveNode/ReplaceSubtree decoder, AddTimer/RemoveTimer, AddImageToCache/OpenMenu/ShowTooltip/
  SetCopyContent/SetCutContent, AddThread/RemoveThread. **Action:** keep as a living TODO; prioritize
  real CallbackInfo + timers when web sprint resumes.

- [ ] **Web server — replace blocking loop with micro tokio runtime** 🔴 —
  `dll/src/web/server.rs:98` `for stream in listener.incoming()` + thread-per-connection
  `std::thread::spawn` (:103, `handle_connection` :119), hand-rolled HTTP parsing. **Action:** small
  tokio runtime + HTTP parsing crate (httparse/hyper); add a "raw request → mock request" conversion
  path. Fixes the unbounded thread model too.

- [ ] **Web server state — sync via RefAny hook, not server-held mutex** 🔴 — `server.rs:46`
  `WebServerState { app_data: Arc<Mutex<RefAny>>, window_state: FullWindowState }` (mirrored in
  `headless.rs:38`). Client/server sync should be a mutation-fired sync-fn on RefAny (see core RefAny
  item), not a server mutex + manual `re_render_body`. **Action:** depends on the RefAny on-update
  hook; then rework server state to subscribe.

- [x] ~~boundary_wasms~~ → **KEEP** 🟢 — `server.rs:73` `Vec<BoundaryWasm>` (type `web/mod.rs:181`,
  built `:752-794`, served `/az/fn/`). Functional M10-D sharding; empty in legacy mode. No action.

- [x] ~~auth_check — difficult on web~~ → **KEEP** 🟢 — `server.rs:524`: bearer-token strip +
  `constant_time_eq`, tested. Fine as-is. No action.

- [x] **Shard manifest — use AzJson + pretty-print** 🟢 — **DONE:** rewrote `build_manifest` with
  `azul_core::json::{Json, JsonKeyValue, JsonKeyValueVec, JsonVec}` + `to_string_pretty()` via small
  `obj`/`arr`/`kv` builders; deleted the now-dead hand-rolled `json_escape` (the AzJson serializer
  handles escaping + number formatting). Safe because `run_web` already uses `azul_layout::json`
  unconditionally, and layout's `json` feature pulls `azul-core/serde-json` — exactly what gates
  `Json::object`/`to_string_pretty`.

- [x] **run_tool / wasm-ld — static-link question answered** 🟢/INVESTIGATE — **DONE (decision):**
  subprocess is the **intentional default** — the in-process/static path (`web-transpiler-static` +
  `AZ_NATIVE_REMILL=1`) is slow and currently miscompiles the full library (per memory + use_native_remill
  docs). `run_tool` is already well-documented (incl. the Windows transient-failure retry). No change;
  not worth investing in making in-process the default.

### dll/ (non-web)

- [x] **Xargo.toml — verify still needed** 🟢 — **DONE (confirmed needed, keep):** `dllgen` asserts
  its presence (`doc/src/dllgen/deploy.rs:632`, `doc/src/dllgen/build.rs:132`) and copies it into the
  generated DLL build dir; no `-Z build-std` migration exists. Not vestigial. No change.

- [ ] **desktop/extra/udp/ — remove for WebTransport** 🟡 — `dll/src/desktop/extra/udp/mod.rs`
  (~8 KB): C-ABI `Udp` handle over `std::net::UdpSocket` (`send_to`/`recv`/`send_chunked`/
  `recv_chunked`), depends on `core::udp_framing`. **Action:** remove with WebTransport migration —
  also clean api.json entries + callers + the core `udp_framing.rs`.

- [ ] **video_codec — H.265 path incomplete / possible dup** 🟡 — `desktop/extra/video_codec/mod.rs:391`
  `VideoDecoder::open` selects H.264/H.265; "H.265 decode isn't wired into the bytes-decoder path
  yet." Backend logic spread across `decode_vulkan.rs`/`demux.rs`/`pipeline.rs`/`stream.rs`.
  **Action:** review `open`/`backend()` vs vulkan/pipeline decoders for overlap; finish or remove the
  H.265 path.

- [x] **gnome_menu/README.md — trim stale plan** 🟢 — **DONE:** dropped the "Implementation Status"
  checklist (stale unchecked integration items), the dated "Week 2 Implementation Summary
  (COMPLETED, Oct 30 2025)", and the "Week 2 Implementation Plan" (Day 1-7). Kept the feature-flag/
  overview/module-structure/public-API/env-var/GTK-DBus-protocol reference + usage + architecture
  diagram + design principles. 509 → 352 lines.

- [ ] **dll clippy — scope the allows** 🟡 — `dll/src/lib.rs:30-53` blanket-allows
  `unused_imports, unused_variables, dead_code, unused_mut, non_snake_case, deprecated,
  unexpected_cfgs, static_mut_refs`. **Action:** move these to `#[allow]` on the generated/FFI/
  platform-gated modules rather than crate-global; also the `static_mut_refs` TODO → migrate to
  `OnceLock`.

- [x] **brotli/zlib — expose compression in api.json?** 🟢 — **DONE (decision, leave internal):**
  confirmed `brotli_decompressor::BrotliDecompress` is internal-only (web/classify.rs,
  desktop/material_icons.rs, debug_server/full.rs) — decode-only, for embedded compressed assets. No
  user-facing-compression demand signal; not adding an `AzBrotli`/`AzDeflate` handle. No change.

- [ ] **App config — add `source_language` field** 🟡 — `core/src/resources.rs:454 AppConfig` has no
  source-language field; `App` + `App::run` at `dll/src/desktop/app.rs:25/:128`. **Action:** add
  `source_language: AppSourceLanguage` to `AppConfig` so the web backend can auto-ship the matching
  runtime (java.wasm etc.). Consumed in `App::run`/`App::create`.

---

## Cross-cutting

- [ ] **AzJson — serde-parity differential tests** 🟡 — types in `core/src/json.rs` (689); logic in
  `layout/src/json.rs` (`json_parse`/`json_stringify`/`serialize_refany_to_json`). 8 basic tests at
  `:156`, but no round-trip-vs-serde_json differential test. Pretty-print already exposed
  (`to_string_pretty`, api.json:90187). **Action:** add explicit round-trip + serde-parity tests.

- [ ] **Swappable `<icon>` for menus/buttons** 🟡 — a swappable icon system already exists:
  `core/src/icon.rs` (673 lines, `IconProviderHandle`, `Dom::create_icon("home")`). But
  `MenuItemIcon` (`core/src/menu.rs:258`) only has `Checkbox`/`Image(ImageRef)`, and Button
  (`widgets/button.rs:77`) uses raw `OptionImageRef`. **Action:** migrate menu + button icons to
  reference the icon-provider system instead of raw `ImageRef`.

- [ ] **Undo/redo system** 🔴 — only text-edit undo exists (`SystemChange::Undo/RedoTextEdit`,
  `events.rs:2511`); no general application-state undo. **Action:** build a generic undo stack on top
  of the RefAny deep-clone/snapshot callback (depends on the RefAny on-update/deep-copy item).

- [x] ~~File API — home dirs / C test~~ → **GOOD** 🟢 — `azul_layout::file::FilePath` exposes
  `get_home_dir`/`get_temp_dir`/`get_cache_dir`/etc. (api.json:59789); `FileDialog`
  open/save/directory; real C test `examples/c/file.c` (424 lines). No action (FileDialog
  interactive path untested, expected).

- [x] **FluentZipLoadResult — typed error enum** 🟢 — **DONE (source):** added a `#[repr(C, u8)]`
  `FluentLoadError` enum (OpenArchive / ReadEntry / UnknownLocale / ReadFile / Parse / InvalidUtf8 /
  UnknownExtension, each carrying the detail `AzString`) + FFI `impl_option!`/`impl_vec!` scaffolding,
  changed `FluentZipLoadResult.errors` to `FluentLoadErrorVec`, converted all 8 construction sites,
  and re-exported the new types from the layout lib. **⚠ api.json sync pending:** run
  `azul-doc autofix` at the final build step to add `FluentLoadError`/`FluentLoadErrorVec`(+Destructor/
  Option) and update the `errors` field type (do NOT hand-curate — use the tool).

- [x] **Dockerfile — trim docs** 🟢 — **DONE:** condensed the 44-line header to a ~12-line summary
  (full design/extend/caveats already in `docker/README.md`) and trimmed the verbose per-stage
  comment blocks to one/two-liners. All 53 directives and 4 stages untouched (comment-only edits).
  210 → 161 lines; comment lines 108 → 59.

- [ ] **URL — thread the typed `Url` through consumers** 🟡 (reclassified from 🟢) — **DEFERRED w/
  analysis.** Blocked as a quick win: the plan's own example `VideoSource::Url(AzString)` is in
  **core** (`core/src/video.rs:24`) and the `Url` type + its `url`-crate parsing live in **layout**
  (`layout/src/url.rs`), so core consumers (video.rs, the many `url: AzString` in `core/src/xml.rs`)
  can't reference `Url` without a core→layout cycle. Correct path (medium): move the POD `Url`/
  `UrlParseError`/`ResultUrlUrlParseError` to `core/src/url.rs` and put the `url`-crate parsing in core
  behind a feature (exactly the Json core+`serde-json` pattern), re-export from layout, then migrate
  `VideoSource`/xml; update api.json `external` paths via autofix. NOTE: `widgets/map.rs` `url` is a
  `{x}/{y}/{z}` tile **template**, not a parseable URL — leave it `AzString`. Font-source `Url(String)`
  variants are internal pass-through — low value. Do in the medium phase with the other FFI moves.

- [ ] **HashMap → BTreeMap** 🔴 — ~322 occurrences in src (core 20, **layout 188**, dll 114; dll
  concentrated in `web/symbol_table.rs` 38, `web/transpiler_remill.rs` 9). **Action:** prioritize
  codegen/lift maps in `web/` where deterministic iteration affects output reproducibility; do the
  rest mechanically. Bulk is in layout, not dll.

- [ ] **Clipboard rich text / images** 🔴 — `ClipboardEventData` (`core/src/events.rs:410`) is
  `content: Option<String>` — plain text only. **Action:** replace with a typed clipboard-content
  enum (text/html/image) and wire through Copy/Cut/Paste.

---

## Suggested ordering

1. **Quick wins (🟢):** events.rs tests split, headless.rs diagram trim, eventloop.rs comment strip,
   gnome README trim, Dockerfile trim, changeset.rs comment, extra.rs fold, shard-manifest AzJson,
   paged.rs → core, FluentZipLoadResult enum.
2. **Lint tightening (🟡):** layout + dll clippy de-liberalization (then AI fixes errors); remove
   `az_mark` markers.
3. **Mechanical refactors (🟡):** prop_cache accessor macro, gpu `synchronize` split,
   `into_loaded_image_source` split, hit_test merge, menu/button `<icon>` migration, html_render
   head/title.
4. **Foundational (🔴):** RefAny on-update/deep-copy hook → unblocks web state sync + undo/redo.
5. **Big architectural (🔴):** CPU hit-test transforms (+ WebRender decouple), SVG DOM unification,
   cpurender.rs split, web server tokio rewrite, HashMap→BTreeMap sweep, rich clipboard, WebTransport
   migration (removes udp + udp_framing).
