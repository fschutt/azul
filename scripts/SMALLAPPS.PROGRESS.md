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
- c64368968 test(e2e): azlin_e2e.py shared driver
- 30d459a21 test(azcalculator): scripts/azcalculator_e2e.py
- 392058389 / f176d2f3b azcontacts vcard.rs (RED / GREEN)
- 66d7fae9b/afee1f564 contact.rs; 0d194a6e3/28a5ca6ed book.rs; 887b60931/89f0b41ef dupes.rs; 15a67f7d0/b85fc8269 store.rs; a5111bd72/716a23d94 sample.rs (RED/GREEN)
- 8d1010329, d1ac2b4ae wip(azcontacts): ui.rs pieces 1-2
- 75d7df15f wip(azcontacts): callbacks 3a; 6148ee264 feat(azcontacts): the window
- 1604b0505 test(azcontacts): scripts/azcontacts_e2e.py
- 10cacc7bd docs(smallapps): the report

## IN PROGRESS
- nothing - task complete

## NEXT
3. DONE (calculator UI + E2E).
4. DONE: AzContacts model (vcard, contact, book, dupes, store, sample), RED + GREEN each.
5. DONE: AzContacts UI (6148ee264).
ALL DONE: azcontacts_e2e.py (1604b0505), report scripts/SMALLAPPS_2026_10_01.md (10cacc7bd). Nothing left
for this task; the parent compiles, runs the suites and the two E2E scripts (commands in the report).
6. (done) scripts/azcontacts_e2e.py on scripts/azlin_e2e.py (start with --data-dir tmp --sample and a
   fixture .vcf as a positional file -> import preview; wait AZCONTACTS_LOADED 300 + SAMPLE_WRITTEN 300; search
   "krug" -> VIEW 1; click a row -> SELECTED; New -> type names -> Save -> SAVED + file exists; edit email bad ->
   PROBLEMS; duplicates -> merge -> MERGED + file deleted; jump bar; settings flora/dark; screenshots).
7. Then the report scripts/SMALLAPPS_2026_10_01.md (api.json list: none new in azul; least-sure spots; test
   commands), memory note not needed.
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
