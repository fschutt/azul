# Review: layout/src/cpurender.rs

## Summary
- Lines: 4382
- Public functions: ~35 (including methods)
- Public structs/enums: 10 (LayerId, CompositorState, Layer, LayerReason, AzulPixmap, PixelDiffResult, RenderOptions, CpuRenderState, ComponentPreviewOptions, ComponentPreviewResult)
- Public type aliases: 1 (ScrollOffsetMap)
- Findings: 3 high, 5 medium, 1 low

## Findings

### [HIGH] Dead Code — `LayerId` struct has zero external call sites
- **Location**: `cpurender.rs:54-55`
- **Details**: `LayerId` is `pub` but never referenced outside cpurender.rs.
- **Evidence**: Grep for `LayerId` outside cpurender.rs returns zero results (all matches are within cpurender.rs).
- **Recommendation**: Mark as `pub(crate)` or remove if the compositor layer system is not yet wired up.

### [HIGH] Dead Code — `Layer` struct has zero external call sites
- **Location**: `cpurender.rs:73-97`
- **Details**: `Layer` is `pub` but never referenced outside cpurender.rs.
- **Evidence**: Grep for `pub struct Layer` or `Layer {` outside cpurender.rs returns zero results (other `Layer` matches are comments in display_list.rs).
- **Recommendation**: Mark as `pub(crate)` or remove.

### [HIGH] Stub Code — Filter/TextShadow/BackdropFilter push/pop are complete no-ops
- **Location**: `cpurender.rs:2914-2920`
- **Details**: Six display list item types are matched with empty bodies:
  ```rust
  // TODO: proper compositing architecture with per-layer pixbufs
  DisplayListItem::PushFilter { .. } => {}
  DisplayListItem::PopFilter => {}
  DisplayListItem::PushBackdropFilter { .. } => {}
  DisplayListItem::PopBackdropFilter => {}
  DisplayListItem::PushTextShadow { .. } => {}
  DisplayListItem::PopTextShadow => {}
  ```
  This means CSS `filter`, `backdrop-filter`, and `text-shadow` have no visual effect in the CPU renderer, which is a significant feature gap.
- **Evidence**: The file `scripts/fix-gradients-filters-plan.md` explicitly documents these as stubs.
- **Recommendation**: Implement or clearly document as known limitation. The compositor layer system (lines 49-511) already has infrastructure for filter layers but isn't wired into the single-pass `render_single_item` path.

### [MEDIUM] Unsafe — Raw pointer cast for font access
- **Location**: `cpurender.rs:3054` and `cpurender.rs:3087`
- **Details**: `unsafe { &*(font_ref.get_parsed() as *const ParsedFont) }` casts a `*const c_void` to `*const ParsedFont`. This is sound only if the `FontRef` remains alive and no concurrent thread drops it. The `FontRef` type has `unsafe impl Send + Sync` without requiring `T: Send + Sync` on the inner pointer.
- **Recommendation**: Use a centralized helper (e.g. `get_parsed_font()` from `text3/default.rs:105`) to keep the cast in one auditable location.

### [MEDIUM] Missing Documentation — `RenderOptions` struct has no doc comment
- **Location**: `cpurender.rs:1938-1942`
- **Details**: `RenderOptions` is widely used across all platform shells but has no documentation.
- **Recommendation**: Add a brief doc comment explaining what each field means.

### [MEDIUM] Missing Documentation — `ScrollOffsetMap` type alias has no doc comment
- **Location**: `cpurender.rs:2032`
- **Details**: Only has a comment, not a `///` doc comment. Used across platform shells.
- **Recommendation**: Convert the `///` comment to a proper doc comment (it already exists as a regular comment on line 2029-2031).

### [MEDIUM] Refactoring — `render_single_item` is ~570 LOC
- **Location**: `cpurender.rs:2373-2955`
- **Details**: This match-based dispatch function is very long at ~570 lines. While each arm is relatively self-contained, the sheer size makes it hard to navigate.
- **Recommendation**: Extract groups of related arms into sub-functions (e.g. `render_gradient_item`, `render_state_push_pop`). Alternatively, keep as-is since match arms are inherently self-contained.

### [MEDIUM] Refactoring — `render_border` is ~185 LOC
- **Location**: `cpurender.rs:3156-3341`
- **Details**: Builds outer + inner paths and has three render paths (dashed, solid-no-radius, solid-rounded). Could be split into sub-functions for each style.
- **Recommendation**: Consider extracting `render_dashed_border`, `render_solid_border_fast`, `render_solid_border_rounded`.

### [LOW] Dead Code — `CompositorState` retained-mode compositor is partially wired
- **Location**: `cpurender.rs:49-511`
- **Details**: `CompositorState` is used in `headless/mod.rs` but only constructed/reset on resize. The layer allocation, damage computation, and compositing methods (`allocate_layers_from_display_list`, `compute_damage`, `render_layers`, `composite_frame`, `scroll_layer`) appear to be unused. The main rendering path goes through the single-pass `render_display_list_with_state` instead.
- **Evidence**: `CompositorState::new` is called in headless (line 136), `allocate_layers_from_display_list` not called outside cpurender.rs. The retained-mode compositor is infrastructure for a future optimization, not currently active.
- **Recommendation**: Document that the compositor is WIP/experimental, or remove if not planned.

## System Documentation
- System identified: yes - **CPU Rendering Pipeline** (software rasterization using agg-rust, display list rendering, damage tracking, compositing)
- Existing doc: none (no `doc/guide/rendering.md` or similar)
- Doc needed: A `doc/guide/rendering.md` covering the CPU rendering pipeline: how display lists are consumed by cpurender, the relationship between `render_display_list_with_state` (single-pass) and the `CompositorState` (retained-mode compositor), damage tracking via `compute_display_list_damage` / `render_display_list_damaged`, the AGG rasterization helpers, and how platform shells integrate the renderer.
