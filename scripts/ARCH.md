# Architecture Review — Cross-Patch Analysis

## 1. Cross-Patch Contradictions

Based on the isolated nature of the agents, several patches attempt to implement the same spec paragraphs using conflicting approaches.

### Conflict 1: Table Anonymous Box Generation
- **Involved Specs**: `table-layout_001`, `table-layout_003`, `table-layout_021`
- **Involved Patches**: `table-layout_001`, `table-layout_003`, `table-layout_021`, `table-layout_024`
- **File/Function**: `layout/src/solver3/layout_tree.rs` (`process_table_children`, `process_table_row_children`)
- **Analysis**: CSS 2.2 § 17.2.1 defines three stages for generating missing table wrappers. Patch `table-layout_001` likely implements the Stage 2/3 wrappers (wrapping `TableCell` in `TableRow`), while `table-layout_003` handles Stage 1 (stripping whitespace-only text nodes). Because both modify the exact same DOM traversal loop in `process_table_children`, applying them sequentially will cause heavy merge conflicts and likely overwrite one stage's logic with the other's.
- **Resolution**: **Merge into a unified pass.** Use `table-layout_001` as the base for structural wrappers, but manually port the `should_skip_for_table_structure` (whitespace stripping) logic from `table-layout_003` into the same traversal loop. Discard `021` and `024` if they are redundant.

### Conflict 2: `word-break` and `line-break` Overlaps
- **Involved Specs**: `line-breaking_013`, `line-breaking_023`, `line-breaking_040`
- **Involved Patches**: `line-breaking_013`, `line-breaking_015`, `line-breaking_023`, `line-breaking_034`, `line-breaking_040`
- **File/Function**: `layout/src/text3/knuth_plass.rs`, `layout/src/text3/cache.rs`
- **Analysis**: Multiple agents attempted to add `word-break` and `line-break` properties to `UnifiedConstraints` and the shaping/breaking logic. Because they worked in isolation, they likely created duplicate enum variants (e.g., `WordBreak::BreakAll`, `LineBreak::Anywhere`) and conflicting match arms in `break_one_line`.
- **Resolution**: **Pick one base implementation.** Accept `line-breaking_040` (the largest, +108/-8 lines) as the canonical implementation for `word-break`. Accept `line-breaking_013` for `line-break`. Manually verify that `line-breaking_015` doesn't contain unique logic for `overflow-wrap: anywhere`, and if it does, adapt it into the accepted patches.

### Conflict 3: Absolute Positioning Constraints
- **Involved Specs**: `width-calculation_012`, `height-calculation_016`, `containing-block_028`, `containing-block_029`
- **Involved Patches**: `containing-block_028`, `containing-block_029`, `width-calculation_036`, `height-calculation_047`
- **File/Function**: `layout/src/solver3/positioning.rs` (`position_out_of_flow_elements`), `layout/src/solver3/sizing.rs`
- **Analysis**: Absolutely positioned elements require solving simultaneous equations for width (CSS 2.2 § 10.3.7) and height (§ 10.6.4). The agents generated distinct patches for the width axis and height axis. Because both sets of patches attempt to rewrite the fallback/auto-resolution logic for `position_out_of_flow_elements`, they will clash directly.
- **Resolution**: **Rewrite to use a shared abstraction.** Ensure that `resolve_position_offsets` is updated to handle both axes cleanly. Apply the width equations (`width-calculation_036`) first, resolve FFI/struct conflicts, and then apply the height equations (`height-calculation_047`) on top.

---

## 2. Tunnel Vision Gaps

Agents operating on single spec paragraphs naturally missed the broader architectural picture.

### Gap 1: Display Blockification (CSS Display Level 3 § 2.7)
- **Involved Specs**: `display-property_001`, `display-property_008`, `containing-block_043`
- **Involved Patches**: `display-property_001`, `containing-block_035`, `containing-block_043`, `containing-block_047`, `table-layout_028`
- **Context Missed**: Multiple paragraphs dictate that certain contexts "blockify" an element (e.g., being the root element, being absolutely positioned, or being a flex/grid item). Agents implemented these by scattering isolated `if` statements throughout `get_display_type` or `determine_formatting_context` in `layout_tree.rs`.
- **Architectural Fix**: These disparate rules represent a single concept: Computed vs. Specified `display`. The architecture needs a centralized `blockify_display(specified_display: LayoutDisplay) -> LayoutDisplay` function called inside `compute_layout_style`. The scattered patches must be consolidated into this single pipeline step to ensure correct cascade resolution.

### Gap 2: Float Clearance vs. Margin Collapsing
- **Involved Specs**: `floats_006`, `margin-collapsing_009` (CSS 2.2 § 9.5.2 & § 8.3.1)
- **Involved Patches**: `floats_006`, `margin-collapsing_039`
- **Context Missed**: A patch fixing clearance (`floats_006`) correctly pushes the `main_pen` down below a float. However, because the agent didn't see the margin collapsing spec (`margin-collapsing_009`), it missed the critical rule: *"Clearance inhibits margin collapsing"*.
- **Architectural Fix**: In `layout_bfc` (`fc.rs`), ensure that the clearance calculation flag explicitly disables the `collapse_margins(last_margin_bottom, child_margin_top)` logic for that specific sibling, forcing `last_margin_bottom = 0.0`.

