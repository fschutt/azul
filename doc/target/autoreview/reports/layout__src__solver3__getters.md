# Review: layout/src/solver3/getters.rs

## Summary
- Lines: 4719
- Public functions: ~80+ (many macro-generated)
- Public structs/enums: 8 (`MultiValue`, `BorderInfo`, `SelectionStyle`, `CaretStyle`, `CollectedFontStacks`, `ResolvedFontChains`, `FontLoadResult`, `ComputedScrollbarStyle`)
- Findings: 2 high, 3 medium, 0 low

## Findings

### [HIGH] Dead Code — 19 public functions with zero external call sites

- **Location**: Various locations throughout the file
- **Details**: The following public functions have zero call sites outside `getters.rs`:
  1. `should_clip_scrollbar_to_border` (line 3879)
  2. `get_scrollbar_width_px` (line 3889)
  3. `is_avoid_page_break` (line 2985)
  4. `is_avoid_break_inside` (line 2990)
  5. `resolve_and_load_fonts` (line 3532)
  6. `register_embedded_fonts_from_styled_dom` (line 3386)
  7. `get_display_raw` (line 4490)
  8. `ResolvedFontChains::font_refs_len` (line 3084)
  9. `ResolvedFontChains::get_by_chain_key` (line 3035)
  10. `ResolvedFontChains::get_for_font_stack` (line 3040)
  11. `ResolvedFontChains::get_for_font_ref` (line 3046)
  12. `get_object_fit_property` (line 1207)
  13. `get_object_position_property` (line 1221)
  14. `get_initial_letter_align_property` (line 1177)
  15. `get_initial_letter_wrap_property` (line 1184)
  16. `get_dominant_baseline_property` (line 1163)
  17. `get_alignment_baseline_property` (line 1170)
  18. `get_text_box_edge_property` (line 1156)
  19. `get_cursor_property` (line 4168)
- **Evidence**: Grepped for each function name across the entire codebase; zero matches outside `getters.rs` itself.
- **Recommendation**: Mark with `#[allow(dead_code)]` if planned for future use per the GETTER_MIGRATION_PLAN.md, or remove. The legacy wrappers (`register_embedded_fonts_from_styled_dom`, `resolve_and_load_fonts`) are explicitly documented as legacy — consider removing them.


### [MEDIUM] Refactoring — `get_border_info` is 157 lines of repetitive code

- **Location**: `getters.rs:1645-1801`
- **Details**: The fast path and slow path each repeat the same pattern 4 times (top/right/bottom/left) for widths, colors, and styles. The slow path alone is 12 near-identical blocks.
- **Recommendation**: Could use a helper macro or closure to reduce repetition, though this is a common pattern in the codebase.


### [MEDIUM] `..Default::default()` in `get_style_properties` — safe but undocumented fields

- **Location**: `getters.rs:2688-2689`
- **Details**: The `StyleProperties` construction uses `..Default::default()` to default 10 fields: `font_features`, `font_variations`, `text_transform`, `writing_mode`, `text_orientation`, `text_combine_upright`, `font_variant_caps`, `font_variant_numeric`, `font_variant_ligatures`, `font_variant_east_asian`. The comment on lines 2685-2687 acknowledges this. All default to sensible zero/empty values.
- **Recommendation**: Safe as-is, but the comment should be kept up to date if new fields are added to `StyleProperties`.

## System Documentation
- System identified: yes — Layout Solver / CSS Property Resolution
- Existing doc: `doc/guide/styling-system.md`, `doc/guide/css-properties.md`, `doc/guide/css-styling.md`
- Doc needed: A `doc/guide/layout-solver.md` covering the layout pipeline (layout_tree → sizing → positioning → display_list), including how getters.rs centralizes CSS property access. The existing styling docs cover CSS parsing/styling, not the layout solver's property resolution layer.
