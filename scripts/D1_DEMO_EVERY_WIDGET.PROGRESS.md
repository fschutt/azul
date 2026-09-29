# D1 - the AzWidgets demo shows every widget (progress)

Branch `wt/d1-demo-every-widget`, cut from `fix/input-bugs-2026-09-19` at `2892031b7`.
Touches only `examples/azul-widgets/` (+ this file and the final report). Nothing compiled.

## DONE
- `04ce4d69b` progress file
- `6417fbf9a` step 1: `.with_theme(theme)` on every themed widget (+ Spinner row auto/spokes/ring,
  Frame, video poster theme, TextArea text round trip)
- `97ddb2be9` checkpoint
- `3d29a3808` step 2: `forms.rs` - "Every input type, in a Form" (FormData label, Reset via app
  state + text-revision ack)
- `25fdcf27d` checkpoint
- `802c35291` step 3: "Raw HTML inputs" - `Dom::create_input(..)` column + XML snippet column in one
  app-built Form
- `5edd7a92f` rustfmt forms.rs + ack comment
- `bbfa6b5e2` checkpoint
- `141c193e2` step 4: new `e2e/every_input_form.json` (+ ids on printed lines / buttons);
  `e2e/global_hotkey.json` needs no change
- `76b77cfce` report `scripts/D1_DEMO_EVERY_WIDGET_2026_09_29.md`
- this final checkpoint

## IN PROGRESS
- (nothing - task complete)

## NEXT
- parent: build AzWidgets, fix compile errors (report "Least sure to compile"), run
  `e2e/every_input_form.json` headless; consider adding `src/forms.rs` to the demo colour lint and
  the preflight demo checks (report Findings 5)

## Open questions
- Should the raw-control replacement take a theme (report Findings 1)?
- Per-node text ack for form resets (report Findings 4)?
