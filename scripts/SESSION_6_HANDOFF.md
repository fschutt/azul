# Session 6 Handoff — Next Session Plan

**Date**: 2026-03-28
**Branch**: `layout-debug-clean`
**Score**: 24/44 reftests passing (started at 17/44 in session 1)
**Commits this session**: 21 (from `fc20a0cb` to `4dc4178a`)

---

## What Was Done This Session

### Layout Fixes (4 commits, +1 new test pass → 23→24/44)
- `fc20a0cb` — white-space:nowrap enforcement in `break_one_line()`, inline-block intrinsic size double-counting fix, table intrinsic sizing implementation
- `064eac20` — **Critical**: `Vec::insert` → `pos_set` in `position_out_of_flow_elements` (BTreeMap→Vec migration leftover that corrupted all sibling positions)
- `c62da631` — 14 regression tests covering all session 1-5 bugs
- `43535c21` (prior session) — cascade text-node exclusion, inline padding, font-size em resolution

### Spec Compliance Audit
- Analyzed all 16 fix commits against `+spec:` comments and prompt files
- Identified root causes: 7 missing implementations, 5 spec misunderstandings, 4 wrong assumptions, 3 incorrect refactorings

### CPU Rendering Performance (9 commits, ~12ms → ~3ms)
- `767f603e` — Rect fast-path: `blend_bar()` for non-rounded rects (5ms → 0.1ms per full-screen fill)
- `7959e265` — Border fast-path: `blend_bar()` strips for solid non-rounded borders
- `24ca33f6` — Frame buffer pooling with `acquire_pixmap()` reuse
- `557de9e8` — macOS GlyphCache persisted across frames (was recreated per frame)
- `3132ada1` — Batched glyph cells into single rasterizer pass per text run

### Glyph Cell Caching (agg-rust + azul, 2 commits)
- agg-rust `e9abe45` — `PathStorage::vertices()`, `add_path_vertices_transformed()`, `add_cells_offset()`, `outline_cells()`
- `767f603e` — Two-level GlyphCache: Level 1 (PathStorage per glyph), Level 2 (rasterizer cells per sub-pixel position)

### Incremental Text Reshaping (4 commits)
- `3097ebdf` — Per-VisualItem shaped cache (`PerItemShapedEntry`, `shape_visual_items_with_per_item_cache`)
- `72d32ef3` — `CachedLineBreaks`, `IncrementalRelayoutResult` enum, `try_incremental_relayout()`
- `a4a422ad` — Phase 2d decision tree wired in `fc.rs:2457` (currently no-op, see bugs)
- `4f2844d1` — `source_node_index` on `DisplayListItem::Text`, `patch_text_glyphs()`, `compute_text_damage_rect()`

### Damage Region Infrastructure (6 commits)
- `7eb9107d` — `DisplayListItem::bounds()` method
- `f20d1668` — `compute_display_list_damage()` with rect coalescing
- `b2181fed` — `render_display_list_damaged()` with per-region clipping
- `d23a0c7f` — Headless backend: full damage-based incremental rendering + grow-only resize
- `2d242ffd` — Wayland: per-rect `wl_surface_damage()` (infrastructure only, not yet populated)
- `acd562f5` — macOS: `previous_display_list` field added

### Tests (2 commits)
- `c62da631` — 14 regression tests (`layout/tests/session_regression.rs`)
- `4dc4178a` — 4 multi-frame rendering tests (`layout/tests/incremental_rendering.rs`)

---

## CRITICAL BUGS IN THIS SESSION'S CODE

These must be fixed before the incremental rendering pipeline is correct.

### Bug 1: `compare_region` loop bounds (ALWAYS RETURNS 0)
**File**: `layout/src/cpurender.rs:2137`
**Problem**: Loop bounds `for row in y..y.min(a.height)` evaluates to `y..0` when `y > 0`, iterating zero times.
**Fix**: Change to `for row in y..(y + h).min(a.height).min(b.height)` and inner loop to `for col in x..(x + w).min(a.width).min(b.width)`.
**Impact**: `resize_preserves_top_left_content` test gives false pass.

### Bug 2: `render_display_list_damaged` skips Push/Pop state items
**File**: `layout/src/cpurender.rs:2240`
**Problem**: `PushClip`, `PushScrollFrame`, etc. return `Some(bounds)` from `bounds()`. When their bounds don't intersect the damage rect, they are skipped. But their matching `Pop*` items return `None` and are always processed. This corrupts clip/scroll/transform stacks.
**Fix**: In `render_display_list_damaged`, always process items that return `None` from `bounds()` AND always process Push/Pop items regardless of bounds intersection. Only skip drawing items (Rect, Text, Border, etc.) that don't intersect.

