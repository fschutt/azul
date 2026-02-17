# Exhaustive CssPropertyCache Access Audit

Generated: 2026-02-17

## 1. Complete List of `pub fn get_*` Methods on CssPropertyCache

File: `core/src/prop_cache.rs`

| Line | Method | CSS Property |
|------|--------|-------------|
| 978 | `get_computed_css_style_string` | (debug: all properties) |
| 1372 | `get_text_color_or_default` | color (helper) |
| 1385 | `get_font_id_or_default` | font-family (helper) |
| 1404 | `get_font_size_or_default` | font-size (helper) |
| 1454 | `get_property` | (generic by CssPropertyType) |
| 1475 | `get_property_slow` | (generic slow path, private) |
| 1729 | `get_property_with_context` | (generic with context) |
| 1804 | `get_background_content` | background |
| 1820 | `get_hyphens` | hyphens |
| 1831 | `get_direction` | direction |
| 1842 | `get_white_space` | white-space |
| 1851 | `get_background_position` | background-position |
| 1865 | `get_background_size` | background-size |
| 1879 | `get_background_repeat` | background-repeat |
| 1893 | `get_font_size` | font-size |
| 1902 | `get_font_family` | font-family |
| 1911 | `get_font_weight` | font-weight |
| 1920 | `get_font_style` | font-style |
| 1929 | `get_text_color` | color |
| 1939 | `get_text_indent` | text-indent |
| 1949 | `get_initial_letter` | initial-letter |
| 1964 | `get_line_clamp` | line-clamp |
| 1974 | `get_hanging_punctuation` | hanging-punctuation |
| 1989 | `get_text_combine_upright` | text-combine-upright |
| 2004 | `get_exclusion_margin` | -azul-exclusion-margin |
| 2019 | `get_hyphenation_language` | -azul-hyphenation-language |
| 2034 | `get_caret_color` | caret-color |
| 2045 | `get_caret_width` | -azul-caret-width |
| 2056 | `get_caret_animation_duration` | -azul-caret-animation-duration |
| 2072 | `get_selection_background_color` | -azul-selection-background-color |
| 2088 | `get_selection_color` | -azul-selection-color |
| 2104 | `get_selection_radius` | -azul-selection-radius |
| 2120 | `get_text_justify` | text-justify |
| 2136 | `get_z_index` | z-index |
| 2147 | `get_flex_basis` | flex-basis |
| 2158 | `get_column_gap` | column-gap |
| 2169 | `get_row_gap` | row-gap |
| 2180 | `get_grid_template_columns` | grid-template-columns |
| 2196 | `get_grid_template_rows` | grid-template-rows |
| 2212 | `get_grid_auto_columns` | grid-auto-columns |
| 2228 | `get_grid_auto_rows` | grid-auto-rows |
| 2244 | `get_grid_column` | grid-column |
| 2255 | `get_grid_row` | grid-row |
| 2266 | `get_grid_auto_flow` | grid-auto-flow |
| 2282 | `get_justify_self` | justify-self |
| 2298 | `get_justify_items` | justify-items |
| 2314 | `get_gap` | gap |
| 2325 | `get_grid_gap` | grid-gap |
| 2336 | `get_align_self` | align-self |
| 2347 | `get_font` | font |
| 2358 | `get_writing_mode` | writing-mode |
| 2374 | `get_clear` | clear |
| 2385 | `get_shape_outside` | shape-outside |
| 2401 | `get_shape_inside` | shape-inside |
| 2417 | `get_clip_path` | clip-path |
| 2428 | `get_scrollbar_style` | -azul-scrollbar-style |
| 2439 | `get_scrollbar_width` | scrollbar-width |
| 2455 | `get_scrollbar_color` | scrollbar-color |
| 2471 | `get_visibility` | visibility |
| 2482 | `get_break_before` | break-before |
| 2498 | `get_break_after` | break-after |
| 2509 | `get_break_inside` | break-inside |
| 2525 | `get_orphans` | orphans |
| 2536 | `get_widows` | widows |
| 2547 | `get_box_decoration_break` | box-decoration-break |
| 2563 | `get_column_count` | column-count |
| 2579 | `get_column_width` | column-width |
| 2595 | `get_column_span` | column-span |
| 2606 | `get_column_fill` | column-fill |
| 2617 | `get_column_rule_width` | column-rule-width |
| 2633 | `get_column_rule_style` | column-rule-style |
| 2649 | `get_column_rule_color` | column-rule-color |
| 2665 | `get_flow_into` | flow-into |
| 2676 | `get_flow_from` | flow-from |
| 2687 | `get_shape_margin` | shape-margin |
| 2703 | `get_shape_image_threshold` | shape-image-threshold |
| 2719 | `get_content` | content |
| 2730 | `get_counter_reset` | counter-reset |
| 2746 | `get_counter_increment` | counter-increment |
| 2762 | `get_string_set` | string-set |
| 2771 | `get_text_align` | text-align |
| 2780 | `get_user_select` | user-select |
| 2789 | `get_text_decoration` | text-decoration |
| 2803 | `get_vertical_align` | vertical-align |
| 2817 | `get_line_height` | line-height |
| 2826 | `get_letter_spacing` | letter-spacing |
| 2840 | `get_word_spacing` | word-spacing |
| 2854 | `get_tab_size` | tab-size |
| 2863 | `get_cursor` | cursor |
| 2872 | `get_box_shadow_left` | -azul-box-shadow-left |
| 2886 | `get_box_shadow_right` | -azul-box-shadow-right |
| 2900 | `get_box_shadow_top` | -azul-box-shadow-top |
| 2914 | `get_box_shadow_bottom` | -azul-box-shadow-bottom |
| 2928 | `get_border_top_color` | border-top-color |
| 2942 | `get_border_left_color` | border-left-color |
| 2956 | `get_border_right_color` | border-right-color |
| 2970 | `get_border_bottom_color` | border-bottom-color |
| 2984 | `get_border_top_style` | border-top-style |
| 2998 | `get_border_left_style` | border-left-style |
| 3012 | `get_border_right_style` | border-right-style |
| 3026 | `get_border_bottom_style` | border-bottom-style |
| 3040 | `get_border_top_left_radius` | border-top-left-radius |
| 3054 | `get_border_top_right_radius` | border-top-right-radius |
| 3068 | `get_border_bottom_left_radius` | border-bottom-left-radius |
| 3082 | `get_border_bottom_right_radius` | border-bottom-right-radius |
| 3096 | `get_opacity` | opacity |
| 3105 | `get_transform` | transform |
| 3114 | `get_transform_origin` | transform-origin |
| 3128 | `get_perspective_origin` | perspective-origin |
| 3142 | `get_backface_visibility` | backface-visibility |
| 3156 | `get_display` | display |
| 3165 | `get_float` | float |
| 3174 | `get_box_sizing` | box-sizing |
| 3183 | `get_width` | width |
| 3192 | `get_height` | height |
| 3201 | `get_min_width` | min-width |
| 3210 | `get_min_height` | min-height |
| 3219 | `get_max_width` | max-width |
| 3228 | `get_max_height` | max-height |
| 3237 | `get_position` | position |
| 3246 | `get_top` | top |
| 3255 | `get_bottom` | bottom |
| 3264 | `get_right` | right |
| 3273 | `get_left` | left |
| 3282 | `get_padding_top` | padding-top |
| 3291 | `get_padding_bottom` | padding-bottom |
| 3305 | `get_padding_left` | padding-left |
| 3319 | `get_padding_right` | padding-right |
| 3333 | `get_margin_top` | margin-top |
| 3342 | `get_margin_bottom` | margin-bottom |
| 3356 | `get_margin_left` | margin-left |
| 3365 | `get_margin_right` | margin-right |
| 3379 | `get_border_top_width` | border-top-width |
| 3393 | `get_border_left_width` | border-left-width |
| 3407 | `get_border_right_width` | border-right-width |
| 3421 | `get_border_bottom_width` | border-bottom-width |
| 3435 | `get_overflow_x` | overflow-x |
| 3444 | `get_overflow_y` | overflow-y |
| 3453 | `get_flex_direction` | flex-direction |
| 3467 | `get_flex_wrap` | flex-wrap |
| 3476 | `get_flex_grow` | flex-grow |
| 3485 | `get_flex_shrink` | flex-shrink |
| 3494 | `get_justify_content` | justify-content |
| 3508 | `get_align_items` | align-items |
| 3517 | `get_align_content` | align-content |
| 3531 | `get_mix_blend_mode` | mix-blend-mode |
| 3545 | `get_filter` | filter |
| 3554 | `get_backdrop_filter` | backdrop-filter |
| 3563 | `get_text_shadow` | text-shadow |
| 3572 | `get_list_style_type` | list-style-type |
| 3586 | `get_list_style_position` | list-style-position |
| 3600 | `get_table_layout` | table-layout |
| 3614 | `get_border_collapse` | border-collapse |
| 3628 | `get_border_spacing` | border-spacing |
| 3642 | `get_caption_side` | caption-side |
| 3656 | `get_empty_cells` | empty-cells |

