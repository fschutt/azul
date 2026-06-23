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
| 2 | display_list pagination text no-op | layout solver3 display_list | TODO |
| 3 | cpurender backdrop-filter + text-shadow | layout cpurender | TODO |
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
- [ ] Thread font resources into the pagination text path; shape the string and emit
      real glyph display items (mirror the main text display-item path).
- [ ] Confirm header/footer text actually renders (PDF export / cpurender path).
- Verify: `cargo test -p azul-layout`; a pagination/PDF render test if one exists.

## Item 3 — cpurender backdrop-filter + text-shadow  (both no-ops)
Files: `layout/src/cpurender/compositor.rs` (~backdrop-filter), `layout/src/cpurender/raster.rs` (~text-shadow).
Note: cpurender is now a directory (post-split). `filter` is already wired; these two are
still complete no-ops in the compositor layer path (allocate_layers + composite_frame).
- [ ] Implement `text-shadow` (offset+blur+color behind glyphs) in the raster path.
- [ ] Implement `backdrop-filter` (sample+filter the backdrop under the layer) in the
      compositor path — OR, if too heavy, document as a known limitation (conservative
      on rendering per maintainer; prefer a reftest if implementing).
- Verify: `cargo check -p azul-layout`; add/extend a reftest (none exist for these yet).

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
