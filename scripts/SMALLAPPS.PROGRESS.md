# SMALLAPPS progress (branch wt/smallapps, base 16d19442c)

Task: AzCalculator (F1/F2/F6/F7 vehicle) + AzContacts (A4). Report: scripts/SMALLAPPS_2026_10_01.md.

## DONE
- 6c7492e66 test(e2e): key_down names every punctuation and keypad key (RED)
- 8a5e2f886 fix(e2e): key_down knows the punctuation and keypad key names (GREEN)
- 7c4ce8bd6 test(appkit): the shared app skeleton (RED)
- 75f052000 feat(appkit): the shared app skeleton and its settings page (GREEN)
- 02a3d355f test(azcalculator): the calculator's model (RED)
- 8fccfdc91 feat(azcalculator): the calculator's model (GREEN)

## IN PROGRESS
- AzCalculator UI (NEXT 3), lib.rs in pieces

## NEXT
3. NEXT STEP: AzCalculator UI in examples/azul-calculator/src/lib.rs (replace the stub `start()`): state
   struct App { kit: RefAny, calc: Calculator, screen, panel (history/memory), convert + date state }, layout on
   UtilityShell (title row = appkit::ui::title_row, modes = Segmented [Standard, Scientific, Programmer, Date,
   Convert]), keypad grids per mode, window key handler (appkit::ui::handle_key first), Paste via
   FocusEventFilter::Paste, history load/save via appkit::ui::spawn_file_jobs (calculator/history.jsonl),
   stdout lines AZCALC_* for the E2E. Write it in pieces (state+start, then layout, then callbacks), commit each.
   Then the E2E scripts/azcalculator_e2e.py. AzCalculator UI on UtilityShell + E2E scripts/azcalculator_e2e.py + registration.
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
