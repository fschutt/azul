# V2_SET_NODE_STYLE progress

Branch `wt/v2-set-node-style`, base `d240a1b1d`.

## DONE
- b006adb20 RED: `layout/tests/a_node_restyled_by_a_callback_resolves_its_hover_and_dark_rules.rs`
  (does not compile until `CallbackInfo::set_node_style` / `CallbackChange::SetNodeStyle` exist).
- fix commit (see `git log`): `set_node_inline_style(.., CssPropertyWithConditionsVec)` ->
  `set_node_style(.., Css)`, `CallbackChange::SetNodeInlineStyle` -> `SetNodeStyle { style: Css }`,
  `ContentChange::NodeStyle` carries a `Css`; dll host, e2e runner, 4 widgets (call lines + in-file
  harnesses), 2 layout tests moved.

## IN PROGRESS
- none

## NEXT
- Report `scripts/V2_SET_NODE_STYLE_2026_09_29.md` (api.json list, inline-vs-component audit).

## Open questions
- api.json still lists `set_node_inline_style`: the parent must run autofix + `codegen all`
  before the dll (cabi_internal) builds.
