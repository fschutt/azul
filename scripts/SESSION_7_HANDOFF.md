# Session 7 Handoff — Next Session Plan

**Date**: 2026-03-28
**Branch**: `layout-debug-clean`
**Score**: 24/44 reftests passing (unchanged from session 6)
**Uncommitted changes**: 14 files, +694 insertions, -105 deletions

---

## What Was Done This Session

### Priority 1: All 7 Critical Bugs Fixed

1. **Bug 1** — `compare_region` loop bounds (`cpurender.rs:2143`): Changed `y..y.min()` to `y..(y+h).min()` — was always iterating zero times for y > 0.
2. **Bug 2** — `render_display_list_damaged` Push/Pop skipping (`cpurender.rs:2302`): Added `is_state_management()` method; always process Push/Pop items regardless of bounds intersection.
3. **Bug 3** — `compute_display_list_damage` content comparison (`cpurender.rs:2035`): Added `is_visually_equal()` on `DisplayListItem` — now compares color, glyphs, font_hash, not just bounds.
4. **Bug 4** — Phase 2d GlyphSwap no-op (`fc.rs:2488`): Added `return Ok(output)` in GlyphSwap match arm with cached layout reconstruction.
5. **Bug 5** — `source_node_index` never set (`display_list.rs:1485`): Added `source_node_index: Option<usize>` param to `push_text_run()`, caller passes `Some(source_node_index)`.
6. **Bug 6** — Wayland `damage_rects` never populated: Added `set_damage_rects()` helper, `pixel_buffer_mut()` accessor, clear-after-use in `present()`.
7. **Bug 7** — Dead `agg_fill_glyph_path`: Deleted (was 35 lines of unused code).

### Priority 2: CPU Rendering on Linux (Replaces draw_blue)

- **Wayland** (`wayland/mod.rs:2728-2806`): Full CPU rendering via `render_with_font_manager_and_scroll()` → RGBA→ARGB conversion → copy into shm buffer. Fallback to draw_blue if layout not ready.
- **X11** (`x11/mod.rs:1530-1679`): Full CPU rendering via `render_with_font_manager_and_scroll()` → RGBA→BGRA conversion → `XCreateImage`/`XPutImage`. Added `XImage` struct, `XCreateImage`/`XPutImage`/`XDestroyImage` to dlopen.
- Both platforms: `glyph_cache` field added (persists across frames like macOS).

### Priority 4: ContentEditable E2E Test Suite

Created `layout/tests/contenteditable_e2e.rs` with 6 tests:
1. **Initial render** — verifies glyphs shaped (>=11 for "Hello World"), fonts resolved, no cursor before focus
2. **Text input** — focus + type "X" → verifies changeset (old='Hello', inserted='X'), cursor at byte 6, glyph count 5→6, pixel diff 0.3%, 1 damage rect
3. **Multiple keystrokes** — types "1","2","3" into "AB", each frame differs from previous
4. **Damage detection** — static header + editable div, verifies <20% pixels change
5. **Two editors** — types in editor1 then editor2, both produce isolated visual changes
6. **Incremental=full** — two renders of same display list produce identical pixmaps

All 6 tests pass. Screenshots saved to `layout/test_output/contenteditable_e2e/`.

### Priority 5: Resize Shrink Buffer Reuse

Added `AzulPixmap::resize_reuse()` (`cpurender.rs:999`): handles both grow AND shrink by copying the overlapping region. Updated headless backend to use it.

### Priority 6: Per-Item Cache Cleanup

- Added `per_item_accessed: HashSet<u64>` and `generation: u64` to `LayoutCache` (`text3/cache.rs:5054-5056`)
- `begin_generation()` evicts entries not accessed, triggered when cache > 4096 entries
- Fixed coalescing to check `LogicalItem::CombinedText` in lookahead loop (was only checking `Text`)

### Priority 3: GPU Damage Regions (All Platforms)

