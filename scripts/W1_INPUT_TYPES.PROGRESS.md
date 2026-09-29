# W1 input types - progress (branch wt/w1-input-types, cut from fix/input-bugs-2026-09-19 @ 9c6065b05)

## DONE
- 1 password: RED 29a66b586, impl ad87f6c10 (TextInputKind on TextInputState, masking,
  clipboard veto, a11y Protected -> accesskit PasswordInput, with_kind_semantics in text_input.rs)
- 2 search: RED db3a54852, impl e7ca659e1 (wrapper row [field, clear x]; flat/flora search_clear_button +
  search_field appended; clear_field / replace_engine_line / sync_live_looks in text_input.rs)
- 3 email/tel/url + pattern: RED 0f9eed9b4, impl 885972248 (ValidityReason::TypeMismatch; form::is_valid_email /
  is_valid_absolute_url / value_matches_type; TextInputState::compute_validity; :user-invalid ring via
  override_node_css_properties, theme found by marker class on constrained fields; mark_user_invalid)
- 4 month/week/datetime-local: RED ebbdb7370, impl 50342904c (DatePickerMode on DatePicker + DatePickerData;
  create_month/create_week; ISO week math; Monday-first week grid; month grid + year nav; new
  datetime_local.rs DateTimeLocalPicker composing DatePicker+TimePicker; flat/flora datetime_local rows)
- 5 reset/submit/image + Form/FormData: RED 901832389, impl 32504b223 (widgets/form.rs: Form, FormData,
  FormEntry, FormStateWrapper, submit_form/reset_form, default_on_form_*; Button.form_action + Button.alt +
  create_submit/create_reset/create_image; TextInput Enter = implicit submission; restore_text_input)
- 6 hidden: RED e689fa68f, impl 4b7496f5a (HiddenInput in widgets/form.rs)
- 7 select optgroup: RED 9f9ac0d1f, impl 17b706795 (DropDown.groups + DropDownOptGroup(Vec); with_optgroup;
  build_menu_items: Disabled heading items without callback, indented options)
- follow-ups: wrapper-root name RED 00dec6398 / fix 293037f2c; borrow-scope refactor 8cb23d046;
  lint manifest RED 610e9a46a + 08bff3ce0, flora badge fix 8bd615659
- 8 report: scripts/W1_INPUT_TYPES_2026_09_29.md

## IN PROGRESS
(none)

## NEXT
- parent: compile, run suites, RED pass; api.json via autofix (list in the report); W2 glue (table in the report)

## Open questions
- see the report's "What is left" (engine overlay gap for programmatic values, W2 memory vs reset)