**Total: 113 getter methods** (including 3 helpers, 1 generic `get_property`, 1 `get_property_slow`, 1 `get_property_with_context`)

---

## 2. Call Sites by File

### 2.1 `layout/src/solver3/getters.rs` — **HOT PATH (layout solver)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 98 | `.get_font_size()` | font-size |
| 294 | compact_cache fast path | width/height/min/max (via raw i16/u32) |
| 419 | compact_cache fast path | padding (via raw u32) |
| 460 | compact_cache fast path | width/height/min/max (via raw u32) |
| 515 | compact_cache fast path | position offsets (via raw u32) |
| 1017 | `.get_border_top_left_radius()` | border-top-left-radius |
| 1025 | `.get_border_top_right_radius()` | border-top-right-radius |
| 1033 | `.get_border_bottom_right_radius()` | border-bottom-right-radius |
| 1041 | `.get_border_bottom_left_radius()` | border-bottom-left-radius |
| 1092-1116 | `.get_border_*_radius()` (x4) | border radii (second usage) |
| 1154 | `.get_z_index()` | z-index |
| 1197 | `.get_background_content()` | background |
| 1281 | `.get_background_content()` | background (child lookup) |
| 1341 | compact_cache fast path for borders | border widths/colors/styles |
| 1347-1354 | `.get_border_*_width()` (x4) | border-*-width (slow fallback) |
| 1370-1376 | `cc.get_border_*_color_raw()` (x4) | border-*-color (compact) |
| 1383-1392 | `cc.get_border_*_style()` (x4) | border-*-style (compact) |
| 1406-1423 | `.get_border_*_width()` (x4) | border-*-width (slow fallback) |
| 1430-1447 | `.get_border_*_color()` (x4) | border-*-color (slow fallback) |
| 1454-1471 | `.get_border_*_style()` (x4) | border-*-style (slow fallback) |
| 1603-1627 | `.get_border_*_radius()` (x4) | border-*-radius |
| 1732 | `.get_selection_background_color()` | -azul-selection-background-color |
| 1744 | `.get_selection_color()` | -azul-selection-color |
| 1752 | `.get_selection_radius()` | -azul-selection-radius |
| 1784 | `.get_caret_color()` | caret-color |
| 1797 | `.get_caret_width()` | -azul-caret-width |
| 1805 | `.get_caret_animation_duration()` | -azul-caret-animation-duration |
| 1918 | cache alias (`let cache = &styled_dom.css_property_cache.ptr`) | (setup) |
| 1924 | `.get_font_family()` | font-family |
| 1940 | `.get_font_size()` | font-size |
| 1973 | compact_cache: `cc.get_font_size_raw()` | font-size (compact) |
| 1990 | `.get_font_size()` | font-size (slow fallback) |
| 2005 | `cc.get_text_color_raw()` | color (compact) |
| 2019 | `.get_text_color()` | color (slow fallback) |
| 2037 | `cc.get_line_height()` | line-height (compact) |
| 2051 | `.get_line_height()` | line-height (slow fallback) |
| 2065 | `.get_display()` | display |
| 2200 | `cc.get_letter_spacing()` | letter-spacing (compact) |
| 2207 | `.get_letter_spacing()` | letter-spacing (slow fallback) |
| 2223 | `cc.get_word_spacing()` | word-spacing (compact) |
| 2230 | `.get_word_spacing()` | word-spacing (slow fallback) |
| 2242 | `.get_text_decoration()` | text-decoration |
| 2253 | `cc.get_tab_size_raw()` | tab-size (compact) |
| 2261 | `.get_tab_size()` | tab-size (slow fallback) |
| 2296 | `.get_list_style_type()` | list-style-type |
| 2313 | `.get_list_style_position()` | list-style-position |
| 2476 | `.get_break_before()` | break-before |
| 2491 | `.get_break_after()` | break-after |
| 2520 | `.get_break_inside()` | break-inside |
| 2535 | `.get_orphans()` | orphans |
| 2551 | `.get_widows()` | widows |
| 2570 | `.get_box_decoration_break()` | box-decoration-break |
| 2706 | cache alias (`let cache = &styled_dom.css_property_cache.ptr`) | (setup) |
| 2724 | `.get_font_family()` | font-family |
| 3196 | `.get_scrollbar_style()` | -azul-scrollbar-style |
| 3218 | `.get_scrollbar_width()` | scrollbar-width |
| 3233 | `.get_scrollbar_color()` | scrollbar-color |
| 3295 | `.get_user_select()` | user-select |

