# R2 - theme-chain matching, rank in the cascade, the compiled-in floor (2026-09-29)

Branch `wt/r2-theme-chain-rank`, cut from `0a326afe5`. Wave 3, step 2 of the theme refactor
(scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md §7.1, §7.4 step 2, §9.1 pitfall 3).
Nothing was compiled (house rule). Section 6 lists the spots I'm least sure compile.

## 1. What the cascade does now

The context's `theme_chain` lists the active app themes, most specific first. R2 CONSUMES the chain;
R3 builds it (`AZ_THEME`, `:` expansion, `fallback:`, the default last).

- **One matcher**: `azul_css::dynamic_selector::app_theme_rank(chain, name) -> Option<usize>`.
  - **Prefix by `:` segment.** `@theme(xyz)` is live under `[xyz:pink]` and never under `[xyzzy]`.
    This follows the `LanguageCondition::Prefix` precedent.
  - **Rank** is the name's position in the chain, where each entry is followed by the themes it
    extends. `[xyz:pink]` ranks as `[xyz:pink, xyz]`. On a chain R3 has already expanded, the rank
    is the index. 0 is the most specific.
  - **Compiled-in floor.** `COMPILED_IN_APP_THEMES = ["flat", "flora", "native"]` sits next to
    `DEFAULT_APP_THEME` and is THE list. Only the chain's first compiled-in theme is live
    (`structural_app_theme(chain)`), with spin-offs counted: `[flora:abc, flat]` makes flora the
    floor. Non-compiled themes layer on top at their rank. So `[abc, flat]` shows flat's look with
    abc's blocks above it. Under `[flora, flat]`, flat's blocks are dead and cannot fill flora's
    gaps.
  - `DynamicSelectorContext::{has_app_theme, app_theme_rank, cascade_rank}` and
    `DynamicSelector::matches` all call this matcher.
- **Rank of a declaration or rule**: `cascade_rank(chain, conditions)` is the rank of its most
  specific app-theme condition. A declaration with no theme condition gets `UNTHEMED_RANK`
  (`usize::MAX`, last). Light/dark, `@media` and states are not layers.
- **The order** is `(priority, rank, specificity, source order)`, and a lower rank wins. This is CSS
  `@layer` semantics: rank sorts BEFORE specificity.
  - `CssRuleBlock::cascade_key(rank)` is THE key.
  - `Css::sort_by_specificity` is the same key with rank ignored (context-free, stable, unchanged
    order).
  - `restyle` stably re-sorts its matched rule lists (global `*` and specific) under the window's
    rank with `Css::sort_rules_in_cascade_order`.
- **A node's own (inline) declarations** use the same order reduced to `(rank, source order)`:
  - `Css::winning_inline_property(ty, applies, rank)`: the slow path (all 8 state lookups) and
    `get_property_with_context`. It replaces the "last match" folds.
  - `Css::inline_properties_in_cascade_order(rank, &mut buf)`: the compact builder, the restyle
    inheritance walk and `compute_inherited_values` step 4. Each applies the declarations in turn.
  - Both use source order when no declaration is in a theme block, so an unthemed DOM resolves
    exactly as before.
- **Global `*` bucket, slow path**: it now answers the LAST declaration of a property (it answered
  the first). The compact builder always applied them in turn, so the two paths disagreed whenever
  two `*` rules set one property. With rank, a spin-off's `* { color }` must win on both paths.
- **`UiTheme::current()`** is now the structural theme of `app_theme_chain(current_theme())`, with
  Flat when there is none. A widget built under `flora:abc` gets flora's DOM shape, which matches
  the floor the cascade keeps live.

## 2. Decisions for Felix (please read)

