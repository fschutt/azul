# V2_SET_NODE_STYLE: a node's live restyle takes a stylesheet (`Css`)

Branch `wt/v2-set-node-style`, base `d240a1b1d`. Nothing was compiled (house rule). Every touched file
passes a `rustfmt --check` parse. Its only complaints are line-width differences of the kind the base
files already show.

## What was built

`CallbackInfo::set_node_inline_style(node_id, CssPropertyWithConditionsVec)` is now
`CallbackInfo::set_node_style(node_id: DomNodeId, style: Css)`. It mirrors `NodeData::set_style`: it
replaces the node's inline stylesheet after the callback returns and ignores a `node_id` without a
node. The old name is removed, not deprecated.

- `CallbackChange::SetNodeInlineStyle` is now `CallbackChange::SetNodeStyle { dom_id, node_id, style: Css }`.
- `overlay::ContentChange::NodeStyle` carries the `Css` straight through. `apply_node_style_change`
  (layout/src/window.rs:7967) no longer converts. It sets the node's `style` and restyles exactly as
  before (same diff, same override clearing, same recascade, same tier).
- The dll host (dll/src/desktop/shell2/common/event.rs:6442) and the e2e runner
  (layout/src/e2e/runner.rs:2448) delegate the renamed change. Each edit is a one-line rename.
- Callers pass their built declarations through the existing `From<CssPropertyWithConditionsVec> for Css`
  (css/src/css.rs:284) as `.into()`. No converter was written.
- The doc (layout/src/callbacks.rs, `set_node_style`) keeps the "why not `set_css_property`" paragraph
  and adds **why not a DOM refresh**. A self-contained widget keeps its state (selected segment,
  current page, picked day) in its own dataset, not in the app's model. `Update::RefreshDom` re-runs
  the APP's layout callback, which rebuilds the widget from the app's data and loses that state. That
  is the answer to "shouldn't this API even exist?": the API is the only way to restyle a node without
  a rebuild while keeping its conditional rules live. The doc also states that rule selectors are not
  matched on a node's own style (see the audit below).

## Commits

- `b006adb20` test(callbacks): a node restyled by a callback resolves its hover and dark rules (RED:
  does not compile without the new API).
- `a5be3fe6a` feat(callbacks): a node's live restyle takes a stylesheet, like the node stores it.
- (this report + checkpoint)

## Tests

- NEW `layout/tests/a_node_restyled_by_a_callback_resolves_its_hover_and_dark_rules.rs`, registered at
  the end of `layout/tests/all.rs`:
  - `a_stylesheet_set_by_a_callback_resolves_its_hover_rule_and_its_dark_twin`: a real callback
    (`invoke_single_callback_at`) calls `set_node_style` with a `Css` holding a light face, a dark twin,
    an `on_hover` rule and a `dark_on_hover` twin. The test checks four things:
    - the change is exactly one `SetNodeStyle` on the named node, carrying the stylesheet unchanged;
    - applied through the chokepoint, it gives tier `RebuildDisplayList` and paints the new colour;
    - by day the node resolves green at rest and purple hovered;
    - after a restyle-only switch to dark it resolves yellow at rest and orange hovered, and green
      again back in light.
  - `a_callback_restyling_a_node_id_without_a_node_pushes_nothing`.
  - It reuses the chokepoint test's helpers (`window`, `switch_scheme`, `replace`, `box_fill`, `fill`,
    `env_pinned`, `BOX`, `RED`, now `pub(crate)`), following the
    `a_layout_blit_repaints_the_scrollbar_it_dragged` precedent. Nothing was copied.
- Moved to the new name / type:
  - `layout/tests/a_replaced_inline_style_follows_the_mode.rs`: `box_style` returns `Css`, the box is
    built with `with_style`, and `replace` takes `Css`.
  - `layout/tests/a_clicked_control_takes_the_new_mode_after_a_scheme_switch.rs`: variant rename.
  - The in-file harnesses of date_picker / pagination / segmented / stepper (see the call-site list).

Commands for the parent:

```bash
cargo test --release -p azul-layout --test all -- a_node_restyled_by_a_callback_resolves_its_hover_and_dark_rules::
cargo test --release -p azul-layout --test all -- a_replaced_inline_style_follows_the_mode::
cargo test --release -p azul-layout --test all -- a_clicked_control_takes_the_new_mode_after_a_scheme_switch::
cargo test --release -p azul-layout --lib -- widgets::segmented:: widgets::pagination:: widgets::stepper:: widgets::date_picker::
cargo check -p azul-dll   # after api.json autofix + `codegen all` (see below)
```

## api.json (via autofix, never by hand)