### 2.2 `layout/src/solver3/fc.rs` — **HOT PATH (flow/column layout)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 2538-2541 | `.get_height()` | height |
| 2577-2582 | `.get_shape_inside()` | shape-inside |
| 2612-2617 | `.get_shape_outside()` | shape-outside |
| 2642-2645 | `.get_text_justify()` | text-justify |
| 2653-2656 | `.get_line_height()` | line-height |
| 2660-2663 | `.get_hyphens()` | hyphens |
| 2706-2709 | `.get_text_indent()` | text-indent |
| 2726-2729 | `.get_column_count()` | column-count |
| 2738-2741 | `.get_column_gap()` | column-gap |
| 2774-2777 | `.get_initial_letter()` | initial-letter |
| 2793-2796 | `.get_line_clamp()` | line-clamp |
| 2801-2804 | `.get_hanging_punctuation()` | hanging-punctuation |
| 2810-2813 | `.get_text_combine_upright()` | text-combine-upright |
| 2822-2825 | `.get_exclusion_margin()` | -azul-exclusion-margin |
| 2831-2834 | `.get_hyphenation_language()` | -azul-hyphenation-language |
| 3135 | cache alias (`let cache = &ctx.styled_dom.css_property_cache.ptr`) | (setup) |
| 3138 | compact_cache check | (compact fast path entry) |
| 3142 | `cc.get_border_top_style()` | border-top-style (compact) |
| 3143 | `cc.get_border_right_style()` | border-right-style (compact) |
| 3144 | `cc.get_border_bottom_style()` | border-bottom-style (compact) |
| 3145 | `cc.get_border_left_style()` | border-left-style (compact) |
| 3161 | `cc.get_border_top_color_raw()` | border-top-color (compact) |
| 3162 | `cc.get_border_right_color_raw()` | border-right-color (compact) |
| 3163 | `cc.get_border_bottom_color_raw()` | border-bottom-color (compact) |
| 3164 | `cc.get_border_left_color_raw()` | border-left-color (compact) |
| 3175 | `cc.get_border_top_width_raw()` | border-top-width (compact) |
| 3176 | `cc.get_border_right_width_raw()` | border-right-width (compact) |
| 3177 | `cc.get_border_bottom_width_raw()` | border-bottom-width (compact) |
| 3178 | `cc.get_border_left_width_raw()` | border-left-width (compact) |
| 3193 | cache alias (slow fallback) | (setup) |
| 3213 | `.get_border_top_style()` | border-top-style (slow) |
| 3217 | `.get_border_top_width()` | border-top-width (slow) |
| 3225 | `.get_border_top_color()` | border-top-color (slow) |
| 3240 | `.get_border_right_style()` | border-right-style (slow) |
| 3244 | `.get_border_right_width()` | border-right-width (slow) |
| 3252 | `.get_border_right_color()` | border-right-color (slow) |
| 3267 | `.get_border_bottom_style()` | border-bottom-style (slow) |
| 3271 | `.get_border_bottom_width()` | border-bottom-width (slow) |
| 3279 | `.get_border_bottom_color()` | border-bottom-color (slow) |
| 3294 | `.get_border_left_style()` | border-left-style (slow) |
| 3298 | `.get_border_left_width()` | border-left-width (slow) |
| 3306 | `.get_border_left_color()` | border-left-color (slow) |
| 3335 | `.get_table_layout()` (compact fast path) | table-layout |
| 3337 | `.get_table_layout()` | table-layout (slow) |
| 3352-3353 | `cc.get_border_collapse()` | border-collapse (compact) |
| 3360-3363 | `.get_border_collapse()` | border-collapse (slow) |
| 3374-3377 | `cc.get_border_spacing_h_raw()` / `cc.get_border_spacing_v_raw()` | border-spacing (compact) |
| 3393 | `.get_border_spacing()` | border-spacing (slow fallback) |
| 3425-3429 | `.get_caption_side()` | caption-side |

