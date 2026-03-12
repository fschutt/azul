# Architecture Review — Cross-Patch Analysis

Based on the provided layout engine source code and the AI agent patch review, here is the cross-patch analysis highlighting contradictions, architectural gaps, and required structural refactoring.

## 1. Cross-Patch Contradictions

These are cases where parallel agents attempted to implement overlapping spec requirements in ways that will collide or overwrite each other.

### A. Inline-Block Baseline Fallback
* **Involved Patches**: `box-model_14da32`, `inline-formatting-context_abe650`, `inline-formatting-context_b50bb4`
* **Spec Paragraph**: CSS 2.2 §10.8.1 (If an inline-block has `overflow` other than `visible`, its baseline is the bottom margin edge).
* **Conflict**: Three separate patches try to override the baseline calculation for `FormattingContext::InlineBlock` inside `collect_and_measure_inline_content_impl` (in `layout/src/solver3/fc.rs`). Because they execute independently, they create duplicate conditional branches or overwrite each other's `baseline_offset` assignments.
* **Resolution**: Merge into a single conditional check inside the `LayoutDisplay::InlineBlock` matching block in `collect_and_measure_inline_content_impl` (around line ~2070 in `fc.rs`), reading `overflow_x` and `overflow_y` from `ComputedLayoutStyle` to force the baseline to the bottom margin edge.

### B. Display Blockification
* **Involved Patches**: `display-property_0b40af`, `display-property_ba53ba`, `positioning_d06368+69468c`, `positioning_744713+00ce38+748d87`, `table-layout_360da0+cfc60a`
* **Spec Paragraphs**: CSS Display 3 §2.7, CSS Position 3, and CSS Floats (Absolute, fixed, floated, and root elements must blockify their display type).
* **Conflict**: Five agents generated overlapping `if`/`else` chains inside `LayoutTreeBuilder::process_node` (in `layout/src/solver3/layout_tree.rs` around line ~420). These patches will conflict during Git merging and cause logical errors due to evaluation order.
* **Resolution**: Delete all scattered blockification logic in `process_node`. Use the existing `get_computed_display` function in `layout/src/solver3/getters.rs` to compute the blockified display type *once* based on position, float, and root status before it assigns `node.computed_style.display`.

### C. `display: contents` on Replaced Elements
* **Involved Patches**: `display-contents_2f80e6...`, `display-contents_c03741...`, `replaced-elements_4f494d...`
* **Spec Paragraph**: CSS Display 3 (Replaced elements cannot be unboxed by `display: contents`; they must fall back to default rendering or `display: none`).
* **Conflict**: All three patches try to intercept `display: contents` in `layout_tree.rs` around line 483, generating duplicate `NodeType::Image` checks.
* **Resolution**: Keep a single combined implementation block inside `process_node`. Add a helper `is_replaced_element()` and fallback to `LayoutDisplay::None` when an element is both replaced and specifies `display: contents`.

### D. Upright LTR Override
* **Involved Patches**: `writing-modes_5d1fd1`, `writing-modes_8307e4`, `writing-modes_f3dd0e`
* **Spec Paragraph**: CSS Writing Modes 4 (When `text-orientation: upright` is active, the direction is forced to LTR).
* **Conflict**: Agents patched different layers. One patched `translate_to_text3_constraints` in `fc.rs`, while others patched `WritingModeContext::new` in `geometry.rs`.
* **Resolution**: Consolidate strictly inside `WritingModeContext` in `layout/src/solver3/geometry.rs`. This ensures that *all* layout math (including margin collapsing and alignment) sees the forced LTR direction, not just the text shaping engine.

## 2. Tunnel Vision Gaps

These are cases where agents implemented a narrow fix for their isolated paragraph, missing the broader architectural requirements.

### A. Scrollbar Gutter Reservation (`scrollbar-gutter`)
* **The Gap**: Agents (`overflow_3c44cc`, `overflow_7e8036`, `overflow_e90f12`) applied space reservation logic directly into `layout_bfc` (in `fc.rs`). However, Taffy-based flex/grid contexts and the display-list painter don't use `layout_bfc`. Reserving space there causes flex/grid layouts to overlap with scrollbars.
* **The Fix**: Move all reservation logic strictly into `compute_scrollbar_info_core` in `layout/src/solver3/cache.rs` (line ~450). Because `compute_taffy_scrollbar_info` and `layout_bfc` both utilize this central function, updating it guarantees that scrollbar reservation applies uniformly to BFC, Flex, and Grid contexts.

