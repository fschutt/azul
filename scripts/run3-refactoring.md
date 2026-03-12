# Refactoring Groundwork Plan (GROUNDWORK.md)

Based on the cross-patch analysis and architectural review, the following groundwork refactoring must be completed **before** applying any of the AI agent patches. This will prevent merge conflicts, ensure correct abstraction boundaries, and fix overlapping logic.

## 1. Unified Display Blockification Helper
**What**: Clean up `LayoutTreeBuilder::process_node` by replacing scattered `if/else` blocks with a single call to `get_computed_display()`.
**Why**: 5 different patches attempt to inject blockification logic for absolute, fixed, floated, or root elements directly into `process_node`, leading to messy overlapping control flow.
**Where**: `layout/src/solver3/layout_tree.rs` inside `LayoutTreeBuilder::process_node`. Use the existing `get_computed_display` from `layout/src/solver3/getters.rs` to compute the display type *once* before assigning `node.computed_style.display`.
**Needed for patches**: `display-property_0b40af`, `display-property_ba53ba`, `positioning_d06368+69468c`, `positioning_744713+00ce38+748d87`, `table-layout_360da0+cfc60a`

## 2. Replaced Elements `display: contents` Fallback
**What**: Combine the `is_replaced_element()` check and `display: contents` override into a single, clean structural block.
**Why**: Multiple patches try to prevent `display: contents` from unboxing replaced elements (like images). Doing this independently leads to duplicate `NodeType::Image` checks.
**Where**: `layout/src/solver3/layout_tree.rs` inside `process_node`. Intercept early and fall back to `LayoutDisplay::None`.
**Needed for patches**: `display-contents_2f80e6`, `display-contents_c03741`, `replaced-elements_4f494d`

## 3. Inline-Block Baseline Fallback
**What**: Add a single unified check for `overflow != visible` on `LayoutDisplay::InlineBlock` to force the baseline to the bottom margin edge.
**Why**: Three patches independently try to implement CSS 2.2 §10.8.1 by patching the same conditional branch.
**Where**: `layout/src/solver3/fc.rs` inside `collect_and_measure_inline_content_impl`, specifically within the `LayoutDisplay::InlineBlock` matching block.
**Needed for patches**: `box-model_14da32`, `inline-formatting-context_abe650`, `inline-formatting-context_b50bb4`

## 4. Centralize Upright LTR Override
**What**: Force `direction` to `Ltr` when `text-orientation: upright` is active, strictly at the context boundary.
**Why**: CSS Writing Modes 4 requires this, but patches were applying it in varying layers (text shaping vs context init), which breaks margin collapsing and alignment that also need to see the forced direction.
**Where**: `layout/src/solver3/geometry.rs` inside the `WritingModeContext` initialization.
**Needed for patches**: `writing-modes_5d1fd1`, `writing-modes_8307e4`, `writing-modes_f3dd0e`

## 5. Unified Font Metrics Abstraction (`x-height` & `cap-height`)
**What**: Add `x_height: Option<f32>` and `cap_height: Option<f32>` to `LayoutFontMetrics`, and add fallback getters (e.g., `get_x_height_or_fallback()`) returning `0.5em` when missing.
**Why**: Five patches independently add these fields and duplicate the `0.5em` fallback logic for baseline alignments.
**Where**: `layout/src/text3/cache.rs` inside the `LayoutFontMetrics` struct and `from_font_metrics` function.
**Needed for patches**: `font-metrics_1c2f21`, `font-metrics_3f5dc0`, `font-metrics_484108`, `font-metrics_6476dd`, `font-metrics_af3b19`

## 6. Scrollbar Gutter Reservation Architecture
**What**: Move all `scrollbar-gutter` space reservation logic purely into `compute_scrollbar_info_core`.
**Why**: Patches attempted to reserve space directly inside `layout_bfc`. Because Taffy-based flex/grid contexts don't use `layout_bfc`, scrollbars would overlap flex content. Centralizing it applies the reservation uniformly.
**Where**: `layout/src/solver3/cache.rs` inside `compute_scrollbar_info_core`.
**Needed for patches**: `overflow_3c44cc`, `overflow_7e8036`, `overflow_e90f12`

