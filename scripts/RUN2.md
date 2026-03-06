# Run 2 Patch Review — Verified Summary

**Date**: 2026-03-05
**Branch base**: `origin/layout-debug` (clean, before any agent patches)
**Patches**: 800 total in `doc/target/skill_tree/all_patches/run2_patches/`

## Overview

| Metric | Count |
|--------|-------|
| Total patches | 800 |
| CODE patches (non-comment changes) | 373 |
| ANNOT patches (comment-only `// +spec:`) | 427 |
| Features covered | 16 |

### CODE patches by intent (from commit subject)

| Type | Count | Description |
|------|-------|-------------|
| `implement` | 69 | New functionality matching spec paragraphs |
| `fix` | 39 | Bug fixes for existing behavior |
| `annotate` (with side-effects) | 241 | Annotations that also touch non-comment code (minor refactors, added enums, etc.) |
| other | 24 | Mixed or unclear subjects |

## Per-Feature Breakdown

| Feature | CODE | ANNOT | Total | Key files |
|---------|------|-------|-------|-----------|
| block-formatting-context | 11 | 39 | 50 | fc.rs |
| box-model | 17 | 33 | 50 | fc.rs, geometry.rs, layout_tree.rs, cache.rs |
| containing-block | 20 | 30 | 50 | mod.rs, positioning.rs, layout_tree.rs |
| display-property | 13 | 37 | 50 | layout_tree.rs, fc.rs |
| floats | 13 | 37 | 50 | fc.rs, positioning.rs |
| height-calculation | 19 | 31 | 50 | sizing.rs, fc.rs |
| inline-block | 18 | 32 | 50 | fc.rs, getters.rs, cache.rs |
| inline-formatting-context | 27 | 23 | 50 | fc.rs, cache.rs |
| intrinsic-sizing | 37 | 13 | 50 | geometry.rs, sizing.rs, fc.rs |
| line-breaking | 35 | 15 | 50 | knuth_plass.rs, cache.rs |
| line-height | 37 | 13 | 50 | glyphs.rs, cache.rs, sizing.rs |
| margin-collapsing | 20 | 30 | 50 | fc.rs, positioning.rs, taffy_bridge.rs |
| positioning | 12 | 38 | 50 | positioning.rs, display_list.rs |
| table-layout | 24 | 26 | 50 | layout_tree.rs, fc.rs, sizing.rs |
| white-space-processing | 36 | 14 | 50 | cache.rs, knuth_plass.rs, fc.rs |
| width-calculation | 34 | 16 | 50 | sizing.rs, fc.rs |

## High-Conflict Files

These files are touched by many CODE patches and will have the most merge conflicts:

| File | CODE patches | Role |
|------|-------------|------|
| `layout/src/solver3/fc.rs` | 171 | Main formatting context solver |
| `layout/src/text3/cache.rs` | 92 | Text layout cache, line breaking |
| `layout/src/solver3/sizing.rs` | 83 | Width/height calculation |
| `layout/src/solver3/layout_tree.rs` | 49 | DOM-to-layout tree, display property |
| `layout/src/solver3/positioning.rs` | 38 | Absolute/relative positioning |
| `layout/src/solver3/geometry.rs` | 37 | Box model geometry structs |
| `layout/src/text3/knuth_plass.rs` | 37 | Knuth-Plass line breaking |
| `layout/src/text3/glyphs.rs` | 26 | Glyph metrics, line height |
| `layout/src/solver3/mod.rs` | 21 | Containing block resolution |
| `layout/src/solver3/getters.rs` | 17 | Property getters |
| `layout/src/solver3/taffy_bridge.rs` | 13 | Taffy flexbox bridge |
| `layout/src/solver3/display_list.rs` | 7 | Display list / stacking |
| `layout/src/solver3/cache.rs` | 6 | Layout cache |

## High-Impact Patches (50+ code lines changed)

These are the patches with the most substantive code changes. They form the core of run2's value.

### Height Calculation
- **height-calculation_043** (+316/-23): Implement §10.6.3 block height from floats/abspos
- **height-calculation_047** (+234/-25): Implement §10.6.4 abspos height calculation

