# Session 8K: Font Fallback + Selection Rendering Fix Plan

## Problem Summary

Two distinct bugs prevent correct text rendering:

### Bug 1: CJK/Unicode Characters Don't Render in Preedit (or anywhere)

**Root cause chain** (3 levels deep):

1. **`expand_font_families` called with empty `&[]` unicode ranges**
   - `rust-fontconfig` `resolve_font_chain_uncached()` (lib.rs:1865) calls:
     ```rust
     let expanded_families = expand_font_families(font_families, os, &[]);
     ```
   - With empty ranges, `has_cjk_ranges(&[])` → false
   - `get_monospace_fonts(&[])` returns only Latin fonts (SF Mono, Menlo, Monaco...)
   - CJK fonts (Hiragino Sans, PingFang SC) are **never added** to the expansion
   - Same issue in both rust-fontconfig 2.0.0 and 3.0.0

2. **`unicode_fallbacks` is permanently `Vec::new()`**
   - `resolve_font_chain_uncached()` line 1983:
     ```rust
     FontFallbackChain { css_fallbacks, unicode_fallbacks: Vec::new(), ... }
     ```
   - `find_unicode_fallbacks()` exists but is `#[allow(dead_code)]` and never called
   - `resolve_char()` checks CSS fallbacks then unicode_fallbacks — both miss CJK

3. **Per-character font fallback in shaping was missing** (fixed this session)
   - `shape_visual_items()` used only the first character to pick a font for the entire run
   - Even if CJK fonts were in the chain, they wouldn't be used
   - **FIXED**: Added `split_text_by_font_coverage()` + `shape_with_font_fallback()`

**Verification from logs:**
```
[FONT FALLBACK] text needs 2 font segments for 'Hello World - Click hにere and type!'
[FONT FALLBACK] text='Hello World - Click h' uses font ...0017 (bytes 0..21)
[FONT FALLBACK] text='ere and type!' uses font ...0017 (bytes 24..37)
```
Bytes 21..24 (the "に" character) is **completely missing** — `resolve_char` returned `None`.

### Bug 2: Mouse Selection Doesn't Render Visually

**Root cause:** `process_mouse_drag_for_selection()` is a TODO stub.

```rust
// layout/src/window.rs:5825-5832
pub fn process_mouse_drag_for_selection(...) -> Option<Vec<DomNodeId>> {
    // TODO: redesign drag selection for multi-cursor model
    None
}
```

- Mouse click creates a cursor at click position via `process_mouse_click_for_selection()`
  (uses `initialize_editing(final_range.start, ...)`)
- But mouse drag never extends the selection — the stub returns `None`
- Keyboard Shift+Arrow works because it goes through `ApplySelectionOp` with `SelectionMode::Extend`
- Display list paints selections via `build_text_selections_map()` which reads `Selection::Range` from `MultiCursorState` — mouse drag never creates Range selections

---

## Architecture: Current Font Loading Pipeline

```
                    ┌─────────────────────────────────────┐
                    │  collect_font_stacks_from_styled_dom │
                    │  (getters.rs:3100)                   │
                    │  Scans DOM CSS for unique font stacks│
                    └──────────────────┬──────────────────┘
                                       │
                    ┌──────────────────▼──────────────────┐
                    │  resolve_font_chains                 │
                    │  (getters.rs:3257)                   │
                    │  For each stack:                     │
                    │    fc_cache.resolve_font_chain(       │
                    │      &families, weight, italic, ...)  │
                    │                                      │
                    │  PROBLEM: expand_font_families(&[])  │
                    │  → CJK fonts never added             │
                    └──────────────────┬──────────────────┘
                                       │
                    ┌──────────────────▼──────────────────┐
                    │  collect_font_ids_from_chains        │
                    │  (getters.rs:3407)                   │
                    │  Extracts FontIds from:              │
                    │    css_fallbacks ✓ (but no CJK)      │
                    │    unicode_fallbacks ✗ (always empty) │
                    └──────────────────┬──────────────────┘
                                       │
                    ┌──────────────────▼──────────────────┐
                    │  load_fonts_from_disk                 │
                    │  (getters.rs:3464)                   │
                    │  Loads+parses font bytes from disk   │
                    │  Only for FontIds in the chain       │
                    │  → CJK fonts never loaded            │
                    └──────────────────┬──────────────────┘
                                       │
                    ┌──────────────────▼──────────────────┐
                    │  font_manager.get_loaded_fonts()     │
                    │  (cache.rs:717)                      │
                    │  Returns LoadedFonts<T> snapshot     │
                    └──────────────────┬──────────────────┘
                                       │
                    ┌──────────────────▼──────────────────┐
                    │  shape_visual_items()                 │
                    │  (cache.rs:6148)                     │
                    │  Per-char font fallback (NEW):       │
                    │    resolve_char() → FontId           │
                    │    loaded_fonts.get(&font_id)        │
                    │  PROBLEM: CJK FontId not in loaded   │
                    └─────────────────────────────────────┘
```

