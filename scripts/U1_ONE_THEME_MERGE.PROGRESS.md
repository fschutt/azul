# U1 - one theme merge (progress)

Branch `wt/u1-one-theme-merge`, cut from `fix/input-bugs-2026-09-19` @ 0a326afe5.
Nothing is compiled here (house rule).

## Plan

1. ONE merge in `themes/theme_blocks.rs` with T3's names (`follow_props`, `follow_dom`,
   `follow_app_theme`, plus `follow_css` for a whole sheet). Semantic: T3's per-PROPERTY sharing
   (a property both themes declare alike is unconditional once), emitted ORDER-PRESERVING (T2's
   guarantee: under app theme T the live declarations ARE pinned T's, in order). Where the two
   orders cross, the property whose twin is further away gives way (written per theme). Rules are
   never split; a rule declaring a shared and a differing property goes whole into each block.
   Keeps T2's `flat == flora` short-circuit (per part / per sheet / per node style) and its
   component-sheet + keyframes merge. RED tests first, then move T3's section out of `flat.rs`,
   delete `every_theme_*`, switch the 18 T2 + all T3 call sites.
2. `theme_checks`: ONE evaluator of app-theme conditions for every probe (`tc::*`,
   `theme_probe::*`, `theme_blocks::checks::*`), under the app theme the test builds for.
3. Silent style-only twin build: a thread-local flag in `widgets/mod.rs`; `follow_app_theme` builds
   the other theme's twin under it. RED: an unnamed follower warns once.
4. Embedders pass their pin: audit; single-look embedders pin their chassis widgets to their one
   look (flat). RED first.
5. frame: skin merge (build once, no content clone). accordion: call-site switch only (V1 owns it).

## DONE

- 1 RED d08f7bfbf (theme_blocks tests: order-preserving per-property merge)
- 1 GREEN 0233bf13a (one merge in theme_blocks.rs; T3 section out of flat.rs; every_theme_* gone;
  all call sites switched; follow_tests moved)

## IN PROGRESS

- 2 theme_checks: one evaluator

## NEXT

- 3, 4, 5, report

## Open questions
