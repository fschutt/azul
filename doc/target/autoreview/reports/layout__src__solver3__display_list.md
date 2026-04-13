# Review: layout/src/solver3/display_list.rs

## Summary
- Lines: 6535
- Public functions: 18
- Public structs/enums: 12
- Findings: 0 high, 7 medium, 4 low

## Findings

### [MEDIUM] Refactoring — `paint_in_flow_descendants` has three near-identical blocks
- **Location**: `display_list.rs:2514-2724`
- **Details**: The three loops (non-float children lines 2514-2589, float children lines 2593-2657, dragging children lines 2660-2724) each contain ~65 lines of nearly identical code: check for GPU transform, push reference frame, push image mask, paint background, push clips, paint descendants, emit VirtualView placeholder, pop clips, pop image mask, paint scrollbars, pop reference frame. The only difference is the input list.
- **Recommendation**: Extract a helper method like `paint_child_with_clips(builder, child_index)` and call it in each loop, reducing ~200 lines to ~30.

### [MEDIUM] File Size — 6535 lines mixing concerns
- **Location**: Entire file
- **Details**: The file contains four distinct subsystems:
  1. Display list types and builder (lines 1-1641)
  2. Display list generation from layout tree (lines 1644-4535)
  3. Pagination / slicer code (lines 5400-6148)
  4. Post-processing helpers (text-overflow, clip-path, SVG rasterization) (lines 6157-6535)
  The pagination/slicer code (~750 lines) and the clip/offset helper functions (~700 lines) are self-contained and could be split into `display_list_pagination.rs` and `display_list_clip.rs`.
- **Recommendation**: Consider splitting pagination and clip helpers into submodules. The generation code is cohesive and should stay together.

### [MEDIUM] Outdated Comment — references non-existent doc
- **Location**: `display_list.rs:133`
- **Details**: `WindowLogicalRect` doc comment says "See `doc/SCROLL_COORDINATE_ARCHITECTURE.md` for background." This file exists in the repo (confirmed via grep), but it's in `dll/src/desktop/compositor2.rs` context — verify that the referenced doc is still accurate for `WindowLogicalRect`'s semantics.
- **Evidence**: `grep -r "SCROLL_COORDINATE_ARCHITECTURE" .` finds references in compositor2.rs and display_list.rs.
- **Recommendation**: Verify the doc file is still accurate or update the reference.

### [MEDIUM] Stub Code — `generate_text_display_items` uses Unicode codepoints as glyph indices
- **Location**: `display_list.rs:6070-6126`
- **Details**: Comment on line 6070 says "For now, this creates a placeholder that renderers should handle specially." Line 6105: `index: c as u32, // Use Unicode codepoint as glyph index (placeholder)`. This is a known placeholder — renderers expecting real glyph IDs will render garbage. Also uses `FontHash::from_hash(0)` (line 6120) meaning "no font".
- **Recommendation**: Either implement proper text shaping using text3, or document this limitation prominently and ensure all callers handle the placeholder case.

### [MEDIUM] Stub Code — `get_image_ref_for_image_source` has unimplemented branches
- **Location**: `display_list.rs:4670-4685`
- **Details**: Two branches return `None` with TODO comments:
  - Line 4676: `// TODO: Look up in ImageCache` for `ImageSource::Url`
  - Line 4681: `// TODO: Decode raw data / SVG to ImageRef` for `ImageSource::Data/Svg/Placeholder`
  These mean CSS `url()` background images, raw image data, and SVG images silently fail to render.
- **Recommendation**: Implement or track these as known limitations.

### [MEDIUM] Code Style — `to_debug_json` is 175 lines of repetitive `writeln!` calls
- **Location**: `display_list.rs:396-571`
- **Details**: The `to_debug_json` method manually formats JSON using `writeln!` with `{{` escapes for every variant. This is error-prone and verbose.
- **Recommendation**: Use `serde_json` or a simpler debug format. If JSON is required but serde is not desired, extract the repeated pattern into a helper.

### [MEDIUM] Known Bug Pattern — `..Default::default()` for margins
- **Location**: `display_list.rs:4351,4354`
- **Details**: `Default::default()` is used as a fallback for `margins` in `paint_inline_shape`. The EdgeSizes default is all zeros, which is correct for "no margins" fallback.
- **Evidence**: Verified EdgeSizes Default impl produces zero margins.
- **Recommendation**: No action needed — this is safe. Documenting for completeness.

### [LOW] Hardcoded `16.0` for shadow pixel conversion
- **Location**: `display_list.rs:978-981`
- **Details**: `shadow.offset_x.to_pixels_internal(16.0)` uses `16.0` as an assumed font size for converting shadow CSS values to pixels. If the actual font size differs, shadow visual_bounds will be wrong.
- **Recommendation**: Pass the actual font size or element font size if available.

### [LOW] TODOs — 11 TODO comments indicating incomplete features
- **Location**: Lines 2341, 2802, 2905, 3379, 4020-4030, 4676, 4681
- **Details**: Various TODO comments for:
  - CSS Overflow 3 abs-pos clipping exemption (lines 2341, 2802, 2905)
  - Table border-collapse conflict resolution (line 3379)
  - Inline z-index ordering (line 4021)
  - Text shadows (line 4023)
  - Text overflow handling (line 4030)
  - CSS url() image lookup (line 4676)
  - Raw data/SVG image decoding (line 4681)
- **Recommendation**: Track as known limitations; these are documented inline.

### [LOW] Documentation Verbosity — `apply_text_overflow_ellipsis` has 25-line doc comment
- **Location**: `display_list.rs:6162-6191`
- **Details**: The doc comment is very detailed for a function that is never called. If the function is kept, the docs are appropriate. If removed as dead code, this is moot.
- **Recommendation**: Address via dead code cleanup.

## System Documentation
- System identified: **Rendering pipeline** (display list generation — the bridge between layout solver and compositor/renderer)
- Existing doc: `doc/guide/architecture.md` covers high-level architecture; no dedicated rendering pipeline guide exists.
- Doc needed: A `doc/guide/rendering-pipeline.md` explaining the layout→display_list→compositor flow, the display list item types, coordinate spaces (window-logical vs frame-relative), and the pagination/slicer subsystem. This file is the central piece of that pipeline.