- **Wayland GPU** (`wayland/mod.rs`): Added `gpu_damage_rects` field. After `swap_buffers()`, calls `wl_surface_damage()` per damage rect instead of full surface.
- **macOS GPU** (`macos/mod.rs`): Added `gpu_damage_rects` field. `request_redraw()` uses `setNeedsDisplayInRect:` when damage rects available, falls back to `setNeedsDisplay:`.
- **Early-return optimization** (all platforms): Skip `renderer.render()` when `display_list_initialized && !display_list_dirty && !scroll_active && !scrollbar_fade`. This is the biggest GPU win — eliminates ~60fps of wasted rendering when UI is static.
  - macOS: In `render_and_present_in_draw_rect()`, checks scroll/fade/virtual-view
  - X11: In `render_and_present()`, checks animations/fade/pending-updates
  - Windows: Same pattern in `render_and_present()`
  - Wayland: Already had effective early-return via `generate_frame_if_needed()`
- **ScrollManager**: Added `has_active_animations()` method (`scroll_state.rs:534`)

### Other Changes

- `DisplayList` now derives `Clone` (needed for damage comparison in tests)
- `compositor2.rs`: Added `source_node_index: _` to Text match pattern
- `.gitignore`: Added `test_output/`
- `azul-doc autofix`: 0 patches needed (clean)
- `azul-doc codegen all`: All 14 bindings generated successfully
- `cargo build -p azul-dll --features build-dll --release`: macOS builds clean (3m14s)

---

## KNOWN BUGS IN THIS SESSION'S CODE

### Bug A: Cursor not visible in screenshots

**Problem**: After `focus_node()` + `type_text()`, `should_draw_cursor()` returns true but no `CursorRect` appears in the display list.

**Root cause**: `paint_cursor()` at `display_list.rs:1948` compares `dom_id != *cursor_node_id`. The cursor must be on the TEXT NODE (NodeId=2), not the contenteditable DIV (NodeId=1). The test's `focus_node` now correctly finds the text child via `find_last_text_child` logic, but `regenerate_display_list_for_dom` (called by `apply_text_changeset`) may not re-run `paint_cursor` on the correct node.

**Investigation needed**: Check if `regenerate_display_list_for_dom` traverses all nodes including text children when calling `paint_cursor`. The cursor location is set on the text child node but the display list generator may skip it.

### Bug B: Text insertion position inconsistent across editors

**Problem**: When switching focus between two editors, the cursor position from the previous editor leaks. Typing "!" in editor1 inserts at the beginning (cursor at position 0 before init), then switching to editor2 and typing "?" inserts after byte 1 instead of at end.

**Root cause**: `apply_text_changeset()` at `window.rs:4410` calls `move_cursor_to()` with the new cursor from `edit_text()`. When focus switches to editor2, the cursor should be re-initialized at end via `initialize_cursor_at_end`, but in the test this is done manually. The real platform pipeline calls `finalize_pending_focus_changes()` after layout which properly initializes cursor per W3C spec.

**Fix**: The test's `focus_node()` correctly calls `initialize_cursor_at_end`, but the test may call `type_text` before the cursor is properly re-initialized. Need to verify the exact cursor state between focus switch and text input.

### Bug C: `compositor2.rs` needed pattern update

**File**: `dll/src/desktop/compositor2.rs:1180`
**Fixed**: Added `source_node_index: _` to `DisplayListItem::Text` match pattern.

---

## TODO LIST — EVERYTHING STILL NEEDED

### Priority 1: Fix Cursor Rendering in CPU Path

The cursor (caret) is not rendered in CPU screenshots despite `should_draw_cursor()=true` and `cursor_location=Some(...)`. This is the most user-visible issue.

Steps:
1. Add debug logging to `paint_cursor()` in display_list.rs to trace why no CursorRect is emitted
2. Check if `regenerate_display_list_for_dom` properly traverses text child nodes
3. Verify `get_cursor_rect()` returns a valid rect from the text layout
4. After fix: update contenteditable_e2e test to assert `has_cursor_rect() == true`

### Priority 2: Fix Focus Restyle (`:focus` CSS)

When focus changes, the `:focus` pseudo-class should trigger a restyle (e.g., changing border color). The test harness bypasses the restyle pipeline by calling `focus_manager.set_focused_node()` directly. Need to either:
- Call the restyle pipeline from the test
- Or add a `restyle_focus_change()` method to LayoutWindow

