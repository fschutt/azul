# A2-gpu: where matrices and offsets are made, how the GPU translation consumes them, CPU vs GPU cross-check

Area agent A2-gpu of the 2026-11-21 audit. Blind: nothing here was compiled or run.
Branch `audit/A2-gpu` (azul only). Base: `audit/base` (`f48bcc7b4`).

## Claims

Functions this area may restructure (owner per RULES.md section 3):

- `core/src/transform.rs`: `ComputedTransform3D::from_style_transform_vec`, `from_style_transform`, the matrix
  constructors (`new_translation`, `new_scale`, `new_rotation`, `new_skew`, `new_perspective`, `make_perspective`),
  `then` / `then_translate`, `inverse`, `transform_point2d`.
- `core/src/gpu.rs`: `GpuValueCache::synchronize` (and its transform / opacity key maps).
- `layout/src/managers/gpu_state.rs`: `GpuStateManager::update_scrollbar_transforms`, `update_sticky_transform`,
  `update_scrollbar_transform_key`, `painted_transform_of`.

Everything else this area touches is a minimal patch in another area's file (a few changed lines inside an existing
function, plus a test directly below it), per RULES.md section 4:

- A4's `dll/src/desktop/compositor2.rs`, `dll/src/desktop/wr_translate2.rs` (not the hit-testing functions: A6).
- A3-core's `layout/src/solver3/display_list.rs` (reference frame / stacking context / scroll frame pushes).
- A3-window / A6 / A10's parts of `layout/src/window.rs` (`refresh_scrollbar_transforms`,
  `synchronize_scrollbar_opacity`, `WindowGpuValues::of_doms`).
- A1's `layout/src/cpurender/{raster,compositor,pixmap}.rs`: read-only for this area; differences go to
  `## For area A1` / `## For area A2-cpu` below.

Tests: `dll/src/desktop/audit_tests/a2_gpu.rs`, `layout/src/audit_tests/a2_gpu.rs`, and `audit_a2_gpu_tests` modules
directly below the functions whose private items they need.

## Spaces

(in progress)

## Findings

(in progress)
