# SMALLAPPS progress (branch wt/smallapps, base 16d19442c)

Task: AzCalculator (F1/F2/F6/F7 vehicle) + AzContacts (A4). Report: scripts/SMALLAPPS_2026_10_01.md.

## DONE
- 6c7492e66 test(e2e): key_down names every punctuation and keypad key (RED)
- 8a5e2f886 fix(e2e): key_down knows the punctuation and keypad key names (GREEN)
- 7c4ce8bd6 test(appkit): the shared app skeleton (RED)
- 75f052000 feat(appkit): the shared app skeleton and its settings page (GREEN)
- 02a3d355f test(azcalculator): the calculator's model (RED)
- 8fccfdc91 feat(azcalculator): the calculator's model (GREEN)
- 2c1c8a6b2, 93e30c7bd wip(azcalculator): ui.rs pieces
- c2ce0f107 feat(azcalculator): the window (ui.rs wired into lib.rs)

## IN PROGRESS
- scripts/azcalculator_e2e.py

## NEXT
3. NEXT STEP: scripts/azcalculator_e2e.py (model on examples/azul-shells/scripts/shells_e2e.py): start
   AzCalculator headless with --data-dir <tmp> --sample, click keys by #key-* ids, type with key_down
   (digits, plus, asterisk, enter), assert AZCALC_DISPLAY lines + #calc-result text, history file content,
   modes (Scientific sin(30)+2^10, Programmer 2A5F bases), Convert 42.195 km, Date, settings page
   (#appkit-settings, theme flora + mode dark), screenshots. Then AzContacts.
4. AzContacts model (RED, GREEN): vCard 3.0/4.0 parse/write, contacts model, sort/index, duplicates + merge,
   storage layout contacts/<uuid>.vcf, sample data.
5. AzContacts UI on PimShell + E2E scripts/azcontacts_e2e.py + registration.
6. Report.

## Decisions (unattended)
- Shared skeleton = new crate examples/azul-appkit (precedent: azul-storage); apps depend on it. Not in azul's
  layout crate: args / data layout / settings files are app-suite concerns, not toolkit widgets.
- Data root: --data-dir > $AZLIN_DATA > <OS data dir>/Azlin; per app folder = <root>/<app>/ (calculator/,
  contacts/), i.e. the S3 bucket layout. settings.json lives in the app folder too.
- Precedence everywhere (Standard too, like GNOME / macOS), not Windows' immediate-execution Standard mode.
- Standard / Scientific numbers: bigdecimal 0.4.10 (already in Cargo.lock via turso_core); transcendental
  functions through f64, rounded to 15 significant digits.
- Button toggle variant (2nd, F-E) not built in azul tonight: active state = ButtonType::Primary (reported as left).
- Clipboard: Copy via Ctrl/Cmd+C in the window key handler (set_clipboard_content); Paste via
  EventFilter::Focus(FocusEventFilter::Paste) on the body (get_clipboard_content is only filled during a paste).
- About: a section of the settings page (ShellSettingsLayout) with a TODO for the DIALOGS agent's About dialog.

## Open questions
- (none)
