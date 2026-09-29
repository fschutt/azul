# I1 - icon metadata, capability-aware resolver, remap rules (PROGRESS)

Branch `wt/i1-icon-metadata`, cut from `fix/input-bugs-2026-09-19` @ `0a326afe5`.
Final report: `scripts/I1_ICON_METADATA_2026_09_29.md`. Nothing compiled (house rule).
Design: `scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` sections 8, 8.1, 8.2, 9.1 pitfall 10.

## DONE
- (none yet)

## IN PROGRESS
- plan checkpoint

## NEXT (in order)
1. RED + impl: `IconMeta` (designed_for / variants / recolor / monochrome) on the registered data,
   defaults (font = CurrentColor, image = None), variant pick per mode, request x capability.
2. RED (headless pixel test, one PNG icon at two tints) + fix E15: `flood(tint) composite(in)` for mask
   artwork; CPU renderer gets Flood + Composite (isolated filter group), WebRender folds the pair into
   one colour matrix.
3. RED + impl SVG: `currentColor` + palette remap in the rasteriser, `SvgIconData`,
   `register_svg_icon(pack, name, svg_bytes, meta)`, `currentColor` flood token resolved per node.
4. RED + impl: pack rank, then registration order.
5. RED + impl: remap rules (`apply-if` in the dynamic-selector vocabulary, evaluated at lookup against the
   live context), loader over `~/.azul/icons/` with an injectable root and traversal checks.
6. Guide update, report.

## Decisions
- Mode is read from `SystemStyle.theme` (R0 renames it to a mode); the core resolution entry point hands the
  resolver a style whose `theme` is the WINDOW's mode (the context's), not the desktop's.
- Variants are icon SPECS; the default resolver redirects by returning `Dom::create_icon(variant)`, which
  the existing icon-to-icon indirection loop resolves.

## Open questions
- (none yet)
