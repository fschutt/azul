# G2_FORM_FOLLOWUPS - progress

Branch `wt/g2-form-followups`, cut from `0a326afe5` (pushed tip of `fix/input-bugs-2026-09-19`).
Nothing is compiled by this agent (house rules); the parent compiles and runs the suites.

## Step 1: audit (state at `0a326afe5`)

| # | item | state | where |
|---|---|---|---|
| 1 | `ChangeNodeText` cannot replace text the user TYPED; typed text survives a form reset and a programmatic value change | STILL OPEN | `dll/src/desktop/shell2/common/event.rs:6136` and `layout/src/e2e/runner.rs:2267` write the DOM text node only (and short-circuit when the DOM already holds the string - exactly the reset case: the DOM still says "ann", the overlay says "annbob"); the text overlay entry of the IFC root (`layout/src/overlay.rs` `ContentOverlay::text`) outranks the DOM (`window.rs` `get_text_before_textinput`) and is retired only by equality / an app ack (`overlay.rs` `gc_converged_text`, `gc_acked_text`). `text_input.rs:1559` `replace_engine_line` works around it for EMPTYING only; `widgets/form.rs:777` `reset_form` restores NAMED `TextInput`s only (no unnamed field, no `TextArea`). |
| 2 | XML runner starts with a fresh `FormControlMemory` | STILL OPEN | `layout/src/xml/mod.rs:220` (`parse_xml_to_styled_dom_resolving_icons`, the DLL's E2E mount `dll/src/desktop/shell2/common/layout.rs:898`). Replaced controls register in a throw-away memory: FormData of a mounted checkbox / slider / select falls back to nothing, a reset forgets nothing, a re-mount forgets the user's values. (The headless runner mounts through the FastDom path `parse_xml_to_styled_dom`, which replaces nothing - a drift, noted, not changed.) |
| 3 | raw `<button>` not replaced by the Button widget | STILL OPEN | `form_controls.rs:797` `widget_for` has no `NodeType::Button` arm; `INPUT_TYPE_WIDGETS` has no `<button>` row. A raw `<button type=submit>` in a form does not submit on click. |
| 4 | the raw `<form>`'s dataset is dropped | STILL OPEN | `form_controls.rs:1397` `form_for` -> `Form::dom` sets the node's dataset to its `FormStateWrapper` (`widgets/form.rs:400`); `graft` carries a dataset only onto a root without one (`form_controls.rs:2036`). |
| 5 | ComboBox: typed (not picked) text is lost / not the value | STILL OPEN | `combobox.rs:1048` / `1078` update `ComboBoxState::text` on typing but call no hook (only `on_select` on a pick, `combobox.rs:1161`); the replacement's recorder is `on_select` only (`form_controls.rs:1696`), so the memory never hears typed text: FormData reads the built text and a rebuild drops the typing (the field is not contenteditable - no overlay keeps it). The root carries no state, so a hand-built named ComboBox is invisible to FormData. |
| 6 | Password: undo restores masked text the mirror cannot follow | STILL OPEN | W1 report "What is left": the engine's undo (`dll/.../event.rs:15577` `undo_text_edit_on`) restores the BULLETS of the pre-edit buffer; the widget's real value (`TextInputState.text`) cannot be mapped back from bullets, so screen and value part ways (and a history of a secret is kept). The undo shortcut is consumed before callbacks (`core/src/events.rs:5155`), so the widget cannot veto it. |
| 7 | checkbox / radio / slider / colour / file / dropdown missing from FormData | DONE for replaced (raw) controls - G1 `750d77133` (`form_controls.rs:383` `Spelling`, `widgets/form.rs:516` `replaced_submission`); PARTIAL for hand-built widgets | hand-built `CheckBox` / `Slider` / ... inside a `Form` participate only through `name` + `value` ATTRIBUTES the app keeps current (W1 design, `widgets/form.rs:13`). |
| 8a | Enter-to-submit | DONE | W1 `73d7ba7c4` (`text_input.rs:2125` -> `widgets/form.rs:901` `submit_enclosing_form`); the engine's `SubmitForm` for non-text controls (`default_actions.rs:188`). Tests: `widgets/form.rs` `enter_in_a_text_field_submits_its_form`, G1 `enter_in_a_text_field_of_a_raw_form_submits_it`. |
| 8b | datalist `display:none` | DONE | G1 `a9bf67b8a` (`form_controls.rs:772` `hide_datalist`), tests `mod datalist`. |
| 8c | `accept` / `multiple` on file inputs | STILL OPEN | `file_input.rs:344` opens `FileDialog::open_file` with `OptionFileTypeList::None`, single file; `form_controls.rs` reads neither attribute (they are only carried to the root). |
| 8d | week in ISO format | DONE | G1 `bbfc2f689` (`form_controls.rs:1125` `parse_date` via `iso_week_monday`; spelled by `date_picker::format_value`), tests `a_week_input_becomes_an_iso_week_picker_on_its_value`, `a_week_value_names_the_monday_of_that_iso_week`. |
| 9 | `collect_form_data` not in api.json | STILL OPEN (list only) | `widgets/form.rs:894` is Rust-only; no FFI-shaped entry point exists. |

## DONE
- `3d1c5a6a9` audit (this table)
- item 1: RED `530b34601` (+ `f6ab00b72` caret check), fix = next commit
  (`LayoutWindow::set_node_text` shared by shell + runner; `DirtyTextNode::typed_over` +
  `ContentOverlay::gc_app_set_text` at a new generation; `reset_form` walks every text field)

## IN PROGRESS
- item 6 (password undo)

## NEXT
- items 6, 2, 3, 4, 5, 8c, 7 (hand-built part), 9 (list + FFI-shaped method)

## Open questions
- none yet