### 2.3 `layout/src/solver3/taffy_bridge.rs` — **HOT PATH (taffy layout bridge)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 439 | cache alias (`let cache = &styled_dom.css_property_cache.ptr`) | (setup) |
| 550 | `.get_property(..., Gap)` | gap |
| 571-575 | `.get_property(..., GridTemplateRows)` | grid-template-rows |
| 589-593 | `.get_property(..., GridTemplateColumns)` | grid-template-columns |
| 607-611 | `.get_property(..., GridTemplateAreas)` | grid-template-areas |
| 638 | `.get_property(..., GridAutoRows)` | grid-auto-rows |
| 650-654 | `.get_property(..., GridAutoColumns)` | grid-auto-columns |
| 667 | `.get_property(..., GridAutoFlow)` | grid-auto-flow |
| 680 | `.get_property(..., GridColumn)` | grid-column |
| 693 | `.get_property(..., GridRow)` | grid-row |
| 711 | `.get_property(..., FlexWrap)` | flex-wrap |
| 729 | `.get_property(..., JustifyItems)` | justify-items |
| 739 | `.get_property(..., JustifyContent)` | justify-content |
| 751 | `.get_property(..., FlexGrow)` | flex-grow |
| 763 | `.get_property(..., FlexShrink)` | flex-shrink |
| 773 | `.get_property(..., FlexBasis)` | flex-basis |
| 798 | `.get_property(..., AlignSelf)` | align-self |
| 807 | `.get_property(..., JustifySelf)` | justify-self |

### 2.4 `layout/src/solver3/cache.rs` — **HOT PATH (layout cache)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 2192 | cache alias (`let cache = &styled_dom.css_property_cache.ptr`) | (setup) |
| 2207 | `.get_counter_reset()` | counter-reset |
| 2227 | `.get_counter_increment()` | counter-increment |

