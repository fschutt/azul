# R2_THEME_CHAIN_RANK - progress

Branch `wt/r2-theme-chain-rank`, cut from `0a326afe5`.

## DONE
- `20715c7ff` RED tests: css `a_theme_chain_selects_blocks_by_prefix_and_floor`, layout
  `a_theme_chain_ranks_its_blocks` (+ all.rs), `widgets_follow_the_app_theme` chain check.

## IN PROGRESS
- css implementation.

## NEXT
1. css: `COMPILED_IN_APP_THEMES`, chain matcher (`app_theme_rank`, `cascade_rank`,
   `structural_app_theme`), context methods; `has_app_theme` = prefix + floor.
2. css: rank in the rule order (`CssRuleBlock::cascade_key`), inline pick / order helpers on `Css`.
3. core: restyle sorts its rule lists by the key; every inline "last match wins" site picks by rank.
4. After the parent's no-context commit (`DynamicSelector::matches_without_context`) lands: make it
   call the chain matcher on `[app_theme]`, and give the no-context sites the same rank.

## Scope change (coordinator)
- Item 4 (no context) is the PARENT's: `DynamicSelector::matches_without_context(&self, app_theme)`,
  called by prop_cache `matches_pseudo_state` + compact.rs ~962 with `current_theme()`. Do NOT write
  a second helper; when its commit lands, route it through the chain matcher on `[app_theme]` and
  give the no-context sites the same rank. Keep only my tests for item 4.

## Open questions
- Unconditional declarations rank LAST (task) - the opposite of CSS `@layer` (unlayered wins). A
  plain inline override appended after a themed widget's block now loses to it. Flag for Felix.
