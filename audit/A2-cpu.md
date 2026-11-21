# A2-cpu: sign, offset, coordinate and rounding audit of the CPU rendering path

Area agent A2-cpu of the 2026-11-21 audit. Blind: nothing here was compiled or run.
Branch `audit/A2-cpu` (azul and m4p). Base: `audit/base` (`f48bcc7b4`).

## Claims

Functions this area may restructure (owner per RULES.md section 3), all in `layout/src/window.rs`:

- `window_space_offset_of_dom`
- `virtual_view_content_offset`
- `virtual_view_frame_scroll_offsets`
- `patch_virtual_view_content_offset`

Everything else this area touches is a minimal patch in another area's file (a few changed lines inside an existing
function, plus a test directly below it), per RULES.md section 4:

- A1's `layout/src/cpurender/{raster,compositor,pixmap}.rs`: only on top of the tag `audit-A1-structure`, never inside a
  function A1 claims; findings there go to `## For area A1` below.
- A7-editor's `mapedit3/src/ui/main/map.rs` (m4p): minimal patches, bigger things to `## For area A7-editor`.

Tests: `dll/src/desktop/audit_tests/a2_cpu.rs`, `layout/src/audit_tests/a2_cpu.rs`, and `audit_a2_cpu_tests` modules
directly below the functions whose private items they need.

## Spaces

(in progress: committed within the first hour)

## Findings

(in progress)
