# R3_THEME_CHAIN_ENV - progress

Branch `wt/r3-theme-chain-env` (from 0a326afe5). Theme chain (`expand_chain`), `AZ_THEME` = chain head,
`AZ_MODE` = light/dark pin, `AZ_THEME=light|dark` = one-release alias.

## DONE
- (see git log; hashes listed on each checkpoint)

## IN PROGRESS
- RED 1: `css/src/theme_chain.rs` - `expand_chain` unit tests, stub body.

## NEXT
1. GREEN 1: `expand_chain` implementation (prefix, BFS fallbacks, dedupe, cycle warning, reserved, default last).
2. RED 2 / GREEN 2: `ThemeEnv::from_values` (AZ_MODE, alias + deprecation, head), `resolve_theme_head`
   (env > app > default), `theme_env()` read once; `theme_pinned_by_env` -> `mode_pinned_by_env` (ONE fn,
   every reader), `apply_env_theme_pin` -> `apply_env_mode_pin`; core `app_theme()` applies the env head;
   `set_app_theme` reports env + chain warnings via diagnostics; dll SetTheme `already` vs resolved; e2e runner.
3. RED 3 / GREEN 3: `app_theme_chain` through `expand_chain` (default floor) - context chain tests
   (css app_theme_selects_theme_blocks, layout app_theme_override). DEPENDS ON R2 exclusive floors.
4. Docs: guide styling/themes.md + debugging.md env list; screenshot_single.sh -> AZ_MODE.
5. Report `scripts/R3_THEME_CHAIN_ENV_2026_09_29.md`.

## Open questions
- none