### 2.5 `layout/src/solver3/display_list.rs` — **COLD PATH (display list generation)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 1363 | `.get_cursor()` | cursor |
| 1730-1732 | `.get_opacity()` | opacity |
| 1745-1748 | `.get_filter()` | filter |
| 1761-1764 | `.get_backdrop_filter()` | backdrop-filter |
| 2408 | `CssPropertyCache::get_box_shadow_left` (fn ptr) | box-shadow-left |
| 2409 | `CssPropertyCache::get_box_shadow_right` (fn ptr) | box-shadow-right |
| 2410 | `CssPropertyCache::get_box_shadow_top` (fn ptr) | box-shadow-top |
| 2411 | `CssPropertyCache::get_box_shadow_bottom` (fn ptr) | box-shadow-bottom |
| 2414 | `&self.ctx.styled_dom.css_property_cache.ptr` | (passed to box shadow fn) |
| 2711-2714 | `.get_text_shadow()` | text-shadow |
| 3415-3418 | `.get_opacity()` | opacity |
| 3430-3433 | `.get_transform()` | transform |

### 2.6 `layout/src/hit_test.rs` — **COLD PATH (hit testing)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 119 | `.get_cursor()` | cursor |

### 2.7 `layout/src/callbacks.rs` — **COLD PATH (callbacks/restyle)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 2419-2421 | `.get_property(..., property_type)` | (generic — dynamic property) |

### 2.8 `core/src/styled_dom.rs` — **COLD PATH (styling/restyle/debug)**

| Line | Method Called | Property Accessed | Context |
|------|-------------|-------------------|---------|
| 817 | `CssPropertyCache::empty(1)` | — | construction |
| 863 | `CssPropertyCache::empty(...)` | — | construction |
| 883 | `.restyle()` | — | restyling |
| 894 | `.apply_ua_css()` | — | UA stylesheet |
| 899 | `.compute_inherited_values()` | — | inheritance |
| 918 | `.build_compact_cache()` | — | compact cache build |
| 996 | `CssPropertyCachePtr::new(...)` | — | wrapping |
| 1074-1075 | `.get_css_property_cache_mut().append()` | — | DOM merging |
| 1170-1171 | `.get_css_property_cache_mut().append()` | — | DOM merging |
| 1252 | `.restyle()` | — | restyling |
| 1261-1266 | `.css_property_cache` | — | inherited values / UA css |
| 1284-1285 | `.get_css_property_cache()` | — | accessor |
| 1290-1291 | `.get_css_property_cache_mut()` | — | mut accessor |
| 1333 | `.get_background_content()` | background | image scanning |
| 1405 | `.get_css_property_cache()` | — | diff_normal_properties |
| 1416-1422 | `css_property_cache.get_keys_normal/inherited()` | — | property key enumeration |
| 1463 | `.get_property(...)` | (generic) | diff old |
| 1469 | `.get_property(...)` | (generic) | diff new |
| 1528 | `.get_css_property_cache()` | — | diff_hover_properties |
| 1588 | `.get_property(...)` | (generic) | diff old hover |
| 1594 | `.get_property(...)` | (generic) | diff new hover |
| 1655 | `.get_css_property_cache()` | — | diff_active_properties |
| 1718 | `.get_property(...)` | (generic) | diff old active |
| 1724 | `.get_property(...)` | (generic) | diff new active |
| 1894-1899 | `.get_property(...)` | (generic) | single_set_css_property |
| 1926 | `.get_css_property_cache_mut()` | — | single_set_css_property write |
| 1995 | `.get_css_property_cache()` | — | debug printing |
| 2053 | (passed to `debug_print_start`) | — | debug printing |
| 2138 | `.get_css_property_cache()` | — | debug introspection |
| 2149-2161 | `css_property_cache: &CssPropertyCache` (param) | — | debug helpers |
| 2345-2352 | `.get_position()` (via param) | position | debug |

### 2.9 `core/src/gpu.rs` — **COLD PATH (GPU transforms)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 105 | `.get_css_property_cache()` | — |
| 136-137 | `.get_transform()` | transform |
| 144 | `.get_transform_origin()` | transform-origin |
| 211 | `.get_opacity()` | opacity |

### 2.10 `core/src/dom_table.rs` — **COLD PATH (table detection)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 97 | `.get_css_property_cache()` | — |
| 102 | `.get_display()` | display |

### 2.11 `core/src/resources.rs` — **COLD PATH (font resolution)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 1116 | `css_property_cache: &CssPropertyCache` (param) | — |
| 1137 | `.get_font_id_or_default()` | font-family |

### 2.12 `core/src/icon.rs` — **COLD PATH (icon construction)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 441 | `CssPropertyCache::empty(1)` | — | construction only |

### 2.13 `core/src/prop_cache.rs` — **INTERNAL (restyle/tag_id generation)**

