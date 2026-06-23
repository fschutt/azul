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
| 4 | Wayland tooltip text shaping | dll wayland shell | DONE |
| 5 | shape-outside path() + ruby shaping | layout text3 | DONE (path() full; ruby sizing real, annotation-glyph render PARTIAL/TODO2) |

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
- [x] Wired the tooltip text into Azul's CPU text pipeline. Added a reusable
      helper `azul_layout::cpurender::render_text_run_to_pixmap`
      (`layout/src/cpurender/raster.rs`) that resolves a sans-serif system font
      from the `FcFontCache`, parses it (`ParsedFont::from_bytes`), shapes the
      string (per-char advances — tooltips are short/single-line/unstyled, same
      simplification as the item-2 pagination header), and rasterizes via the
      shared display-list text path (`render_display_list` → `render_text`) into
      an `AzulPixmap`. The wayland tooltip (`linux/wayland/tooltip.rs`) now
      threads in the `FcFontCache` (via `new()`), calls the helper in `show()`,
      and blits the RGBA8 pixmap into the ARGB8888 (BGRA-byte) `wl_shm` buffer
      with a channel swap (`blit_pixmap`). No-font fallback draws a sized,
      background-only box (`render_fallback_background`).
- [x] Aligned `show`/`hide`/`is_visible` with X11/macOS/Windows:
      `show(text, position: LogicalPosition, dpi: DpiScaleFactor) -> Result<…>`,
      `hide() -> Result<…>`, `is_visible() -> bool`, tracking an `is_visible`
      field. Updated the call sites in `linux/wayland/mod.rs` (`show_tooltip`
      now takes a `LogicalPosition` + sources dpi from
      `current_window_state.size.dpi`, mirroring X11; `show_tooltip_from_callback`
      passes the logical position through).
- Verify: `cargo check --target x86_64-unknown-linux-gnu -p azul-dll --features build-dll`
  PASSES (toolchain via target-scoped `CC_x86_64_unknown_linux_gnu` etc. — the
  global `CC` form in the original instructions leaks the linux gcc into host
  build scripts like libz-sys and fails). `cargo check -p azul-layout` PASSES.
  Runtime (showing a tooltip on a live Wayland compositor) still needs a
  Linux+Wayland session — not possible on this macOS host.

## Item 5 — shape-outside path() + ruby shaping  (CSS/text3 stubs)
File: `layout/src/text3/cache.rs` (shape-outside ~3059/9831; ruby ~7073 magic 0.6, ~1576).
- [x] Implement CSS `shape-outside: path()` (was rect/empty fallback). `from_css_shape`'s
      `CssShape::Path` arm now parses `path.data` (SVG `d=""`) via
      `azul_core::path_parser::parse_svg_path_d`, flattens each subpath into
      `Vec<PathSegment>` (curves arc-length-sampled, ~1 seg/4px capped 64) offset to the
      reference box (`flatten_svg_to_path_segments`), and emits `ShapeBoundary::Path`.
      `get_shape_horizontal_spans`'s Path arm now does a real per-line scanline
      intersection (`path_segments_line_intersection`) over all subpaths with an even-odd
      fill rule (so reversed rings = holes carve out space) — mirrors the polygon path.
      Empty/unparseable falls back to the reference rectangle (so shape-inside doesn't
      collapse). Also fixed the float-bottom `Path => f32::MAX` stub to compute the real
      max-y from segments.
- [x] Real ruby shaping (replaced the `0.6` magic-ratio stub = `RUBY_BASE_CHAR_WIDTH_RATIO`,
      now deleted). Base + annotation are SHAPED to get real inline advances
      (`measure_run_advance`, reusing the same font-resolution as the main shaper:
      `FontStack::Ref` direct + `FontStack::Stack` via `shape_with_font_fallback`). The
      annotation is shaped at its used font-size (`RUBY_ANNOTATION_FONT_SCALE` = 50% of
      base, the spec UA value — not a fudge). Reserved box = wider of base/annotation
      inline-size + (base line-height + annotation line-height) block-size so the base
      reserves vertical space above for the annotation (`ruby_reserved_box`). Fallback
      estimate (no font chain) is 1em/char, not 0.6em.
      TODO2 (PARTIAL): the annotation glyphs are sized + reserve space but are NOT yet
      emitted as a separately-positioned centered run — `ShapedItem::Object` carries only
      the base `StyledRun`; rendering the centered annotation needs a ruby-aware
      `ShapedItem` variant (rendering-structural change, deferred to stay layout-safe).