### Bug 3: `compute_display_list_damage` only compares bounds, not content
**File**: `layout/src/cpurender.rs:1980`
**Problem**: A color change or text change within the same bounds produces identical `bounds()` values → no damage rect generated.
**Fix**: Compare full item content, not just bounds. Use `std::mem::discriminant` + field-by-field comparison, or derive `PartialEq` on `DisplayListItem` and compare `old_item != new_item`.

### Bug 4: Phase 2d incremental relayout is a no-op
**File**: `layout/src/solver3/fc.rs:2480`
**Problem**: `try_incremental_relayout(&[], ...)` always passes empty dirty items → always returns `GlyphSwap`. The match arm logs but doesn't `return` → always falls through to full `layout_flow()`.
**Fix**: (a) Pass actual dirty item indices from text edit path, (b) add `return Ok(cached_output)` in the GlyphSwap match arm.

### Bug 5: `source_node_index` never set to `Some`
**File**: `layout/src/solver3/display_list.rs:1426` (and all other Text constructors)
**Problem**: `source_node_index: None` everywhere → `patch_text_glyphs()` never matches any items.
**Fix**: In `push_text_run()`, pass the current layout node index and store it in the Text item.

### Bug 6: Wayland `damage_rects` never populated
**File**: `dll/src/desktop/shell2/linux/wayland/mod.rs:489`
**Problem**: `CpuFallbackState.damage_rects` is initialized to `Vec::new()` and never written. The per-rect damage branch is dead code.
**Fix**: After CPU rendering, populate `damage_rects` from the render result.

### Bug 7: `agg_fill_glyph_path` is dead code
**File**: `layout/src/cpurender.rs:1268`
**Problem**: Function defined but never called (superseded by cell-based approach).
**Fix**: Delete it.

---

## TODO LIST — EVERYTHING STILL NEEDED

### Priority 1: Fix Critical Bugs Above
1. Fix `compare_region` loop bounds
2. Fix `render_display_list_damaged` to not skip Push/Pop items
3. Fix `compute_display_list_damage` to compare full items, not just bounds
4. Wire Phase 2d to actually return cached layout (add `return`)
5. Set `source_node_index` in `push_text_run()`
6. Populate Wayland `damage_rects` from render result
7. Delete dead `agg_fill_glyph_path`

### Priority 2: Fix "draw_blue" Placeholder Windows
All CPU-rendered windows on Linux currently show a solid blue rectangle instead of actual content.

- **Wayland CPU** (`dll/src/desktop/shell2/linux/wayland/mod.rs:482`): `cpu_state.draw_blue()` → replace with actual `cpurender::render_with_font_manager_and_scroll()` into the shm buffer
- **X11 CPU** (`dll/src/desktop/shell2/linux/x11/mod.rs:510-527`): `XFillRectangle` blue fill → implement `XPutImage` with rendered pixmap
- Both need: render display list → convert RGBA→BGRA (X11) or ARGB (Wayland) → copy into platform buffer

### Priority 3: GPU Damage Regions (OpenGL Scissoring)
WebRender handles its own internal damage, but the OS compositor doesn't know which region changed. On all GPU backends:
- Before `renderer.render()`, set `glScissor()` to the damage region
- After render, on Wayland: call `wl_surface_damage_buffer()` per damage rect
- On macOS: use `setNeedsDisplayInRect:` instead of `setNeedsDisplay:`
- Early-return if no damage rects (skip `renderer.render()` entirely after first frame)

### Priority 4: Headless ContentEditable E2E Test
Create a full integration test (`layout/tests/contenteditable_e2e.rs`):
1. Create headless window with `<div contenteditable>Hello</div>`
2. Focus the div, set cursor position
3. Simulate `TextInput` event inserting "X" at cursor
4. Verify: text content is "HXello", cursor moved right by one character
5. Verify: damage rects cover only the text region
6. Verify: pixmap diff shows changes only in text area
7. Test text selection: select "Hel", verify selection highlight renders
8. Test selection damage: changing selection triggers damage in selection region only

### Priority 5: Resize Shrink Buffer Reuse
`resize_grow_only` only handles expansion. For shrinking:
- Copy the top-left `new_w × new_h` portion of the old buffer
- No new strips needed (content is clipped)
- Rename to `resize_reuse` or add `resize_shrink` that copies the overlapping region

