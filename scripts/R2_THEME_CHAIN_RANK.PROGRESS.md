# R2_THEME_CHAIN_RANK - progress

Branch `wt/r2-theme-chain-rank`, cut from `0a326afe5`.

## DONE
- `20715c7ff` RED tests: css `a_theme_chain_selects_blocks_by_prefix_and_floor`, layout
  `a_theme_chain_ranks_its_blocks` (+ all.rs), `widgets_follow_the_app_theme` chain check.

- `aa9438649` impl: css matcher + rank + cascade key + inline helpers; core restyle / slow path /
  compact / inheritance sites; `UiTheme::current()` = structural theme.

- Report `scripts/R2_THEME_CHAIN_RANK_2026_09_29.md` committed (with this checkpoint).

## IN PROGRESS
- Waiting for the parent's no-context commit (hash not yet received).

## NEXT
1. After the parent's no-context commit (`DynamicSelector::matches_without_context`) lands: merge it,
   make it call `app_theme_rank(&[app_theme], name)`, and give the no-context sites the same rank
   (`cascade_rank(&[app_theme], conds)` instead of `UNTHEMED_RANK` when `ctx` is None) - report §5.

## Scope change (coordinator)
- Item 4 (no context) is the PARENT's: `DynamicSelector::matches_without_context(&self, app_theme)`,
  called by prop_cache `matches_pseudo_state` + compact.rs ~962 with `current_theme()`. Do NOT write
  a second helper; when its commit lands, route it through the chain matcher on `[app_theme]` and
  give the no-context sites the same rank. Keep only my tests for item 4.

## Open questions
- Unconditional declarations rank LAST (task) - the opposite of CSS `@layer` (unlayered wins). A
  plain inline override appended after a themed widget's block now loses to it. Flag for Felix.
