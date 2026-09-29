# G1 form glue - progress (branch wt/g1-form-glue, cut from fix/input-bugs-2026-09-19 @ 433423e57)

Task: wire W2's replacement pass (`layout/src/form_controls.rs`) to W1's widgets, `<form>` -> `Form`,
form reset clears the replaced controls' memory, FormData collects replaced controls, `<datalist>`
display:none. NEVER compile. Report: `scripts/G1_FORM_GLUE_2026_09_29.md`.

## DONE
- (none yet)

## IN PROGRESS
- step 1 RED: table rows -> W1 widgets (tests in layout/tests/form_controls_become_widgets.rs)

## NEXT
1. RED + impl: WAVE2-GLUE rows (TextInputKind, DatePickerMode month/week ISO, DateTimeLocal,
   submit/reset/image buttons, HiddenInput, optgroups)
2. RED + impl: raw `<form>` -> `Form` (Submit/Reset event handlers -> on_submit/on_reset trampolines)
3. RED + impl: form reset forgets the replaced controls' memory (+ RefreshDom)
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