| Line | Method Called | Property Accessed | Context |
|------|-------------|-------------------|---------|
| 762 | `.get_display()` | display | restyle (tag_id) |
| 881 | `.get_cursor()` | cursor | restyle (tag_id) |
| 895 | `.get_overflow_x()` | overflow-x | restyle (tag_id, scroll) |
| 898 | `.get_overflow_y()` | overflow-y | restyle (tag_id, scroll) |
| 947 | `.get_user_select()` | user-select | restyle (tag_id) |
| 985-1219 | `self.get_*()` (56 calls) | all visual properties | `get_computed_css_style_string` (debug only) |
| 1330 | `self.get_overflow_x()` | overflow-x | `has_overflow_x_hidden` |
| 1342 | `self.get_overflow_y()` | overflow-y | `has_overflow_y_hidden` |
| 1354 | `self.get_overflow_x()` | overflow-x | `has_overflow_x_scroll` |
| 1366 | `self.get_overflow_y()` | overflow-y | `has_overflow_y_scroll` |
| 1379 | `self.get_text_color()` | color | `get_text_color_or_default` |
| 1396 | `self.get_font_family()` | font-family | `get_font_id_or_default` |
| 1411 | `self.get_font_size()` | font-size | `get_font_size_or_default` |
| 1422-1431 | `self.get_border_*_width()` (x4) | border widths | `get_has_border` |
| 1441-1450 | `self.get_box_shadow_*()` (x4) | box shadows | `has_box_shadow` |
| 1470 | `self.get_property_slow()` | (generic) | `get_property` dispatch |
| 3674 | `self.get_width()` | width | `resolve_width_prop` |
| 3692 | `self.get_min_width()` | min-width | `resolve_min_width_prop` |
| 3709 | `self.get_max_width()` | max-width | `resolve_max_width_prop` |
| 3726 | `self.get_height()` | height | `resolve_height_prop` |
| 3744 | `self.get_min_height()` | min-height | `resolve_min_height_prop` |
| 3761 | `self.get_max_height()` | max-height | `resolve_max_height_prop` |
| 3778 | `self.get_left()` | left | `resolve_left_prop` |
| 3794 | `self.get_right()` | right | `resolve_right_prop` |
| 3810 | `self.get_top()` | top | `resolve_top_prop` |
| 3826 | `self.get_bottom()` | bottom | `resolve_bottom_prop` |
| 3843 | `self.get_border_left_width()` | border-left-width | `resolve_border_left_prop` |
| 3860 | `self.get_border_right_width()` | border-right-width | `resolve_border_right_prop` |
| 3877 | `self.get_border_top_width()` | border-top-width | `resolve_border_top_prop` |
| 3894 | `self.get_border_bottom_width()` | border-bottom-width | `resolve_border_bottom_prop` |
| 3912 | `self.get_padding_left()` | padding-left | `resolve_padding_left_prop` |
| 3929 | `self.get_padding_right()` | padding-right | `resolve_padding_right_prop` |
| 3946 | `self.get_padding_top()` | padding-top | `resolve_padding_top_prop` |
| 3963 | `self.get_padding_bottom()` | padding-bottom | `resolve_padding_bottom_prop` |
| 3981 | `self.get_margin_left()` | margin-left | `resolve_margin_left_prop` |
| 3998 | `self.get_margin_right()` | margin-right | `resolve_margin_right_prop` |
| 4015 | `self.get_margin_top()` | margin-top | `resolve_margin_top_prop` |
| 4032 | `self.get_margin_bottom()` | margin-bottom | `resolve_margin_bottom_prop` |
| 5005 | `self.get_property_slow()` | (generic) | testing/internal |
| 5084 | `self.get_property_slow()` | (generic) | testing/internal |

### 2.14 `core/src/compact_cache_builder.rs` — **ONE-TIME (compact cache build)**

