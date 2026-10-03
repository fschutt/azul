# R3_THEME_CHAIN_ENV - progress

Branch `wt/r3-theme-chain-env` (from 0a326afe5). Theme chain (`expand_chain`), `AZ_THEME` = chain head,
`AZ_MODE` = light/dark pin, `AZ_THEME=light|dark` = one-release alias.

## DONE
- 47d7d1bf6 RED 1 expand_chain tests (stub)
- b77a50979 GREEN 1 expand_chain (prefix, BFS fallbacks, dedupe, cycle warning, reserved, default last)
- e58dd4228 RED 2 ThemeEnv / resolve_theme_head tests (stub)
- d2b2855a8 GREEN 2 AZ_MODE pin, AZ_THEME head (env > app > default), alias + deprecation line,
  mode_pinned_by_env / apply_env_mode_pin renames at every caller, dll SetTheme, e2e runner,
  screenshot script
- d752a09b5 checkpoint
- 3b534a3a1 RED 3 context chain tests (css app_theme_selects_theme_blocks, layout app_theme_override)
- 257018134 GREEN 3 app_theme_chain through expand_chain (default floor) - NEEDS R2 exclusive floors
- a14a2b105 guide docs (styling/themes.md, debugging.md)
- e821cbf55 rustfmt theme_chain.rs, doc-line fixes
- report: scripts/R3_THEME_CHAIN_ENV_2026_09_29.md

## IN PROGRESS
- none

## NEXT
- none (parent: compile, run the suites in the report, merge with R2)

## Open questions
- none
