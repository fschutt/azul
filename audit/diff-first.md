# diff-first: the rebuild pipeline that diffs before it styles

Task of the 2026-11-21 audit (RULES.md 8.8). Blind: nothing here was compiled or run.
Branch `audit/diff-first`, cut from the tip of `audit/A3-window` (`4d7cabccc`). The trace this builds on is
`audit/A3-window.md`, "Why a full DOM rebuild is slow" and "The diff-first pipeline" (step 1, `dd25f38e7`).

## Claims

Functions this task rewrites or restructures:

- `core/src/diff.rs`: `fingerprint_dom` (new tiers), `DomFingerprints` (new fields), new `same_tree_changes`,
  `changed_nodes_stay_themselves`, `adopt_changed_nodes`.
- `core/src/prop_cache.rs`: `FlatVecVec` (row replacement), `InheritedValues` (node removal), new
  `CssPropertyCache::recascade_nodes` and what it needs of `restyle_cascade`, `apply_ua_css_inner`,
  `hand_ua_defaults_down`, `compute_inherited_values`, `prune_compact_normal_props` (shared per-node bodies).
- `core/src/compact.rs`: `build_compact_cache_with_inheritance_debug` (a restricted pass over named nodes).
- `core/src/styled_dom.rs`: new `StyledDom::restyle_changed_nodes`.
- `layout/src/window.rs`: `diff_first_enabled` (step names), new `LayoutWindow::restyle_rebuilt_nodes`,
  `styled_dom_declares_an_animation`.
- `dll/src/desktop/shell2/common/layout.rs`: `regenerate_layout` (the exits 1.45 / 1.46).
- Tests: `core/src/diff_test.rs`, `core/src/prop_cache_test.rs` (own modules at the end), the harness of
  `dll/src/desktop/shell2/headless/mod.rs` `mod tests` (`a3w_*` helpers extended, new `df_*` tests).

Not touched (the parallel A3-window agent's): `node_patch.rs`, `apply_node_style_change`, `tick_animations`.