### Gap 3: White Space Processing Pipeline
- **Involved Specs**: `white-space-processing_021` (Phase I), `white-space-processing_002` (Phase II)
- **Involved Patches**: `white-space-processing_024`, `white-space-processing_032`, `white-space-processing_030`
- **Context Missed**: CSS Text Level 3 defines a strict multi-phase pipeline for white space (Phase I: Collapsing & Transformation -> Phase II: Trimming & Positioning). Isolated patches for segment break transformation (`032`) and space collapsing (`024`) will step on each other's toes if they iterate over the string simultaneously or in the wrong order.
- **Architectural Fix**: Restructure `split_text_for_whitespace` in `fc.rs` to strictly enforce the Phase I -> Phase II order. Do not let patches arbitrarily mutate the text string without respecting this pipeline.

---

## 3. Architectural Changes Needed

Before merging the bulk of the codebase, the *patches themselves* must be grouped, ordered, and structurally modified.

### Merging and Abstraction
1. **Combine Table Border Logic**: Patches `box-model_012`, `box-model_018`, `box-model_019` implement the separated border model (CSS 2.2 § 17.6.1). They should be merged into a single PR that introduces the `TableBorderModel` structs, updates `get_border_spacing_property`, and applies it to `position_table_cells`.
2. **Abstract Intrinsic Sizing (`fit-content`)**: `intrinsic-sizing_009` (adds Enum), `012` (keyword), and `019` (function) implement `<length-percentage>` vs `fit-content()`. They need to be sequenced manually because they all touch the `AvailableSpace` enum and `calculate_used_size_for_node` match arms.

### Strict Ordering Constraints
Patches MUST be applied in this specific order to minimize conflicts and regressions:
1. **ANNOT Patches**: Apply all 427 `// +spec:` comment patches first. This establishes the traceability baseline and rarely causes conflicts.
2. **Data Structure Patches**: Apply patches that add enums/struct fields (e.g., `white-space-processing_028` adding `line-break` strictness, `intrinsic-sizing_009`).
3. **Phase-Dependent Logic**:
    - *Width/Height Calculation*: Apply base block logic (`width-calculation_001`), then inline-block (`032`), then floats (`050`), then abspos (`036`).
    - *Text Processing*: Apply Phase I white-space (`024`), then segment breaks (`032`), then Phase II (`030`), then line-breaking (`040`).

---

## 4. ABI and Regression Concerns

### 1. `#[repr(C)] LayoutNode` Padding and Cache Line Exhaustion
- **File**: `layout/src/solver3/layout_tree.rs`
- **Risk**: `LayoutNode` is explicitly marked `#[repr(C)]` and heavily optimized for CPU cache lines (Hot/Warm/Cold tiers). Patches like `box-model_024` or `line-breaking_015` might attempt to add new tracking fields (like `escaped_margins`, `ifc_membership`, or line break states) to the `LayoutNode`.
- **Action**: Any new fields added to `LayoutNode` must be carefully placed in the WARM or COLD tiers. Adding fields to the HOT tier (top 128-192 bytes) will cause severe layout performance regressions due to cache misses.

### 2. Destruction of the O(n) Two-Pass Cache
- **File**: `layout/src/solver3/cache.rs`
- **Risk**: Patches targeting width/height bugs might attempt to recursively call `calculate_layout_for_subtree` from within `layout_bfc` to get a child's dimensions. If an agent modified the 9+1 Taffy-inspired cache slots (`ComputeSize` vs `PerformLayout`) to fix a local sizing issue, it will destroy the O(n) complexity guarantee.
- **Action**: Reject any patches that bypass the `NodeCache::get_size` / `store_size` mechanisms. Intrinsic sizing fixes must happen in `calculate_intrinsic_sizes`, not by forcing synchronous full-layouts in Pass 1.

### 3. FFI / Taffy Bridge Hallucinations
- **File**: `layout/src/solver3/taffy_bridge.rs`
- **Risk**: Patches dealing with flex/grid containers (`intrinsic-sizing_027`, `margin-collapsing_046`) might invent Taffy API calls that don't exist (e.g., hallucinating `taffy::Style::max_width: auto()` handling). Taffy's flexbox bridge requires precise dimension mapping.
- **Action**: Verify that flex-basis and dimension assignments go through the existing `calc_storage` (`RefCell<Vec<Box<CalcResolveContext>>>`) mechanism. Do not allow patches to directly interpret `LayoutWidth::Calc` without heap-pinning, as this violates the unsafe FFI boundaries established with Taffy.

---