### Containing Block
- **containing-block_029** (+220/-10): Implement CSS 2.2 §10.1 abspos CB chain
- **containing-block_028** (+88/-14): Implement §10.6.4 abspos vertical sizing
- **containing-block_047** (+98/-2): Display blockification for root/abspos elements

### Table Layout
- **table-layout_001** (+141/-78): Fix §17.2.1 anonymous table box generation
- **table-layout_003** (+132/-61): Fix §17.2.1 anonymous table box generation (variant)
- **table-layout_021** (+118/-62): Fix §17.2.1 anonymous table box generation (variant)
- **table-layout_024** (+132/-66): Fix anonymous table cell wrapping
- **table-layout_038** (+75/-3): Implement empty-cells:hide zero-height

### White Space Processing
- **white-space-processing_024** (+139/-63): Implement Phase I white-space collapsing
- **white-space-processing_032** (+119/-8): Implement Phase II segment break transformation
- **white-space-processing_045** (+147/-6): Implement tab-size processing
- **white-space-processing_030** (+95/-11): Implement Phase II trailing space removal
- **white-space-processing_028** (+100/-0): Add line-break strictness enum
- **white-space-processing_034** (+64/-19): Implement line-ending normalization
- **white-space-processing_036** (+48/-1): Implement hanging whitespace behavior
- **white-space-processing_035** (+25/-15): Implement break-spaces

### Line Breaking
- **line-breaking_038** (+149/-12): Implement text-indent each-line and hanging
- **line-breaking_040** (+108/-8): Implement word-break property
- **line-breaking_023** (+94/-21): Implement word-break (variant)
- **line-breaking_013** (+87/-21): Implement line-break CSS property
- **line-breaking_015** (+81/-6): Implement word-break and line-break
- **line-breaking_014** (+72/-4): Implement overflow-wrap
- **line-breaking_034** (+72/-6): Implement word-break (variant)
- **line-breaking_019** (+62/-5): Implement word-break (variant)
- **line-breaking_042** (+50/-4): Implement text-align-last property
- **line-breaking_048** (+56/-2): Implement non-tailorable Unicode line break

### Width Calculation
- **width-calculation_002** (+130/-11): Implement §10.3.3 block width
- **width-calculation_019** (+119/-10): Implement §17.5.2 table column width
- **width-calculation_036** (+108/-10): Implement §10.3.8 abspos width
- **width-calculation_020** (+101/-14): Implement §17.5.2 table width (variant)
- **width-calculation_028** (+73/-9): Implement §10.3.7 abspos width
- **width-calculation_050** (+70/-8): Implement §10.3.5 float shrink-to-fit
- **width-calculation_032** (+57/-6): Implement §10.3.4 inline-block width

### Intrinsic Sizing
- **intrinsic-sizing_027** (+109/-2): Implement flex container intrinsic sizing
- **intrinsic-sizing_019** (+94/-2): Implement fit-content() sizing function
- **intrinsic-sizing_012** (+61/-4): Implement fit-content keyword support
- **intrinsic-sizing_044** (+39/-6): Fix flex container intrinsic size

### Line Height
- **line-height_018** (+97/-10): Implement table cell baseline alignment
- **line-height_046** (+55/-11): Implement vertical-align:baseline for inline-block
- **line-height_005** (+45/-5): Implement half-leading calculation
- **line-height_007** (+47/-3): Fix line box height calculation

### Floats
- **floats_004** (+79/-13): Fix float margin box overlap
- **floats_006** (+21/-11): Fix clearance margin collapsing

### Inline Formatting
- **inline-formatting-context_004** (+66/-9): Implement two-pass line box height
- **inline-formatting-context_027** (+52/-14): Implement inline box decoration

### Box Model
- **box-model_024** (+59/-4): Implement inline box split border/padding
- **box-model_039** (+47/-7): Implement bidi-aware inline box decoration
- **box-model_034** (+26/-5): Implement table wrapper for display:table
- **box-model_012** (+56/-1): Annotate §17.6.1 (adds table cell border model structs)

### Display Property
- **display-property_001** (+50/-3): Implement §2.7 display blockification
- **display-property_016** (+17/-2): Fix misparented table anonymous boxes

