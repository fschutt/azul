# G1 form glue - progress (branch wt/g1-form-glue, cut from fix/input-bugs-2026-09-19 @ 433423e57)

Task: wire W2's replacement pass (`layout/src/form_controls.rs`) to W1's widgets, `<form>` -> `Form`,
form reset clears the replaced controls' memory, FormData collects replaced controls, `<datalist>`
display:none. NEVER compile. Report: `scripts/G1_FORM_GLUE_2026_09_29.md`.

## DONE
- 9f9ef57e4 progress file
- 330ea5048 RED step 1: `mod dedicated_widgets` in layout/tests/form_controls_become_widgets.rs +
  unit tests `a_week_value_names_the_monday_of_that_iso_week`,
  `month_and_datetime_values_are_checked_for_their_html_shape`
- 1c68d83b4 step 1: WAVE2-GLUE rows -> W1 widgets (all markers gone)
- 9ac424f07 RED step 2: `mod forms` (mount() harness inserts a DomLayoutResult, no layout)
- 5ca767bcb step 2: `<form>` row, form_for() trampolines, collect_form_data(), submit/reset
  take the callback out of the state before invoking

- 8c6c62af7 RED step 3: `mod form_reset`
- 4ccbd9a68 step 3: MEMORY_KEY_ATTRIBUTE on replaced roots, memory registry (`controls` map,
  `register`, `reset_control`), reset_form forgets + RefreshDom

## IN PROGRESS
- step 4 RED: FormData collects replaced controls

## NEXT
4. RED + impl: FormData collects replaced checkbox/radio/range/colour/number/date/time/select/...
5. RED + impl: `<datalist>` display:none
6. report

## Design decisions
- `FormWidget` variants carry the HTML type: `TextInput(TextInputKind)`, `DatePicker(DatePickerMode)`,
  `Button(ButtonFormAction)`, new `DateTimeLocal`, `ImageButton`, `Form`; the table stays the one source.
- the raw `type` attribute wins over the widget's own on the root (an image button renders as a
  submit button labelled by `alt`, but its root says `type=image`).
- replaced controls register (key -> how to spell the value, built + default value) in
  `FormControlMemory`; the root carries `data-azul-form-key`; FormData / reset look it up through
  `CallbackInfo::get_layout_window().form_control_memory`.

## Open questions
- none yet
