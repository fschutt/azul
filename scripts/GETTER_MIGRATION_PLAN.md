# Getter Migration Plan: CssPropertyCache → Centralized Getters

## Goal

Route ALL CSS property access through `layout/src/solver3/getters.rs`.  
This allows us to later replace the BTreeMap-based `CssPropertyCache` with an  
FxHashMap-based system that uses the 3-tier compact cache as the primary path.

After migration, `CssPropertyCache.get_*()` methods should ONLY be called from:
1. `getters.rs` (as the slow-path fallback)
2. `compact_cache_builder.rs` (to build the compact cache once)
3. `core/src/prop_cache.rs` internal helper methods
4. `core/src/styled_dom.rs` for generic property access (devtools, animation)

## Current State

### Properties WITH compact cache fast path (already migrated)

**Tier 1 — Bitpacked enums (21 properties):**
- display, position, float, overflow_x, overflow_y, box_sizing
- flex_direction, flex_wrap, justify_content, align_items, align_content
- writing_mode, clear, font_weight, font_style, text_align
- visibility, white_space, direction, vertical_align, border_collapse

**Tier 2 — CompactNodeProps (96 bytes/node):**
- width, height, min_width, max_width, min_height, max_height, flex_basis, font_size
- padding_top/right/bottom/left, margin_top/right/bottom/left
- border_top/right/bottom/left_width, top/right/bottom/left
- flex_grow, flex_shrink, z_index
- border_top/right/bottom/left_color (u32 RGBA)
- border_styles_packed (u16 — top/right/bottom/left)
- border_spacing_h, border_spacing_v, tab_size

**Tier 2b — CompactTextProps (24 bytes/node):**
- text_color, font_family_hash, line_height, letter_spacing, word_spacing, text_indent

### Files with remaining direct CssPropertyCache access

| File | Direct calls | Status |
|------|-------------|--------|
| layout/src/solver3/fc.rs | 23 | height, shape_inside/outside, text_justify, line_height, hyphens, text_indent, column_count/gap, initial_letter, line_clamp, hanging_punctuation, text_combine_upright, exclusion_margin, hyphenation_language, table_layout, border_collapse (slow), border_spacing (slow), caption_side, border resolver |
| layout/src/solver3/taffy_bridge.rs | 17 | gap, grid_template_rows/columns/areas, grid_auto_rows/columns/flow, grid_column/row, flex_wrap, justify_items, justify_content, flex_grow/shrink, flex_basis, align_self, justify_self |
| layout/src/solver3/display_list.rs | 12 | cursor, opacity(×2), filter, backdrop_filter, box_shadow(×4), text_shadow, transform |
| layout/src/solver3/getters.rs | ~30 | border_radius(×12), background_content(×2), z_index, selection_*, caret_*, text_decoration, list_style, break_*, orphans, widows, box_decoration_break, scrollbar_*, user_select, font_family |
| layout/src/solver3/cache.rs | 2 | counter_reset, counter_increment |
| layout/src/hit_test.rs | 1 | cursor |
| layout/src/callbacks.rs | 1 | generic get_property() |
| core/src/styled_dom.rs | 8 | generic get_property(×6), background_content, position |
| core/src/gpu.rs | 3 | transform, transform_origin, opacity |

## Migration Plan — Property by Property

### Phase 1: taffy_bridge.rs — Use existing getters + add new ones

These properties already have getters in getters.rs but taffy_bridge.rs bypasses them:

| Property | Existing getter | taffy_bridge.rs line |
|----------|----------------|---------------------|
| FlexWrap | `get_wrap()` | L711 |
| JustifyContent | `get_justify_content()` | L739 |

These need NEW getters in getters.rs:

| Property | New getter name | Compact cache? | taffy_bridge.rs line |
|----------|----------------|---------------|---------------------|
| FlexGrow | `get_flex_grow()` | Yes (u16) | L751 |
| FlexShrink | `get_flex_shrink()` | Yes (u16) | L763 |
| FlexBasis | `get_flex_basis()` | Yes (u32_dim) | L773 |
| AlignSelf | `get_align_self()` | No (Tier 3) | L798 |
| JustifySelf | `get_justify_self()` | No (Tier 3) | L807 |
| JustifyItems | `get_justify_items()` | No (Tier 3) | L729 |
| Gap | `get_gap()` | No (Tier 3) | L550 |
| GridTemplateRows | `get_grid_template_rows()` | No (Tier 3) | L571 |
| GridTemplateColumns | `get_grid_template_columns()` | No (Tier 3) | L589 |
| GridTemplateAreas | `get_grid_template_areas()` | No (Tier 3) | L607 |
| GridAutoRows | `get_grid_auto_rows()` | No (Tier 3) | L638 |
| GridAutoColumns | `get_grid_auto_columns()` | No (Tier 3) | L650 |
| GridAutoFlow | `get_grid_auto_flow()` | No (Tier 3) | L667 |
| GridColumn | `get_grid_column()` | No (Tier 3) | L680 |
| GridRow | `get_grid_row()` | No (Tier 3) | L693 |

### Phase 2: fc.rs — IFC/text properties + table properties

Need NEW getters in getters.rs:

