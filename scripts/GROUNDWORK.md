# Architecture Groundwork Outline

Before applying the 800 patches (especially the complex merge clusters), several structural helpers and abstractions must be introduced to the codebase. Implementing these first ensures that incoming patches plug into a unified architecture rather than scattering ad-hoc logic across the layout engine.

## 1. Unified CSS Property Resolution (The Cascade Bridge)

**What:** 
Expand the `getters.rs` module with standardized accessors and enum converters for newly introduced CSS properties. This includes macro-generated getters for properties like `word-break`, `line-break`, `hyphens`, `empty-cells`, and `text-indent`. It also requires mapping functions to convert `azul_css` enums into `text3::cache` internal enums.

**Why:** 
Currently, `fc.rs` manually reads and maps CSS properties when building `UnifiedConstraints`. As patches introduce dozens of new text-formatting properties, appending them directly into `translate_to_text3_constraints` will create a massive, unmaintainable bottleneck. Establishing the getters and type-converters first ensures patches only need to add one-liners to the constraint builder.

**Needed for patches:**
*   `line-breaking_*` (especially `008`, `013`, `040` for word/line-break, and `038`, `039` for text-indent)
*   `white-space-processing_028`, `029` (line-break strictness)
*   `table-layout_038` (empty-cells)

## 2. Robust Box Model Math Helpers

**What:** 
Add explicit geometry computation methods to `ResolvedBoxProps` and `LogicalSize` in `geometry.rs`. Specifically, methods that explicitly calculate `margin_box()`, `padding_box()`, `content_box()`, and directional inset helpers (e.g., `shrink_by_edges`, `expand_by_edges`). 

**Why:** 
The layout solver currently scatters raw math (e.g., `size.width + margin.left + margin.right`) throughout `fc.rs`, `sizing.rs`, and `positioning.rs`. Several patches attempt to fix "double-margin applications" or "float overlap checks" by modifying these raw calculations. Creating semantic helpers forces patches to declare *which* box they are operating on (margin box vs content box), eliminating off-by-one errors and coordinate space confusion during BFC and float layouts.

**Needed for patches:**
*   `floats_004`, `006`, `042` (margin box overlap and clearance math)
*   `box-model_024`, `039` (inline box split border/padding calculations)
*   `width-calculation_*` and `height-calculation_*` (especially absolute positioning size resolution)

## 3. White-Space Processing Pipeline Abstractions

**What:** 
Refactor `split_text_for_whitespace` in `fc.rs` into a structured, phase-based pipeline. Define distinct internal phases: `Phase 1: Collapse`, `Phase 2: Segment Break Transformation`, `Phase 3: Edge Trimming`, and `Phase 4: Tab Resolution`.

**Why:** 
There is a massive conflict cluster (Group 5) containing seven overlapping patches that implement different rules from the CSS Text Level 3 specification. If applied to the current monolithic string-manipulation block, they will overwrite each other or create regex spaghetti. By defining the pipeline architecture first, each patch can be cleanly slotted into its respective phase.

**Needed for patches:**
*   `white-space-processing_024` (Phase I collapsing)
*   `white-space-processing_032`, `034` (Segment breaks and line endings)
*   `white-space-processing_030`, `036` (Trailing and hanging spaces)
*   `white-space-processing_045` (Tab size processing)

## 4. Layout Tree Traversal Helpers for Table Generation

**What:** 
Add semantic iterators and query methods to `LayoutTreeBuilder` (in `layout_tree.rs`) specifically for table structure validation. Helpers should include `find_consecutive_non_cell_children()`, `is_misparented_table_item()`, and `wrap_nodes_in_anonymous_box()`.

**Why:** 
Cluster 1 contains four competing, massive rewrites of `process_table_children` to fix CSS 2.2 §17.2.1 anonymous box generation. The underlying cause of this complexity is the lack of safe DOM-to-Layout-Tree lookahead tools. Providing these helpers in advance allows the chosen patch to safely identify missing parents and inject `AnonymousBoxType::TableRow` or `TableWrapper` without corrupting the tree indices.

**Needed for patches:**
*   `table-layout_001`, `003`, `021`, `024` (Anonymous table box generation)
*   `display-property_016` (Fixing misparented table boxes)

## 5. Unified Out-Of-Flow (Absolute/Fixed) Containing Block Resolution

**What:** 
Create a definitive `resolve_absolute_containing_block(tree, node_idx)` helper function that returns both the padding-box dimensions and the absolute coordinate offsets of the nearest positioned ancestor.

**Why:** 
Currently, the logic to find an absolute element's containing block is duplicated between the sizing phase (`sizing.rs`) and the positioning phase (`positioning.rs`). Several incoming patches update how width and height are calculated for absolute elements (CSS 2.2 §10.3.7 and §10.6.4). If this logic isn't unified first, the sizing patches and positioning patches will drift out of sync, resulting in elements that are sized according to one containing block but positioned according to another.

