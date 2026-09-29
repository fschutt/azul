# W1 input types - progress (branch wt/w1-input-types, cut from fix/input-bugs-2026-09-19)

## DONE
- 1 password: RED 29a66b586, impl ad87f6c10 (TextInputKind on TextInputState, masking,
  clipboard veto, a11y Protected -> accesskit PasswordInput, with_kind_semantics in text_input.rs)

- 2 search: RED db3a54852, impl e7ca659e1 (wrapper row [field, clear x]; flat/flora search_clear_button +
  search_field appended; clear_field / replace_engine_line / sync_live_looks in text_input.rs)

- 3 email/tel/url + pattern: RED 0f9eed9b4, impl 885972248 (ValidityReason::TypeMismatch; form::is_valid_email /
  is_valid_absolute_url / value_matches_type; TextInputState::compute_validity; :user-invalid ring via
  override_node_css_properties, theme found by marker class on constrained fields; mark_user_invalid)

## IN PROGRESS
- 4 month/week (DatePicker modes) + datetime-local

## NEXT (in order)
2. search (wrapper div [container, clear x]; x hidden when empty; Escape + x clear)
3. email/tel/url + pattern (validity_of stub in text_input.rs -> real checks; ValidityReason::TypeMismatch
   appended in core/src/form.rs; validators in layout/src/form.rs; invalid ring look via
   override_node_css_properties at runtime, inline at build)
4. month/week (DatePicker modes) + datetime-local (new DateTimeLocalPicker composing DatePicker+TimePicker)
5. reset/submit/image + Form/FormData (Form captures initial FormData at build from child datasets)
6. hidden (HiddenInput)
7. select optgroup (drop_down)
8. final report scripts/W1_INPUT_TYPES_2026_09_29.md

## Design notes so far
- TextInput kinds are post-processed in text_input.rs `with_kind_semantics` (theme-agnostic);
  flat/flora text_input only changed to render `display_text(state)`.
- Engine gap (report it): programmatic value replacement of a contenteditable after user edits -
  ChangeNodeText does not supersede the content overlay; widget helpers use change_node_text +
  select-all/delete_backward when focused.

## Open questions