| Property | New getter name | Compact cache? | fc.rs line |
|----------|----------------|---------------|-----------|
| Height (raw) | (use existing get_css_height) | Yes | L2538 |
| ShapeInside | `get_shape_inside()` | No (Tier 3) | L2577 |
| ShapeOutside | `get_shape_outside()` | No (Tier 3) | L2612 |
| TextJustify | `get_text_justify()` | No (Tier 3) | L2642 |
| LineHeight | `get_line_height()` | Yes (Tier 2b) | L2653 |
| Hyphens | `get_hyphens()` | No (Tier 3) | L2660 |
| TextIndent | `get_text_indent()` | Yes (Tier 2b) | L2706 |
| ColumnCount | `get_column_count()` | No (Tier 3) | L2726 |
| ColumnGap | `get_column_gap()` | No (Tier 3) | L2738 |
| InitialLetter | `get_initial_letter()` | No (Tier 3) | L2774 |
| LineClamp | `get_line_clamp()` | No (Tier 3) | L2793 |
| HangingPunctuation | `get_hanging_punctuation()` | No (Tier 3) | L2801 |
| TextCombineUpright | `get_text_combine_upright()` | No (Tier 3) | L2810 |
| ExclusionMargin | `get_exclusion_margin()` | No (Tier 3) | L2822 |
| HyphenationLanguage | `get_hyphenation_language()` | No (Tier 3) | L2831 |
| TableLayout | `get_table_layout()` | No (Tier 3) | L3335 |
| CaptionSide | `get_caption_side()` | No (Tier 3) | L3425 |

### Phase 3: display_list.rs — Paint properties

Need NEW getters in getters.rs:

| Property | New getter name | Compact cache? | display_list.rs line |
|----------|----------------|---------------|---------------------|
| Cursor | `get_cursor()` | No (Tier 3) | L1363 |
| Opacity | `get_opacity()` | No (Tier 3) | L1730, L3415 |
| Filter | `get_filter()` | No (Tier 3) | L1745 |
| BackdropFilter | `get_backdrop_filter()` | No (Tier 3) | L1761 |
| BoxShadowLeft | `get_box_shadow_left()` | No (Tier 3) | L2408 |
| BoxShadowRight | `get_box_shadow_right()` | No (Tier 3) | L2409 |
| BoxShadowTop | `get_box_shadow_top()` | No (Tier 3) | L2410 |
| BoxShadowBottom | `get_box_shadow_bottom()` | No (Tier 3) | L2411 |
| TextShadow | `get_text_shadow()` | No (Tier 3) | L2711 |
| Transform | `get_transform()` | No (Tier 3) | L3430 |

### Phase 4: getters.rs internal — Add compact fast paths where missing

Already in getters.rs but without compact fast path:

| Property | Current getter | Add compact? |
|----------|---------------|-------------|
| BorderRadius (×4) | `get_style_border_radius()` | No (Tier 3) |
| ZIndex | `get_z_index()` | Yes (i16 in Tier 2) |
| BackgroundContent | `get_background_color()` / `get_background_contents()` | No (Tier 3) |
| SelectionBgColor | `get_selection_style()` | No (Tier 3) |
| SelectionColor | `get_selection_style()` | No (Tier 3) |
| SelectionRadius | `get_selection_style()` | No (Tier 3) |
| CaretColor | `get_caret_style()` | No (Tier 3) |
| CaretWidth | `get_caret_style()` | No (Tier 3) |
| CaretAnimationDuration | `get_caret_style()` | No (Tier 3) |
| TextDecoration | `get_style_properties()` | No (Tier 3) |
| ListStyleType | `get_list_style_type()` | No (Tier 3) |
| ListStylePosition | `get_list_style_position()` | No (Tier 3) |
| BreakBefore | `get_break_before()` | No (Tier 3) |
| BreakAfter | `get_break_after()` | No (Tier 3) |
| BreakInside | `get_break_inside()` | No (Tier 3) |
| Orphans | `get_orphans()` | No (Tier 3) |
| Widows | `get_widows()` | No (Tier 3) |
| BoxDecorationBreak | `get_box_decoration_break()` | No (Tier 3) |
| ScrollbarStyle | `get_scrollbar_style()` | No (Tier 3) |
| ScrollbarWidth | `get_scrollbar_width_px()` | No (Tier 3) |
| ScrollbarColor | `get_scrollbar_style()` | No (Tier 3) |
| UserSelect | `is_text_selectable()` | No (Tier 3) |
| FontFamily | `collect_font_stacks_from_styled_dom()` | No (Tier 3) |

### Phase 5: cache.rs, hit_test.rs, callbacks.rs

| Property | File | New getter name |
|----------|------|----------------|
| CounterReset | cache.rs L2207 | `get_counter_reset()` |
| CounterIncrement | cache.rs L2229 | `get_counter_increment()` |
| Cursor | hit_test.rs L119 | `get_cursor()` (reuse from Phase 3) |
| Generic | callbacks.rs L2420 | Keep generic — this is public API |

### Phase 6: core/ files (gpu.rs, styled_dom.rs)

These are in `core/`, not `layout/`. They need to call getters too,  
but getters.rs is in `layout/`. Options:
1. **Move getters to core/** — complex, breaks layering
2. **Keep core/ accesses as-is** — they're cold-path (GPU compositing, devtools)
3. **Add a thin wrapper in core/** — duplicates code

**Decision: Keep core/ accesses as-is for now.** They are cold-path,  
and the eventual FxHash migration will handle them.

## Implementation Order

1. Create plan (this file) ✅
2. Add new getters to getters.rs for ALL missing properties
3. Migrate taffy_bridge.rs to use all getters
4. Migrate fc.rs to use all getters
5. Migrate display_list.rs to use all getters
6. Migrate cache.rs to use getters
7. Migrate hit_test.rs to use getters
8. `cargo check --package azul-layout`
9. Run tests
10. Commit