---

## Fix Plan

### Phase 1: Fix Font Chain Resolution (rust-fontconfig)

**Goal:** Make the font chain include CJK/Arabic/Cyrillic fonts for ALL generic families.

**Option A (preferred):** Use local rust-fontconfig 3.0.0 with a fix

In `azul/dll/Cargo.toml` and `azul/layout/Cargo.toml`:
```toml
rust-fontconfig = { path = "../rust-fontconfig", ... }
```

Then fix `resolve_font_chain_uncached()` in rust-fontconfig to always pass
comprehensive unicode ranges to `expand_font_families`:

```rust
// In resolve_font_chain_uncached():
// Instead of:
let expanded_families = expand_font_families(font_families, os, &[]);

// Use:
let all_ranges = vec![
    UnicodeRange { start: 0x0000, end: 0xFFFF },  // BMP
];
let expanded_families = expand_font_families(font_families, os, &all_ranges);
```

This ensures `has_cjk_ranges()` returns true, and CJK fonts are added to every
generic family expansion (Hiragino Sans for monospace on macOS, etc.).

**Alternative**: Pass `UnicodeRange { start: 0, end: 0x10FFFF }` to cover all planes.

**Impact:** The font chain for `monospace` will now include:
- macOS: SF Mono, Menlo, Monaco, Courier, **Hiragino Sans, PingFang SC**
- Linux: Source Code Pro, DejaVu Sans Mono, **Noto Sans Mono CJK SC/JP**
- Windows: Consolas, Courier New, **MS Gothic, SimHei**

These will be matched by `resolve_char()` for CJK characters.

### Phase 2: On-Demand Font Loading

**Goal:** When `resolve_char()` returns a FontId for a CJK font, that font must
be loaded and parsed. Currently, only fonts needed at initial layout are loaded.

**Where to fix:** `layout/src/text3/cache.rs` — `shape_with_font_fallback()`

When `loaded_fonts.get(&font_id)` returns `None` for a segment:

1. **Immediate fix:** Change the signature to accept `&FontManager<T>` and call
   `font_manager.load_font_on_demand(font_id, fc_cache)` which:
   - Gets font bytes from `fc_cache.get_font_bytes(&font_id)`
   - Parses them with the same parser used during initial load
   - Inserts into the FontManager's parsed_fonts cache
   - Returns a reference to the parsed font

2. **Cache it:** The parsed font stays in FontManager for all future uses.

**Files to modify:**
- `layout/src/text3/cache.rs`: `shape_with_font_fallback()` — add font_manager param
- `layout/src/text3/cache.rs`: `shape_visual_items()` — pass font_manager through
- `layout/src/text3/cache.rs`: `FontManager` — add `load_font_on_demand()` method
- `layout/src/window.rs`: `relayout_text_node_internal()` — pass font_manager
- `layout/src/solver3/fc.rs`: existing call sites — pass font_manager
- `layout/src/solver3/sizing.rs`: existing call sites — pass font_manager

**Key design decision:** `loaded_fonts` is currently a snapshot (`LoadedFonts<T>`)
taken once before shaping. For on-demand loading, we need either:
- (a) Take `&FontManager<T>` directly and use `get_loaded_fonts()` per-segment
  (simple but acquires lock per-segment)
- (b) Try the snapshot first, fall back to font_manager on miss
  (optimal, lock-free fast path)

Approach (b) is better:
```rust
fn shape_with_font_fallback<T: ParsedFontTrait>(
    ...
    loaded_fonts: &LoadedFonts<T>,
    font_manager: &FontManager<T>,  // NEW: for on-demand loading
    fc_cache: &FcFontCache,         // already passed
) -> Result<Vec<ShapedCluster>, LayoutError> {
    for (seg_start, seg_end, font_id) in &segments {
        // Try snapshot first (lock-free, fast)
        let font = match loaded_fonts.get(font_id) {
            Some(f) => f.shallow_clone(),
            None => {
                // On-demand load: get bytes, parse, cache
                match font_manager.load_font_on_demand(font_id, fc_cache) {
                    Some(f) => f,
                    None => continue,
                }
            }
        };
        ...
    }
}
```

