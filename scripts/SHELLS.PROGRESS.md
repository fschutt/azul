# SHELLS progress

Branch `wt/shells-s1-s11` from `wt/fb2-azdrive-explorer` (82f1d74c9). Report:
`scripts/SHELLS_2026_09_30.md`. Nothing is compiled here (house rule); every new Rust file is
parse-checked with `rustfmt --emit stdout` (scratchpad `shells_parse_check.py`).

## Plan
1. RED: `layout/src/widgets/shells/` - the shared pieces (OfficeShell, ShellNavigationPane,
   ShellCommandPalette, ShellSettingsLayout, ShellEmptyState, ShellThemeScope) and S1..S11
   (DocumentShell, CanvasShell, TimelineShell, PimShell, BrowserShell, RecordsShell, MediaShell,
   DeveloperShell, UtilityShell, CallShell, MobileShell) with their unit tests; `pub mod shells`
   and the lint manifest entries in widgets/mod.rs; `roving::test_support::press_window`.
   They reference `themes::flat::shell_look` / `themes::flora::shell_look`, which do not exist
   yet (RED). The tree view's half pair (its label's dark twin) loses its mask (RED).
2. GREEN: the two theme sections (`// ==== shells ====` at the END of flat.rs / flora.rs), the
   tree view label's light ink.
3. autofix: a `shells` api.json module (MODULES, keywords, the source-path arms, a test) so the
   shell classes land in `from azul.shells import ...` (user ruling: separate from widgets, the
   smaller items stay in widgets; every class name carries "Shell").
4. `examples/azul-shells` (AzShells): the picker of S1..S11, NoTitle + Titlebar title row;
   workspace member, `scripts/workspace_test_members.txt`, the `dll_tests` CI step.
5. `examples/azul-shells/scripts/shells_e2e.py`.
6. Report `scripts/SHELLS_2026_09_30.md`.

## DONE
- (see the commit list below once committed)

## IN PROGRESS
- step 1 commit (RED), step 3 commit (classifier)

## NEXT
- step 2 (GREEN themes), then 4, 5, 6

## Open questions
- `ShellNavigationPane` is what MAILWIDGETS composes into S4's navigation slot: groups + tree +
  module switcher, one `on_event` callback (see navigation_pane.rs).