### Positioning
- **positioning_014** (+49/-3): Fix stacking context for z-index
- **positioning_016** (+46/-4): Fix stacking context creation
- **positioning_019** (+44/-4): Fix stacking context (variant)
- **positioning_023** (+15/-12): Fix relative positioning calculation

### Block Formatting Context
- **block-formatting-context_050** (+76/-7): Implement p050 — BFC float containment

## Conflict Clusters (Verified)

Patches within the same cluster touch overlapping code regions. During merge, one must be applied first (or they must be merged manually).

### Cluster 1: Table Anonymous Box Generation
**Patches**: table-layout_001, table-layout_003, table-layout_021, table-layout_024
**File**: layout_tree.rs (anonymous box wrapping logic)
**Action**: PICK_ONE or MERGE — all four fix the same §17.2.1 anonymous table box generation. They are large rewrites of the same function. Pick the most complete one and verify the others don't add anything extra.

### Cluster 2: Display Blockification
**Patches**: display-property_001, containing-block_035, containing-block_043, containing-block_047, margin-collapsing_046, table-layout_028, table-layout_026, table-layout_029, table-layout_043
**File**: layout_tree.rs (blockify_display / display mapping)
**Action**: MERGE — these all add blockification rules for different contexts (root, abspos, flex items, grid items, table internals). They can likely be applied sequentially since they add to different match arms, but need ordering care.

### Cluster 3: Word-Break / Line-Break Property
**Patches**: line-breaking_008, line-breaking_013, line-breaking_015, line-breaking_016, line-breaking_018, line-breaking_019, line-breaking_022, line-breaking_023, line-breaking_034, line-breaking_040
**File**: knuth_plass.rs, cache.rs
**Action**: PICK_ONE or MERGE — many of these implement the same `word-break` property with overlapping enums and match arms. The larger ones (040, 023, 013) are more complete. Pick best and verify no unique logic lost.

### Cluster 4: White Space Processing Phases
**Patches**: white-space-processing_024, white-space-processing_030, white-space-processing_032, white-space-processing_034, white-space-processing_045
**File**: cache.rs (text processing pipeline)
**Action**: MERGE in order — these implement distinct phases (I: collapsing, II: segment breaks, II: trailing removal, line-ending normalization, tab-size). They should be mergeable if applied in the right order.

### Cluster 5: Abspos Height/Width Calculation
**Patches**: containing-block_028, containing-block_029, height-calculation_043, height-calculation_047, width-calculation_036, width-calculation_028
**File**: sizing.rs, positioning.rs
**Action**: MERGE — these implement different spec sections (§10.6.4 vertical, §10.3.7/8 horizontal) for absolutely positioned elements. They touch different functions but the same files.

### Cluster 6: Stacking Context / Z-Index
**Patches**: positioning_014, positioning_016, positioning_019
**File**: display_list.rs
**Action**: PICK_ONE — all three fix stacking context creation with overlapping changes. Compare and pick the most complete.

### Cluster 7: Intrinsic Sizing (fit-content)
**Patches**: intrinsic-sizing_012, intrinsic-sizing_019, intrinsic-sizing_009
**File**: geometry.rs, sizing.rs, dimensions.rs
**Action**: MERGE — 009 adds the FitContent enum variant, 012 adds fit-content keyword support, 019 implements fit-content() function. Sequential application in this order should work.

### Cluster 8: Line Height / Half-Leading
**Patches**: line-height_001, line-height_002, line-height_003, line-height_004, line-height_005, line-height_006, line-height_007
**File**: glyphs.rs, cache.rs, getters.rs
**Action**: MERGE — these implement the §10.8.1 half-leading model and line box height calculation. Multiple patches touch the same `calculate_line_height` / `half_leading` functions. Need careful ordering.

### Cluster 9: Float Margin Box / Clearance
**Patches**: floats_004, floats_006, floats_042
**File**: fc.rs
**Action**: MERGE — 004 fixes margin box overlap detection, 006 fixes clearance margin collapsing, 042 uses margin box for float move-down. Related but touching different functions.

