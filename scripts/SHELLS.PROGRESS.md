# SHELLS progress

Branch `wt/shells-s1-s11` from `wt/fb2-azdrive-explorer` (82f1d74c9). Report:
`scripts/SHELLS_2026_09_30.md`. Nothing is compiled here (house rule); every new Rust file is
parse-checked with `rustfmt --emit stdout` (scratchpad `shells_parse_check.py`).

## DONE
- `48f139892` RED: `layout/src/widgets/shells/` - OfficeShell, ShellNavigationPane,
  ShellCommandPalette, ShellSettingsLayout, ShellEmptyState, ShellThemeScope, S1..S11
  (DocumentShell .. MobileShell) with unit tests and shell-wide lints; `pub mod shells` + the
  manifest / contrast-group entries in widgets/mod.rs; `roving::test_support::press_window`; the
  tree view's half-pair masks removed.
- `ad2f75d36` autofix: the `shells` api.json module (MODULES, keywords, path arms, a test).
- `4bb2f0855` GREEN: `flat::shell_look` / `flora::shell_look` (appended at the end of each theme
  file), `shells::stack_state`, the tree view label's light ink beside its dark twin.
- `aea1efade` `examples/azul-shells` (AzShells) + `scripts/shells_e2e.py` + workspace member +
  `scripts/workspace_test_members.txt` + the `dll_tests` step.
- Report `scripts/SHELLS_2026_09_30.md` (api.json list, least-sure spots, test commands, what is
  left).

## IN PROGRESS
- nothing

## NEXT (for the parent)
- autofix (api.json `shells` module), compile, the suites (commands in the report), the AzShells
  build, the E2E under the capped runner, look at the screenshots.

## Open questions
- `ShellNavigationPane` is what MAILWIDGETS composes into S4's navigation slot (groups + tree +
  module switcher, one `on_event`); see the report's "What is left".
- User ruling applied: the shells are a separate api.json module `shells`
  (`from azul.shells import ShellNavigationPane`), every class name carries "Shell"; the
  smaller items stay in `widgets`.
