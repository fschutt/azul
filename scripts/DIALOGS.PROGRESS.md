# DIALOGS - progress (branch `wt/dialogs`, base `16d19442c`)

Task: install-wizard pages, settings-dialog patterns, standard dialogs, the AzSetup
demo (examples/azul-setup), AzWidgets cards, tests, scripts/azsetup_e2e.py.
Report: scripts/DIALOGS_2026_10_01.md. House rules: no cargo, no rust-analyzer.

## DONE
- (none yet)

## IN PROGRESS
- Phase 1 (wizard): RED commit - dialog_kit, path_input, wizard_pages, wizard_layout
  extensions (banner / side panel / blocked reason / can_go_back / sizes),
  file::disk_space_for_new.

## NEXT
- Phase 1 GREEN (build + flat / flora looks).
- Phase 2: shortcut_recorder, ShellSettingsLayout extensions (category icons, section
  keywords, footer), shells/settings_dialog (rows, search across categories with
  highlight, dirty tracking, Apply / OK / Cancel or instant, restore defaults, restart
  badge). RED then GREEN.
- Phase 3: standard_dialogs (About, MessageBox, ProgressDialog, LoginDialog,
  FindReplaceDialog). RED then GREEN.
- Phase 4: examples/azul-setup (AzSetup), AzWidgets "Dialogs" cards, CI wiring,
  scripts/azsetup_e2e.py.
- Phase 5: report.

## Decisions (made unattended)
- ONE look for every new dialog widget: `widgets::dialog_kit::DialogKitLook`
  (flat / flora `dialog_kit_look()` appended to the theme files), merged per part
  for the follow path (the shells' `follow_look` shape) - no per-widget look twins.
- The wizard's new chrome parts (banner, side panel, reason, held button) go into
  the EXISTING `WizardLayoutLook` (its two theme fns gain the fields).
- A held Next is inert, dimmed through its box, announced Unavailable and
  described by the reason; Button itself has no `disabled` yet (reported).
- Path field + Browse is ONE widget (`PathInput`), used by the destination page and
  the settings path row; Browse calls `FileDialog::open_directory` / `open_file`
  (cfg `extra`, as FileInput does).
- The shortcut recorder records a `GlobalHotkey` (modifiers + one key; the type
  apps already declare hotkeys with), shown with `to_display_string`.
- The macOS-installer side panel is its own step list (Stepper has no vertical
  orientation); reported.
- The fake installer writes nothing; the settings demo keeps its values in memory.

## Open questions
- (none)
