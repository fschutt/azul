# Azul Cleanup Plan

Investigation-backed checklist from the 2026-06-20 code-review pass. Each item carries the
verified current state (file:line) and a recommended action. Categories:
**REMOVE** / **REFACTOR** / **BUILD-OUT** / **KEEP** (no action, finding corrected) / **INVESTIGATE**.

Effort: 🟢 small · 🟡 medium · 🔴 large.

---

## Execution status (2026-06-20, this pass)

**All 🟢 small items done** (except `events.rs` test-split, which the user is doing by hand) and
**most 🟡 medium done** (animation fold, paged→core, extra fold, headless trim, eventloop M11,
shard-manifest AzJson, gnome README, Dockerfile, FluentLoadError enum, URL→core wiring, gpu
`synchronize` split, hit_test merge, **prop_cache accessor macro −1831 lines**, html_render head/title).
Reasoned KEEP/DEFER decisions recorded inline for: window_state, udp_framing, run_tool, Xargo,
brotli, az_mark (already gated), EVENT_PATCH_SCHEMA, desktop/extra/udp, video_codec H.265,
source_language, icon migration.

**Architecture cluster DONE this pass (all compile-validated):** the **RefAny on-update hook**
(`update_fn` on `RefCountInner`, fired on `downcast_mut`), the generic **undo/redo** manager
(`RefAnyUndoManager`, JSON-snapshot based; +bugfix: `restore` re-attaches live (de)serialize hooks
across `replace_contents`), **AzJson serde-parity tests** (+fixed 9 pre-existing E0308 assertions),
and the **web server bounded worker pool** (no tokio — `mpsc`+`Mutex<Receiver>`, zero new deps). Also
fixed a rebase regression: ungated `symbol_table` refs (web-transpiler-only) broke `--features web`.
Validated: `cargo check` on core(+url), layout(+fluent,+json), dll(web, web-transpiler) + `cargo test
--lib json::tests` (10/10).

**Still remaining — follow-ups (concrete approach inline / below):**
- **clippy de-liberalization** (layout + dll) — remove the blanket `allow`s then fix surfaced errors
  incrementally with the compiler open.
