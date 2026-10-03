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
- ddc5e3b92 docs(guide), b7efc5199 test helper, 1bf8908e4 relayout scope of var()
- read-only review agent: no compile error found; findings fixed or in the report
- report: scripts/R1_CASCADE_VAR_2026_09_29.md

## IN PROGRESS
- (none)

## NEXT
- parent: compile, run the suites in the report, apply the api.json entries via autofix and
  regenerate (blocking: FFI enum mismatch until then)

## Open questions
- icon.rs copy_appropriate_styles_vec copies static declarations only (a var() on an
  <icon> node is dropped when the icon resolves) - pre-existing, left for the icon work.
