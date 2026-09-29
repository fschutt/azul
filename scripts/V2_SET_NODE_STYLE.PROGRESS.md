# V2_SET_NODE_STYLE progress

Branch `wt/v2-set-node-style`, base `d240a1b1d`.

## DONE
- RED: `layout/tests/a_node_restyled_by_a_callback_resolves_its_hover_and_dark_rules.rs`
  (does not compile until `CallbackInfo::set_node_style` / `CallbackChange::SetNodeStyle` exist).

## IN PROGRESS
- Fix: `set_node_inline_style(.., CssPropertyWithConditionsVec)` -> `set_node_style(.., Css)`,
  `CallbackChange::SetNodeInlineStyle` -> `SetNodeStyle { style: Css }`, `ContentChange::NodeStyle`
  carries a `Css`; every call site and harness moved.

## NEXT
- Report `scripts/V2_SET_NODE_STYLE_2026_09_29.md` (api.json list, inline-vs-component audit).

## Open questions
- none yet