### Cluster 10: Inline Box Decoration (Split Border/Padding)
**Patches**: box-model_024, box-model_039, inline-formatting-context_027
**File**: cache.rs, getters.rs, glyphs.rs
**Action**: MERGE — 024 implements inline box split, 039 adds bidi-awareness, 027 implements inline box decoration breaks. Conceptually related, likely sequential.

### Cluster 11: Table Cell Border Model
**Patches**: box-model_012, box-model_018, box-model_019
**File**: layout_tree.rs, fc.rs
**Action**: MERGE — 012 annotates §17.6.1 (adds structs), 018 zeroes padding for non-cell table internals, 019 zeroes margins for internal table elements. Different functions.

### Cluster 12: Width Calculation (Block/Float/Abspos)
**Patches**: width-calculation_001, width-calculation_002, width-calculation_010, width-calculation_032, width-calculation_050
**File**: sizing.rs
**Action**: MERGE — each implements a different §10.3.x subsection. They add separate functions/match arms in sizing.rs. Order: 001 (§10.3.3 block), 002 (§10.3.3 detail), 010 (§10.3.4), 032 (§10.3.4 inline-block), 050 (§10.3.5 float).

### Cluster 13: Text-Indent
**Patches**: line-breaking_038, line-breaking_039
**File**: cache.rs, fc.rs
**Action**: MERGE — 038 implements text-indent each-line and hanging, 039 implements text-indent in the inline formatting context. Different functions.

### Cluster 14: Hyphens
**Patches**: line-breaking_041, line-breaking_045
**File**: text.rs (CSS props), cache.rs
**Action**: MERGE — 041 adds `StyleHyphens::Manual` enum variant, 045 implements the manual hyphenation logic. Sequential.

### Cluster 15: Line-Break Strictness
**Patches**: white-space-processing_028, white-space-processing_029
**File**: cache.rs, text.rs
**Action**: MERGE — 028 adds the line-break strictness enum, 029 annotates usage. Sequential.

## ANNOT-Only Patches (427)

These 427 patches add ONLY `// +spec:feature-pNNN` annotation comments. They are valuable for traceability but have zero functional impact.

**Recommendation**: APPLY ALL — they don't conflict with each other or with CODE patches (they add comment lines that won't interfere with code changes). Apply these first as a bulk operation, then apply CODE patches on top.

**Exception**: Some ANNOT patches may fail to apply after CODE patches have been applied (context drift). Apply ANNOT first, CODE second.

## Recommended Application Order

1. **Phase 1: ANNOT patches** (427 patches)
   - Apply all annotation-only patches first. They add comments to existing lines.
   - These rarely conflict with each other since they add new comment lines.
   - Use `git am --3way` and auto-resolve where possible.

2. **Phase 2: Independent CODE patches** (non-clustered, ~260 patches)
   - Apply CODE patches that don't appear in any conflict cluster.
   - These are small, targeted changes to unique code regions.

3. **Phase 3: Conflict clusters** (15 clusters, ~113 patches)
   - Process one cluster at a time.
   - For PICK_ONE clusters: select the most complete patch, verify no unique logic in rejected patches.
   - For MERGE clusters: apply in dependency order, resolve conflicts.

## Patches to Skip

Based on verification, these patches should be skipped or handled carefully:

- **Redundant word-break implementations**: Clusters 3 has ~10 patches implementing the same property. Keep the 2-3 most complete (line-breaking_040, line-breaking_023, line-breaking_013).
- **Redundant table anonymous box**: Cluster 1 has 4 large rewrites. Only one should survive.
- **Redundant stacking context**: Cluster 6 has 3 patches doing the same fix. Pick one.

## Notes for review-arch

When feeding this into `review-arch`, the architecture reviewer should:

1. Group the 15 conflict clusters into merge groups with explicit PICK_ONE/MERGE actions
2. Order independent CODE patches by file (process all fc.rs patches together, all sizing.rs patches together, etc.) to minimize context switching
3. Each merge group's `agent_context` must be self-contained — the applying agent sees only this field + patch diffs + live source code
4. ANNOT patches can be a single large APPLY group (or per-feature groups)
5. The applying agent needs instructions about which patch to prefer in PICK_ONE groups