1. **Compiled-in themes are exclusive floors** (the parent's decision, built as specified). Only the
   first compiled-in theme in a chain is live. Widgets carry COMPLETE blocks for each, and mixing
   two complete looks leaks (flora never declares `border` and would inherit flat's). What this
   means:
   - A user theme can't say "flora, but fall back to flat's rule where flora has none".
   - A `flat:pink` block is still live under `[flora, flat:pink, flat]`. The registry is exact
     names, and a spin-off of a dead floor is a user theme.
2. **Unconditional declarations rank LAST** (as the task says). In CSS `@layer` it's the other way
   round: unlayered styles WIN. So an unconditional declaration of property P now loses to any live
   `@theme(..)` declaration of P on the same node, whatever the source order. Where this bites:
   - An app appending a plain inline override to an unpinned (theme-following) widget's root
     through `NodeData::set_css` / `add_css_property`. A `Dom::with_css` override goes through the
     stylesheet path at INLINE priority, and a node's inline style always beat it, before R2 too.
   - `NodeData::upsert_inline_css_property`, which runtime patches (`window.rs` ~8067) and
     `focus_cursor.rs` (`OverflowY` / `SpatialNavigationContain` on panels) use. On a themed node,
     the upserted value loses to a themed declaration of the same property. In a live window the
     `user_overridden_properties` layer is consulted first, so the patch still shows. Only a later
     re-cascade from the node's inline style would lose it.
   - Widget-internal merges are safe. `follow_props` makes a property either all unconditional or
     all themed, and `every_theme_css` shares only an unconditional PREFIX. So within one property,
     unthemed never comes after themed. The `[flora, flat]` widget suite checks this.
   - If you want `@layer` semantics instead, only one line changes: make `UNTHEMED_RANK` sort
     above every theme (use `0` and shift theme ranks by one, or special-case it in
     `cascade_key`). The consequence is that a theme block could no longer override an app's
     unconditional rule at the same priority.
3. §9.1 pitfall 3 is now live: `@theme(xyz:pink) { * { color: pink } }` beats every specific rule
   of `xyz`. That is by design; the lint warning (§9.1) is not built.

## 3. Commits

| hash | what |
|---|---|
| `20715c7ff` | RED tests: css `a_theme_chain_selects_blocks_by_prefix_and_floor` (new target); layout `a_theme_chain_ranks_its_blocks` (+ `all.rs` append); `widgets_follow_the_app_theme` also resolves every migrated widget under `[T, other]` |
| `aa9438649` | impl: css matcher / registry / rank / `cascade_key` / inline helpers (+ unit tests in `dynamic_selector.rs`, `css.rs`); core `restyle`, slow path, `get_property_with_context`, compact builder, inheritance sites; `UiTheme::current()` + registry guard test |
| checkpoints | `d7cc180d6`, and the progress / report commits |

What was RED before `aa9438649`:
- the css prefix and floor tests (exact membership);
- every layout rank test in at least one source order;
- `a_second_compiled_in_theme_in_the_chain_fills_no_gap`;
- `a_spin_off_of_flora_builds_and_styles_like_flora`: the structure was flat, and `@theme(flora)`
  was dead under `[flora:abc]`;
- the `[flora, flat]` leg of the widget suite, for every widget whose flat look declares something
  flora's does not.

`a_headless_styled_dom_of_an_unpinned_widget_styles_like_the_current_app_theme` stays RED on this
branch until the parent's no-context commit lands (section 5).

## 4. api.json

**No api.json changes.** `Css::sort_by_specificity` keeps its signature. Everything new is
Rust-only, because it takes closures or generics or returns tuples:
- `azul_css::dynamic_selector::{COMPILED_IN_APP_THEMES, is_compiled_in_app_theme, UNTHEMED_RANK,
  structural_app_theme, app_theme_rank, cascade_rank}`;
- `DynamicSelectorContext::{app_theme_rank, cascade_rank}`;
- `CssRuleBlock::cascade_key`;
- `Css::{sort_rules_in_cascade_order, winning_inline_property, inline_properties_in_cascade_order}`.

## 5. Open: the no-context half (item 4 went to the parent)

On the coordinator's instruction, I did NOT build item 4. The parent is landing
`DynamicSelector::matches_without_context(&self, app_theme: &str)`, called from prop_cache's
`matches_pseudo_state` and `compact.rs` ~978 with `current_theme()`. I kept only my test. Once
that commit is merged in, two follow-ups remain (about 10 lines):

1. Route it through the chain matcher on a one-entry chain:
   `matches!(self, Self::Theme(ThemeCondition::Custom(n)) if app_theme_rank(&[app_theme], n.as_str()).is_some())`.
   `[flora:abc]` then makes flora live, and `[flat]` never makes flora live.
2. Give the no-context sites the same RANK. Every `rank` closure falls back to `UNTHEMED_RANK`
   without a context, and should use `cascade_rank(&[app_theme], conds)` there instead:
   - `prop_cache.rs` ~2861 (slow path);
   - `compact.rs` ~722.

   If the parent extends no-context matching to the three sites below, the same applies to them:
   `prop_cache.rs` ~1616 (restyle), ~1888 and ~5151. Rank only matters headless when a node
   carries both `@theme(x:y)` and `@theme(x)` and the app theme is `x:y`.
3. For the parent: three sites still evaluate non-pseudo conditions context-only. Headless, they
   disagree with `matches_pseudo_state` / compact:
   - the restyle inheritance walk (`prop_cache.rs` ~1888);
   - `compute_inherited_values` step 4 (~5151). A headless widget's TEXT child would inherit the
     unthemed colour while the widget node itself resolves themed;
   - `rule_applies` (~1603). Stylesheet `@theme` rules are dead headless.

## 6. Least sure to compile

1. `css/src/dynamic_selector.rs` `chain_names`:
   - RPIT `impl Iterator<Item = &str> + '_` over a generic `S: AsRef<str>`;
   - nested `move` closures capturing `chain` and `i`;
   - `core::iter::successors(Some(entry.as_ref()), |&n| parent_app_theme(n))`.
2. `COMPILED_IN_APP_THEMES.contains(&name)` with a non-`'static` `name`. This relies on slice
   covariance.
3. The core `rank` closures (annotated `|conds: &[DynamicSelector]|`) are passed BY VALUE several
   times. They must be `Copy`: they capture only a reference to `dyn_ctx` / `ctx`.
4. `prop_cache.rs` restyle: `let mut parent_inline = Vec::new();` before the parent loop, holding
   `(&CssProperty, &DynamicSelectorVec)` borrowed from `node_data[parent_id]` across iterations
   while `self.cascaded_props.build_mut(..)` runs.
5. `Css::sort_rules_in_cascade_order(&mut global_only_rules, rank)`: a `&mut Vec<&CssRuleBlock>`
   coerced to `&mut [&CssRuleBlock]`.
6. `css.rs` unit test `rules_sort_by_priority_then_theme_rank_then_specificity`:
   `assert_eq!(Vec<&CssRuleBlock>, vec![&a, ..])`.

## 7. Test commands for the parent

```
cargo test --release -p azul-css --test a_theme_chain_selects_blocks_by_prefix_and_floor
cargo test --release -p azul-css --test app_theme_selects_theme_blocks
cargo test --release -p azul-css --lib -- a_themes_rank_is_its_position a_declarations_cascade_rank rules_sort_by_priority_then_theme_rank the_winning_inline_declaration sort_by_priority_then_specificity
cargo test --release -p azul-css            # whole crate: the matcher moved under every @theme test
cargo test --release -p azul-core --lib     # prop_cache / compact / styled_dom unit tests
cargo test --release -p azul-layout --lib every_widget_theme_is_a_compiled_in_app_theme
cargo test --release -p azul-layout --test all -- a_theme_chain_ranks_its_blocks widgets_follow_the_app_theme app_theme_override inline_media_follows_source_order
cargo test --release -p azul-layout --test all      # full: every inline "last match" moved
```

The last one matters. Every inline resolution site changed shape, although it keeps source order
whenever no `@theme` block is involved.

## 8. Notes for the parallel tasks

- **R1 (`var()` at cascade time)**: variable DEFINITIONS must be ordered by the same rank.
  - Use `CssRuleBlock::cascade_key` for rules and `Css::inline_properties_in_cascade_order` /
    `winning_inline_property` for a node's own `--name` declarations, with the rank from
    `DynamicSelectorContext::cascade_rank`.
  - Don't write a second ordering. `@theme(xyz:pink) { :root { --color: pink } }` must beat
    `@theme(xyz) { :root { --color: blue } }` whatever the source order.
  - My edits in `prop_cache.rs` / `compact.rs` are confined to the rule-list sort, the inline pick
    / order sites and one `.rev()` on the global bucket.
- **R3 (chain building)**: the matcher already treats `:` spin-offs as prefixes, so an unexpanded
  `[xyz:pink]` behaves like `[xyz:pink, xyz]`. The default floor still has to be appended by R3:
  without a compiled-in entry, no widget block is live.
  - `UiTheme::current()` and (after the parent's commit) the no-context path read
    `app_theme_chain(current_theme())`. If `expand_chain` replaces `app_theme_chain`, update that
    call in `layout/src/widgets/themes/mod.rs`.
  - `structural_app_theme(chain)` is the DOM-shape answer.
- **Pre-existing gap (not R2)**: on the slow path, the inheritance walk never reads the global `*`
  bucket. A text node inside an element coloured by a `* { color }` rule inherits the UA colour
  there, while the compact tier has it right. The `*` rank test checks the element only.
