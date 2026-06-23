# High-level items 1–5 — work plan (branch `feat/highlevel-items-1-5`)

**Branch base:** `18a1f85ef` (the clippy `--fix` wave; the `b3caeb5b8` extreme-lints
"wall" is intentionally NOT on this branch — it lives on `master` as the Monday
lint-report marker). This branch is the clean/green base for feature work; other
agents work on sibling branches and we merge later.

**Source of truth for the 5 items:** the HIGHLEVEL_SUPERPLAN audit (2026-06-20).
These are the 5 genuinely-REMAINING functionality gaps (the other 35/40 are DONE
or documented-deferrals). Churn through them; **update the Status column + check
boxes as you go** so this survives compaction. Each item is independent (different
subsystems) — safe to do in any order / parallel worktrees. Commit per item with a
clear message; verify before marking done.

| # | Item | Subsystem | Status |
|---|------|-----------|--------|
| 1 | macOS file-drop end-to-end | dll macOS shell + file_drop mgr | DONE |
| 2 | display_list pagination text no-op | layout solver3 display_list | DONE |
| 3 | cpurender backdrop-filter + text-shadow | layout cpurender | DONE (both) |
| 4 | Wayland tooltip text shaping | dll wayland shell | TODO |
| 5 | shape-outside path() + ruby shaping | layout text3 | TODO |

---

## Item 1 — macOS file-drop end-to-end  (BIGGEST gap; 3 linked sub-tasks)
Files: `dll/src/desktop/shell2/macos/events.rs`, `dll/src/desktop/shell2/macos/mod.rs`,
`layout/src/managers/file_drop.rs`, `layout/src/event_determination.rs:~641`.
Gap (audit): file-drag-hover is entirely non-functional on macOS. Windows also lacks
modern `IDropTarget` hover (it uses legacy `WM_DROPFILES` drop-only).
- [x] `handle_file_drop` (macos/events.rs:~827) now has a caller (`performDragOperation:`).
- [x] Added `NSDraggingDestination` methods to BOTH macOS views (GLView + CPUView) in mod.rs
      (`draggingEntered`/`draggingUpdated`/`draggingExited`/`performDragOperation`),
      registering `NSFilenamesPboardType` via `registerForDraggedTypes:` at view creation.
      Shared `view_handlers::{dragging_entered,dragging_exited,perform_drag}` + pasteboard
      path extraction, mirroring the Windows `WM_DROPFILES` `file_drop_manager` flow.
- [x] `FileDropManager::set_hovered_file` driven from `draggingEntered`/`draggingUpdated`
      (Some) and `draggingExited` (None) so `FileHover`/`FileHoverCancel`
      (event_determination.rs:641) fire. (Windows OLE `IDropTarget` hover NOT added —
      out of scope for the macOS item; Windows keeps its legacy drop-only path.)
- [ ] (cleanup) macOS-only lossy `EventProcessResult` vs core `ProcessEventResult` — SKIPPED
      (would widen scope; result routing matches the existing mouse/key handlers exactly).