| Line | Method Called | Property Accessed |
|------|-------------|-------------------|
| 42 | `.get_display()` | display |
| 46 | `.get_position()` | position |
| 50 | `.get_float()` | float |
| 54 | `.get_overflow_x()` | overflow-x |
| 58 | `.get_overflow_y()` | overflow-y |
| 62 | `.get_box_sizing()` | box-sizing |
| 66 | `.get_flex_direction()` | flex-direction |
| 70 | `.get_flex_wrap()` | flex-wrap |
| 74 | `.get_justify_content()` | justify-content |
| 78 | `.get_align_items()` | align-items |
| 82 | `.get_align_content()` | align-content |
| 86 | `.get_writing_mode()` | writing-mode |
| 90 | `.get_clear()` | clear |
| 94 | `.get_font_weight()` | font-weight |
| 98 | `.get_font_style()` | font-style |
| 102 | `.get_text_align()` | text-align |
| 106 | `.get_visibility()` | visibility |
| 110 | `.get_white_space()` | white-space |
| 114 | `.get_direction()` | direction |
| 118 | `.get_vertical_align()` | vertical-align |
| 123 | `.get_border_collapse()` | border-collapse |
| 156 | `.get_width()` | width |
| 159 | `.get_height()` | height |
| 164 | `.get_min_width()` | min-width |
| 167 | `.get_max_width()` | max-width |
| 170 | `.get_min_height()` | min-height |
| 173 | `.get_max_height()` | max-height |
| 178 | `.get_flex_basis()` | flex-basis |
| 183 | `.get_font_size()` | font-size |
| 188 | `.get_padding_top()` | padding-top |
| 191 | `.get_padding_right()` | padding-right |
| 194 | `.get_padding_bottom()` | padding-bottom |
| 197 | `.get_padding_left()` | padding-left |
| 202 | `.get_margin_top()` | margin-top |
| 205 | `.get_margin_right()` | margin-right |
| 208 | `.get_margin_bottom()` | margin-bottom |
| 211 | `.get_margin_left()` | margin-left |
| 216 | `.get_border_top_width()` | border-top-width |
| 219 | `.get_border_right_width()` | border-right-width |
| 222 | `.get_border_bottom_width()` | border-bottom-width |
| 225 | `.get_border_left_width()` | border-left-width |
| 230 | `.get_top()` | top |
| 233 | `.get_right()` | right |
| 236 | `.get_bottom()` | bottom |
| 239 | `.get_left()` | left |
| 244 | `.get_flex_grow()` | flex-grow |
| 249 | `.get_flex_shrink()` | flex-shrink |
| 256 | `.get_z_index()` | z-index |
| 273 | `.get_border_top_style()` | border-top-style |
| 277 | `.get_border_right_style()` | border-right-style |
| 281 | `.get_border_bottom_style()` | border-bottom-style |
| 285 | `.get_border_left_style()` | border-left-style |
| 294 | `.get_border_top_color()` | border-top-color |
| 299 | `.get_border_right_color()` | border-right-color |
| 304 | `.get_border_bottom_color()` | border-bottom-color |
| 309 | `.get_border_left_color()` | border-left-color |
| 316 | `.get_border_spacing()` | border-spacing |
| 328 | `.get_tab_size()` | tab-size |
| 337 | `.get_text_color()` | color |
| 346 | `.get_font_family()` | font-family |
| 357 | `.get_line_height()` | line-height |
| 372 | `.get_letter_spacing()` | letter-spacing |
| 377 | `.get_word_spacing()` | word-spacing |
| 382 | `.get_text_indent()` | text-indent |

### 2.15 Tests

| File | Lines | Context |
|------|-------|---------|
| `core/tests/css_inheritance.rs` | 31 | `styled_dom.css_property_cache.ptr.clone()` |
| `core/tests/prop_cache.rs` | 25, 33, 87, 113, 146, 195, 238, 283, 304, 377, 419, 450, 505 | `css_property_cache.ptr.clone()`, `.get_property()` |
| `layout/tests/test_font_family_parsing.rs` | 34, 102, 145 | `&styled_dom.css_property_cache.ptr` |
| `layout/tests/test_html_body_selector.rs` | 30, 78, 91 | `&styled_dom.css_property_cache.ptr` |
| `tests/src/layout.rs` | 290, 1356 | `css_property_cache = CssPropertyCachePtr::new(...)` |

### 2.16 `dll/src/` — **NO REFERENCES**

No direct CssPropertyCache references found in the DLL FFI binding layer.

---

## 3. Summary Statistics

| Location | Hot/Cold | # Direct Getter Calls | # `get_property()` (generic) Calls | # Compact Cache (cc.) Calls |
|----------|---------|----------------------|-----------------------------------|----------------------------|
| `layout/src/solver3/getters.rs` | **HOT** | ~50 | 0 | ~20 |
| `layout/src/solver3/fc.rs` | **HOT** | ~30 | 0 | ~18 |
| `layout/src/solver3/taffy_bridge.rs` | **HOT** | 0 | ~17 | 0 |
| `layout/src/solver3/cache.rs` | **HOT** | 2 | 0 | 0 |
| `layout/src/solver3/display_list.rs` | COLD | 7 | 0 | 0 |
| `layout/src/hit_test.rs` | COLD | 1 | 0 | 0 |
| `layout/src/callbacks.rs` | COLD | 0 | 1 | 0 |
| `core/src/styled_dom.rs` | COLD | 1 | ~10 | 0 |
| `core/src/gpu.rs` | COLD | 3 | 0 | 0 |
| `core/src/dom_table.rs` | COLD | 1 | 0 | 0 |
| `core/src/resources.rs` | COLD | 1 | 0 | 0 |
| `core/src/icon.rs` | COLD | 0 | 0 | 0 |
| `core/src/prop_cache.rs` (internal) | MIXED | ~80+ | ~5 | 0 |
| `core/src/compact_cache_builder.rs` | ONE-TIME | ~60 | 0 | 0 |

