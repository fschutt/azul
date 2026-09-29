# R1_CASCADE_VAR progress (branch wt/r1-cascade-var, from 0a326afe5)

Task: cascade-level custom properties (`--name` / `var()`), design
`scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` §7.2, §7.3, §9 gap 1,
pitfalls 1, 2, 11.

## DONE
- (none yet)

## IN PROGRESS
- RED tests

## NEXT
1. RED: core/tests/custom_properties.rs (step-1 test, cross-sheet, inheritance, fallback,
   cycle), I5 epoch extension, diff test, parser2 shape tests, lint test.
2. css: `CssDeclaration::CustomProperty(CssCustomProperty)`, parser keeps `--x` and leaves
   `var()` Dynamic (fallback chain in dynamic_id, no fallback = initial + warning),
   resolver module `css/src/custom_properties.rs`.
3. core: per-node env in `restyle`, css_props placeholders resolved, inline side table +
   `CssPropertyCache::inline_properties` iterator used by every inline reader in
   prop_cache.rs / compact.rs / styled_dom.rs.
4. context change re-runs restyle when variables depend on it; diff marks
   custom-property changes (CUSTOM_PROPERTIES flag, Full scope).
5. match arms elsewhere (xml.rs, dom.rs, e2e export, widgets, reftest, codegen).
6. lint in the widget manifest lint (layout/src/widgets/mod.rs).
7. report.

## Open questions
- (none yet)