- REMOVE `CallbackInfo.functions.set_node_inline_style` (api.json:20061-20088 today).
- ADD `CallbackInfo.functions.set_node_style`:
  - `fn_args`: `[{"self": "refmut"}, {"node_id": "DomNodeId"}, {"style": "Css"}]`, no return
  - `fn_body`: `object.set_node_style(node_id, style)`
  - `doc`: the ASCII doc comment of `CallbackInfo::set_node_style` in layout/src/callbacks.rs, checked
    ASCII-only.
- **Build blocker until done:** the dll's FFI is generated from api.json into
  `target/codegen/dll_api_*.rs` (dll/src/lib.rs:312). While api.json still names
  `set_node_inline_style`, the generated body calls a method that no longer exists, so run autofix and
  then `azul-doc codegen all` before building the dll.
- `CallbackChange` and `overlay::ContentChange` are Rust-only (not FFI): no api.json change.

## Every call site changed

- `layout/src/widgets/date_picker.rs`:
  - call line 2087 `info.set_node_style(cell, face.into())`;
  - doc 248;
  - harness `pushed_faces` (2718-2735, returns `Css`) and `resting` (2741, reads `iter_inline_properties`).
- `layout/src/widgets/pagination.rs`:
  - call line 754 `(skin.button)(..).into()`;
  - docs 6, 421;
  - harness `inline_writes` (1186-1203), `restyle` (1213-1217);
  - flora test `got` (2535-2538) and `with_style` at 2551 / 2600.
- `layout/src/widgets/segmented.rs`:
  - call at 727, rustfmt-wrapped to four lines because `.into()` pushed the arguments past
    `fn_call_width`;
  - docs 10, 361; `use azul_css::css::Css` added to the `autotest_generated` imports (760);
  - harness `inline_writes` (1231-1251) and `restyle_writes` (1263-1268);
  - `Css::from(..)` in the expected values at 2482, 3028 and 3075;
  - `with_style` at 3035 / 3078.
- `layout/src/widgets/stepper.rs`:
  - calls at 876, 879, 882-885, 888;
  - docs 9, 434; `css::Css` import (935);
  - harness `restyle_writes` (1548-1552) and `inline_writes` (1571-1579);
  - flora test `Css::from(..)` at 3931-3936;
  - `with_style` at 3941 / 3979.
- `layout/src/overlay.rs:176`, `layout/src/callbacks.rs` (variant at 525, method at 2729), `layout/src/window.rs:7967`.

**Coordination note (R5-B/C/D own these widget files).** The production edits are the call lines and
the one-word doc renames only. The harness hunks had to move too: they pattern-match the renamed
variant and read the old vector type. They stay inside the `#[cfg(test)] mod autotest_generated`
harness functions and the three flora tests that consume them. Rather than write another
`Css -> Vec<CssPropertyWithConditions>` converter (see the twins below), the harnesses now read the
`Css` itself (`iter_inline_properties`) and compare against `Css::from(skin(..))`. Expect small merge
conflicts only if R5 touched the same harness lines.

## Audit: does anything still store inline style as something other than a `Css`?

**Storage: the user's model already holds.**

- `NodeData`'s only style field is `pub style: azul_css::css::Css` (core/src/dom.rs:1779). `NodeDataExt`
  (core/src/dom.rs:2351) has no style field.
- `Dom.css: CssVec` (core/src/dom.rs:4386) is the subtree's sheets; `FastDom` uses `CssWithNodeId.css: Css`
  (core/src/dom.rs:4413).
- Every legacy setter converts on entry, and none keeps a raw vector:
  - `set_css_props` / `with_css_props` (core/src/dom.rs:3418, 3834, 7371) do `css_props.into()`;
  - `add_css_property` (3669, 7240) pushes one `CssRuleBlock`;
  - `upsert_inline_css_property` (3435) edits the rules;
  - `NodeData::set_css(&str)` (3920) appends `Css::parse_inline` rules.

Per-node raw property vectors do exist, but none of them is the node's inline style:

- `CssPropertyCache.user_overridden_properties: Vec<Vec<(CssPropertyType, CssProperty)>>`
  (core/src/prop_cache.rs:1031): the user-override channel that `set_css_property` /
  `override_node_css_properties` write. That is the pinning layer this API exists to avoid.
- `css_props` / `cascaded_props` (prop_cache.rs:1082-1086): cascade outputs, not inputs.
- Widget SKIN structs hold `CssPropertyWithConditionsVec` / `Vec<CssPropertyWithConditions>` fields
  (e.g. date_picker.rs:214-255, radio_group.rs:370-378, tooltip.rs:165, dialog.rs:789). These are build
  inputs converted with `.into()` when the node is built. They could become `Css` (proposal P3).

**Semantics: the model holds only in part.** A node's `Css` is not a stylesheet scoped to that node:

1. **Selectors are ignored.** Every cascade reader of a node's own style goes through
   `CssPropertyCache::inline_properties` (prop_cache.rs:2763), which flattens every rule's
   declarations and never reads `rule.path`. Pseudo-state gating uses only `rule.conditions`
   (`matches_pseudo_state`, prop_cache.rs:3036).
   - The parser puts a nested `:hover` into the PATH (css/src/parser2.rs:1994-1997), not into
     `conditions`.
   - So `NodeData::with_css("color: blue; :hover { color: red; }")` and `Css::parse_inline(...)` on a
     node should resolve red at rest. I believe this is a live bug but it is **not verified**.
   - `@theme(dark)` / `@media` / `@os` blocks do become conditions, so they work.
   - `form_controls::graft` (layout/src/form_controls.rs:2120, 2187) moves `* :hover {..}` rules from
     a raw input's scoped sheets INTO the widget root's inline style, so it may hit the same issue.
   - Widgets are unaffected because they build pseudo-states as conditions (`on_hover`, ...).
2. **Priority.** Inline rules sit at `rule_priority::INLINE`, above author sheets. `Dom.css` rules are
   selector-matched within the subtree (`scope_inline_css`, core/src/styled_dom.rs:3110).
3. **Same name, different storage.** `Dom::with_css(&str)` (core/src/dom.rs:7447) goes to the
   subtree's `Dom.css` via `add_component_css`. `NodeData::with_css(&str)` (3920) goes to the node's
   `style`.
4. **FFI gap.** api.json exposes no `NodeData.set_style` / `with_style` and no `Dom.with_style`. From
   C/Python the only way to set a node's own `Css` is the declaration-list setters (`with_css_props`,
   `add_css_property`). `Dom.with_css(String)` sets component CSS instead.

So "inline style = a full stylesheet attached to a leaf node" holds on a leaf only for rules whose
state and mode live in `conditions`, and with INLINE priority. `with_component_css` on a leaf matches
selectors; the node's own style does not.

## Proposals (not done: outside the callback API)

- **P1 (bug, needs RED first).** Lower pseudo-selectors in a node's own style to conditions:
  - either at `Css::parse_inline` / `NodeData::set_style`, turning a Global-rooted `[*, :hover]` path
    into `PseudoState(Hover)` conditions;
  - or have `inline_properties` honour a path of `*` plus pseudo-selectors.

  Suggested RED: `NodeData::with_css("color: blue; :hover { color: red; }")` resolves blue at rest.
- **P2 (`set_node_style` with `var()` / custom properties).** `changed_property_types`
  (layout/src/window.rs:8060) uses `Css::iter_inline_properties`, which skips `Dynamic` and
  `CustomProperty` declarations. A new style that differs only in `var()` / `--x` declarations is
  stored, but `Unchanged` is returned with no recascade (window.rs:7999). Also,
  `recascade_ua_inheritance_and_compact` (core/src/styled_dom.rs:1935) does not re-run the
  custom-property resolution pass. This was already true before; taking a full `Css` makes it
  reachable from the public API.
- **P3.** Widget skins could hold `Css` instead of `CssPropertyWithConditionsVec`, removing the
  `.into()` at every call site.
- **P4.** Add `NodeData.set_style` / `with_style` and `Dom.with_style` to api.json, so FFI users can
  set a node's own `Css` directly.
- **P5 (NO DUPLICATION: existing twins).** About 20 test helpers each hand-roll the same
  "`Css` -> `Vec<CssPropertyWithConditions>`" loop (`iter_inline_properties().map(|(p, c)| CssPropertyWithConditions { .. })`):
  - layout/src/icon.rs:653, 1191;
  - widgets/mod.rs:1401 `inline_props`;
  - stepper.rs:1243 `inline_declarations`;
  - date_picker.rs:5563 `declarations`;
  - pagination.rs:841;
  - themes/theme_checks.rs:312; themes/theme_blocks.rs:1128 `inline`;
  - chip.rs:3118, breadcrumb.rs:1403, menubar.rs:1288, color_input.rs:3943, accordion.rs:1837,
    toast.rs:2404, combobox.rs:3942, video.rs:3699 / 3709, alert.rs:1915, spinner.rs:1087.

  Line numbers point at each loop's `apply_if` line. One shared helper (in `theme_checks`) would fold
  them. I added none.

## Least sure to compile

- The new test: `data.downcast_ref::<Restyle>().map(|r| (r.target, r.style.clone()))` (`Ref` derefs;
  `DomNodeId` is `Copy`). There is also the slice pattern
  `let [CallbackChange::SetNodeStyle { dom_id, node_id, style }] = changes.as_slice() else { .. }`.
- Harness closures `.filter(|(p, c)| c.as_ref().is_empty() && p.get_type() == ty)` over
  `iter_inline_properties()`. The same shape already compiles at stepper.rs:1236.
- `Css::from(skin(..))` compared inside tuples / `Vec<(usize, Css)>`: `Css` derives `PartialEq` + `Debug`.

## Left

- api.json autofix + `codegen all` (parent).
- Proposals P1-P5 above, each needing its own RED.
