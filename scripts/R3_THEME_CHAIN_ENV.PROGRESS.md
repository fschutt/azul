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

## IN PROGRESS
- RED 3: context chain tests (css app_theme_selects_theme_blocks, layout app_theme_override).

## NEXT
3. GREEN 3: `app_theme_chain` through `expand_chain` (default floor); `set_app_theme` reports the
   resolved head's chain warnings. DEPENDS ON R2's exclusive-floors rule (flat inert under flora).
4. Docs: guide styling/themes.md + debugging.md env list.
5. Report `scripts/R3_THEME_CHAIN_ENV_2026_09_29.md`.

## Open questions
- none