### Priority 3: Scroll-Into-View for Single-Line Contenteditable

When text overflows a `white-space: nowrap; overflow: hidden` contenteditable, the view should scroll horizontally to keep the cursor visible. Test:
1. Create narrow contenteditable with long text
2. Type at end — cursor should remain visible
3. Arrow key left — should scroll left
4. Verify `scroll_selection_into_view()` fires correctly

### Priority 4: Selection Rendering Verification

Add E2E test for text selection:
1. Focus contenteditable, set selection range
2. Verify `SelectionRect` appears in display list with correct color
3. Verify pixel diff shows selection highlight only in selected region
4. Test selection + typing (should replace selected text)

### Priority 5: Cross-Platform Build Fixes

Check and fix compilation for:
- `--target x86_64-unknown-linux-gnu`
- `--target x86_64-pc-windows-gnu`
Any errors from `source_node_index` pattern matches, feature gating, or new X11/Wayland symbols.

### Priority 6: Remaining Reftest Failures (20/44 still failing)

Most cascade test diffs are font metrics. Top failures:
| Diff | Test | Issue |
|-----:|------|-------|
| 90182 | block-margin-collapse-complex-001 | Margin collapse height |
| 67254 | cascade-display-block-001 | Font metrics |
| 40946 | block-padding-border-001 | Block padding/border |
| 40691 | absolute-non-replaced-height-001 | Abs-pos height |

---

## Architecture Notes

### Cursor/ContentEditable Architecture

```
DOM:  <div contenteditable>Hello</div>
      ├── NodeId=1 (div, contenteditable flag)
      └── NodeId=2 (Text "Hello")

Focus Flow (W3C "flag and defer"):
1. Event: Tab → focus lands on div (NodeId=1)
2. Flag: set_pending_contenteditable_focus(dom_id, container=1, text_child=2)
3. Layout: compute text layout → inline_layout_result on IFC root
4. Finalize: finalize_pending_focus_changes() → initialize_cursor_at_end(dom_id, NodeId=2, layout)
5. Render: paint_cursor() matches NodeId=2 → emit CursorRect

Cursor is ALWAYS on the TEXT NODE, not the container div.
```

### Selection Architecture

- **SelectionManager** stores: `BTreeMap<DomId, SelectionState>` (legacy) + `BTreeMap<DomId, TextSelection>` (new)
- **Selection color**: CSS `-azul-selection-background-color`, default `ColorU(173, 214, 255, 255)` (light blue)
- **Rendering**: `paint_selections()` → `layout.get_selection_rects(range)` → `DisplayListItem::SelectionRect`
- CPU path renders via `render_rect()` at `cpurender.rs:2417`

### Key Files Changed This Session

| File | Changes |
|------|---------|
| `layout/src/cpurender.rs` | Bugs 1,3,7 + resize_reuse |
| `layout/src/solver3/display_list.rs` | Bugs 2,5 + is_visually_equal, is_state_management, Clone |
| `layout/src/solver3/fc.rs` | Bug 4 (Phase 2d return) |
| `layout/src/text3/cache.rs` | Priority 6 (cache eviction + CombinedText fix) |
| `dll/src/desktop/shell2/linux/wayland/mod.rs` | Bug 6 + Priority 2 (CPU render) |
| `dll/src/desktop/shell2/linux/x11/mod.rs` | Priority 2 (CPU render via XPutImage) |
| `dll/src/desktop/shell2/linux/x11/defines.rs` | XImage struct + XCreateImage/XPutImage/XDestroyImage |
| `dll/src/desktop/compositor2.rs` | source_node_index pattern fix |
| `dll/src/desktop/shell2/headless/mod.rs` | resize_reuse integration |
| `dll/src/desktop/shell2/macos/mod.rs` | GPU damage rects + setNeedsDisplayInRect + early-return |
| `dll/src/desktop/shell2/windows/mod.rs` | GPU early-return optimization |
| `layout/src/managers/scroll_state.rs` | has_active_animations() for GPU early-return |
| `layout/tests/contenteditable_e2e.rs` | 6 E2E tests |