- **HashMap→BTreeMap** (322 sites) — mechanical; needs compile for `Ord` bounds + unused imports;
  web/ codegen maps first for reproducible lift output (don't blind-replace the `std::HashMap` prose
  in those files' comments).
- **cpurender.rs split** — module reorg into `cpurender/{compositor,raster,svg,pixmap}.rs`; do with
  the compiler to fix cross-module visibility.
- **web-server state dirty-sync** — FOUNDATION DONE (the hook); register a dirty-marking `update_fn`
  on `app_data` to skip redundant `re_render_body`. The `Arc<Mutex<>>` stays (thread-safety w/ the
  pool). Contained follow-up.
- **SVG DOM-path unification** & **CPU hit-test CSS transforms** — genuine rendering / transform-math
  features; higher-risk, best as focused sessions (see items below for the exact approach).
- **rich clipboard (typed content enum)** — lives in `core/events.rs`, which the user is editing by
  hand (test split) — coordinate to avoid conflicts.
- **ICU parity tests** — awkward: the three backends are mutually-exclusive features, so a single-build
  parity test isn't possible; a per-backend reference test is the realistic shape.

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

- [x] **gpu.rs — split `synchronize`** 🟡 — **DONE:** `synchronize` is now a ~15-line orchestrator
  calling `init_simd_features()` + read-only `compute_transform_events`/`compute_opacity_events`
  (diff vs. cache, return `Vec<…Event>`) + `apply_transform_events`/`apply_opacity_events` (mutate
  cache). Closure bodies untouched; `&self` compute / `&mut self` apply split avoids borrow
  conflicts.

- [x] **Merge hit_test.rs + hit_test_tag.rs** 🟡 — **DONE:** merged `hit_test_tag.rs` (TAG_TYPE_*
  consts, `ScrollbarComponent`, `HitTestTag`, `CursorType`) into `core/src/hit_test.rs` (~848 lines),
  dropped `pub mod hit_test_tag`, and updated all 6 importers (`layout/hit_test.rs`,
  `solver3/display_list.rs`, dll `wr_translate2.rs` ×2, `compositor2.rs`) from `hit_test_tag::` →
  `hit_test::` (merging the duplicate `hit_test::` import groups). Not FFI-exported (0 api.json refs).
- [ ] **⚠ CPU hit tester ignores CSS transforms** 🔴 (correctness, not cleanup) — zero `transform`
  matches in either file. **Action:** account for CSS transforms in hit testing. Tracks with the
  "use CPU hit tester always" decoupling-from-WebRender goal.

- [x] **changeset.rs trailing comment** 🟢 — **DONE:** removed the trailing comment in
  `layout/src/managers/changeset.rs`; the module-level docs (lines 8-11) already cover that the
  `create_*_changeset` helpers were removed and that the payload types are retained/used by
  `window.rs`/`undo_redo.rs`.

- [x] **CssPropertyCache — macro-generate accessors** 🟡 — **DONE:** added an `impl_get_prop!(name,
  ValueType, Variant, as_method)` macro and collapsed **170** mechanical accessors to one-liners
  (more than the estimated ~92 matched the exact shape). `core/src/prop_cache.rs` shrank 5482 → 3651
  lines (-1831). Left untouched: `get_property`/`get_property_slow`/`get_property_with_context`
  (resolvers), the `*_or_default` accessors, `pub(crate) get_grid_gap`, and the 5 `get_scrollbar_*`
  fns (substantive docs). Spot-checked tuples byte-match the originals.

- [x] **RawImage::into_loaded_image_source — split** 🟡 — **DONE:** the ~420-line `match data_format`
  is now a thin dispatch table; each of the 12 format arms is a private `load_<fmt>(pixels,
  expected_len, [premultiplied_alpha]) -> Option<(U8Vec, bool /*is_opaque*/)>` helper, with
  `premultiply_alpha`/`normalize_u16` + the BPP/channel consts hoisted to module scope. Conversion
  math/bounds/premultiply/is_opaque preserved byte-for-byte; the match site sets `data_format`
  (BGRA8 for converted formats, R8 stays R8). Public signature unchanged.

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

- [x] **RefAny — add optional on-update / sync callback** 🔴 — **DONE:** added `update_fn: usize`
  to `RefCountInner` (mirrors the internal `serialize_fn`/`deserialize_fn` pattern — not a new FFI
  ctor param, init 0), fired from `downcast_mut` BEFORE the mutable borrow as
  `extern "C" fn(*const c_void, usize)` (pre-mutation data ptr + byte len), copied across
  `replace_contents`, with `set_update_fn`/`get_update_fn` (Rust-side; not FFI-exported yet since the
  consumers are Rust). Validated: `cargo check -p azul-core` clean.

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

- [x] **`az_mark` / `az_mark_read` markers — remove/gate** 🟡 — **DONE (already gated, keep):**
  verified `az_mark`/`az_mark_read` are `#[inline(always)]` fns whose *bodies* are
  `#[cfg(feature = "web_lift")]` — so the ~120 call sites compile to **nothing** without `web_lift`
  (off by default), i.e. they are already feature-gated and zero-cost. The plan's "or feature-gate
  them" is satisfied by design. Deliberately NOT removing the 120 hot-path call sites: zero runtime
  cost, blind removal across the solver path is high-risk, and they still aid the in-progress
  web-lift mis-lift hunts (memory: g147). (The 198-`unsafe` audit is a separate, larger task — left
  as a follow-up.)

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

- [x] **html_render.rs — head/title support** 🟡 — **DONE:** added `extract_head_meta(styled_dom)`
  which scans the arena for the `<title>` text (a `NodeType::Title` node's first text child) and the
  `lang` attribute on the root `NodeType::Html` node; `render_initial_page` now threads these into the
  template (`<html lang="{…}">` / `<title>{…}</title>`) via captured-ident format args, defaulting to
  `en` / "Azul Web App". Escaped via `html_escape`/`html_escape_attr`.

- [x] **html_render.rs:640 — incomplete head emission** 🟡 — **DONE (with the item above):** the
  body walk (`render_node_recursive`) now skips `NodeType::Head`/`Title` subtrees entirely (they
  belong in the real `<head>`, populated from `extract_head_meta`), so head content no longer leaks
  into `<div id="az-body">`.

- [x] ~~html_render.rs — disallow `_`~~ → **SKIP / low value** 🟢 — ~17 `_` usages are all legitimate
  match wildcards / `Err(_)` over a ~80-variant `NodeType`; no `let _ =` discards. A blanket ban
  would force full enumeration with little benefit. **Action:** none (revisit only if NodeType
  exhaustiveness is independently wanted).

- [x] **EVENT_PATCH_SCHEMA — track deferred wiring** 🟡 — **DONE (reviewed, keep):** the
  `## What's NOT yet wired (intentional)` section is already an accurate, well-maintained living TODO
  (real CallbackInfo wasm-side, MoveNode/ReplaceSubtree decoder, AddTimer/RemoveTimer, image-cache/
  menu/tooltip/clipboard, threads). No change — it's serving its purpose; the listed items are web
  sprint work, not cleanup.

- [x] **Web server — replace blocking loop with micro tokio runtime** 🔴 — **DONE (conservative,
  no tokio):** replaced the unbounded thread-per-connection spawn with a **bounded worker pool**
  (`std::sync::mpsc` channel + `Arc<Mutex<Receiver>>`, `2×available_parallelism` clamped 4..64) — the
  lock is held only across `recv()`, so request handling stays concurrent. This fixes the unbounded
  thread model (DoS-resistant) with **zero new dependencies** and no async runtime (per the request
  to avoid tokio's dep weight). Kept the hand-rolled HTTP parsing rather than pull in httparse/hyper.

- [ ] **Web server state — sync via RefAny hook, not server-held mutex** 🔴 — **FOUNDATION DONE,
  rework deferred.** The RefAny on-update hook (`set_update_fn`, fired on `downcast_mut`) now exists —
  the server can register a dirty-marking `update_fn` on `app_data` to skip redundant `re_render_body`
  passes. NOTE: the `Arc<Mutex<RefAny>>` itself must STAY (thread-safe concurrent access — now more so
  with the bounded worker pool); the hook adds dirty-tracking, it doesn't remove the mutex. The
  remaining server-subscription wiring (thread the dirty flag through the request loop) is a contained
  follow-up on the now-available hook.

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

- [x] **desktop/extra/udp/ — remove for WebTransport** 🟡 — **DONE (reviewed, defer):** the `Udp`
  handle is FFI-exported (present in api.json) and explicitly conditioned on the WebTransport/AzMeet
  migration, which is NOT part of this cleanup. Removing it now would break the live FFI surface.
  Defer; remove together with `core::udp_framing` + api.json entries when WebTransport lands (pairs
  with the udp_framing.rs item above).

- [x] **video_codec — H.265 path incomplete / possible dup** 🟡 — **DONE (reviewed):** no
  duplication — `VideoDecoder::open` layers cleanly on `decode_vulkan::VulkanVideoDecoder::open_h264`.
  The H.265 **decode** path is an intentional, clearly-documented stub (`backend: None` with a "not
  wired into the bytes-decoder path yet; demos are H.264" comment), while the **encoder** + the
  `open(h265)` API do support H.265. Finishing decode = implementing a Vulkan-Video H.265 decoder +
  test content — that's feature work, out of cleanup scope; removing it would drop encoder H.265
  support. Left as the documented stub. (Tracked as a feature follow-up, not cleanup.)

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

- [ ] **App config — add `source_language` field** 🟡 — **DEFERRED w/ reasoning.** The consumer (web
  backend auto-shipping `java.wasm` etc.) does **not exist yet**, so the `AppSourceLanguage` enum's
  variant set + semantics are unconstrained guesses that would churn once that feature is built.
  `AppConfig` is FFI-exported with multiple construction sites (api.json autofix + Default + create).
  Adding unused speculative FFI surface now is an anti-pattern — better to co-design the enum WITH the
  web-runtime-shipping feature. Add `source_language: AppSourceLanguage` to `AppConfig` then.

---

## Cross-cutting

- [x] **AzJson — serde-parity differential tests** 🟡 — **DONE:** added `test_roundtrip_serde_parity`
  (nested value → `to_string_pretty` → re-parse → equal) and **fixed 9 pre-existing broken assertions**
  in the existing parse tests that compared azul `OptionBool`/`OptionF64`/`OptionI64`/`OptionString`
  to std `Option` (E0308 — they never compiled under `cargo test`; added `.into_option()`). All 10
  `json::tests` pass under `cargo test -p azul-layout --features json --lib`.

- [ ] **Swappable `<icon>` for menus/buttons** 🟡 — **DEFERRED w/ reasoning + concrete plan.** The
  Button side is tractable (add `OptionAzString icon_name`; when set, render a `Dom::create_icon(name)`
  node — reuses the existing `resolve_icons_in_styled_dom` path). The **menu** side is the blocker:
  desktop menus render via *platform-native* menus that need a concrete `ImageRef` bitmap, so a
  `MenuItemIcon::Named(AzString)` must be resolved through the `SharedIconProvider` at menu-build time
  — which means threading the provider into native-menu construction (dll shell2). That's an FFI +
  cross-platform render change that's risky to do blind. Both are FFI (api.json autofix). Deferred to
  avoid a half-migrated icon API on a working feature; do Button + menu together once the provider can
  be threaded into menu building.

- [x] **Undo/redo system** 🔴 — **DONE (core building block):** added `RefAnyUndoManager` in
  `layout/src/json.rs` — a generic application-state undo/redo stack that snapshots the whole app
  `RefAny` via its serialize fn (JSON) and restores via deserialize + `replace_contents`
  (`snapshot`/`undo`/`redo`/`can_undo`/`can_redo`/`clear`, bounded depth). Drive it manually at
  action boundaries or from the new RefAny `update_fn` hook. **Fixed a real bug found while testing:**
  `replace_contents` copies the *new* value's (de)serialize fns, so `restore` now re-attaches the
  live serialize/deserialize/update hooks across the swap. Validated by `test_undo_manager_roundtrip`.
  (Wiring it into the desktop event loop's command palette is a separate app-level step.)

- [x] ~~File API — home dirs / C test~~ → **GOOD** 🟢 — `azul_layout::file::FilePath` exposes
  `get_home_dir`/`get_temp_dir`/`get_cache_dir`/etc. (api.json:59789); `FileDialog`
  open/save/directory; real C test `examples/c/file.c` (424 lines). No action (FileDialog
  interactive path untested, expected).

- [x] **FluentZipLoadResult — typed error enum** 🟢 — **DONE (source):** added a `#[repr(C, u8)]`
  `FluentLoadError` enum (OpenArchive / ReadEntry / UnknownLocale / ReadFile / Parse / InvalidUtf8 /
  UnknownExtension, each carrying the detail `AzString`) + FFI `impl_option!`/`impl_vec!` scaffolding,
  changed `FluentZipLoadResult.errors` to `FluentLoadErrorVec`, converted all 8 construction sites,
  and re-exported the new types from the layout lib. **api.json synced:** `azul-doc autofix add
  FluentLoadErrorVec.create` generated + applied patches adding `FluentLoadError` (error module) +
  `FluentLoadErrorVec`/`...Destructor`/`...DestructorType` (vec module); the `errors` field is now
  `FluentLoadErrorVec`. `codegen all` regenerated all 35 language bindings cleanly.

- [x] **Dockerfile — trim docs** 🟢 — **DONE:** condensed the 44-line header to a ~12-line summary
  (full design/extend/caveats already in `docker/README.md`) and trimmed the verbose per-stage
  comment blocks to one/two-liners. All 53 directives and 4 stages untouched (comment-only edits).
  210 → 161 lines; comment lines 108 → 59.

- [x] **URL — thread the typed `Url` through consumers** 🟡 (was 🟢) — **DONE.** Moved the POD
  `Url`/`UrlParseError`/`ResultUrlUrlParseError` from `layout/src/url.rs` to `core/src/url.rs`
  (deriving `Default`), with the `url`-crate parsing (`parse`/`join`) gated behind a new core `url`
  feature (`= ["dep:url", "std"]`) — exactly the Json core+`serde-json` pattern. Layout now re-exports
  `azul_core::url` (so `azul_layout::url::*` keeps resolving) and its `http` feature enables
  `azul-core/url` (dropped layout's own `url` dep). Migrated `VideoSource::Url(AzString)` →
  `Url` (core/video.rs + the azul-video example; the dll consumer reads `u.as_str()`, unchanged).
  api.json: updated the 3 `external` paths + 2 fn_bodies (`azul_layout::url` → `azul_core::url`) and
  the `VideoSource.Url` field (`String` → `Url`); valid JSON re-verified. **Correctly left as
  `AzString`:** `widgets/map.rs` (tile `{x}/{y}/{z}` template, not parseable) and `core/src/xml.rs`
  attr URLs (relative/data-URI). Final build must run `azul-doc codegen all` to regenerate
  `target/codegen/reexports.rs` from the updated api.json.

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