- Verify: `cargo check -p azul-dll --features build-dll` PASSES (host=macOS).
  Manual drag-file-onto-window still needs a GUI session (can't run headless).
- Side fix (verification blocker): `layout/src/managers/a11y.rs` `impl A11yManager` was
  missing `#[cfg(feature = "a11y")]` (the struct + a `not(a11y)` stub impl were gated but
  the real impl wasn't) → any no-a11y build (azul-doc codegen) failed with 187 errors.
  Added the gate; behavior-preserving (a11y builds unchanged).

## Item 2 — display_list pagination text no-op  (REGRESSION-ish)
File: `layout/src/solver3/display_list.rs` (text path ~6027; `generate_text_display_items`).
Gap (audit): the codepoint-as-glyph stub became a **no-op** — `generate_text_display_items`
returns nothing, so paginated header/footer text renders NOTHING (worse than the old
garbage-glyph stub). Needs real font/`renderer_resources` threading to shape + emit glyphs.
- [x] Thread font resources into the pagination text path; shape the string and emit
      real glyph display items (mirror the main text display-item path).
      `generate_text_display_items` now takes `&RendererResources`, picks the first
      registered font, resolves its `ParsedFont` (`font_ref_to_parsed_font`), shapes
      per-char (cmap `lookup_glyph_index` + `get_horizontal_advance`), centers the run
      horizontally + vertically (ascent/descent metrics), and emits a real
      `DisplayListItem::Text` with the font's registered hash. Threaded
      `renderer_resources` through `paginate_display_list_with_slicer_and_breaks`
      (sole caller in `paged_layout.rs` already had it in scope).
- [x] Confirm header/footer text actually renders: added `pagination_text_tests` unit
      tests in `display_list.rs` — `generate_text_display_items_emits_glyphs` asserts a
      non-empty Text item with real GIDs (not codepoints), the registered (non-zero)
      font hash, and a strictly-advancing pen; `_empty_without_font` asserts the
      no-font path stays empty. Both pass (cargo test -p azul-layout --lib = 125 ok).
- Verify: `cargo check -p azul-layout` + `cargo test -p azul-layout --lib` PASS.
  Note: simple per-char shaper (no kerning/bidi/complex shaping) is sufficient for
  short single-line running headers/footers; the full pipeline isn't reachable here
  because the call site carries no styled run.

## Item 3 — cpurender backdrop-filter + text-shadow  (both no-ops)
Files: `layout/src/cpurender/compositor.rs` (~backdrop-filter), `layout/src/cpurender/raster.rs` (~text-shadow).
Note: cpurender is now a directory (post-split). `filter` is already wired; these two are
still complete no-ops in the compositor layer path (allocate_layers + composite_frame).
- [x] Implemented `text-shadow` (offset+blur+color behind glyphs) in the raster path.
      Threaded a `text_shadow_stack: &mut Vec<StyleBoxShadow>` through `render_single_item`
      (Push/PopTextShadow now maintain it; was a no-op). In the `Text` arm, each active
      shadow is painted back-to-front by new `render_text_shadow`: rasterizes the glyph
      run (via the existing `render_text`) offset by the shadow offset into a transparent
      offscreen, blurs it with the SAME `stack_blur_rgba32` used by box-shadow/filter,
      then alpha-composites (existing `blit_buffer`) below the real glyphs. Multiple
      stacked shadows supported. All 4 `render_single_item` call sites updated (two
      top-level loops seed a fresh stack; the VirtualView recursion + compositor pass it
      through).
- [x] Implemented `backdrop-filter` in the compositor path. `allocate_layers_from_display_list`
      now allocates a layer for `PushBackdropFilter` (mirroring `PushFilter`) tagged
      `is_backdrop_filter`; `MatchKind::BackdropFilter` + `find_matching_pop` arm added.
      In `composite_layer_recursive`, a backdrop-filter layer snapshots the already-
      composited `output` region under its bounds (`snapshot_region`), runs the existing
      `apply_layer_filters` on it, writes it back (new `write_region` direct-copy helper),
      THEN blits the layer's own (unfiltered) content over it — exactly the design the old
      TODO described. Bottom-up compositing means the backdrop is already in `output`, so
      no restructuring was needed. Bug fixed along the way: an empty backdrop-filter
      element (empty display-list range) kept the Layer::new opaque-white pixbuf (render
      skips empty ranges) which would wipe the filtered backdrop — backdrop-filter pixbufs
      are now cleared transparent at allocation.
- Verify: `cargo check -p azul-layout` clean; `cargo test -p azul-layout --lib` = 128 ok.
  Three new proof-tests (render to in-memory `AzulPixmap`, assert real pixels):
  `text_shadow_paints_offset_colored_pixels` (red shadow offset +24px appears where the
  no-shadow render has zero red pixels), `text_shadow_blur_spreads_coverage` (blurred
  shadow covers strictly more pixels than hard-edged), and
  `backdrop_filter_inverts_backdrop_region` (blue backdrop under the element inverts to
  yellow; pixels outside the element box stay blue). The glyph tests skip gracefully if no
  system font is found (still real-assert when one is, e.g. CI/macOS/Linux).

## Item 4 — Wayland tooltip text shaping
File: `dll/src/desktop/shell2/linux/wayland/tooltip.rs:~279` (`render_tooltip_content`).
Gap (audit): draws black-bar placeholders instead of shaped text; signatures not aligned
with the other backends.
- [ ] Wire `render_tooltip_content` into Azul's text-shaping pipeline (same as X11/macOS/Windows).
- [ ] Align `show`/`hide` signatures (`DpiScaleFactor` + `Result`) with the other backends.
- Verify: `cargo check -p azul-dll --features build-dll --target x86_64-unknown-linux-gnu`
  (wayland is Linux-only; can't run on macOS host — compile-check + CI).

## Item 5 — shape-outside path() + ruby shaping  (CSS/text3 stubs)
File: `layout/src/text3/cache.rs` (shape-outside ~3059/9831; ruby ~7073 magic 0.6, ~1576).
- [ ] Implement CSS `shape-outside: path()` (currently falls back to rect/empty).
- [ ] Real ruby shaping (replace the 0.6 magic-ratio stub).
- Verify: `cargo test -p azul-layout`; add text-layout tests.

---

## Conventions
- Keep changes scoped to the named files per item (low cross-item conflict).
- Conservative on rendering (items 2/3/4/5 touch render) — prefer reftest verification;
  don't ship plausible-but-unverified visual output. If an item is too risky to finish,
  leave a `TODO2:` with the reason and mark PARTIAL here.
- Commit per item; do NOT touch the lint policy (that's the separate master-side Monday work).
- ENOSPC watch: `rm -rf target` if the disk fills (~38G recurring).
