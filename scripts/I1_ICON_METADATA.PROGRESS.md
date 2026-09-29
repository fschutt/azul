# I1 - icon metadata, capability-aware resolver, remap rules (PROGRESS)

Branch `wt/i1-icon-metadata`, cut from `fix/input-bugs-2026-09-19` @ `0a326afe5`.
Final report: `scripts/I1_ICON_METADATA_2026_09_29.md`. Nothing compiled (house rule).
Design: `scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` sections 8, 8.1, 8.2, 9.1 pitfall 10.

## DONE
- `3a2c83b43` plan checkpoint
- `8e1d021ac` RED: metadata defaults, variant per mode, request x capability (layout/src/icon.rs tests)
- `b4fa0dc75` impl: IconMeta & friends (core), capability-aware resolver (layout), CURRENT_COLOR_TOKEN (css)
- `35cb194ff` RED: E15 pixel test (layout/tests/a_tinted_raster_icon_is_tinted_inside_its_own_alpha.rs),
  CPU flood/composite unit tests, WR fold tests (css filter.rs)
- `4452689d9` fix: filter => stacking context; currentColor flood token per node; CPU flood + composite +
  isolated filter group; WR fold_flood_in + column-major colour matrix

- `e2ca23f73` RED: SVG currentColor / palette (cpurender/svg.rs unit tests, layout icon.rs unit tests,
  layout/tests/an_svg_icon_follows_the_colour_of_its_node.rs)
- `0d61a4568` impl: SvgPaintContext, render_svg_to_imageref_painted, svg_natural_size,
  svg_uses_only_current_color; SvgIconData, register_svg_icon, default_svg_icon_meta

- `b691e355f` RED: rank-ordered lookup (core icon_test.rs, layout icon.rs)
- `29b5b631b` impl: pack_order / pack_ranks / set_pack_rank / insert_icon / remove_pack

- `044a79ec8` RED: remap rules / apply-if at lookup / window mode (core icon_test.rs remap_rules_tests)
- `063c8c62a` impl: IconRemapRule, parse_icon_apply_if, lookup_spec_in_context, resolve_icons_in_dom_with_context

- `5f169baf9` RED: loader (layout/tests/user_icon_rules_follow_the_theme_chain.rs)
- `775790a2a` impl: layout/src/icon_remap.rs (walk_theme_dirs, load_user_icon_rules, ...), dll wiring

- `308e0893d` guide doc/guide/en/styling/icon-packs.md (+ two tiny review fixes)
- `e972f00af` fix: Option<&str> in the loader
- report `scripts/I1_ICON_METADATA_2026_09_29.md`

## IN PROGRESS
- applying the compile-review agent's findings (report section 7)

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
