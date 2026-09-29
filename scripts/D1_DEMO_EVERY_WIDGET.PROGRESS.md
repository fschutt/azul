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

## IN PROGRESS
- step 3: "Raw HTML inputs" - `Dom::create_input(..)` + an XML snippet inside a Form
  (full version of forms.rs/lib.rs kept in the scratchpad as forms_full.rs / lib_full.rs)

## NEXT
1. step 4: e2e scenario check (`e2e/global_hotkey.json`), report

## Open questions
- (none yet)