### Priority 6: Per-Item Cache Cleanup
- `per_item_shaped` HashMap grows unboundedly — add LRU eviction or generation-based cleanup
- Coalescing in `shape_visual_items_with_per_item_cache` doesn't check `LogicalItem::CombinedText` in the lookahead loop
- Cache key uses `DefaultHasher` (u64) — collision risk. Consider storing key components for verification

### Priority 7: Cross-Platform Builds
- Run `azul-doc autofix` until 0 patches remain (separate commit per patch round)
- Build `azul-dll` with `--features build-dll --target x86_64-unknown-linux-gnu`
- Build `azul-dll` with `--features build-dll --target x86_64-pc-windows-gnu`
- Fix any cross-compilation issues

### Priority 8: Remaining Reftest Failures (20/44 still failing)
Top failing tests by diff:
| Diff | Test | Issue |
|-----:|------|-------|
| 90182 | block-margin-collapse-complex-001 | Margin collapse height overshoot |
| 67254 | cascade-display-block-001 | Font metrics |
| 49045 | cascade-multiple-classes-001 | Font metrics |
| 46300 | cascade-font-weight-inherit-001 | Font metrics |
| 45519 | cascade-specificity-001 | Font metrics |
| 42134 | cascade-ua-defaults-001 | Font metrics + table |
| 42760 | cascade-nested-selectors-001 | Font metrics |
| 41659 | cascade-inheritance-001 | Font metrics |
| 40946 | block-padding-border-001 | Block padding/border |
| 40691 | absolute-non-replaced-height-001 | Abs-pos height |

Most cascade test diffs are from font metrics differences (text rendering slightly different from Chrome), not CSS bugs.

---

## Architecture Notes for Next Agent

### Key Files
| File | Purpose |
|------|---------|
| `layout/src/cpurender.rs` | CPU rendering: fast paths, damage rendering, glyph cache, pixmap |
| `layout/src/glyph_cache.rs` | Two-level glyph cache (PathStorage + rasterizer cells) |
| `layout/src/text3/cache.rs` | Text shaping pipeline: 4-stage cache, per-item shaped cache, line breaks |
| `layout/src/solver3/fc.rs` | BFC/IFC layout, Phase 2d stub at line 2457 |
| `layout/src/solver3/display_list.rs` | Display list generation, bounds(), patch_text_glyphs() |
| `layout/src/solver3/cache.rs` | Layout cache, reconciliation, position calculation |
| `layout/src/solver3/positioning.rs` | Relative/absolute positioning (Vec::insert fix was here) |
| `dll/src/desktop/shell2/headless/mod.rs` | Headless backend with damage-based rendering |
| `dll/src/desktop/shell2/linux/wayland/mod.rs` | Wayland backend with per-rect damage |
| `dll/src/desktop/shell2/macos/mod.rs` | macOS backend with persistent glyph cache |
| `../agg-rust/src/` | Anti-Grain Geometry rasterizer (vertices, cells, scanlines) |

### agg-rust Custom Additions
- `PathStorage::vertices()` — immutable vertex slice
- `RasterizerScanlineAa::add_path_vertices_transformed()` — iterate without mutable cursor
- `RasterizerScanlineAa::add_cells_offset()` — replay cached cells at offset
- `RasterizerScanlineAa::outline_cells()` — extract cells for caching
- `RasterizerCellsAa::add_cells_offset()` — import cells with offset

### Performance Numbers
- Full frame render (showcase, 1920x1080): ~12ms → ~3ms (4x faster)
- Rect fill: 5ms → 0.1ms (blend_bar instead of rasterizer)
- Border: ~0.6ms → ~0.05ms (blend_bar strips)
- Text: glyph cell caching eliminates per-frame path→cell conversion

### Incremental Pipeline Status
```
Text change → per-item shaped cache ✓ (implemented, works)
           → line break check ✓ (implemented, not wired)
           → Phase 2d decision ✗ (stub, no-op — Bug 4)
           → display list patch ✗ (source_node_index never set — Bug 5)
           → damage render ✗ (skips push/pop items — Bug 2)
           → platform damage ✗ (Wayland rects never populated — Bug 6)
```

### Memory/Safety Notes
- `per_item_shaped` cache grows unboundedly (no eviction)
- `RowAccessor::new_with_buf()` uses unsafe raw pointer — sound as long as pixmap isn't reallocated during render
- `GlyphCellKey` uses u64 hash — theoretical collision risk
- `CpuFallbackState` on Wayland holds raw pointers to mmap'd memory (cleaned in Drop)