### Phase 3: Mouse Drag Selection

**Goal:** Implement `process_mouse_drag_for_selection()` to create Range selections.

**Implementation** (`layout/src/window.rs:5825`):

```rust
pub fn process_mouse_drag_for_selection(
    &mut self,
    _start_position: LogicalPosition,
    current_position: LogicalPosition,
) -> Option<Vec<DomNodeId>> {
    // 1. Get the anchor cursor from MultiCursorState primary cursor
    let mc = self.text_edit_manager.multi_cursor.as_mut()?;
    let anchor = mc.get_primary_cursor()?;
    let dom_id = mc.node_id.dom;
    let node_id = mc.node_id.node.into_crate_internal()?;

    // 2. Hit-test current_position to get focus cursor
    let layout_result = self.layout_results.get(&dom_id)?;
    let tree = &layout_result.layout_tree;
    let layout_idx = tree.nodes.iter()
        .position(|n| n.dom_node_id == Some(node_id))?;
    let cached = tree.warm(layout_idx)?.inline_layout_result.as_ref()?;

    // Convert to local coordinates
    let node_pos = layout_result.calculated_positions.get(layout_idx)?;
    let local_pos = LogicalPosition {
        x: current_position.x - node_pos.x,
        y: current_position.y - node_pos.y,
    };

    let focus = cached.layout.hittest_cursor(local_pos)?;

    // 3. Update primary selection to Range(anchor, focus)
    mc.set_primary_selection(Selection::Range(SelectionRange {
        start: anchor,
        end: focus,
    }));

    // 4. Mark dirty + regenerate
    self.text_edit_manager.mark_dirty();
    self.regenerate_display_list_for_dom(dom_id);

    let dom_node_id = DomNodeId { dom: dom_id, node: mc.node_id.node };
    Some(vec![dom_node_id])
}
```

**Also needed:** `MultiCursorState::set_primary_selection()` method in `core/src/selection.rs`.

### Phase 4: Cleanup

1. Remove all debug `eprintln!` lines:
   - `[FONT FALLBACK]` logs in `cache.rs`
   - `[PREEDIT inject]` logs in `window.rs`
   - `[IME ...]` logs in `macos/mod.rs`
   - `[RENDER]` logs in `window.rs`
   - `[PAINT]` logs in `display_list.rs`

2. Run `azul-doc autofix` + `azul-doc codegen` to verify no FFI changes.

---

## File Reference

| File | Lines | What |
|------|-------|------|
| `rust-fontconfig/src/lib.rs` | 1865 | `expand_font_families(&[])` — root cause |
| `rust-fontconfig/src/lib.rs` | 1983 | `unicode_fallbacks: Vec::new()` |
| `rust-fontconfig/src/lib.rs` | 256-289 | `get_monospace_fonts()` CJK-aware expansion |
| `layout/src/text3/cache.rs` | 6046-6070 | `split_text_by_font_coverage()` (new) |
| `layout/src/text3/cache.rs` | 6077-6147 | `shape_with_font_fallback()` (new) |
| `layout/src/text3/cache.rs` | 6148+ | `shape_visual_items()` — 3 call sites fixed |
| `layout/src/text3/cache.rs` | 710-723 | `FontManager::get_loaded_fonts()` |
| `layout/src/solver3/getters.rs` | 3257-3318 | `resolve_font_chains()` |
| `layout/src/solver3/getters.rs` | 3407-3425 | `collect_font_ids_from_chains()` |
| `layout/src/solver3/getters.rs` | 3464-3512 | `load_fonts_from_disk()` |
| `layout/src/window.rs` | 4946-4983 | `apply_preedit_to_text_cache()` |
| `layout/src/window.rs` | 5096-5140 | `relayout_text_node_internal()` |
| `layout/src/window.rs` | 5528-5807 | `process_mouse_click_for_selection()` |
| `layout/src/window.rs` | 5825-5832 | `process_mouse_drag_for_selection()` — TODO stub |
| `layout/src/managers/text_edit.rs` | 264-307 | `build_text_selections_map()` |

---

## Execution Order

1. **Phase 1** (font chain) — unblocks Phase 2, fast to implement
2. **Phase 2** (on-demand loading) — makes CJK actually render
3. **Phase 3** (mouse selection) — independent of Phase 1-2
4. **Phase 4** (cleanup) — after verification

Phases 1+2 are the critical path for CJK rendering.
Phase 3 is independent and can be done in parallel.
