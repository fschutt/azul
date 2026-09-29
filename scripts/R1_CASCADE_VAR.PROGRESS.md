# R1_CASCADE_VAR progress (branch wt/r1-cascade-var, from 0a326afe5)

Task: cascade-level custom properties (`--name` / `var()`), design
`scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` §7.2, §7.3, §9 gap 1,
pitfalls 1, 2, 11.

## DONE
- 6c4c0b13c test: RED (core/tests/custom_properties.rs, I5 epoch extension, parser2
  shape tests, css/tests/custom_property_resolution.rs, widget var-fallback lint)
- 20ec9b875 feat(css): CssDeclaration::CustomProperty, parser keeps definitions and
  var() Dynamic, azul_css::custom_properties resolver, codegen + match arms
- 989832a63 feat(core): variable pass in restyle, CustomPropertyEnvs, resolved_inline,
  inline_properties view, context re-cascade, diff CUSTOM_PROPERTIES

## IN PROGRESS
- review pass (compile-by-reading), report

## NEXT
1. second read of every changed site for compile errors
2. report scripts/R1_CASCADE_VAR_2026_09_29.md (api.json list, least-sure spots,
   test commands, what is left)

## Open questions
- icon.rs copy_appropriate_styles_vec copies static declarations only (a var() on an
  <icon> node is dropped when the icon resolves) - pre-existing, left for the icon work.