## 7. Dedicated Sticky Positioning Module
**What**: Create a dedicated `adjust_sticky_positions` function for scrollport clamping math.
**Why**: Sticky positioning requires complex boundary mathematics. Patches inject this directly into relative positioning loops or the BFC pass, destroying readability. 
**Where**: `layout/src/solver3/positioning.rs`. Create `adjust_sticky_positions` and call it sequentially after `adjust_relative_positions`.
**Needed for patches**: `box-model_af9af8`, `overflow_bac4e5`, `position-sticky_9449f1`

## 8. Overflow Clip Margin Helper
**What**: Extract the `clip_rect` expansion mathematics into a small, standalone helper function.
**Why**: Five patches implement `overflow-clip-margin` independently by modifying dense inline calculations.
**Where**: `layout/src/solver3/display_list.rs` around the `clip_rect` assignments.
**Needed for patches**: `box-model_6180a2`, `containing-block_9d73a7`, `overflow_342f47`, `overflow_8f5473`, `overflow_e6334e`

## 9. Centralized `is_node_visible()` Helper
**What**: Create a single `is_node_visible()` state/helper that takes CSS `visibility` inheritance into account.
**Why**: Multiple patches try to suppress drawing borders/backgrounds by injecting local `get_visibility() == Hidden` checks. This is incorrect because visibility inherits.
**Where**: `layout/src/solver3/getters.rs` or derived in the `LayoutNode` state.
**Needed for patches**: `overflow_66d5e6`, `overflow_960968`, `overflow_ea10a3`, `overflow_efe07e`

## 10. `unicode-bidi: plaintext` Auto-Direction
**What**: Add `unicode-bidi` state to `UnifiedConstraints`, and update the paragraph auto-direction heuristics to trigger *only* when the constraint dictates `plaintext`.
**Why**: Duplicate attempts to implement P2/P3 heuristics clobber each other. 
**Where**: `layout/src/text3/cache.rs` inside `get_base_direction_from_logical`.
**Needed for patches**: `containing-block_55d224`, `writing-modes_0a5368`

## 11. Extract Hanging Punctuation Logic
**What**: Extract the large `match` block of Unicode commas and stops into a single, reusable `is_hanging_punctuation_char(c: char) -> bool` function.
**Why**: Three patches copy-paste the exact same massive match block.
**Where**: `layout/src/text3/cache.rs`.
**Needed for patches**: `containing-block_5166be`, `text-alignment-spacing_1d988c`, `writing-modes_6f16cc`

## 12. Resolve Visible/Clip Overflow Method
**What**: Keep the method approach (`is_visible_or_clip`) on the `LayoutOverflow` enum to resolve CSS Overflow 3 §3.1 rules (if one axis scrolls, visible -> auto, clip -> hidden). Delete standalone getter implementations.
**Why**: Conflicts between object-oriented and functional implementations of the same rule.
**Where**: `layout/src/solver3/getters.rs` (on the `MultiValue<LayoutOverflow>` block).
**Needed for patches**: `overflow_8935f0`, `overflow_c4b9fe`

## 13. `text-box-trim` Architecture Standard
**What**: Prepare the architecture to modify container bounds rather than shifting physical item positions. Add a `text_box_trim_bounds` hook or local variable setup.
**Why**: Trying to shift physical text item positions breaks hit-testing and cursor mapping. Shrinking the bounding box in `layout_bfc` is the correct approach.
**Where**: `layout/src/solver3/fc.rs` inside `layout_bfc`.
**Needed for patches**: `box-model_02e0f9`, `line-height_a5626f`

## 14. Fixed Position Pagination State
**What**: Use the `fixed_position_item_ranges` data structure for tracking `position: fixed` elements across pages in paged media.
**Why**: Two patches conflict by tracking single indices vs. ranges. Ranges are required for proper multi-element support.
**Where**: `layout/src/solver3/display_list.rs` (DisplayListBuilder state) and potentially `paged_layout.rs`.
**Needed for patches**: `overflow_ab9999`, `positioning_4c5432`