**Needed for patches:**
*   `containing-block_028`, `029` (abspos vertical sizing and CB chain)
*   `height-calculation_043`, `047` (abspos vertical sizing)
*   `width-calculation_028`, `036` (abspos horizontal sizing)
*   `positioning_023` (relative/absolute positioning calculation fixes)

---

Yes, to fully secure the architecture before applying the remaining conflict clusters, there are five more critical abstractions that need to be established. These address the most complex mathematical and state-tracking problems highlighted in the patch review.

## 6. Centralized Display Blockification Matrix

**What:** 
Create a definitive `get_computed_display(raw_display, is_absolute, is_floated, is_root, parent_formatting_context)` helper in `getters.rs`. This function must implement the exact blockification table defined in CSS Display Module Level 3 (e.g., translating `inline-block` to `block` when floated or absolutely positioned). 

**Why:** 
Cluster 2 contains 8+ patches that independently scatter new `match` arms across `layout_tree.rs` to enforce blockification for flex items, grid items, table internals, and root nodes. Without a centralized matrix, these patches will create conflicting, order-dependent logic where an element might be blockified by one rule but reverted by another.

**Needed for patches:**
*   `display-property_001`
*   `containing-block_035`, `043`, `047`
*   `margin-collapsing_046`
*   `table-layout_026`, `028`, `029`, `043`

## 7. CSS Dimension Equation Solvers (CSS 2.2 §10.3 / §10.6)

**What:** 
Extract the logic for calculating widths and heights into dedicated solver functions (e.g., `solve_horizontal_formatting_equation`) in `sizing.rs`. This solver should take the knowns (containing block width, borders, paddings, and non-auto variables) and solve for the `auto` unknowns (usually width and/or margins), applying min/max constraints and over-constrained rules automatically.

**Why:** 
Cluster 12 (`width-calculation`) and the absolute positioning patches contain massive blocks of procedural `if/else` logic to handle the interaction between `width: auto`, `margin-left: auto`, and `margin-right: auto`. Because different display types (block, float, inline-block, abspos) resolve these equations slightly differently, patching them without a mathematical solver abstraction results in deeply nested, unreadable code that is prone to dropping CSS edge cases.

**Needed for patches:**
*   `width-calculation_001`, `002`, `010`, `028`, `032`, `036`, `050`
*   `height-calculation_043`, `047`

## 8. Inline Fragment Edge Tracking for Box Decorations

**What:** 
Add edge-tracking metadata (e.g., `is_first_visual_fragment`, `is_last_visual_fragment`) to the `ShapedItem` and `PositionedItem` structs in `text3/cache.rs`. 

**Why:** 
Cluster 10 implements `box-decoration-break` and bidi-aware inline box borders. When an inline element (like a `<span style="border: 1px solid">`) wraps across three lines or is reordered by RTL text rules, the layout engine must know which fragment gets the left border, which gets the right border, and which gets neither. If the patches try to infer this dynamically during positioning, it will create fragile, deeply coupled rendering code. Adding the state trackers beforehand makes the patches trivial.

**Needed for patches:**
*   `box-model_024`, `039`
*   `inline-formatting-context_027`

## 9. Stacking Context Predicate

**What:** 
Create a single `establishes_stacking_context(node, computed_style)` boolean helper. This must comprehensively check z-index, opacity, transforms, filters, and positioning.

**Why:** 
Cluster 6 contains competing patches trying to fix z-index and stacking context creation in `display_list.rs`. By centralizing the W3C rules for what creates a stacking context into a single predicate *before* applying the patches, you guarantee that display list generation remains clean and doesn't get cluttered with inline CSS property lookups.

**Needed for patches:**
*   `positioning_014`, `016`, `019`

## 10. Line Box Metrics Accumulator (Half-Leading)

**What:** 
Create a `LineBoxMetrics` struct in `glyphs.rs` with a method like `add_item(ascent, descent, line_height)`. This struct should encapsulate the logic for distributing CSS `line-height` as "half-leading" (padding added equally to the top and bottom of the text's baseline).

**Why:** 
Cluster 8 consists of 7 patches rewriting `calculate_line_metrics` to implement CSS 2.2 §10.8.1. Currently, the code folds over items to find the maximum physical bounds. True CSS line-height requires aligning baselines, determining the half-leading for each specific run, and *then* finding the maximum bounds of the resulting line box. Setting up the accumulator structure first allows the incoming patches to just feed data into it rather than rewriting the iterator logic 7 different ways.

**Needed for patches:**
*   `line-height_001` through `007`
*   `line-height_018`, `046`