### Hot Path Properties (most critical to optimize):

Properties accessed in the **layout solver hot loop** (`getters.rs` + `fc.rs` + `taffy_bridge.rs`):

| Property | # Call Sites in Hot Path | Already in CompactCache? |
|----------|------------------------|--------------------------|
| width | ~3 (getters via macro) | YES (u32) |
| height | ~3 (getters + fc) | YES (u32) |
| min-width | ~2 | YES (u32) |
| max-width | ~2 | YES (u32) |
| min-height | ~2 | YES (u32) |
| max-height | ~2 | YES (u32) |
| padding-top/right/bottom/left | ~4 (via macro) | YES (u32) |
| margin-top/right/bottom/left | ~4 (via macro) | YES (u32) |
| border-*-width (x4) | ~8 (getters+fc) | YES (i16) |
| border-*-style (x4) | ~8 (getters+fc) | YES (u8 enum) |
| border-*-color (x4) | ~8 (getters+fc) | YES (i16 raw) |
| display | ~2 | YES (enum in tier1) |
| position | ~1 (via macro) | YES (enum in tier1) |
| flex-grow | ~2 (taffy+builder) | YES (i16) |
| flex-shrink | ~2 (taffy+builder) | YES (i16) |
| flex-wrap | ~1 (taffy) | YES (enum in tier1) |
| flex-basis | ~2 | YES (u32) |
| flex-direction | ~1 | YES (enum in tier1) |
| justify-content | ~2 (taffy) | YES (enum in tier1) |
| align-items | ~1 (taffy) | YES (enum in tier1) |
| align-content | ~1 (taffy) | YES (enum in tier1) |
| overflow-x/y | ~2 | YES (enum in tier1) |
| top/right/bottom/left | ~4 | YES (u32) |
| z-index | ~2 | YES (i16) |
| font-size | ~3 (getters) | YES (u32) |
| font-family | ~2 (getters) | YES (u16 index) |
| text-color (color) | ~2 (getters) | YES (i16 raw) |
| line-height | ~2 (getters+fc) | YES (u16 normalized) |
| letter-spacing | ~2 (getters) | YES (i16) |
| word-spacing | ~2 (getters) | YES (i16) |
| tab-size | ~2 (getters) | YES (i16) |
| text-indent | ~2 (fc) | NO |
| border-*-radius (x4) | ~12 (getters) | NO |
| background | ~3 (getters) | NO |
| border-collapse | ~2 (fc) | YES (enum in tier1) |
| border-spacing | ~2 (fc) | YES (i16 pair) |
| table-layout | ~1 (fc) | NO (slow path only) |
| caption-side | ~1 (fc) | NO |
| gap | ~1 (taffy) | NO |
| grid-template-rows | ~1 (taffy) | NO |
| grid-template-columns | ~1 (taffy) | NO |
| grid-auto-flow | ~1 (taffy) | NO |
| grid-column | ~1 (taffy) | NO |
| grid-row | ~1 (taffy) | NO |
| shape-inside | ~1 (fc) | NO |
| shape-outside | ~1 (fc) | NO |
| hyphens | ~1 (fc) | NO |
| text-justify | ~1 (fc) | NO |
| column-count | ~1 (fc) | NO |
| column-gap | ~1 (fc) | NO |
| initial-letter | ~1 (fc) | NO |
| line-clamp | ~1 (fc) | NO |
| hanging-punctuation | ~1 (fc) | NO |
| text-combine-upright | ~1 (fc) | NO |
| exclusion-margin | ~1 (fc) | NO |
| hyphenation-language | ~1 (fc) | NO |
| break-before/after/inside | ~3 (getters) | NO |
| orphans/widows | ~2 (getters) | NO |
| list-style-type | ~1 (getters) | NO |
| list-style-position | ~1 (getters) | NO |
| counter-reset | ~1 (cache.rs) | NO |
| counter-increment | ~1 (cache.rs) | NO |
| text-decoration | ~1 (getters) | NO |
| user-select | ~1 (getters) | NO |
| scrollbar-style/width/color | ~3 (getters) | NO |

### Cold Path Properties (display_list, gpu, hit_test, callbacks):

| Property | File | # Calls |
|----------|------|---------|
| cursor | display_list:1363, hit_test:119 | 2 |
| opacity | display_list:1731+3417, gpu:211 | 3 |
| transform | display_list:3432, gpu:137 | 2 |
| transform-origin | gpu:144 | 1 |
| filter | display_list:1746 | 1 |
| backdrop-filter | display_list:1762 | 1 |
| box-shadow-* (x4) | display_list:2408-2411 | 4 |
| text-shadow | display_list:2712 | 1 |
| font-family | resources:1137 | 1 |