- Verify: `cargo check -p azul-layout` clean; `cargo test -p azul-layout` all green
      (lib 134 ok incl. 6 new in `shape_outside_and_ruby_tests`; all integration + doc
      tests pass). New tests assert REAL non-stub behavior: path() triangle narrows the
      per-line band as y increases (~89.5px@y=10 vs ~19.5px@y=80, and differs from a
      full-width rect), a path-with-hole splits the band into 2 spans (even-odd), garbage
      path falls back to rect, and ruby reserves the max width + stacks annotation height
      (and the scale is not 0.6).

---

## Conventions
- Keep changes scoped to the named files per item (low cross-item conflict).
- Conservative on rendering (items 2/3/4/5 touch render) — prefer reftest verification;
  don't ship plausible-but-unverified visual output. If an item is too risky to finish,
  leave a `TODO2:` with the reason and mark PARTIAL here.
- Commit per item; do NOT touch the lint policy (that's the separate master-side Monday work).
- ENOSPC watch: `rm -rf target` if the disk fills (~38G recurring).

---

## Round 2 — platform integration (research-gated; user request 2026-06-20)
Research agent `a7fad4ed37b731e3e` is discovering the correct OS APIs + mapping them onto
azul's shell2 infra. These items are BLOCKED until that report lands, then TODO. Implement
BLIND (no live runtime test here) — compile-verify per platform + mirror existing patterns
(item 1 macOS `NSDraggingDestination` is the reference for the FileDropManager wiring).
Verify: macOS `build-dll` (host); Win/Linux via cross-compile (`--target x86_64-pc-windows-msvc`
/ `x86_64-unknown-linux-gnu`, **target-scoped** `CC_*`/`CXX_*`/`AR_*`/`CARGO_TARGET_*_LINKER`
env — NOT global, which leaks into host build scripts). FileDropManager hooks:
`set_hovered_file`(Some/None)→FileHover/Cancel, `set_dropped_file`+`handle_file_drop`→FileDrop
(event_determination.rs:641).

| # | Item | Subsystem | Status |
|---|------|-----------|--------|
| 6 | macOS global menu bar + context menu (NSMenu) — missing (azul-paint demo) | dll macOS shell + core Menu API | DONE |
| 7 | Windows file DnD hover+drop (OLE IDropTarget; today legacy WM_DROPFILES drop-only) | dll windows shell | DONE |
| 8 | X11 file DnD (XDND protocol) — none today | dll x11 shell | TODO (research done; see recipe) |
| 9 | Wayland file DnD (wl_data_device) — none today | dll wayland shell | TODO (research done; see recipe) |

## Item 7 — Windows file DnD hover+drop (OLE IDropTarget)  (DONE)
Files: `dll/src/desktop/shell2/windows/dnd.rs` (new), `dll/src/desktop/shell2/windows/mod.rs`,
`dll/src/desktop/shell2/run.rs`, `dll/Cargo.toml`.
Replaced the legacy drop-only `DragAcceptFiles`/`WM_DROPFILES` path with a modern OLE
`IDropTarget` COM object so Windows gets file-drag HOVER (`FileHover`/`FileHoverCancel`) in
addition to drop — mirroring macOS item 1 + the cross-platform `FileDropManager`.
- [x] `mod.rs`: added `handle_file_drag_entered`/`handle_file_drag_exited`/`handle_file_drop`
      (mirror macOS: save-prev-state → `set_hovered_file`/`set_dropped_file` → hit-test at the
      cached cursor (OLE drags deliver no `WM_MOUSEMOVE`) → `process_window_events(0)`), plus
      `register_drag_drop`. Removed the `DragAcceptFiles` call + the entire `WM_DROPFILES` arm;
      `RevokeDragDrop` added to `WM_DESTROY` (before the HWND dies).
- [x] `dnd.rs` (new): `#[implement(IDropTarget)]` COM object via `windows::core`. `DragEnter`/
      `DragOver`→entered, `DragLeave`→exited, `Drop`→drop. Path extraction from
      `IDataObject::GetData` (`FORMATETC{CF_HDROP, DVASPECT_CONTENT, -1, TYMED_HGLOBAL}`) →
      HDROP → `DragQueryFileW` loop → `ReleaseStgMedium`. `*pdweffect = DROPEFFECT_COPY`/`NONE`
      on every call. `OleInitialize` (STA, `Once`) + `RegisterDragDrop`; `.into::<IDropTarget>()`
      (not Boxed — COM owns lifetime via RegisterDragDrop's AddRef). COM resolves the
      `Win32Window` from the HWND via the registry, routes via `route_main_window_result`.
- [x] `run.rs`: `register_drag_drop()` called after the window enters the registry (main + child).
- [x] `Cargo.toml`: windows features `Win32_System_Ole`/`_SystemServices`/`_Com_StructuredStorage`,
      `Win32_Graphics_Gdi`, `Win32_UI_Shell`; added a direct `windows-core` 0.62 dep so the
      `implement` macro's `::windows_core::` paths resolve to 0.62 (cpal pulls 0.54 transitively;
      two versions otherwise clash → `IUnknownImpl not implemented`).
- Verify: `cargo check --target x86_64-pc-windows-msvc -p azul-dll --no-default-features
      --features link-static` PASSES (windows crate is metadata-only). `build-dll` for that target
      only fails on the pre-existing `vk-mem` C++ cross-toolchain gap (msvc C++ headers absent on
      this macOS host), unrelated to this change — `link-static` exercises the full windows shell +
      the `windows` crate. Runtime drag-onto-window still needs a real Windows session.

## Item 6 — macOS global menu bar + context menu  (DONE)
Files: `dll/src/desktop/shell2/macos/menu.rs`, `dll/src/desktop/shell2/macos/mod.rs`.
Investigation: the context-menu path was ALREADY fully wired (`events.rs` `try_show_context_menu`
→ native `popUpMenuPositioningItem:` / window-based popup; tags allocated via `next_tag` +
`merge_callbacks`). The real gap was the **app menu bar**: the window ctor used an empty
`MenuState::new()` (TODO), the launch menu was a hardcoded Quit-only stub, and
`set_application_menu` had ZERO callers — the DOM root's `menu_bar` was never read. azul-paint
DOES define both `with_menu_bar` and `with_context_menu`, so it was a framework gap, not a demo
gap.
- [x] `menu.rs`: added `build_app_submenu` (standard app submenu = app name + Quit→`terminate:`/
      Cmd+Q), `create_menubar_nsmenu` (prepends app submenu, then user items), and
      `MenuState::update_menubar_if_changed` (hash-guarded, allocates tags via the shared
      `next_tag`).
- [x] `mod.rs`: `setup_main_menu` (launch stub) now routes through `menu::build_app_submenu`;
      `set_application_menu` now uses `update_menubar_if_changed`; new `apply_menu_bar_from_dom`
      reads `NodeData::get_menu_bar()` on the root (`DomId::ROOT_ID` / `NodeId::ZERO`) and
      installs it — called after initial layout AND at the end of `regenerate_layout` (hash
      guard makes re-apply cheap).
- [x] Context menu: confirmed working at framework level (no fix needed); azul-paint already
      sets one on the canvas + body.
- Verify: `cargo check -p azul-dll --features build-dll` PASSES (host macOS); `cargo build
  -p azul-paint` PASSES. Visual confirmation (seeing the bar / clicking File→Import) needs a
  live GUI session.

Original notes:
- Item 6 (macOS menu): convert azul `Menu`→`NSMenu` for BOTH `NSApplication.mainMenu` (app
  bar, installed at launch) AND right-click context menu (`rightMouseDown:`→popup); wire
  menu-item clicks back to azul callbacks via the existing objc2 target/ivar bridge. Check
  existing `macos/menu.rs`/menu_state scaffolding + `MenuConversion` (superplan G7).
- Items 7/8/9: file DnD as a drop *target* — declare support, handle enter/position(hover)/
  leave/drop, extract `text/uri-list` (CF_HDROP on Windows), route through FileDropManager.