### B. `visibility: hidden` Edge Cases
* **The Gap**: Multiple patches (`overflow_66d5e6`, `overflow_960968`, etc.) injected isolated `get_visibility() == Hidden` checks all over `display_list.rs` to skip drawing borders, backgrounds, and scrollbars. However, visibility in CSS is *inherited*. A child of a `visibility: hidden` parent can set `visibility: visible` and must be painted. Scattered checks miss this inheritance logic.
* **The Fix**: Create a centralized `is_node_visible()` state derived during the layout tree generation or reconciliation phase, taking CSS inheritance into account, rather than locally checking the property at paint-time.

### C. `text-box-trim` Implementation
* **The Gap**: Patch `line-height_a5626f` tries to shift physical item positions (`position.y`) in `text3/cache.rs` to implement half-leading trimming. Shifting physical items breaks the engine's ability to map logical coordinates to hit-testing (selection, cursors).
* **The Fix**: Adopt the approach from patch `box-model_02e0f9`. Modify the container's bounds inside `layout_bfc` in `fc.rs` so the line box itself shrinks. The text layout coordinates remain untouched, preserving stable hit-testing while achieving the correct visual trim.

## 3. Architectural Changes Needed

### A. Unified Font Metrics Abstraction
Five patches (`font-metrics_1c2f21...` to `af3b19`) independently append `x_height` and `cap_height` to `LayoutFontMetrics` in `layout/src/text3/cache.rs` to support baseline alignments.
* **Refactor Required**:
  Rewrite `LayoutFontMetrics` to include `x_height: Option<f32>` and `cap_height: Option<f32>`. Update `LayoutFontMetrics::from_font_metrics` to parse these from the OS/2 table if available, and add fallback getter methods (e.g., `get_x_height_or_fallback()`) that default to `0.5em` when font data is missing.

### B. Consolidate `unicode-bidi: plaintext`
Patches `containing-block_55d224` and `writing-modes_0a5368` both target paragraph auto-direction.
* **Refactor Required**:
  Store the `unicode-bidi` state properly in `UnifiedConstraints`. In `layout/src/text3/cache.rs`, update `get_base_direction_from_logical()` to apply the P2/P3 heuristic dynamically *only* when the constraint dictates `plaintext`, overriding the inherited LTR/RTL base direction.

### C. Sticky Positioning Module
Patches `box-model_af9af8`, `overflow_bac4e5`, and `position-sticky_9449f1` inject complex scrollport clamping math directly into the main BFC pass or relative positioning loop.
* **Refactor Required**:
  In `layout/src/solver3/positioning.rs`, create a dedicated `adjust_sticky_positions` function to run immediately after `adjust_relative_positions`. Keep the scrollport boundary math isolated there to preserve the readability of the standard positioning pipeline.

## 4. ABI and Regression Concerns

* **`LayoutNode` struct memory layout**:
  `LayoutNode` in `layout/src/solver3/layout_tree.rs` is explicitly marked `#[repr(C)]` and heavily optimized for cache line utilization. Any patch that adds raw fields (like `clip_rect` for `overflow-clip-margin` or specific sticky offsets) must be scrutinized. New fields should be placed in the `COLD` or `WARM` tiers to avoid pushing hot fields out of the L1 cache. Ideally, these properties should remain in `ComputedLayoutStyle` rather than bloating the base `LayoutNode`.
* **Two-Pass Layout Thrashing**:
  The architecture relies heavily on Taffy's 9+1 cache slot system (`NodeCache` in `cache.rs`). Patches that attempt to resolve cyclic sizing constraints (like intrinsic ratios combined with `text-box-trim`) by forcing a third measurement pass will cause O(n²) performance regressions. Implementations must strictly respect the `ComputeMode::ComputeSize` vs `ComputeMode::PerformLayout` flow.
* **Fake/Lazy Implementations**:
  Patch `box-model_3393da+56c1d3.md.done.001.patch` claims to apply `shape-margin` but only drops a code comment `// +spec:box-model...` in `layout_initial_letter` without touching the geometry. This patch must be completely rejected and rewritten to actually expand the floating exclusion bounds returned by the function.